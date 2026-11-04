"""Íconos y lista de aplicaciones de Neo Orión, con una REGLA ESTÉTICA fija.

Cada programa instalado (nube, series, juegos, navegador, oficina…) aparece en el HUD con un ícono de
bloques que respeta la estética del sistema. Cuando la IA instala algo nuevo, lo registra aquí: si trae
un ícono propio se valida contra la regla; si no, se le asigna el pictograma de su categoría. Así la
interfaz nunca pierde su identidad, por muchos programas que se agreguen.

REGLA ESTÉTICA (la misma que siguen los íconos del HUD):
  1. Cuadrícula de 9 × 9 bloques. Cada fila es un texto de 9 caracteres: «#» bloque, «.» vacío.
  2. Un solo color: el acento de la persona (lo pinta el HUD). Sin degradados, sin sombras, sin fotos.
  3. Entre 20 % y 65 % de la cuadrícula lleno: ni mancha ni hilo.
  4. Sin bloques sueltos: cada bloque toca a otro (también en diagonal).
  5. Debe usar al menos 6 filas y 6 columnas (que se lea a distancia).
  6. Pictograma de lo que HACE el programa, nunca su logo o marca registrada (eso es de su dueño).
  7. Debajo del ícono va el nombre en letras de bloques y una sigla de 2 letras.
  8. Ningún ícono se repite: cada programa tiene el suyo, para que se reconozca de un vistazo. Si el de la
     categoría ya está ocupado y no trae uno propio, se le hace un monograma con su sigla.
"""
from __future__ import annotations

import json
import shlex
import shutil
import subprocess
from pathlib import Path

LADO = 9
REGLA = __doc__[__doc__.index("  1. "):].strip()

# pictogramas por categoría (todos cumplen la regla: lo verifica la prueba)
CATEGORIAS: dict[str, str] = {
    "nube": ".........|...###...|..#...##.|.#......#|#.......#|#.......#|.#######.|.........|.........",
    "video": "#########|#.......#|#..#....#|#..##...#|#..###..#|#..##...#|#..#....#|#.......#|#########",
    "juego": ".........|.........|.#######.|##.###.##|#...#.#.#|##.###.##|#########|##.....##|.........",
    "musica": "...######|...#....#|...#....#|...#....#|...#....#|.###..###|####.####|.##...##.|.........",
    "navegador": "..#####..|.#..#..#.|#..###..#|#########|#..###..#|#########|#..###..#|.#..#..#.|..#####..",
    "terminal": "#########|#########|#.......#|#.#.....#|#..#....#|#.#..##.#|#.......#|#########|.........",
    "oficina": ".#####...|.#...##..|.#....##.|.#.###.#.|.#.....#.|.#.####.#|.#......#|.########|.........",
    "ia": "....#....|...###...|..#####..|#########|.#######.|..#####..|..##.##..|.##...##.|##.....##",
    "chat": ".........|#########|#.......#|#.##.##.#|#.......#|#########|..##.....|.#.......|.........",
    "foto": ".........|..###....|#########|#...#...#|#..###..#|#...#...#|#.......#|#########|.........",
    "cad": "....#....|..##.##..|##.....##|#.##.##.#|#...#...#|#...#...#|#...#...#|.##.#.##.|...###...",
    "robot": "....#....|..#####..|.#.....#.|.#.#.#.#.|.#.#.#.#.|..#####..|#########|..#...#..|.##...##.",
    "red": "#.#.#.#.#|.#.#.#.#.|#.#...#.#|.#.....#.|#.#...#.#|.#.#.#.#.|#.#.#.#.#|.........|.........",
    "correo": ".........|#########|##.....##|#.#...#.#|#..#.#..#|#...#...#|#.......#|#########|.........",
    "calendario": ".#.....#.|#########|#.......#|#.##.##.#|#.......#|#.##.##.#|#.......#|#########|.........",
    "seguridad": ".........|...###...|..#...#..|..#...#..|.#######.|.###.###.|.###.###.|.#######.|.........",
    "taller": "...#.#...|.#######.|.##...##.|###.#.###|.#.###.#.|###.#.###|.##...##.|.#######.|...#.#...",
    "cuaderno": ".........|.###.###.|#...#...#|#.##.##.#|#...#...#|#.##.##.#|#...#...#|.###.###.|.........",
    "app": "#########|#.......#|#.##.##.#|#.##.##.#|#.......#|#.##.##.#|#.##.##.#|#.......#|#########",
}
SINONIMOS = {
    "series": "video", "peliculas": "video", "tv": "video", "streaming": "video", "medios": "video",
    "juegos": "juego", "gaming": "juego", "emulador": "juego", "audio": "musica", "podcast": "musica",
    "web": "navegador", "internet": "navegador", "codigo": "terminal", "programacion": "terminal", "consola": "terminal",
    "documentos": "oficina", "texto": "oficina", "planilla": "oficina", "pdf": "oficina", "notas": "cuaderno",
    "estudio": "cuaderno", "memoria": "cuaderno", "investigacion": "cuaderno", "mensajes": "chat", "video-llamada": "chat",
    "camara": "foto", "imagenes": "foto", "diseno": "cad", "3d": "cad", "electronica": "taller", "cnc": "taller",
    "impresion3d": "taller", "robotica": "robot", "remoto": "red", "vpn": "red", "respaldo": "nube", "almacenamiento": "nube",
    "claves": "seguridad", "contrasenas": "seguridad", "agenda": "calendario",
}

