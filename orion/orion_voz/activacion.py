"""Formas de iniciar Neo Orión: varias frases, palmadas y rutinas. «Buenos días, señoritas» 👏👏 y todo despierta.

ACTIVADORES (los que la persona quiera, en ~/.orion/activacion.json):
  · frase      «Buenos días, señoritas», «Jarvis, a trabajar», «modo taller»… (tolerante a errores del oído)
  · palmadas   2 o 3 palmadas seguidas (golpes cortos y fuertes, separados 0,15–0,8 s)
  · frase + palmadas, o la frase solo si la dice el dueño (huella de la llave de voz)
Cada activador dispara una RUTINA: una lista de acciones en orden, por ejemplo
  despertar (pantalla + IA) → saludo («Buenos días, Ana») → luces del taller → resumen del día → abrir proyecto.

NIVELES DE REPOSO (lo que se puede y lo que no):
  · reposo atento (recomendado): pantalla apagada (wlopm), IA descargada de la memoria, brillo al mínimo; el oído
    sigue escuchando con el detector de voz (muy poco consumo). Despierta con frase, palmadas, teclado o teléfono.
  · suspendido (memoria encendida, procesador apagado): el PC NO puede oír. Lo despierta un NODO DESPERTADOR
    (Raspberry Pi o ESP32 con micrófono) que escucha la frase/palmadas y manda Wake-on-LAN; también teclado,
    mouse o el teléfono (botón en el HUD vinculado).
  · apagado: Wake-on-LAN si la placa madre lo permite (opción en el BIOS «Wake on LAN / Power on by PCI-E»).
"""
from __future__ import annotations

import json
import re
import socket
import struct
import subprocess
import time
from dataclasses import dataclass, field
from pathlib import Path

import numpy as np

CONFIG = Path("~/.orion/activacion.json").expanduser()
RATE = 16000
ACCIONES = {
    "despertar": "enciende la pantalla y vuelve a cargar la IA",
    "saludo": "te saluda por tu nombre y la hora del día",
    "luces": "enciende las luces (Home Assistant)",
    "musica": "pone tu música",
    "resumen": "resumen del día: pendientes, proyectos y memoria de ayer",
    "proyecto": "abre el último proyecto",
    "taller": "modo taller: CNC, cámaras y robots a la vista",
    "reposo": "apaga la pantalla y descarga la IA (reposo atento)",
    "suspender": "suspende el equipo (lo despierta el nodo, el teclado o el teléfono)",
}
POR_DEFECTO = {
    "activadores": [
        {"id": "buenos-dias", "tipo": "frase", "frase": "Buenos días, señoritas", "solo_dueno": False,
         "rutina": ["despertar", "saludo", "luces", "resumen"]},
        {"id": "palmadas", "tipo": "palmadas", "veces": 2, "rutina": ["despertar", "saludo"]},
        {"id": "a-dormir", "tipo": "frase", "frase": "Buenas noches", "solo_dueno": True, "rutina": ["reposo"]},
    ],
    "reposo": {"tras_minutos": 20, "nivel": "atento"},
    "saludo": "{saludo}, {nombre}. Todo listo.",
    "palmadas": {"sensibilidad": "normal"},
    "wol": {"mac": "", "interfaz": ""},
}


def leer(ruta: Path = CONFIG) -> dict:
    try:
        return json.loads(ruta.read_text())
    except (OSError, ValueError):
        return json.loads(json.dumps(POR_DEFECTO))


def guardar(c: dict, ruta: Path = CONFIG) -> None:
    ruta.parent.mkdir(parents=True, exist_ok=True)
    ruta.write_text(json.dumps(c, ensure_ascii=False, indent=2))


PASO_LIBRE = re.compile(r"^(di|abre|orden):(.{1,300})$", re.S)     # pasos propios: decir, abrir una página, dar una orden
SENSIBILIDAD = {"sensible": 5.0, "normal": 8.0, "firme": 12.0}


def paso_valido(a: str) -> bool:
    if a in ACCIONES:
        return True
    m = PASO_LIBRE.match(a)
    if not m:
        return False
    if m.group(1) == "abre":
        return bool(re.match(r"^(https?://|[\w-]+(\.[\w-]+)+)", m.group(2).strip()))
    return bool(m.group(2).strip())


