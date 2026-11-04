"""Memoria de Neo Orión — una carpeta de Markdown que cualquier IA y cualquier app puede leer.

Formato (compatible con Obsidian, Logseq, SilverBullet, Zettlr; importable en Joplin,
Trilium y SiYuan):
    ~/Orion/                      ← la "bóveda"
      diario/2026-11-03.md        ← todo lo escuchado y hecho ese día
      proyectos/<proyecto>.md     ← una nota por proyecto, con [[enlaces]]
      personas/, temas/, robots/  ← notas por tema
      .orion/indice.db            ← índice (se puede borrar: se reconstruye)

Cada nota: frontmatter YAML (tipo, fecha, etiquetas, proyecto) + texto + [[enlaces]].
El índice usa SQLite FTS5 (en cualquier equipo) y, si hay un modelo de embeddings
(Ollama), suma búsqueda por significado. Cambiar de IA no pierde nada: la memoria
son archivos, no está dentro de ningún modelo.
"""
from __future__ import annotations
import datetime as dt, hashlib, json, math, os, re, sqlite3, struct, urllib.request
from pathlib import Path

STOP = set("del las los una uno unos unas que con por para como pero sus este esta esto ese esa eso hay muy mas sin sobre entre cuando donde todo toda".split())
DIAS = ["lunes", "martes", "miércoles", "jueves", "viernes", "sábado", "domingo"]
MESES = ["enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre"]
LINK = re.compile(r"\[\[([^\]|#]+)(?:[#|][^\]]*)?\]\]")
TAG = re.compile(r"(?<![\w/])#([\wáéíóúñ-]+)")


def _slug(s: str) -> str:
    s = s.strip().lower()
    for a, b in zip("áéíóúñü", "aeiounu"):
        s = s.replace(a, b)
    return re.sub(r"[^a-z0-9]+", "-", s).strip("-")[:80] or "nota"


def _front(meta: dict) -> str:
    lines = ["---"]
    for k, v in meta.items():
        if isinstance(v, list):
            lines.append(f"{k}: [{', '.join(str(x) for x in v)}]")
        else:
            lines.append(f"{k}: {v}")
    return "\n".join(lines + ["---", ""])


def _parse(text: str) -> tuple[dict, str]:
    if not text.startswith("---"):
        return {}, text
    end = text.find("\n---", 3)
    if end < 0:
        return {}, text
    meta = {}
    for line in text[3:end].strip().splitlines():
        if ":" in line:
            k, v = line.split(":", 1)
            v = v.strip()
            meta[k.strip()] = [x.strip() for x in v[1:-1].split(",") if x.strip()] if v.startswith("[") else v
    return meta, text[end + 4:].lstrip("\n")


