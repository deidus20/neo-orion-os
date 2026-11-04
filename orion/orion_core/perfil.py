"""Perfil del equipo → qué alternativa usar en cada pieza.

Cada computador es distinto: una Raspberry 4 no puede con lo mismo que una torre
con GPU. Este módulo clasifica el equipo en un NIVEL y elige, para cada pieza
(oído, voz, memoria, IA, 3D…), la opción más capaz que corre fluida.
La persona puede forzar cualquier elección en ~/.orion/backends.json.

Licencias: se marcan las que NO son libres para uso comercial, porque si algún
día vendes algo hecho con ellas importa.
"""
from __future__ import annotations
import json, os
from pathlib import Path
from . import detect

NIVELES = ["micro", "patata", "ligero", "medio", "potente", "estacion"]   # patata: PC viejo, 2–4 GB, sin escritorio

# pieza → nivel → lista en orden de preferencia (la primera instalada gana)
CATALOGO: dict[str, dict[str, list[dict]]] = {
    "oido": {   # voz → texto
        "micro":    [{"id": "vosk-small-es", "lic": "Apache-2.0", "nota": "~40 MB, streaming, corre hasta en Pi Zero"}],
        "ligero":   [{"id": "moonshine-tiny", "lic": "MIT (inglés) / Community no comercial (español)", "nota": "streaming, <300 ms en Pi 5"},
                     {"id": "whisper.cpp-base", "lic": "MIT", "nota": "más lento, muy preciso"},
                     {"id": "vosk-small-es", "lic": "Apache-2.0", "nota": "respaldo"}],
        "medio":    [{"id": "faster-whisper-small", "lic": "MIT", "nota": "CPU int8"},
                     {"id": "moonshine-base", "lic": "MIT / Community", "nota": "menor latencia"}],
        "potente":  [{"id": "faster-whisper-large-v3-turbo", "lic": "MIT", "nota": "GPU, máxima precisión"},
                     {"id": "moonshine-medium", "lic": "MIT / Community", "nota": ""}],
        "estacion": [{"id": "faster-whisper-large-v3", "lic": "MIT", "nota": "con diarización (quién habla)"}],
    },
    "voz": {    # texto → voz
        "micro":    [{"id": "espeak-ng", "lic": "GPL-3.0", "nota": "robótica, casi sin recursos"}],
        "ligero":   [{"id": "piper", "lic": "GPL-3.0 (piper1-gpl) / licencia según cada voz", "nota": "natural, ~60 MB por voz"}],
        "medio":    [{"id": "piper", "lic": "GPL-3.0 / según voz", "nota": ""}, {"id": "kokoro-82m", "lic": "Apache-2.0", "nota": "más expresiva"}],
        "potente":  [{"id": "kokoro-82m", "lic": "Apache-2.0", "nota": ""}, {"id": "xtts-v2", "lic": "Coqui Public (no comercial)", "nota": "clona voces"}],
        "estacion": [{"id": "xtts-v2", "lic": "Coqui Public (no comercial)", "nota": ""}],
        "_nube":    [{"id": "elevenlabs", "lic": "servicio pagado", "nota": "se enlaza con tu clave"}],
    },
    "vad": {    # detectar cuándo alguien habla (sin botón)
        "micro":    [{"id": "energia", "lic": "propio", "nota": "umbral de energía"}],
        "ligero":   [{"id": "webrtcvad", "lic": "BSD", "nota": ""}],
        "medio":    [{"id": "silero-vad", "lic": "MIT", "nota": "más fino con ruido"}],
        "potente":  [{"id": "silero-vad", "lic": "MIT", "nota": ""}],
        "estacion": [{"id": "silero-vad", "lic": "MIT", "nota": ""}],
    },
    "memoria_indice": {
        "micro":    [{"id": "sqlite-fts5", "lic": "dominio público", "nota": "búsqueda por palabras, 0 RAM extra"}],
        "ligero":   [{"id": "sqlite-fts5+embeddings-mini", "lic": "Apache-2.0", "nota": "all-MiniLM vía Ollama/ONNX"}],
        "medio":    [{"id": "sqlite-fts5+nomic-embed", "lic": "Apache-2.0", "nota": "búsqueda por significado"}],
        "potente":  [{"id": "lancedb+bge-m3", "lic": "Apache-2.0 / MIT", "nota": "millones de notas"}],
        "estacion": [{"id": "qdrant+bge-m3", "lic": "Apache-2.0", "nota": "servidor vectorial"}],
    },
    "memoria_app": {   # cómo ves y editas la memoria (todas leen la misma carpeta)
        "_todos": [
            {"id": "carpeta-markdown", "lic": "—", "nota": "base: archivos .md, nada que instalar"},
            {"id": "obsidian", "lic": "propietaria gratuita (no es código abierto)", "nota": "abre la carpeta directo"},
            {"id": "logseq", "lic": "AGPL-3.0", "nota": "abre la carpeta directo; diario por día"},
            {"id": "silverbullet", "lic": "MIT", "nota": "web liviana: ideal para Raspberry y teléfono"},
            {"id": "zettlr", "lic": "GPL-3.0", "nota": "editor Markdown académico"},
            {"id": "joplin", "lic": "AGPL-3.0", "nota": "importa la carpeta; sincroniza cifrado"},
            {"id": "trilium-notes", "lic": "AGPL-3.0", "nota": "importa Markdown; árbol jerárquico"},
            {"id": "siyuan", "lic": "AGPL-3.0", "nota": "importa Markdown; bloques"},
        ]},
    "imagen": {   # generar imágenes de alternativas
        "micro": [], "ligero": [{"id": "nube", "lic": "según servicio", "nota": "no alcanza localmente"}],
        "medio":    [{"id": "sd-1.5-lcm", "lic": "CreativeML OpenRAIL-M", "nota": "4–6 GB VRAM o CPU lento"}],
        "potente":  [{"id": "flux.1-schnell", "lic": "Apache-2.0", "nota": "12 GB+ (cuantizado)"}, {"id": "sdxl-turbo", "lic": "Stability no comercial", "nota": ""}],
        "estacion": [{"id": "flux.1-schnell", "lic": "Apache-2.0", "nota": ""}],
    },
    "foto_a_3d": {
        "micro":    [{"id": "silueta-extruida", "lic": "propio", "nota": "contorno de la foto → sólido (CPU)"}],
        "ligero":   [{"id": "silueta-extruida", "lic": "propio", "nota": ""}],
        "medio":    [{"id": "triposr", "lic": "MIT", "nota": "~6 GB VRAM; CPU tarda minutos"}, {"id": "silueta-extruida", "lic": "propio", "nota": "respaldo"}],
        "potente":  [{"id": "hunyuan3d-2.1-forma", "lic": "Tencent Community (no UE/UK/Corea, tope comercial)", "nota": "~10 GB solo forma"},
                     {"id": "triposr", "lic": "MIT", "nota": ""}],
        "estacion": [{"id": "trellis.2", "lic": "MIT", "nota": "24 GB, la mejor calidad abierta"},
                     {"id": "hunyuan3d-2.1", "lic": "Tencent Community", "nota": "forma + textura ~29 GB"}],
    },
    "escaneo_3d": {   # escanear un objeto, una pieza, un lugar o a ti mismo con fotos/video
        "micro": [{"id": "capturar-y-enviar", "lic": "—", "nota": "toma las fotos; otro equipo procesa"}],
        "ligero": [{"id": "capturar-y-enviar", "lic": "—", "nota": ""}],
        "medio": [{"id": "colmap+openmvs", "lic": "BSD / AGPL-3.0", "nota": "fotogrametría en CPU (lento, preciso)"},
                  {"id": "meshroom", "lic": "MPL-2.0", "nota": "fotogrametría con interfaz"}],
        "potente": [{"id": "nerfstudio-splatfacto", "lic": "Apache-2.0", "nota": "gaussian splatting desde video del teléfono"},
                    {"id": "colmap+openmvs", "lic": "BSD / AGPL-3.0", "nota": "malla para CAD/impresión"}],
        "estacion": [{"id": "nerfstudio-splatfacto", "lic": "Apache-2.0", "nota": ""}],
    },
    "cuerpo_3d": {   # modelo de cuerpo humano ajustable (altura, contextura, musculatura)
        "medio": [{"id": "smpl-x", "lic": "no comercial (investigación)", "nota": "cuerpo paramétrico: se ajusta forma y pose"}],
        "potente": [{"id": "smpl-x", "lic": "no comercial (investigación)", "nota": ""}],
        "estacion": [{"id": "smpl-x", "lic": "no comercial (investigación)", "nota": ""}],
    },
    "editar_imagen": {   # "muéstrame esta foto con X cambio"
        "potente": [{"id": "qwen-image-edit", "lic": "Apache-2.0", "nota": "edición por instrucción"}],
        "estacion": [{"id": "qwen-image-edit", "lic": "Apache-2.0", "nota": ""}],
        "_nube": [{"id": "nube", "lic": "según servicio", "nota": ""}],
    },
}


