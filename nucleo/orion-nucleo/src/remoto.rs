//! Escritorio remoto. En la casa: wayvnc (VNC de Wayland, con clave y TLS, solo red local). Desde afuera: RustDesk
//! (ID y clave de un solo uso; servidor propio opcional). La clave queda en ~/.config/wayvnc/config (600).

use crate::util::{existe_programa, ip_local, Ejecutor};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const PUERTO: u16 = 5900;
pub const INSTALAR_RUSTDESK: [&str; 3] = [
    "# baja el .deb oficial (x86_64) desde https://github.com/rustdesk/rustdesk/releases/latest",
    "sudo apt-get install -y ./rustdesk-*-x86_64.deb",
    "# opcional, servidor propio: https://rustdesk.com/docs/es/self-host/",
];

pub fn conf() -> PathBuf {
    crate::util::casa().join(".config/wayvnc")
}

/// Clave nueva al azar (letras y números), desde /dev/urandom.
pub fn clave_nueva(n: usize) -> String {
    let alf: Vec<char> = ('a'..='z').chain('A'..='Z').chain('0'..='9').collect();
    let mut b = vec![0u8; n * 2];
    use std::io::Read;
    let _ = std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b));
    b.chunks(2).map(|c| alf[(u16::from_le_bytes([c[0], c[1]]) as usize) % alf.len()]).collect()
}

pub fn preparar_vnc(usuario: &str, clave: Option<&str>, dir: &Path, ejecutar: Ejecutor) -> Value {
    let _ = std::fs::create_dir_all(dir);
    let clave = clave.map(String::from).unwrap_or_else(|| clave_nueva(10));
    let (cert, llave) = (dir.join("tls_cert.pem"), dir.join("tls_key.pem"));
    if !cert.exists() {
        ejecutar(&["openssl", "req", "-x509", "-newkey", "rsa:3072", "-sha256", "-days", "3650", "-nodes", "-keyout", &llave.to_string_lossy(), "-out", &cert.to_string_lossy(), "-subj", "/CN=neo-orion"]);
    }
    let cfg = dir.join("config");
    let texto = format!("address=0.0.0.0\nport={PUERTO}\nenable_auth=true\nusername={usuario}\npassword={clave}\nprivate_key_file={}\ncertificate_file={}\n", llave.display(), cert.display());
    if let Err(e) = std::fs::write(&cfg, texto) {
        return json!({"ok": false, "error": e.to_string()});
    }
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(&cfg, std::fs::Permissions::from_mode(0o600));
    json!({"ok": true, "direccion": format!("{}:{PUERTO}", ip_local()), "usuario": usuario, "clave_guardada_en": cfg, "clave": clave})
}

pub fn encender_vnc() -> Value {
    if !existe_programa("wayvnc") {
        return json!({"ok": false, "error": "falta wayvnc (sudo apt install wayvnc)"});
    }
    if !conf().join("config").exists() {
        preparar_vnc("orion", None, &conf(), &crate::util::correr);
    }
    let _ = std::process::Command::new("wayvnc").args(["--config", &conf().join("config").to_string_lossy()]).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
    json!({"ok": true, "direccion": format!("{}:{PUERTO}", ip_local()), "como": "app VNC del teléfono (RealVNC Viewer, bVNC, AVNC)"})
}

pub fn apagar(ejecutar: Ejecutor) -> Value {
    for p in ["wayvnc", "rustdesk"] {
        ejecutar(&["pkill", "-x", p]);
    }
    json!({"ok": true, "mensaje": "acceso remoto apagado"})
}

pub fn rustdesk_id(ejecutar: Ejecutor) -> Value {
    if !existe_programa("rustdesk") {
        return json!({"ok": false, "error": "RustDesk no está instalado", "instalar": INSTALAR_RUSTDESK});
    }
    let (c, out, _) = ejecutar(&["rustdesk", "--get-id"]);
    json!({"ok": c == 0, "id": out.trim(), "nota": "la clave de un solo uso aparece en la app de RustDesk"})
}

pub fn estado(ejecutar: Ejecutor) -> Value {
    let corre = |p: &str| ejecutar(&["pgrep", "-x", p]).0 == 0;
    json!({"vnc": corre("wayvnc"), "rustdesk": corre("rustdesk"), "ip": ip_local(), "puerto_vnc": PUERTO})
}

pub fn accion(d: &Value) -> Value {
    let e = &crate::util::correr;
    match d["accion"].as_str().unwrap_or("") {
        "casa" => {
            let mut r = encender_vnc();
            if r["ok"] == true {
                let t = std::fs::read_to_string(conf().join("config")).unwrap_or_default();
                r["clave"] = t.lines().find_map(|l| l.strip_prefix("password=")).unwrap_or("").into();
            }
            r
        }
        "apagar" => apagar(e),
        "afuera" => rustdesk_id(e),
        _ => estado(e),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    #[test]
    fn vnc_con_tls_y_clave_privada() {
        let d = tempfile::tempdir().unwrap();
        let usados = std::cell::RefCell::new(vec![]);
        let f = |c: &[&str]| {
            usados.borrow_mut().push(c.join(" "));
            (0, String::new(), String::new())
        };
        let r = preparar_vnc("orion", Some("Secreta123"), d.path(), &f);
        assert_eq!(r["ok"], true);
        let cfg = std::fs::read_to_string(d.path().join("config")).unwrap();
        assert!(cfg.contains("password=Secreta123") && cfg.contains("enable_auth=true") && cfg.contains("certificate_file"));
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(d.path().join("config")).unwrap().permissions().mode() & 0o777, 0o600);
        assert!(usados.borrow()[0].starts_with("openssl req -x509"));
        assert_eq!(clave_nueva(10).len(), 10);
        assert_ne!(clave_nueva(12), clave_nueva(12));
        assert_eq!(estado(&|_| (1, String::new(), String::new()))["vnc"], false);
    }
}
