"""Cámaras y visión: Frigate (MIT) detecta personas, autos y mascotas en cámaras IP o USB, y avisa a Neo Orión.

Neo Orión arma la configuración de Frigate desde una lista simple de cámaras, la levanta con Docker y escucha sus
eventos por MQTT. Un evento «persona» en una zona marcada como «robot» o «cnc» le avisa al guardián: la máquina
se frena igual que cuando el LiDAR ve a alguien. Los videos quedan en el equipo (nada sube a la nube).

Las direcciones RTSP llevan usuario y clave de la cámara: la persona las escribe en ~/.orion/claves/camaras.json
(permiso 600); en la configuración se usan variables {FRIGATE_RTSP_...} y nunca la clave en texto.
"""
from __future__ import annotations

import json
import re
import unicodedata
from pathlib import Path

CLAVES = Path("~/.orion/claves/camaras.json").expanduser()
CARPETA = Path("~/.orion/frigate").expanduser()
OBJETOS = ["person", "car", "dog", "cat"]
ZONAS_GUARDIAN = ("robot", "cnc", "taller", "maquina")


def _id(nombre: str) -> str:
    s = unicodedata.normalize("NFD", nombre.lower())
    s = "".join(c for c in s if unicodedata.category(c) != "Mn")
    return re.sub(r"[^a-z0-9]+", "_", s).strip("_") or "camara"


def configuracion(camaras: list[dict], detector: str = "cpu", mqtt: str = "127.0.0.1", grabar_dias: int = 7) -> str:
    """camaras: [{"nombre": "Taller", "fuente": "rtsp" | "usb", "dispositivo": "/dev/video0", "zonas": {"cnc": [[x,y],...]}}]
    Devuelve el YAML de Frigate (sin claves: las URLs RTSP salen de variables de entorno)."""
    lineas = ["# Generado por Neo Orión (orion_vision). Las claves RTSP vienen de variables FRIGATE_RTSP_*.",
              "mqtt:", f"  host: {mqtt}", "  topic_prefix: frigate",
              "detectors:", "  principal:", f"    type: {'openvino' if detector == 'intel' else 'edgetpu' if detector == 'coral' else 'cpu'}"]
    if detector == "coral":
        lineas.append("    device: usb")
    if detector == "intel":
        lineas.append("    device: GPU")
    lineas += ["record:", "  enabled: true", "  retain:", f"    days: {grabar_dias}", "    mode: motion",
               "snapshots:", "  enabled: true", "objects:", "  track: [" + ", ".join(OBJETOS) + "]", "cameras:"]
    for c in camaras:
        cid = _id(c["nombre"])
        if c.get("fuente", "rtsp") == "usb":
            lineas += [f"  {cid}:", "    ffmpeg:", "      input_args: -f v4l2 -input_format mjpeg -video_size 1280x720",
                       "      inputs:", f"        - path: {c.get('dispositivo', '/dev/video0')}", "          roles: [detect, record]"]
        else:
            lineas += [f"  {cid}:", "    ffmpeg:", "      inputs:", "        - path: \"{FRIGATE_RTSP_" + cid.upper() + "}\"",
                       "          roles: [detect, record]"]
        lineas += ["    detect:", "      width: 1280", "      height: 720", "      fps: 5"]
        if c.get("zonas"):
            lineas.append("    zones:")
            for zn, pts in c["zonas"].items():
                lineas += [f"      {_id(zn)}:", "        coordinates: " + ",".join(f"{x},{y}" for x, y in pts),
                           "        objects: [person]"]
    return "\n".join(lineas) + "\n"


def variables_entorno(camaras: list[dict], claves: dict | None = None) -> dict:
    """FRIGATE_RTSP_<ID> desde ~/.orion/claves/camaras.json ({"Taller": "rtsp://usuario:clave@ip:554/..."})."""
    if claves is None:
        try:
            claves = json.loads(CLAVES.read_text())
        except (OSError, ValueError):
            claves = {}
    return {f"FRIGATE_RTSP_{_id(c['nombre']).upper()}": claves[c["nombre"]] for c in camaras
            if c.get("fuente", "rtsp") != "usb" and c["nombre"] in claves}


def compose(detector: str = "cpu") -> str:
    dispositivos = {"coral": "      - /dev/bus/usb:/dev/bus/usb", "intel": "      - /dev/dri:/dev/dri"}.get(detector, "")
    return f"""services:
  mqtt:
    image: eclipse-mosquitto:2
    restart: unless-stopped
    ports: ["127.0.0.1:1883:1883"]
    command: mosquitto -c /mosquitto-no-auth.conf
  frigate:
    image: ghcr.io/blakeblackshear/frigate:stable
    restart: unless-stopped
    shm_size: "256mb"
    env_file: [.env]
    volumes:
      - ./config:/config
      - ./media:/media/frigate
      - /etc/localtime:/etc/localtime:ro
{('    devices:' + chr(10) + dispositivos) if dispositivos else ''}
    ports: ["127.0.0.1:5000:5000"]
"""


def preparar(camaras: list[dict], detector: str = "cpu", carpeta: Path = CARPETA) -> dict:
    (carpeta / "config").mkdir(parents=True, exist_ok=True)
    (carpeta / "config/config.yml").write_text(configuracion(camaras, detector, mqtt="mqtt"))
    (carpeta / "docker-compose.yml").write_text(compose(detector))
    env = carpeta / ".env"
    env.write_text("".join(f"{k}={v}\n" for k, v in variables_entorno(camaras).items()))
    env.chmod(0o600)
    return {"ok": True, "carpeta": str(carpeta), "levantar": f"cd {carpeta} && sudo docker compose up -d",
            "ver": "http://127.0.0.1:5000", "faltan_claves": [c["nombre"] for c in camaras if c.get("fuente", "rtsp") != "usb"
                                                               and f"FRIGATE_RTSP_{_id(c['nombre']).upper()}" not in variables_entorno(camaras)]}


def evento_guardian(evento: dict) -> dict | None:
    """Mensaje MQTT frigate/events → orden para el guardián si hay una persona en una zona de máquina."""
    d = evento.get("after") or evento
    if d.get("label") != "person":
        return None
    zonas = [z for z in d.get("current_zones", []) if any(k in z for k in ZONAS_GUARDIAN)]
    if not zonas:
        return None
    return {"accion": "frenar", "motivo": f"persona en la zona {zonas[0]} (cámara {d.get('camera', '?')})",
            "confianza": round(float(d.get("top_score") or d.get("score") or 0), 2)}


def escuchar(al_frenar, host: str = "127.0.0.1"):
    """Bucle MQTT (paho-mqtt). al_frenar(orden) se llama con cada persona en zona de máquina."""
    import paho.mqtt.client as mqtt

    def llega(_c, _u, msg):
        try:
            o = evento_guardian(json.loads(msg.payload))
        except ValueError:
            return
        if o:
            al_frenar(o)
    try:                                   # paho-mqtt 2.x
        c = mqtt.Client(mqtt.CallbackAPIVersion.VERSION2)
    except AttributeError:                 # paho-mqtt 1.6 (Ubuntu 24.04)
        c = mqtt.Client()
    c.on_message = llega
    c.connect(host, 1883)
    c.subscribe("frigate/events")
    c.loop_forever()
