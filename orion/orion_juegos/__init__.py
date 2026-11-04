"""Sala de juegos y medios de Neo Orión.

Juegos locales (Steam + Proton, Heroic, Lutris, emuladores), juegos en la nube
(GeForce NOW nativo, Xbox Cloud por navegador), transmisión desde otro PC de la
casa (Sunshine → Moonlight) y series/películas (Jellyfin, Kodi, plataformas
oficiales como app web).

Todo se maneja por voz o código: la persona dice "quiero jugar X" o "pon la
serie" y esto decide dónde correrlo según el equipo, lo instala si falta y lo
lanza. Teclado, mouse, mandos Xbox/PlayStation/Switch y genéricos se detectan
solos.
"""
from __future__ import annotations

import glob
import os
import re
import shutil
import subprocess
from dataclasses import dataclass, field


@dataclass
class Plataforma:
    clave: str
    nombre: str
    tipo: str            # local | nube | casa | medios
    instalar: list[str]  # comandos de instalación (en orden)
    lanzar: list[str]    # comando para abrir
    licencia: str
    repo: str = ""
    nota: str = ""
    necesita_gpu: bool = False
    detecta: list[str] = field(default_factory=list)   # binarios o ids flatpak que indican que ya está
    chile: str = ""      # disponibilidad en Chile (verificada nov-2026; revisar cada cierto tiempo)
    modelo: str = ""     # "tus juegos" (los traes de Steam/Epic…) | "catálogo" (incluidos) | "PC completo"


_FH = "flatpak remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo"