# ícono propio de cada programa conocido: NINGUNO se repite (ni entre sí ni con las categorías)
PROPIOS: dict[str, str] = {
    "nube": "...###...|..#...#..|.#.....##|#.......#|.#######.|....#....|...###...|..#.#.#..|....#....",   # nube con flecha de subir
    "jellyfin": "#.#.#.#.#|#########|#.......#|#..#....#|#..##...#|#..###..#|#..##...#|#..#....#|#########",   # pantalla de series con play
    "kodi": ".#.....#.|..#...#..|...#.#...|#########|#.......#|#.#####.#|#.......#|#########|.##...##.",   # tele con antena
    "steam": "..#####..|.#.....#.|#.......#|#..###..#|#..#.#..#|#..###..#|#.......#|.#.....#.|..#####..",   # disco: biblioteca de juegos
    "geforcenow": "...###...|..#...##.|.#......#|#..#....#|#..##...#|#..###..#|#..##...#|.###.###.|.........",   # nube con play: juegos en la nube
    "heroic": ".#######.|#.......#|#...#...#|#...#...#|#.#####.#|.#..#..#.|..#.#.#..|...#.#...|....#....",   # escudo con espada
    "retroarch": "...###...|...###...|....#....|....#....|#########|#.......#|#.##.##.#|#########|.........",   # palanca de máquina arcade
    "chrome": "..#####..|.##...##.|#..#.#..#|#...#...#|#########|#...#...#|#..#.#..#|.##...##.|..#####..",   # globo terráqueo
    "firefox": "#########|#.#.#...#|#########|#.......#|#..###..#|#.#.#.#.#|#..###..#|#.......#|#########",   # ventana con globo
    "libreoffice": "#########|#...#...#|#########|#...#...#|#########|#...#...#|#########|.........|.........",   # planilla y documentos
    "obsidian": "....#....|...###...|..#.#.#..|.#..#..#.|#...#...#|.#..#..#.|..#.#.#..|...###...|....#....",   # cristal de notas
    "logseq": "##.######|#........|#.##.####|#.#......|#.#.##.##|#........|##.######|.........|.........",   # esquema con viñetas
    "gemini-notebook": "....#....|...###...|....#....|.........|#########|#.......#|#.#####.#|#.......#|#########",   # cuaderno con chispa
    "vscode": ".........|.....#...|..#..##..|.#..#..#.|#...#...#|.#..#..#.|..##..#..|...#.....|.........",   # código </>
    "freecad": "..######.|.#....##.|########.|#.....#.#|#.....#.#|#.....#.#|#.....##.|#######..|.........",   # pieza en 3D
    "kicad": ".#.#.#.#.|#########|.#.....#.|##.###.##|.#.#.#.#.|##.###.##|.#.....#.|#########|.#.#.#.#.",   # chip de placa
    "prusaslicer": "#########|#.......#|#..###..#|#...#...#|#.......#|#..###..#|#.#####.#|#########|.........",   # impresora 3D
    "ollama": "..#####..|.#.....#.|#.##.##.#|#.......#|#..###..#|.#.....#.|..#.#.#..|..#.#.#..|..#####..",   # cara de IA local
    "rustdesk": "#####....|#...#....|#####.#..|..#...##.|######.##|....#####|....#...#|....#####|.........",   # dos pantallas: control remoto
}
SIGNIFICADO = {"nube": "nube con flecha de subir", "jellyfin": "pantalla de series con play", "kodi": "tele con antena", "steam": "disco: biblioteca de juegos", "geforcenow": "nube con play: juegos en la nube", "heroic": "escudo con espada", "retroarch": "palanca de máquina arcade", "chrome": "globo terráqueo", "firefox": "ventana con globo", "libreoffice": "planilla y documentos", "obsidian": "cristal de notas", "logseq": "esquema con viñetas", "gemini-notebook": "cuaderno con chispa", "vscode": "código </>", "freecad": "pieza en 3D", "kicad": "chip de placa", "prusaslicer": "impresora 3D", "ollama": "cara de IA local", "rustdesk": "dos pantallas: control remoto"}

