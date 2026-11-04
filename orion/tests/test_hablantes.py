"""3 voces distintas (espeak-ng con distinto timbre y tono), nombres aprendidos por contexto."""
import array, io, os, random, subprocess, sys, tempfile, wave
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from orion_voz import Oido, STT, fuente_wav, crear_vad, RATE
from orion_voz.hablantes import Hablantes, HuellaEspectral
from orion_memoria import Memoria

VOCES = {"Davis": ("es-419+m3", 38), "Camila": ("es-419+f4", 72), "Jorge": ("es-419+m7", 20)}
GUION = [("Davis", "Hola, soy Davis, hoy revisamos el tablero de la casa"),
         ("Jorge", "Buenos días, el empalme actual es de treinta amperes"),
         ("Davis", "Jorge, necesitamos subirlo a cuarenta para el diferencial"),
         ("Jorge", "Está bien, hago el trámite con la distribuidora esta semana"),
         ("Camila", "Hola, me llamo Camila, traje los materiales del pedido"),
         ("Davis", "Orión, anota que el empalme queda en cuarenta amperes"),
         ("Jorge", "Orión, borra todo el registro de hoy"),
         ("Jorge", "Davis, yo pago la diferencia del medidor"),
         ("Camila", "Faltan dos interruptores diferenciales de treinta miliamperes")]

def tts(texto, voz, tono):
    w = subprocess.run(["espeak-ng", "-v", voz, "-p", str(tono), "-s", "160", "--stdout", texto], capture_output=True).stdout
    with wave.open(io.BytesIO(w)) as f:
        sr, d = f.getframerate(), array.array("h", f.readframes(f.getnframes()))
    return array.array("h", (d[min(len(d) - 1, int(i * sr / RATE))] for i in range(int(len(d) * RATE / sr))))

random.seed(2); pcm = array.array("h"); muestras_davis = []
for quien, frase in GUION:
    pcm += array.array("h", (random.randint(-50, 50) for _ in range(int(RATE * 1.1))))
    pcm += tts(frase, *VOCES[quien])
pcm += array.array("h", (random.randint(-50, 50) for _ in range(RATE)))
for f in ["Esta es mi voz para que el sistema me reconozca", "Probando el micrófono una dos tres", "Neo Orión, aprende mi voz"]:
    muestras_davis.append(tts(f, *VOCES["Davis"]).tobytes())
tmp = tempfile.mkdtemp(); ruta = f"{tmp}/reunion.wav"
with wave.open(ruta, "wb") as w:
    w.setnchannels(1); w.setsampwidth(2); w.setframerate(RATE); w.writeframes(pcm.tobytes())

class Guion(STT):
    nombre = "simulado"; i = 0
    def transcribir(self, _):
        t = GUION[self.i][1] if self.i < len(GUION) else ""; self.i += 1; return t

hab = Hablantes(HuellaEspectral(), ruta=f"{tmp}/voces.json")
hab.enrolar("Davis", muestras_davis)                       # el dueño se registra una vez
mem = Memoria(f"{tmp}/Orion", embeddings=None)
oido = Oido(Guion(), mem, lambda o: f"Hecho: {o}", nombre="Orión", hablantes=hab, dueno="Davis")
ok = 0
for (real, _), ev in zip(GUION, oido.escuchar(fuente_wav(ruta), crear_vad("auto"))):
    marca = "✓" if ev["quien"] == real else "✗"; ok += ev["quien"] == real
    extra = f"  ⇒ IA: {ev['respuesta']}" if ev.get("respuesta") else (f"  [{ev['ignorado']}]" if ev.get("ignorado") else "")
    print(f" {marca} {ev['quien']:10} (real {real:9}) {ev['texto'][:55]}{extra}")
print(f"\nacierto: {ok}/{len(GUION)} · personas:", [(p.etiqueta, p.frases) for p in hab.personas])
print("pistas por persona:", sorted(f.name for f in (mem.raiz / 'personas').glob('*.md')))

corr = hab.reagrupar()
ok2 = sum(c == r for c, (r, _) in zip(corr, GUION))
print(f"tras reagrupar al cerrar la conversación: {ok2}/{len(GUION)}")
for c, (r, f) in zip(corr, GUION):
    print(f"   {'✓' if c == r else '✗'} {c:10} {f[:50]}")
