"""Foto (silueta de un eslabón de brazo) → sólido a escala → servos integrados → vista previa."""
import os, sys, tempfile
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from PIL import Image, ImageDraw
import orion_forma as f

tmp = tempfile.mkdtemp()
img = Image.new("L", (900, 420), 235); d = ImageDraw.Draw(img)   # "foto" con fondo claro
d.rounded_rectangle((60, 120, 840, 300), radius=90, fill=30)       # eslabón tipo hueso
d.ellipse((20, 80, 280, 340), fill=30); d.ellipse((620, 80, 880, 340), fill=30)
ruta = f"{tmp}/eslabon.png"; img.save(ruta)

print("imagen→3D IA:", f.foto_a_malla(ruta)["motivo"])
r = f.silueta_a_solido(ruta, alto_real_mm=60, espesor_mm=24, nombre="eslabon_brazo", salida=tmp)
print("silueta→sólido:", {k: r[k] for k in ("medidas_mm", "volumen_cm3", "vertices_contorno", "mm_por_pixel")})
L = r["medidas_mm"][0]
r2 = f.integrar_servos(r["step"], [{"modelo": "MG996R", "x": 30, "y": 30, "eje": "A2"},
                                   {"modelo": "MG996R", "x": L - 30, "y": 30, "rot": 180, "eje": "A3"}], salida=tmp)
print("servos:", r2["servos"], "· volumen", r2["volumen_cm3"], "cm³ ·", r2["masa_pla_g"], "g PLA")
print("alternativas:", f.alternativas("brazo robótico de 6 ejes bajo costo")["motivo"][:90])
print(f.vista_previa(r2["stl"], "/tmp/eslabon_servos.png"))