PLATAFORMAS = {p.clave: p for p in [
    Plataforma("steam", "Steam + Proton", "local",
               ["sudo dpkg --add-architecture i386", "sudo apt-get update", "sudo apt-get install -y steam-installer steam-devices"],
               ["steam"], "propietaria (gratis)", nota="Proton corre la mayoría de los juegos de Windows. Steam Input mapea cualquier mando.",
               necesita_gpu=True, detecta=["steam"]),
    Plataforma("heroic", "Heroic (Epic, GOG, Amazon)", "local",
               [_FH, "flatpak install -y flathub com.heroicgameslauncher.hgl"], ["flatpak", "run", "com.heroicgameslauncher.hgl"],
               "GPL-3.0", "Heroic-Games-Launcher/HeroicGamesLauncher", necesita_gpu=True, detecta=["com.heroicgameslauncher.hgl"]),
    Plataforma("lutris", "Lutris", "local", ["sudo apt-get install -y lutris"], ["lutris"], "GPL-3.0", "lutris/lutris",
               "Instaladores comunitarios para juegos fuera de Steam.", necesita_gpu=True, detecta=["lutris"]),
    Plataforma("retroarch", "RetroArch (emuladores)", "local", ["sudo apt-get install -y retroarch"], ["retroarch"], "GPL-3.0",
               "libretro/RetroArch", "Con tus propios juegos/ROMs y BIOS sacados de tus consolas.", detecta=["retroarch"]),
    Plataforma("geforcenow", "GeForce NOW", "nube",
               [_FH, "flatpak remote-add --if-not-exists GeForceNOW https://international.download.nvidia.com/GFNLinux/flatpak/geforcenow.flatpakrepo",
                "flatpak install -y GeForceNOW com.nvidia.geforcenow"],
               ["flatpak", "run", "com.nvidia.geforcenow"], "propietaria (cuenta NVIDIA)",
               nota="App nativa oficial para Ubuntu 24.04+. El juego corre en servidores RTX: sirve aunque el equipo no tenga GPU. "
                    "Pide H.264/H.265 por hardware y buena red (≥15 Mb/s 720p, ≥25 Mb/s 1080p).",
               detecta=["com.nvidia.geforcenow"], chile="sí (en Chile lo opera ABYA, socio de NVIDIA: revisar que la cuenta funcione en la app de Linux)",
               modelo="tus juegos"),
    Plataforma("xboxcloud", "Xbox Cloud Gaming", "nube", [], ["xdg-open", "https://www.xbox.com/play"], "servicio (Game Pass Ultimate)",
               nota="Por navegador (Chrome/Edge); funciona con mando. Se detectó un servidor 'ChileCentral' aún sin anuncio oficial.",
               chile="sí", modelo="catálogo"),
    Plataforma("boosteroid", "Boosteroid", "nube", [], ["xdg-open", "https://cloud.boosteroid.com"], "servicio (suscripción)",
               nota="Tiene app nativa para Linux (.deb desde boosteroid.com) y versión web. Juegos que ya tienes en Steam, Epic, GOG…",
               chile="verificar en boosteroid.com (tiene centros de datos en Sudamérica)", modelo="tus juegos", detecta=["boosteroid"]),
    Plataforma("shadow", "Shadow PC", "nube",
               ["wget -O /tmp/shadow-amd64.deb https://update.shadow.tech/launcher/prod/linux/x86_64/shadow-amd64.deb",
                "sudo apt-get install -y /tmp/shadow-amd64.deb"], ["shadow-prod"], "servicio (arriendo de PC)",
               nota="Un PC Windows completo en la nube: instalas lo que quieras, también juegos que no están en otras nubes. "
                    "Pide decodificación H.264/H.265 por hardware.",
               chile="verificar (centros de datos en Europa y EE.UU.)", modelo="PC completo",
               detecta=["shadow-prod"]),  # nombre del ejecutable según el .deb oficial: confirmar al instalar
    Plataforma("luna", "Amazon Luna", "nube", [], ["xdg-open", "https://luna.amazon.com"], "servicio (Prime / Luna+)",
               nota="Por navegador Chrome o Edge.", chile="no disponible en Chile (nov-2026)", modelo="catálogo"),
    Plataforma("blacknut", "Blacknut", "nube", [], ["xdg-open", "https://www.blacknut.com"], "servicio (suscripción)",
               nota="Catálogo familiar por navegador.", chile="verificar", modelo="catálogo"),
    Plataforma("moonlight", "Moonlight (jugar desde otro PC de la casa)", "casa",
               [_FH, "flatpak install -y flathub com.moonlight_stream.Moonlight"], ["flatpak", "run", "com.moonlight_stream.Moonlight"],
               "GPL-3.0", "moonlight-stream/moonlight-qt", "Recibe el juego que corre en tu torre con Sunshine.", detecta=["com.moonlight_stream.Moonlight"]),
    Plataforma("steamlink", "Steam Remote Play / Steam Link", "casa", [_FH, "flatpak install -y flathub com.valvesoftware.SteamLink"],
               ["flatpak", "run", "com.valvesoftware.SteamLink"], "propietaria (gratis)",
               nota="Juega en otra pantalla lo que corre en tu PC con Steam.", detecta=["com.valvesoftware.SteamLink"]),
    Plataforma("sunshine", "Sunshine (convertir este PC en servidor de juegos)", "casa",
               [_FH, "flatpak install -y flathub dev.lizardbyte.app.Sunshine"], ["flatpak", "run", "dev.lizardbyte.app.Sunshine"],
               "GPL-3.0", "LizardByte/Sunshine", "En la torre potente; el teléfono, la tele o un portátil juegan con Moonlight.",
               necesita_gpu=True, detecta=["dev.lizardbyte.app.Sunshine"]),
    Plataforma("jellyfin", "Jellyfin (tus series y películas)", "medios",
               [_FH, "flatpak install -y flathub org.jellyfin.JellyfinServer com.github.iwalton3.jellyfin-media-player"],
               ["flatpak", "run", "com.github.iwalton3.jellyfin-media-player"], "GPL-2.0", "jellyfin/jellyfin",
               "Tu propio 'Netflix' en casa con tus archivos; se ve en tele, teléfono y PC.", detecta=["org.jellyfin.JellyfinServer"]),
    Plataforma("kodi", "Kodi (centro multimedia)", "medios", ["sudo apt-get install -y kodi"], ["kodi"], "GPL-2.0", "xbmc/xbmc",
               "Pantalla completa para la tele, se maneja con mando o control remoto.", detecta=["kodi"]),
    Plataforma("servicios", "Plataformas oficiales (Netflix, Disney+, Max, YouTube…)", "medios", [], ["xdg-open", "https://www.netflix.com"],
               "servicios con suscripción", nota="Como app web en el navegador; Neo Orión abre la que pidas."),
]}

