"""¿Quién está hablando? — separación por voz y una línea de registro por persona.

Cada frase se convierte en una "huella de voz" (vector) y se compara con las
conocidas. Si se parece a alguien → es esa persona; si no → "Persona N" nueva.
Los nombres se aprenden por contexto:
  · "soy Camila" / "me llamo Jorge"      → esa voz es Camila/Jorge (seguro)
  · "Jorge, pásame el taladro" + responde una voz nueva → probablemente Jorge
    (queda como sugerencia hasta que se repita 2 veces o lo confirmes)

Huellas de voz (de más a menos precisa):
  · sherpa-onnx + 3D-Speaker/ECAPA (Apache-2.0, corre hasta en Raspberry)
  · SpeechBrain ECAPA-TDNN (Apache-2.0)
  · pyannote (MIT; modelos con registro en Hugging Face) — además separa voces superpuestas
  · espectral (propio, sin dependencias): MFCC + tono. Sirve para pocas personas y buen audio.

PRIVACIDAD — la huella de voz es un dato biométrico (dato sensible en la ley chilena
de protección de datos). Por eso:
  · modo "anonimo" (por defecto para terceros): se separan las líneas como Persona 2, 3…
    sin guardar huella permanente ni nombre
  · modo "con_nombres": solo para quienes aceptaron ("Orión, Camila acepta")
  · "Orión, olvida la voz de X" borra su huella (sus líneas de texto quedan o se borran a elección)
"""
from __future__ import annotations
import json, math, os, re, time, unicodedata
from dataclasses import dataclass, field
from pathlib import Path

import numpy as np

RATE = 16000


