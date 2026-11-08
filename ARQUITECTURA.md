# Neo Orión OS — Arquitectura v0.4 · laboratorio inteligente

Sistema operativo **de voz**: no hay escritorio. Hablas, la IA hace el trabajo por detrás
y solo aparece una interfaz (estilo O.R.C.A. / Marathon) cuando pides ver algo.

## Capas
| Capa | Qué hace | Tecnología propuesta |
|---|---|---|
| Base | Ubuntu 24.04 LTS (x86_64 y arm64) | debootstrap + casper (`build.sh`) |
| Oído | Micrófono siempre activo, sin botón | VAD (Silero) + Whisper (`orion-fit` elige el tamaño) |
| Voz | Respuesta hablada | Piper TTS |
| Cerebro | Varios LLM: local + nube | Ollama (modelo según `orion-fit`) + Claude/API |
| Memoria | Todo lo hablado/hecho → documentos | Markdown + índice vectorial local (SQLite) |
| HUD | Paneles generados a pedido | `hud/index.html` en navegador kiosko (cage + Chromium) |
| Movimiento | IA habla → traductor → guardián → motores | `orion/orion_motion` (Python) + ROS 2 Jazzy |
| Flota | PC central configura Raspberry headless | imagen arm64 + sincronización por SSH/ROS 2 |

## La IA controla Linux entero: `orion-mcp`
El sistema operativo ES un servidor MCP (`orion/orion_mcp/server.py`). Cualquier IA compatible
(Claude, o un modelo local con un cliente MCP) trabaja por código, sin mouse ni ventanas:
`sistema_info · ejecutar · leer/escribir_archivo · clonar_repo · instalar · cad_construir ·
cad_modificar · robot_orden · plc_escribir · hud_mostrar · memoria_anotar · identidad`.
Permisos: todo el software sin preguntar (modo "libre"); solo esperan un "sí" humano lo
IRREVERSIBLE (borrar el sistema, discos) y lo FÍSICO (PLC, máquinas). La IA no puede aprobarse sola.

## Dónde corre y qué ve (`orion_core/detect.py`)
- Hosts (ejecutan Neo Orión): torre, notebook, mini-PC, Raspberry Pi, Jetson, Android con Termux.
- Periféricos (ojos/oídos/manos): ESP32/Arduino/Pico/STM32 por USB, cámaras, micrófonos,
  teléfonos (adb / app), lentes Meta vía teléfono, PLC por red. iPhone y ESP32 no ejecutan Linux.

## CAD por código
build123d (núcleo OpenCascade, Apache 2.0) → STEP + STL. Parámetros editables por voz
(`cad_modificar`): "reduce el grosor a 3" regenera la pieza y marca el cambio.

## Primer arranque
Login por voz → `orion-fit` elige el modelo → descarga del stack (Ollama, Whisper, Piper, ROS 2)
→ reinicio → todo listo. Extensiones = `instalar(...)` o servidores MCP adicionales.

## Cada equipo usa lo que aguanta (`orion_core/perfil.py`)
Niveles micro (Pi 4) · ligero (Pi 5 / 8 GB) · medio (16 GB CPU) · potente (GPU 8 GB+) · estación (GPU 20 GB+).
| Pieza | micro | ligero | medio | potente | estación |
|---|---|---|---|---|---|
| Oído | Vosk | Moonshine / whisper.cpp | faster-whisper small | whisper large-v3-turbo | + diarización |
| Voz | espeak-ng | Piper | Piper / Kokoro | Kokoro / XTTS | XTTS · ElevenLabs con tu clave |
| Índice memoria | SQLite FTS5 | + embeddings mini | + nomic-embed | LanceDB | Qdrant |
| Foto → 3D | silueta (CPU) | silueta | TripoSR | Hunyuan3D 2.1 | TRELLIS.2 |
| Escaneo 3D | enviar fotos | enviar fotos | COLMAP+OpenMVS / Meshroom | Gaussian splatting | idem |
Se fuerza cualquier elección en `~/.orion/backends.json`. Licencias no comerciales quedan marcadas.

