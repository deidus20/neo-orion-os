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

/// Segundos desde 1970 (como `time.time()` de Python).
pub fn ahora_s() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| (d.as_millis() as f64) / 1000.0).unwrap_or(0.0)
}

/// Parecido entre dos textos (0..1), igual que `difflib.SequenceMatcher(None, a, b).ratio()` de Python.
pub fn parecido(a: &str, b: &str) -> f64 {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    fn coincidencias(a: &[char], b: &[char]) -> usize {
        // el bloque común más largo y, recursivamente, lo que queda a cada lado
        let (mut mejor, mut ia, mut ib) = (0, 0, 0);
        let mut previo = vec![0usize; b.len() + 1];
        for i in 0..a.len() {
            let mut actual = vec![0usize; b.len() + 1];
            for j in 0..b.len() {
                if a[i] == b[j] {
                    actual[j + 1] = previo[j] + 1;
                    if actual[j + 1] > mejor {
                        mejor = actual[j + 1];
                        ia = i + 1 - mejor;
                        ib = j + 1 - mejor;
                    }
                }
            }
            previo = actual;
        }
        if mejor == 0 {
            return 0;
        }
        mejor + coincidencias(&a[..ia], &b[..ib]) + coincidencias(&a[ia + mejor..], &b[ib + mejor..])
    }
    2.0 * coincidencias(&a, &b) as f64 / (a.len() + b.len()) as f64
}

/// Resultado de un programa: (código, salida, errores).
pub type Salida = (i32, String, String);
/// Quien corre programas (en pruebas se reemplaza por uno falso).
pub type Ejecutor<'a> = &'a dyn Fn(&[&str]) -> Salida;

pub fn correr(cmd: &[&str]) -> Salida {
    let Some((p, a)) = cmd.split_first() else { return (-1, String::new(), String::new()) };
    match std::process::Command::new(p).args(a).stdin(std::process::Stdio::null()).output() {
        Ok(o) => (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stdout).into(), String::from_utf8_lossy(&o.stderr).into()),
        Err(e) => (-1, String::new(), e.to_string()),
    }
}

/// Expande «~/» a la carpeta de la persona.
pub fn expandir(r: &str) -> std::path::PathBuf {
    match r.strip_prefix("~/") {
        Some(x) => casa().join(x),
        None if r == "~" => casa(),
        None => std::path::PathBuf::from(r),
    }
}

/// IP de esta máquina en la red de la casa (sin mandar nada).
pub fn ip_local() -> String {
    std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("10.255.255.255:1")?;
            s.local_addr()
        })
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".into())
}
