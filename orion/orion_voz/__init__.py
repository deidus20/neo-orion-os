"""Oído y voz de Neo Orión — siempre escuchando, sin botón.

    micrófono ─► VAD (¿alguien habla?) ─► segmento ─► STT ─► texto
                                                          ├─► memoria: diario del día (TODO lo escuchado)
                                                          └─► si te hablan a ti ("Orión, …") ─► IA ─► TTS

Privacidad, por diseño:
  · todo se procesa en el equipo; el audio no se guarda, solo el texto
  · "Orión, pausa" deja de registrar (solo queda atento a "Orión, sigue")
  · la luz/indicador del HUD muestra siempre si está registrando
Cada pieza tiene alternativas (ver orion_core/perfil.py) según lo que aguante el equipo.
"""
from __future__ import annotations
import array, io, json, math, os, re, shutil, subprocess, time, unicodedata, urllib.request, wave
from dataclasses import dataclass, field
from typing import Callable, Iterator

RATE, FRAME_MS = 16000, 30
FRAME_BYTES = RATE * FRAME_MS // 1000 * 2


def _norm(t: str) -> str:
    return "".join(c for c in unicodedata.normalize("NFD", t.lower()) if unicodedata.category(c) != "Mn")


# ================================================================ fuentes de audio
def fuente_arecord(dispositivo: str = "default") -> Iterator[bytes]:
    """Micrófono local vía ALSA (funciona en PC, Raspberry, Jetson)."""
    p = subprocess.Popen(["arecord", "-q", "-D", dispositivo, "-f", "S16_LE", "-r", str(RATE), "-c", "1", "-t", "raw"],
                         stdout=subprocess.PIPE)
    while True:
        b = p.stdout.read(FRAME_BYTES)
        if len(b) < FRAME_BYTES:
            return
        yield b


