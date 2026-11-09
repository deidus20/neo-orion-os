"""Puente entre IAs: Neo Orión le pasa trabajo a otra IA con contexto y trae la respuesta.

Ida  (Neo Orión → otra IA):  "usa Claude para crear el documento con lo que hemos hablado".
       Se arma un PAQUETE DE CONTEXTO (memoria + conversación reciente + tarea) y se envía
       por la vía OFICIAL de cada IA, con la cuenta de la persona:
         · claude   → Claude Code (`claude -p`), entra con tu plan Pro/Max, sin API key.
         · gemini   → Gemini CLI (`gemini -p`), entra con tu cuenta de Google.
         · codex    → Codex CLI (`codex exec`), entra con tu cuenta de ChatGPT.
         · opencode → OpenCode (`opencode run`), con el proveedor que tenga configurado.
         · local    → Ollama, sin internet.
         · web      → abre el chat web de la IA con el paquete copiado al portapapeles y TÚ
                      aprietas enviar; después pegas la respuesta. (Automatizar el clic en la
                      web va contra los términos de varios servicios —el de Claude incluido— y
                      arriesga la cuenta, por eso aquí siempre lo aprieta la persona.)
       La respuesta queda en la memoria, en colaboraciones/.

Vuelta (otra IA → Neo Orión): conecta el servidor MCP de Neo Orión a la app de Claude
       (escritorio) o a Claude Code. Desde ese chat, Claude le pide cosas a tu PC
       ("instala esto", "arma el plano", "mueve el brazo") y Neo Orión las hace con sus
       permisos de siempre. Ver `config_vuelta()`.
"""
from __future__ import annotations

import datetime as dt
import json
import os
import re
import shutil
import subprocess
from pathlib import Path

VIAS = {
    "claude":   {"nombre": "Claude", "bin": "claude",   "cmd": lambda p: ["claude", "-p", p, "--output-format", "json", "--max-turns", "6"],
                 "instalar": "npm install -g @anthropic-ai/claude-code && claude   # entra con tu cuenta (Pro/Max)",
                 "web": "https://claude.ai/new"},
    "gemini":   {"nombre": "Gemini", "bin": "gemini",   "cmd": lambda p: ["gemini", "-p", p],
                 "instalar": "npm install -g @google/gemini-cli && gemini   # entra con tu cuenta de Google",
                 "web": "https://gemini.google.com/app"},
    "codex":    {"nombre": "ChatGPT / Codex", "bin": "codex", "cmd": lambda p: ["codex", "exec", p],
                 "instalar": "npm install -g @openai/codex && codex   # entra con tu cuenta de ChatGPT",
                 "web": "https://chatgpt.com/"},
    "opencode": {"nombre": "OpenCode", "bin": "opencode", "cmd": lambda p: ["opencode", "run", p],
                 "instalar": "curl -fsSL https://opencode.ai/install | bash", "web": None},
    "local":    {"nombre": "IA local", "bin": "ollama",   "cmd": None, "instalar": "orion-fit --install", "web": None},
}
ALIAS = {"anthropic": "claude", "claude code": "claude", "google": "gemini", "chatgpt": "codex", "openai": "codex", "gpt": "codex", "ollama": "local"}


def via_de(texto: str) -> str:
    t = texto.lower()
    for k in list(VIAS) + list(ALIAS):
        if re.search(rf"\b{re.escape(k)}\b", t):
            return ALIAS.get(k, k)
    return "claude"


def disponible(via: str) -> bool:
    return bool(shutil.which(VIAS[via]["bin"]))


