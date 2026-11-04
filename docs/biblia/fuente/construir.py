"""Arma «La biblia de Neo Orión» (HTML → PDF horizontal, modo oscuro, estética Marathon)."""
import html
import json
from pathlib import Path

B = Path(__file__).resolve().parent          # esta carpeta (fuente de la biblia)
R = B.parents[2]                              # raíz del repo
CAT = json.loads((R / "config/ia-catalogo.json").read_text())
HERR = json.loads((B / "herramientas.json").read_text())
CMDS = json.loads((B / "comandos.json").read_text())
TOK = json.loads((B / "tokens.json").read_text())
ED = "EDICIÓN 03-11-2026 · B"
e = html.escape

PAGES = []      # (seccion, html)
FIG = [0]
TOC = []        # (num, titulo, parte)


def fig(img, cap, alto=None):
    FIG[0] += 1
    st = f"max-height:{alto}" if alto else ""
    return (f'<figure><img src="img/{img}.jpg" style="{st}"><figcaption><b>F-{FIG[0]:02d}</b> {cap}</figcaption></figure>')


def tabla(cab, filas, anchos=None):
    th = "".join(f'<th style="width:{anchos[i]}">{c}</th>' if anchos else f"<th>{c}</th>" for i, c in enumerate(cab))
    tr = "".join("<tr>" + "".join(f"<td>{c}</td>" for c in f) + "</tr>" for f in filas)
    return f'<table><thead><tr>{th}</tr></thead><tbody>{tr}</tbody></table>'


def pag(sec, cuerpo, clase=""):
    PAGES.append((sec, cuerpo, clase))


def seccion(num, titulo, lead, parte):
    TOC.append((num, titulo, parte))
    return f'<div class="sh"><span class="sn">{num}</span><h1>{titulo}</h1></div><p class="lead">{lead}</p>'


def kv(filas):
    return tabla(["DATO", "VALOR"], filas, ["26%", "74%"])


def caja(titulo, txt, clase=""):
    return f'<div class="caja {clase}"><div class="ct">{titulo}</div>{txt}</div>'


# =====================================================================================
# PORTADA
pag("", """<div class="portada"><div class="pl"><div id="logo" class="logo"></div>
<div class="kick">DOCUMENTO MAESTRO · LA BIBLIA DE NEO ORIÓN · {ed}</div>
<div class="frase">«Todo lo que es Neo Orión OS y cómo se ve: la idea, la voz, las IA, los robots, el instalador, el HUD y cada regla de su diseño.»</div>
<p class="dim">Para cualquier persona o IA que vaya a trabajar en el sistema.</p></div>
<div class="pr"><div class="ct">CONTENIDO</div><div id="toc"></div></div></div>""".replace("{ed}", ED), "sin")

# =====================================================================================
P1 = "PARTE 1 · QUÉ ES"
pag("01 QUÉ ES NEO ORIÓN", seccion("01", "QUÉ ES NEO ORIÓN",
    "Neo Orión OS es un Linux sin escritorio que se maneja con la voz: la pantalla queda vacía mientras escucha y la IA arma la interfaz solo cuando hace falta mostrar algo. Por debajo, la IA controla todo el computador por código.", P1) +
    '<div class="g2"><div>' + kv([
        ("Nombre", "Neo Orión OS (asistente por defecto: «Neo Orión»; nombre, voz, tono y color se cambian)"),
        ("Autor", "Davis Reyes · software libre GPL-3.0"),
        ("Base", "Ubuntu 24.04 LTS «noble», núcleo 6.8, arranque seguro"),
        ("Ediciones", "full · lite · patata (PC que no rinde) · test"),
        ("Interfaz", "Voz siempre activa + HUD que aparece y se va; chat con la tecla /"),
        ("Inteligencia", "IA locales (Ollama) elegidas según el equipo + IA en la nube opcionales"),
        ("Control", "Servidor MCP «orion-linux»: 30 herramientas (shell, archivos, CAD, robots, PLC, navegador…)"),
        ("Memoria", "Bóveda Markdown legible por cualquier IA y por Obsidian, Logseq o SilverBullet"),
        ("Código", "Python (núcleo, ~4.200 líneas) + HTML/JS (HUD e instalador, ~2.200 líneas), 13 pruebas"),
        ("Estética", "Marathon / O.R.C.A.: píxel, un acento a todo volumen (escarlata), carbón"),
    ]) + '</div><div>' + fig("hud-02-reposo", "Reposo: pantalla vacía, Neo Orión escuchando.") +
    caja("PARA QUIÉN", "Para quien trabaja en ingeniería, robótica, fabricación o proyectos, y para cualquier persona que quiera un computador que se maneje hablando, incluso en un PC viejo.") + "</div></div>")

pag("02 LA IDEA", seccion("02", "LA IDEA", "Cómo nació y qué tiene que cumplir. Son las reglas que no se negocian.", P1) + """<div class="g3">
<div class="caja"><div class="ct">HABLAR EN VEZ DE MANIPULAR</div>No hay escritorio, íconos ni ventanas que mover. La persona dice lo que necesita y la IA lo hace por detrás, en el código, y muestra solo el resultado. Como Jarvis: el poder está en la máquina, la persona solo dirige.</div>
<div class="caja"><div class="ct">LA IA ES LA PC</div>La IA tiene acceso completo al Linux por MCP: clona, instala, compila, dibuja, programa placas, mueve robots. Lo que es físico o irreversible siempre pide confirmación humana.</div>
<div class="caja"><div class="ct">LABORATORIO INTELIGENTE</div>Ingeniería en general: robots con calibración por foto, CAD por código, placas, LiDAR, PLC, gestión de proyectos, planos, escaneo 3D, juegos, investigación.</div>
<div class="caja"><div class="ct">INTERFAZ SOLO CUANDO SE PIDE</div>Inspirada en el asistente «Rose» y en O.R.C.A.: paneles que se materializan, cumplen y se van. Todo con la misma estética pixel.</div>
<div class="caja"><div class="ct">CUALQUIER EQUIPO</div>Desde una Raspberry hasta una estación con GPU. Si el equipo no rinde, baja solo a modo liviano o patata y manda lo pesado a la nube.</div>
<div class="caja"><div class="ct">BASE NEUTRA + CAPA PERSONAL</div>Se instala limpio, sin datos de nadie. Lo personal de cada quien (notas, memoria, módulos propios) va en una capa aparte que solo se activa en sus equipos.</div></div>""")