def fuente_wav(ruta: str) -> Iterator[bytes]:
    """Archivo WAV (pruebas, o audio que llega del teléfono/lentes)."""
    with wave.open(ruta) as w:
        assert w.getframerate() == RATE and w.getnchannels() == 1 and w.getsampwidth() == 2, "WAV 16 kHz mono 16 bit"
        while True:
            b = w.readframes(FRAME_BYTES // 2)
            if len(b) < FRAME_BYTES:
                return
            yield b


# ================================================================ VAD
class VADEnergia:
    """Sin dependencias: umbral adaptativo de energía (sirve hasta en una Pi Zero)."""
    def __init__(self, factor: float = 3.0):
        self.ruido, self.factor = 200.0, factor

    def es_voz(self, frame: bytes) -> bool:
        a = array.array("h", frame)
        rms = math.sqrt(sum(x * x for x in a) / len(a)) if a else 0
        voz = rms > self.ruido * self.factor
        if not voz:
            self.ruido = 0.95 * self.ruido + 0.05 * max(rms, 50)
        return voz


class VADWebRTC:
    def __init__(self, agresividad: int = 2):
        import webrtcvad
        self.v = webrtcvad.Vad(agresividad)

    def es_voz(self, frame: bytes) -> bool:
        return self.v.is_speech(frame, RATE)


class VADCombinado:
    """WebRTC + energía: los dos deben estar de acuerdo. Evita que ventiladores,
    motores o el zumbido de un taller se registren como voz."""
    def __init__(self):
        self.w, self.e = VADWebRTC(3), VADEnergia(2.5)

    def es_voz(self, frame: bytes) -> bool:
        e = self.e.es_voz(frame)
        return e and self.w.es_voz(frame)


def crear_vad(nombre: str = "auto"):
    if nombre in ("auto", "webrtcvad", "combinado"):
        try:
            return VADCombinado() if nombre != "webrtcvad" else VADWebRTC()
        except Exception:
            pass
    return VADEnergia()


def segmentos(frames: Iterator[bytes], vad, silencio_ms: int = 700, min_ms: int = 300, max_s: float = 20) -> Iterator[bytes]:
    """Agrupa frames en frases: empieza con 3 frames de voz, termina tras `silencio_ms` de silencio."""
    buf, previos, activos, silencio = bytearray(), [], 0, 0
    fin = silencio_ms // FRAME_MS
    for f in frames:
        voz = vad.es_voz(f)
        if not buf:
            previos = (previos + [f])[-10:]            # 300 ms antes, para no cortar la primera sílaba
            activos = activos + 1 if voz else 0
            if activos >= 3:
                buf.extend(b"".join(previos)); silencio = 0
            continue
        buf.extend(f)
        silencio = 0 if voz else silencio + 1
        if silencio >= fin or len(buf) >= max_s * RATE * 2:
            if len(buf) >= min_ms * RATE * 2 // 1000:
                yield bytes(buf)
            buf, previos, activos = bytearray(), [], 0
    if len(buf) >= min_ms * RATE * 2 // 1000:
        yield bytes(buf)


# ================================================================ STT (voz → texto)
class STT:
    nombre = "base"

    def transcribir(self, pcm: bytes) -> str:
        raise NotImplementedError


def _wav_bytes(pcm: bytes) -> bytes:
    bio = io.BytesIO()
    with wave.open(bio, "wb") as w:
        w.setnchannels(1); w.setsampwidth(2); w.setframerate(RATE); w.writeframes(pcm)
    return bio.getvalue()


class STTFasterWhisper(STT):
    def __init__(self, modelo="small", dispositivo="auto"):
        from faster_whisper import WhisperModel
        self.nombre = f"faster-whisper-{modelo}"
        self.m = WhisperModel(modelo, device=dispositivo, compute_type="int8")

    def transcribir(self, pcm):
        import numpy as np
        audio = np.frombuffer(pcm, np.int16).astype(np.float32) / 32768
        segs, _ = self.m.transcribe(audio, language="es", vad_filter=False, beam_size=1)
        return " ".join(s.text.strip() for s in segs)


class STTVosk(STT):
    def __init__(self, ruta_modelo=os.path.expanduser("~/.orion/modelos/vosk-small-es")):
        from vosk import Model, KaldiRecognizer
        self.nombre = "vosk"
        self.rec = KaldiRecognizer(Model(ruta_modelo), RATE)

    def transcribir(self, pcm):
        self.rec.AcceptWaveform(pcm)
        return json.loads(self.rec.FinalResult()).get("text", "")


class STTWhisperCpp(STT):
    def __init__(self, modelo=os.path.expanduser("~/.orion/modelos/ggml-base.bin"), binario="whisper-cli"):
        self.nombre, self.modelo, self.bin = "whisper.cpp", modelo, shutil.which(binario) or binario

    def transcribir(self, pcm):
        tmp = f"/tmp/orion-{os.getpid()}.wav"
        open(tmp, "wb").write(_wav_bytes(pcm))
        out = subprocess.run([self.bin, "-m", self.modelo, "-l", "es", "-nt", "-f", tmp], capture_output=True, text=True)
        return out.stdout.strip()


class STTMoonshine(STT):
    def __init__(self, modelo="tiny"):
        import moonshine_voice as mv          # paquete Python de moonshine-ai
        self.nombre, self.t = f"moonshine-{modelo}", mv.Transcriber(model=modelo, language="es")

    def transcribir(self, pcm):
        import numpy as np
        return self.t.transcribe(np.frombuffer(pcm, np.int16).astype(np.float32) / 32768)


def crear_stt(eleccion: str) -> STT:
    tabla = {"faster-whisper": lambda m: STTFasterWhisper(m or "small"), "vosk": lambda m: STTVosk(),
             "whisper.cpp": lambda m: STTWhisperCpp(), "moonshine": lambda m: STTMoonshine(m or "tiny")}
    for clave, fab in tabla.items():
        if eleccion.startswith(clave):
            modelo = eleccion[len(clave):].lstrip("-") or None
            return fab(modelo)
    raise ValueError(f"STT desconocido: {eleccion}")


# ================================================================ TTS (texto → voz)
def hablar(texto: str, motor: str = "auto", voz: str = "", reproducir: bool = True) -> bytes | None:
    """Devuelve WAV (o MP3 en ElevenLabs) y lo reproduce. Alternativas: piper, kokoro, espeak-ng, elevenlabs."""
    if motor == "auto":
        motor = "elevenlabs" if os.environ.get("ELEVENLABS_API_KEY") else ("piper" if shutil.which("piper") else "espeak-ng")
    audio = None
    if motor == "piper":
        modelo = voz or os.path.expanduser("~/.orion/voces/es_MX-claude-high.onnx")
        audio = subprocess.run(["piper", "--model", modelo, "--output_file", "-"], input=texto.encode(), capture_output=True).stdout
    elif motor == "espeak-ng":
        audio = subprocess.run(["espeak-ng", "-v", voz or "es-419", "-s", "165", "--stdout", texto], capture_output=True).stdout
    elif motor == "elevenlabs":
        vid = voz or os.environ.get("ELEVENLABS_VOICE_ID", "")
        req = urllib.request.Request(f"https://api.elevenlabs.io/v1/text-to-speech/{vid}",
                                     data=json.dumps({"text": texto, "model_id": "eleven_multilingual_v2"}).encode(),
                                     headers={"xi-api-key": os.environ["ELEVENLABS_API_KEY"], "Content-Type": "application/json"})
        audio = urllib.request.urlopen(req, timeout=30).read()
    elif motor == "kokoro":
        from kokoro import KPipeline                # Apache-2.0
        import numpy as np
        pcm = np.concatenate([a for _, _, a in KPipeline(lang_code="e")(texto, voice=voz or "ef_dora")])
        bio = io.BytesIO()
        with wave.open(bio, "wb") as w:
            w.setnchannels(1); w.setsampwidth(2); w.setframerate(24000); w.writeframes((pcm * 32767).astype("int16").tobytes())
        audio = bio.getvalue()
    if reproducir and audio:
        player = ["mpg123", "-q", "-"] if motor == "elevenlabs" else ["aplay", "-q"]
        if shutil.which(player[0]):
            subprocess.run(player, input=audio)
    return audio


# ================================================================ el servicio
@dataclass
class Oido:
    stt: STT
    memoria: object | None = None                    # orion_memoria.Memoria
    al_hablarme: Callable[[str], str | None] | None = None   # la IA: recibe la orden, devuelve respuesta
    nombre: str = "Orión"
    hablante: str = "voz"
    ventana_s: float = 25.0                          # tras "Orión, …" sigue la conversación sin repetir el nombre
    pausado: bool = False
    hablantes: object | None = None                  # orion_voz.hablantes.Hablantes
    solo_dueno_ordena: bool = True                   # solo la voz del dueño puede dar órdenes
    dueno: str = ""
    _hasta: float = 0.0
    eventos: list = field(default_factory=list)

    def _me_hablan(self, texto: str) -> tuple[bool, str]:
        t = _norm(texto)
        n = _norm(self.nombre).replace("neo ", "")
        m = re.match(rf"^\s*(?:oye\s+|hey\s+|ey\s+)?(?:neo\s+)?{re.escape(n)}\b[\s,.:]*(.*)", t)
        if m and len(t) == len(texto):          # misma longitud → se puede cortar el original (con tildes)
            return True, texto[m.start(1):].strip()
        if m:
            return True, m.group(1)
        return time.time() < self._hasta, texto

    def procesar(self, texto: str) -> dict:
        texto = texto.strip()
        if not texto:
            return {}
        dirigido, orden = self._me_hablan(texto)
        ev = {"texto": texto, "dirigido": dirigido, "quien": self.hablante}
        if dirigido and self.solo_dueno_ordena and self.dueno and self.hablante != self.dueno:
            dirigido, ev["dirigido"], ev["ignorado"] = False, False, "orden de una voz que no es la del dueño"
        if dirigido and re.search(r"\b(pausa|deja de escuchar|no registres|privado)\b", orden):
            self.pausado, ev["accion"] = True, "pausa"
        elif dirigido and re.search(r"\b(sigue|vuelve a escuchar|registra de nuevo|reanuda)\b", orden):
            self.pausado, ev["accion"] = False, "reanuda"
        if self.pausado and ev.get("accion") != "pausa":
            ev["registrado"] = False
            self.eventos.append(ev)
            return ev
        if self.memoria and ev.get("accion") != "pausa":
            self.memoria.diario(texto, quien=self.hablante, etiquetas=["orden"] if dirigido else None)
            if self.hablantes:                       # una línea por persona
                self.memoria.nota(self.hablante, f"- {time.strftime('%Y-%m-%d %H:%M')} {texto}", "personas")
            ev["registrado"] = True
        if dirigido and not ev.get("accion") and self.al_hablarme:
            self._hasta = time.time() + self.ventana_s
            ev["respuesta"] = self.al_hablarme(orden)
            if ev["respuesta"] and self.memoria:
                self.memoria.diario(ev["respuesta"], quien=self.nombre)
        self.eventos.append(ev)
        return ev

    def escuchar(self, frames: Iterator[bytes], vad=None) -> Iterator[dict]:
        vad = vad or crear_vad()
        for seg in segmentos(frames, vad):
            texto = self.stt.transcribir(seg)
            if self.hablantes:
                p, sim = self.hablantes.identificar(seg)
                p = self.hablantes.aprender(p, texto)
                self.hablante = p.etiqueta
            yield self.procesar(texto)
