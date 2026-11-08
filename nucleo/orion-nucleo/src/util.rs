//! Utilidades comunes: correr programas, leer archivos sin pánico, rutas de la persona.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

/// Corre un programa y devuelve su salida estándar (vacía si falla o no existe).
pub fn sh(cmd: &[&str]) -> String {
    sh_tiempo(cmd, 20)
}

/// Igual que `sh`, con tiempo máximo en segundos (si se pasa, se mata el proceso).
pub fn sh_tiempo(cmd: &[&str], segundos: u64) -> String {
    let Some((prog, args)) = cmd.split_first() else { return String::new() };
    let Ok(mut hijo) = Command::new(prog).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()
    else {
        return String::new();
    };
    let limite = std::time::Instant::now() + Duration::from_secs(segundos);
    loop {
        match hijo.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() > limite => {
                let _ = hijo.kill();
                let _ = hijo.wait();
                return String::new();
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(15)),
            Err(_) => return String::new(),
        }
    }
    hijo.wait_with_output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default()
}

pub fn leer(ruta: &str) -> String {
    std::fs::read_to_string(ruta).map(|s| s.trim_matches(|c| c == '\0' || c == ' ' || c == '\n').to_string()).unwrap_or_default()
}

pub fn casa() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/root"))
}

pub fn existe_programa(nombre: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(nombre).is_file()))
        .unwrap_or(false)
}

/// Quita tildes y pasa a minúsculas (para reconocer frases y categorías).
pub fn sin_tilde(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    s.nfd().filter(|c| !unicode_normalization::char::is_combining_mark(*c)).collect::<String>().to_lowercase()
}

/// Redondeo a n decimales (como `round(x, n)` de Python).
pub fn redondear(x: f64, n: i32) -> f64 {
    let f = 10f64.powi(n);
    (x * f).round() / f
}
