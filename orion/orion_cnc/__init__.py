"""Modo taller: la fresadora CNC (GRBL) desde Neo Orión, con el mismo guardián que los robots.

Lo que hace:
  · revisar un G-code ANTES de cortar: recorrido, límites X/Y/Z, tiempo aproximado, herramientas, avances,
    profundidad máxima; lo compara con el área útil de la máquina y avisa si se sale
  · vista previa del recorrido (PNG) para el HUD
  · hablar con GRBL por USB: estado (?), desbloquear ($X), ir a casa ($H), mover a mano (jog $J=), cero de pieza
    (G10 L20), pausa (!), seguir (~), parada inmediata (Ctrl-X)
  · mandar el trabajo línea por línea esperando el «ok» de cada una (modo seguro) y con avance en %

Seguridad: mover la máquina y cortar son acciones FÍSICAS: el MCP pide confirmación humana (ticket) antes de
`iniciar` o `mover`. «detente» en el HUD manda la parada inmediata. Pausa y parada no piden permiso.
Alternativas gráficas que se pueden instalar al lado: CNCjs (web), Candle, bCNC, UGS.
"""
from __future__ import annotations

import math
import re
import time
from dataclasses import dataclass, field
from pathlib import Path

PALABRA = re.compile(r"([A-Z])\s*(-?\d*\.?\d+)")
TIEMPO_RAPIDO = 1500.0          # mm/min si la máquina no informa su G0


@dataclass
class Maquina:
    nombre: str = "CNC 3018"
    x_mm: float = 300
    y_mm: float = 180
    z_mm: float = 45
    g0_mm_min: float = TIEMPO_RAPIDO
    puerto: str = "/dev/ttyUSB0"
    baudios: int = 115200


@dataclass
class Revision:
    lineas: int = 0
    movimientos: int = 0
    minimo: list = field(default_factory=lambda: [math.inf] * 3)
    maximo: list = field(default_factory=lambda: [-math.inf] * 3)
    corte_mm: float = 0.0
    rapido_mm: float = 0.0
    minutos: float = 0.0
    avances: set = field(default_factory=set)
    herramientas: set = field(default_factory=set)
    rpm: set = field(default_factory=set)
    puntos: list = field(default_factory=list)       # (x, y, z, corte?)
    avisos: list = field(default_factory=list)
    errores: list = field(default_factory=list)

    def resumen(self) -> dict:
        ok = self.minimo[0] != math.inf
        return {"lineas": self.lineas, "movimientos": self.movimientos,
                "desde_mm": [round(v, 2) for v in self.minimo] if ok else None,
                "hasta_mm": [round(v, 2) for v in self.maximo] if ok else None,
                "tamano_mm": [round(b - a, 2) for a, b in zip(self.minimo, self.maximo)] if ok else None,
                "profundidad_max_mm": round(self.minimo[2], 2) if ok else None,
                "corte_m": round(self.corte_mm / 1000, 2), "minutos": round(self.minutos, 1),
                "avances_mm_min": sorted(self.avances), "herramientas": sorted(self.herramientas), "rpm": sorted(self.rpm),
                "avisos": self.avisos, "errores": self.errores, "listo_para_cortar": not self.errores}


def revisar(gcode: str, maquina: Maquina | None = None) -> Revision:
    """Simula el G-code (G0/G1/G2/G3, G20/G21, G90/G91) sin mover nada."""
    m = maquina or Maquina()
    r = Revision()
    pos = [0.0, 0.0, 0.0]
    absoluto, escala, f, modo = True, 1.0, 0.0, 0
    for n, cruda in enumerate(gcode.splitlines(), 1):
        linea = re.sub(r"\(.*?\)|;.*$", "", cruda).strip().upper()
        if not linea:
            continue
        r.lineas += 1
        pal = {}
        for k, v in PALABRA.findall(linea):
            if k in "GM":
                c = float(v)
                if k == "G":
                    if c in (0, 1, 2, 3):
                        modo = int(c)
                    elif c == 20:
                        escala = 25.4
                    elif c == 21:
                        escala = 1.0
                    elif c == 90:
                        absoluto = True
                    elif c == 91:
                        absoluto = False
                elif c in (3, 4):
                    pass
            else:
                pal[k] = float(v)
        if "F" in pal:
            f = pal["F"] * escala
            r.avances.add(round(f))
        if "S" in pal:
            r.rpm.add(int(pal["S"]))
        if "T" in pal:
            r.herramientas.add(int(pal["T"]))
        if not any(k in pal for k in "XYZ"):
            continue
        nuevo = list(pos)
        for i, k in enumerate("XYZ"):
            if k in pal:
                nuevo[i] = pal[k] * escala if absoluto else pos[i] + pal[k] * escala
        d = math.dist(pos, nuevo)
        if modo in (2, 3) and ("I" in pal or "J" in pal):
            radio = math.hypot(pal.get("I", 0), pal.get("J", 0)) * escala
            d = max(d, radio * 0.5 * math.pi)          # aproximación conservadora del arco
        if modo == 0:
            r.rapido_mm += d
            r.minutos += d / m.g0_mm_min
        else:
            if f <= 0:
                r.errores.append(f"línea {n}: corte sin avance F definido")
                f = 100
            r.corte_mm += d
            r.minutos += d / f
        r.movimientos += 1
        for punto in (pos, nuevo):          # el recorrido incluye desde dónde parte
            for i in range(3):
                r.minimo[i] = min(r.minimo[i], punto[i])
                r.maximo[i] = max(r.maximo[i], punto[i])
        pos = nuevo
        if len(r.puntos) < 20000:
            r.puntos.append((pos[0], pos[1], pos[2], modo != 0))
    if r.movimientos:
        tam = [b - a for a, b in zip(r.minimo, r.maximo)]
        for eje, t, lim in zip("XYZ", tam, (m.x_mm, m.y_mm, m.z_mm)):
            if t > lim:
                r.errores.append(f"el trabajo mide {t:.1f} mm en {eje} y la máquina tiene {lim:g} mm")
        if r.minimo[2] < -m.z_mm:
            r.errores.append(f"baja hasta Z {r.minimo[2]:.1f} mm: más que el recorrido de la máquina")
        if r.minimo[2] < -10:
            r.avisos.append(f"profundidad máxima {r.minimo[2]:.1f} mm: revisa el largo de la fresa")
        if max(r.avances, default=0) > 3000:
            r.avisos.append(f"avance de {max(r.avances)} mm/min: alto para una CNC de escritorio")
        if not r.rpm:
            r.avisos.append("el archivo no enciende el husillo (no hay S/M3): enciéndelo a mano")
    else:
        r.errores.append("el archivo no tiene movimientos")
    return r


