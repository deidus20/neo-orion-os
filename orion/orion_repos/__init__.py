"""Buscador de repositorios: antes de construir algo, la IA busca si ya existe.

Para cada capacidad consulta GitHub (gh CLI, con tu sesión), filtra los archivados,
puntúa por estrellas, actividad reciente y licencia, y marca si sirve para uso
comercial. Si no encuentra nada útil, lo dice y la IA lo construye (y lo deja
como repositorio propio en ~/orion/repos/propios/).
"""
from __future__ import annotations
import datetime as dt, json, math, shutil, subprocess, time
from pathlib import Path

LIBRES = {"mit", "apache-2.0", "bsd-2-clause", "bsd-3-clause", "isc", "mpl-2.0", "unlicense", "zlib", "0bsd"}
COPYLEFT = {"gpl-2.0", "gpl-3.0", "agpl-3.0", "lgpl-2.1", "lgpl-3.0"}


def buscar(consulta: str, n: int = 8) -> list[dict]:
    if not shutil.which("gh"):
        raise RuntimeError("falta gh (GitHub CLI): sudo apt install gh && gh auth login")
    campos = "fullName,description,stargazersCount,license,pushedAt,isArchived,url,language"
    out = subprocess.run(["gh", "search", "repos", consulta, "--sort", "stars", "--limit", str(n * 2), "--json", campos],
                         capture_output=True, text=True, timeout=60)
    if out.returncode != 0:
        raise RuntimeError(out.stderr.strip()[:300])
    res = []
    for r in json.loads(out.stdout or "[]"):
        if r.get("isArchived"):
            continue
        lic = (r.get("license") or {}).get("key") or "sin-licencia"
        dias = (dt.datetime.now(dt.timezone.utc) - dt.datetime.fromisoformat(r["pushedAt"].replace("Z", "+00:00"))).days
        puntaje = math.log10(r["stargazersCount"] + 1) * 10 - min(dias, 720) / 72 + (3 if lic in LIBRES else 0) - (8 if lic == "sin-licencia" else 0)
        res.append({"repo": r["fullName"], "estrellas": r["stargazersCount"], "licencia": lic,
                    "comercial": "sí" if lic in LIBRES else ("sí, compartiendo el código" if lic in COPYLEFT else "revisar"),
                    "ultimo_cambio_dias": dias, "lenguaje": r.get("language"), "que_es": (r.get("description") or "")[:160],
                    "url": r["url"], "puntaje": round(puntaje, 1)})
    return sorted(res, key=lambda x: -x["puntaje"])[:n]


def catalogo(capacidades: dict[str, list[str]], n: int = 6, pausa: float = 2.2) -> dict:
    """capacidades: {"oído (voz a texto)": ["speech recognition offline", ...], ...}"""
    out = {}
    for cap, consultas in capacidades.items():
        vistos, lista = set(), []
        for q in consultas:
            try:
                for r in buscar(q, n):
                    if r["repo"] not in vistos:
                        vistos.add(r["repo"]); lista.append(r)
            except RuntimeError as e:
                lista.append({"error": str(e)})
            time.sleep(pausa)                     # límite de búsquedas de GitHub
        out[cap] = sorted([r for r in lista if "repo" in r], key=lambda x: -x["puntaje"])[:n * 2] or lista
    return out


def verificar(repos: list[str]) -> list[dict]:
    """Confirma que repositorios conocidos existen y trae estrellas, licencia y actividad."""
    out = []
    for i in range(0, len(repos), 5):
        q = " ".join(f"repo:{r}" for r in repos[i:i + 5])
        r = subprocess.run(["gh", "api", "-X", "GET", "search/repositories", "-f", f"q={q}", "-f", "per_page=10"],
                           capture_output=True, text=True, timeout=60)
        if r.returncode != 0:
            continue
        for j in json.loads(r.stdout).get("items", []):
            if j.get("archived"):
                continue
            lic = (j.get("license") or {}).get("key") or "sin-licencia"
            dias = (dt.datetime.now(dt.timezone.utc) - dt.datetime.fromisoformat(j["pushed_at"].replace("Z", "+00:00"))).days
            out.append({"repo": j["full_name"], "estrellas": j["stargazers_count"], "licencia": lic,
                        "comercial": "sí" if lic in LIBRES else ("sí, compartiendo el código" if lic in COPYLEFT else "revisar"),
                        "ultimo_cambio_dias": dias, "lenguaje": j.get("language"), "que_es": (j.get("description") or "")[:160],
                        "url": j["html_url"], "puntaje": 100, "referente": True})
        time.sleep(2.2)
    return out
