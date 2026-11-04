"""Escritorio remoto: entrar al PC de Neo Orión desde el teléfono u otro computador.

Dos caminos, los dos libres:
  · EN LA CASA (misma red): wayvnc comparte la pantalla de Neo Orión (cage es Wayland/wlroots, wayvnc es su
    servidor VNC oficial). Con clave y cifrado TLS; se abre desde el teléfono con cualquier app VNC (o noVNC en
    el navegador). Solo escucha en la red local.
  · DESDE AFUERA: RustDesk (AGPL-3.0). Da un ID y una clave de un solo uso; se puede usar el servidor público de
    RustDesk o montar uno propio (hbbs/hbbr) para que nada pase por terceros.

La clave de acceso la elige la persona y queda en ~/.config/wayvnc/ (permiso 600); Neo Orión nunca la dice en voz
alta ni la muestra en el HUD salvo que se pida, y el acceso remoto se apaga con «apaga el acceso remoto».
"""
from __future__ import annotations

import secrets
import shutil
import socket
import string
import subprocess
from pathlib import Path

CONF = Path("~/.config/wayvnc").expanduser()
PUERTO = 5900


def ip_local() -> str:
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    try:
        s.connect(("10.255.255.255", 1))
        return s.getsockname()[0]
    except OSError:
        return "127.0.0.1"
    finally:
        s.close()


def clave_nueva(n: int = 10) -> str:
    alf = string.ascii_letters + string.digits
    return "".join(secrets.choice(alf) for _ in range(n))


def preparar_vnc(usuario: str = "orion", clave: str | None = None, conf: Path = CONF, ejecutar=subprocess.run) -> dict:
    """Escribe la configuración de wayvnc con TLS (certificado propio) y clave."""
    conf.mkdir(parents=True, exist_ok=True)
    clave = clave or clave_nueva()
    if not (conf / "tls_cert.pem").exists():
        ejecutar(["openssl", "req", "-x509", "-newkey", "rsa:3072", "-sha256", "-days", "3650", "-nodes",
                  "-keyout", str(conf / "tls_key.pem"), "-out", str(conf / "tls_cert.pem"),
                  "-subj", "/CN=neo-orion"], capture_output=True)
    cfg = conf / "config"
    cfg.write_text(f"address=0.0.0.0\nport={PUERTO}\nenable_auth=true\nusername={usuario}\npassword={clave}\n"
                   f"private_key_file={conf / 'tls_key.pem'}\ncertificate_file={conf / 'tls_cert.pem'}\n")
    cfg.chmod(0o600)
    return {"ok": True, "direccion": f"{ip_local()}:{PUERTO}", "usuario": usuario, "clave_guardada_en": str(cfg),
            "clave": clave}


def encender_vnc(ejecutar=subprocess.Popen) -> dict:
    if not shutil.which("wayvnc"):
        return {"ok": False, "error": "falta wayvnc (sudo apt install wayvnc)"}
    if not (CONF / "config").exists():
        preparar_vnc()
    ejecutar(["wayvnc", "--config", str(CONF / "config")], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return {"ok": True, "direccion": f"{ip_local()}:{PUERTO}", "como": "app VNC del teléfono (RealVNC Viewer, bVNC, AVNC)"}


def apagar(ejecutar=subprocess.run) -> dict:
    for p in ("wayvnc", "rustdesk"):
        ejecutar(["pkill", "-x", p], capture_output=True)
    return {"ok": True, "mensaje": "acceso remoto apagado"}


def rustdesk_id(ejecutar=subprocess.run) -> dict:
    if not shutil.which("rustdesk"):
        return {"ok": False, "error": "RustDesk no está instalado", "instalar": INSTALAR_RUSTDESK}
    r = ejecutar(["rustdesk", "--get-id"], capture_output=True, text=True, timeout=15)
    return {"ok": r.returncode == 0, "id": r.stdout.strip(), "nota": "la clave de un solo uso aparece en la app de RustDesk"}


def estado() -> dict:
    def corre(p):
        return subprocess.run(["pgrep", "-x", p], capture_output=True).returncode == 0
    return {"vnc": corre("wayvnc"), "rustdesk": corre("rustdesk"), "ip": ip_local(), "puerto_vnc": PUERTO}


INSTALAR_RUSTDESK = [
    "# baja el .deb oficial (x86_64) desde https://github.com/rustdesk/rustdesk/releases/latest",
    "sudo apt-get install -y ./rustdesk-*-x86_64.deb",
    "# opcional, servidor propio: https://rustdesk.com/docs/es/self-host/"]
