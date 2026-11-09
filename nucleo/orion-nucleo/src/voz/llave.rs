//! Llave de voz: tu frase, con tu voz (como «Oye Siri», pero la frase la eliges tú).
//!
//! Dos capas que deben calzar: QUÉ se dice (la frase elegida, tolerante a errores del oído) y QUIÉN lo dice (huella
//! de voz, ver `hablantes`). 1 o 3 grabaciones: con 3 se calibra el umbral. Modos: voz · clave · voz_o_clave ·
//! voz_y_clave. Nunca reemplaza la clave de Linux; lo irreversible no se aprueba solo con la voz.
//! La huella se guarda solo en este equipo (~/.orion/llave-voz.json, permiso 600): es un dato biométrico.

use crate::util::{parecido, sin_tilde};
use crate::voz::hablantes::{coseno, crear_huella, Huella};
use regex::Regex;
use serde_json::{json, Value};
use std::path::PathBuf;

pub const UMBRAL_MIN: f64 = 0.55;
pub const UMBRAL_MAX: f64 = 0.85;
pub const UMBRAL_UNA: f64 = 0.75;
pub const MODOS: [(&str, &str); 4] = [("voz", "solo con tu frase"), ("clave", "solo con tu clave"), ("voz_o_clave", "con tu frase o tu clave"), ("voz_y_clave", "con tu frase y además tu clave")];