# programas que Neo Orión ya conoce (se muestran solo si están instalados)
CONOCIDAS = [
    {"id": "nube", "nombre": "Mi nube", "categoria": "nube", "comando": "xdg-open {casa}/Nube", "cuando": "rclone"},
    {"id": "jellyfin", "nombre": "Series", "categoria": "video", "web": "http://127.0.0.1:8096", "cuando": "jellyfin"},
    {"id": "kodi", "nombre": "Kodi", "categoria": "video", "comando": "kodi"},
    {"id": "steam", "nombre": "Steam", "categoria": "juego", "comando": "steam"},
    {"id": "geforcenow", "nombre": "GeForce Now", "categoria": "juego", "comando": "flatpak run com.nvidia.geforcenow", "flatpak": "com.nvidia.geforcenow"},
    {"id": "heroic", "nombre": "Heroic", "categoria": "juego", "comando": "flatpak run com.heroicgameslauncher.hgl", "flatpak": "com.heroicgameslauncher.hgl"},
    {"id": "retroarch", "nombre": "RetroArch", "categoria": "juego", "comando": "retroarch"},
    {"id": "chrome", "nombre": "Chrome", "categoria": "navegador", "comando": "google-chrome-stable"},
    {"id": "firefox", "nombre": "Firefox", "categoria": "navegador", "comando": "firefox"},
    {"id": "libreoffice", "nombre": "Oficina", "categoria": "oficina", "comando": "libreoffice"},
    {"id": "obsidian", "nombre": "Obsidian", "categoria": "cuaderno", "comando": "obsidian"},
    {"id": "logseq", "nombre": "Logseq", "categoria": "cuaderno", "comando": "logseq"},
    {"id": "gemini-notebook", "nombre": "Gemini Notebook", "categoria": "cuaderno", "web": "https://notebooklm.google.com"},
    {"id": "vscode", "nombre": "VS Code", "categoria": "terminal", "comando": "code"},
    {"id": "freecad", "nombre": "FreeCAD", "categoria": "cad", "comando": "freecad"},
    {"id": "kicad", "nombre": "KiCad", "categoria": "taller", "comando": "kicad"},
    {"id": "prusaslicer", "nombre": "PrusaSlicer", "categoria": "taller", "comando": "prusa-slicer"},
    {"id": "ollama", "nombre": "IA local", "categoria": "ia", "web": "http://127.0.0.1:11434", "cuando": "ollama"},
    {"id": "rustdesk", "nombre": "RustDesk", "categoria": "red", "comando": "rustdesk"},
]


def _carpeta() -> Path:
    return Path.home() / ".orion"


def norm_cat(c: str) -> str:
    import unicodedata
    c = "".join(ch for ch in unicodedata.normalize("NFD", (c or "").lower().strip()) if unicodedata.category(ch) != "Mn")
    return c if c in CATEGORIAS else SINONIMOS.get(c, "app")