## Memoria (`orion_memoria`)
Carpeta `~/Orion` en Markdown con frontmatter y [[enlaces]]: se abre con Obsidian, Logseq, SilverBullet
o Zettlr, y se importa en Joplin, Trilium o SiYuan. Diario por día (todo lo escuchado), notas por
proyecto/tema/persona/robot. Índice SQLite FTS5 + búsqueda por significado si hay Ollama. Cualquier IA
la lee con `memoria_contexto`; cambiar de modelo no pierde nada.

## Oído siempre activo (`orion_voz`)
Micrófono → VAD combinado (WebRTC + energía) → frase → STT → diario. "Orión, …" activa la IA y abre
una ventana de 25 s de conversación. "Orión, pausa" deja de registrar hasta "Orión, sigue".
Servicio: `orion-oido.service`. TTS: Piper, Kokoro, espeak-ng o ElevenLabs.

## Forma (`orion_forma`)
Idea → imágenes de alternativas → malla (TRELLIS.2 / Hunyuan3D / TripoSR) o silueta a escala (CPU)
→ sólido build123d (STEP+STL) → bolsillos de servos (MG996R, SG90, STS3215, NEMA17) → vista previa.

## Proyectos (`orion_proyectos`)
EDT, CPM con ruta crítica, Carta Gantt (SVG + Excel), RACI, avance real vs. planificado y presupuesto
chileno (CD → GG → utilidad → contingencia → IVA 19 %). Se guarda como JSON + nota en la memoria.

## Módulos (`config/modulos.json`)
- `orion_mcp/config.py` genera la configuración para Claude Code, OpenCode o Claude Desktop.

## Las dos mentes
1. **IA** (no determinista): entiende, planifica, habla en palabras.
2. **Traductor** (Python, determinista): reconoce la frase por aproximación → `SET A2 45`.
3. **Guardián** (en el robot, sin IA): límites, velocidad, piso, zonas prohibidas, frenado.
   La IA puede *pedir* saltarse una regla; solo una persona puede *autorizarlo*.
   El paro de emergencia físico corta la energía y no depende del software.

## Movimiento por onda
θ(t) = c + A·sin(2π·f·k·t) · velocidad angular pico = 2π·f·k·A (debe ser ≤ vel. máx. del servo).
Multiplicar la ecuación entera cambia la amplitud; la velocidad se cambia con la frecuencia (k).

## Calibración
Foto de referencia → marcar cada servo (A1…A6) y la vista → prueba de rango 0/45/90°
→ cámara mide ángulo real → ajuste lineal (offset, sentido, escala) → queda guardado
en `~/.orion/robots/<robot>/calibracion.json`.

## Estado
- [x] `orion-fit` (hardware → modelo) probado
- [x] `orion_motion` (traductor, onda, cinemática DH, guardián, memoria/calibración) probado
- [x] Prototipo del HUD (login por voz, CAD 3D, tareas en segundo plano, QR)
- [x] Memoria Markdown multi-app, oído siempre activo, forma 3D, proyectos, perfil por nivel — probados
- [x] `orion-mcp` (24 herramientas) + `orion-detect` + permisos, probados
- [ ] Etapas del build de la ISO (base, voz, HUD kiosko, imagen Raspberry)
- [ ] Servicio de voz siempre activo + memoria documental
- [ ] Driver real de servos (Feetech STS / PCA9685) y puente ROS 2

## Juegos y medios (`orion_juegos`, herramienta MCP `jugar`)
- **Local:** Steam + Proton, Heroic (Epic/GOG/Amazon), Lutris, RetroArch con juegos propios.
- **Nube (todas):** GeForce NOW (app nativa oficial), Xbox Cloud, Boosteroid (app nativa), Shadow PC (.deb oficial), Blacknut y Amazon Luna (no está en Chile). Cada una lleva su disponibilidad en Chile; PlayStation Plus no tiene cliente Linux: se ofrece Shadow o Remote Play con Chiaki-ng.
- **Casa:** Sunshine + Moonlight, Steam Link y Remote Play de PS4/PS5 (Chiaki-ng).
- **Medios:** Jellyfin (biblioteca propia), Kodi y las plataformas oficiales como app web.
- **Entrada:** teclado, mouse y mandos Xbox/PlayStation/Nintendo/8BitDo/genéricos detectados solos; xpadneo, AntiMicroX, GameMode y MangoHud.
- Decide local, nube o casa según la GPU y la red del equipo. **No** integra fuentes de descarga de juegos crackeados ni IPTV sin licencia.
- Bandera de compilación `WITH_GAMING` (desactivada en las ediciones lite/test).

