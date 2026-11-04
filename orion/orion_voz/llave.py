"""Llave de voz: tu frase, con tu voz. Como «Oye Siri», pero la frase la eliges tú.

Funciona en dos capas, y las dos tienen que calzar:
  1. QUÉ se dice: la frase que la persona eligió («Neo Orión, soy yo», «Orión, abre el taller», «hola compadre»…).
     Si hay oído (voz a texto), se revisa que lo dicho se parezca a la frase (tolera acentos y muletillas).
  2. QUIÉN lo dice: la huella de la voz (orion_voz.hablantes: sherpa-onnx/ECAPA si está, si no la espectral).
     Se graba la frase 3 veces al configurar; la huella guardada es el promedio y el umbral se calibra con lo
     parecidas que salieron esas 3 (una voz muy pareja exige más parecido).

Para qué sirve:
  · despertar y DESBLOQUEAR el HUD (después de un rato sin uso o al encender);
  · reconocer que quien da una orden de sistema es el dueño;
  · aprobar acciones FÍSICAS o IRREVERSIBLES junto al botón de mantener (dos factores: voz + gesto).

Lo que NO reemplaza: la clave de Linux (sudo, entrar por consola, recuperar el equipo). Una grabación de la voz
podría engañar a cualquier sistema de voz, por eso lo irreversible nunca se aprueba solo con la voz.

Cuántas veces: 1 (rápido) o 3 (más preciso: calibra el umbral). Cómo se desbloquea, a elección de la persona:
solo voz · solo clave · voz o clave (por defecto) · voz y clave (las dos).
Configurar: en el instalador (paso «Tu cuenta») o cuando quieras: «cambia mi frase», «desbloquea solo con clave». La frase se puede cambiar
sin reinstalar; borrarla: «olvida mi voz». La huella se guarda solo en este equipo (~/.orion/llave-voz.json,
permiso 600): es un dato biométrico.
"""
from __future__ import annotations

import datetime as dt
import difflib
import json
import os
import re
import unicodedata
import wave
from pathlib import Path

import numpy as np

ARCHIVO = Path("~/.orion/llave-voz.json").expanduser()
UMBRAL_MIN, UMBRAL_MAX = 0.55, 0.85
UMBRAL_UNA = 0.75          # con una sola grabación no hay cómo calibrar: se exige más parecido
MODOS = {"voz": "solo con tu frase", "clave": "solo con tu clave", "voz_o_clave": "con tu frase o tu clave",
         "voz_y_clave": "con tu frase y además tu clave"}
MULETILLAS = r"\b(eh+|em+|mmm+|este|o sea|oye|ya)\b"


def _norm(t: str) -> str:
    t = unicodedata.normalize("NFD", t.lower())
    t = "".join(c for c in t if unicodedata.category(c) != "Mn")
    t = re.sub(MULETILLAS, " ", t)
    return " ".join(re.sub(r"[^a-z0-9 ]+", " ", t).split())


def frase_calza(dicho: str, frase: str) -> float:
    """0..1: qué tanto lo dicho contiene la frase (tolerante a errores del oído)."""
    a, b = _norm(dicho), _norm(frase)
    if not b:
        return 0.0
    if b in a:
        return 1.0
    pal_b = b.split()
    hits = sum(1 for w in pal_b if any(difflib.SequenceMatcher(None, w, x).ratio() >= 0.8 for x in a.split()))
    return max(difflib.SequenceMatcher(None, a, b).ratio(), hits / len(pal_b))


def leer_wav(ruta: str | Path) -> bytes:
    with wave.open(str(ruta), "rb") as w:
        return w.readframes(w.getnframes())


def _cos(a, b) -> float:
    a, b = np.asarray(a, float), np.asarray(b, float)
    return float(a @ b / ((np.linalg.norm(a) * np.linalg.norm(b)) or 1))