# arquitectura
diag = """<div class="diag">
<div class="dc"><div class="db">VOZ · CHAT</div><div class="dd">micrófono siempre activo<br>VAD + Whisper/Vosk<br>tecla / para escribir</div></div><div class="da">▸</div>
<div class="dc"><div class="db acc">IA</div><div class="dd">local (Ollama) o nube<br>elige según la tarea<br>y el equipo</div></div><div class="da">▸</div>
<div class="dc"><div class="db">MCP ORION-LINUX</div><div class="dd">30 herramientas<br>shell · archivos · CAD<br>robots · PLC · navegador</div></div><div class="da">▸</div>
<div class="dc"><div class="db">MÓDULOS</div><div class="dd">motion · lidar · forma<br>proyectos · juegos<br>puente · navegador</div></div><div class="da">▸</div>
<div class="dc"><div class="db">EQUIPO Y MUNDO</div><div class="dd">Linux · robots · placas<br>Raspberry · PLC<br>web · otras IA</div></div></div>
<div class="diag2"><div class="db pel">GUARDIÁN Y PERMISOS · lo físico y lo irreversible esperan a la persona</div><div class="db">MEMORIA · bóveda Markdown que lee cualquier IA</div><div class="db">HUD · muestra el resultado y se retira</div></div>"""
pag("03 CÓMO FUNCIONA", seccion("03", "CÓMO FUNCIONA POR DENTRO",
    "La IA solo habla y decide; el trabajo lo hacen herramientas con nombre, y cada acción pasa por los permisos. Así cualquier IA (local, Claude, otra) controla el mismo sistema de la misma forma.", P1) + diag +
    '<div class="g2" style="margin-top:14px">' + caja("TRES REGLAS DEL NÚCLEO",
    "<ol><li><b>La IA nunca toca los motores directo</b>: un traductor (Python, sin IA) convierte sus palabras en órdenes exactas y el guardián decide si se ejecutan.</li>"
    "<li><b>Riesgos con nombre</b>: LEER · ESCRIBIR · SISTEMA · IRREVERSIBLE · FÍSICO. Los dos últimos crean un ticket que solo la persona aprueba, por voz o en el HUD.</li>"
    "<li><b>Todo queda en la bitácora</b> de acciones y en la memoria, para que la próxima IA sepa lo que pasó.</li></ol>") +
    caja("DÓNDE VIVE CADA COSA", tabla(["CARPETA", "QUÉ ES"], [
        ("orion/orion_core", "detección de equipo, perfil, permisos, instalación, rendimiento"),
        ("orion/orion_mcp", "servidor MCP «orion-linux» y configuración para cada IA"),
        ("orion/orion_voz", "oído, VAD, voz, hablantes"), ("orion/orion_memoria", "bóveda Markdown + índice"),
        ("hud/", "HUD, instalador y marathon.js"), ("overlay/", "lo que se copia al sistema instalado"),
        ("config/", "distro, paquetes, catálogo de IA, módulos, capas"), ("capas/", "capas personales (opcionales)")])) + "</div>")

# =====================================================================================
P2 = "PARTE 2 · RECORRIDO"
pag("04 ARRANQUE E INSTALACIÓN", seccion("04", "ARRANQUE E INSTALACIÓN",
    "Se graba la ISO en un pendrive y se arranca el PC desde él. Neo Orión detecta que viene del pendrive (boot=casper) y abre el instalador a pantalla completa, sin escritorio. Ya instalado, al encender aparece el HUD.", P2) +
    '<div class="g2">' + fig("ins-00-intro", "Arranque del instalador: logo de bloques, barra blanca tipo O.R.C.A. y columna de letras.") +
    fig("ins-01-idioma", "Paso 1: idioma, región y TU COLOR de entrada (escarlata por defecto).") + "</div>")
pag("04 ARRANQUE E INSTALACIÓN", '<div class="g2">' + fig("ins-02-equipo", "Paso 2: escaneo real del equipo y su nivel (micro → estación).") +
    fig("ins-03-disco", "Paso 3: disco. Borrar todo exige mantener apretado el botón.") +
    fig("ins-03b-disco-confirmar", "Al confirmar el borrado: alerta a pantalla completa (amarilla porque el acento es rojo).") +
    fig("ins-04-ia", "Paso 4: IA. Barra de si cabe, recomendada, filtros y ficha con «i».") + "</div>")
pag("04 ARRANQUE E INSTALACIÓN", '<div class="g2">' + fig("ins-04b-ia-sinfiltro", "Filtro SIN FILTRO con la explicación de cada modelo.") +
    fig("ins-04c-ia-apps", "APPS: Odysseus (PewDiePie), interfaz que se instala aparte.") +
    fig("ins-04d-ia-patata", "En un PC patata avisa y recomienda sumar una IA en la nube.") +
    fig("ins-05-voz", "Paso 5: oído, voz y app de memoria, con su licencia.") + "</div>")
pag("04 ARRANQUE E INSTALACIÓN", '<div class="g2">' + fig("ins-06-identidad", "Paso 6: nombre, voz, tono y color, con vista previa y voz de prueba.") +
    fig("ins-07-modulos", "Paso 7: módulos y tipo de instalación (limpia o con capa personal).") +
    fig("ins-08-cuenta", "Paso 8: cuenta, clave en bloques y registro de voz (3 veces la frase).") +
    fig("ins-09-instalando", "Paso 9: instalación con porcentaje, bloques y registro.") + "</div>")
pag("04 ARRANQUE E INSTALACIÓN", '<div class="g2"><div>' + fig("ins-10-completo", "Final: bloque de acento con check. Reiniciar lleva a la pantalla vacía.") +
    '</div><div>' + caja("LO QUE GENERA EL INSTALADOR",
    "Un JSON con todo lo elegido. <code>orion_core/instalacion.py</code> lo valida (nombres de modelo y de usuario seguros, color hex) y lo convierte en pasos: descargar las IA, escribir la identidad, activar módulos, importar la capa y encender el oído. El particionado y la copia los hace Calamares con los mismos datos.") +
    caja("EN EL TELÉFONO", "Todo el instalador funciona en pantalla angosta: el riel de pasos pasa arriba en horizontal y los botones quedan fijos abajo.") +
    fig("ins-movil-ia", "Paso 4 en formato teléfono.", "60mm") + "</div></div>")

hud = [("hud-01-arranque", "Arranque del HUD: registro y logo de bloques."), ("hud-03-chat", "Chat: pestaña y tecla /, historial TÚ / ORION."),
       ("hud-04-hardware", "«escanea el hardware»: qué IA cabe (igual que orion-fit)."), ("hud-05-robot", "Gemelo digital del brazo de 6 ejes; la orden pasa por el traductor y el guardián."),
       ("hud-06-onda", "Movimiento en onda: θ(t) = c + A·sin(2π·f·t), con tope de velocidad del servo."), ("hud-07-cad", "CAD por código (build123d) con visor 3D."),
       ("hud-07b-cad-reducir", "«reduce el grosor a 3»: se reconstruye la pieza y cambian las medidas."), ("hud-08-lidar", "LiDAR en vivo: mapa, personas en color de peligro y decisión de frenado."),
       ("hud-09-radar", "Radar del entorno desde el LiDAR del nodo 02."), ("hud-10-lentes", "Vista de los lentes con radar en la esquina."),
       ("hud-11-gantt", "Carta Gantt con ruta crítica (CPM)."), ("hud-12-proyecto", "Proyecto del brazo: piezas y archivos."),
       ("hud-13-alternativas", "Alternativas de diseño del robot."), ("hud-14-placa", "Placa de control de bajo costo."),
       ("hud-15-nodos", "Nodos Raspberry de la flota."), ("hud-16-memoria", "Memoria: notas y vínculos."),
       ("hud-17-calibrar", "Calibración con foto de referencia."), ("hud-18-telefono", "Vincular teléfono o lentes con QR."),
       ("hud-19-juegos", "Sala de juegos: local, todas las nubes (con disponibilidad en Chile), casa y medios."), ("hud-20-navegador", "Chrome normal: pestañas, barra, + para abrir más."),
       ("hud-21-piloto", "«maneja tú»: Neo Orión maneja el mismo Chrome con franja y cursor de bloque."), ("hud-22-puente", "Puente entre IAs: Neo Orión ⇄ Claude con paquete de contexto."),
       ("hud-23-color", "«cambia el color a #b14bff»: todo el HUD en morado."), ("hud-24-patata", "Modo patata: sin brillos ni 3D, la identidad se mantiene.")]
pag("05 EL HUD", seccion("05", "EL HUD, PANTALLA POR PANTALLA", "Cada panel aparece porque la persona lo pidió (por voz o por chat), cumple y se cierra. Estos son los que existen hoy.", P2) +
    '<div class="g2">' + fig(*hud[0]) + fig(*hud[1]) + "</div>")