## Instalador (`hud/instalar.html` + `orion_core/instalacion.py`)
Nueve pasos con la estética Marathon: idioma → escaneo del equipo → disco (borrar exige mantener apretado) → IA locales según lo que cabe (misma regla que `orion-fit`) y nubes opcionales → oído, voz y app de memoria → nombre, voz y **color** → módulos y capa (limpia o personal) → cuenta con registro de voz → instalación. Genera un JSON que `instalacion.py` valida y convierte en pasos (descargar IA, identidad, módulos). Las claves de nube nunca pasan por el instalador.
El color de acento es uno solo para todo (`marathon.js` → `MX.acento`), se cambia por voz: «cambia el color a magenta».

## Sistema de diseño y PC patata
- Sistema de diseño publicado aparte ("Neo Orión — Sistema de diseño"): tokens, pantallas, movimiento, modo patata y componentes pixel. `hud/marathon.js` es su librería (`window.MX` / `NeoOrion`): fuente de bloques 5×5 con tildes, iconos de bloques, disolución por bloques, alerta roja, check de confirmación.
- **Edición patata** (`EDITION=patata`): 2–4 GB, CPU sin AVX2, sin escritorio; HUD en `cage` + `cog`, zram, Vosk + espeak, IA local mínima + nube. `orion-fit` y `perfil.nivel()` detectan el nivel `patata`; el HUD entra solo en modo liviano (sin 3D, sin brillos) y three.js se descarga solo al abrir el CAD.
- **Catálogo de IA** (3-nov-2026): 24 modelos locales de 270M a 120B en `models-fallback.json`, gigantes abiertos marcados "solo servidor", 19 proveedores en la nube; `orion-fit --actualizar` revisa el catálogo en vivo de Ollama y avisa de familias nuevas.

## Patata por rendimiento, catálogo de IA, chat y arranque desde el pendrive
- **Patata = el equipo no está rindiendo**, no una ficha. `orion_core/rendimiento.py` (servicio `orion-vigia`, herramienta MCP `rendimiento`) mide presión de memoria y CPU, temperatura, batería y tokens/s de la IA; el HUD mide sus FPS. Baja a LIVIANO/PATATA en ~15 s, sube de a un nivel en ~2 min, y avisa por qué. "modo completo" lo fija.
- **`config/ia-catalogo.json`**: fuente única (instalador, `orion-fit`, explicación al cliente). 34 locales con filtro / neutral / sin filtro (Dolphin, Hermes, abliterados de huihui-ai), DeepSeek, Qwen, GLM, Gemma, Llama, Phi, gpt-oss…; 19 en la nube (EE.UU., China, Europa); 12 gigantes solo servidor; apps: **Odysseus** (PewDiePie, interfaz, no modelo). Cada una con velocidad, calidad, memoria, licencia, origen. `orion-fit` recomienda con filtro por defecto; `--sin-filtro` elige entre los sin filtro.
- **Chat**: pestaña CHAT abajo a la izquierda y teclas `/`, `Ctrl+Espacio` o `F2`; `Esc` lo esconde. Si el equipo no tiene micrófono, el chat queda abierto. Historial de la conversación sobre el campo.
- **Arranque**: `orion-sesion` + `orion-sesion.service`. Desde el pendrive (`boot=casper`) abre el **instalador** a pantalla completa; ya instalado, el HUD. Compositor `cage` + navegador `cog`.