def paquete(tarea: str, memoria=None, conversacion: list[str] | None = None, max_chars: int = 9000) -> str:
    """Lo que la otra IA necesita para trabajar sin preguntar todo de nuevo."""
    partes = ["# Encargo de Neo Orión", "",
              "Trabajas para la misma persona que usa Neo Orión (su asistente en el PC). Responde en español de Chile, claro y completo.",
              "Si entregas un documento, entrégalo completo en Markdown, listo para guardar.", "", "## Tarea", tarea.strip(), ""]
    if conversacion:
        partes += ["## Lo que se ha conversado (más reciente al final)"] + [f"- {c}" for c in conversacion[-12:]] + [""]
    if memoria is not None:
        try:
            partes += ["## Notas de su memoria relacionadas", memoria.contexto(tarea, max_chars=max_chars // 2), ""]
        except Exception:
            pass
    partes += ["## Al terminar", "Devuelve solo el resultado. Si te falta un dato que no puedes suponer, dilo en una línea al final."]
    texto = "\n".join(partes)
    return texto[:max_chars]


def _guardar(memoria, via: str, tarea: str, respuesta: str) -> str | None:
    if memoria is None:
        return None
    titulo = f"{dt.date.today()} {VIAS[via]['nombre']} — {tarea[:60]}"
    f = memoria.nota(titulo, f"**Tarea:** {tarea}\n\n**Respuesta de {VIAS[via]['nombre']}:**\n\n{respuesta}",
                     carpeta="colaboraciones", meta={"ia": via}, anexar=False)
    return str(f)


def _limpia(via: str, salida: str) -> str:
    if via == "claude":
        try:
            return json.loads(salida).get("result", salida)
        except json.JSONDecodeError:
            return salida
    return salida


def delegar(tarea: str, via: str | None = None, memoria=None, conversacion: list[str] | None = None,
            modelo_local: str = "qwen3:8b", ejecutar=subprocess.run, segundos: int = 600) -> dict:
    """Envía la tarea con contexto. Si la vía no está instalada, dice cómo dejarla lista
    y ofrece la vía web manual."""
    via = via or via_de(tarea)
    p = paquete(tarea, memoria, conversacion)
    v = VIAS[via]
    if via == "local":
        if not disponible("local"):
            return {"ok": False, "via": via, "falta": v["instalar"]}
        cmd = ["ollama", "run", modelo_local, p]
    else:
        if not disponible(via):
            return {"ok": False, "via": via, "falta": v["instalar"], "alternativa_web": v["web"], "paquete": p}
        cmd = v["cmd"](p)
    r = ejecutar(cmd, capture_output=True, text=True, timeout=segundos)
    if r.returncode != 0:
        return {"ok": False, "via": via, "error": (r.stderr or r.stdout)[-600:]}
    resp = _limpia(via, r.stdout).strip()
    return {"ok": True, "via": via, "ia": v["nombre"], "respuesta": resp, "guardado_en": _guardar(memoria, via, tarea, resp)}


def abrir_web(tarea: str, via: str = "claude", memoria=None, conversacion=None) -> dict:
    """Vía manual: copia el paquete al portapapeles y abre el chat web. La persona aprieta enviar."""
    p = paquete(tarea, memoria, conversacion)
    url = VIAS[via]["web"]
    copiado = False
    for c in (["wl-copy"], ["xclip", "-selection", "clipboard"]):
        if shutil.which(c[0]):
            subprocess.run(c, input=p, text=True); copiado = True; break
    if url and shutil.which("xdg-open"):
        subprocess.Popen(["xdg-open", url])
    return {"ok": True, "via": via, "url": url, "copiado": copiado, "paquete": p,
            "siguiente": "Pega con Ctrl+V, envía, y cuando responda dime «pega la respuesta» o pégala en el chat de Neo Orión."}


def recibir(via: str, tarea: str, respuesta: str, memoria=None) -> dict:
    """Guarda lo que la persona trae de vuelta desde la web."""
    return {"ok": True, "guardado_en": _guardar(memoria, via, tarea, respuesta)}


def config_vuelta(raiz: Path | None = None) -> dict:
    """Cómo conectar Neo Orión a Claude para que Claude le pida cosas al PC."""
    raiz = raiz or Path(__file__).resolve().parents[2]
    srv = {"command": "/usr/local/bin/orion-nucleo", "args": ["mcp"]}
    escritorio = Path(os.path.expanduser("~/.config/Claude/claude_desktop_config.json"))
    return {
        "claude_code": "claude mcp add orion-linux -- /usr/local/bin/orion-nucleo mcp",
        "claude_escritorio": {"archivo": str(escritorio), "agregar": {"mcpServers": {"orion-linux": srv}}},
        "nota": "Con esto, desde el chat de Claude puedes decir «pídele a mi PC que…» y Neo Orión lo hace. "
                "Lo físico (robots, PLC) y lo irreversible siguen pidiendo tu confirmación en el PC.",
    }
