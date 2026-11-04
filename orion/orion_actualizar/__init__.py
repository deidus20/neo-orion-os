"""Actualizaciones de Neo Orión por internet, sin reinstalar.

Dos partes:
  1. El sistema (Ubuntu): parches de seguridad automáticos (unattended-upgrades) y «actualiza todo» por voz.
  2. Neo Orión mismo (núcleo, HUD, instalador, catálogo de IA): paquete `neo-orion-core_<versión>_all.deb`
     publicado en un CANAL (una carpeta web: GitHub Releases, un sitio propio o un pendrive). En el canal hay un
     `canal.json` con la última versión, la dirección del .deb y su SHA-256. Neo Orión baja, verifica la suma y
     recién entonces instala. Si la suma no calza, no instala nada.

`construir_deb(repo, version, salida)` arma el paquete desde el repositorio (lo usa el que publica);
`publicar_canal(...)` escribe el canal.json.
"""
from __future__ import annotations

import hashlib
import json
import os
import re
import shutil
import subprocess
import tempfile
import urllib.request
from pathlib import Path

CONF = Path("/etc/orion/actualizaciones.json")
VERSION = Path("/etc/orion/version")


def version_actual() -> str:
    try:
        m = re.search(r"(\d+\.\d+(?:\.\d+)?)", VERSION.read_text())
        return m.group(1) if m else "0.0"
    except OSError:
        return "0.0"


def mayor(a: str, b: str) -> bool:
    """¿a es más nueva que b?"""
    pa, pb = ([int(x) for x in re.findall(r"\d+", v)] for v in (a, b))
    n = max(len(pa), len(pb))
    return (pa + [0] * (n - len(pa))) > (pb + [0] * (n - len(pb)))


def canal() -> str:
    try:
        return json.loads(CONF.read_text()).get("canal", "")
    except (OSError, ValueError):
        return ""


def revisar(url_canal: str | None = None, leer=None) -> dict:
    url = url_canal or canal()
    if not url:
        return {"ok": False, "error": "no hay canal de actualizaciones configurado (/etc/orion/actualizaciones.json)"}
    leer = leer or (lambda u: urllib.request.urlopen(u, timeout=20).read())
    try:
        c = json.loads(leer(url.rstrip("/") + "/canal.json"))
    except Exception as e:
        return {"ok": False, "error": f"no pude leer el canal ({e.__class__.__name__})"}
    actual = version_actual()
    return {"ok": True, "actual": actual, "ultima": c["version"], "hay_nueva": mayor(c["version"], actual),
            "novedades": c.get("novedades", []), "deb": c["deb"], "sha256": c["sha256"], "canal": url}


def bajar_y_verificar(info: dict, leer=None, carpeta: Path | None = None) -> Path:
    leer = leer or (lambda u: urllib.request.urlopen(u, timeout=120).read())
    url = info["deb"] if re.match(r"^(https?|file)://", info["deb"]) else info["canal"].rstrip("/") + "/" + info["deb"]
    datos = leer(url)
    suma = hashlib.sha256(datos).hexdigest()
    if suma != info["sha256"].lower():
        raise ValueError(f"la suma SHA-256 no calza (esperada {info['sha256'][:12]}…, llegó {suma[:12]}…): no se instala")
    destino = (carpeta or Path(tempfile.mkdtemp())) / Path(url).name
    destino.write_bytes(datos)
    return destino


def actualizar_orion(info: dict | None = None, ejecutar=subprocess.run, leer=None) -> dict:
    info = info or revisar(leer=leer)
    if not info.get("ok"):
        return info
    if not info["hay_nueva"]:
        return {"ok": True, "mensaje": f"Neo Orión ya está al día ({info['actual']})"}
    deb = bajar_y_verificar(info, leer)
    r = ejecutar(["sudo", "apt-get", "install", "-y", str(deb)], capture_output=True, text=True)
    return {"ok": r.returncode == 0, "de": info["actual"], "a": info["ultima"], "novedades": info["novedades"],
            "error": (r.stderr or "")[-300:] if r.returncode else ""}