## Colaboración entre IAs (`orion_puente`, herramienta MCP `colaborar`)
- **Ida**: "usa Claude para crear el documento…" → paquete de contexto (tarea + conversación + memoria) enviado por la vía **oficial** de cada IA con la cuenta de la persona: Claude Code (`claude -p`, plan Pro/Max), Gemini CLI, Codex CLI (cuenta ChatGPT), OpenCode u Ollama. La respuesta queda en `colaboraciones/` de la memoria.
- **Web manual**: abre el chat web con el encargo copiado; la persona aprieta enviar y pega la respuesta. No se automatiza el clic en la web: los términos de Claude (y de otros) prohíben el acceso automatizado al producto de consumo.
- **Vuelta**: `config_vuelta()` conecta el MCP de Neo Orión a Claude (escritorio o Claude Code) para que desde ese chat Claude le pida cosas al PC; lo físico y lo irreversible siguen pidiendo confirmación.

## Navegador (`orion_navegador`, herramienta MCP `navegador`)
- **Chrome y Firefox normales** (pestañas, barra, extensiones), desde sus repositorios oficiales (`config/repos/`). "abre chrome", "abre firefox github.com", "busca en google …".
- **Dos modos sobre el MISMO navegador visible**: MANEJAS TÚ (como siempre) y MANEJA NEO ORIÓN ("maneja tú"): abre pestañas, navega, hace clic y escribe a la vista, con una franja de aviso y un cursor de bloque que muestra dónde va a tocar. Mover el mouse o decir «yo sigo» devuelve el control.
  - Chrome: perfil propio de Neo Orión con depuración remota solo en 127.0.0.1 → Playwright `connect_over_cdp`.
  - Firefox: `--marionette` + geckodriver `--connect-existing` → Selenium. No se cierra el Firefox de la persona al soltar.
- Automatización en segundo plano con Playwright para tareas largas.
- Chats de IA (claude.ai, chatgpt.com, gemini…) no se manejan por código; se abren a la vista o se usa la extensión oficial Claude in Chrome.


## La ISO: cómo se construye (`build.sh` + `stages/`)
`sudo EDITION=patata ./build.sh` (o full | lite | test). Etapas: **00** revisa espacio · **10** Ubuntu 24.04 mínimo
con debootstrap (fuentes deb822, sin recomendados) · **20** núcleo, firmware, arranque, red, sonido, herramientas y,
en patata, el quiosco (cage + orion-pantalla) · **30** repos oficiales de terceros (Chrome, Mozilla, ROS 2; si uno
no responde se avisa y se sigue, `ORION_ESTRICTO=yes` para fallar) · **40** módulos por edición · **50** Neo Orión
(/opt/orion con su venv y MCP, HUD e instalador en /usr/share/orion, servicios, capa personal solo con `CAPA=<nombre>`) ·
**60** sesión del pendrive (usuario `orion` con sudo, casper.conf con FLAVOUR para que casper no lo renombre,
initrd regenerado) · **80** limpieza · **90** ISO híbrida: squashfs, menú GRUB propio (probar/instalar, modo patata,
modo seguro, cargar en RAM) con fondo pixel, UEFI con shim y grub firmados por Canonical (arranque seguro), BIOS
con El Torito y MBR híbrido para pendrive. `--from NN` retoma; `ORION_CA` agrega un certificado si se compila
detrás de un proxy. Probada: edición patata 1,3 GB, arranca en QEMU hasta la sesión de Neo Orión.

## Instalador real (`orion_core/instalador.py` + `orion_core/servidor.py` + `orion-pantalla`)
Al arrancar desde el pendrive, `orion-sesion` levanta el servidor local (127.0.0.1:7700, solo con la cabecera
`X-Orion`) como root y abre `instalar.html` en `orion-pantalla` (GTK + WebKit sin barras, dentro de cage; `cog` no
existe en Ubuntu 24.04). La interfaz pide `/api/equipo` (RAM, CPU, GPU, discos reales sin el pendrive), graba las 3
frases de voz con arecord y manda la configuración a `/api/instalar`. El instalador: revisa todo, particiona
(**borrar**: GPT EFI 512 MB + BIOS 1 MB + raíz; **junto**: usa el espacio libre ≥ 20 GB y la EFI de Windows sin
formatearla), formatea, copia el squashfs con avance, escribe fstab/hostname/idioma/zona/teclado, swap (4 GB con
≤ 8 GB de RAM), crea la cuenta (la clave llega por variable de entorno y nunca queda en disco ni en comandos), saca
casper, deja la identidad y la sesión con la cuenta nueva, y GRUB en UEFI (con nombre + ruta de respaldo) y BIOS.
Lo que no cabe en el pendrive (Ollama, modelos, oído, voz, MCP) lo baja `orion-primer-arranque` apenas hay internet,
reintentando cada 10 min. Ya instalado, el mismo servidor sirve el HUD y `/api/chat` (IA local con identidad y memoria).

