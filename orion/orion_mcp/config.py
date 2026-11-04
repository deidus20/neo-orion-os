"""Genera la configuración MCP para la IA que elijas (Claude Code, OpenCode, Claude Desktop, Goose…)
a partir de config/modulos.json. Así cualquier IA ve los mismos módulos."""
import json, os, sys
from pathlib import Path
RAIZ = Path(__file__).resolve().parents[2]
mods = json.loads((RAIZ / "config/modulos.json").read_text())
exp = lambda x: os.path.expanduser(x) if isinstance(x, str) else x
servidores = {}
for nombre, m in mods.items():
    if nombre.startswith("_") or m.get("tipo") != "mcp":
        continue
    cmd = [exp(c) for c in m["comando"]]
    servidores[m["nombre_visible"] if not m.get("mostrar_marca", True) else nombre] = {
        "command": cmd[0], "args": cmd[1:], "env": {k: exp(v) for k, v in m.get("env", {}).items()}}
formato = sys.argv[1] if len(sys.argv) > 1 else "claude"
if formato == "opencode":
    print(json.dumps({"$schema": "https://opencode.ai/config.json",
                      "mcp": {k: {"type": "local", "command": [v["command"], *v["args"]], "environment": v["env"]} for k, v in servidores.items()}}, indent=2))
else:
    print(json.dumps({"mcpServers": servidores}, indent=2))
