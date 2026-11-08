"""orion-linux — servidor MCP que le da a la IA el control de todo el sistema.

Cualquier IA compatible con MCP (Claude, o un modelo local vía un cliente MCP)
se conecta aquí y trabaja "por detrás": sin mover el mouse ni abrir ventanas.
El HUD solo muestra resultados cuando la IA llama a `hud_mostrar`.

Arranque:  python3 -m orion_mcp.server            (transporte stdio)
"""
from __future__ import annotations
import datetime as dt, json, os, re, shutil, subprocess, sys, textwrap, time
from pathlib import Path

try:                                   # SDK MCP 2.x
    from mcp.server.mcpserver import MCPServer as FastMCP
except ImportError:                    # SDK MCP 1.x
    from mcp.server.fastmcp import FastMCP

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from orion_core import detect                        # noqa: E402
from orion_core.policy import Policy, Risk            # noqa: E402

HOME = Path(os.environ.get("ORION_HOME", os.path.expanduser("~/.orion")))
WORK = Path(os.environ.get("ORION_WORKSPACE", os.path.expanduser("~/orion")))
REPO = Path(__file__).resolve().parents[2]
for d in (HOME / "hud" / "inbox", WORK / "cad", WORK / "repos", WORK / "forma"):
    d.mkdir(parents=True, exist_ok=True)


def _identity() -> dict:
    p = HOME / "identidad.json"
    if p.exists():
        return json.loads(p.read_text())
    return {"nombre": "Neo Orión", "tono": "directo", "genero": "neutro", "color": "#cdf564", "permisos": "libre"}


policy = Policy(_identity().get("permisos", "libre"), str(WORK), str(HOME))
mcp = FastMCP("orion-linux", instructions=textwrap.dedent(f"""
    Eres {_identity()['nombre']}, el sistema operativo por voz Neo Orión. Trabajas por
    detrás: ejecutas, instalas, clonas y programas sin pedir permiso para tareas de
    software. Antes de una tarea, consulta memoria_contexto; al terminar, deja lo importante con
    memoria_anotar. Antes de construir algo, busca con repos_buscar si ya existe.
    Cuando instales un programa que la persona va a abrir, regístralo con apps(accion='registrar'): su ícono
    sigue la REGLA ESTÉTICA (9×9 bloques, un color, pictograma de lo que hace, nunca el logo de la marca).
    Si la persona no tiene micrófono, ofrece usar su teléfono con telefono_microfono.
    REGLA DE CÓDIGO (obligatoria): todo programa o módulo nuevo de Neo Orión se escribe en Rust (nucleo/).
    REGLA DE ESTÉTICA (obligatoria): todo lo que agregues a la interfaz usa marathon.js y conserva las
    animaciones del sistema (decodificar títulos, bloques que llegan, falla RGB, disolución, barrido) y la
    paleta de la persona. Si algo nuevo no calza, busca la forma de que quede bien antes de mostrarlo.
    No muestres pasos intermedios; cuando haya un resultado que la persona
    deba ver o manipular, llama a hud_mostrar. Si una herramienta devuelve un ticket
    "en espera", díselo en una frase y continúa con otra cosa: la persona lo aprueba.
"""))


def _gate(action: str, detail: dict, risk: Risk, why: str = "") -> dict | None:
    if policy.needs_human(risk):
        t = policy.request(action, detail, risk, why)
        return {"estado": "en-espera", "ticket": t, "motivo": why or risk.name,
                "mensaje": "Requiere confirmación de la persona (voz o HUD)."}
    return None


# ------------------------------------------------------------------ sistema
@mcp.tool()
def sistema_info() -> dict:
    """Qué equipo es (torre, notebook, Raspberry, Jetson, Android…), qué tiene
    conectado (ESP32, cámaras, teléfonos, micrófonos) y qué IA local cabe."""
    info = detect.scan()
    fit = REPO / "overlay/usr/local/bin/orion-fit"
    out = subprocess.run([sys.executable, str(fit), "--json"], capture_output=True, text=True)
    try:
        info["ia_local"] = json.loads(out.stdout)["recommendation"]
    except Exception:
        info["ia_local"] = None
    return info


@mcp.tool()
def ejecutar(comando: str, carpeta: str = "", segundos: int = 300) -> dict:
    """Ejecuta un comando de shell. Software: sin preguntar. Irreversible: espera confirmación."""
    risk, why = policy.classify_shell(comando)
    if (g := _gate("ejecutar", {"comando": comando}, risk, why)):
        return g
    cwd = Path(os.path.expanduser(carpeta)) if carpeta else WORK
    t0 = time.time()
    p = subprocess.run(comando, shell=True, cwd=cwd, capture_output=True, text=True, timeout=segundos)
    policy.record("ejecutar", {"comando": comando}, risk, f"código {p.returncode}")
    return {"codigo": p.returncode, "salida": p.stdout[-8000:], "errores": p.stderr[-4000:],
            "segundos": round(time.time() - t0, 1)}


