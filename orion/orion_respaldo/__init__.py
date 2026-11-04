"""Respaldo automático de lo que importa: la memoria (~/Orion), los proyectos y la configuración de Neo Orión.

Destinos:
  · disco externo o pendrive (se detecta solo: cualquier disco extraíble montado con espacio)
  · una carpeta cualquiera (otro disco interno, un NAS montado)
  · la nube con rclone (Google Drive, OneDrive, Dropbox, S3…): la persona conecta su cuenta UNA vez con
    `rclone config` en su equipo; la clave nunca pasa por Neo Orión ni por un chat.

Cada respaldo local es una foto con fecha (AAAA-MM-DD_HHMM) hecha con rsync --link-dest: lo que no cambió se
enlaza al respaldo anterior, así cien respaldos ocupan casi lo mismo que uno. Se guardan los últimos 30.
"""
from __future__ import annotations

import datetime as dt
import json
import os
import shutil
import subprocess
from pathlib import Path

QUE = ["~/Orion", "~/Proyectos", "~/.orion", "/etc/orion"]
EXCLUIR = [".orion/indice.db-wal", "*.tmp", "__pycache__", ".cache", "node_modules", "chrome-perfil", "firefox-perfil"]
CONSERVAR = 30
CONFIG = Path("~/.orion/respaldo.json").expanduser()


def configuracion() -> dict:
    try:
        return json.loads(CONFIG.read_text())
    except (OSError, ValueError):
        return {"destino": "auto", "que": QUE, "cada": "diario"}


def guardar_configuracion(c: dict) -> None:
    CONFIG.parent.mkdir(parents=True, exist_ok=True)
    CONFIG.write_text(json.dumps(c, ensure_ascii=False, indent=2))


def discos_externos() -> list[dict]:
    """Discos extraíbles montados con espacio libre (pendrive, disco USB)."""
    try:
        data = json.loads(subprocess.run(["lsblk", "-J", "-b", "-o", "NAME,RM,HOTPLUG,MOUNTPOINT,FSAVAIL,LABEL,TRAN"],
                                         capture_output=True, text=True, timeout=10).stdout or "{}")
    except (OSError, ValueError, subprocess.SubprocessError):
        return []
    out = []

    def rec(n, ext=False):
        ext = ext or bool(n.get("rm")) or bool(n.get("hotplug")) or n.get("tran") == "usb"
        mp = n.get("mountpoint")
        if ext and mp and mp not in ("/", "/boot", "/boot/efi") and not mp.startswith(("/cdrom", "/snap")):
            out.append({"montaje": mp, "libre_gb": round(int(n.get("fsavail") or 0) / 1e9, 1), "etiqueta": n.get("label") or n["name"]})
        for h in n.get("children", []) or []:
            rec(h, ext)
    for n in data.get("blockdevices", []):
        rec(n)
    return sorted(out, key=lambda d: -d["libre_gb"])


def destino_local(c: dict) -> Path | None:
    d = c.get("destino", "auto")
    if d == "auto":
        ext = discos_externos()
        return Path(ext[0]["montaje"]) / "NeoOrion-respaldo" if ext else None
    if d.startswith("rclone:"):
        return None
    return Path(d).expanduser()


