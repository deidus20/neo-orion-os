"""El "traductor": convierte lo que la IA dice en palabras en órdenes exactas.

Son dos mentes trabajando juntas:
  1. La IA (Claude, Qwen, la que sea) piensa y habla en lenguaje natural:
       "mueve el hombro a 45 grados y después haz una onda suave en el codo"
  2. Este traductor (Python, determinista, sin IA) reconoce esa frase por
     aproximación y la convierte en órdenes que el robot entiende:
       SET A2 45.0 · WAVE A3 amp=20 f=0.5
Así la IA nunca toca los motores directamente: todo pasa por aquí y luego
por el Guardián (guardian.py), que puede rechazar la orden.
"""
from __future__ import annotations
import difflib, re
from dataclasses import dataclass, field
from .robot import Robot, norm

NUM = r"(-?\d+(?:[.,]\d+)?)"


@dataclass
class Command:
    kind: str                     # SET, MOVE_REL, WAVE, MACRO, SAVE, DRIVE, TURN, STOP, CALIBRATE, HOME
    joint: str | None = None
    value: float | None = None
    params: dict = field(default_factory=dict)
    source: str = ""              # la frase original de la IA
    confidence: float = 1.0

    def __str__(self):
        parts = [self.kind]
        if self.joint:
            parts.append(self.joint)
        if self.value is not None:
            parts.append(f"{self.value:g}")
        parts += [f"{k}={v:g}" if isinstance(v, (int, float)) else f"{k}={v}" for k, v in self.params.items()]
        return " ".join(parts)


def _f(x: str) -> float:
    return float(x.replace(",", "."))