def nivel(h: dict | None = None) -> str:
    h = h or detect.host()
    ram, tipo = h.get("ram_gb", 0), h.get("tipo")
    vram = 0
    if h.get("gpu_nvidia"):
        try:
            import subprocess
            out = subprocess.run(["nvidia-smi", "--query-gpu=memory.total", "--format=csv,noheader,nounits"],
                                 capture_output=True, text=True).stdout
            vram = max(int(x) for x in out.split()) / 1024
        except Exception:
            vram = 0
    if vram >= 20:
        return "estacion"
    if vram >= 8:
        return "potente"
    if tipo in ("raspberry-pi", "android-termux") and ram <= 4.5:
        return "micro"
    if ram <= 4.5 or not h.get("avx2", True):
        return "patata"
    if ram <= 8.5 or tipo in ("raspberry-pi", "jetson"):
        return "ligero"
    return "medio"


def elegir(pieza: str, nv: str, override: dict | None = None) -> dict | None:
    override = override or {}
    if pieza in override:
        return {"id": override[pieza], "lic": "?", "nota": "elegido por la persona"}
    cat = CATALOGO[pieza]
    if "_todos" in cat:
        return cat["_todos"][0]
    idx = NIVELES.index(nv)
    for n in NIVELES[idx::-1]:          # si el nivel no tiene opción, baja un nivel
        if cat.get(n):
            return cat[n][0]
    return (cat.get("_nube") or [None])[0]


def perfil(override_path: str | None = None, h: dict | None = None) -> dict:
    p = Path(override_path or os.path.expanduser("~/.orion/backends.json"))
    override = json.loads(p.read_text()) if p.exists() else {}
    h = h or detect.host()
    nv = override.get("nivel") or nivel(h)
    return {"nivel": nv, "host": h.get("tipo"), "ram_gb": h.get("ram_gb"),
            "eleccion": {k: elegir(k, nv, override) for k in CATALOGO},
            "alternativas": {k: v.get(nv) or v.get("_todos") or [] for k, v in CATALOGO.items()}}


if __name__ == "__main__":
    print(json.dumps(perfil(), ensure_ascii=False, indent=2))
