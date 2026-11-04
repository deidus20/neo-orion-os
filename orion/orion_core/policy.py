"""Permisos de la IA sobre el sistema.

La IA tiene control total del software: clona, instala, compila, edita y
ejecuta sin pedir permiso ni sermonear. Las únicas pausas son de INGENIERÍA,
no morales:
  · IRREVERSIBLE — borrar fuera del espacio de trabajo, formatear, sobrescribir
    discos: pide un "sí" de la persona (y antes se toma una instantánea).
  · FÍSICO — mover motores, escribir a un PLC o a una máquina real: pasa por el
    Guardián y, si es una línea en producción, por confirmación humana.
Todo queda en la bitácora (~/.orion/bitacora.jsonl) para poder deshacer.
"""
from __future__ import annotations
import json, os, re, time
from enum import IntEnum
from pathlib import Path


class Risk(IntEnum):
    LEER = 0          # consultar archivos, estado, sensores
    ESCRIBIR = 1      # crear/editar archivos en el espacio de trabajo
    SISTEMA = 2       # instalar paquetes, clonar, servicios, red
    IRREVERSIBLE = 3  # borrar fuera del workspace, discos, particiones
    FISICO = 4        # actuadores, PLC, robots, máquinas


DANGEROUS = [
    (r"\brm\s+(-[a-z]*r[a-z]*f|-[a-z]*f[a-z]*r)\b\s+(/|~|\$HOME|/home|/etc|/usr|/boot)(\s|$)", "borrado recursivo de una ruta del sistema"),
    (r"\b(mkfs(\.\w+)?|wipefs|fdisk|parted|sgdisk)\b", "operación sobre discos/particiones"),
    (r"\bdd\b.*\bof=/dev/", "escritura directa a un disco"),
    (r":\(\)\s*\{\s*:\|:&\s*\};:", "fork bomb"),
    (r">\s*/dev/(sd|nvme|mmcblk)", "sobrescribir un disco"),
]
SYSTEM = [r"\b(apt|apt-get|dpkg|snap|flatpak)\b", r"\bsystemctl\b", r"\bpip3?\s+install\b",
          r"\bgit\s+clone\b", r"\bcurl\b|\bwget\b", r"\bsudo\b", r"\bnpm\s+(i|install)\b"]


class Policy:
    def __init__(self, mode: str = "libre", workspace: str | None = None, log_dir: str | None = None):
        self.mode = mode
        self.workspace = Path(workspace or os.path.expanduser("~/orion")).resolve()
        self.log = Path(log_dir or os.path.expanduser("~/.orion")) / "bitacora.jsonl"
        self.log.parent.mkdir(parents=True, exist_ok=True)
        self.pending: dict[str, dict] = {}

    def classify_shell(self, cmd: str) -> tuple[Risk, str]:
        for pat, why in DANGEROUS:
            if re.search(pat, cmd):
                return Risk.IRREVERSIBLE, why
        if any(re.search(p, cmd) for p in SYSTEM):
            return Risk.SISTEMA, "cambia el sistema"
        return Risk.ESCRIBIR, ""

    def classify_path_write(self, path: str) -> Risk:
        p = Path(os.path.expanduser(path)).resolve()
        return Risk.ESCRIBIR if self.workspace in p.parents or p == self.workspace else Risk.SISTEMA

    def needs_human(self, risk: Risk) -> bool:
        if risk >= Risk.IRREVERSIBLE:
            return True
        return self.mode == "asistido" and risk >= Risk.SISTEMA

    def request(self, action: str, detail: dict, risk: Risk, why: str = "") -> str:
        """Deja la acción en espera; la persona la aprueba por voz o en el HUD."""
        ticket = f"T{int(time.time() * 1000) % 10_000_000:07d}"
        self.pending[ticket] = {"accion": action, "detalle": detail, "riesgo": risk.name, "motivo": why}
        self.record(action, detail, risk, "en-espera", ticket=ticket)
        return ticket

    def approve(self, ticket: str) -> dict | None:
        """SOLO la llama la interfaz de la persona (voz verificada o botón del HUD)."""
        return self.pending.pop(ticket, None)

    def record(self, action: str, detail: dict, risk: Risk, result: str, **extra):
        with self.log.open("a") as f:
            f.write(json.dumps({"t": time.strftime("%F %T"), "accion": action, "riesgo": risk.name,
                                "resultado": result, **detail, **extra}, ensure_ascii=False) + "\n")