@mcp.tool()
def leer_archivo(ruta: str, max_bytes: int = 200_000) -> str:
    """Lee un archivo de texto."""
    return Path(os.path.expanduser(ruta)).read_text(errors="replace")[:max_bytes]


@mcp.tool()
def escribir_archivo(ruta: str, contenido: str) -> dict:
    """Crea o reemplaza un archivo (guarda copia .bak si ya existía)."""
    p = Path(os.path.expanduser(ruta))
    risk = policy.classify_path_write(str(p))
    if (g := _gate("escribir_archivo", {"ruta": str(p)}, risk)):
        return g
    p.parent.mkdir(parents=True, exist_ok=True)
    if p.exists():
        shutil.copy2(p, p.with_suffix(p.suffix + ".bak"))
    p.write_text(contenido)
    policy.record("escribir_archivo", {"ruta": str(p)}, risk, "ok", bytes=len(contenido))
    return {"ok": True, "ruta": str(p), "bytes": len(contenido)}


@mcp.tool()
def clonar_repo(url: str, nombre: str = "") -> dict:
    """Clona (o actualiza si ya existe) un repositorio git en el espacio de trabajo.
    Ej.: 'conéctate a mi-proyecto' → clonar_repo('github.com/<usuario>/mi-proyecto')."""
    if not re.match(r"^(https?://|git@)", url):
        url = "https://" + url.lstrip("/")
    dest = WORK / "repos" / (nombre or url.rstrip("/").split("/")[-1].removesuffix(".git"))
    cmd = ["git", "-C", str(dest), "pull", "--ff-only"] if dest.exists() else ["git", "clone", "--depth", "50", url, str(dest)]
    p = subprocess.run(cmd, capture_output=True, text=True, timeout=600)
    policy.record("clonar_repo", {"url": url}, Risk.SISTEMA, f"código {p.returncode}")
    files = sum(1 for _ in dest.rglob("*") if _.is_file()) if dest.exists() else 0
    return {"ok": p.returncode == 0, "ruta": str(dest), "archivos": files, "log": (p.stdout + p.stderr)[-2000:]}


@mcp.tool()
def instalar(paquetes: list[str], gestor: str = "apt") -> dict:
    """Instala software: gestor = apt | pip | flatpak | npm | ollama (modelos)."""
    safe = [x for x in paquetes if re.fullmatch(r"[\w.+:@/=-]+", x)]
    cmd = {"apt": f"sudo apt-get install -y {' '.join(safe)}",
           "pip": f"pip install --user {' '.join(safe)}",
           "flatpak": f"flatpak install -y flathub {' '.join(safe)}",
           "npm": f"npm install -g {' '.join(safe)}",
           "ollama": " && ".join(f"ollama pull {x}" for x in safe)}.get(gestor)
    if not cmd:
        return {"ok": False, "error": f"gestor desconocido: {gestor}"}
    return ejecutar(cmd, segundos=3600)



@mcp.tool()
def jugar(pedido: str, hacerlo: bool = False) -> dict:
    """Juegos y medios: 'quiero jugar X', 'GeForce NOW', 'pon Netflix', 'mis series', 'emulador'.
    Decide local / nube / desde otro PC según el equipo, detecta teclado, mouse y mandos,
    e instala lo que falte. Con hacerlo=True ejecuta los pasos (pasan por los permisos de siempre).
    No integra fuentes de descarga de juegos crackeados ni IPTV sin licencia."""
    from orion_core import perfil as pf
    from orion_juegos import plan
    try:
        eq = pf.perfil()
    except Exception:
        eq = {}
    pl = plan(pedido, {"gpu": eq.get("gpu"), "vram_gb": eq.get("vram_gb", 0)})
    if hacerlo:
        pl["resultado"] = []
        for paso in pl["pasos"]:
            r = ejecutar(paso, segundos=3600)
            pl["resultado"].append(r)
            if not r.get("ok", False):
                break
    return pl


@mcp.tool()
def rendimiento(tok_s: float | None = None) -> dict:
    """¿El equipo está rindiendo? Mide memoria, CPU, temperatura, batería y (si se pasa) la velocidad
    de la IA. Devuelve NORMAL / LIVIANO / PATATA con las razones y qué conviene apagar o mover a la nube."""
    from orion_core import rendimiento as r
    m = r.medir({"tok_s": tok_s} if tok_s else None)
    n, razones = r.nivel_de(m)
    return {"nivel": n, "razones": razones, "acciones": r.acciones(n), "medicion": m}


