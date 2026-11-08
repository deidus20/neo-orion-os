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
- El HUD sigue en HTML/JS: lo dibuja WebKit. La ventana que lo muestra (`orion-pantalla`, hoy Python+GTK) pasa a Rust
  con `wry`/`webkit2gtk-rs` en la etapa 3.

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
| `orion_telefono` | 337 | etapa 2 | `tokio-tungstenite` + `rustls` (WebSocket TLS) |
| `orion_voz` (oído, palmadas, llave, hablantes) | 1.117 | etapa 2 | `vosk` (crate), `cpal` (audio), `ort` (huella de voz ONNX) |
| `orion_core.rendimiento` (vigía) | 170 | etapa 2 | `sysinfo` |
| `orion_respaldo`, `orion_remoto`, `orion_actualizar` | 394 | etapa 2 | `std::process` (rsync, rclone, wayvnc, apt) |
| `orion_mcp` (servidor MCP) | 720 | etapa 3 | `rmcp` (SDK oficial MCP en Rust) |
| `orion_core.instalador` | 449 | etapa 3 | particionado con `sgdisk`/`mkfs` vía `std::process` |
| `orion_cnc` (GRBL) | 246 | etapa 3 | `serialport` |
| `orion_casa` (MQTT / Home Assistant) | 103 | etapa 3 | `rumqttc`, `ureq` |
| `orion_dictado` (Word) | 158 | etapa 3 | `docx-rs` |
| `orion_proyectos` (Gantt, presupuesto) | 202 | etapa 3 | `rust_xlsxwriter`, `plotters` |
| `orion_traductor`, `orion_cuaderno`, `orion_puente`, `orion_repos` | 517 | etapa 3 | `ureq` |
| `orion_navegador` | 299 | etapa 4 | `chromiumoxide` (CDP) |
| `orion_juegos`, `orion_vision`, `orion_lidar` | 521 | etapa 4 | `ureq`, `v4l`, `rplidar` por serie |
| `orion_motion` (robots, senoides, guardián) | 532 | etapa 4 | Rust puro (`nalgebra`) + `r2r` (ROS 2) |
| `orion_forma` (CAD por código) | 144 | **queda como programa externo** | CadQuery/FreeCAD son motores CAD hechos en Python/C++ sin equivalente maduro en Rust: el núcleo los llama como cualquier programa, igual que llama a Blender o KiCad |

**Migrado hoy:** ~1.400 líneas de las 7.500 (los servicios que corren todo el día).

## Etapas

1. ✅ **Núcleo**: servidor, equipo, controladores, RIC, íconos, memoria, archivo, chat. Puente para el resto.
2. **Servicios de fondo**: oído y palmadas, vigía de rendimiento, teléfono como micrófono, respaldo, remoto, actualizar.
   Al cerrar esta etapa, **ningún Python corre al encender**.
3. **Herramientas**: MCP, instalador, CNC, casa, dictado, proyectos, traductor, cuadernos, ventana del HUD.
4. **Robots y visión**: navegador, juegos, cámaras, LiDAR, movimiento y guardián.
5. **Limpieza**: se borra `orion/` (salvo los scripts CAD) y el puente.

## Compilar y probar

```bash
cd nucleo
cargo test                 # pruebas de cada módulo
cargo build --release      # binario: target/release/orion-nucleo (~5 MB)
./target/release/orion-nucleo servir --modo hud --raiz ..
./target/release/orion-nucleo electrica 20 A a 35 m
```
