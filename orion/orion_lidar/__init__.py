"""LiDAR 2D para robots: leer, mapear, detectar obstáculos y frenar a tiempo.

Fuentes (la primera disponible):
  · ROS 2: tópico /scan (sllidar_ros2 / rplidar_ros de Slamtec, BSD-2) → para cualquier LiDAR con driver ROS
  · rplidar (Python, puerto serie)        → RPLidar A1/A2/C1 directo, sin ROS
  · Simulador                              → sala con muros y personas en movimiento (pruebas y demo)
Procesamiento propio, liviano (corre en Raspberry):
  · Mapa de ocupación (log-odds, trazado de rayos Bresenham) con la pose del robot
  · Obstáculo más cercano por sector y decisión de frenado (se conecta al Guardián)
  · Personas/objetos móviles: puntos que no coinciden con el mapa estático
Para SLAM completo (sin pose conocida) se integra slam_toolbox (ROS 2) o KISS-ICP.
"""
from __future__ import annotations
import math
from dataclasses import dataclass, field

import numpy as np


@dataclass
class Escaneo:
    angulos: np.ndarray          # rad, 0 = frente del robot, sentido antihorario
    distancias: np.ndarray       # mm (0 o nan = sin retorno)
    pose: tuple = (0.0, 0.0, 0.0)   # x_mm, y_mm, theta_rad del robot en el mapa

    def puntos_mundo(self) -> np.ndarray:
        ok = np.isfinite(self.distancias) & (self.distancias > 0)
        x, y, th = self.pose
        a = self.angulos[ok] + th
        d = self.distancias[ok]
        return np.stack([x + d * np.cos(a), y + d * np.sin(a)], 1)


# ------------------------------------------------------------------ fuentes
class Simulador:
    """Sala poligonal + obstáculos + personas que caminan. Rayos con intersección exacta."""
    def __init__(self, muros: list[tuple], personas: list[dict] | None = None, rayos: int = 360, alcance: float = 8000, ruido: float = 8.0):
        self.muros = [np.array(m, float) for m in muros]          # segmentos (x1,y1,x2,y2) mm
        self.personas = personas or []                             # {"centro":(x,y), "radio":180, "vel":(vx,vy)}
        self.rayos, self.alcance, self.ruido = rayos, alcance, ruido
        self.t = 0.0

    def avanzar(self, dt: float):
        self.t += dt
        for p in self.personas:
            p["centro"] = (p["centro"][0] + p["vel"][0] * dt, p["centro"][1] + p["vel"][1] * dt)

    def escanear(self, pose) -> Escaneo:
        x, y, th = pose
        ang = np.linspace(-math.pi, math.pi, self.rayos, endpoint=False)
        dx, dy = np.cos(ang + th), np.sin(ang + th)
        dist = np.full(self.rayos, np.inf)
        for m in self.muros:
            x1, y1, x2, y2 = m
            ex, ey = x2 - x1, y2 - y1
            den = dx * ey - dy * ex
            with np.errstate(divide="ignore", invalid="ignore"):
                t = ((x1 - x) * ey - (y1 - y) * ex) / den
                u = ((x1 - x) * dy - (y1 - y) * dx) / den
            hit = (np.abs(den) > 1e-9) & (t > 0) & (u >= 0) & (u <= 1)
            dist = np.where(hit & (t < dist), t, dist)
        for p in self.personas:
            cx, cy, r = p["centro"][0] - x, p["centro"][1] - y, p.get("radio", 180)
            b = dx * cx + dy * cy
            disc = b * b - (cx * cx + cy * cy - r * r)
            t = b - np.sqrt(np.maximum(disc, 0))
            dist = np.where((disc > 0) & (t > 0) & (t < dist), t, dist)
        dist = np.where(dist > self.alcance, np.nan, dist + np.random.normal(0, self.ruido, self.rayos))
        return Escaneo(ang, dist, tuple(pose))


def fuente_rplidar(puerto: str = "/dev/ttyUSB0"):
    """RPLidar por puerto serie (paquete `rplidar`). Entrega Escaneos sin pose (pose = 0,0,0)."""
    from rplidar import RPLidar
    lidar = RPLidar(puerto)
    try:
        for scan in lidar.iter_scans():
            q, a, d = np.array(scan).T
            yield Escaneo(np.radians(-a), d)                       # el RPLidar gira en sentido horario
    finally:
        lidar.stop(); lidar.disconnect()