def validar(icono: str) -> list[str]:
    """Devuelve la lista de reglas que NO cumple (vacía = válido)."""
    filas = icono.split("|") if isinstance(icono, str) else []
    if len(filas) != LADO or any(len(f) != LADO for f in filas):
        return [f"1 · debe ser de {LADO}×{LADO} (filas de {LADO} caracteres separadas por «|»)"]
    if any(ch not in "#." for f in filas for ch in f):
        return ["1 · solo «#» (bloque) y «.» (vacío)"]
    errores = []
    llenos = {(y, x) for y, f in enumerate(filas) for x, ch in enumerate(f) if ch == "#"}
    p = len(llenos) / LADO ** 2
    if not .20 <= p <= .65:
        errores.append(f"3 · lleno {p:.0%}: debe estar entre 20 % y 65 %")
    sueltos = [b for b in llenos if not any((b[0] + dy, b[1] + dx) in llenos
                                             for dy in (-1, 0, 1) for dx in (-1, 0, 1) if dy or dx)]
    if sueltos:
        errores.append(f"4 · {len(sueltos)} bloque(s) suelto(s), p. ej. fila {sueltos[0][0] + 1} columna {sueltos[0][1] + 1}")
    if len({y for y, _ in llenos}) < 6 or len({x for _, x in llenos}) < 6:
        errores.append("5 · debe ocupar al menos 6 filas y 6 columnas")
    return errores


LETRAS = {  # 4×5, para monogramas
    "A": ".##.|#..#|####|#..#|#..#", "B": "###.|#..#|###.|#..#|###.", "C": ".###|#...|#...|#...|.###", "D": "###.|#..#|#..#|#..#|###.",
    "E": "####|#...|###.|#...|####", "F": "####|#...|###.|#...|#...", "G": ".###|#...|#.##|#..#|.###", "H": "#..#|#..#|####|#..#|#..#",
    "I": "###.|.#..|.#..|.#..|###.", "J": "..##|...#|...#|#..#|.##.", "K": "#..#|#.#.|##..|#.#.|#..#", "L": "#...|#...|#...|#...|####",
    "M": "#..#|####|####|#..#|#..#", "N": "#..#|##.#|#.##|#..#|#..#", "O": ".##.|#..#|#..#|#..#|.##.", "P": "###.|#..#|###.|#...|#...",
    "Q": ".##.|#..#|#..#|#.#.|.#.#", "R": "###.|#..#|###.|#.#.|#..#", "S": ".###|#...|.##.|...#|###.", "T": "####|.#..|.#..|.#..|.#..",
    "U": "#..#|#..#|#..#|#..#|.##.", "V": "#..#|#..#|#..#|.##.|.##.", "W": "#..#|#..#|####|####|#..#", "X": "#..#|.##.|.##.|.##.|#..#",
    "Y": "#..#|#..#|.##.|.#..|.#..", "Z": "####|..#.|.#..|#...|####", "0": ".##.|#.##|##.#|#..#|.##.", "1": ".#..|##..|.#..|.#..|###.",
    "2": "###.|...#|.##.|#...|####", "3": "###.|...#|.##.|...#|###.", "4": "#..#|#..#|####|...#|...#", "5": "####|#...|###.|...#|###.",
    "6": ".##.|#...|###.|#..#|.##.", "7": "####|...#|..#.|.#..|.#..", "8": ".##.|#..#|.##.|#..#|.##.", "9": ".##.|#..#|.###|...#|.##."}


def monograma(sg: str) -> str:
    """Ícono de respaldo: la sigla en bloques entre dos barras (cumple la regla)."""
    a, b = (LETRAS.get(c, LETRAS["A"]).split("|") for c in (sg + "AP")[:2])
    medio = [a[i] + "." + b[i] for i in range(5)]
    return "|".join(["#########", "........."] + medio + [".........", "#########"])


def usados(excepto: str = "") -> set[str]:
    return set(PROPIOS.values()) | {a["icono"] for a in _propias() if a.get("icono") and a.get("id") != excepto}


def sigla(nombre: str) -> str:
    p = [w for w in "".join(ch if ch.isalnum() else " " for ch in nombre).split() if w]
    return ((p[0][0] + p[1][0]) if len(p) > 1 else (p[0][:2] if p else "AP")).upper()


def _instalada(a: dict) -> bool:
    if a.get("flatpak"):
        try:
            return subprocess.run(["flatpak", "info", a["flatpak"]], capture_output=True, timeout=5).returncode == 0
        except (OSError, subprocess.SubprocessError):
            return False
    clave = a.get("cuando") or (shlex.split(a["comando"])[0] if a.get("comando") else "")
    if not clave:
        return bool(a.get("web"))
    return shutil.which(clave) is not None


