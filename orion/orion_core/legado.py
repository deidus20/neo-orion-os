"""Puente corto para el núcleo en Rust: atiende UNA acción de un módulo que todavía está en Python y termina.

    echo '{"accion":"ver"}' | python3 -m orion_core.legado respaldo

No queda ningún proceso Python de fondo: el núcleo (Rust) lo abre solo cuando hace falta. A medida que cada módulo
pasa a Rust, deja de llegar aquí (ver nucleo/MIGRACION.md).
"""
from __future__ import annotations

import json
import sys


def main(argv: list[str]) -> int:
    modulo = argv[0] if argv else ""
    try:
        datos = json.loads(sys.stdin.read() or "{}")
    except ValueError:
        datos = {}
    if modulo == "instalar":                       # solo la revisión previa; el núcleo lanza el instalador
        from orion_core.instalador import revisar
        r = {"ok": True, "errores": revisar(dict(datos.get("config") or {}))}
    else:
        from orion_core.servidor import accion
        r = accion(modulo, datos)
    sys.stdout.write(json.dumps(r, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
