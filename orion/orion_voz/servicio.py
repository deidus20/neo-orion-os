"""Servicio del oído (systemd --user orion-oido): micrófono → texto → memoria → IA → voz.

Lee la elección de /etc/orion/instalacion.json (oido, voz, identidad). Si todavía no hay motor de
voz a texto descargado (primer arranque sin internet), espera y reintenta sin romper nada.
Lo que oye también lo publica en ~/.orion/oido.jsonl para que el HUD lo muestre.
"""
from __future__ import annotations

import json
import shutil
import sys
import time
from pathlib import Path

from orion_voz import Oido, crear_stt, fuente_arecord, hablar


def config() -> dict:
    try:
        return json.loads(Path("/etc/orion/instalacion.json").read_text())
    except (OSError, ValueError):
        return {}


def main() -> int:
    c = config()
    if not shutil.which("arecord"):
        print("oído: falta arecord (alsa-utils)", file=sys.stderr)
        return 1
    try:
        stt = crear_stt(c.get("oido", "auto"))
    except Exception as e:                       # modelo aún no descargado
        print(f"oído: motor de voz no disponible todavía ({e}); reintento en 5 min", file=sys.stderr)
        time.sleep(300)
        return 1
    try:
        from orion_memoria import Memoria
        mem = Memoria(str(Path.home() / "Orion"), embeddings=None)
    except Exception:
        mem = None
    from orion_core.servidor import charlar
    nombre = (c.get("identidad") or {}).get("nombre", "Neo Orión")
    motor = c.get("voz", "auto")
    salida = Path.home() / ".orion/oido.jsonl"
    salida.parent.mkdir(parents=True, exist_ok=True)

    def responder(orden: str) -> str:
        r = charlar(orden).get("texto", "")
        if r:
            try:
                hablar(r, motor=motor)
            except Exception:
                pass
        return r

    from orion_voz import activacion as act
    palmadas = act.palmadas_de(act.leer())
    eventos = Path.home() / ".orion/activaciones.jsonl"

    def disparar(activador: dict, como: str):
        """El HUD lee este archivo (/api/eventos) y hace lo visual; aquí se hace lo del sistema (pantalla, IA, luces)."""
        hechos = act.ejecutar_rutina([a for a in activador["rutina"] if a in ("despertar", "reposo", "luces", "suspender")])
        for paso in activador["rutina"]:
            if paso.startswith("di:"):
                try:
                    hablar(paso[3:].strip(), motor=motor)
                except Exception:
                    pass
        with eventos.open("a") as f:
            f.write(json.dumps({"t": time.time(), "activador": activador["id"], "como": como,
                                "rutina": activador["rutina"], "hechos": hechos}, ensure_ascii=False) + "\n")
        if "saludo" in activador["rutina"]:
            try:
                hablar(act.saludo(c.get("cuenta", {}).get("usuario", "").capitalize(), plantilla=act.leer().get("saludo")), motor=motor)
            except Exception:
                pass

    def frames_con_palmadas():
        for fr in fuente_arecord():
            n = palmadas.alimentar(fr)
            if n:
                a = act.elegir(act.leer(), palmadas=n)
                if a:
                    disparar(a, f"{n} palmadas")
            yield fr

    llave = None
    try:
        from orion_voz.llave import LlaveVoz
        llave = LlaveVoz()
    except Exception:
        pass
    oido = Oido(stt=stt, memoria=mem, al_hablarme=responder, nombre=nombre.split()[-1])
    for ev in oido.escuchar(frames_con_palmadas()):
        a = act.elegir(act.leer(), texto=ev.get("texto", ""), es_dueno=(ev.get("quien") == oido.dueno) if oido.dueno else None)
        if a:
            disparar(a, f"«{a.get('frase')}»")
        with salida.open("a") as f:
            f.write(json.dumps({"t": time.time(), **ev}, ensure_ascii=False) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