class Translator:
    def __init__(self, robot: Robot, macros: dict | None = None):
        self.robot = robot
        self.macros = macros or {}
        self.vocab: dict[str, str] = {}
        for j in robot.joints:
            self.vocab[norm(j.id)] = j.id
            for a in j.alias:
                self.vocab[norm(a)] = j.id

    # -- reconocer a qué articulación se refiere ---------------------------
    def find_joint(self, text: str) -> tuple[str | None, float]:
        t = norm(text)
        m = re.search(r"\b(?:servo|motor|articulacion|eje)?\s*([a-z]\d)\b", t)
        if m and norm(m.group(1)) in self.vocab:
            return self.vocab[norm(m.group(1))], 1.0
        # frases de varias palabras primero ("rotacion muneca")
        for alias in sorted(self.vocab, key=len, reverse=True):
            if len(alias) > 2 and re.search(rf"\b{re.escape(alias)}\b", t):
                return self.vocab[alias], 1.0
        # aproximación: tolera errores de dictado ("ombro", "codito")
        best, score = None, 0.0
        for word in re.findall(r"[a-z]{3,}", t):
            match = difflib.get_close_matches(word, self.vocab.keys(), n=1, cutoff=0.75)
            if match:
                s = difflib.SequenceMatcher(None, word, match[0]).ratio()
                if s > score:
                    best, score = self.vocab[match[0]], s
        return best, score

    # -- una frase puede traer varias órdenes ------------------------------
    def parse(self, text: str) -> list[Command]:
        chunks = re.split(r"\s*(?:,|;|\by luego\b|\by despues\b|\bdespues\b|\bluego\b|\by\b(?=\s+(?:mueve|gira|haz|lleva|ejecuta|guarda|avanza|retrocede|abre|cierra)))\s*", norm(text))
        out: list[Command] = []
        for c in filter(None, (c.strip() for c in chunks)):
            cmd = self.parse_one(c)
            if cmd:
                cmd.source = c
                out.append(cmd)
        return out

    def parse_one(self, t: str) -> Command | None:
        if re.search(r"\b(alto|detente|para|stop|emergencia|frena)\b", t):
            return Command("STOP")
        if re.search(r"\b(inicio|home|reposo|posicion inicial)\b", t):
            return Command("HOME")
        m = re.search(r"\b(?:guarda|graba|memoriza)\b.*?\b(?:como|en)\s+([a-z0-9]{1,6})\b", t)
        if m:
            return Command("SAVE", params={"name": m.group(1).upper()})
        m = re.search(r"\b(?:ejecuta|posicion|repite|movimiento|secuencia)\s+([0-9]{1,3}[a-z]|[a-z][0-9]{1,3})\b", t)
        if m and m.group(1).upper() in self.macros:
            return Command("MACRO", params={"name": m.group(1).upper()})
        # base con ruedas
        m = re.search(rf"\b(avanza|retrocede)\s+{NUM}\s*(cm|m|mm)?", t)
        if m:
            d = _f(m.group(2)) * {"m": 1000, "cm": 10, "mm": 1, None: 10}[m.group(3)]
            return Command("DRIVE", value=d if m.group(1) == "avanza" else -d, params={"unit": "mm"})
        m = re.search(rf"\bgira\s+{NUM}\s*(?:grados|°)?\s+(?:a la\s+)?(izquierda|derecha)", t)
        if m:
            return Command("TURN", value=_f(m.group(1)) * (1 if m.group(2) == "izquierda" else -1))

        joint, conf = self.find_joint(t)
        if re.search(r"\bcalibra", t):
            return Command("CALIBRATE", joint=joint, confidence=conf)
        if joint is None:
            return None
        # onda / oscilación
        if re.search(r"\b(onda|oscila|ondula|balancea|sinusoidal|senoidal)", t):
            amp = re.search(rf"amplitud\s+(?:de\s+)?{NUM}", t)
            frq = re.search(rf"frecuencia\s+(?:de\s+)?{NUM}|{NUM}\s*hz", t)
            dur = re.search(rf"(?:durante|por)\s+{NUM}\s*(?:s|seg)", t)
            mul = re.search(rf"(?:x|por|multiplicad[oa] por)\s*{NUM}\s*(?:veces)?\s*(?:mas rapido|la velocidad)", t)
            p = {"amp": _f(amp.group(1)) if amp else (10.0 if "suave" in t else 20.0),
                 "freq": _f(next(g for g in frq.groups() if g)) if frq else 0.5,
                 "dur": _f(dur.group(1)) if dur else 4.0,
                 "mult": _f(mul.group(1)) if mul else 1.0}
            return Command("WAVE", joint=joint, params=p, confidence=conf)
        # pinza
        if joint == self.robot.joints[-1].id and re.search(r"\b(abre|cierra)\b", t):
            j = self.robot.joint(joint)
            return Command("SET", joint=joint, value=j.max if "abre" in t else j.min, confidence=conf)
        speed = re.search(rf"(?:a|con)\s+(?:velocidad\s+)?{NUM}\s*(?:grados por segundo|°/s|gps)", t)
        params = {"vel": _f(speed.group(1))} if speed else {}
        if re.search(r"\b(lento|despacio|suave)\b", t):
            params.setdefault("vel", 30.0)
        # relativo: "20 grados mas", "baja 10"
        m = re.search(rf"{NUM}\s*(?:grados|°)?\s*(mas|menos)\b", t) or \
            re.search(rf"\b(sube|baja|abre|cierra)\s+(?:el\s+\w+\s+)?{NUM}", t)
        if m:
            g = m.groups()
            if g[0] in ("sube", "baja", "abre", "cierra"):
                val = _f(g[1]) * (1 if g[0] in ("sube", "abre") else -1)
            else:
                val = _f(g[0]) * (1 if g[1] == "mas" else -1)
            return Command("MOVE_REL", joint=joint, value=val, params=params, confidence=conf)
        m = re.search(rf"(?:a|en|hasta)\s+{NUM}\s*(?:grados|°|deg)?", t) or re.search(rf"{NUM}\s*(?:grados|°|deg)", t)
        if m:
            return Command("SET", joint=joint, value=_f(m.group(1)), params=params, confidence=conf)
        return None