@mcp.tool()
def colaborar(tarea: str, ia: str = "", modo: str = "auto") -> dict:
    """Pásale trabajo a otra IA con el contexto de la memoria: 'usa Claude para crear el documento…'.
    ia: claude | gemini | codex | opencode | local (vacío = la que se nombre en la tarea, si no Claude).
    modo: auto (por su app/CLI oficial con la cuenta de la persona) | web (abre su chat con el encargo
    copiado; la persona aprieta enviar). La respuesta queda en la memoria (colaboraciones/)."""
    import orion_puente as pu
    from orion_memoria import Memoria
    mem = Memoria()
    via = ia or pu.via_de(tarea)
    if modo == "web":
        return pu.abrir_web(tarea, via, mem)
    return pu.delegar(tarea, via, mem)


@mcp.tool()
def controladores(accion: str = "ver") -> dict:
    """Procesador y gráfica(s) del equipo y sus drivers. accion: ver (qué hay, qué driver usa y el plan) |
    al_dia (¿Ubuntu recomienda un driver NVIDIA más nuevo?) | instalar (aplica el plan: microcódigo, Mesa/VA-API,
    driver NVIDIA recomendado; SISTEMA) | firmware (actualizaciones de BIOS/firmware por fwupd: solo informa,
    instalarlas es IRREVERSIBLE y lo aprueba la persona)."""
    from orion_core import controladores as ctl
    if accion == "al_dia":
        return ctl.revisar_al_dia()
    if accion == "firmware":
        return ctl.firmware()
    d = ctl.detectar()
    if accion == "instalar":
        g = _gate("controladores", {"paquetes": d["plan"]["paquetes"], "pasos": d["plan"]["pasos"]}, Risk.SISTEMA, "instala drivers")
        if g:
            return g
        r = subprocess.run(["sudo", "apt-get", "install", "-y", *d["plan"]["paquetes"]], capture_output=True, text=True) if d["plan"]["paquetes"] else None
        if any(x["fabricante"] == "NVIDIA" for x in d["gpus"]):
            subprocess.run(["sudo", "ubuntu-drivers", "install"], capture_output=True, text=True)
        return {"ok": r is None or r.returncode == 0, "plan": d["plan"], "reiniciar": any(x["fabricante"] == "NVIDIA" for x in d["gpus"])}
    return d


@mcp.tool()
def electrica(pregunta: str = "", corriente_a: float = 0, largo_m: float = 0, tension_v: float = 220, fases: int = 1,
              tipo: str = "subalimentador", material: str = "cobre", iz_a: float | None = None, caida_previa_pct: float = 0) -> dict:
    """Calculadora eléctrica chilena (RIC N°03): sección por caída de tensión (≤3 % alimentadores/subalimentadores,
    ≤5 % total), mínimos de 4 y 2,5 mm², protección (In ≥ Ib; In ≤ Iz si se entrega Iz de la tabla RIC N°04).
    pregunta en palabras («4500 W a 40 metros en 220») o los valores sueltos."""
    import orion_electrica as el
    r = el.desde_frase(pregunta) if pregunta else el.seccion(corriente_a, largo_m, tension_v, fases, material, tipo,
                                                             iz_a=iz_a, caida_previa_pct=caida_previa_pct)
    if r is None:
        return {"ok": False, "error": "di la corriente (A) o la potencia (W) y el largo en metros"}
    from dataclasses import asdict
    return {"ok": True, **asdict(r), "resumen": r.texto()}


@mcp.tool()
def respaldo(accion: str = "ahora", destino: str = "", sello: str = "", carpeta: str = "Orion") -> dict:
    """Respaldo de la memoria, proyectos y configuración. accion: ahora | listar | discos | configurar (destino =
    auto | /ruta | rclone:remoto:carpeta) | restaurar (sello, carpeta → ~/Restaurado/, no pisa lo actual)."""
    import orion_respaldo as rs
    if accion == "listar":
        return {"ok": True, "respaldos": rs.listar()}
    if accion == "discos":
        return {"ok": True, "discos": rs.discos_externos()}
    if accion == "configurar":
        c = rs.configuracion(); c["destino"] = destino or "auto"; rs.guardar_configuracion(c)
        return {"ok": True, "configuracion": c}
    if accion == "restaurar":
        return rs.restaurar(sello, carpeta)
    return rs.respaldar()


@mcp.tool()
def remoto(accion: str = "estado") -> dict:
    """Escritorio remoto. accion: estado | casa (VNC en la red local con clave y TLS) | afuera (ID de RustDesk) | apagar.
    La clave queda guardada en ~/.config/wayvnc; no se lee en voz alta."""
    import orion_remoto as rm
    if accion == "casa":
        r = rm.encender_vnc(); r.pop("clave", None); return r
    if accion == "afuera":
        return rm.rustdesk_id()
    if accion == "apagar":
        return rm.apagar()
    return rm.estado()