class LlaveVoz:
    def __init__(self, huella=None, stt=None, archivo: Path = ARCHIVO):
        if huella is None:
            from orion_voz.hablantes import crear_huella
            huella = crear_huella("auto")
        self.huella, self.stt, self.archivo = huella, stt, Path(archivo)
        self.datos = self._leer()

    def _leer(self) -> dict:
        try:
            return json.loads(self.archivo.read_text())
        except (OSError, ValueError):
            return {}

    def _guardar(self) -> None:
        self.archivo.parent.mkdir(parents=True, exist_ok=True)
        self.archivo.write_text(json.dumps(self.datos, ensure_ascii=False))
        os.chmod(self.archivo, 0o600)

    @property
    def configurada(self) -> bool:
        return bool(self.datos.get("huella"))

    def estado(self) -> dict:
        return {"configurada": self.configurada, "frase": self.datos.get("frase", ""), "desde": self.datos.get("creada", ""),
                "umbral": self.datos.get("umbral"), "grabaciones": self.datos.get("muestras", 0), **self.modo()}

    def configurar(self, frase: str, muestras: list[bytes], textos: list[str] | None = None) -> dict:
        """frase elegida + 3 grabaciones (PCM 16 kHz mono). textos: lo que el oído entendió en cada una (opcional)."""
        frase = frase.strip()
        if len(_norm(frase).split()) < 2:
            return {"ok": False, "error": "la frase necesita al menos 2 palabras (mejor 3 o 4)"}
        if not muestras:
            return {"ok": False, "error": "graba la frase al menos una vez (mejor 3)"}
        textos = textos or ([self.stt.transcribir(m) for m in muestras] if self.stt else [])
        if textos:
            malas = [i + 1 for i, t in enumerate(textos) if frase_calza(t, frase) < 0.6]
            if malas:
                return {"ok": False, "error": f"en la grabación {', '.join(map(str, malas))} no se entendió la frase; repítela"}
        vecs = [np.asarray(self.huella(m), float) for m in muestras]
        media = np.mean(vecs, axis=0)
        parecidos = [_cos(v, media) for v in vecs]
        # umbral: un poco bajo lo menos parecido de las propias grabaciones, dentro de límites razonables
        if len(vecs) == 1:
            umbral = UMBRAL_UNA
        else:
            umbral = float(np.clip(min(parecidos) - 0.08, UMBRAL_MIN, UMBRAL_MAX))
        modo = self.datos.get("modo", "voz_o_clave")
        self.datos = {"modo": modo, "frase": frase, "huella": media.round(6).tolist(), "umbral": round(umbral, 3),
                      "creada": dt.datetime.now().isoformat(timespec="seconds"), "muestras": len(vecs)}
        self._guardar()
        r = {"ok": True, "frase": frase, "umbral": self.datos["umbral"], "consistencia": round(min(parecidos), 3), "grabaciones": len(vecs)}
        if len(vecs) == 1:
            r["aviso"] = "con una sola grabación te reconoce peor (ruido, resfrío): grabarla 3 veces es más preciso"
        return r

    def modo(self, nuevo: str | None = None) -> dict:
        """Cómo se desbloquea: voz | clave | voz_o_clave | voz_y_clave. Sin frase guardada, la voz no se puede exigir."""
        if nuevo is None:
            return {"modo": self.datos.get("modo", "voz_o_clave" if self.configurada else "clave")}
        if nuevo not in MODOS:
            return {"ok": False, "error": "modo: " + " | ".join(MODOS)}
        if nuevo != "clave" and not self.configurada:
            return {"ok": False, "error": "primero graba tu frase (aunque sea una vez)"}
        self.datos["modo"] = nuevo
        self._guardar()
        return {"ok": True, "modo": nuevo, "explicacion": "se desbloquea " + MODOS[nuevo]}

    def abrir(self, voz_ok: bool, clave_ok: bool) -> bool:
        """¿Con lo que se entregó alcanza para desbloquear según el modo elegido?"""
        m = self.modo()["modo"]
        return {"voz": voz_ok, "clave": clave_ok, "voz_o_clave": voz_ok or clave_ok, "voz_y_clave": voz_ok and clave_ok}[m]

    def cambiar_frase(self, frase: str, muestras: list[bytes], textos: list[str] | None = None) -> dict:
        return self.configurar(frase, muestras, textos)

    def verificar(self, muestra: bytes, texto: str | None = None) -> dict:
        """¿Es la frase y es la voz del dueño?"""
        if not self.configurada:
            return {"ok": False, "error": "no hay llave de voz configurada"}
        if texto is None and self.stt:
            texto = self.stt.transcribir(muestra)
        frase_ok = True if texto is None else frase_calza(texto, self.datos["frase"]) >= 0.7
        parecido = _cos(self.huella(muestra), self.datos["huella"])
        voz_ok = parecido >= self.datos["umbral"]
        return {"ok": frase_ok and voz_ok, "frase_ok": frase_ok, "voz_ok": voz_ok, "parecido": round(parecido, 3),
                "umbral": self.datos["umbral"], "oido": texto}

    def olvidar(self) -> dict:
        self.datos = {"modo": "clave"}
        self._guardar()          # queda solo el modo «clave»: la huella se borra
        return {"ok": True, "mensaje": "huella de voz borrada de este equipo; se desbloquea con tu clave"}


def configurar_desde_archivos(frase: str, wavs: list[str], modo: str = "voz_o_clave", **kw) -> dict:
    """Lo usa el primer arranque con las frases (1 a 3) que se grabaron en el instalador."""
    k = LlaveVoz(**kw)
    r = k.configurar(frase, [leer_wav(w) for w in wavs]) if wavs else {"ok": True}
    k.modo(modo if (wavs or modo == "clave") else "clave")
    return r
