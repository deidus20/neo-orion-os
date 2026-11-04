"""Casa inteligente por voz con Home Assistant: «apaga la luz del taller», «¿qué temperatura hay en la pieza?».

Home Assistant (Apache-2.0) es el centro: habla con Zigbee, Wi-Fi (Shelly, Tuya, Sonoff), Matter, cámaras…
Neo Orión usa su API REST local con un token de acceso de larga duración que la PERSONA crea en Home Assistant
(Perfil → Seguridad → Tokens) y pega en ~/.orion/claves/home-assistant (permiso 600). El token no pasa por chats.

Seguridad: cerraduras, alarmas, portones y garajes son acciones FÍSICAS: piden confirmación (ticket). Encender
luces o enchufes no.
"""
from __future__ import annotations

import difflib
import json
import os
import re
import unicodedata
import urllib.request
from pathlib import Path

URL = os.environ.get("ORION_HA_URL", "http://homeassistant.local:8123")
TOKEN = Path("~/.orion/claves/home-assistant").expanduser()
SENSIBLES = ("lock.", "alarm_control_panel.", "cover.garage", "cover.porton", "cover.gate")
ACCIONES = [
    (r"\b(apaga|apagar|desactiva|cierra la llave de)\b", "turn_off"),
    (r"\b(enciende|encender|prende|prender|activa|abre la llave de)\b", "turn_on"),
    (r"\b(alterna|cambia)\b", "toggle"),
    (r"\b(desbloquea|abre la cerradura)\b", "unlock"),
    (r"\b(bloquea|cierra con llave|cierra la cerradura)\b", "lock"),
    (r"\b(abre|sube)\b", "open_cover"),
    (r"\b(cierra|baja)\b", "close_cover"),
]


def _norm(s: str) -> str:
    s = unicodedata.normalize("NFD", s.lower())
    return "".join(c for c in s if unicodedata.category(c) != "Mn")


class Casa:
    def __init__(self, url: str = URL, token: str | None = None, abrir=urllib.request.urlopen):
        self.url = url.rstrip("/")
        self.token = token if token is not None else (TOKEN.read_text().strip() if TOKEN.exists() else "")
        self._abrir = abrir

    def _req(self, metodo: str, ruta: str, datos=None):
        if not self.token:
            raise PermissionError("Falta el token de Home Assistant: créalo en tu perfil de HA y guárdalo en "
                                  "~/.orion/claves/home-assistant")
        req = urllib.request.Request(self.url + ruta, method=metodo, data=json.dumps(datos).encode() if datos is not None else None,
                                     headers={"Authorization": f"Bearer {self.token}", "Content-Type": "application/json"})
        with self._abrir(req, timeout=10) as r:
            return json.loads(r.read() or b"null")

    def estados(self) -> list[dict]:
        return self._req("GET", "/api/states")

    def buscar(self, texto: str, dominios: tuple = ()) -> dict | None:
        """La entidad cuyo nombre se parece más a lo dicho («luz del taller» → light.taller)."""
        objetivo = _norm(re.sub(r"\b(apaga|enciende|prende|abre|cierra|sube|baja|la|el|los|las|del|de|al|a)\b", " ", texto))
        objetivo = re.sub(r"\s+", " ", objetivo).strip()
        mejor, puntaje = None, 0.0
        for e in self.estados():
            if dominios and not e["entity_id"].startswith(dominios):
                continue
            nombre = _norm(e.get("attributes", {}).get("friendly_name", "") + " " + e["entity_id"].split(".", 1)[1].replace("_", " "))
            p = difflib.SequenceMatcher(None, objetivo, nombre).ratio()
            p += 0.25 * sum(1 for w in objetivo.split() if len(w) > 2 and w in nombre)
            if p > puntaje:
                mejor, puntaje = e, p
        return mejor if puntaje >= 0.55 else None

    def ordenar(self, texto: str, confirmado: bool = False) -> dict:
        t = _norm(texto)
        servicio = next((s for pat, s in ACCIONES if re.search(pat, t)), None)
        if servicio is None:
            e = self.buscar(texto, ("sensor.", "climate.", "binary_sensor."))
            if not e:
                return {"ok": False, "error": "no entendí qué aparato"}
            a = e.get("attributes", {})
            return {"ok": True, "consulta": a.get("friendly_name", e["entity_id"]), "valor": e["state"],
                    "unidad": a.get("unit_of_measurement", "")}
        dominios = {"open_cover": ("cover.",), "close_cover": ("cover.",), "lock": ("lock.",), "unlock": ("lock.",)}.get(
            servicio, ("light.", "switch.", "fan.", "climate.", "media_player.", "input_boolean.", "valve."))
        e = self.buscar(texto, dominios)
        if not e:
            return {"ok": False, "error": "no encontré ese aparato en Home Assistant"}
        ent = e["entity_id"]
        if ent.startswith(SENSIBLES) and not confirmado:
            return {"ok": False, "necesita_confirmar": True, "entidad": ent, "servicio": servicio,
                    "motivo": "cerraduras, alarmas y portones necesitan tu confirmación"}
        dominio = ent.split(".")[0]
        if servicio in ("turn_on", "turn_off", "toggle") and dominio not in ("light", "switch", "fan", "climate", "media_player", "input_boolean"):
            dominio = "homeassistant"
        self._req("POST", f"/api/services/{dominio}/{servicio}", {"entity_id": ent})
        return {"ok": True, "entidad": ent, "nombre": e.get("attributes", {}).get("friendly_name", ent), "servicio": servicio}


INSTALAR = ["sudo apt-get install -y docker.io",
            "sudo docker run -d --name homeassistant --restart=unless-stopped --privileged -e TZ=America/Santiago "
            "-v ~/.orion/home-assistant:/config -v /run/dbus:/run/dbus:ro --network=host "
            "ghcr.io/home-assistant/home-assistant:stable",
            "# abre http://localhost:8123, crea tu cuenta y luego un token en Perfil → Seguridad",
            "install -m 600 /dev/stdin ~/.orion/claves/home-assistant   # pega el token y Ctrl-D"]