@mcp.tool()
def cnc(accion: str, archivo: str = "", x: float = 0, y: float = 0, z: float = 0, puerto: str = "/dev/ttyUSB0") -> dict:
    """Fresadora CNC (GRBL). accion: revisar (archivo .nc/.gcode: medidas, tiempo, profundidad, si cabe; sin mover nada) |
    estado | casa | desbloquear | cero (pieza) | mover (x,y,z en mm) | iniciar (archivo) | pausa | seguir | parar.
    mover, casa e iniciar son FÍSICAS: piden confirmación humana. parar y pausa no."""
    import orion_cnc as cn
    if accion == "revisar":
        rev = cn.revisar(Path(archivo).expanduser().read_text())
        png = cn.vista(rev, str(Path.home() / ".orion/cnc" / (Path(archivo).stem + "_recorrido.png")))
        return {"ok": True, **rev.resumen(), "vista": png}
    if accion in {"mover", "casa", "iniciar", "cero"}:      # FÍSICAS: la persona aprueba el ticket y la interfaz ejecuta
        if accion == "iniciar":
            rev = cn.revisar(Path(archivo).expanduser().read_text())
            if rev.errores:
                return {"ok": False, "errores": rev.errores}
        return _gate("cnc", {"accion": accion, "archivo": archivo, "x": x, "y": y, "z": z, "puerto": puerto},
                     Risk.FISICO, f"la CNC va a {accion}") or {"ok": False}
    g = cn.Grbl(cn.Maquina(puerto=puerto))
    if accion == "estado":
        return {"ok": True, **g.estado()}
    if accion == "desbloquear":
        return {"ok": True, "respuesta": g.desbloquear()}
    if accion == "casa":
        return {"ok": True, "respuesta": g.casa()}
    if accion == "cero":
        return {"ok": True, "respuesta": g.cero_pieza()}
    if accion == "mover":
        return {"ok": True, "respuesta": g.mover(x, y, z)}
    if accion == "pausa":
        g.pausa(); return {"ok": True}
    if accion == "seguir":
        g.seguir(); return {"ok": True}
    if accion == "parar":
        g.parar(); return {"ok": True, "mensaje": "parada inmediata; desbloquea con $X cuando sea seguro"}
    if accion == "iniciar":
        gcode = Path(archivo).expanduser().read_text()
        rev = cn.revisar(gcode)
        if rev.errores:
            return {"ok": False, "errores": rev.errores}
        return g.trabajo(gcode)
    return {"ok": False, "error": "accion desconocida"}


@mcp.tool()
def camaras(accion: str = "preparar", camaras: list[dict] | None = None, detector: str = "cpu") -> dict:
    """Cámaras con Frigate (detección de personas, grabación local). accion: preparar (camaras = [{nombre, fuente:
    rtsp|usb, dispositivo, zonas}] → config y docker-compose; las claves RTSP van en ~/.orion/claves/camaras.json).
    Una persona en una zona 'robot', 'cnc' o 'taller' frena las máquinas como el LiDAR."""
    import orion_vision as vi
    if accion == "preparar":
        return vi.preparar(camaras or [{"nombre": "Taller", "fuente": "usb"}], detector)
    return {"ok": False, "error": "accion: preparar"}


@mcp.tool()
def casa(orden: str) -> dict:
    """Casa inteligente con Home Assistant: «apaga la luz del taller», «qué temperatura hay en la pieza».
    Cerraduras, alarmas y portones piden confirmación. El token lo pega la persona en ~/.orion/claves/home-assistant."""
    import orion_casa as ca
    try:
        r = ca.Casa().ordenar(orden)
        if r.get("necesita_confirmar"):     # cerraduras, alarmas, portones: ticket para la persona
            return _gate("casa", {"orden": orden, "entidad": r["entidad"], "servicio": r["servicio"]}, Risk.FISICO, r["motivo"]) or r
        return r
    except PermissionError as e:
        return {"ok": False, "error": str(e), "instalar": ca.INSTALAR}
    except OSError as e:
        return {"ok": False, "error": f"Home Assistant no responde ({e})"}