for i in range(2, len(hud), 4):
    pag("05 EL HUD", '<div class="g2">' + "".join(fig(*h) for h in hud[i:i + 4]) + "</div>")
pag("05 EL HUD", '<div class="g2">' + fig("hud-28-alerta", "«detente»: el guardián frena y muestra la alerta.") +
    fig("hud-movil-investigacion", "Investigación en el teléfono.", "105mm") + "</div>")

pag("06 INVESTIGACIÓN", seccion("06", "INVESTIGACIÓN ESTILO ROSE", "«investiga lidar para robots»: el agente busca por detrás (web, repositorios, memoria) y arma un muro de bloques que explica en voz alta.", P2) +
    '<div class="g3">' + fig("hud-25-investigacion-1", "1 · Tarjeta INVESTIGACIÓN // con reloj, 26 segmentos y contadores.") +
    fig("hud-26-investigacion-2", "2 · La tarjeta vuela a la cámara y llega el muro en perspectiva.") +
    fig("hud-27-investigacion-3", "3 · Tocar un bloque lo trae al frente con su explicación.") + "</div>" +
    caja("CÓMO SE COMPORTA", "Pestañas TODO · RESUMEN · DATOS · FUENTES · SIGUIENTE; subtítulos «ORION ›» mientras explica; MUTE corta la voz; arrastrar gira el muro y doble toque vuelve a la vista automática. En el equipo los bloques llegan del agente real; en la vista previa, del LiDAR."))

# IA
tiers = [("PC patata · 2 GB", "gemma3:1b", "órdenes; lo pesado a la nube"), ("Notebook 8 GB, sin GPU", "gemma3:4b", "general liviano con visión"),
         ("Notebook 16 GB, sin GPU", "qwen3:8b", "general, muy buen español"), ("Torre 32 GB + GPU 12 GB", "qwen3:14b", "razonamiento + herramientas"),
         ("Estación 32 GB + GPU 24 GB", "qwen3:30b-a3b", "MoE rápido"), ("Sin filtro, 16 GB CPU", "dolphin3 abliterado 8b", "sin rechazos (opcional)")]
pag("07 INTELIGENCIA", seccion("07", "INTELIGENCIA: CUÁL IA Y CUÁNDO",
    f"El catálogo trae {len(CAT['locales'])} IA locales, {len(CAT['nube'])} en la nube, {len(CAT['solo_servidor'])} abiertas pero gigantes (solo servidor) y apps como Odysseus. Neo Orión mide la RAM y la GPU, calcula cuál es la más grande que corre fluida y recomienda esa; el resto se puede instalar igual.", P2) +
    '<div class="g2"><div>' + tabla(["EQUIPO", "RECOMIENDA", "PARA"], tiers) +
    caja("CÓMO DECIDE (orion-fit)", "Presupuesto = VRAM × 0,9 con GPU; sin GPU, (RAM − sistema − voz) × 0,9 con tope de 6,5 GB para que la conversación sea fluida (los MoE sí pasan). Por defecto recomienda modelos con filtro; <code>--sin-filtro</code> elige entre los sin filtro. <code>--actualizar</code> revisa el catálogo vivo de Ollama y avisa de familias nuevas.") +
    "</div><div>" + caja("CON FILTRO · NEUTRAL · SIN FILTRO",
    "<b>Con filtro</b>: rechaza temas sensibles según quien lo hizo.<br><b>Neutral</b> (Hermes): obedece el prompt de sistema; tú pones las reglas.<br><b>Sin filtro</b> (Dolphin, abliterados de huihui-ai): sin rechazos.<br><br>El guardián no cambia con ninguno: robots, PLC y acciones irreversibles siempre piden permiso.") +
    caja("ORIGEN", "EE.UU. (Google, Meta, Microsoft, OpenAI), China (Alibaba Qwen, DeepSeek, Z.ai GLM, Moonshot Kimi, MiniMax, Xiaomi), Europa (Mistral, Aleph Alpha) y comunidad. Los modelos y servicios chinos filtran temas políticos de China; los servicios guardan datos en sus servidores.") +
    caja("COLABORAN", "Con el puente (sección 11) una IA le pasa trabajo a otra con contexto: Claude, Gemini, ChatGPT/Codex, OpenCode o la local.") + "</div></div>")

pag("08 VOZ Y MEMORIA", seccion("08", "VOZ, OÍDO, HABLANTES Y MEMORIA", "Escucha siempre, sin botón. Sabe quién habla y guarda todo en archivos que cualquier IA puede leer.", P2) + '<div class="g3">' +
    caja("OÍDO", "VAD combinado (WebRTC + energía) para no confundir ruido con voz; STT faster-whisper (MIT), whisper.cpp o Vosk según el equipo. Se dirige por nombre («Orión, …») con ventana de 25 s; «Orión, pausa / sigue».") +
    caja("HABLANTES", "Huellas de voz (sherpa-onnx, SpeechBrain o espectral). Aprende nombres por contexto («soy…», «me llamo…»), separa pistas por persona y re-agrupa al cerrar la conversación (9/9 en las pruebas). Solo el dueño da órdenes de sistema.") +
    caja("VOZ", "Piper (GPL-3.0), Kokoro (Apache), espeak-ng (mínima) o ElevenLabs en la nube. Nombre, género y tono cambian por voz.") + "</div>" + '<div class="g2" style="margin-top:12px"><div>' +
    caja("MEMORIA", "Bóveda Markdown (diario, notas, personas, proyectos, colaboraciones) con índice SQLite FTS5 y embeddings opcionales. Compatible con Obsidian, Logseq, SilverBullet, Zettlr; importable en Joplin, Trilium y SiYuan. <code>memoria_contexto</code> le da a cualquier IA lo relevante antes de responder.") +
    "</div><div>" + fig("hud-16-memoria", "La memoria en el HUD.", "62mm") + caja("CAPA PERSONAL", "Cada persona puede tener su capa (notas, memoria, módulos propios) en <code>capas/&lt;nombre&gt;</code>; solo se activa en sus equipos.") + "</div></div>")

pag("09 ROBOTS", seccion("09", "ROBOTS, GUARDIÁN Y LIDAR", "De la palabra al motor sin que la IA toque el hardware directo.", P2) + '<div class="g2"><div>' +
    tabla(["PIEZA", "QUÉ HACE"], [("Traductor", "«mueve el hombro a 45» → SET A2 45; comandos difusos en español, macros «1A»"),
                                  ("Guardián", "límites por eje, velocidad máxima, piso y zonas prohibidas; frena ante obstáculos"),
                                  ("Onda", "θ(t) = c + A·sin(2π·f·k·t); exige 2π·f·k·A ≤ velocidad del servo"),
                                  ("Cinemática", "Denavit–Hartenberg para saber dónde está la punta"),
                                  ("Calibración", "foto de referencia + ajuste lineal grados ↔ pulso"),
                                  ("LiDAR", "mapa de ocupación (log-odds), personas en movimiento, frenado 150 mm + v·0,3 s, corredor 400 mm"),
                                  ("Nodos", "Raspberry configuradas desde el PC central; ROS 2 Jazzy")]) +
    "</div><div>" + fig("hud-05-robot", "Gemelo digital y la orden traducida.", "62mm") + fig("hud-08-lidar", "LiDAR en vivo.", "62mm") + "</div></div>")

