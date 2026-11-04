"""Aplica la configuración que deja el instalador (hud/instalar.html).

El instalador gráfico genera un JSON (idioma, disco, IA, voz, identidad, color,
módulos, cuenta). Aquí se convierte en pasos concretos para el primer arranque:
descargar las IA elegidas, escribir la identidad, activar módulos y capa.
El particionado y la copia del sistema los hace Calamares con los mismos datos.

Uso:  python -m orion_core.instalacion setup.json [--aplicar]
"""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

PAQ = {"robotica": "robotics", "maker": "maker", "dev": "dev", "juegos": "gaming"}
STT = {"faster-whisper": "pip install --user faster-whisper", "whisper.cpp": "orion install whisper.cpp", "vosk": "pip install --user vosk"}
TTS = {"piper": "pip install --user piper-tts", "kokoro": "pip install --user kokoro", "espeak-ng": "sudo apt-get install -y espeak-ng", "elevenlabs": ""}
MEMORIA_APP = {"obsidian": "flatpak install -y flathub md.obsidian.Obsidian", "logseq": "flatpak install -y flathub com.logseq.Logseq",
               "silverbullet": "orion install silverbullet", "ninguna": ""}
_MODELO = re.compile(r"^[a-z0-9._-]+(:[a-z0-9._-]+)?$")


def validar(c: dict) -> list[str]:
    err = []
    for m in c.get("ia", {}).get("locales", []):
        if not _MODELO.match(m):
            err.append(f"modelo con nombre inválido: {m!r}")
    col = c.get("identidad", {}).get("color", "")
    if col and not re.fullmatch(r"#[0-9a-fA-F]{6}", col):
        err.append(f"color inválido: {col!r}")
    if not re.fullmatch(r"[a-z_][a-z0-9_-]{0,31}", (c.get("cuenta", {}).get("usuario", "") or "").lower()):
        err.append("nombre de usuario inválido para Linux")
    return err


def identidad_yaml(c: dict, base: str) -> str:
    i = c.get("identidad", {})
    genero = {"femenina": "femenino", "masculina": "masculino", "neutra": "neutro"}.get(i.get("voz"), "neutro")
    tono = {"tecnico": "técnico"}.get(i.get("tono"), i.get("tono", "directo"))
    out = []
    for linea in base.splitlines():
        k = linea.split(":", 1)[0].strip()
        if linea.startswith("nombre:"):
            linea = f"nombre: {i.get('nombre', 'Neo Orión')}"
        elif linea.startswith("idioma:"):
            linea = f"idioma: {c.get('idioma', 'es-CL')}"
        elif linea.startswith("color:") and i.get("color"):
            linea = re.sub(r'"#[0-9a-fA-F]{6}"', f'"{i["color"]}"', linea)
        elif linea.startswith("tono:"):
            linea = re.sub(r"^tono:\s*\S+", f"tono: {tono}", linea)
        elif k == "genero":
            linea = re.sub(r"genero:\s*\S+", f"genero: {genero}", linea)
        elif k == "motor" and c.get("voz"):
            linea = re.sub(r"motor:\s*\S+", f"motor: {c['voz']}", linea)
        out.append(linea)
    return "\n".join(out) + "\n"


def pasos(c: dict) -> list[str]:
    p = []
    ia = c.get("ia", {})
    if ia.get("locales"):
        p.append("command -v ollama || curl -fsSL https://ollama.com/install.sh | sh")
        p += [f"ollama pull {m}" for m in ia["locales"]]
    if STT.get(c.get("oido")):
        p.append(STT[c["oido"]])
    if TTS.get(c.get("voz")):
        p.append(TTS[c["voz"]])
    app = (c.get("memoria") or {}).get("app")
    if MEMORIA_APP.get(app):
        p.append(MEMORIA_APP[app])
    for m in c.get("modulos", []):
        if m in PAQ:
            p.append(f"orion install --lista {PAQ[m]}")
    if c.get("capa") == "personal":
        p.append("orion capa importar   # pide el archivo o el otro equipo")
    if ia.get("nube"):
        p.append("orion claves abrir " + " ".join(ia["nube"]) + "   # las claves las pega la persona")
    p.append("systemctl --user enable --now orion-oido.service")
    return p


def aplicar(c: dict, raiz: Path, ejecutar: bool = False) -> dict:
    err = validar(c)
    if err:
        return {"ok": False, "errores": err}
    ident = raiz / "config/identidad.yaml"
    nuevo = identidad_yaml(c, ident.read_text())
    lista = pasos(c)
    if ejecutar:
        ident.write_text(nuevo)
        import subprocess
        for cmd in lista:
            subprocess.run(cmd, shell=True, check=False)
    return {"ok": True, "identidad": nuevo, "pasos": lista}


if __name__ == "__main__":
    cfg = json.loads(Path(sys.argv[1]).read_text())
    r = aplicar(cfg, Path(__file__).resolve().parents[2], "--aplicar" in sys.argv)
    print(json.dumps(r, ensure_ascii=False, indent=2))
