"""Memoria del robot: posiciones guardadas (macros tipo "1A") y calibración.

Todo queda en JSON legible en ~/.orion/robots/<nombre>/ para que la IA,
la persona y git puedan leerlo y versionarlo.

Calibración por foto de referencia:
  la persona marca sobre la foto dónde está cada servo (x, y en píxeles),
  desde qué vista se tomó y hacia dónde es positivo el giro. Después el
  sistema manda un ángulo de prueba, la cámara mide el ángulo real y se
  corrige offset y dirección hasta que coincidan.
"""
from __future__ import annotations
import json, os, time
from pathlib import Path


class RobotMemory:
    def __init__(self, robot_name: str, root: str | None = None):
        safe = "".join(c if c.isalnum() else "-" for c in robot_name.lower()).strip("-")
        self.dir = Path(root or os.path.expanduser("~/.orion/robots")) / safe
        self.dir.mkdir(parents=True, exist_ok=True)
        self.macros_file = self.dir / "posiciones.json"
        self.calib_file = self.dir / "calibracion.json"
        self.log_file = self.dir / "bitacora.jsonl"

    # -- posiciones / macros ---------------------------------------------
    def macros(self) -> dict:
        return json.loads(self.macros_file.read_text()) if self.macros_file.exists() else {}

    def save_macro(self, name: str, pose: dict, descripcion: str = ""):
        m = self.macros()
        m[name.upper()] = {"pose": pose, "descripcion": descripcion, "creado": time.strftime("%F %T")}
        self.macros_file.write_text(json.dumps(m, ensure_ascii=False, indent=2))

    # -- calibración ---------------------------------------------------
    def calibration(self) -> dict:
        return json.loads(self.calib_file.read_text()) if self.calib_file.exists() else {"foto": None, "servos": {}}

    def mark_servo(self, joint: str, px: tuple[int, int], vista: str, foto: str):
        c = self.calibration()
        c["foto"] = foto
        c["servos"].setdefault(joint, {})
        c["servos"][joint].update({"px": list(px), "vista": vista})
        self.calib_file.write_text(json.dumps(c, ensure_ascii=False, indent=2))

    def record_trial(self, joint: str, commanded: float, measured: float):
        """Cada prueba de calibración: ángulo mandado vs. ángulo medido por cámara."""
        c = self.calibration()
        s = c["servos"].setdefault(joint, {})
        s.setdefault("pruebas", []).append([commanded, measured])
        pts = s["pruebas"]
        if len(pts) >= 2:   # ajuste lineal: medido = dir·mandado + offset
            n = len(pts)
            mx = sum(p[0] for p in pts) / n
            my = sum(p[1] for p in pts) / n
            sxx = sum((p[0] - mx) ** 2 for p in pts) or 1e-9
            slope = sum((p[0] - mx) * (p[1] - my) for p in pts) / sxx
            s["direccion"] = 1 if slope >= 0 else -1
            s["escala"] = round(abs(slope), 4)
            s["offset"] = round(my - slope * mx, 2)
        self.calib_file.write_text(json.dumps(c, ensure_ascii=False, indent=2))
        return s

    def log(self, event: dict):
        with self.log_file.open("a") as f:
            f.write(json.dumps({"t": time.strftime("%F %T"), **event}, ensure_ascii=False) + "\n")