@mcp.tool()
def dictado(texto: str, plantilla: str = "informe-duoc", nombre: str = "", asignatura: str = "", docente: str = "",
            integrantes: list[str] | None = None, pulir: bool = True) -> dict:
    """Dictado → documento Word con formato. plantilla: informe-duoc (logo DUOC arriba a la derecha) |
    memoria-electrica | carta | acta. Marcas al dictar: «título», «sección», «subtítulo», «punto», «punto aparte», «tabla»."""
    import orion_dictado as dc
    t = dc.pulir_con_ia(texto) if pulir else texto
    b = dc.ordenar(t)
    titulo = next((x for k, x in b if k == "titulo"), nombre or "documento")
    ruta = Path.home() / "Documentos" / (re.sub(r"[^\w-]+", "_", nombre or titulo)[:60] + ".docx")
    dc.documento(b, str(ruta), plantilla, {"asignatura": asignatura, "docente": docente, "integrantes": integrantes or []})
    return {"ok": True, "archivo": str(ruta), "bloques": len(b)}


@mcp.tool()
def actualizar(que: str = "revisar") -> dict:
    """Actualizaciones. que: revisar (¿hay una versión nueva de Neo Orión?) | neo-orion (baja, verifica SHA-256 e instala) |
    sistema (apt update + full-upgrade)."""
    import orion_actualizar as ac
    if que == "neo-orion":
        return ac.actualizar_orion()
    if que == "sistema":
        return ac.actualizar_sistema()
    return ac.revisar()


@mcp.tool()
def apps(accion: str = "listar", id: str = "", nombre: str = "", categoria: str = "", comando: str = "", web: str = "",
         icono: str = "") -> dict:
    """Programas que la persona ve en el HUD, cada uno con su ícono de bloques.
    accion: listar | registrar (después de instalar un programa) | quitar | abrir | regla.
    registrar: id (minúsculas), nombre, categoria (nube, video, juego, musica, navegador, terminal, oficina, ia,
    chat, foto, cad, robot, red, correo, calendario, seguridad, taller, cuaderno, app), comando o web, e icono
    opcional: 9 filas de 9 caracteres «#»/«.» separadas por «|». Ningún ícono se repite. Si no cumple la regla o
    ya lo usa otro programa, se rechaza con el motivo: corrígelo, o déjalo vacío (categoría libre o monograma)."""
    import orion_iconos as ic
    if accion == "registrar":
        return ic.registrar(id, nombre, categoria, comando, web, icono)
    if accion == "quitar":
        return ic.quitar(id)
    if accion == "abrir":
        return ic.abrir(id)
    if accion == "regla":
        return {"regla": ic.REGLA, "categorias": list(ic.CATEGORIAS)}
    return {"apps": ic.listar()}


@mcp.tool()
def telefono_microfono(accion: str = "iniciar") -> dict:
    """Usa el teléfono (iPhone o Android, sin instalar nada) como micrófono REAL del PC: sirve para Neo Orión,
    juegos en la nube, Discord o Meet. accion: iniciar (devuelve la dirección; el HUD muestra el QR) | estado | parar.
    Al iniciar, muestra el panel con hud_mostrar para que la persona escanee el código."""
    import orion_telefono as tf
    return tf.ordenar(accion)


@mcp.tool()
def traductor(texto: str, idioma_a: str = "es", idioma_b: str = "en", decir: bool = True) -> dict:
    """Traductor en vivo para conversar: lo que dice uno sale en el idioma del otro (voz + texto). Sin internet con
    Argos Translate; si no, con la IA local."""
    import orion_traductor as tr
    c = tr.Conversacion(idioma_a, idioma_b, hablar=None if decir else (lambda *a: None))
    return c.turno(texto)


@mcp.tool()
def cuaderno(accion: str, tema: str, texto: str = "", archivos: list[str] | None = None) -> dict:
    """Cuadernos de investigación con fuentes. Gemini Notebook (antes NotebookLM) u Open Notebook (local).
    accion: preparar (arma las fuentes del tema desde la memoria en ~/Orion/exportar/cuaderno/) |
    gemini (prepara + abre Gemini Notebook; la persona sube las fuentes: no tiene API personal y no se maneja por código) |
    recibir (texto = lo que respondió el cuaderno → memoria) | local (todo automático en Open Notebook, si está corriendo) |
    instalar_local (pasos para instalar Open Notebook con Ollama)."""
    import orion_cuaderno as cu
    from orion_memoria import Memoria
    mem = Memoria()
    extra = {"Encargo": texto} if texto and accion != "recibir" else None
    if accion == "preparar":
        return cu.preparar_fuentes(tema, mem, extra, archivos)
    if accion == "gemini":
        return cu.abrir_gemini_notebook(tema, mem, extra, archivos)
    if accion == "recibir":
        return cu.recibir(tema, texto, mem)
    if accion == "local":
        return cu.enviar_open_notebook(tema, mem, extra)
    if accion == "instalar_local":
        return {"pasos": cu.instalar_open_notebook()}
    return {"ok": False, "error": "accion: preparar | gemini | recibir | local | instalar_local"}