pag("10 FABRICACIÓN Y PROYECTOS", seccion("10", "FABRICACIÓN Y PROYECTOS", "Diseñar, cotizar y planificar sin abrir un programa de CAD.", P2) + '<div class="g3">' +
    caja("CAD POR CÓDIGO", "build123d (OpenCascade): piezas paramétricas que se cambian por voz («reduce el grosor a 3») y se exportan a STEP y STL.") +
    caja("FORMA", "Foto → alternativas → malla (TRELLIS.2, Hunyuan3D, TripoSR) → sólido por silueta si no hay GPU → integrar servos (MG996R, SG90, STS3215, NEMA17).") +
    caja("CAD", "CAD por código (CadQuery), FreeCAD, KiCad y slicers en el módulo Taller.") + "</div>" +
    '<div class="g3" style="margin-top:10px">' + caja("PROYECTOS", "Tareas con CPM y ruta crítica, Gantt, RACI, presupuesto chileno (CD → GG 15 % → utilidad 10 % → contingencia 5 % → IVA 19 %) y Excel.") +
    caja("PLACAS", "Placa de control de bajo costo: ESP32-S3, PCA9685, buck 5 V, LDO 3,3 V, parada de emergencia.") +
    caja("REPOSITORIOS", "Siempre busca repositorios antes de construir: 154 catalogados con licencia y uso comercial.") + "</div>" +
    '<div class="g2" style="margin-top:10px">' + fig("hud-07-cad", "Soporte de servo por código.", "58mm") + fig("hud-11-gantt", "Gantt con ruta crítica.", "58mm") + "</div>")

pag("11 JUEGOS, NAVEGADOR Y PUENTE", seccion("11", "JUEGOS, NAVEGADOR Y PUENTE ENTRE IAs", "El computador completo, también para descansar y para trabajar con otras IA.", P2) + '<div class="g3">' +
    caja("JUEGOS Y MEDIOS", "Steam + Proton, Heroic, Lutris, RetroArch; nubes: GeForce NOW (app oficial), Xbox Cloud, Boosteroid, Shadow, Blacknut, Luna (no en Chile); casa: Sunshine/Moonlight, Steam Link, Chiaki-ng; medios: Jellyfin, Kodi, plataformas. Mandos Xbox/PS/Nintendo detectados. Sin fuentes de juegos crackeados ni IPTV sin licencia.") +
    caja("NAVEGADOR", "Chrome y Firefox normales (pestañas, barra, extensiones) desde sus repos oficiales. Dos modos sobre el mismo navegador: MANEJAS TÚ y MANEJA NEO ORIÓN («maneja tú»), con franja y cursor de bloque. Chrome por CDP; Firefox por Marionette. Los chats de IA no se manejan por código.") +
    caja("PUENTE ENTRE IAs", "«usa Claude para crear el documento…»: paquete de contexto (tarea + conversación + memoria) por la vía oficial con tu cuenta: Claude Code (plan Pro/Max), Gemini CLI, Codex CLI, OpenCode o local. Modo web: abre el chat y tú aprietas enviar. Vuelta: el MCP de Neo Orión se conecta a Claude para que Claude le pida cosas al PC.") + "</div>" +
    '<div class="g3" style="margin-top:10px">' + fig("hud-19-juegos", "Sala de juegos.", "52mm") + fig("hud-21-piloto", "Chrome manejado por Neo Orión.", "52mm") + fig("hud-22-puente", "Puente Neo Orión ⇄ Claude.", "52mm") + "</div>")

pag("12 RENDIMIENTO", seccion("12", "RENDIMIENTO Y PC PATATA", "Patata no es un tipo de PC: es cuando el equipo no está rindiendo. Neo Orión lo mide en vivo y se ajusta solo.", P2) + '<div class="g2"><div>' +
    tabla(["SEÑAL", "LIVIANO", "PATATA"], [("Cuadros por segundo del HUD", "< 40", "< 24"), ("Velocidad de la IA (tokens/s)", "< 8", "< 3"), ("Presión de memoria (PSI)", "> 10 %", "> 30 %"),
                                         ("Presión de CPU (PSI)", "> 40 %", "> 75 %"), ("Temperatura del procesador", "> 88 °C", "> 96 °C"), ("Batería < 20 % descargando", "sí", "—")]) +
    caja("CÓMO REACCIONA", "Baja en ~15 s de malas lecturas y sube de a un nivel en ~2 min. Avisa por qué. LIVIANO: HUD sin efectos ni 3D, IA un paso menor. PATATA: además oído Vosk, IA mínima para órdenes y lo pesado a la nube. «modo completo» fija el nivel.") +
    "</div><div>" + fig("hud-24-patata", "Modo patata: misma identidad, menos costo.", "70mm") +
    caja("EDICIÓN PATATA", "Para PC viejos: sin escritorio, HUD en cage + cog (~150 MB), zram, voz espeak-ng. Desde 2 GB de RAM.") + "</div></div>")

pag("13 HERRAMIENTAS", seccion("13", "HERRAMIENTAS DEL TALLER Y LA CASA",
    "Nueve herramientas que se piden hablando. Las físicas (CNC, cerraduras) esperan tu confirmación; el resto es inmediato.", P2) +
    '<div class="g2">' + fig("hud-elec", "«qué cable para 4500 W a 40 metros»: sección por caída de tensión (RIC N°03: ≤ 3 % y ≤ 5 % total).", "62mm") +
    fig("hud-cnc", "Fresadora CNC (GRBL): revisa el G-code sin mover nada; cortar pide confirmación; PARAR YA es inmediato.", "62mm") + "</div>" +
    tabla(["HERRAMIENTA", "DI…", "QUÉ HACE"], [
        ("Calculadora RIC", "«20 amperes a 35 metros»", "sección, caída, protección; mínimos 4 y 2,5 mm²; Iz del RIC N°04 o del fabricante"),
        ("Respaldo", "«respalda ahora»", "fotos diarias a disco externo o nube (rclone); guarda 30; restaurar sin pisar"),
        ("Escritorio remoto", "«entrar desde el teléfono»", "en la casa: VNC con clave y cifrado; desde afuera: RustDesk"),
        ("Taller CNC", "«abre la fresadora»", "medidas, tiempo, profundidad y si cabe; pausa, seguir, parar"),
        ("Cámaras", "«muéstrame las cámaras»", "Frigate: personas, autos, mascotas; persona en zona de máquina → frena"),
        ("Casa", "«apaga la luz del taller»", "Home Assistant; cerraduras, alarmas y portones con confirmación"),
        ("Dictado", "«dicta un informe»", "Word con formato: informe DUOC (logo arriba a la derecha), memoria eléctrica, carta, acta"),
        ("Actualizaciones", "«hay actualizaciones»", "parches de Ubuntu + canal de Neo Orión con suma SHA-256"),
        ("Traductor", "«modo traductor con inglés»", "conversación en vivo; sin internet con Argos; audífonos de conducción ósea")], ["17%", "25%", "58%"]))
pag("13 HERRAMIENTAS", '<div class="g2">' + fig("hud-remo", "Escritorio remoto: en la casa (VNC) o desde afuera (RustDesk). La clave aparece solo si tocas VER.") +
    fig("hud-trad", "Traductor en vivo: cada uno habla en su idioma y sale en el del otro.") +
    fig("hud-dict", "Dictado a Word: marcas «título», «sección», «punto», «tabla».") + fig("hud-drv", "«revisa los drivers»: procesador, gráfica, driver en uso y si hay uno más nuevo.") + "</div>")