SERVICIOS = {"netflix": "https://www.netflix.com", "disney": "https://www.disneyplus.com", "max": "https://play.max.com",
             "prime": "https://www.primevideo.com", "youtube": "https://www.youtube.com/tv", "crunchyroll": "https://www.crunchyroll.com",
             "apple": "https://tv.apple.com", "paramount": "https://www.paramountplus.com", "twitch": "https://www.twitch.tv"}

# Herramientas que siempre convienen al jugar en Linux
EXTRAS = {
    "gamemode": ("sudo apt-get install -y gamemode", "BSD-3-Clause", "sube el rendimiento mientras juegas"),
    "mangohud": ("sudo apt-get install -y mangohud", "MIT", "FPS y temperaturas en pantalla"),
    "antimicrox": ("sudo apt-get install -y antimicrox", "GPL-3.0", "usar el mando como teclado/mouse en juegos sin soporte"),
    "chiaki-ng": ("flatpak install -y flathub io.github.streetpea.Chiaki4deck", "AGPL-3.0",
                  "Remote Play de tu PS4/PS5 en el PC (cliente libre)"),
    "xpadneo": ("git clone https://github.com/atar-axis/xpadneo /tmp/xpadneo && sudo /tmp/xpadneo/install.sh", "GPL-3.0",
                "mando Xbox por Bluetooth con vibración y batería"),
}

_MANDOS = [
    (r"xbox|x-box|microsoft.*controller|045e:", "Xbox"), (r"dualsense|dualshock|sony.*controller|wireless controller|054c:", "PlayStation"),
    (r"nintendo|pro controller|joy-con|057e:", "Nintendo"), (r"8bitdo|2dc8:", "8BitDo"), (r"steam controller|28de:", "Steam"),
    (r"gamepad|joystick|controller", "genérico")]


def dispositivos(texto: str | None = None) -> dict:
    """Lee /proc/bus/input/devices y clasifica teclados, mouse y mandos."""
    if texto is None:
        try:
            texto = open("/proc/bus/input/devices").read()
        except OSError:
            texto = ""
    out = {"teclados": [], "mouse": [], "mandos": []}
    for bloque in texto.strip().split("\n\n"):
        nombre = re.search(r'N: Name="([^"]*)"', bloque)
        if not nombre:
            continue
        n = nombre.group(1)
        idm = re.search(r"Vendor=([0-9a-f]{4}) Product=([0-9a-f]{4})", bloque)
        vid = f"{idm.group(1)}:{idm.group(2)}" if idm else ""
        h = re.search(r"H: Handlers=(.*)", bloque)
        hand = h.group(1) if h else ""
        low = f"{n.lower()} {vid}"
        if "js" in hand or re.search(r"gamepad|controller|joystick", low):
            if re.search(r"motion sensors|touchpad", low):
                continue  # sub-dispositivos del mismo mando (giroscopio, panel táctil)
            marca = next(m for p, m in _MANDOS if re.search(p, low))
            out["mandos"].append({"nombre": n, "marca": marca, "id": vid})
        elif "mouse" in hand and not re.search(r"touchpad|trackpad", low) or re.search(r"\bmouse\b", low):
            out["mouse"].append({"nombre": n, "id": vid})
        elif "kbd" in hand and re.search(r"keyboard|teclado", low):
            out["teclados"].append({"nombre": n, "id": vid})
    return out


def _flatpaks() -> set[str]:
    if not shutil.which("flatpak"):
        return set()
    try:
        r = subprocess.run(["flatpak", "list", "--columns=application"], capture_output=True, text=True, timeout=10)
        return set(r.stdout.split())
    except (OSError, subprocess.TimeoutExpired):
        return set()


def instaladas() -> dict[str, bool]:
    fp = _flatpaks()
    return {k: (not p.detecta) or any(shutil.which(d) or d in fp for d in p.detecta) for k, p in PLATAFORMAS.items()}


def recomendar(gpu: str | None = None, vram_gb: float = 0, red_mbps: float | None = None) -> dict:
    """Dónde conviene jugar en este equipo."""
    potente = bool(gpu) and vram_gb >= 6
    if potente:
        prin, razon = ["steam", "heroic"], "Tienes GPU dedicada: los juegos corren aquí mismo con Proton."
    else:
        prin, razon = ["geforcenow", "xboxcloud"], "Sin GPU fuerte: el juego corre en la nube y aquí solo llega el video."
    if red_mbps is not None and red_mbps < 15 and not potente:
        razon += " Ojo: tu red mide menos de 15 Mb/s, la nube se va a ver mal; mejor Moonlight desde otro PC de la casa."
        prin = ["moonlight"] + prin
    return {"principal": prin, "razon": razon, "siempre": ["retroarch", "jellyfin", "kodi"],
            "extras": list(EXTRAS) + (["sunshine"] if potente else [])}