@mcp.tool()
def navegador(accion: str, que: str = "", texto: str = "", en: str = "chrome", pasos: list[dict] | None = None) -> dict:
    """Chrome o Firefox (en = chrome | firefox), normal y a la vista (pestañas, barra, extensiones). La persona lo usa como siempre y
    Neo Orión puede manejar ESE MISMO Chrome cuando se lo pida.
    accion: abrir (que = url, atajo 'youtube' o texto a buscar) | pestanas | nueva (que) | ir (que) |
    click (que = texto o selector) | escribir (que = selector, texto) | leer (que = selector) |
    soltar (devuelve el control a la persona) | automatizar (en segundo plano, con pasos).
    Los chats de IA no se manejan por código (sus términos lo prohíben): se abren y los usa la persona."""
    import orion_navegador as nv
    if accion == "abrir":
        return nv.abrir(que, familia=en)
    if accion == "automatizar":
        return nv.automatizar(nv.destino(que), pasos or [])
    try:
        pi = nv.piloto(en)
    except Exception as e:
        return {"ok": False, "error": f"Chrome no está abierto en modo piloto ({e}). Primero: navegador('abrir')."}
    try:
        if accion == "pestanas":
            return {"ok": True, "pestanas": pi.pestanas()}
        if accion == "nueva":
            pi.nueva(que); return {"ok": True, "pestanas": pi.pestanas()}
        if accion == "ir":
            return {"ok": True, "titulo": pi.ir(que)}
        if accion == "click":
            pi.click(que); return {"ok": True}
        if accion == "escribir":
            pi.escribir(que, texto); return {"ok": True}
        if accion == "leer":
            return {"ok": True, "texto": pi.leer(que or "body")}
        if accion == "soltar":
            return {"ok": True, "control": "persona"}
        return {"ok": False, "error": f"acción desconocida: {accion}"}
    except PermissionError as e:
        return {"ok": False, "error": str(e)}
    finally:
        pi.cerrar_conexion()

# ------------------------------------------------------------------ CAD por código
CAD_TEMPLATE = '''"""Modelo paramétrico generado por Neo Orión. Edita PARAMS o pídelo por voz."""
from build123d import *
PARAMS = {params}

{body}

if __name__ == "__main__":
    import json, sys
    out = sys.argv[1]
    export_step(pieza, out + ".step")
    export_stl(pieza, out + ".stl")
    bb = pieza.bounding_box()
    print(json.dumps({{"x": round(bb.size.X, 2), "y": round(bb.size.Y, 2), "z": round(bb.size.Z, 2),
                      "volumen_cm3": round(pieza.volume / 1000, 2)}}))
'''


@mcp.tool()
def cad_construir(nombre: str, codigo: str, params: dict) -> dict:
    """Crea una pieza 3D por código (build123d, núcleo OpenCascade, libre) y exporta
    STEP (para FreeCAD/Fusion/SolidWorks) y STL (para imprimir).
    `codigo` debe usar PARAMS[...] y dejar el resultado en la variable `pieza`."""
    src = WORK / "cad" / f"{nombre}.py"
    src.write_text(CAD_TEMPLATE.format(params=json.dumps(params, indent=4), body=codigo))
    return _cad_build(nombre)


@mcp.tool()
def cad_modificar(nombre: str, cambios: dict) -> dict:
    """Cambia parámetros de una pieza existente y la regenera.
    Ej.: 'reduce el grosor a 3 mm' → cad_modificar('soporte_servo', {'grosor': 3})."""
    src = WORK / "cad" / f"{nombre}.py"
    text = src.read_text()
    m = re.search(r"PARAMS = (\{.*?\n\})", text, re.S)
    params = json.loads(m.group(1))
    antes = dict(params)
    params.update(cambios)
    src.write_text(text.replace(m.group(1), json.dumps(params, indent=4)))
    r = _cad_build(nombre)
    r["cambios"] = {k: [antes.get(k), v] for k, v in cambios.items()}
    return r


def _cad_build(nombre: str) -> dict:
    src = WORK / "cad" / f"{nombre}.py"
    out = WORK / "cad" / nombre
    p = subprocess.run([sys.executable, str(src), str(out)], capture_output=True, text=True, timeout=300)
    if p.returncode != 0:
        return {"ok": False, "error": p.stderr[-3000:]}
    policy.record("cad", {"pieza": nombre}, Risk.ESCRIBIR, "ok")
    return {"ok": True, "medidas_mm": json.loads(p.stdout.strip().splitlines()[-1]),
            "step": f"{out}.step", "stl": f"{out}.stl", "fuente": str(src)}


# ------------------------------------------------------------------ mundo físico
_ORION = None


