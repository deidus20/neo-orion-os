//! Permisos de la IA sobre el sistema (antes orion_core/policy.py).
//!
//! La IA tiene control total del software: clona, instala, compila, edita y ejecuta sin pedir permiso. Las únicas
//! pausas son de INGENIERÍA: IRREVERSIBLE (borrar fuera del espacio de trabajo, discos) y FÍSICO (motores, PLC,
//! cerraduras) esperan un «sí» de la persona. Todo queda en ~/.orion/bitacora.jsonl; lo que espera, en
//! ~/.orion/pendientes.json, para que el HUD o la voz lo aprueben.

use regex::Regex;
use serde_json::{json, Map, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Riesgo {
    Leer,
    Escribir,
    Sistema,
    Irreversible,
    Fisico,
}

impl Riesgo {
    pub fn nombre(self) -> &'static str {
        ["LEER", "ESCRIBIR", "SISTEMA", "IRREVERSIBLE", "FISICO"][self as usize]
    }
}

fn peligrosos() -> &'static Vec<(Regex, &'static str)> {
    static P: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    P.get_or_init(|| {
        [
            (r"\brm\s+(-[a-z]*r[a-z]*f|-[a-z]*f[a-z]*r)\b\s+(/|~|\$HOME|/home|/etc|/usr|/boot)(\s|$)", "borrado recursivo de una ruta del sistema"),
            (r"\b(mkfs(\.\w+)?|wipefs|fdisk|parted|sgdisk)\b", "operación sobre discos/particiones"),
            (r"\bdd\b.*\bof=/dev/", "escritura directa a un disco"),
            (r":\(\)\s*\{\s*:\|:&\s*\};:", "fork bomb"),
            (r">\s*/dev/(sd|nvme|mmcblk)", "sobrescribir un disco"),
        ]
        .iter()
        .map(|(p, w)| (Regex::new(p).unwrap(), *w))
        .collect()
    })
}

fn de_sistema() -> &'static Vec<Regex> {
    static S: OnceLock<Vec<Regex>> = OnceLock::new();
    S.get_or_init(|| {
        [r"\b(apt|apt-get|dpkg|snap|flatpak)\b", r"\bsystemctl\b", r"\bpip3?\s+install\b", r"\bgit\s+clone\b", r"\bcurl\b|\bwget\b", r"\bsudo\b", r"\bnpm\s+(i|install)\b"]
            .iter()
            .map(|p| Regex::new(p).unwrap())
            .collect()
    })
}

/// Normaliza una ruta sin exigir que exista (como Path.resolve() de Python con strict=False).
pub fn absoluta(p: &Path) -> PathBuf {
    let p = if p.is_absolute() { p.to_path_buf() } else { std::env::current_dir().unwrap_or_default().join(p) };
    if let Ok(r) = p.canonicalize() {
        return r;
    }
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            o => out.push(o),
        }
    }
    out
}

pub struct Politica {
    pub modo: String,
    pub trabajo: PathBuf,
    pub bitacora: PathBuf,
    pub pendientes: PathBuf,
}

impl Politica {
    pub fn new(modo: &str, trabajo: &Path, carpeta: &Path) -> Self {
        let _ = std::fs::create_dir_all(carpeta);
        Politica { modo: modo.into(), trabajo: absoluta(trabajo), bitacora: carpeta.join("bitacora.jsonl"), pendientes: carpeta.join("pendientes.json") }
    }

    pub fn clasificar_orden(&self, cmd: &str) -> (Riesgo, &'static str) {
        if let Some((_, w)) = peligrosos().iter().find(|(r, _)| r.is_match(cmd)) {
            return (Riesgo::Irreversible, w);
        }
        if de_sistema().iter().any(|r| r.is_match(cmd)) {
            return (Riesgo::Sistema, "cambia el sistema");
        }
        (Riesgo::Escribir, "")
    }

    pub fn clasificar_escritura(&self, ruta: &Path) -> Riesgo {
        let p = absoluta(ruta);
        if p.starts_with(&self.trabajo) { Riesgo::Escribir } else { Riesgo::Sistema }
    }

    pub fn necesita_persona(&self, r: Riesgo) -> bool {
        r >= Riesgo::Irreversible || (self.modo == "asistido" && r >= Riesgo::Sistema)
    }

