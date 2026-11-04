"""Cuadernos de investigación: Gemini Notebook (antes NotebookLM) y Open Notebook (libre, local).

Gemini Notebook (Google renombró NotebookLM el 16-jul-2026; sigue en notebooklm.google.com)
  No tiene API pública para cuentas personales y sus términos no permiten manejarlo por código.
  Neo Orión hace lo que sí corresponde:
    1. arma las FUENTES: notas de la memoria, del proyecto o de una investigación → archivos Markdown
       listos para subir (carpeta ~/Orion/exportar/cuaderno/<tema>/)
    2. abre Gemini Notebook en el navegador normal; la persona crea el cuaderno y sube los archivos
    3. lo que el cuaderno responda (resumen, guía, preguntas), la persona lo pega y queda en la memoria

Open Notebook (lfnovo/open-notebook, MIT): la misma idea pero en el propio PC, con las IA locales
  (Ollama) o la nube que se elija. Tiene API REST local (puerto 5055) y Neo Orión sí la maneja entera:
  crea el cuaderno y le carga las fuentes sin que la persona toque nada.
"""
from __future__ import annotations

import datetime as dt
import json
import re
import shutil
import urllib.error
import urllib.request
from pathlib import Path

WEB = "https://notebooklm.google.com"
OPEN_NB_API = "http://127.0.0.1:5055"
OPEN_NB_WEB = "http://127.0.0.1:8502"
MAX_FUENTE = 400_000          # caracteres por archivo (Gemini Notebook admite fuentes grandes; se corta con holgura)
COMPOSE_OLLAMA = "https://raw.githubusercontent.com/lfnovo/open-notebook/main/examples/docker-compose-ollama.yml"


def _slug(s: str) -> str:
    s = re.sub(r"[^\w\s-]", "", s.lower(), flags=re.U)
    return re.sub(r"[\s_]+", "-", s).strip("-")[:60] or "cuaderno"


def preparar_fuentes(tema: str, memoria=None, textos: dict[str, str] | None = None,
                     archivos: list[str] | None = None, raiz: Path | None = None, n_notas: int = 12) -> dict:
    """Junta todo lo que sirve sobre `tema` en archivos listos para subir a un cuaderno.

    memoria: orion_memoria.Memoria (busca las notas relacionadas)
    textos:  {titulo: texto} extra (p. ej. el resultado de una investigación del HUD)
    archivos: rutas que se copian tal cual (PDF, docx, md…)
    """
    base = (raiz or Path.home() / "Orion") / "exportar" / "cuaderno" / f"{dt.date.today():%Y-%m-%d}-{_slug(tema)}"
    base.mkdir(parents=True, exist_ok=True)
    hechos: list[str] = []

    def escribir(nombre: str, titulo: str, cuerpo: str):
        f = base / f"{len(hechos) + 1:02d}-{_slug(nombre)}.md"
        f.write_text(f"# {titulo}\n\n{cuerpo.strip()[:MAX_FUENTE]}\n")
        hechos.append(str(f))

    if memoria is not None:
        try:
            for r in memoria.buscar(tema, n=n_notas):
                ruta = Path(r.get("ruta", ""))
                if not ruta.is_absolute():
                    ruta = Path(memoria.raiz) / ruta
                cuerpo = ruta.read_text() if ruta.is_file() else r.get("fragmento", "")
                escribir(ruta.stem or r.get("titulo", "nota"), r.get("titulo") or ruta.stem, cuerpo)
        except Exception:
            pass
    for titulo, texto in (textos or {}).items():
        escribir(titulo, titulo, texto)
    for a in archivos or []:
        p = Path(a).expanduser()
        if p.is_file():
            destino = base / f"{len(hechos) + 1:02d}-{p.name}"
            shutil.copy2(p, destino)
            hechos.append(str(destino))
    guia = (f"# Cómo subir estas fuentes\n\nTema: **{tema}**\n\n"
            "1. En Gemini Notebook toca **Crear** (cuaderno nuevo).\n"
            "2. **Agregar fuentes → Subir** y elige todos los archivos de esta carpeta (menos este).\n"
            "3. Pide lo que quieras: resumen, guía de estudio, resumen en audio, preguntas de prueba.\n"
            "4. Copia la respuesta y pégala en el chat de Neo Orión (tecla /) diciendo «guarda lo del cuaderno».\n")
    (base / "00-LEEME.md").write_text(guia)
    return {"ok": bool(hechos), "carpeta": str(base), "fuentes": hechos,
            "aviso": "" if hechos else "No encontré notas sobre ese tema: agrega textos o archivos."}


