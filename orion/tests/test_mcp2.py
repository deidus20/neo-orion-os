import asyncio, json, os, sys, tempfile
tmp = tempfile.mkdtemp()
os.environ.update(ORION_HOME=tmp + "/home", ORION_WORKSPACE=tmp + "/work", ORION_BOVEDA=tmp + "/Orion")
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from mcp.client import Client
from orion_mcp.server import mcp
from PIL import Image, ImageDraw
img = Image.new("L", (600, 300), 240); ImageDraw.Draw(img).rounded_rectangle((40, 60, 560, 240), 60, fill=20); img.save(tmp + "/pieza.png")
async def main():
    async with Client(mcp) as c:
        names = [t.name for t in (await c.list_tools()).tools]
        print(len(names), "herramientas:", ", ".join(names))
        async def call(n, **k):
            r = await c.call_tool(n, k); t = r.content[0].text if r.content else ""
            try: return json.loads(t)
            except Exception: return t
        print("perfil:", (await call("perfil_equipo"))["nivel"])
        print("modulos:", {k: v["nombre"] for k, v in (await call("modulos")).items()})
        r = await call("proyecto_crear", id="brazo", nombre="Brazo robótico 6 ejes", inicio="2026-11-04", tareas=[
            {"id": "1", "nombre": "Diseño"}, {"id": "1.1", "nombre": "CAD", "dias": 4, "costo": 0},
            {"id": "1.2", "nombre": "Placa de control", "dias": 3, "depende": ["1.1"], "costo": 25000},
            {"id": "2", "nombre": "Fabricación"}, {"id": "2.1", "nombre": "Impresión 3D", "dias": 2, "depende": ["1.1"], "costo": 18000},
            {"id": "2.2", "nombre": "Ensamble y calibración", "dias": 2, "depende": ["2.1", "1.2"], "costo": 120000}])
        print("proyecto:", r["programa"], "total", r["presupuesto"]["total"])
        print("actualizar:", (await call("proyecto_actualizar", id="brazo", cambios=[{"id": "1.1", "avance": 1}]))["avance"])
        print("exportar:", list((await call("proyecto_exportar", id="brazo")).keys()))
        r = await call("forma_foto_a_3d", imagen=tmp + "/pieza.png", alto_real_mm=40, espesor_mm=15)
        print("foto→3D:", r.get("via"), r.get("medidas_mm"))
        r = await call("forma_integrar_servos", step=r["step"], servos=[{"modelo": "SG90", "x": 20, "y": 20, "eje": "A1"}])
        print("servos:", r["servos"])
        await call("memoria_anotar", texto="Brazo: placa con ESP32-S3 + PCA9685, ver [[brazo]]", titulo="Decisión placa brazo", carpeta="proyectos")
        r = await c.call_tool("memoria_buscar", {"consulta": "placa ESP32 brazo"}); print("memoria:", [json.loads(x.text)["ruta"] if x.text.startswith("{") else x.text[:80] for x in r.content])
asyncio.run(main())
