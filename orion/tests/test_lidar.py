"""Robot recorre una sala de 8×6 m con una mesa y una persona caminando: mapa, móviles y frenado."""
import json, math, os, sys
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import numpy as np
from orion_lidar import Simulador, Mapa, sectores, decision_avance
np.random.seed(3)
muros = [(0, 0, 8000, 0), (8000, 0, 8000, 6000), (8000, 6000, 0, 6000), (0, 6000, 0, 0),
         (3000, 2500, 4200, 2500), (4200, 2500, 4200, 3300), (4200, 3300, 3000, 3300), (3000, 3300, 3000, 2500),   # mesa
         (5500, 0, 5500, 1800)]                                                                                     # tabique
sim = Simulador(muros, [{"centro": (6500, 4500), "radio": 200, "vel": (-600, -150)}])
mapa = Mapa(10000, 8000, 50, (-1000, -1000))
ruta = [(1000 + 120 * k, 1200 + 25 * k, 0.15) for k in range(40)]
for k, pose in enumerate(ruta):
    e = sim.escanear(pose); mapa.actualizar(e); sim.avanzar(0.1)
print("celdas ocupadas:", int(mapa.ocupado().sum()), "· celdas exploradas:", int((np.abs(mapa.logodds) > 0.3).sum()))
mov = mapa.moviles(e)
print("puntos móviles (persona):", len(mov), "· centro aprox:", mov.mean(0).round(0).tolist() if len(mov) else None, "· real:", [round(c) for c in sim.personas[0]["centro"]])
print("sectores:", [(s["sector"], round(s["min_mm"]) if s["min_mm"] else None) for s in sectores(e)])
# frente a un muro a 600 mm a 400 mm/s:
d = decision_avance(sim.escanear((7400, 3000, 0.0)), 400); print("muro a 600 mm, 400 mm/s →", d)
d = decision_avance(sim.escanear((2000, 3000, 0.0)), 400); print("pasillo libre →", d)
# imagen del mapa en la estética del sistema
import matplotlib; matplotlib.use("Agg"); import matplotlib.pyplot as plt
fig, ax = plt.subplots(figsize=(7, 5.6), facecolor="#101113"); ax.set_facecolor("#101113")
L = mapa.logodds; img = np.zeros(L.shape + (3,)); img[L < -0.3] = (0.11, 0.12, 0.13); img[mapa.ocupado()] = (0.80, 0.96, 0.39)
ax.imshow(img, origin="lower", extent=(-1000, 9000, -1000, 7000))
pts = e.puntos_mundo(); ax.scatter(pts[:, 0], pts[:, 1], s=2, c="#e6e8ea")
if len(mov): ax.scatter(mov[:, 0], mov[:, 1], s=14, c="#ff4a2a")
xs, ys, _ = zip(*ruta); ax.plot(xs, ys, c="#cdf564", lw=1, ls="--"); ax.add_patch(plt.Rectangle((xs[-1] - 150, ys[-1] - 150), 300, 300, fc="#cdf564"))
ax.set_axis_off(); ax.text(-900, 6700, "LIDAR · MAPA DE OCUPACIÓN", color="#cdf564", family="monospace", fontsize=10)
fig.savefig("/tmp/lidar_mapa.png", dpi=120, bbox_inches="tight", facecolor="#101113")
json.dump({"mapa": np.packbits(mapa.ocupado()).tolist()[:5], "puntos": pts[::3].round().tolist(), "moviles": mov.round().tolist()}, open("/tmp/lidar_demo.json", "w"))
print("OK")