def editar(c: dict, ide: str, cambios: dict) -> dict:
    """Cambia una forma de inicio existente: frase, veces, solo_dueno, rutina (en el orden dado), nombre."""
    a = next((x for x in c.get("activadores", []) if x["id"] == ide), None)
    if a is None:
        raise ValueError("no existe esa forma de inicio")
    nuevo = {**a, **{k: v for k, v in cambios.items() if k in ("frase", "veces", "solo_dueno", "rutina", "nombre", "activo")}}
    if nuevo["tipo"] == "frase" and len(str(nuevo.get("frase", "")).split()) < 2:
        raise ValueError("la frase necesita al menos 2 palabras")
    if nuevo["tipo"] == "palmadas" and nuevo.get("veces") not in (2, 3, 4):
        raise ValueError("palmadas: 2, 3 o 4")
    malas = [x for x in nuevo["rutina"] if not paso_valido(x)]
    if malas or not nuevo["rutina"]:
        raise ValueError("pasos inválidos: " + ", ".join(malas) if malas else "la rutina necesita al menos un paso")
    c["activadores"] = [nuevo if x["id"] == ide else x for x in c["activadores"]]
    return c


def agregar(c: dict, tipo: str, rutina: list[str], frase: str = "", veces: int = 2, solo_dueno: bool = False) -> dict:
    malas = [a for a in rutina if not paso_valido(a)]
    if malas:
        raise ValueError(f"acciones desconocidas: {', '.join(malas)}")
    if tipo == "frase" and len(frase.split()) < 2:
        raise ValueError("la frase necesita al menos 2 palabras")
    if tipo == "palmadas" and veces not in (2, 3, 4):
        raise ValueError("palmadas: 2, 3 o 4")
    ide = re.sub(r"[^a-z0-9]+", "-", (frase or f"{veces}-palmadas").lower()).strip("-")[:40]
    c["activadores"] = [a for a in c["activadores"] if a["id"] != ide] + [
        {"id": ide, "tipo": tipo, **({"frase": frase, "solo_dueno": solo_dueno} if tipo == "frase" else {"veces": veces}),
         "rutina": rutina}]
    return c


# ------------------------------------------------------------------ palmadas
def palmadas_de(c: dict) -> "Palmadas":
    return Palmadas(salto=SENSIBILIDAD.get(c.get("palmadas", {}).get("sensibilidad", "normal"), 8.0))


@dataclass
class Palmadas:
    """Detecta golpes cortos y fuertes en el audio (16 kHz mono, int16) y cuenta series de 2–4.

    Una palmada: la energía en 10 ms salta > `salto`× sobre el ruido de fondo, con un piso absoluto, y cae en < 120 ms.
    Una serie: golpes separados entre 0,15 y 0,8 s; la serie se cierra tras 0,9 s de silencio.
    """
    salto: float = 8.0
    piso: float = 0.06            # RMS mínimo (fracción de escala completa)
    min_sep: float = 0.15
    max_sep: float = 0.8
    fondo: float = 0.003
    _t: float = 0.0
    _golpes: list = field(default_factory=list)
    _alto_desde: float | None = None

    def alimentar(self, pcm: bytes) -> int | None:
        """Devuelve el número de palmadas cuando se completa una serie (si no, None)."""
        x = np.frombuffer(pcm, dtype="<i2").astype(np.float32) / 32768
        n = int(RATE * 0.01)
        hecho = None
        for i in range(0, len(x) - n + 1, n):
            rms = float(np.sqrt(np.mean(x[i:i + n] ** 2)) + 1e-9)
            alto = rms > max(self.piso, self.fondo * self.salto)
            if alto and self._alto_desde is None:
                self._alto_desde = self._t
            elif not alto and self._alto_desde is not None:
                dur = self._t - self._alto_desde
                if dur <= 0.12 and (not self._golpes or self._t - self._golpes[-1] >= self.min_sep):
                    if self._golpes and self._t - self._golpes[-1] > self.max_sep:
                        self._golpes = []
                    self._golpes.append(self._t)
                self._alto_desde = None
            if not alto:
                self.fondo = 0.995 * self.fondo + 0.005 * rms
            if self._golpes and self._t - self._golpes[-1] > 0.9:
                if len(self._golpes) >= 2:
                    hecho = len(self._golpes)
                self._golpes = []
            self._t += 0.01
        return hecho


# ------------------------------------------------------------------ decidir qué activador calza
def elegir(c: dict, texto: str | None = None, palmadas: int | None = None, es_dueno: bool | None = None) -> dict | None:
    from orion_voz.llave import frase_calza
    for a in c.get("activadores", []):
        if a.get("activo") is False:
            continue
        if a["tipo"] == "palmadas" and palmadas and palmadas == a.get("veces", 2):
            return a
        if a["tipo"] == "frase" and texto and frase_calza(texto, a["frase"]) >= 0.75:
            if a.get("solo_dueno") and es_dueno is False:
                continue
            return a
    return None


# ------------------------------------------------------------------ acciones
def saludo(nombre: str = "", ahora: time.struct_time | None = None, plantilla: str | None = None) -> str:
    h = (ahora or time.localtime()).tm_hour
    parte = "Buenos días" if 5 <= h < 12 else "Buenas tardes" if h < 20 else "Buenas noches"
    if plantilla:                                  # editable: «{saludo}, jefe. ¿Qué construimos hoy?»
        t = plantilla.replace("{saludo}", parte).replace("{nombre}", nombre or "")
        return re.sub(r"\s*,\s*\.", ".", re.sub(r",\s*,", ",", t)).strip()
    return f"{parte}{', ' + nombre if nombre else ''}. Todo listo."