pag("14 LLAVE DE VOZ Y CONTROLADORES", seccion("14", "FORMAS DE INICIAR Y REPOSO",
    "Como en Iron Man: «Buenos días, señoritas» y dos palmadas, y Neo Orión despierta y prepara todo. Tú eliges las frases, las palmadas y qué hace cada una.", P2) +
    '<div class="g2"><div>' + fig("hud-activacion", "Todo editable: cada frase o palmada con su rutina en el orden que quieras, pasos propios (decir, abrir, dar una orden), saludo y sensibilidad.", "64mm") +
    caja("TRES NIVELES DE REPOSO", "<b>Atento</b> (recomendado): pantalla apagada, IA fuera de la memoria, el oído escucha → despierta con tu frase o tus palmadas. <b>Suspendido</b> y <b>apagado</b>: el PC no puede oír; lo despierta un nodo con micrófono (Raspberry o ESP32) que manda la señal de encendido por la red (Wake-on-LAN), el teclado o el teléfono.") +
    '</div><div>' + fig("hud-buenos-dias", "«Buenos días, señoritas»: despertar → saludo → luces → resumen del día.", "64mm") +
    caja("DESBLOQUEO A TU GUSTO", "La frase se graba 1 vez (rápido) o 3 (más preciso). Desbloquear: solo con tu frase · solo con tu clave · frase o clave · frase y clave. Se cambia por voz: «desbloquea solo con clave».") + "</div></div>")
pag("14 LLAVE DE VOZ Y CONTROLADORES", seccion("14", "LLAVE DE VOZ Y CONTROLADORES",
    "Tu frase con tu voz para entrar, como «Oye Siri» pero eligiendo la frase. Y la gráfica y el procesador reconocidos con sus drivers al día.", P2) +
    '<div class="g2"><div>' + fig("ins-cuenta-frase", "Instalador, paso «Tu cuenta»: eliges la frase y la dices 3 veces.", "62mm") +
    caja("CÓMO FUNCIONA LA LLAVE", "Calzan dos cosas: <b>la frase</b> (el oído revisa que sea, tolera errores) y <b>la voz</b> (la huella se parece a la guardada más que el umbral, calibrado con tus 3 grabaciones). Sirve para despertar y desbloquear, reconocer al dueño y, junto al botón de mantener, aprobar lo físico. La clave de la cuenta sigue de respaldo y lo irreversible nunca se aprueba solo con la voz. «cambia mi frase» · «olvida mi voz» · «bloquea».") +
    '</div><div>' + fig("hud-bloqueo", "Bloqueado: di tu frase o escribe tu clave.", "58mm") +
    caja("CONTROLADORES", "Procesador → microcódigo Intel/AMD. NVIDIA → el driver que Ubuntu recomienda para ese modelo (firmado, con CUDA). AMD → amdgpu + Mesa (Vulkan, VA-API). Intel → i915/xe + Mesa + video. El instalador deja el plan y el primer arranque lo instala; se actualizan con el sistema y «revisa los drivers» avisa si hay uno NVIDIA más nuevo. El BIOS/firmware (fwupd) solo con tu permiso.") + "</div></div>")

# =====================================================================================
P3 = "PARTE 3 · DISEÑO"
pag("15 PRINCIPIOS", seccion("15", "DISEÑO: DE DÓNDE VIENE Y SUS PRINCIPIOS",
    "Marathon (Bungie, 2026) llama a su estilo «Graphic Realism»: lenguaje gráfico simplificado, color sin pudor y pocos materiales, con raíz en el diseño gráfico de los 90 (The Designers Republic). O.R.C.A. de danneuenhaus aporta el movimiento pixel; Rose, el flujo de investigación.", P3) + '<div class="g3">' +
    "".join(caja(t, x) for t, x in [
        ("1 · EL PÍXEL ES EL MATERIAL", "Logos, iconos, títulos, barras y transiciones se arman con cuadrados. Nada redondeado, nada con degradado suave."),
        ("2 · UN ACENTO A TODO VOLUMEN", "Escarlata por defecto, lleno (no en tintes). Es el color de la persona: se elige al instalar y se cambia por voz."),
        ("3 · PELIGRO ES PELIGRO", "Solo para lo irreversible, lo físico y personas en el LiDAR. Si el acento es rojo, el peligro pasa a amarillo."),
        ("4 · LECTURA EN MONO", "Silkscreen para rótulos cortos y números; JetBrains Mono para todo lo que se lee de corrido. El estilo nunca tapa el dato."),
        ("5 · ETIQUETADO DE PRODUCTO", "Cada caja lleva TIPO // DETALLE, contador 03 / 10 y pie AGREGADO RECIÉN · 0x1A3F. Parece equipo, no página web."),
        ("6 · MOVIMIENTO POR PASOS", "Decodificar, bloques que llegan, disolución por bloques. Nunca suave; una animación protagonista a la vez."),
    ]) + "</div>" + caja("LA LECCIÓN DE MARATHON", "Mucha gente encontró su interfaz difícil de leer. Neo Orión toma la inversa como regla: una pantalla = una idea, y el dato siempre legible. El carácter va en los bordes, rótulos y transiciones."))

pag("16 COLOR", seccion("16", "COLOR", "Un acento elegido por la persona sobre carbón, neutros fríos y un color de peligro que se ajusta solo.", P3) +
    fig("lam-paleta", "Paleta: 8 acentos (escarlata por defecto) + neutros + peligro (amarillo con acento rojo, rojo en los demás).") + '<div class="g3">' +
    caja("TINTA AUTOMÁTICA", "El texto sobre un bloque de acento es negro si el acento es claro y blanco si es oscuro (marathon.js calcula la luminancia).") +
    caja("CONTRASTE", "Texto y gris sobre fondo ≥ 4,5:1. «Tenue» (#55585e) y acento oscuro solo para decoración.") +
    caja("CAMBIAR", "«cambia el color a magenta» o «pon el color #ff3d9a». Cambia todo: paneles, canvas, 3D, muro.") + "</div>")

pag("17 TIPOGRAFÍA", seccion("17", "TIPOGRAFÍA", "Dos familias libres (OFL) y una fuente de bloques propia.", P3) + '<div class="g2">' +
    fig("lam-tipo", "Silkscreen, JetBrains Mono y la fuente de bloques 5×5 con tildes y Ñ.") + fig("lam-escala", "Escala: marca, titular, dato, etiqueta, micro, cuerpo, dato mono.") + "</div>")

pag("18 PÍXEL", seccion("18", "PÍXEL: LOGO E ICONOS", "El logo, los títulos de pantalla y los iconos se dibujan en canvas con bloques que llegan en desorden.", P3) +
    fig("lam-logo", "Logo NEO ORIÓN: sobre fondo, sobre acento (tinta automática) y versión mínima.") + fig("lam-iconos", "20 iconos de bloques: check, x, alerta, mic, robot, cpu, disco, ia, candado, mando, radar, onda, globo, mundo, color, llave, play, usuario, carpeta, señal."))

pag("19 COMPONENTES", seccion("19", "COMPONENTES", "Las piezas con que se arma cualquier pantalla.", P3) + '<div class="g2">' +
    fig("lam-comp", "Botones (principal, secundario, mantener), chapas, tarjeta normal / elegida / deshabilitada, segmentos, bloques y barra de carga.") +
    fig("lam-hitos", "Confirmación (check sobre acento, 520 ms) y alerta a pantalla completa (1,3–1,5 s, vuelve sola).") + "</div>" + '<div class="g3">' +
    caja("BOTÓN", "Principal lleno de acento, uno por pantalla, abajo a la derecha. Lo irreversible se mantiene apretado 1,6 s.") +
    caja("TARJETA", "Elegida = llena de acento. Rótulo micro arriba: dato a la izquierda, estado a la derecha.") +
    caja("PANEL", "Chapa de tipo, título que se decodifica, cerrar cuadrado, cruces + en las esquinas.") + "</div>")

