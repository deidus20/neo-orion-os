import sys
from pathlib import Path

RAIZ = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(RAIZ / "orion"))
from orion_core.instalacion import aplicar  # noqa: E402

# Mismo formato que genera hud/instalar.html al terminar
CFG = {"version": "0.8", "idioma": "es-CL", "teclado": "latam", "zona": "America/Santiago", "equipo": "potente",
       "disco": {"dispositivo": "/dev/nvme0n1", "modo": "junto"}, "ia": {"locales": ["qwen3:14b", "gemma3:4b"], "nube": ["claude"]},
       "oido": "faster-whisper", "voz": "piper", "memoria": {"app": "obsidian"},
       "identidad": {"nombre": "Vega", "voz": "femenina", "tono": "cercano", "color": "#ff3d9a"},
       "modulos": ["robotica", "juegos", "proyectos"], "capa": "personal", "cuenta": {"usuario": "davis", "equipo": "orion"}}

r = aplicar(CFG, RAIZ)
assert r["ok"], r
y = r["identidad"]
assert "nombre: Vega" in y and '"#ff3d9a"' in y and "genero: femenino" in y and "tono: cercano" in y, y
p = r["pasos"]
assert "ollama pull qwen3:14b" in p and "ollama pull gemma3:4b" in p
assert "orion install --lista robotics" in p and "orion install --lista gaming" in p
assert any("orion capa importar" in x for x in p) and any("claves abrir claude" in x for x in p)
assert (RAIZ / "config/identidad.yaml").read_text().startswith("# Identidad")  # no se escribió nada sin --aplicar

malo = dict(CFG, ia={"locales": ["qwen3; rm -rf /"]}, cuenta={"usuario": "Da vis"})
r = aplicar(malo, RAIZ)
assert not r["ok"] and len(r["errores"]) == 2, r
print("instalación ok:", len(p), "pasos")