    fn leer_pendientes(&self) -> Map<String, Value> {
        std::fs::read_to_string(&self.pendientes).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    /// Deja la acción en espera; la persona la aprueba por voz o en el HUD.
    pub fn pedir(&self, accion: &str, detalle: &Value, r: Riesgo, motivo: &str) -> String {
        let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        let ticket = format!("T{:07}", ms % 10_000_000);
        let mut p = self.leer_pendientes();
        p.insert(ticket.clone(), json!({"accion": accion, "detalle": detalle, "riesgo": r.nombre(), "motivo": motivo}));
        let _ = std::fs::write(&self.pendientes, serde_json::to_string_pretty(&p).unwrap());
        self.anotar(accion, detalle, r, "en-espera", json!({"ticket": ticket}));
        ticket
    }

    /// SOLO la llama la interfaz de la persona (voz verificada o botón del HUD).
    pub fn aprobar(&self, ticket: &str) -> Option<Value> {
        let mut p = self.leer_pendientes();
        let v = p.remove(ticket)?;
        let _ = std::fs::write(&self.pendientes, serde_json::to_string_pretty(&p).unwrap());
        Some(v)
    }

    /// Si hace falta la persona, devuelve el ticket «en espera» que recibe la IA.
    pub fn compuerta(&self, accion: &str, detalle: Value, r: Riesgo, motivo: &str) -> Option<Value> {
        if !self.necesita_persona(r) {
            return None;
        }
        let t = self.pedir(accion, &detalle, r, motivo);
        Some(json!({"estado": "en-espera", "ticket": t, "motivo": if motivo.is_empty() { r.nombre() } else { motivo }, "mensaje": "Requiere confirmación de la persona (voz o HUD)."}))
    }

    pub fn anotar(&self, accion: &str, detalle: &Value, r: Riesgo, resultado: &str, extra: Value) {
        let mut l = json!({"t": chrono::Local::now().format("%F %T").to_string(), "accion": accion, "riesgo": r.nombre(), "resultado": resultado});
        for v in [detalle, &extra] {
            if let Some(m) = v.as_object() {
                for (k, x) in m {
                    l[k] = x.clone();
                }
            }
        }
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&self.bitacora) {
            let _ = writeln!(f, "{l}");
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn clasifica_como_python() {
        let d = tempfile::tempdir().unwrap();
        let p = Politica::new("libre", &d.path().join("orion"), d.path());
        assert_eq!(p.clasificar_orden("rm -rf / ").0, Riesgo::Irreversible);
        assert_eq!(p.clasificar_orden("sudo mkfs.ext4 /dev/sdb1").0, Riesgo::Irreversible);
        assert_eq!(p.clasificar_orden("dd if=x of=/dev/sda").0, Riesgo::Irreversible);
        assert_eq!(p.clasificar_orden("sudo apt-get install -y htop").0, Riesgo::Sistema);
        assert_eq!(p.clasificar_orden("ls -la && cargo build").0, Riesgo::Escribir);
        assert_eq!(p.clasificar_escritura(&d.path().join("orion/a/b.txt")), Riesgo::Escribir);
        assert_eq!(p.clasificar_escritura(&d.path().join("orion/../fuera.txt")), Riesgo::Sistema);
        assert!(!p.necesita_persona(Riesgo::Sistema));
        assert!(p.necesita_persona(Riesgo::Fisico));
        let asistido = Politica::new("asistido", d.path(), d.path());
        assert!(asistido.necesita_persona(Riesgo::Sistema));
    }

    #[test]
    fn ticket_queda_y_se_aprueba() {
        let d = tempfile::tempdir().unwrap();
        let p = Politica::new("libre", d.path(), d.path());
        assert!(p.compuerta("ejecutar", json!({"comando": "ls"}), Riesgo::Escribir, "").is_none());
        let g = p.compuerta("ejecutar", json!({"comando": "rm -rf /"}), Riesgo::Irreversible, "borrado").unwrap();
        assert_eq!(g["estado"], "en-espera");
        let t = g["ticket"].as_str().unwrap();
        assert_eq!(p.aprobar(t).unwrap()["detalle"]["comando"], "rm -rf /");
        assert!(p.aprobar(t).is_none());
        let log = std::fs::read_to_string(d.path().join("bitacora.jsonl")).unwrap();
        assert!(log.contains("\"resultado\":\"en-espera\"") && log.contains(t));
    }
}