def abrir_gemini_notebook(tema: str, memoria=None, textos=None, archivos=None, abrir=None) -> dict:
    """Prepara las fuentes y abre Gemini Notebook en el navegador normal (lo usa la persona)."""
    r = preparar_fuentes(tema, memoria, textos, archivos)
    if abrir is None:
        import orion_navegador as nv
        abrir = nv.abrir
    try:
        nav = abrir(WEB)
    except Exception as e:                       # sin navegador: igual quedan las fuentes
        nav = {"ok": False, "error": str(e)}
    for cmd in (["xdg-open", r["carpeta"]],):    # carpeta a la vista para arrastrar los archivos
        if shutil.which(cmd[0]):
            try:
                import subprocess
                subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            except OSError:
                pass
    return {**r, "navegador": nav, "modo": "manejas tú",
            "pasos": ["Crea un cuaderno", f"Sube los archivos de {r['carpeta']}", "Pídele lo que necesites",
                      "Pega la respuesta en Neo Orión para guardarla"],
            "por_que": "Gemini Notebook no tiene API para cuentas personales y sus términos no permiten manejarlo por código."}


def recibir(tema: str, respuesta: str, memoria=None) -> dict:
    """Guarda en la memoria lo que devolvió el cuaderno (pegado por la persona)."""
    if not respuesta.strip():
        return {"ok": False, "error": "respuesta vacía"}
    if memoria is None:
        return {"ok": True, "guardado": None}
    f = memoria.nota(f"Cuaderno · {tema}", f"_Desde Gemini Notebook, {dt.datetime.now():%Y-%m-%d %H:%M}_\n\n{respuesta.strip()}",
                     carpeta="colaboraciones", meta={"fuente": "gemini-notebook", "tema": tema})
    return {"ok": True, "guardado": str(f)}


# ------------------------------------------------------------------ Open Notebook (local)
class OpenNotebook:
    """Cliente mínimo de la API REST local de Open Notebook."""

    def __init__(self, base: str = OPEN_NB_API, timeout: float = 30):
        self.base, self.timeout = base.rstrip("/"), timeout

    def _req(self, metodo: str, ruta: str, datos: dict | None = None):
        req = urllib.request.Request(self.base + ruta, method=metodo,
                                     data=json.dumps(datos).encode() if datos is not None else None,
                                     headers={"Content-Type": "application/json"})
        with urllib.request.urlopen(req, timeout=self.timeout) as r:
            cuerpo = r.read()
            return json.loads(cuerpo) if cuerpo else {}

    def disponible(self) -> bool:
        try:
            self._req("GET", "/api/notebooks")
            return True
        except (urllib.error.URLError, OSError, ValueError):
            return False

    def cuadernos(self) -> list[dict]:
        return self._req("GET", "/api/notebooks")

    def crear(self, nombre: str, descripcion: str = "") -> dict:
        return self._req("POST", "/api/notebooks", {"name": nombre, "description": descripcion})

    def agregar_texto(self, cuaderno_id: str, titulo: str, texto: str) -> dict:
        return self._req("POST", "/api/sources", {"type": "text", "notebook_id": cuaderno_id, "title": titulo,
                                                  "content": texto, "embed": True})

    def agregar_url(self, cuaderno_id: str, url: str) -> dict:
        return self._req("POST", "/api/sources", {"type": "link", "notebook_id": cuaderno_id, "url": url, "embed": True})


def enviar_open_notebook(tema: str, memoria=None, textos=None, cliente: OpenNotebook | None = None) -> dict:
    """Todo automático: crea el cuaderno en Open Notebook y le carga las fuentes."""
    c = cliente or OpenNotebook()
    if not c.disponible():
        return {"ok": False, "error": "Open Notebook no está corriendo", "instalar": instalar_open_notebook()}
    r = preparar_fuentes(tema, memoria, textos)
    nb = c.crear(f"{tema}", "Creado por Neo Orión")
    nid = nb.get("id") or nb.get("notebook_id")
    cargadas = 0
    for f in r["fuentes"]:
        p = Path(f)
        if p.suffix == ".md":
            c.agregar_texto(nid, p.stem, p.read_text())
            cargadas += 1
    return {"ok": True, "cuaderno": nid, "fuentes": cargadas, "abrir": OPEN_NB_WEB, "carpeta": r["carpeta"]}


def instalar_open_notebook() -> list[str]:
    """Pasos (Docker + Ollama local). La clave de cifrado se genera en el equipo; no pasa por ningún chat."""
    return ["sudo apt-get install -y docker.io docker-compose-v2",
            "mkdir -p ~/.orion/open-notebook && cd ~/.orion/open-notebook",
            f"curl -fsSL {COMPOSE_OLLAMA} -o docker-compose.yml",
            "sed -i \"s/change-me-to-a-secret-string/$(openssl rand -hex 32)/\" docker-compose.yml",
            "sudo docker compose up -d",
            f"# luego abre {OPEN_NB_WEB}"]
