import json
import sys
import tempfile
from pathlib import Path
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import orion_puente as pu  # noqa: E402
from orion_memoria import Memoria  # noqa: E402

assert pu.via_de("usa Claude para crear el documento") == "claude"
assert pu.via_de("pregúntale a ChatGPT") == "codex"
assert pu.via_de("que lo haga gemini") == "gemini"

tmp = tempfile.mkdtemp()
mem = Memoria(tmp, embeddings=None)
mem.nota("Brazo", "Brazo de 4 ejes, servos en el hombro, base giratoria.", carpeta="notas")
conv = ["Davis: quiero el informe del brazo", "Orion: tengo las notas con 14 fichas"]

p = pu.paquete("crea un informe del brazo", mem, conv)
assert "## Tarea" in p and "hombro" in p.lower() and "14 fichas" in p, p

# Sin la herramienta instalada: dice cómo instalarla y ofrece la web
pu.shutil.which = lambda b: None
r = pu.delegar("usa Claude para el informe", memoria=mem, conversacion=conv)
assert not r["ok"] and "claude-code" in r["falta"] and r["alternativa_web"] == "https://claude.ai/new" and "## Tarea" in r["paquete"]

# Con la herramienta (simulada): envía el paquete y guarda la respuesta en la memoria
pu.shutil.which = lambda b: "/usr/bin/" + b
visto = {}
def falso(cmd, **k):
    visto["cmd"] = cmd
    return SimpleNamespace(returncode=0, stdout=json.dumps({"result": "# Informe\nTodo listo."}), stderr="")
r = pu.delegar("usa Claude para crear el informe del brazo", memoria=mem, conversacion=conv, ejecutar=falso)
assert r["ok"] and r["respuesta"].startswith("# Informe"), r
assert visto["cmd"][:2] == ["claude", "-p"] and "--max-turns" in visto["cmd"]
assert Path(r["guardado_en"]).exists() and "colaboraciones" in r["guardado_en"]
assert mem.buscar("Informe brazo")

v = pu.config_vuelta()
assert v["claude_code"].startswith("claude mcp add orion-linux")
print("puente ok")