def vista(rev: Revision, png: str) -> str:
    """Recorrido visto desde arriba: cortes en el color de acento, rápidos en gris punteado."""
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    fig, ax = plt.subplots(figsize=(6, 4.2), dpi=150, facecolor="#0d0e10")
    ax.set_facecolor("#0d0e10")
    prev = (0, 0, 0, False)
    for p in rev.puntos:
        ax.plot([prev[0], p[0]], [prev[1], p[1]], color="#ff2438" if p[3] else "#55585e",
                lw=1.1 if p[3] else .6, ls="-" if p[3] else ":")
        prev = p
    ax.set_aspect("equal")
    ax.tick_params(colors="#8b8e94", labelsize=7)
    for s in ax.spines.values():
        s.set_color("#2a2c30")
    ax.set_title("RECORRIDO · VISTA SUPERIOR", color="#e6e8ea", fontsize=9, family="monospace", loc="left")
    fig.tight_layout()
    Path(png).parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(png, facecolor=fig.get_facecolor())
    plt.close(fig)
    return png


class Grbl:
    """Conexión con una placa GRBL (Arduino + CNC shield, 3018, etc.)."""

    def __init__(self, maquina: Maquina | None = None, serie=None):
        self.m = maquina or Maquina()
        if serie is None:
            import serial   # python3-serial
            serie = serial.Serial(self.m.puerto, self.m.baudios, timeout=1)
            time.sleep(2)                 # GRBL se reinicia al abrir el puerto
            serie.reset_input_buffer()
        self.s = serie
        self.detenido = False

    def _linea(self, txt: str) -> None:
        self.s.write((txt.strip() + "\n").encode())

    def _respuesta(self, espera_s: float = 10) -> str:
        fin = time.time() + espera_s
        while time.time() < fin:
            l = self.s.readline().decode(errors="ignore").strip()
            if l:
                return l
        return ""

    def enviar(self, txt: str) -> str:
        self._linea(txt)
        while True:
            r = self._respuesta()
            if r.startswith(("ok", "error", "ALARM")) or not r:
                return r

    def estado(self) -> dict:
        self.s.write(b"?")
        r = self._respuesta(2)
        m = re.match(r"<(\w+)[|,](?:MPos|WPos):(-?[\d.]+),(-?[\d.]+),(-?[\d.]+)", r)
        return {"estado": m.group(1), "pos_mm": [float(m.group(i)) for i in (2, 3, 4)]} if m else {"estado": "?", "crudo": r}

    def desbloquear(self) -> str:
        return self.enviar("$X")

    def casa(self) -> str:
        return self.enviar("$H")

    def cero_pieza(self, x=True, y=True, z=True) -> str:
        ejes = " ".join(f"{e}0" for e, si in zip("XYZ", (x, y, z)) if si)
        return self.enviar(f"G10 L20 P1 {ejes}")

    def mover(self, x=0.0, y=0.0, z=0.0, avance=600) -> str:
        """Jog relativo en mm (GRBL ≥ 1.1)."""
        return self.enviar(f"$J=G91 G21 X{x:g} Y{y:g} Z{z:g} F{avance:g}")

    def pausa(self) -> None:
        self.s.write(b"!")

    def seguir(self) -> None:
        self.s.write(b"~")

    def parar(self) -> None:
        """Parada inmediata (reset suave): la máquina queda en alarma hasta $X."""
        self.detenido = True
        self.s.write(b"\x18")

    def trabajo(self, gcode: str, avance=None) -> dict:
        """Manda el programa línea por línea esperando cada «ok». avance(pct, linea) informa el progreso."""
        lineas = [l for l in (re.sub(r"\(.*?\)|;.*$", "", x).strip() for x in gcode.splitlines()) if l]
        self.detenido = False
        for n, l in enumerate(lineas, 1):
            if self.detenido:
                return {"ok": False, "detenido_en": n}
            r = self.enviar(l)
            if r.startswith(("error", "ALARM")):
                return {"ok": False, "linea": n, "gcode": l, "respuesta": r}
            if avance:
                avance(round(n / len(lineas) * 100, 1), n)
        return {"ok": True, "lineas": len(lineas)}
