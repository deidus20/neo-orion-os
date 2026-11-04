"""Maneja la VM de prueba por QMP: clic, mantener, teclas, texto y capturas.
uso: python3 vm.py foto archivo.png | clic X Y | mantener X Y ms | tecla ret | texto "hola" | esperar s"""
import json, socket, sys, time
from PIL import Image

W, H = 1280, 800


class QMP:
    def __init__(self, ruta="/tmp/qmp"):
        self.s = socket.socket(socket.AF_UNIX); self.s.connect(ruta); self.f = self.s.makefile("rw")
        self.f.readline(); self.cmd("qmp_capabilities")

    def cmd(self, c, **a):
        self.f.write(json.dumps({"execute": c, "arguments": a} if a else {"execute": c}) + "\n"); self.f.flush()
        while True:
            r = json.loads(self.f.readline())
            if "return" in r or "error" in r:
                return r

    def mover(self, x, y):
        self.cmd("input-send-event", events=[{"type": "abs", "data": {"axis": "x", "value": int(x * 32767 / W)}},
                                             {"type": "abs", "data": {"axis": "y", "value": int(y * 32767 / H)}}])

    def boton(self, abajo):
        self.cmd("input-send-event", events=[{"type": "btn", "data": {"down": abajo, "button": "left"}}])

    def clic(self, x, y):
        self.mover(x, y); time.sleep(.15); self.boton(True); time.sleep(.12); self.boton(False)

    def mantener(self, x, y, ms):
        self.mover(x, y); time.sleep(.15); self.boton(True); time.sleep(ms / 1000); self.boton(False)

    def tecla(self, *nombres):
        ev = [{"type": "key", "data": {"down": True, "key": {"type": "qcode", "data": n}}} for n in nombres]
        ev += [{"type": "key", "data": {"down": False, "key": {"type": "qcode", "data": n}}} for n in reversed(nombres)]
        self.cmd("input-send-event", events=ev); time.sleep(getattr(self, "pausa", .08))

    def texto(self, t):
        for ch in t:
            if ch.isupper():
                self.tecla("shift", ch.lower())
            elif ch == " ":
                self.tecla("spc")
            elif ch == "-":
                self.tecla("minus")
            elif ch.isdigit() or ch.isalpha():
                self.tecla(ch)

    def foto(self, ruta):
        self.cmd("screendump", filename="/tmp/vm.ppm"); Image.open("/tmp/vm.ppm").save(ruta)


if __name__ == "__main__":
    q = QMP(); a = sys.argv[1:]
    while a:
        c = a.pop(0)
        if c == "foto": q.foto(a.pop(0))
        elif c == "clic": q.clic(float(a.pop(0)), float(a.pop(0)))
        elif c == "mantener": q.mantener(float(a.pop(0)), float(a.pop(0)), float(a.pop(0)))
        elif c == "tecla": q.tecla(*a.pop(0).split("+"))
        elif c == "texto": q.texto(a.pop(0))
        elif c == "esperar": time.sleep(float(a.pop(0)))
