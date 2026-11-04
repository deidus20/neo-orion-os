"""Prueba de la voz siempre activa con audio real sintetizado (espeak-ng) y un STT simulado."""
import array, io, os, random, subprocess, sys, tempfile, wave
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from orion_voz import Oido, STT, fuente_wav, crear_vad, segmentos, hablar, RATE
from orion_memoria import Memoria

FRASES = ["hoy terminé el tablero de la casa de Jorge",
          "Orión, mueve el hombro a cuarenta y cinco grados",
          "y después abre la pinza",
          "Orión, pausa",
          "esto es una conversación privada",
          "Orión, sigue",
          "anota que el empalme debe ser de cuarenta amperes"]

def tts16k(texto):
    wavb = subprocess.run(["espeak-ng", "-v", "es-419", "--stdout", texto], capture_output=True).stdout
    with wave.open(io.BytesIO(wavb)) as w:
        sr, data = w.getframerate(), array.array("h", w.readframes(w.getnframes()))
    n = int(len(data) * RATE / sr)
    return array.array("h", (data[min(len(data) - 1, int(i * sr / RATE))] for i in range(n)))

random.seed(1)
pcm = array.array("h")
ruido = lambda s: array.array("h", (random.randint(-60, 60) for _ in range(int(RATE * s))))
for f in FRASES:
    pcm += ruido(1.2); pcm += tts16k(f)
pcm += ruido(1.5)
tmp = tempfile.mkdtemp(); ruta = f"{tmp}/prueba.wav"
with wave.open(ruta, "wb") as w:
    w.setnchannels(1); w.setsampwidth(2); w.setframerate(RATE); w.writeframes(pcm.tobytes())
print(f"audio: {len(pcm)/RATE:.1f} s")

for nombre in ("webrtcvad", "energia", "combinado"):
    vad = crear_vad(nombre) if nombre != "energia" else __import__("orion_voz").VADEnergia()
    segs = list(segmentos(fuente_wav(ruta), vad))
    print(f"VAD {type(vad).__name__}: {len(segs)} frases detectadas (esperadas {len(FRASES)}) · duraciones",
          [round(len(s) / RATE / 2, 1) for s in segs])

class STTGuion(STT):      # en tu equipo aquí va faster-whisper / moonshine / vosk / whisper.cpp
    nombre = "simulado"
    def __init__(self): self.i = 0
    def transcribir(self, pcm):
        t = FRASES[self.i] if self.i < len(FRASES) else ""; self.i += 1; return t

mem = Memoria(f"{tmp}/Orion", embeddings=None)
ia = lambda orden: f"Hecho: «{orden}»."
oido = Oido(STTGuion(), mem, ia, nombre="Neo Orión", hablante="Davis")
for ev in oido.escuchar(fuente_wav(ruta), crear_vad("auto")):
    print(f"  {'→ IA ' if ev.get('respuesta') else '     '}{'[NO REGISTRADO] ' if ev.get('registrado') is False else ''}{ev['texto']}" + (f"  ⇒ {ev['respuesta']}" if ev.get("respuesta") else "") + (f"  [{ev['accion']}]" if ev.get("accion") else ""))
print("\nDIARIO:\n" + open(next(mem.raiz.glob("diario/*.md"))).read())
print("TTS espeak-ng bytes:", len(hablar("Sistema en línea. Te escucho.", "espeak-ng", reproducir=False)))
