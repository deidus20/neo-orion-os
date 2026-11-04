"""Ejemplo real de obra: habilitación de oficina 30 m² (estructura usada en tus propuestas)."""
import os, sys, tempfile
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from orion_proyectos import Proyecto, Tarea as T
p = Proyecto("oficina-30m2", "Habilitación oficina 30 m²", cliente="Cliente particular", inicio="2026-11-09", tareas=[
    T("1", "Preliminares"), T("1.1", "Levantamiento y medición", 1, responsable="Davis", raci={"Cliente": "I"}, costo=60000, avance=1),
    T("1.2", "Permisos y compra de materiales", 3, ["1.1"], "Davis", {"Cliente": "A"}, 40000, .5),
    T("2", "Obra gruesa"), T("2.1", "Demolición y retiro", 2, ["1.1"], "Maestro", {"Davis": "A"}, 180000),
    T("2.2", "Tabiquería", 4, ["2.1", "1.2"], "Maestro", {"Davis": "A"}, 520000),
    T("3", "Instalaciones"), T("3.1", "Canalización eléctrica", 3, ["2.2"], "Davis", {"Cliente": "I"}, 380000),
    T("3.2", "Tablero y protecciones", 1, ["3.1"], "Davis", {}, 290000),
    T("3.3", "Sanitario", 2, ["2.2"], "Gasfíter", {"Davis": "A"}, 240000),
    T("4", "Terminaciones"), T("4.1", "Pintura", 3, ["3.2", "3.3"], "Maestro", {"Davis": "A"}, 310000),
    T("4.2", "Pisos", 2, ["4.1"], "Maestro", {"Davis": "A"}, 420000),
    T("4.3", "Entrega y TE1", 0, ["4.2"], "Davis", {"Cliente": "A"}, 0)])
info = p.programar()
print("Duración:", info["duracion_dias"], "días · término", info["termino"])
print("Ruta crítica:", " → ".join(info["ruta_critica"]))
for t in p.tareas:
    print(f"  {t.id:5} {t.nombre:32} EI {t.ei:4g} EF {t.ef:4g} holgura {t.holgura:3g} {'CRÍTICA' if t.critica else ''}")
print("Presupuesto:", p.presupuesto())
print("Avance al 2026-11-12:", p.avance("2026-11-12"))
print("RACI alertas:", p.raci()["alertas"], "· personas", p.raci()["personas"])
tmp = tempfile.mkdtemp()
open("/tmp/gantt.svg", "w").write(p.gantt_svg())
print(p.a_excel(f"{tmp}/oficina.xlsx"), p.guardar(tmp))
q = Proyecto.cargar(str(p.guardar(tmp))); print("recarga OK:", q.programar()["duracion_dias"])