## Cuadernos: Gemini Notebook (ex NotebookLM) y Open Notebook (`orion_cuaderno`, herramienta `cuaderno`)
Google renombró NotebookLM a Gemini Notebook el 16-jul-2026 (misma dirección). No tiene API para cuentas personales
y no se maneja por código: Neo Orión arma las fuentes (memoria, proyecto, investigación) en
`~/Orion/exportar/cuaderno/<fecha-tema>/` con una guía, abre la página y la persona las sube; lo que responda se
pega y queda en la memoria. Open Notebook (lfnovo, MIT) es la alternativa local con Ollama y API REST: ahí Neo Orión
crea el cuaderno y carga las fuentes sola.

## Herramientas del taller y la casa (9 módulos, panel en el HUD, herramienta MCP y prueba en `test_ideas.py`)
| Módulo | Qué hace |
|---|---|
| `orion_electrica` | Calculadora RIC por voz: sección por caída (≤ 3 % alimentadores y subalimentadores, ≤ 5 % total, RIC N°03 5.1.2/5.1.3), mínimos 4 y 2,5 mm², protección In ≥ Ib; Iz se toma del RIC N°04 o del fabricante (no se inventa). |
| `orion_respaldo` | Fotos diarias con rsync --link-dest a disco externo (detectado solo), carpeta o nube con rclone; guarda 30; restaurar sin pisar. Timer de usuario 21:30. |
| `orion_remoto` | En la casa: wayvnc con clave y TLS sobre cage. Desde afuera: RustDesk (ID). «apaga el acceso remoto». |
| `orion_cnc` | Revisa G-code sin mover (medidas, tiempo, profundidad, si cabe, pulgadas/mm), vista del recorrido y GRBL por USB (estado, jog, casa, cero, pausa, parada Ctrl-X). Mover y cortar piden ticket. |
| `orion_vision` | Frigate + MQTT: configuración sin claves en texto (variables FRIGATE_RTSP_*), persona en zona robot/cnc → el guardián frena. |
| `orion_casa` | Home Assistant por voz (luces, enchufes, clima, sensores); cerraduras, alarmas y portones con confirmación; token en ~/.orion/claves. |
| `orion_dictado` | Dictado → Word con marcas («título», «sección», «punto», «tabla»); plantillas informe DUOC (logo arriba a la derecha), memoria eléctrica, carta, acta. |
| `orion_actualizar` | Parches de Ubuntu + canal propio de Neo Orión (`canal.json` con .deb y SHA-256; si la suma no calza no instala). `construir_deb` arma `neo-orion-core_<v>_all.deb`. |
| `orion_traductor` | Conversación en vivo entre dos idiomas: Argos Translate sin internet o IA local; voz por parlantes o audífonos de conducción ósea. |

## Controladores: procesador y gráfica al día (`orion_core/controladores.py`, herramienta `controladores`)
Detecta el procesador (`/proc/cpuinfo`: fabricante, modelo, AVX2/AVX-512) y cada gráfica por su ID PCI
(`lspci -nn`: 10de NVIDIA, 1002 AMD, 8086 Intel; máquinas virtuales aparte) con el driver en uso. Plan: microcódigo
Intel/AMD; NVIDIA → el driver que Ubuntu recomienda para ese modelo (`ubuntu-drivers`, firmado para arranque seguro,
habilita CUDA); AMD → amdgpu + Mesa (Vulkan, VA-API); Intel → i915/xe + Mesa + intel-media-va-driver; núcleo HWE si
el hardware es muy nuevo. El instalador guarda el plan en `/etc/orion/controladores.json` y `orion-primer-arranque`
lo aplica con internet. Al día: los drivers se actualizan con el sistema; «revisa los drivers» avisa si hay un driver
NVIDIA recomendado más nuevo; el firmware y el BIOS (fwupd/LVFS) solo se informan y se instalan con permiso.

