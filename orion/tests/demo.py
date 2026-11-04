"""Prueba: frases como las diría la IA → órdenes → guardián."""
import sys, tempfile, os
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from orion_motion import Orion, Wave

tmp = tempfile.mkdtemp()
o = Orion(os.path.join(os.path.dirname(__file__), "../profiles/brazo-6dof.json"),
          distance_sensor=lambda: 400, memory_root=tmp)
frases = [
    "Mueve el hombro a 45 grados",
    "baja el codo 30 y luego abre la pinza",
    "servo A1 a 200 grados",                           # fuera de rango
    "ombro a 20 grados lento",                         # error de dictado
    "guarda esta posición como 1A",
    "haz una onda en el codo amplitud 25 frecuencia 0.5 hz durante 6 s",
    "onda en la muñeca amplitud 60 frecuencia 2 hz",   # demasiado rápida
    "posicion inicial",
    "ejecuta 1A",
    "avanza 50 cm",                                    # obstáculo a 40 cm
    "detente",
]
for f in frases:
    print(f"\nIA  › {f}")
    for line in o.say(f):
        print(f"     {line}")
print("\nPose final:", o.robot.pose)
print("Efector (mm):", o.robot.end_effector())
w = Wave("A3", 25, 0.5)
print(f"\nOnda A=25°, f=0.5 Hz → pico {w.peak_velocity:.1f}°/s; x2 velocidad → f=1.0 Hz, pico {Wave('A3',25,0.5,speed_mult=2).peak_velocity:.1f}°/s")
c = o.mem
for cmd, meas in [(0, 3), (45, 47.5), (90, 92)]:
    s = c.record_trial("A2", cmd, meas)
print("Calibración A2:", {k: s[k] for k in ("direccion", "escala", "offset")})
print("Driver recibió", len(o.driver.sent), "órdenes")