mov = tabla(["EFECTO", "FUNCIÓN", "DURACIÓN", "CUÁNDO"], [("Decodificar", "MX.scramble", "300–700 ms", "títulos y estados"), ("Bloques que llegan", "MX.pixel", "300–1000 ms", "logo, títulos, iconos"),
    ("Falla RGB", "MX.glitch", "260 ms", "al aparecer; al azar cada ~5 s"), ("Disolución", "MX.disolver", "560 ms", "cambio de pantalla y de color"), ("Barrido", "clase mx-in", "500 ms", "paneles y tarjetas"),
    ("Confirmación", "check sobre acento", "520 ms", "fin de un paso"), ("Mantener", "MX.mantener", "1600 ms", "lo irreversible"), ("Alerta", "MX.alerta", "1300–1500 ms", "peligro y guardián")])
pag("20 MOVIMIENTO", seccion("20", "MOVIMIENTO", "Todo anima por pasos. Con «reducir movimiento» del sistema, aparece directo; en modo liviano, la disolución usa bloques grandes y no hay fallas al azar.", P3) +
    '<div class="g2"><div>' + mov + "</div><div>" + fig("lam-mov", "Decodificación por etapas y cuatro cuadros de la disolución por bloques.") + "</div></div>")

pag("21 PANTALLAS", seccion("21", "COMPOSICIÓN Y PANTALLAS", "Reglas de armado que se repiten en todo.", P3) + '<div class="g3">' +
    "".join(caja(t, x) for t, x in [
        ("GRILLA", "Puntos cada 22 px en el fondo; márgenes y separaciones en múltiplos (3, 6, 10, 14, 22, 44 px)."),
        ("MARCAS", "Escuadras en las 4 esquinas de la pantalla; cruces + en las áreas principales."),
        ("RIEL", "Pasos numerados 01…09 a la izquierda (arriba en teléfono); cuadrado lleno = hecho."),
        ("DECORACIÓN", "Máximo dos por pantalla: columna de letras, códigos hex, reloj."),
        ("VOZ DEL TEXTO", "Español de Chile, corto, en segunda persona. Estados de máquina: ESCUCHANDO, [ OK ] copiando sistema."),
        ("APARECER Y IRSE", "La interfaz aparece cuando se pide y se retira; no más de 3 paneles a la vez."),
    ]) + "</div>" + caja("SISTEMA DE DISEÑO VIVO", "Todo esto también está publicado como sistema de diseño («Neo Orión — Sistema de diseño») con tokens, componentes con vista en vivo y la librería <code>marathon.js</code> (window.NeoOrion), para que cualquier IA construya pantallas nuevas iguales."))

# =====================================================================================
P4 = "PARTE 4 · CONSTRUCCIÓN"
pag("22 LA DISTRO", seccion("22", "LA DISTRO", "Cómo se arma la ISO y qué lleva cada edición.", P4) + '<div class="g2"><div>' +
    tabla(["EDICIÓN", "PARA", "LLEVA"], [("full", "PC de trabajo", "KDE de respaldo, desarrollo, robótica (ROS 2 Jazzy), taller, IA, juegos, instalador"),
                                         ("lite", "notebook liviano", "sin robótica, IA ni juegos (se instalan después)"),
                                         ("patata", "PC que no rinde", "sin escritorio, quiosco cage + cog, zram, IA mínima + nube"),
                                         ("test", "probar la ISO", "lo mínimo para arrancar")]) +
    caja("PAQUETES", "<code>config/packages/</code>: base, desktop, dev, robotics, maker, ai, gaming, navegador, kiosko. Repos oficiales extra: Google Chrome, Mozilla, ROS 2, flatpak (GeForce NOW desde el repo de NVIDIA).") +
    "</div><div>" + caja("ARRANQUE", "<code>orion-sesion</code> + <code>orion-sesion.service</code>: desde el pendrive abre el instalador; instalado, el HUD. Servicios de usuario: <code>orion-oido</code> (escucha) y <code>orion-vigia</code> (rendimiento).") +
    caja("CAPAS", "Base neutra para regalar o vender. Capa personal opcional: identidad, módulos propios, notas.") +
    caja("PRUEBAS", "13 pruebas automáticas: MCP (2), voz, hablantes, forma, proyectos, LiDAR, juegos, instalación, rendimiento, puente, navegador (con Chrome real) e instalación de la capa.") + "</div></div>")

pag("23 LA ISO PROBADA", seccion("23", "LA ISO, PROBADA DE PUNTA A PUNTA",
    "Edición patata (1,4 GB) arrancada en una máquina virtual sin aceleración: el instalador real, la instalación en un disco y el equipo instalado arrancando solo.", P4) +
    '<div class="g3">' + fig("vm-01-inicio", "Arranque desde el pendrive: «firmware bios», equipo medido de verdad (2,9 GB, 1 disco).", "40mm") +
    fig("vm-03-equipo", "Escaneo real: nivel PATATA, IA recomendada qwen3:1.7b.", "40mm") + fig("vm-00-grub", "Menú de arranque de la ISO: probar o instalar, modo patata, modo seguro y cargar en RAM.", "40mm") + "</div>" +
    '<div class="g3">' + fig("vm-10-hud-instalado", "Disco instalado, sin pendrive: directo al HUD. El vigía bajó solo a modo liviano (12 cuadros/s).", "40mm") +
    fig("vm-11-drivers", "Controladores reales del equipo instalado (driver bochs-drm de la máquina virtual).", "40mm") +
    fig("vm-12-ric", "Calculadora RIC respondiendo desde el servidor del equipo instalado.", "40mm") + "</div>" +
    caja("LO QUE SE ENCONTRÓ Y SE CORRIGIÓ", "cog no existe en Ubuntu 24.04 → pantalla propia (GTK + WebKit) · casper renombraba al usuario → FLAVOUR · menú GRUB invisible en BIOS → letra cargada · barra de título de GTK → sin decoración · letras desde internet → incluidas · GRUB con root=/dev/loop0p3 → root=UUID · listas de apt borradas al retomar → apt update."))
pag("24 SEGURIDAD", seccion("24", "SEGURIDAD Y PERMISOS", "Poder completo con frenos donde importan.", P4) + '<div class="g2"><div>' +
    tabla(["RIESGO", "EJEMPLO", "QUÉ PASA"], [("LEER", "listar archivos, leer memoria", "se hace"), ("ESCRIBIR", "crear un archivo en el espacio de trabajo", "se hace y queda registrado"),
        ("SISTEMA", "instalar paquetes, clonar", "se hace (modo libre) o pregunta (modo asistido)"), ("IRREVERSIBLE", "borrar fuera del espacio, formatear", "ticket: solo la persona aprueba"),
        ("FÍSICO", "motores, PLC, máquinas", "ticket: solo la persona aprueba")]) + "</div><div>" +
    caja("LO QUE NO HACE", "No integra fuentes de juegos crackeados ni IPTV sin licencia. No maneja por código los chats web de IA (sus términos lo prohíben). Las claves de servicios en la nube las pega la persona; nunca pasan por un chat ni por el instalador.") +
    caja("RED", "El control remoto del navegador escucha solo en 127.0.0.1 y con un perfil propio. El MCP corre en el equipo; conectar otra IA es decisión de la persona.") + "</div></div>")

