"""Entrada de consola de orion_forma: el núcleo (Rust) la llama como a cualquier programa externo de CAD.

    echo '{"descripcion": "soporte"}' | python3 -m orion_forma alternativas
    echo '{"imagen": "foto.png", "alto_real_mm": 60}' | python3 -m orion_forma foto_a_3d
    echo '{"step": "pieza.step", "servos": [{"modelo": "MG996R", "x": 30, "y": 30}]}' | python3 -m orion_forma integrar_servos

Responde una línea JSON. Es Python porque los motores CAD (build123d/CadQuery sobre OpenCascade) lo son; el resto de
Neo Orión corre en Rust.
"""
from __future__ import annotations

import json
import os
import sys

from . import alternativas, foto_a_malla, integrar_servos, silueta_a_solido, vista_previa

SALIDA = os.path.join(os.environ.get("ORION_WORKSPACE", os.path.expanduser("~/orion")), "forma")


def main(argv: list[str]) -> int:
    orden = argv[0] if argv else ""
    a = json.loads(sys.stdin.read() or "{}")
    if orden == "alternativas":
        r = alternativas(a["descripcion"], int(a.get("n", 3)), salida=SALIDA)
    elif orden == "foto_a_3d":
        r = foto_a_malla(a["imagen"], salida=SALIDA)
        if not r["ok"]:
            r = silueta_a_solido(a["imagen"], float(a.get("alto_real_mm", 100)), float(a.get("espesor_mm", 10)), salida=SALIDA) | {"via": "silueta (CPU)"}
        if r.get("stl"):
            r["vista"] = vista_previa(r["stl"], r["stl"].replace(".stl", ".png"))
    elif orden == "integrar_servos":
        r = integrar_servos(a["step"], a.get("servos") or [], salida=SALIDA)
        r["vista"] = vista_previa(r["stl"], r["stl"].replace(".stl", ".png"))
    else:
        r = {"ok": False, "error": "orden: alternativas | foto_a_3d | integrar_servos"}
    sys.stdout.write(json.dumps(r, ensure_ascii=False, default=str) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