# ------------------------------------------------------------------ mapa de ocupación
@dataclass
class Mapa:
    ancho_mm: float = 12000
    alto_mm: float = 9000
    celda_mm: float = 50
    origen: tuple = (-1000.0, -1000.0)
    logodds: np.ndarray = field(init=False)

    def __post_init__(self):
        self.logodds = np.zeros((int(self.alto_mm / self.celda_mm), int(self.ancho_mm / self.celda_mm)), np.float32)

    def _celda(self, xy):
        return ((np.asarray(xy) - self.origen) / self.celda_mm).astype(int)

    def actualizar(self, e: Escaneo, libre=-0.4, ocupado=0.9):
        H, W = self.logodds.shape
        r0 = self._celda(e.pose[:2])
        for c in self._celda(e.puntos_mundo()):
            n = int(max(abs(c[0] - r0[0]), abs(c[1] - r0[1])))
            if n == 0:
                continue
            xs = np.linspace(r0[0], c[0], n, endpoint=False).astype(int)
            ys = np.linspace(r0[1], c[1], n, endpoint=False).astype(int)
            ok = (xs >= 0) & (xs < W) & (ys >= 0) & (ys < H)
            self.logodds[ys[ok], xs[ok]] += libre
            if 0 <= c[0] < W and 0 <= c[1] < H:
                self.logodds[c[1], c[0]] += ocupado
        np.clip(self.logodds, -4, 6, out=self.logodds)

    def ocupado(self) -> np.ndarray:
        return self.logodds > 1.5

    def moviles(self, e: Escaneo, radio_celdas: int = 2) -> np.ndarray:
        """Puntos del escaneo que no calzan con el mapa estático → personas u objetos que se mueven."""
        occ = self.ocupado(); H, W = occ.shape
        pts = e.puntos_mundo(); cel = self._celda(pts)
        res = []
        for p, (cx, cy) in zip(pts, cel):
            y0, y1, x0, x1 = max(cy - radio_celdas, 0), min(cy + radio_celdas + 1, H), max(cx - radio_celdas, 0), min(cx + radio_celdas + 1, W)
            if y0 >= y1 or x0 >= x1 or not occ[y0:y1, x0:x1].any():
                res.append(p)
        return np.array(res).reshape(-1, 2)


# ------------------------------------------------------------------ seguridad
def sectores(e: Escaneo, n: int = 8) -> list[dict]:
    """Distancia mínima por sector (frente, frente-izq, izq, …)."""
    nombres = ["frente", "frente-izq", "izquierda", "atrás-izq", "atrás", "atrás-der", "derecha", "frente-der"]
    a = (e.angulos + math.pi / n) % (2 * math.pi)
    idx = (a / (2 * math.pi / n)).astype(int) % n
    out = []
    for k in range(n):
        d = e.distancias[(idx == k) & np.isfinite(e.distancias)]
        out.append({"sector": nombres[k] if n == 8 else str(k), "min_mm": float(d.min()) if len(d) else None})
    return out


def decision_avance(e: Escaneo, velocidad_mm_s: float, frenado_mm: float = 150, reaccion_s: float = 0.3, ancho_robot_mm: float = 400) -> dict:
    """¿Puede seguir avanzando? Mira el corredor del ancho del robot hacia adelante."""
    ok = np.isfinite(e.distancias) & (np.abs(np.sin(e.angulos) * e.distancias) < ancho_robot_mm / 2) & (np.cos(e.angulos) > 0)
    frente = (np.cos(e.angulos) * e.distancias)[ok]
    libre = float(frente.min()) if len(frente) else math.inf
    necesaria = frenado_mm + velocidad_mm_s * reaccion_s
    if libre < necesaria:
        v = max(0.0, (libre - frenado_mm) / reaccion_s)
        return {"accion": "FRENAR" if v < 1 else "REDUCIR", "libre_mm": round(libre), "vel_max_mm_s": round(v)}
    return {"accion": "SEGUIR", "libre_mm": round(libre), "vel_max_mm_s": velocidad_mm_s}