def respaldar(c: dict | None = None, ahora: dt.datetime | None = None, ejecutar=subprocess.run) -> dict:
    c = c or configuracion()
    fuentes = [Path(q).expanduser() for q in c.get("que", QUE)]
    fuentes = [f for f in fuentes if f.exists()]
    if not fuentes:
        return {"ok": False, "error": "no hay nada que respaldar todavía"}
    d = c.get("destino", "auto")
    sello = (ahora or dt.datetime.now()).strftime("%Y-%m-%d_%H%M")
    if d.startswith("rclone:"):
        if not shutil.which("rclone"):
            return {"ok": False, "error": "falta rclone: sudo apt install rclone y luego `rclone config` para conectar tu nube"}
        remoto = d.split(":", 1)[1].rstrip("/")
        hechos = []
        for f in fuentes:
            r = ejecutar(["rclone", "sync", str(f), f"{remoto}/actual/{f.name.lstrip('.')}",
                          "--backup-dir", f"{remoto}/anteriores/{sello}/{f.name.lstrip('.')}",
                          *sum([["--exclude", e] for e in EXCLUIR], [])], capture_output=True, text=True)
            hechos.append({"carpeta": str(f), "ok": r.returncode == 0})
        return {"ok": all(h["ok"] for h in hechos), "destino": d, "sello": sello, "carpetas": hechos}
    base = destino_local(c)
    if base is None:
        return {"ok": False, "error": "no hay disco externo conectado; conecta uno o elige una carpeta o tu nube"}
    base.mkdir(parents=True, exist_ok=True)
    previos = sorted(p for p in base.iterdir() if p.is_dir() and p.name[:4].isdigit())
    nuevo = base / sello
    nuevo.mkdir(exist_ok=True)
    hechos = []
    for f in fuentes:
        destino = nuevo / f.name.lstrip(".")
        cmd = ["rsync", "-a", "--delete", *sum([["--exclude", e] for e in EXCLUIR], [])]
        if previos:
            cmd += ["--link-dest", str(previos[-1] / f.name.lstrip("."))]
        r = ejecutar(cmd + [str(f) + "/", str(destino)], capture_output=True, text=True)
        hechos.append({"carpeta": str(f), "ok": r.returncode == 0, "error": (r.stderr or "")[-200:] if r.returncode else ""})
    for viejo in previos[:max(0, len(previos) - (CONSERVAR - 1))]:   # quedan CONSERVAR fotos contando la nueva
        shutil.rmtree(viejo, ignore_errors=True)
    (base / "ULTIMO.txt").write_text(sello + "\n")
    return {"ok": all(h["ok"] for h in hechos), "destino": str(nuevo), "sello": sello, "carpetas": hechos}


def listar(c: dict | None = None) -> list[str]:
    base = destino_local(c or configuracion())
    if not base or not base.exists():
        return []
    return sorted((p.name for p in base.iterdir() if p.is_dir() and p.name[:4].isdigit()), reverse=True)


def restaurar(sello: str, carpeta: str, a: str | None = None, c: dict | None = None, ejecutar=subprocess.run) -> dict:
    """Trae de vuelta una carpeta de un respaldo. Por defecto a ~/Restaurado/<sello>/ (no pisa lo actual)."""
    base = destino_local(c or configuracion())
    if not base:
        return {"ok": False, "error": "no encuentro el disco del respaldo"}
    origen = base / sello / carpeta.lstrip(".")
    if not origen.exists():
        return {"ok": False, "error": f"no existe {origen}"}
    destino = Path(a).expanduser() if a else Path.home() / "Restaurado" / sello / carpeta.lstrip(".")
    destino.mkdir(parents=True, exist_ok=True)
    r = ejecutar(["rsync", "-a", str(origen) + "/", str(destino)], capture_output=True, text=True)
    return {"ok": r.returncode == 0, "en": str(destino)}


TIMER = """[Unit]
Description=Neo Orión · respaldo diario

[Timer]
OnCalendar=*-*-* 21:30
RandomizedDelaySec=20min
Persistent=true

[Install]
WantedBy=timers.target
"""
SERVICIO = """[Unit]
Description=Neo Orión · respaldo de memoria y proyectos

[Service]
Type=oneshot
Environment=PYTHONPATH=/opt/orion
ExecStart=/opt/orion/venv/bin/python3 -m orion_respaldo
Nice=15
IOSchedulingClass=idle
"""

if __name__ == "__main__":
    r = respaldar()
    print(json.dumps(r, ensure_ascii=False))
    raise SystemExit(0 if r.get("ok") else 1)
