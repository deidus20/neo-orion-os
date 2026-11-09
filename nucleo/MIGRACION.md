# Migración de Neo Orión a Rust

**Norma:** todo Neo Orión corre en Rust nativo. Todo código nuevo se escribe en Rust; lo que hoy está en Python
(`orion/`) se pasa módulo por módulo, con sus pruebas, sin romper la API que usa el HUD.

## Cómo se hace sin que nada se rompa

1. **El núcleo (`orion-nucleo`) atiende todo.** Es el único servidor del HUD y del instalador (127.0.0.1:7700).
2. **Lo ya migrado corre en Rust.** Lo que falta pasa por un puente corto (`orion_core.legado`): se abre un Python solo
   para esa acción, responde y termina. **No queda ningún Python de fondo.**
3. **Cada módulo migrado sale del puente** (`legado::PENDIENTES`) y su par en Python se borra cuando sus pruebas en Rust
   cubren lo mismo.
4. **Las pruebas viajan con el código.** Cada módulo Rust trae las mismas pruebas que tenía en Python (`cargo test`).

## Por qué Rust aquí (y qué no cambia)

- Un servidor Python vive con el intérprete, el recolector de basura y cada librería cargada en memoria: medido en este
  equipo, el servidor Python ocupa **28 MB** y el núcleo Rust **9 MB**, con la misma API. Rust no tiene recolector de
  basura: la memoria se libera en el momento exacto, sin pausas.
- Lo más pesado de Neo Orión **ya era nativo**: la IA local (Ollama, C++/Go), el oído (Vosk/whisper.cpp, C++), la voz
  (Piper, C++) y la pantalla (WebKit, C++). La ganancia de Rust está en los servicios que corren todo el día.
- El HUD sigue en HTML/JS: lo dibuja WebKit. La ventana que lo muestra (`orion-pantalla`) ya es Rust (webkit2gtk-rs).

## Estado, módulo por módulo

