//! Puente temporal a los módulos que todavía están en Python (mientras se migran a Rust, uno por uno).
//!
//! No queda ningún Python corriendo de fondo: para una acción de un módulo no migrado se abre un proceso corto
//! (`python3 -m orion_core.legado <módulo>`), responde en JSON y termina. Cuando el módulo pasa a Rust, sale de
//! `PENDIENTES` y este puente deja de usarse para él.

use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

/// Módulos que todavía se atienden en Python (la lista se achica en cada etapa de la migración).
pub const PENDIENTES: &[&str] = &[]; // el HUD ya no usa el puente: solo quedan herramientas MCP

pub fn raiz_python() -> String {
    std::env::var("ORION_PY").unwrap_or_else(|_| if std::path::Path::new("/opt/orion/orion_core").is_dir() { "/opt/orion".into() } else { "orion".into() })
}

pub fn python() -> String {
    if std::path::Path::new("/opt/orion/venv/bin/python3").exists() { "/opt/orion/venv/bin/python3".into() } else { "python3".into() }
}

pub fn llamar(modulo: &str, datos: &Value) -> Value {
    if !PENDIENTES.contains(&modulo) {
        return json!({"ok": false, "error": "módulo desconocido"});
    }
    lanzar(modulo, datos)
}

/// Una herramienta MCP cuyo módulo sigue en Python (solo la usa `orion-nucleo mcp`, nunca el HUD por HTTP).
pub fn herramienta_python(nombre: &str, args: &Value) -> Value {
    lanzar("mcp", &json!({"herramienta": nombre, "args": args}))
}

fn lanzar(modulo: &str, datos: &Value) -> Value {
    let hijo = Command::new(python())
        .args(["-m", "orion_core.legado", modulo])
        .env("PYTHONPATH", raiz_python())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let Ok(mut hijo) = hijo else { return json!({"ok": false, "error": "falta el puente Python para este módulo"}) };
    if let Some(mut e) = hijo.stdin.take() {
        let _ = e.write_all(datos.to_string().as_bytes());
    }
    match hijo.wait_with_output() {
        Ok(o) if o.status.success() => serde_json::from_slice(&o.stdout).unwrap_or_else(|_| json!({"ok": false, "error": "respuesta inválida del módulo"})),
        Ok(o) => json!({"ok": false, "error": String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("el módulo falló").chars().take(240).collect::<String>()}),
        Err(e) => json!({"ok": false, "error": e.to_string()}),
    }
}
