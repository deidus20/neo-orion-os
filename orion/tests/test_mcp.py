"""Simula a una IA usando el servidor MCP por detrás, como lo haría Claude o un modelo local."""
import asyncio, json, os, sys, tempfile
tmp = tempfile.mkdtemp()
os.environ["ORION_HOME"] = tmp + "/home"; os.environ["ORION_WORKSPACE"] = tmp + "/work"
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from mcp.client import Client
from orion_mcp.server import mcp, policy

BRACKET = '''
with BuildPart() as b:
    Box(PARAMS["largo"], PARAMS["ancho"], PARAMS["grosor"])
    with Locations((0, 0, PARAMS["grosor"]/2)):
        Box(PARAMS["largo_servo"] + 2*PARAMS["pared"], PARAMS["ancho"], PARAMS["alto"], align=(Align.CENTER, Align.CENTER, Align.MIN))
    with Locations((0, 0, PARAMS["grosor"]/2 + PARAMS["pared"])):
        Box(PARAMS["largo_servo"], PARAMS["ancho"], PARAMS["alto"], align=(Align.CENTER, Align.CENTER, Align.MIN), mode=Mode.SUBTRACT)
    with Locations((-PARAMS["largo"]/2 + 6, 0, 0), (PARAMS["largo"]/2 - 6, 0, 0)):
        Hole(PARAMS["tornillo"]/2)
pieza = b.part
'''

async def main():
    async with Client(mcp) as s:
        tools = await s.list_tools()
        print("HERRAMIENTAS:", ", ".join(t.name for t in tools.tools))
        async def call(name, **kw):
            r = await s.call_tool(name, kw)
            txt = r.content[0].text if r.content else ""
            try: return json.loads(txt)
            except Exception: return txt
        info = await call("sistema_info")
        print("\nHOST:", info["host"]["tipo"], info["host"]["arch"], f'{info["host"].get("ram_gb")} GB',
              "· IA local:", (info["ia_local"] or {}).get("best", {}).get("tag"))
        r = await call("cad_construir", nombre="soporte_servo", codigo=BRACKET,
                       params={"largo": 70, "ancho": 22, "grosor": 5, "largo_servo": 40.5, "alto": 20, "pared": 3, "tornillo": 3.4})
        print("\nCAD:", r.get("medidas_mm"), r.get("error", "")[:300])
        r = await call("cad_modificar", nombre="soporte_servo", cambios={"grosor": 3, "pared": 2.5})
        print("CAD modificado:", r.get("cambios"), r.get("medidas_mm"))
        r = await call("ejecutar", comando="rm -rf / --no-preserve-root")
        print("\nPELIGROSO:", r)
        r = await call("ejecutar", comando="echo hola desde $(uname -m) && python3 --version")
        print("NORMAL:", r["salida"].strip())
        r = await call("clonar_repo", url="github.com/dorianborian/sesame-robot")
        print("\nCLONAR:", r["ok"], r["archivos"], "archivos")
        r = await call("robot_orden", texto="hombro a 45 grados y luego onda en el codo amplitud 25 frecuencia 0.5")
        print("\nROBOT:", r["resultado"])
        r = await call("plc_escribir", ip="192.168.0.10", area="DB", direccion="DB1.DBX0.0", valor="1", motivo="arrancar cinta 2")
        print("PLC:", r)
        r = await call("hud_mostrar", tipo="modelo3d", titulo="Soporte servo MG996R", datos={"stl": "soporte_servo.stl"})
        print("\nHUD:", r)
        r = await call("identidad", cambios={"nombre": "Vega", "genero": "femenino", "color": "#ffb000", "permisos": "root"})
        print("IDENTIDAD:", r)
        print("\nPENDIENTES (solo la persona aprueba):", list(policy.pending.values()))
asyncio.run(main())