def actualizar_sistema(ejecutar=subprocess.run) -> dict:
    pasos = [["sudo", "apt-get", "update"], ["sudo", "apt-get", "-y", "full-upgrade"], ["sudo", "apt-get", "-y", "autoremove"]]
    for p in pasos:
        r = ejecutar(p, capture_output=True, text=True)
        if r.returncode:
            return {"ok": False, "paso": " ".join(p), "error": (r.stderr or "")[-300:]}
    reinicio = Path("/var/run/reboot-required").exists()
    try:
        from orion_core.controladores import revisar_al_dia
        drv = revisar_al_dia()
    except Exception:
        drv = {}
    return {"ok": True, "reiniciar": reinicio, "driver_grafica": drv}


# ------------------------------------------------------------ para el que publica
def construir_deb(repo: Path, version: str, salida: Path) -> Path:
    """neo-orion-core_<version>_all.deb con /opt/orion, /usr/share/orion, /usr/local/bin y los servicios."""
    if not shutil.which("dpkg-deb"):
        raise RuntimeError("falta dpkg-deb")
    raiz = Path(tempfile.mkdtemp()) / f"neo-orion-core_{version}_all"
    opt = raiz / "opt/orion"
    opt.mkdir(parents=True)
    for m in sorted((repo / "orion").glob("orion_*")) + [repo / "orion/profiles", repo / "orion/modulos"]:
        if m.exists():
            shutil.copytree(m, opt / m.name, ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))
    ov = repo / "overlay"
    for f in ov.rglob("*"):
        if f.is_file():
            d = raiz / f.relative_to(ov)
            d.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(f, d)
    for carpeta, archivos in (("usr/share/orion/hud", ["index.html", "marathon.js"]), ("usr/share/orion/instalador", ["instalar.html", "marathon.js"])):
        (raiz / carpeta).mkdir(parents=True, exist_ok=True)
        for a in archivos:
            shutil.copy2(repo / "hud" / a, raiz / carpeta / a)
    shutil.copy2(repo / "config/ia-catalogo.json", raiz / "usr/share/orion/ia-catalogo.json")
    (raiz / "DEBIAN").mkdir()
    (raiz / "DEBIAN/control").write_text(
        f"Package: neo-orion-core\nVersion: {version}\nArchitecture: all\nMaintainer: Neo Orión <hola@neo-orion.local>\n"
        "Depends: python3 (>= 3.12), python3-venv, cage\nSection: misc\nPriority: optional\n"
        "Description: Neo Orión OS - núcleo, HUD e instalador\n Interfaz por voz, servidor MCP y módulos de Neo Orión.\n")
    (raiz / "DEBIAN/postinst").write_text("#!/bin/sh\nset -e\n"
        f"mkdir -p /etc/orion\nsed -i 's/^\\(.*OS \\)[0-9.]*/\\1{version}/' /etc/orion/version 2>/dev/null || true\n"
        "systemctl daemon-reload 2>/dev/null || true\n"
        "[ -x /opt/orion/venv/bin/python3 ] || python3 -m venv --system-site-packages /opt/orion/venv\n"
        "echo /opt/orion > \"$(/opt/orion/venv/bin/python3 -c 'import site;print(site.getsitepackages()[0])')/orion.pth\"\n")
    os.chmod(raiz / "DEBIAN/postinst", 0o755)
    salida.mkdir(parents=True, exist_ok=True)
    deb = salida / f"neo-orion-core_{version}_all.deb"
    subprocess.run(["dpkg-deb", "--root-owner-group", "--build", str(raiz), str(deb)], check=True, capture_output=True)
    return deb


def publicar_canal(deb: Path, version: str, novedades: list[str], carpeta: Path) -> Path:
    carpeta.mkdir(parents=True, exist_ok=True)
    if deb.parent.resolve() != carpeta.resolve():
        shutil.copy2(deb, carpeta / deb.name)
    c = {"version": version, "deb": deb.name, "sha256": hashlib.sha256(deb.read_bytes()).hexdigest(), "novedades": novedades}
    (carpeta / "canal.json").write_text(json.dumps(c, ensure_ascii=False, indent=2))
    return carpeta / "canal.json"