pub fn normalizar(t: &str) -> String {
    let t = sin_tilde(t);
    let t = Regex::new(r"\b(eh+|em+|mmm+|este|o sea|oye|ya)\b").unwrap().replace_all(&t, " ");
    Regex::new(r"[^a-z0-9 ]+").unwrap().replace_all(&t, " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 0..1: qué tanto lo dicho contiene la frase (tolerante a errores del oído).
pub fn frase_calza(dicho: &str, frase: &str) -> f64 {
    let (a, b) = (normalizar(dicho), normalizar(frase));
    if b.is_empty() {
        return 0.0;
    }
    if a.contains(&b) {
        return 1.0;
    }
    let pal_b: Vec<&str> = b.split_whitespace().collect();
    let hits = pal_b.iter().filter(|w| a.split_whitespace().any(|x| parecido(w, x) >= 0.8)).count();
    parecido(&a, &b).max(hits as f64 / pal_b.len() as f64)
}

pub fn leer_wav(ruta: &std::path::Path) -> Result<Vec<i16>, String> {
    hound::WavReader::open(ruta).map_err(|e| e.to_string()).map(|r| r.into_samples::<i16>().flatten().collect())
}

pub struct LlaveVoz {
    pub huella: Box<dyn Huella>,
    pub archivo: PathBuf,
    pub datos: Value,
}

impl LlaveVoz {
    pub fn new(huella: Option<Box<dyn Huella>>, archivo: Option<PathBuf>) -> Self {
        let archivo = archivo.unwrap_or_else(|| crate::util::casa().join(".orion/llave-voz.json"));
        let datos = std::fs::read_to_string(&archivo).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(|| json!({}));
        LlaveVoz { huella: huella.unwrap_or_else(|| crear_huella("auto")), archivo, datos }
    }

    fn guardar(&self) -> Result<(), String> {
        std::fs::create_dir_all(self.archivo.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&self.archivo, self.datos.to_string()).map_err(|e| e.to_string())?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&self.archivo, std::fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())
    }

    pub fn configurada(&self) -> bool {
        self.datos["huella"].as_array().is_some_and(|h| !h.is_empty())
    }

    pub fn estado(&self) -> Value {
        let mut v = json!({"configurada": self.configurada(), "frase": self.datos["frase"].as_str().unwrap_or(""), "desde": self.datos["creada"].as_str().unwrap_or(""),
                           "umbral": self.datos["umbral"], "grabaciones": self.datos["muestras"].as_i64().unwrap_or(0)});
        v["modo"] = self.modo_actual().into();
        v
    }

    pub fn modo_actual(&self) -> String {
        self.datos["modo"].as_str().map(String::from).unwrap_or_else(|| if self.configurada() { "voz_o_clave".into() } else { "clave".into() })
    }

    /// Frase elegida + 1 a 3 grabaciones (PCM 16 kHz). `textos`: lo que el oído entendió en cada una (opcional).
    pub fn configurar(&mut self, frase: &str, muestras: &[Vec<i16>], textos: &[String]) -> Value {
        let frase = frase.trim();
        if normalizar(frase).split_whitespace().count() < 2 {
            return json!({"ok": false, "error": "la frase necesita al menos 2 palabras (mejor 3 o 4)"});
        }
        if muestras.is_empty() {
            return json!({"ok": false, "error": "graba la frase al menos una vez (mejor 3)"});
        }
        let malas: Vec<String> = textos.iter().enumerate().filter(|(_, t)| frase_calza(t, frase) < 0.6).map(|(i, _)| (i + 1).to_string()).collect();
        if !malas.is_empty() {
            return json!({"ok": false, "error": format!("en la grabación {} no se entendió la frase; repítela", malas.join(", "))});
        }
        let vecs: Vec<Vec<f64>> = muestras.iter().map(|m| self.huella.vector(m)).collect();
        let dim = vecs[0].len();
        let media: Vec<f64> = (0..dim).map(|i| vecs.iter().map(|v| v[i]).sum::<f64>() / vecs.len() as f64).collect();
        let parecidos: Vec<f64> = vecs.iter().map(|v| coseno(v, &media)).collect();
        let minimo = parecidos.iter().cloned().fold(f64::INFINITY, f64::min);
        let umbral = if vecs.len() == 1 { UMBRAL_UNA } else { (minimo - 0.08).clamp(UMBRAL_MIN, UMBRAL_MAX) };
        let modo = self.datos["modo"].as_str().unwrap_or("voz_o_clave").to_string();
        self.datos = json!({"modo": modo, "frase": frase, "huella": media.iter().map(|x| (x * 1e6).round() / 1e6).collect::<Vec<_>>(),
                            "umbral": crate::util::redondear(umbral, 3), "creada": chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(), "muestras": vecs.len()});
        if let Err(e) = self.guardar() {
            return json!({"ok": false, "error": e});
        }
        let mut r = json!({"ok": true, "frase": frase, "umbral": self.datos["umbral"], "consistencia": crate::util::redondear(minimo, 3), "grabaciones": vecs.len()});
        if vecs.len() == 1 {
            r["aviso"] = "con una sola grabación te reconoce peor (ruido, resfrío): grabarla 3 veces es más preciso".into();
        }
        r
    }

    /// Cambia cómo se desbloquea. Sin frase guardada, la voz no se puede exigir.
    pub fn modo(&mut self, nuevo: &str) -> Value {
        let Some((_, exp)) = MODOS.iter().find(|(m, _)| *m == nuevo) else {
            return json!({"ok": false, "error": format!("modo: {}", MODOS.map(|m| m.0).join(" | "))});
        };
        if nuevo != "clave" && !self.configurada() {
            return json!({"ok": false, "error": "primero graba tu frase (aunque sea una vez)"});
        }
        self.datos["modo"] = nuevo.into();
        match self.guardar() {
            Ok(_) => json!({"ok": true, "modo": nuevo, "explicacion": format!("se desbloquea {exp}")}),
            Err(e) => json!({"ok": false, "error": e}),
        }
    }

    pub fn abrir(&self, voz_ok: bool, clave_ok: bool) -> bool {
        match self.modo_actual().as_str() {
            "voz" => voz_ok,
            "clave" => clave_ok,
            "voz_y_clave" => voz_ok && clave_ok,
            _ => voz_ok || clave_ok,
        }
    }

    /// ¿Es la frase y es la voz del dueño? `texto`: lo que entendió el oído (si hay).
    pub fn verificar(&mut self, muestra: &[i16], texto: Option<&str>) -> Value {
        if !self.configurada() {
            return json!({"ok": false, "error": "no hay llave de voz configurada"});
        }
        let frase_ok = texto.map_or(true, |t| frase_calza(t, self.datos["frase"].as_str().unwrap_or("")) >= 0.7);
        let h: Vec<f64> = self.datos["huella"].as_array().unwrap().iter().filter_map(Value::as_f64).collect();
        let p = coseno(&self.huella.vector(muestra), &h);
        let voz_ok = p >= self.datos["umbral"].as_f64().unwrap_or(UMBRAL_UNA);
        json!({"ok": frase_ok && voz_ok, "frase_ok": frase_ok, "voz_ok": voz_ok, "parecido": crate::util::redondear(p, 3), "umbral": self.datos["umbral"], "oido": texto})
    }

    pub fn olvidar(&mut self) -> Value {
        self.datos = json!({"modo": "clave"});
        let _ = self.guardar();
        json!({"ok": true, "mensaje": "huella de voz borrada de este equipo; se desbloquea con tu clave"})
    }
}

/// La clave de la cuenta, revisada por unix_chkpwd (igual que la pantalla de entrada de Linux).
pub fn clave_ok(clave: &str) -> bool {
    if clave.is_empty() {
        return false;
    }
    let usuario = std::env::var("USER").unwrap_or_else(|_| crate::util::sh(&["id", "-un"]).trim().to_string());
    let Ok(mut h) = std::process::Command::new("/usr/sbin/unix_chkpwd").args([&usuario, "nullok"]).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn() else { return false };
    use std::io::Write;
    let _ = h.stdin.take().map(|mut e| e.write_all(format!("{clave}\0").as_bytes()));
    h.wait().map(|s| s.success()).unwrap_or(false)
}

/// Lo usa el primer arranque con las frases (1 a 3) grabadas en el instalador.
pub fn configurar_desde_archivos(frase: &str, wavs: &[PathBuf], modo: &str) -> Value {
    let mut k = LlaveVoz::new(None, None);
    let muestras: Vec<Vec<i16>> = wavs.iter().filter_map(|w| leer_wav(w).ok()).collect();
    let r = if muestras.is_empty() { json!({"ok": true}) } else { k.configurar(frase, &muestras, &[]) };
    k.modo(if !muestras.is_empty() || modo == "clave" { modo } else { "clave" });
    r
}

/// Acciones del HUD (/api/accion/llave).
pub fn accion(d: &Value) -> Value {
    let mut k = LlaveVoz::new(None, None);
    let voz = crate::util::casa().join(".orion/voz");
    match d["accion"].as_str().unwrap_or("") {
        "configurar" => {
            let veces = if d["veces"].as_i64() == Some(1) { 1 } else { 3 };
            let wavs: Vec<PathBuf> = (1..=veces).map(|n| voz.join(format!("nueva-{n}.wav"))).collect();
            if !wavs.iter().all(|w| w.exists()) {
                return json!({"ok": false, "error": format!("faltan grabaciones ({veces})")});
            }
            let m: Vec<Vec<i16>> = wavs.iter().filter_map(|w| leer_wav(w).ok()).collect();
            k.configurar(d["frase"].as_str().unwrap_or(""), &m, &[])
        }
        "modo" => k.modo(d["modo"].as_str().unwrap_or("")),
        "desbloquear" => {
            let modo = k.modo_actual();
            let mut v = json!({"ok": false});
            if modo != "clave" && d["con_voz"] == true {
                let g = crate::servidor::grabar_frase(0, false);
                v = if g["ok"] == true { leer_wav(&voz.join("verificar.wav")).map(|m| k.verificar(&m, None)).unwrap_or(g) } else { g };
            }
            let c = modo != "voz" && clave_ok(d["clave"].as_str().unwrap_or(""));
            json!({"ok": k.abrir(v["ok"] == true, c), "modo": modo, "voz": v, "clave_ok": c})
        }
        "olvidar" => k.olvidar(),
        _ => {
            let mut e = k.estado();
            e["ok"] = true.into();
            e
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::voz::hablantes::HuellaEspectral;

    fn voz(f0: f64, formante: f64, ruido: u64) -> Vec<i16> {
        let mut s = ruido;
        (0..16000)
            .map(|i| {
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let t = i as f64 / 16000.0;
                let v = (2.0 * std::f64::consts::PI * f0 * t).sin() * 0.5 + (2.0 * std::f64::consts::PI * formante * t).sin() * 0.3 + ((s >> 33) as f64 / 4.3e9 - 0.5) * 0.05;
                (v * 12000.0) as i16
            })
            .collect()
    }

    #[test]
    fn frase_tolerante() {
        assert_eq!(frase_calza("eh neo orión soy yo", "Neo Orión, soy yo"), 1.0);
        assert!(frase_calza("buenos dias señorita", "Buenos días, señoritas") >= 0.75);
        assert!(frase_calza("apaga la luz", "Neo Orión, soy yo") < 0.5);
    }

    #[test]
    fn configurar_verificar_y_modos() {
        let d = tempfile::tempdir().unwrap();
        let mut k = LlaveVoz::new(Some(Box::new(HuellaEspectral::new())), Some(d.path().join("llave.json")));
        assert_eq!(k.modo_actual(), "clave");
        assert_eq!(k.modo("voz")["ok"], false); // sin frase no se exige voz
        assert_eq!(k.configurar("hola", &[voz(120.0, 700.0, 1)], &[])["ok"], false);
        let dueno = [voz(120.0, 700.0, 1), voz(122.0, 705.0, 2), voz(119.0, 695.0, 3)];
        let r = k.configurar("Neo Orión soy yo", &dueno, &[]);
        assert_eq!(r["ok"], true, "{r}");
        assert!(k.verificar(&voz(121.0, 702.0, 9), Some("neo orión soy yo"))["voz_ok"] == true);
        assert!(k.verificar(&voz(121.0, 702.0, 9), Some("otra cosa"))["ok"] == false);
        assert_eq!(k.modo("voz_y_clave")["ok"], true);
        assert!(!k.abrir(true, false) && k.abrir(true, true));
        assert!(std::fs::metadata(d.path().join("llave.json")).unwrap().len() > 0);
        assert_eq!(k.olvidar()["ok"], true);
        assert!(!k.configurada());
        let una = k.configurar("Neo Orión soy yo", &dueno[..1], &[]);
        assert!(una["aviso"].is_string() && una["umbral"] == UMBRAL_UNA);
    }
}