class Memoria:
    def __init__(self, raiz: str | None = None, embeddings: str | None = "auto"):
        self.raiz = Path(raiz or os.environ.get("ORION_BOVEDA", os.path.expanduser("~/Orion")))
        for d in ("diario", "proyectos", "temas", "personas", "robots", ".orion"):
            (self.raiz / d).mkdir(parents=True, exist_ok=True)
        self.db = sqlite3.connect(self.raiz / ".orion" / "indice.db")
        self.db.executescript("""
            CREATE VIRTUAL TABLE IF NOT EXISTS fts USING fts5(ruta UNINDEXED, titulo, cuerpo, etiquetas, tokenize='unicode61 remove_diacritics 2');
            CREATE TABLE IF NOT EXISTS archivos(ruta TEXT PRIMARY KEY, hash TEXT, vec BLOB);
            CREATE TABLE IF NOT EXISTS enlaces(origen TEXT, destino TEXT);""")
        self.embed_model = None if embeddings is None else (os.environ.get("ORION_EMBED", "nomic-embed-text") if embeddings == "auto" else embeddings)

    # ------------------------------------------------------------ escribir
    def diario(self, texto: str, quien: str = "voz", etiquetas: list[str] | None = None, cuando: dt.datetime | None = None) -> Path:
        """Agrega una línea al diario del día (lo escuchado, lo hecho)."""
        cuando = cuando or dt.datetime.now()
        f = self.raiz / "diario" / f"{cuando:%Y-%m-%d}.md"
        if not f.exists():
            f.write_text(_front({"tipo": "diario", "fecha": f"{cuando:%Y-%m-%d}", "etiquetas": ["diario"]}) + f"# {DIAS[cuando.weekday()].capitalize()} {cuando.day} de {MESES[cuando.month-1]} de {cuando.year}\n\n")
        tags = " " + " ".join(f"#{t}" for t in etiquetas) if etiquetas else ""
        with f.open("a") as fh:
            fh.write(f"- **{cuando:%H:%M}** _{quien}_: {texto}{tags}\n")
        self.indexar(f)
        return f

    def nota(self, titulo: str, cuerpo: str, carpeta: str = "temas", meta: dict | None = None, anexar: bool = True) -> Path:
        f = self.raiz / carpeta / f"{_slug(titulo)}.md"
        f.parent.mkdir(parents=True, exist_ok=True)
        if f.exists() and anexar:
            with f.open("a") as fh:
                fh.write(f"\n## {dt.datetime.now():%Y-%m-%d %H:%M}\n{cuerpo}\n")
        else:
            m = {"tipo": carpeta.rstrip("s"), "creado": f"{dt.date.today()}", **(meta or {})}
            f.write_text(_front(m) + f"# {titulo}\n\n{cuerpo}\n")
        self.indexar(f)
        return f

    # ------------------------------------------------------------ índice
    def indexar(self, f: Path):
        text = f.read_text(errors="ignore")
        h = hashlib.sha1(text.encode()).hexdigest()
        rel = str(f.relative_to(self.raiz))
        old = self.db.execute("SELECT hash FROM archivos WHERE ruta=?", (rel,)).fetchone()
        if old and old[0] == h:
            return
        meta, body = _parse(text)
        tags = set(meta.get("etiquetas", []) if isinstance(meta.get("etiquetas"), list) else []) | set(TAG.findall(body))
        title = (re.search(r"^# (.+)$", body, re.M) or [None, f.stem])[1]
        self.db.execute("DELETE FROM fts WHERE ruta=?", (rel,))
        self.db.execute("INSERT INTO fts VALUES (?,?,?,?)", (rel, title, body, " ".join(tags)))
        self.db.execute("DELETE FROM enlaces WHERE origen=?", (rel,))
        self.db.executemany("INSERT INTO enlaces VALUES (?,?)", [(rel, _slug(x)) for x in LINK.findall(body)])
        vec = self._embed(f"{title}\n{body[:4000]}")
        self.db.execute("INSERT OR REPLACE INTO archivos VALUES (?,?,?)", (rel, h, struct.pack(f"{len(vec)}f", *vec) if vec else None))
        self.db.commit()

    def reindexar(self) -> int:
        n = 0
        for f in self.raiz.rglob("*.md"):
            if ".orion" not in f.parts:
                self.indexar(f); n += 1
        return n

    def _embed(self, text: str) -> list[float] | None:
        if not self.embed_model:
            return None
        try:
            req = urllib.request.Request("http://127.0.0.1:11434/api/embeddings",
                                         data=json.dumps({"model": self.embed_model, "prompt": text}).encode(),
                                         headers={"Content-Type": "application/json"})
            return json.loads(urllib.request.urlopen(req, timeout=10).read())["embedding"]
        except Exception:
            self.embed_model = None       # sin Ollama: se queda en búsqueda por palabras
            return None

    # ------------------------------------------------------------ leer
    def buscar(self, consulta: str, n: int = 8) -> list[dict]:
        terms = [t for t in re.findall(r"\w+", consulta.lower()) if len(t) > 2 and t not in STOP]
        res: dict[str, dict] = {}
        if terms:
            q = " OR ".join(f'"{t}"*' for t in terms)
            for ruta, titulo, frag, score in self.db.execute(
                    "SELECT ruta, titulo, snippet(fts, 2, '«', '»', '…', 14), bm25(fts) FROM fts WHERE fts MATCH ? ORDER BY bm25(fts) LIMIT ?", (q, n * 2)):
                res[ruta] = {"ruta": ruta, "titulo": titulo, "fragmento": frag, "puntaje": round(-score, 3), "via": "palabras"}
        qv = self._embed(consulta)
        if qv:
            for ruta, blob in self.db.execute("SELECT ruta, vec FROM archivos WHERE vec IS NOT NULL"):
                v = struct.unpack(f"{len(blob)//4}f", blob)
                cos = sum(a * b for a, b in zip(qv, v)) / (math.hypot(*qv) * math.hypot(*v) or 1)
                if cos > .45:
                    r = res.setdefault(ruta, {"ruta": ruta, "titulo": Path(ruta).stem, "fragmento": "", "puntaje": 0, "via": "significado"})
                    r["puntaje"] += round(cos * 10, 3)
        return sorted(res.values(), key=lambda r: -r["puntaje"])[:n]

    def contexto(self, consulta: str, max_chars: int = 6000) -> str:
        """Lo que cualquier IA necesita leer antes de responder: notas relevantes + vínculos."""
        out, used = [], 0
        for r in self.buscar(consulta, 6):
            body = _parse((self.raiz / r["ruta"]).read_text(errors="ignore"))[1]
            chunk = f"### {r['ruta']}\n{body[:1800]}\n"
            if used + len(chunk) > max_chars:
                break
            out.append(chunk); used += len(chunk)
        return "\n".join(out) or "(sin notas relacionadas)"

    def vinculos(self, nota: str) -> dict:
        s = _slug(Path(nota).stem)
        salientes = [d for (d,) in self.db.execute("SELECT destino FROM enlaces WHERE origen LIKE ?", (f"%{s}.md",))]
        entrantes = [o for (o,) in self.db.execute("SELECT origen FROM enlaces WHERE destino=?", (s,))]
        return {"nota": nota, "enlaza_a": salientes, "la_enlazan": entrantes}