@mcp.tool()
def robot_orden(texto: str, perfil: str = "") -> dict:
    """Manda una orden en palabras al robot ('hombro a 45 grados, luego onda en el codo').
    Pasa por el traductor y el Guardián; la IA nunca mueve motores directo."""
    global _ORION
    from orion_motion import Orion
    if _ORION is None:
        _ORION = Orion(perfil or str(REPO / "orion/profiles/brazo-6dof.json"), memory_root=str(HOME / "robots"))
    lines = _ORION.say(texto)
    policy.record("robot", {"orden": texto}, Risk.FISICO, "; ".join(lines))
    return {"resultado": lines, "pose": _ORION.robot.pose}


@mcp.tool()
def plc_escribir(ip: str, area: str, direccion: str, valor: str, motivo: str) -> dict:
    """Escribe a un PLC (Siemens S7 vía snap7, Modbus, OPC UA). SIEMPRE espera
    confirmación humana: una línea real puede lastimar a alguien si arranca sola."""
    return _gate("plc_escribir", {"ip": ip, "area": area, "direccion": direccion, "valor": valor},
                 Risk.FISICO, motivo) or {}


# ------------------------------------------------------------------ interfaz y memoria
@mcp.tool()
def hud_mostrar(tipo: str, titulo: str, datos: dict) -> dict:
    """Hace aparecer un panel en el HUD. tipo = modelo3d | tabla | grafico | codigo |
    plano | robot | camara | texto | qr. El usuario podrá pedir cambios sobre él."""
    pid = f"{dt.datetime.now():%H%M%S}-{re.sub(r'[^a-z0-9]+', '-', titulo.lower())[:30]}"
    (HOME / "hud" / "inbox" / f"{pid}.json").write_text(
        json.dumps({"id": pid, "tipo": tipo, "titulo": titulo, "datos": datos}, ensure_ascii=False))
    return {"ok": True, "panel": pid}


# ------------------------------------------------------------------ memoria (Markdown: Obsidian/Logseq/…)
_MEM = None


def _mem():
    global _MEM
    if _MEM is None:
        from orion_memoria import Memoria
        _MEM = Memoria(os.environ.get("ORION_BOVEDA", str(HOME.parent / "Orion")))
    return _MEM


@mcp.tool()
def memoria_anotar(texto: str, titulo: str = "", carpeta: str = "temas", etiquetas: list[str] | None = None) -> dict:
    """Guarda en la memoria. Sin título → diario del día; con título → nota propia (usa [[enlaces]])."""
    f = _mem().nota(titulo, texto, carpeta) if titulo else _mem().diario(texto, quien="IA", etiquetas=etiquetas)
    return {"ok": True, "archivo": str(f)}


@mcp.tool()
def memoria_buscar(consulta: str, n: int = 8) -> list:
    """Busca en todo lo registrado (palabras y, si hay embeddings, por significado)."""
    return _mem().buscar(consulta, n)


@mcp.tool()
def memoria_contexto(consulta: str) -> str:
    """Notas relevantes listas para leer antes de responder. Úsalo al empezar cualquier tarea."""
    return _mem().contexto(consulta)


# ------------------------------------------------------------------ equipo y módulos
@mcp.tool()
def perfil_equipo() -> dict:
    """Nivel del equipo (micro…estacion) y qué alternativa se usa para oído, voz, memoria, 3D, imagen."""
    from orion_core import perfil
    return perfil.perfil()


@mcp.tool()
def modulos() -> dict:
    """Módulos instalables/instalados. Respeta 'mostrar_marca': usa nombre_visible al hablar con la persona."""
    m = json.loads((REPO / "config/modulos.json").read_text())
    return {k: {"nombre": v.get("nombre_visible"), "que_hace": v.get("descripcion_ia", ""),
                "instalado": Path(os.path.expanduser(v["comando"][-1])).exists() if v.get("tipo") != "mcp" or k != "orion-linux" else True}
            for k, v in m.items() if not k.startswith("_")}


# ------------------------------------------------------------------ proyectos
def _proy(pid: str):
    from orion_proyectos import Proyecto
    return Proyecto.cargar(str(Path(os.environ.get("ORION_BOVEDA", str(HOME.parent / "Orion"))) / "proyectos" / f"{pid}.json"))


@mcp.tool()
def proyecto_crear(id: str, nombre: str, tareas: list[dict], cliente: str = "", inicio: str = "",
                   gg: float = 0.15, utilidad: float = 0.10, contingencia: float = 0.05) -> dict:
    """Crea un proyecto. tareas: [{id:'1.1', nombre, dias, depende:['1.0'], responsable, raci:{}, costo, avance}].
    Devuelve programa CPM (ruta crítica) y presupuesto con IVA 19 %."""
    from orion_proyectos import Proyecto, Tarea
    p = Proyecto(id, nombre, cliente, inicio, [Tarea(**t) for t in tareas], gg, utilidad, contingencia)
    info = p.programar(); p.guardar(os.environ.get("ORION_BOVEDA", str(HOME.parent / "Orion")))
    return {"programa": info, "presupuesto": p.presupuesto()}