pag("25 PENDIENTES", seccion("25", "PENDIENTES", "Lo que falta para que la ISO arranque en un PC real.", P4) + tabla(["#", "PENDIENTE", "POR QUÉ IMPORTA"], [
    ("1", "Probar en hardware real: PC patata, notebook UEFI con arranque seguro, torre con NVIDIA", "aquí se probó en máquina virtual BIOS sin aceleración"),
    ("2", "Compilar las ediciones full y lite en un equipo con acceso a los repos de Google, Mozilla y ROS", "este entorno no llega a esos servidores"),
    ("3", "Primer arranque con internet: Ollama, modelos, oído (Vosk/Whisper), voz y driver NVIDIA", "se bajan solos; falta verlos en un equipo real"),
    ("4", "Publicar un canal de actualizaciones (canal.json + .deb) en GitHub Releases", "el mecanismo está listo y probado"),
    ("5", "Estudio de video, orion_cuerpo (maniquí, medidas, dieta) y visores de cartón para la CNC", "acordados, aún sin construir"),
], ["5%", "55%", "40%"]))

glos = [("ACENTO", "el color de la persona; manda en toda la interfaz"), ("CAPA", "conjunto de cosas personales que se activa aparte de la base"), ("CDP", "protocolo para manejar Chrome desde código"),
        ("GUARDIÁN", "parte que decide si una orden física se ejecuta"), ("HUD", "la interfaz que aparece sobre la pantalla vacía"), ("MCP", "protocolo con que una IA usa herramientas del equipo"),
        ("MARIONETTE", "vía oficial para manejar Firefox desde código"), ("MoE", "modelo de «mezcla de expertos»: grande pero rápido"), ("ORION-FIT", "programa que dice qué IA cabe en el equipo"),
        ("PATATA", "estado de un equipo que no está rindiendo"), ("PSI", "medida del kernel de cuánto se espera por memoria o CPU"), ("STT / TTS", "voz a texto / texto a voz"),
        ("VAD", "detector de cuándo alguien está hablando"), ("ABLITERADO", "modelo al que se le quitó la «dirección de rechazo»")]
pag("26 GLOSARIO", seccion("26", "GLOSARIO", "Palabras que aparecen en el sistema.", P4) + '<div class="g2">' + tabla(["PALABRA", "QUÉ ES"], glos[:7]) + tabla(["PALABRA", "QUÉ ES"], glos[7:]) + "</div>")

# =====================================================================================
PA = "ANEXOS"
def estrellas(n):
    return f'<span class="b5">{"".join("<i class=on></i>" if i < n else "<i></i>" for i in range(5))}</span>'
L = CAT["locales"]
filas = [(f"<b>{e(m['id'])}</b>", e(m["de"]), e(m["origen"]), f"{m['params_b']}B · {m['memoria_gb']} GB", {"con": "con", "neutral": "neutral", "sin": "<span class=am>SIN</span>"}[m["filtro"]],
          estrellas(m["velocidad"]), estrellas(m["calidad"]), e(m["para"])) for m in L]
for k in range(0, len(filas), 12):
    pag("ANEXO A · CATÁLOGO DE IA", (seccion("A", "CATÁLOGO DE IA LOCALES", "Velocidad en el equipo para el que está pensada y calidad general, de 1 a 5. Revisado el 3-11-2026; el catálogo se actualiza solo.", PA) if k == 0 else "") +
        tabla(["MODELO", "DE", "ORIGEN", "TAMAÑO", "FILTRO", "VELOCIDAD", "CALIDAD", "PARA"], filas[k:k + 12], ["17%", "14%", "9%", "11%", "6%", "9%", "9%", "25%"]))
N = CAT["nube"]
pag("ANEXO A · CATÁLOGO DE IA", tabla(["IA EN LA NUBE", "DE", "ORIGEN", "FILTRO", "CALIDAD", "PARA", "PRECIO"],
    [(f"<b>{e(m['nombre'])}</b>", e(m["de"]), e(m["origen"]), e(m["filtro"]), estrellas(m["calidad"]), e(m["para"]), e(m["precio"])) for m in N[:12]], ["16%", "13%", "8%", "9%", "9%", "30%", "15%"]))
pag("ANEXO A · CATÁLOGO DE IA", tabla(["IA EN LA NUBE", "DE", "ORIGEN", "FILTRO", "CALIDAD", "PARA", "PRECIO"],
    [(f"<b>{e(m['nombre'])}</b>", e(m["de"]), e(m["origen"]), e(m["filtro"]), estrellas(m["calidad"]), e(m["para"]), e(m["precio"])) for m in N[12:]], ["16%", "13%", "8%", "9%", "9%", "30%", "15%"]) +
    '<div class="g2" style="margin-top:12px"><div>' + tabla(["ABIERTAS PERO GIGANTES", "DE", "NOTA"], [(f"<b>{e(x['nombre'])}</b>", e(x["de"]), e(x["nota"])) for x in CAT["solo_servidor"]]) +
    "</div><div>" + caja("APPS", "".join(f"<b>{e(a['nombre'])}</b> — {e(a['de'])}<br>{e(a['para'])}<br><span class=dim>{e(a['nota'])}</span>" for a in CAT["apps"])) + "</div></div>")

for k in range(0, len(HERR), 15):
    pag("ANEXO B · HERRAMIENTAS MCP", (seccion("B", "HERRAMIENTAS MCP (ORION-LINUX)", "Lo que cualquier IA conectada puede pedirle al equipo.", PA) if k == 0 else "") +
        tabla(["HERRAMIENTA", "QUÉ HACE"], [(f"<code>{h[0]}</code>", e(h[1])) for h in HERR[k:k + 15]], ["22%", "78%"]))

half = (len(CMDS) + 1) // 2
pag("ANEXO C · COMANDOS", seccion("C", "COMANDOS DE VOZ DE EJEMPLO", "Se dicen o se escriben con la tecla /. Hay muchos más: la IA entiende variaciones.", PA) + '<div class="g2">' +
    tabla(["DI…"], [(f"«{e(c)}»",) for c in CMDS[:half]]) + tabla(["DI…"], [(f"«{e(c)}»",) for c in CMDS[half:]]) + "</div>")

colt = [(f"<span class=sw style='background:{t['value']}'></span> <code>{t['name']}</code>", t["value"], e(t["usage"])) for t in TOK["color"]["tokens"]]
pag("ANEXO D · TOKENS", seccion("D", "TOKENS DE DISEÑO", "Los valores exactos. La fuente es tokens.json del sistema de diseño.", PA) + tabla(["COLOR", "VALOR", "USO"], colt[:12], ["22%", "10%", "68%"]))
pag("ANEXO D · TOKENS", tabla(["COLOR", "VALOR", "USO"], colt[12:], ["22%", "10%", "68%"]) + '<div class="g2" style="margin-top:10px">' +
    tabla(["ESPACIO", "VALOR", "USO"], [(f"<code>{t['name']}</code>", t["value"], e(t["usage"])) for t in TOK["spacing"]["tokens"]], ["20%", "12%", "68%"]) +
    tabla(["PÍXEL / DURACIÓN", "VALOR", "USO"], [(f"<code>{t['name']}</code>", t["value"], e(t["usage"])) for t in TOK["pixel"]["tokens"] + TOK["duracion"]["tokens"]], ["24%", "12%", "64%"]) + "</div>")