| Módulo Python | Líneas | Estado | En Rust |
|---|---:|---|---|
| `orion_core.servidor` | 470 | ✅ **migrado** | `servidor.rs` (tiny_http, 4 hilos fijos) |
| `orion_core.controladores` | 150 | ✅ **migrado** | `controladores.rs` |
| `orion_core.detect` (equipo, discos) | 160 | ✅ **migrado** | `equipo.rs` |
| `orion_electrica` (RIC) | 144 | ✅ **migrado** | `electrica.rs` |
| `orion_iconos` | 263 | ✅ **migrado** | `iconos.rs` (datos en `config/iconos.json`) |
| `orion_memoria` | 168 | ✅ **migrado** (sin embeddings aún) | `memoria.rs` (rusqlite + FTS5, mismo índice) |
| chat con la IA local | — | ✅ **migrado** | `ia.rs` (ureq → Ollama) |
| vista Archivo | — | ✅ **nuevo en Rust** | `archivo.rs` |
| `orion_telefono` | 337 | ✅ **migrado** | `telefono.rs` (rustls + WebSocket propio, micrófono virtual de PipeWire) |
| `orion_voz` (oído, palmadas, llave, hablantes) | 1.117 | ✅ **migrado** | `voz/` (VAD WebRTC, libvosk y sherpa-onnx cargados al vuelo, whisper.cpp, huella MFCC con rustfft) |
| `orion_core.rendimiento` (vigía) | 170 | ✅ **migrado** | `rendimiento.rs` |
| `orion_respaldo`, `orion_remoto`, `orion_actualizar` | 394 | ✅ **migrado** | `respaldo.rs`, `remoto.rs`, `actualizar.rs` |
| `orion-primer-arranque` (bash + Python) | 60 | ✅ **migrado** | `primer.rs` (Vosk, whisper.cpp y Piper nativos; sin pip) |
| `orion_mcp` (servidor MCP) | 720 | ✅ **migrado** (22 de 43 herramientas nativas; el resto por el puente hasta que su módulo pase) | `mcp.rs` (JSON-RPC por stdio, 5 MB en reposo; catálogo en `config/mcp-herramientas.json`) |
| `orion_core.policy` (permisos de la IA) | 76 | ✅ **migrado** | `politica.rs` (los tickets en espera quedan en `~/.orion/pendientes.json`) |
| `orion_core.instalador` + `instalacion` | 552 | ✅ **migrado** | `instalador.rs` (`orion-nucleo instalar`; en simulación da los mismos 68 comandos, en el mismo orden, que el de Python) |
| `orion_cnc` (GRBL) | 246 | ✅ **migrado** | `cnc.rs` (serialport; vista del recorrido en SVG con la estética del HUD) |
| `orion_core.detect` + `perfil` | 262 | ✅ **migrado** | `perfil.rs` (catálogo en `config/perfil-catalogo.json`; mismo nivel y mismas elecciones que Python en este equipo) |
| `orion-fit` (qué IA cabe) | 239 | ✅ **migrado** | `fit.rs` (`orion-nucleo fit`; mismo resultado que el de Python en 7 equipos simulados) |
| `orion_casa` (Home Assistant) | 103 | ✅ **migrado** (adelantado) | `casa.rs` |
| `orion_dictado` (Word) | 158 | ✅ **migrado** | `dictado.rs` (docx-rs; hoja A4, verificado abriéndolo con LibreOffice) |
| `orion_proyectos` (Gantt, presupuesto) | 202 | ✅ **migrado** | `proyectos.rs` (CPM, presupuesto con IVA, RACI, Gantt SVG y Excel con rust_xlsxwriter; mismos números que Python en el caso de la oficina de 30 m²) |
| `orion_traductor` | 120 | ✅ **migrado** | `traductor.rs` (translateLocally/Bergamot sin internet en vez de Argos, o la IA local) |
| `orion_repos` | 74 | ✅ **migrado** | `repos.rs` (gh) |
| `orion_cuaderno`, `orion_puente` | 323 | ✅ **migrado** | `cuaderno.rs` (Open Notebook por su API local), `puente.rs` (claude/gemini/codex/opencode/ollama) |
| `orion-pantalla` (ventana del HUD) | 70 | ✅ **migrado** | programa aparte `orion-pantalla/` (webkit2gtk-rs, 360 KB; el núcleo no depende de librerías gráficas) |
| `orion_navegador` | 299 | ✅ **migrado** | `navegador.rs` (Chrome por DevTools con WebSocket propio, Firefox por Marionette; sin Playwright ni Selenium; probado con Chrome real: escribe, hace clic, lee y captura) |
| `orion_lidar` | 151 | ✅ **migrado** | `lidar.rs` (sin numpy; protocolo RPLidar propio por serie; mapa SVG; `orion-nucleo lidar demo`) |
| `orion_vision` (Frigate) | 133 | ✅ **migrado** | `vision.rs` (mismos config.yml y docker-compose.yml que Python; cliente MQTT propio para los avisos al Guardián) |
| `orion_juegos` | 237 | ✅ **migrado** | `juegos.rs` (datos en `config/juegos.json`; ahora sí usa la gráfica real y `hacerlo` sigue todos los pasos) |
| `orion_motion` (robots, senoides, guardián) | 532 | ✅ **migrado** | `movimiento.rs` (Rust puro; mismas 29 órdenes que Python con 26 frases de prueba, protecciones incluidas; además arregla la coma decimal «12,5») |
| `nodos/` (despertador en Raspberry) | 248 | ✅ **migrado** | `orion-nucleo despertador <MAC>` (mismo detector de palmadas y Wake-on-LAN del núcleo) |
| `orion_forma` (CAD por código) | 144 | **queda como programa externo** | CadQuery/FreeCAD son motores CAD hechos en Python/C++ sin equivalente maduro en Rust: el núcleo los llama como cualquier programa, igual que llama a Blender o KiCad |

**Migración terminada (9-11-2026):** todo Neo Orión corre en Rust (`orion-nucleo` y `orion-pantalla`, 71 pruebas). El
puente Python y los módulos migrados se borraron. Python queda solo para el CAD por código (`orion/orion_forma`,
build123d/CadQuery), que el núcleo llama como programa externo, igual que a Blender o KiCad.

## Etapas

1. ✅ **Núcleo**: servidor, equipo, controladores, RIC, íconos, memoria, archivo, chat. Puente para el resto.
2. ✅ **Servicios de fondo**: oído y palmadas, vigía de rendimiento, teléfono como micrófono, respaldo, remoto, actualizar.
   Al cerrar esta etapa, **ningún Python corre al encender**.
3. ✅ **Herramientas**: MCP, instalador, CNC, dictado, proyectos, traductor, cuadernos, ventana del HUD.
4. ✅ **Robots y visión**: navegador, juegos, cámaras, LiDAR, movimiento y guardián.
5. ✅ **Limpieza**: se borró `orion/` (salvo el CAD) y el puente; las etapas de la ISO y `instalar.sh` ya no necesitan Python.

## Compilar y probar

```bash
cd nucleo
cargo test                 # pruebas de cada módulo
cargo build --release      # binario: target/release/orion-nucleo (~5 MB)
./target/release/orion-nucleo servir --modo hud --raiz ..
./target/release/orion-nucleo electrica 20 A a 35 m
```