def pantalla(encender: bool, ejecutar=subprocess.run) -> bool:
    r = ejecutar(["wlopm", "--on" if encender else "--off", "*"], capture_output=True)
    return getattr(r, "returncode", 1) == 0


def descargar_ia() -> None:
    """Saca los modelos de la memoria (Ollama keep_alive=0); vuelven solos con la próxima pregunta."""
    import urllib.request
    try:
        from orion_core.servidor import modelos_locales, OLLAMA
        for m in modelos_locales():
            req = urllib.request.Request(OLLAMA + "/api/generate", data=json.dumps({"model": m, "keep_alive": 0}).encode(),
                                         headers={"Content-Type": "application/json"})
            urllib.request.urlopen(req, timeout=10).read()
    except Exception:
        pass


def ejecutar_rutina(rutina: list[str], hacer=None, nombre: str = "") -> list[dict]:
    """Corre las acciones en orden. `hacer(accion)` permite conectarlas al HUD/servidor (y probarlas)."""
    hechos = []
    for a in rutina:
        try:
            if hacer:
                r = hacer(a)
            elif a == "despertar":
                r = pantalla(True)
            elif a == "reposo":
                pantalla(False)
                descargar_ia()
                r = True
            elif a == "suspender":
                r = subprocess.run(["systemctl", "suspend"], capture_output=True).returncode == 0
            elif a == "saludo":
                r = saludo(nombre)
            elif a.startswith("di:"):
                r = a[3:].strip()
            elif a.startswith(("abre:", "orden:")):
                r = "pendiente en el HUD"
            elif a == "luces":
                from orion_casa import Casa
                r = Casa().ordenar("enciende las luces")
            else:
                r = "pendiente en el HUD"          # musica, resumen, proyecto, taller: los muestra la interfaz
            hechos.append({"accion": a, "ok": r not in (False, None), "resultado": r})
        except Exception as e:
            hechos.append({"accion": a, "ok": False, "error": str(e)[:160]})
    return hechos


# ------------------------------------------------------------------ Wake-on-LAN
def paquete_magico(mac: str) -> bytes:
    h = re.sub(r"[^0-9a-fA-F]", "", mac)
    if len(h) != 12:
        raise ValueError("MAC inválida")
    return b"\xff" * 6 + bytes.fromhex(h) * 16


def despertar_equipo(mac: str, broadcast: str = "255.255.255.255", puerto: int = 9) -> bool:
    """Lo usa el NODO DESPERTADOR (o el teléfono) para encender/despertar el PC central por la red."""
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
    s.sendto(paquete_magico(mac), (broadcast, puerto))
    s.close()
    return True


def preparar_wol(ejecutar=subprocess.run) -> dict:
    """Deja la tarjeta de red cableada lista para Wake-on-LAN (persistente con un servicio) y devuelve su MAC."""
    for d in sorted(Path("/sys/class/net").iterdir()) if Path("/sys/class/net").exists() else []:
        if d.name == "lo" or (d / "wireless").exists() or not (d / "device").exists():
            continue
        mac = (d / "address").read_text().strip()
        r = ejecutar(["sudo", "ethtool", "-s", d.name, "wol", "g"], capture_output=True, text=True)
        return {"ok": getattr(r, "returncode", 1) == 0, "interfaz": d.name, "mac": mac,
                "bios": "activa «Wake on LAN» o «Power on by PCI-E» en el BIOS para encender desde apagado"}
    return {"ok": False, "error": "no hay tarjeta de red cableada (Wake-on-LAN no funciona por Wi-Fi en la mayoría de los PC)"}


NODO_DESPERTADOR = '''#!/usr/bin/env python3
"""Nodo despertador (Raspberry Pi con micrófono USB): escucha palmadas o la frase y despierta al PC central.
Instalar: sudo apt install python3-numpy alsa-utils && pip install vosk ; copiar orion_voz/activacion.py al lado."""
import subprocess, sys
from activacion import Palmadas, despertar_equipo
MAC = sys.argv[1]                      # MAC del PC central (la muestra «prepara el encendido por red» en el HUD)
p = Palmadas()
arec = subprocess.Popen(["arecord", "-q", "-f", "S16_LE", "-r", "16000", "-c", "1", "-t", "raw"], stdout=subprocess.PIPE)
while True:
    n = p.alimentar(arec.stdout.read(3200))
    if n and n >= 2:
        despertar_equipo(MAC)
        print("palmadas:", n, "→ despertando", MAC, flush=True)
'''