# =====================================================================================
CSS = (B / "fuentes.css").read_text() + """
@page{size:297mm 210mm;margin:0}
:root{--acc:#ff2438;--bg:#0d0e10;--panel:#141518;--line:#2a2c30;--txt:#e6e8ea;--grey:#8b8e94;--dim:#55585e;--pel:#ffd400}
*{box-sizing:border-box}html,body{margin:0;background:var(--bg);color:var(--txt);font-family:'JetBrains Mono',monospace;font-size:8.6pt;line-height:1.45;-webkit-print-color-adjust:exact;print-color-adjust:exact}
.pg{width:297mm;height:210mm;position:relative;overflow:hidden;page-break-after:always;padding:15mm 13mm 13mm;
 background:var(--bg);background-image:radial-gradient(#1c1e21 0.7px,transparent 0.9px);background-size:5.8mm 5.8mm}
.hd{position:absolute;left:13mm;right:13mm;top:6mm;display:flex;align-items:center;gap:10px;font-family:Silkscreen;font-size:7.5pt;color:var(--grey);letter-spacing:.08em}
.hd b{background:var(--acc);color:#111;padding:1px 6px;font-weight:400}.hd span{margin-left:auto}
.ft{position:absolute;left:13mm;right:13mm;bottom:5.5mm;display:flex;justify-content:space-between;font-family:Silkscreen;font-size:6.5pt;color:var(--dim);letter-spacing:.08em;border-top:1px solid var(--line);padding-top:2mm}
.reg{position:absolute;width:4mm;height:4mm;border-color:#3a3d42}.r1{left:4mm;top:4mm;border-left:1px solid;border-top:1px solid}.r2{right:4mm;top:4mm;border-right:1px solid;border-top:1px solid}
.r3{left:4mm;bottom:4mm;border-left:1px solid;border-bottom:1px solid}.r4{right:4mm;bottom:4mm;border-right:1px solid;border-bottom:1px solid}
.sh{display:flex;align-items:baseline;gap:12px;margin:0 0 2mm}.sn{font-family:Silkscreen;font-size:22pt;color:var(--acc)}
h1{font-family:Silkscreen;font-weight:400;font-size:20pt;margin:0;color:#fff;letter-spacing:.02em;line-height:1}
.lead{color:var(--grey);font-size:9.5pt;max-width:230mm;margin:0 0 5mm}
.g2{display:grid;grid-template-columns:1fr 1fr;gap:5mm}.g3{display:grid;grid-template-columns:1fr 1fr 1fr;gap:4mm}
figure{margin:0 0 3mm;border:1px solid var(--line);background:#0a0b0c;padding:2mm}
figure img{width:100%;max-height:72mm;object-fit:contain;display:block;margin:0 auto}
figcaption{font-size:7.4pt;color:var(--grey);margin-top:1.5mm}figcaption b{font-family:Silkscreen;font-weight:400;color:var(--acc);margin-right:4px}
.caja{border:1px solid var(--line);background:var(--panel);padding:3mm 3.5mm;margin-bottom:3mm;position:relative}
.caja::before{content:"";position:absolute;left:-1px;top:-1px;width:2.4mm;height:2.4mm;border-left:2px solid var(--acc);border-top:2px solid var(--acc)}
.ct{font-family:Silkscreen;font-size:8pt;color:var(--acc);letter-spacing:.06em;margin-bottom:1.5mm}
.caja ol{margin:0;padding-left:4.5mm}.caja li{margin-bottom:1mm}
table{width:100%;border-collapse:collapse;margin-bottom:3mm;table-layout:fixed}
th{font-family:Silkscreen;font-weight:400;font-size:7pt;letter-spacing:.06em;text-align:left;background:var(--acc);color:#111;padding:1.6mm 2mm}
td{border-bottom:1px solid var(--line);padding:1.4mm 2mm;vertical-align:top;font-size:7.8pt;word-wrap:break-word}
tr:nth-child(even) td{background:#111214}
code{color:var(--acc);font-size:7.6pt}.dim{color:var(--grey)}.am{color:var(--pel);font-family:Silkscreen;font-size:7pt}
.b5{display:inline-flex;gap:1px}.b5 i{width:2.2mm;height:1.8mm;background:#26282c;display:block}.b5 i.on{background:var(--acc)}
.sw{display:inline-block;width:4mm;height:3mm;vertical-align:middle;border:1px solid #3a3d42}
.diag{display:flex;align-items:stretch;gap:2mm}.dc{flex:1;border:1px solid var(--line);background:var(--panel)}.da{font-family:Silkscreen;color:var(--acc);align-self:center;font-size:12pt}
.db{font-family:Silkscreen;font-size:8pt;padding:2mm;background:#1d1f22;letter-spacing:.05em}.db.acc{background:var(--acc);color:#111}.db.pel{background:var(--pel);color:#111}
.dd{padding:2.5mm;font-size:7.6pt;color:var(--grey)}.diag2{display:grid;grid-template-columns:2fr 1.3fr 1.3fr;gap:2mm;margin-top:2mm}.diag2 .db{border:1px solid var(--line)}
.portada{display:grid;grid-template-columns:1.6fr 1fr;gap:10mm;height:100%}
.pl{border:1px solid var(--line);padding:18mm 14mm;display:flex;flex-direction:column;justify-content:center;background:#0b0c0d}
.logo canvas{display:block;margin:0 0 4mm;image-rendering:pixelated}
.kick{font-family:Silkscreen;font-size:8pt;color:var(--grey);letter-spacing:.08em;margin-top:8mm}
.frase{font-size:15pt;color:#fff;font-weight:600;margin:4mm 0;line-height:1.3}
.pr{border-left:2px solid var(--acc);padding-left:6mm;padding-top:4mm}
.pr .ct{font-size:10pt;margin-bottom:3mm}.toc{font-size:7.2pt;letter-spacing:.02em;line-height:1.55}.toc .pt{font-family:Silkscreen;color:var(--acc);font-size:7.5pt;margin-top:2.5mm;letter-spacing:.06em}
.toc div{display:flex;gap:6px}.toc span:last-child{margin-left:auto;color:var(--dim)}
.sin .hd,.sin .ft{display:none}
"""

out = []
paginas_toc = {}
for i, (sec, cuerpo, clase) in enumerate(PAGES, start=1):
    import re
    m = re.search(r'<span class="sn">([^<]+)</span>', cuerpo)
    if m: paginas_toc.setdefault(m.group(1), i)
    hd = f'<div class="hd"><b>NEO ORIÓN</b>{e(sec)}<span>LA BIBLIA DE NEO ORIÓN</span></div>'
    ft = f'<div class="ft"><span>NEO ORIÓN / LA BIBLIA DE NEO ORIÓN / {ED} / GPL-3.0</span><span>{i:02d}</span></div>'
    out.append(f'<section class="pg {clase}"><i class="reg r1"></i><i class="reg r2"></i><i class="reg r3"></i><i class="reg r4"></i>{hd}{cuerpo}{ft}</section>')

toc_html, parte = "", None
for num, tit, pt in TOC:
    if pt != parte:
        toc_html += f'<div class="pt">{pt}</div>'; parte = pt
    toc_html += f'<div><span>{num}</span><span>{e(tit)}</span><span>{paginas_toc.get(num, "")}</span></div>'

doc = f"""<!doctype html><html lang="es"><head><meta charset="utf-8"><title>La biblia de Neo Orión</title><style>{CSS}</style></head><body>
{''.join(out)}
<script src="file://{R}/hud/marathon.js"></script>
<script>MX.acento('#ff2438',false);
document.getElementById('toc').innerHTML = {json.dumps(toc_html)}; document.getElementById('toc').className='toc';
const l=document.getElementById('logo');l.append(MX.pixel('NEO',{{px:13,color:'#ffffff',anim:false}}),MX.pixel('ORIÓN',{{px:13,anim:false}}));
document.querySelectorAll('.logo canvas').forEach(c=>{{c.style.width=(c.width*0.62)+'px'}});
</script></body></html>"""
(B / "biblia.html").write_text(doc)
print("páginas:", len(PAGES), "figuras:", FIG[0])