## Llave de voz: tu frase con tu voz (`orion_voz/llave.py`)
Como «Oye Siri», pero la frase la elige la persona (instalador, paso «Tu cuenta», o «cambia mi frase»). Se graba 3
veces; se guarda el promedio de la huella (sherpa-onnx/ECAPA si está, si no la espectral) y un umbral calibrado con
lo parecidas que salieron las 3. Para abrir hace falta que calcen las dos cosas: la frase (oído, tolerante a errores)
y la voz (parecido ≥ umbral). Sirve para despertar y desbloquear el HUD («bloquea»), reconocer al dueño y, junto al
botón de mantener, aprobar lo físico o irreversible. La clave de Linux sigue siendo el respaldo (la pantalla de
bloqueo la acepta con `unix_chkpwd`) y lo irreversible nunca se aprueba solo con la voz. La huella vive solo en
`~/.orion/llave-voz.json` (600); «olvida mi voz» la borra.

## Prueba de punta a punta (3-nov-2026)
ISO patata (1,4 GB) en QEMU sin aceleración: arranca, abre el instalador con el equipo medido de verdad (2,9 GB,
1 disco, BIOS), recorre los 9 pasos y confirma el borrado. El instalador real se corrió sobre un disco de 20 GB
(loop): GPT EFI 512 MB + BIOS 1 MB + raíz, sistema copiado, cuenta con clave cifrada (la clave no queda en ningún
archivo), GRUB en el MBR con `root=UUID`, restos del pendrive borrados; en 2 min. Ese disco, arrancado solo, entra
directo al HUD con la cuenta nueva, el vigía baja a modo liviano por rendimiento (12 cuadros/s) y responden el panel
de controladores (driver real `bochs-drm`) y la calculadora RIC por el servidor del equipo instalado.
Encontrado y corregido en la prueba: `cog` no existe en 24.04 (→ orion-pantalla), casper renombraba al usuario
(→ FLAVOUR), GRUB en BIOS sin letra (→ loadfont), barra de título de GTK, letras desde internet (→ locales),
`root=/dev/loop0p3` sin udev (→ enlace by-uuid), listas de apt borradas al retomar (→ apt update en la etapa 20).

## Formas de iniciar: frases, palmadas y rutinas (`orion_voz/activacion.py`) · desbloqueo a elección
Como Tony Stark: «Buenos días, señoritas» 👏👏. La persona agrega las frases y palmadas que quiera («formas de
despertar»), cada una con su rutina en orden: despertar (pantalla + IA), saludo, luces, música, resumen del día,
último proyecto, modo taller, reposo, suspender; una frase puede exigir la voz del dueño. El oído siempre activo
cuenta palmadas (golpes cortos > 8× el ruido de fondo, separados 0,15–0,8 s) y reconoce las frases; lo del sistema lo
hace el servicio y lo visual el HUD (`/api/eventos`). Reposo: **atento** (pantalla apagada con wlopm, IA fuera de
la memoria, el oído escucha) tras N minutos o con «buenas noches»; **suspendido** o **apagado**: el PC no oye, lo
despierta el nodo despertador (`nodos/nodo-despertador.py` en una Raspberry con micrófono → Wake-on-LAN), el
teclado o el teléfono. Llave de voz: la frase se graba 1 vez (rápido, umbral fijo más exigente) o 3 (calibrado), y
el desbloqueo se elige: solo voz · solo clave · voz o clave · voz y clave.
Todo es editable desde «formas de despertar»: cada forma se edita, se pausa, se prueba o se quita; la rutina se arma
en el orden que se quiera, con los pasos fijos y pasos propios (`di:` decir un texto, `abre:` una página, `orden:`
cualquier cosa que se le diría a Neo Orión); el saludo es una plantilla ({saludo}, {nombre}) y las palmadas tienen
sensibilidad (sensible · normal · firme).

