"""Modo patata por RENDIMIENTO, no por ficha técnica.

Un PC "patata" es el que no está dando el rendimiento, sea viejo o no: un notebook
bueno sin cargador, recalentado, con poca RAM libre o con la IA respondiendo lenta.
Este módulo mide en vivo y baja (o sube) el nivel solo:

  NORMAL   → todo encendido.
  LIVIANO  → HUD sin efectos ni 3D, IA local más chica.
  PATATA   → además: oído mínimo, IA local solo para órdenes, lo pesado a la nube.

Señales (Linux): presión de memoria y CPU (/proc/pressure, PSI), carga por núcleo,
temperatura y bajada de frecuencia, batería, y la velocidad real de la IA (tokens/s)
que reporta el propio asistente. Para no oscilar usa histéresis: baja rápido
(≈15 s malos) y sube lento (≈2 min buenos).
"""
from __future__ import annotations

import os
import re
import time
from dataclasses import dataclass, field
from pathlib import Path

NIVELES = ["NORMAL", "LIVIANO", "PATATA"]

# umbrales: (liviano, patata)
UMBRAL = {
    "mem_psi": (10.0, 30.0),      # % del tiempo con tareas esperando memoria (avg10)
    "cpu_psi": (40.0, 75.0),      # % del tiempo con tareas esperando CPU
    "carga": (1.2, 2.0),          # carga media 1 min por núcleo
    "tok_s": (8.0, 3.0),          # IA: menos de 8 tok/s ya cuesta conversar; menos de 3, no sirve
    "temp": (88.0, 96.0),         # °C del paquete de CPU
    "fps": (40.0, 24.0),          # cuadros por segundo del HUD
}


def _psi(nombre: str, raiz: str = "/proc/pressure") -> float | None:
    try:
        m = re.search(r"some avg10=([\d.]+)", Path(raiz, nombre).read_text())
        return float(m.group(1)) if m else None
    except OSError:
        return None


def _temp() -> float | None:
    mejor = None
    for z in Path("/sys/class/thermal").glob("thermal_zone*"):
        try:
            tipo = (z / "type").read_text().strip().lower()
            if any(k in tipo for k in ("x86_pkg", "cpu", "soc", "k10temp", "acpitz")):
                t = int((z / "temp").read_text()) / 1000
                mejor = max(mejor or 0, t)
        except (OSError, ValueError):
            pass
    return mejor


def _bateria() -> tuple[bool, int | None]:
    for b in Path("/sys/class/power_supply").glob("BAT*"):
        try:
            estado = (b / "status").read_text().strip()
            cap = int((b / "capacity").read_text())
            return estado == "Discharging", cap
        except (OSError, ValueError):
            pass
    return False, None


def medir(extra: dict | None = None) -> dict:
    """Una lectura. `extra` trae lo que mide el asistente o el HUD: tok_s, fps."""
    nucleos = os.cpu_count() or 1
    try:
        carga = os.getloadavg()[0] / nucleos
    except OSError:
        carga = None
    desc, cap = _bateria()
    m = {"mem_psi": _psi("memory"), "cpu_psi": _psi("cpu"), "carga": carga, "temp": _temp(),
         "bateria_descargando": desc, "bateria": cap}
    m.update(extra or {})
    return m


def nivel_de(m: dict) -> tuple[str, list[str]]:
    """Nivel que corresponde a UNA lectura y las razones (en palabras simples)."""
    peor, razones = 0, []
    nombres = {"mem_psi": "falta memoria", "cpu_psi": "CPU saturada", "carga": "CPU saturada",
               "tok_s": "la IA responde lento", "temp": "el procesador está muy caliente", "fps": "la pantalla va a tirones"}
    for k, (liv, pat) in UMBRAL.items():
        v = m.get(k)
        if v is None:
            continue
        menor_es_peor = k in ("tok_s", "fps")
        malo = (lambda x, u: x < u) if menor_es_peor else (lambda x, u: x > u)
        if malo(v, pat):
            peor = 2; razones.append(nombres[k])
        elif malo(v, liv):
            peor = max(peor, 1); razones.append(nombres[k])
    if m.get("bateria_descargando") and (m.get("bateria") or 100) < 20:
        peor = max(peor, 1); razones.append("batería baja")
    return NIVELES[peor], sorted(set(razones))


@dataclass
class Vigia:
    """Decide el nivel con histéresis y dice qué cambiar."""
    bajar_s: float = 15.0
    subir_s: float = 120.0
    nivel: str = "NORMAL"
    fijo: str | None = None              # la persona lo fijó por voz ("modo patata" / "modo completo")
    _desde: dict = field(default_factory=dict)

    def actualizar(self, m: dict, ahora: float | None = None) -> dict | None:
        ahora = time.monotonic() if ahora is None else ahora
        if self.fijo:
            return None
        objetivo, razones = nivel_de(m)
        i_obj, i_act = NIVELES.index(objetivo), NIVELES.index(self.nivel)
        if i_obj == i_act:
            self._desde.clear(); return None
        clave = "bajar" if i_obj > i_act else "subir"
        self._desde = {clave: self._desde.get(clave, ahora)}
        espera = self.bajar_s if clave == "bajar" else self.subir_s
        if ahora - self._desde[clave] < espera:
            return None
        # sube de a un nivel; baja directo al que corresponde
        nuevo = objetivo if clave == "bajar" else NIVELES[i_act - 1]
        anterior, self.nivel = self.nivel, nuevo
        self._desde.clear()
        return {"de": anterior, "a": nuevo, "razones": razones, "acciones": acciones(nuevo),
                "aviso": aviso(anterior, nuevo, razones)}


def acciones(nivel: str) -> list[str]:
    if nivel == "NORMAL":
        return ["hud:completo", "ia:modelo-recomendado", "oido:normal"]
    if nivel == "LIVIANO":
        return ["hud:liviano", "ia:modelo-un-paso-menor", "oido:normal"]
    return ["hud:liviano", "ia:modelo-minimo-para-ordenes", "ia:pesado-a-la-nube", "oido:vosk", "servicios:pausar-no-esenciales"]


def aviso(de: str, a: str, razones: list[str]) -> str:
    porque = " y ".join(razones) if razones else "el equipo va justo"
    if NIVELES.index(a) > NIVELES.index(de):
        return f"Bajé a modo {a.lower()} porque {porque}. Di «modo completo» si prefieres todo encendido."
    return "El equipo se recuperó: vuelvo a encender lo que había bajado."


def vigilar(cada: float = 5.0, salida: str = "~/.orion/rendimiento.json"):
    """Servicio: mide cada pocos segundos y deja el estado para el HUD y el asistente.
    El asistente escribe su velocidad en ~/.orion/ia_tok_s; el HUD, sus FPS en ~/.orion/hud_fps."""
    import json
    base = Path(os.path.expanduser("~/.orion")); base.mkdir(parents=True, exist_ok=True)
    out, v = Path(os.path.expanduser(salida)), Vigia()
    fijo = base / "nivel_fijo"
    while True:
        extra = {}
        for k, f in (("tok_s", "ia_tok_s"), ("fps", "hud_fps")):
            try:
                extra[k] = float((base / f).read_text())
            except (OSError, ValueError):
                pass
        v.fijo = fijo.read_text().strip() if fijo.exists() else None
        m = medir(extra)
        cambio = v.actualizar(m)
        out.write_text(json.dumps({"nivel": v.fijo or v.nivel, "medicion": m, "cambio": cambio}, ensure_ascii=False))
        time.sleep(cada)


if __name__ == "__main__":
    vigilar()