# ================================================================ huellas de voz
class HuellaEspectral:
    """Sin modelos: estadísticos de MFCC (timbre) + tono fundamental."""
    nombre = "espectral"

    def __init__(self, n_mfcc: int = 20, n_mel: int = 40):
        self.n_mfcc, self.n_mel = n_mfcc, n_mel
        nfft = 512
        mel = lambda f: 2595 * np.log10(1 + f / 700)
        imel = lambda m: 700 * (10 ** (m / 2595) - 1)
        pts = imel(np.linspace(mel(60), mel(7600), n_mel + 2))
        bins = np.floor((nfft + 1) * pts / RATE).astype(int)
        fb = np.zeros((n_mel, nfft // 2 + 1))
        for i in range(n_mel):
            a, b, c = bins[i], bins[i + 1], bins[i + 2]
            fb[i, a:b] = (np.arange(a, b) - a) / max(b - a, 1)
            fb[i, b:c] = (c - np.arange(b, c)) / max(c - b, 1)
        self.fb, self.nfft = fb, nfft
        n = np.arange(n_mel)
        self.dct = np.cos(np.pi / n_mel * (n[None, :] + .5) * np.arange(n_mfcc)[:, None])

    def __call__(self, pcm: bytes) -> np.ndarray:
        x = np.frombuffer(pcm, np.int16).astype(np.float32) / 32768
        x = np.append(x[0], x[1:] - .97 * x[:-1])
        hop, win = 160, 400
        n = 1 + max(0, (len(x) - win) // hop)
        frames = np.stack([x[i * hop:i * hop + win] for i in range(n)]) * np.hamming(win)
        energia = (frames ** 2).mean(1)
        frames = frames[energia > np.percentile(energia, 40)]            # solo tramos con voz
        spec = np.abs(np.fft.rfft(frames, self.nfft)) ** 2
        mfcc = (self.dct @ np.log(spec @ self.fb.T + 1e-9).T).T[:, 1:]  # sin c0 (volumen)
        mfcc -= mfcc.mean(0, keepdims=True) * 0                          # se deja la media: es el timbre
        # tono fundamental por autocorrelación (70–400 Hz)
        f0s = []
        for fr in frames[:: max(1, len(frames) // 60)]:
            ac = np.correlate(fr, fr, "full")[win - 1:]
            lo, hi = RATE // 400, RATE // 70
            k = lo + int(np.argmax(ac[lo:hi]))
            if ac[k] > .3 * ac[0]:
                f0s.append(RATE / k)
        f0 = np.median(f0s) if f0s else 0
        v = np.concatenate([mfcc.mean(0), mfcc.std(0), [np.log(f0 + 1) * 6]])
        return v / (np.linalg.norm(v) + 1e-9)


class HuellaSherpa:
    nombre = "sherpa-onnx"

    def __init__(self, modelo=os.path.expanduser("~/.orion/modelos/3dspeaker_eres2net_base.onnx")):
        import sherpa_onnx
        self.ex = sherpa_onnx.SpeakerEmbeddingExtractor(sherpa_onnx.SpeakerEmbeddingExtractorConfig(model=modelo))

    def __call__(self, pcm):
        s = self.ex.create_stream()
        s.accept_waveform(RATE, np.frombuffer(pcm, np.int16).astype(np.float32) / 32768)
        s.input_finished()
        v = np.array(self.ex.compute(s))
        return v / np.linalg.norm(v)


class HuellaSpeechBrain:
    nombre = "speechbrain-ecapa"

    def __init__(self):
        from speechbrain.inference.speaker import EncoderClassifier
        self.m = EncoderClassifier.from_hparams("speechbrain/spkrec-ecapa-voxceleb")

    def __call__(self, pcm):
        import torch
        v = self.m.encode_batch(torch.tensor(np.frombuffer(pcm, np.int16).astype(np.float32) / 32768)[None]).squeeze().numpy()
        return v / np.linalg.norm(v)


def crear_huella(nombre: str = "auto"):
    for cls in ([HuellaSherpa, HuellaSpeechBrain] if nombre == "auto" else []):
        try:
            return cls()
        except Exception:
            continue
    return {"sherpa-onnx": HuellaSherpa, "speechbrain-ecapa": HuellaSpeechBrain}.get(nombre, HuellaEspectral)()


# ================================================================ registro de personas
def _norm(t):
    return "".join(c for c in unicodedata.normalize("NFD", t.lower()) if unicodedata.category(c) != "Mn")


@dataclass
class Persona:
    id: str
    nombre: str = ""
    centro: list = field(default_factory=list)
    frases: int = 0
    acepto: bool = False          # consintió quedar con nombre y huella permanente
    sugerencias: dict = field(default_factory=dict)   # nombre → veces

    @property
    def etiqueta(self):
        return self.nombre or self.id


class Hablantes:
    """Asigna cada frase a una persona y aprende nombres por contexto."""
    UMBRAL = {"espectral": .25, "sherpa-onnx": .55, "speechbrain-ecapa": .45}
    CENTRAR = {"espectral"}       # huellas sin modelo: se comparan contra el promedio de todo lo escuchado

    def __init__(self, huella=None, ruta: str | None = None, dueno: str = "", modo: str = "anonimo"):
        self.h = huella or crear_huella()
        self.umbral = self.UMBRAL.get(self.h.nombre, .6)
        self.ruta = Path(ruta or os.path.expanduser("~/.orion/voces/perfiles.json"))
        self.modo = modo
        self.personas: list[Persona] = []
        if self.ruta.exists():
            self.personas = [Persona(**p) for p in json.loads(self.ruta.read_text())]
        self.dueno = dueno
        self._ultimo: Persona | None = None
        self._ultimo_v = None
        self._hist: list = []                  # huellas recientes (para centrar)
        self._sesion: list = []                # [huella, persona] de la conversación en curso
        self._vocativo: tuple[str, float] | None = None     # (nombre, instante) dicho al inicio de una frase

    # ---- registrar al dueño (o a quien acepte) con unas frases
    def enrolar(self, nombre: str, muestras: list[bytes], acepto: bool = True) -> Persona:
        vs = np.stack([self.h(m) for m in muestras])
        self._hist.extend(vs)
        c = vs.mean(0); c /= np.linalg.norm(c)
        p = Persona(f"P{len(self.personas) + 1}", nombre, c.tolist(), len(muestras), acepto)
        self.personas.append(p); self._guardar()
        return p

    def _cmp(self, a, b) -> float:
        if self.h.nombre in self.CENTRAR and len(self._hist) > 2:
            mu = np.mean(self._hist[-300:], axis=0)
            a, b = a - mu, b - mu
        return float(np.dot(a, b) / (np.linalg.norm(a) * np.linalg.norm(b) + 1e-9))

    def identificar(self, pcm: bytes) -> tuple[Persona, float]:
        v = self.h(pcm)
        self._hist.append(v); self._ultimo_v = v
        mejor, sim = None, -1.0
        for p in self.personas:
            s = self._cmp(v, np.array(p.centro))
            if s > sim:
                mejor, sim = p, s
        if mejor is None or sim < self.umbral:
            mejor = Persona(f"Persona {len(self.personas) + 1}", centro=v.tolist())
            self.personas.append(mejor); sim = 1.0
        else:                                   # el perfil se afina con cada frase
            c = np.array(mejor.centro) * .8 + v * .2
            mejor.centro = (c / np.linalg.norm(c)).tolist()
        mejor.frases += 1
        self._sesion.append([v, mejor])
        return mejor, sim

    def reagrupar(self, iteraciones: int = 4) -> list[str]:
        """Segunda pasada al cerrar la conversación: con todas las voces ya escuchadas, el
        promedio de referencia es representativo y se corrigen las asignaciones provisorias."""
        if not self._sesion:
            return []
        V = np.stack([x[0] for x in self._sesion])
        mu = np.mean(self._hist[-300:], axis=0) if self.h.nombre in self.CENTRAR else 0
        U = V - mu; U /= np.linalg.norm(U, axis=1, keepdims=True)
        gente = [p for p in self.personas if any(x[1] is p for x in self._sesion) or p.acepto]
        asign = [gente.index(x[1]) if x[1] in gente else 0 for x in self._sesion]
        for _ in range(iteraciones):
            C = []
            for k, p in enumerate(gente):
                miembros = U[[i for i, a in enumerate(asign) if a == k]]
                base = (np.array(p.centro) - mu) if p.acepto else None
                if base is not None:
                    base = base / np.linalg.norm(base)
                c = miembros.sum(0) + (base * 2 if base is not None else 0) if len(miembros) else (base if base is not None else np.zeros(U.shape[1]))
                C.append(c / (np.linalg.norm(c) + 1e-9))
            asign = list(np.argmax(U @ np.stack(C).T, axis=1))
        # fusiona grupos sin nombre que en realidad son una persona ya conocida
        C = np.stack(C)
        for k, p in enumerate(gente):
            if p.nombre:
                continue
            sims = [(float(C[k] @ C[j]), j) for j, q in enumerate(gente) if q.nombre and j != k]
            if sims and max(sims)[0] > .35:
                destino = max(sims)[1]
                asign = [destino if a == k else a for a in asign]
        for x, a in zip(self._sesion, asign):
            x[1] = gente[a]
        vacias = [p for p in self.personas if not p.acepto and not any(x[1] is p for x in self._sesion)]
        for p in vacias:
            self.personas.remove(p)
        return [x[1].etiqueta for x in self._sesion]

    def nueva_conversacion(self):
        self._sesion = []

    # ---- nombres por contexto
    def aprender(self, p: Persona, texto: str, conocidos: list[str] | None = None) -> Persona:
        t = texto.strip()
        m = re.search(r"\b(?:soy|me llamo|mi nombre es|habla)\s+([A-ZÁÉÍÓÚÑ][a-záéíóúñ]+)", t)
        if m and p.nombre and p.nombre != m.group(1):
            # una voz ya conocida "se presenta" con otro nombre → la identificación falló: perfil nuevo
            p.frases -= 1
            p = Persona(f"Persona {len(self.personas) + 1}", m.group(1), self._ultimo_v.tolist(), 1)
            self.personas.append(p)
        elif m:
            p = self._nombrar(p, m.group(1), seguro=True)
        elif self._vocativo and p is not self._ultimo and time.time() - self._vocativo[1] < 30 and not p.nombre:
            nom = self._vocativo[0]
            p.sugerencias[nom] = p.sugerencias.get(nom, 0) + 1
            if p.sugerencias[nom] >= 2:
                p = self._nombrar(p, nom, seguro=False)
        v = re.match(r"^\s*(?:oye\s+|hola\s+)?([A-ZÁÉÍÓÚÑ][a-záéíóúñ]+)\s*[,!]", t)
        if v and _norm(v.group(1)) not in ("orion", "neo", "hola", "oye", "bueno", "ya", "si", "no", "mira"):
            if p.nombre != v.group(1):
                self._vocativo = (v.group(1), time.time())
        if self._sesion:
            self._sesion[-1][1] = p
        self._ultimo = p
        return p

    def _nombrar(self, p: Persona, nombre: str, seguro: bool):
        otro = next((q for q in self.personas if q is not p and q.nombre == nombre), None)
        if otro:                                     # misma persona con dos perfiles → se fusionan
            c = np.array(otro.centro) * otro.frases + np.array(p.centro) * p.frases
            otro.centro = (c / np.linalg.norm(c)).tolist(); otro.frases += p.frases
            self.personas.remove(p)
            for seg in self._sesion:
                if seg[1] is p:
                    seg[1] = otro
            return otro
        p.nombre = nombre
        p.sugerencias = {} if seguro else p.sugerencias
        return p

    def olvidar(self, nombre: str) -> bool:
        antes = len(self.personas)
        self.personas = [p for p in self.personas if _norm(p.etiqueta) != _norm(nombre)]
        self._guardar()
        return len(self.personas) < antes

    def _guardar(self):
        self.ruta.parent.mkdir(parents=True, exist_ok=True)
        guardables = [p for p in self.personas if p.acepto or self.modo == "con_nombres"]
        self.ruta.write_text(json.dumps([p.__dict__ for p in guardables], ensure_ascii=False))

    def cerrar_sesion(self):
        """Fin del día en modo anónimo: se descartan las huellas de quien no aceptó."""
        self._guardar()