## Instalación directa y repos (4-nov-2026)

- **`instalar.sh`**: instala Neo Orión sobre un Ubuntu 24.04 que ya existe, sin ISO. Hace lo mismo que las etapas
  20/50 (paquetes de `herramientas` y `kiosko`, código en /opt/orion con su venv y MCP, HUD en /usr/share/orion,
  servicios), escribe la cuenta (`/etc/orion/usuario`, drop-in de `orion-sesion`), detecta controladores, elige la IA
  con `orion-fit` y corre `orion-primer-arranque`. Opciones: `--pantalla` (entra directo al HUD en tty1), `--sin-ia`,
  `--capa NOMBRE`, `--quitar`. Se puede repetir para actualizar. Probado en un Ubuntu noble limpio (chroot): HUD 200, MCP OK, `--quitar` OK.
- **`orion hud`**: abre la interfaz en una ventana dentro del escritorio que ya tenga la persona.
- **Capa personal genérica**: `CAPA=<nombre>` en el build y `capas/<nombre>/aplicar.sh`; el instalador aplica la que traiga la ISO.
- **Guía de uso**: `docs/Guia_de_uso_Neo_Orion.pdf`.

## Efecto CRT, íconos de apps y teléfono como micrófono (4-nov-2026)

- **CRT** (`MX.crt('no'|'suave'|'curva')` en `hud/marathon.js`): líneas de barrido, viñeta, brillo y separación de color;
  `curva` agrega la curvatura del vidrio con un filtro SVG de desplazamiento (barril). Se guarda por equipo; viene del
  instalador (`identidad.crt`) y se cambia en el panel de color o por voz. En modo liviano la curva se apaga.
- **`orion_iconos`**: lista de apps con ícono de bloques 9×9 bajo una regla estética validada (tamaño, un color,
  20–65 % lleno, sin bloques sueltos, pictograma y no logo, **ningún ícono se repite**). Los programas conocidos tienen
  su ícono propio; los nuevos usan el de su categoría si está libre o un monograma con su sigla. MCP `apps`, HUD «mis apps»,
  `/api/accion/apps`. Registro en `~/.orion/apps.json`.
- **`orion_telefono`**: el teléfono (iPhone o Android, sin app) como micrófono real. Fuente virtual de PipeWire
  (`module-null-sink media.class=Audio/Source/Virtual`), página HTTPS con certificado propio + WebSocket (PCM 16 bits),
  `pacat` escribe el audio; mientras está conectado es la fuente predeterminada y al soltar vuelve la anterior. Clave de un
  solo uso en el QR, apagado tras 30 min. Probado con Chromium y micrófono simulado por TLS (el audio llega).

## Núcleo en Rust y vistas de archivo (8-nov-2026)

- **Norma: todo en Rust.** Lo nuevo se escribe en Rust; lo que está en Python se migra por etapas sin romper la API
  (`nucleo/MIGRACION.md`, tabla módulo por módulo). El núcleo `orion-nucleo` reemplaza a `orion_core.servidor`: 9 MB de
  RAM contra 28 MB, sin intérprete ni recolector de basura. Lo no migrado pasa por `orion_core.legado` (un Python corto
  por acción, nada de fondo). Compila en la etapa 45 de la ISO; `instalar.sh` baja el binario de Releases.
- **Archivo · documento · movimiento** (`hud/index.html`): manifiesto de documentos con una fila de acento que recorre la
  tabla y los va «leyendo», ficha de documento con su ID y estado grandes, diagrama orbital animado (de dónde viene y cómo
  llega a la memoria) y rastreo en línea de tiempo. Datos reales del núcleo (memoria + Documentos + Descargas).
  `marathon.js` suma `MX.cargando` (barra por segmentos), `MX.banda` (aviso que corre sobre el acento, usado por el
  guardián y la CNC) y `MX.pixelar` (aparecer en bloques). Por voz: «archivo de lidar», «mis documentos», o la pestaña
  ARCHIVO del muro de investigación.
