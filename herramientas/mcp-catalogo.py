"""Saca de orion_mcp/server.py el catálogo de herramientas MCP (nombre, descripción, esquema) para el núcleo Rust.

    python3 herramientas/mcp-catalogo.py > config/mcp-herramientas.json

Se corre cuando cambia una herramienta del lado Python; al terminar la migración el catálogo se edita directo.
"""
import inspect, json, sys, typing
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "orion"))
sys.modules.setdefault("mcp", None)            # fuerza el FastMCP mínimo: no hace falta el SDK
from orion_mcp import server as S             # noqa: E402

def tipo(t):
    o = typing.get_origin(t)
    if o in (typing.Union, getattr(__import__("types"), "UnionType", None)):
        a = [x for x in typing.get_args(t) if x is not type(None)]
        return tipo(a[0]) if a else {}
    if o in (list, tuple):
        a = typing.get_args(t)
        return {"type": "array", "items": tipo(a[0]) if a else {}}
    if o is dict or t is dict:
        return {"type": "object"}
    return {str: {"type": "string"}, int: {"type": "integer"}, float: {"type": "number"}, bool: {"type": "boolean"},
            list: {"type": "array"}}.get(t, {})

fuera = {"_identity", "_gate", "_mem", "_proy", "_cad_build"}
herr = []
for n, f in inspect.getmembers(S, inspect.isfunction):
    if n in fuera or n.startswith("_") or f.__module__ != S.__name__:
        continue
    tipos = typing.get_type_hints(f)
    props, req = {}, []
    for p in inspect.signature(f).parameters.values():
        props[p.name] = tipo(tipos.get(p.name, str))
        if p.default is inspect.Parameter.empty:
            req.append(p.name)
        elif p.default is not None:
            props[p.name]["default"] = p.default
    herr.append({"name": n, "description": inspect.cleandoc(f.__doc__ or ""),
                 "inputSchema": {"type": "object", "properties": props, "required": req}})
orden = {n: i for i, n in enumerate(S.__dict__)}
herr.sort(key=lambda h: orden[h["name"]])
print(json.dumps({"instrucciones": S.mcp.instrucciones if hasattr(S.mcp, "instrucciones") else "", "herramientas": herr},
                 ensure_ascii=False, indent=1))
