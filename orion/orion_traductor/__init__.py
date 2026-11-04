"""Traductor en vivo: dos personas que no hablan el mismo idioma conversan a través de Neo Orión.

Cadena: oído (voz → texto, con detección de idioma) → traducción → voz en el otro idioma + subtítulos en el HUD.
Motores de traducción, en orden:
  1. Argos Translate (MIT, sin internet, modelos por par de idiomas ~100 MB)
  2. la IA local (Ollama) con un pedido de traducción estricto
  3. cualquier función que se le pase (p. ej. una IA en la nube elegida por la persona)

Modo conversación: A habla español → sale en inglés por los parlantes; B responde en inglés → sale en español.
Salida por audífonos de conducción ósea o Bluetooth: se elige la salida de PipeWire con `salida=` (encaja con
el concepto del traductor por conducción ósea del dueño).
"""
from __future__ import annotations

import re
import subprocess
from dataclasses import dataclass, field

NOMBRES = {"es": "español", "en": "inglés", "pt": "portugués", "fr": "francés", "de": "alemán", "it": "italiano",
           "zh": "chino", "ja": "japonés", "ko": "coreano", "ru": "ruso", "ar": "árabe", "hi": "hindi"}
VOCES_ESPEAK = {"es": "es-419", "en": "en-us", "pt": "pt-br", "fr": "fr-fr", "de": "de", "it": "it", "zh": "cmn",
                "ja": "ja", "ko": "ko", "ru": "ru", "ar": "ar", "hi": "hi"}


def detectar_idioma(texto: str, opciones: tuple[str, str] = ("es", "en")) -> str:
    """Detección liviana entre los dos idiomas de la conversación (Whisper ya entrega el idioma cuando se usa)."""
    t = texto.lower()
    pistas = {
        "es": r"\b(el|la|los|las|que|de|y|en|un|una|es|por|para|con|no|qué|cómo|dónde|hola|gracias|ñ)\b|[ñáéíóú¿¡]",
        "en": r"\b(the|is|are|you|what|how|where|and|to|of|in|it|that|hello|thanks|please|i'm|don't)\b",
        "pt": r"\b(não|você|obrigad[oa]|olá|é|são|com|uma|isso|onde)\b|[ãõç]",
        "fr": r"\b(le|les|est|vous|je|nous|bonjour|merci|où|c'est|pas)\b|[èêàç]",
        "de": r"\b(der|die|das|ist|und|ich|nicht|danke|hallo|wo|sie)\b|[äöüß]",
    }
    puntos = {k: len(re.findall(pistas.get(k, r"$^"), t)) for k in opciones}
    return max(opciones, key=lambda k: (puntos[k], k == opciones[0]))


def argos(texto: str, de: str, a: str) -> str | None:
    try:
        import argostranslate.translate as at
    except ImportError:
        return None
    try:
        return at.translate(texto, de, a)
    except Exception:
        return None


def ia_local(texto: str, de: str, a: str, charlar=None) -> str | None:
    if charlar is None:
        try:
            from orion_core.servidor import charlar
        except Exception:
            return None
    r = charlar(f"Traduce del {NOMBRES.get(de, de)} al {NOMBRES.get(a, a)}. Responde SOLO la traducción, sin comillas "
                f"ni explicaciones:\n{texto}")
    if r.get("sin_ia") or r.get("error"):
        return None
    return r.get("texto", "").strip().strip('"«»') or None


@dataclass
class Conversacion:
    idioma_a: str = "es"
    idioma_b: str = "en"
    motores: list = field(default_factory=lambda: [argos, ia_local])
    salida: str = ""                   # nombre del sink de PipeWire (audífonos óseos, parlante)
    hablar: object = None              # función(texto, idioma) → None; por defecto espeak-ng / Piper
    historial: list = field(default_factory=list)

    def traducir(self, texto: str, de: str, a: str) -> tuple[str, str]:
        for m in self.motores:
            r = m(texto, de, a)
            if r:
                return r, getattr(m, "__name__", "motor")
        return "", "ninguno"

    def turno(self, texto: str, idioma: str | None = None) -> dict:
        """Una frase de cualquiera de los dos: se traduce al idioma del otro y se dice."""
        de = idioma or detectar_idioma(texto, (self.idioma_a, self.idioma_b))
        a = self.idioma_b if de == self.idioma_a else self.idioma_a
        tr, motor = self.traducir(texto, de, a)
        ev = {"original": texto, "de": de, "a": a, "traduccion": tr, "motor": motor}
        if not tr:
            ev["error"] = "no hay motor de traducción: instala Argos Translate o una IA local"
        elif self.hablar:
            self.hablar(tr, a)
        else:
            decir(tr, a, self.salida)
        self.historial.append(ev)
        return ev


def decir(texto: str, idioma: str, salida: str = "") -> None:
    env = None
    if salida:
        import os
        env = {**os.environ, "PULSE_SINK": salida}
    try:
        subprocess.run(["espeak-ng", "-v", VOCES_ESPEAK.get(idioma, idioma), texto], env=env, timeout=60,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    except (OSError, subprocess.SubprocessError):
        pass


def salidas_audio() -> list[str]:
    """Salidas de PipeWire/Pulse disponibles (para elegir los audífonos)."""
    try:
        out = subprocess.run(["pactl", "list", "short", "sinks"], capture_output=True, text=True, timeout=5).stdout
        return [l.split("\t")[1] for l in out.splitlines() if "\t" in l]
    except (OSError, subprocess.SubprocessError):
        return []


def instalar_par(de: str, a: str) -> list[str]:
    return ["/opt/orion/venv/bin/pip install argostranslate",
            f"/opt/orion/venv/bin/python3 -c \"import argostranslate.package as p; p.update_package_index(); "
            f"[p.install_from_path(x.download()) for x in p.get_available_packages() if (x.from_code, x.to_code) in "
            f"[('{de}','{a}'), ('{a}','{de}')]]\""]