def _propias() -> list[dict]:
    try:
        return json.loads((_carpeta() / "apps.json").read_text())
    except (OSError, ValueError):
        return []


def registrar(id: str, nombre: str, categoria: str, comando: str = "", web: str = "", icono: str = "") -> dict:
    """Agrega (o actualiza) un programa en el HUD. Si trae ícono, debe cumplir la REGLA; si no, se usa el
    pictograma de su categoría. Devuelve el registro o los errores de la regla."""
    import re
    if not re.fullmatch(r"[a-z0-9][a-z0-9._-]{0,40}", id or ""):
        return {"ok": False, "error": "id: minúsculas, números, punto, guion (máx. 41)"}
    if not comando and not web:
        return {"ok": False, "error": "falta cómo abrirlo: comando o web"}
    if web and not re.match(r"^https?://", web):
        return {"ok": False, "error": "la web debe empezar con http:// o https://"}
    cat = norm_cat(categoria)
    ocupados = usados(excepto=id)
    if icono:
        mal = validar(icono)
        if mal:
            return {"ok": False, "error": "el ícono no cumple la regla estética", "reglas": mal, "regla": REGLA}
        if icono in ocupados:
            return {"ok": False, "error": "8 · ese ícono ya lo usa otro programa: dibuja uno distinto", "regla": REGLA}
    else:                                       # sin ícono propio: el de la categoría si está libre; si no, monograma
        icono = CATEGORIAS[cat] if CATEGORIAS[cat] not in ocupados else monograma(sigla(nombre or id))
        n = 0
        while icono in ocupados and n < 40:     # dos apps con la misma sigla: se corre la barra de abajo
            n += 1
            f = icono.split("|"); f[8] = "#" * (9 - n % 9) + "." * (n % 9); icono = "|".join(f)
    reg = {"id": id, "nombre": nombre.strip()[:24] or id, "categoria": cat, "sigla": sigla(nombre or id)}
    if comando:
        reg["comando"] = comando
    if web:
        reg["web"] = web
    reg["icono"] = icono
    lista = [a for a in _propias() if a.get("id") != id] + [reg]
    _carpeta().mkdir(parents=True, exist_ok=True)
    (_carpeta() / "apps.json").write_text(json.dumps(lista, ensure_ascii=False, indent=2) + "\n")
    return {"ok": True, **reg}


def quitar(id: str) -> dict:
    lista = _propias()
    nueva = [a for a in lista if a.get("id") != id]
    (_carpeta() / "apps.json").write_text(json.dumps(nueva, ensure_ascii=False, indent=2) + "\n")
    return {"ok": len(nueva) < len(lista)}


def listar(todas: bool = False) -> list[dict]:
    """Apps para el HUD: las conocidas que estén instaladas + las registradas por la persona o la IA."""
    propias = {a["id"]: a for a in _propias() if isinstance(a, dict) and a.get("id")}
    salida = []
    for a in CONOCIDAS:
        if a["id"] in propias:
            continue
        if todas or _instalada(a):
            salida.append({**a, "sigla": sigla(a["nombre"]), "icono": PROPIOS.get(a["id"], CATEGORIAS[a["categoria"]]),
                           "instalada": _instalada(a) if todas else True})
    for a in propias.values():
        ic = a.get("icono") if a.get("icono") and not validar(a["icono"]) else CATEGORIAS[norm_cat(a.get("categoria", ""))]
        salida.append({**a, "icono": ic, "sigla": a.get("sigla") or sigla(a.get("nombre", a["id"])), "instalada": True})
    return salida


def abrir(id: str, ejecutar=subprocess.Popen) -> dict:
    """Abre una app de la lista (solo de la lista: nada arbitrario)."""
    a = next((x for x in listar(todas=True) if x["id"] == id), None)
    if not a:
        return {"ok": False, "error": f"no conozco la app «{id}»"}
    if a.get("web") and not a.get("comando"):
        cmd = ["xdg-open", a["web"]]
    else:
        cmd = shlex.split(a["comando"].replace("{casa}", str(Path.home())))
        if a["id"] == "nube":
            (Path.home() / "Nube").mkdir(exist_ok=True)
    try:
        ejecutar(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
    except OSError as e:
        return {"ok": False, "error": f"no se pudo abrir {a['nombre']}: {e}"}
    return {"ok": True, "abierta": a["nombre"]}