def _plataforma_de(pedido: str) -> str | None:
    t = pedido.lower()
    reglas = [("boosteroid", "boosteroid"), ("shadow", "shadow"), ("luna", "luna"), ("blacknut", "blacknut"),
              ("steam link|remote play", "steamlink"), ("playstation plus|ps plus|psn", "psplus"),
              ("geforce|nvidia|nube", "geforcenow"), ("xbox|game pass", "xboxcloud"), ("epic|gog|amazon games|heroic", "heroic"),
              ("steam|proton", "steam"), ("lutris", "lutris"), ("emulador|retro|nes|snes|playstation 1|ps1|n64|gba", "retroarch"),
              ("moonlight|desde la torre|desde el otro pc", "moonlight"), ("sunshine|servidor de juegos", "sunshine"),
              ("jellyfin|mis series|mis pel", "jellyfin"), ("kodi", "kodi")]
    for patron, clave in reglas:
        if re.search(patron, t):
            return clave
    if any(s in t for s in SERVICIOS):
        return "servicios"
    return None


def nubes(pais: str = "CL") -> list[dict]:
    """Todas las nubes de juego, primero las que funcionan en el país."""
    orden = lambda p: (0 if p.chile.startswith("sí") else 1 if p.chile.startswith("verificar") else 2)
    return [{"clave": p.clave, "nombre": p.nombre, "modelo": p.modelo, "chile": p.chile, "nota": p.nota}
            for p in sorted((p for p in PLATAFORMAS.values() if p.tipo == "nube"), key=orden)]


def plan(pedido: str, perfil: dict | None = None) -> dict:
    """Convierte 'quiero jugar en la nube' / 'pon Netflix' en pasos concretos."""
    perfil = perfil or {}
    clave = _plataforma_de(pedido)
    if clave == "psplus":
        return {"plataforma": "PlayStation Plus (nube)", "clave": "psplus", "tipo": "nube", "pasos": [],
                "aviso": "Sony no tiene app de PlayStation Plus para Linux ni versión web. Alternativas: jugarlo en Shadow PC "
                         "(Windows en la nube) o Remote Play desde tu PS5 con Chiaki-ng.", "mandos": dispositivos()["mandos"]}
    if clave is None:
        rec = recomendar(perfil.get("gpu"), perfil.get("vram_gb", 0), perfil.get("red_mbps"))
        clave = rec["principal"][0] if re.search(r"jug|juego", pedido.lower()) else "jellyfin"
    p = PLATAFORMAS[clave]
    lanzar = list(p.lanzar)
    if clave == "servicios":
        s = next((s for s in SERVICIOS if s in pedido.lower()), "netflix")
        lanzar = ["xdg-open", SERVICIOS[s]]
    ya = instaladas().get(clave, False)
    pasos = ([] if ya else p.instalar) + [" ".join(lanzar)]
    aviso = ""
    if p.necesita_gpu and not perfil.get("gpu"):
        aviso = "Este equipo no tiene GPU dedicada; para juegos pesados usa GeForce NOW."
    return {"plataforma": p.nombre, "clave": clave, "tipo": p.tipo, "instalada": ya, "pasos": pasos,
            "licencia": p.licencia, "nota": p.nota, "aviso": aviso, "mandos": dispositivos()["mandos"]}


def ejecutar(pasos: list[str], simular: bool = True) -> list[dict]:
    """Corre los pasos. Con simular=True solo los devuelve (lo usa el permiso de Neo Orión)."""
    res = []
    for p in pasos:
        if simular:
            res.append({"cmd": p, "simulado": True})
            continue
        r = subprocess.run(p, shell=True, capture_output=True, text=True)
        res.append({"cmd": p, "ok": r.returncode == 0, "salida": (r.stdout + r.stderr)[-400:]})
        if r.returncode != 0:
            break
    return res