@mcp.tool()
def proyecto_actualizar(id: str, cambios: list[dict]) -> dict:
    """Cambia tareas existentes o agrega nuevas: [{id:'2.1', avance:0.5}, {id:'5.1', nombre:'…', dias:2, depende:['4.2']}]."""
    from orion_proyectos import Tarea
    p = _proy(id); idx = {t.id: t for t in p.tareas}
    for c in cambios:
        if c["id"] in idx:
            for k, v in c.items():
                setattr(idx[c["id"]], k, v)
        else:
            p.tareas.append(Tarea(**c))
    p.tareas.sort(key=lambda t: [int(x) for x in t.id.split(".")])
    info = p.programar(); p.guardar(os.environ.get("ORION_BOVEDA", str(HOME.parent / "Orion")))
    return {"programa": info, "avance": p.avance(), "presupuesto": p.presupuesto()}


@mcp.tool()
def proyecto_exportar(id: str) -> dict:
    """Carta Gantt (SVG para el HUD) + Excel (programa, Gantt, presupuesto, RACI)."""
    p = _proy(id); out = WORK / "proyectos"; out.mkdir(parents=True, exist_ok=True)
    svg = out / f"{id}_gantt.svg"; svg.write_text(p.gantt_svg())
    return {"gantt_svg": str(svg), "excel": p.a_excel(str(out / f"{id}.xlsx")), "raci": p.raci(), "avance": p.avance()}


# ------------------------------------------------------------------ forma: foto/idea → 3D → CAD → servos
@mcp.tool()
def forma_alternativas(descripcion: str, n: int = 3) -> dict:
    """Imágenes de variantes de un diseño (necesita GPU o nube)."""
    import orion_forma
    return orion_forma.alternativas(descripcion, n, salida=str(WORK / "forma"))


@mcp.tool()
def forma_foto_a_3d(imagen: str, alto_real_mm: float = 100, espesor_mm: float = 10) -> dict:
    """Foto → modelo 3D. Usa TRELLIS.2/Hunyuan3D/TripoSR si están; si no, silueta extruida a escala (CPU)."""
    import orion_forma
    r = orion_forma.foto_a_malla(imagen, salida=str(WORK / "forma"))
    if not r["ok"]:
        r = orion_forma.silueta_a_solido(imagen, alto_real_mm, espesor_mm, salida=str(WORK / "forma")) | {"via": "silueta (CPU)"}
    if r.get("stl"):
        r["vista"] = orion_forma.vista_previa(r["stl"], r["stl"].replace(".stl", ".png"))
    return r


@mcp.tool()
def forma_integrar_servos(step: str, servos: list[dict]) -> dict:
    """Corta bolsillos, ejes y tornillos. servos: [{modelo:'MG996R'|'SG90'|'STS3215'|'NEMA17', x, y, rot, eje:'A2'}]."""
    import orion_forma
    r = orion_forma.integrar_servos(step, servos, salida=str(WORK / "forma"))
    r["vista"] = orion_forma.vista_previa(r["stl"], r["stl"].replace(".stl", ".png"))
    return r


@mcp.tool()
def repos_buscar(consulta: str, n: int = 8) -> list:
    """ANTES de construir algo nuevo: busca en GitHub si ya existe (estrellas, licencia, actividad,
    si sirve para uso comercial). Si nada sirve, constrúyelo y guárdalo en ~/orion/repos/propios."""
    from orion_repos import buscar
    return buscar(consulta, n)


@mcp.tool()
def repos_verificar(repos: list[str]) -> list:
    """Confirma repositorios conocidos ('dueño/nombre'): existencia, licencia y última actividad."""
    from orion_repos import verificar
    return verificar(repos)


@mcp.tool()
def voz_decir(texto: str, motor: str = "auto") -> dict:
    """Habla por el parlante (piper, kokoro, espeak-ng o ElevenLabs si hay clave)."""
    from orion_voz import hablar
    a = hablar(texto, motor)
    return {"ok": bool(a), "bytes": len(a or b"")}


@mcp.tool()
def identidad(cambios: dict) -> dict:
    """Cambia nombre, voz, género, tono o color del asistente ('desde ahora te llamas Vega')."""
    allowed = {"nombre", "tono", "genero", "color", "voz", "idioma", "saludo"}
    ident = _identity()
    ident.update({k: v for k, v in cambios.items() if k in allowed})
    (HOME / "identidad.json").write_text(json.dumps(ident, ensure_ascii=False, indent=2))
    return ident


if __name__ == "__main__":
    mcp.run()
