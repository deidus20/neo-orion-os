//! Vigía de rendimiento: modo patata por RENDIMIENTO, no por ficha técnica.
//!
//! NORMAL → todo encendido · LIVIANO → HUD sin efectos ni 3D, IA más chica · PATATA → además oído mínimo y lo
//! pesado a la nube. Señales: presión de memoria y CPU (PSI), carga por núcleo, temperatura, batería y la velocidad
//! real de la IA (tok/s) y del HUD (FPS). Histéresis: baja rápido (≈15 s malos) y sube lento (≈2 min buenos).

use serde_json::{json, Map, Value};
use std::path::Path;
use std::time::Instant;

pub const NIVELES: [&str; 3] = ["NORMAL", "LIVIANO", "PATATA"];
/// (clave, liviano, patata, menor_es_peor, cómo se dice)
const UMBRAL: [(&str, f64, f64, bool, &str); 6] = [
    ("mem_psi", 10.0, 30.0, false, "falta memoria"),
    ("cpu_psi", 40.0, 75.0, false, "CPU saturada"),
    ("carga", 1.2, 2.0, false, "CPU saturada"),
    ("tok_s", 8.0, 3.0, true, "la IA responde lento"),
    ("temp", 88.0, 96.0, false, "el procesador está muy caliente"),
    ("fps", 40.0, 24.0, true, "la pantalla va a tirones"),
];

pub fn psi(nombre: &str, raiz: &str) -> Option<f64> {
    let t = std::fs::read_to_string(Path::new(raiz).join(nombre)).ok()?;
    let i = t.find("some avg10=")? + 11;
    t[i..].split_whitespace().next()?.parse().ok()
}

fn temp() -> Option<f64> {
    let mut mejor: Option<f64> = None;
    for z in std::fs::read_dir("/sys/class/thermal").into_iter().flatten().flatten() {
        if !z.file_name().to_string_lossy().starts_with("thermal_zone") {
            continue;
        }
        let tipo = std::fs::read_to_string(z.path().join("type")).unwrap_or_default().to_lowercase();
        if ["x86_pkg", "cpu", "soc", "k10temp", "acpitz"].iter().any(|k| tipo.contains(k)) {
            if let Ok(t) = std::fs::read_to_string(z.path().join("temp")).unwrap_or_default().trim().parse::<f64>() {
                mejor = Some(mejor.unwrap_or(0.0).max(t / 1000.0));
            }
        }
    }
    mejor
}

fn bateria() -> (bool, Option<i64>) {
    for b in std::fs::read_dir("/sys/class/power_supply").into_iter().flatten().flatten() {
        if b.file_name().to_string_lossy().starts_with("BAT") {
            let e = std::fs::read_to_string(b.path().join("status")).unwrap_or_default();
            if let Ok(c) = std::fs::read_to_string(b.path().join("capacity")).unwrap_or_default().trim().parse() {
                return (e.trim() == "Discharging", Some(c));
            }
        }
    }
    (false, None)
}

/// Una lectura del equipo; `extra` trae tok_s y fps.
pub fn medir(extra: &Map<String, Value>) -> Map<String, Value> {
    let nucleos = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as f64;
    let carga = std::fs::read_to_string("/proc/loadavg").ok().and_then(|s| s.split_whitespace().next()?.parse::<f64>().ok()).map(|c| c / nucleos);
    let (desc, cap) = bateria();
    let mut m = Map::new();
    m.insert("mem_psi".into(), json!(psi("memory", "/proc/pressure")));
    m.insert("cpu_psi".into(), json!(psi("cpu", "/proc/pressure")));
    m.insert("carga".into(), json!(carga));
    m.insert("temp".into(), json!(temp()));
    m.insert("bateria_descargando".into(), json!(desc));
    m.insert("bateria".into(), json!(cap));
    m.extend(extra.clone());
    m
}

/// Nivel para UNA lectura y las razones en palabras simples.
pub fn nivel_de(m: &Map<String, Value>) -> (&'static str, Vec<String>) {
    let (mut peor, mut razones) = (0usize, std::collections::BTreeSet::new());
    for (k, liv, pat, menor, nombre) in UMBRAL {
        let Some(v) = m.get(k).and_then(Value::as_f64) else { continue };
        let malo = |u: f64| if menor { v < u } else { v > u };
        if malo(pat) {
            peor = 2;
            razones.insert(nombre.to_string());
        } else if malo(liv) {
            peor = peor.max(1);
            razones.insert(nombre.to_string());
        }
    }
    if m.get("bateria_descargando").and_then(Value::as_bool) == Some(true) && m.get("bateria").and_then(Value::as_i64).unwrap_or(100) < 20 {
        peor = peor.max(1);
        razones.insert("batería baja".into());
    }
    (NIVELES[peor], razones.into_iter().collect())
}

pub fn acciones(nivel: &str) -> Vec<&'static str> {
    match nivel {
        "NORMAL" => vec!["hud:completo", "ia:modelo-recomendado", "oido:normal"],
        "LIVIANO" => vec!["hud:liviano", "ia:modelo-un-paso-menor", "oido:normal"],
        _ => vec!["hud:liviano", "ia:modelo-minimo-para-ordenes", "ia:pesado-a-la-nube", "oido:vosk", "servicios:pausar-no-esenciales"],
    }
}

pub fn aviso(de: &str, a: &str, razones: &[String]) -> String {
    let i = |n: &str| NIVELES.iter().position(|x| *x == n).unwrap_or(0);
    if i(a) > i(de) {
        let porque = if razones.is_empty() { "el equipo va justo".to_string() } else { razones.join(" y ") };
        format!("Bajé a modo {} porque {porque}. Di «modo completo» si prefieres todo encendido.", a.to_lowercase())
    } else {
        "El equipo se recuperó: vuelvo a encender lo que había bajado.".into()
    }
}

/// Decide el nivel con histéresis.
pub struct Vigia {
    pub bajar_s: f64,
    pub subir_s: f64,
    pub nivel: &'static str,
    pub fijo: Option<String>,
    desde: Option<(&'static str, f64)>,
}

impl Default for Vigia {
    fn default() -> Self {
        Vigia { bajar_s: 15.0, subir_s: 120.0, nivel: "NORMAL", fijo: None, desde: None }
    }
}

impl Vigia {
    pub fn actualizar(&mut self, m: &Map<String, Value>, ahora: f64) -> Option<Value> {
        if self.fijo.is_some() {
            return None;
        }
        let (objetivo, razones) = nivel_de(m);
        let i = |n: &str| NIVELES.iter().position(|x| *x == n).unwrap();
        let (io, ia) = (i(objetivo), i(self.nivel));
        if io == ia {
            self.desde = None;
            return None;
        }
        let clave = if io > ia { "bajar" } else { "subir" };
        let t0 = match self.desde {
            Some((c, t)) if c == clave => t,
            _ => ahora,
        };
        self.desde = Some((clave, t0));
        if ahora - t0 < if clave == "bajar" { self.bajar_s } else { self.subir_s } {
            return None;
        }
        let nuevo = if clave == "bajar" { objetivo } else { NIVELES[ia - 1] };
        let anterior = std::mem::replace(&mut self.nivel, nuevo);
        self.desde = None;
        Some(json!({"de": anterior, "a": nuevo, "razones": razones, "acciones": acciones(nuevo), "aviso": aviso(anterior, nuevo, &razones)}))
    }
}

/// Servicio: mide cada pocos segundos y deja ~/.orion/rendimiento.json para el HUD y el asistente.
pub fn vigilar(cada_s: u64) -> ! {
    let base = crate::util::casa().join(".orion");
    let _ = std::fs::create_dir_all(&base);
    let (mut v, inicio) = (Vigia::default(), Instant::now());
    loop {
        let mut extra = Map::new();
        for (k, f) in [("tok_s", "ia_tok_s"), ("fps", "hud_fps")] {
            if let Ok(x) = std::fs::read_to_string(base.join(f)).unwrap_or_default().trim().parse::<f64>() {
                extra.insert(k.into(), json!(x));
            }
        }
        v.fijo = std::fs::read_to_string(base.join("nivel_fijo")).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        let m = medir(&extra);
        let cambio = v.actualizar(&m, inicio.elapsed().as_secs_f64());
        let nivel = v.fijo.clone().unwrap_or_else(|| v.nivel.to_string());
        let _ = std::fs::write(base.join("rendimiento.json"), json!({"nivel": nivel, "medicion": m, "cambio": cambio}).to_string());
        std::thread::sleep(std::time::Duration::from_secs(cada_s));
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    fn m(pares: &[(&str, Value)]) -> Map<String, Value> {
        pares.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    #[test]
    fn niveles_y_razones() {
        assert_eq!(nivel_de(&m(&[("mem_psi", json!(2.0)), ("carga", json!(0.3))])).0, "NORMAL");
        let (n, r) = nivel_de(&m(&[("tok_s", json!(5.0))]));
        assert_eq!((n, r), ("LIVIANO", vec!["la IA responde lento".to_string()]));
        assert_eq!(nivel_de(&m(&[("mem_psi", json!(40.0)), ("temp", json!(90.0))])).0, "PATATA");
        assert_eq!(nivel_de(&m(&[("bateria_descargando", json!(true)), ("bateria", json!(10))])).0, "LIVIANO");
    }

    #[test]
    fn histeresis_baja_rapido_sube_lento() {
        let mut v = Vigia::default();
        let malo = m(&[("mem_psi", json!(50.0))]);
        assert!(v.actualizar(&malo, 0.0).is_none());
        assert!(v.actualizar(&malo, 10.0).is_none());
        let c = v.actualizar(&malo, 16.0).unwrap();
        assert_eq!((c["de"].as_str(), c["a"].as_str()), (Some("NORMAL"), Some("PATATA")));
        assert!(c["aviso"].as_str().unwrap().contains("falta memoria"));
        let bueno = m(&[("mem_psi", json!(1.0))]);
        assert!(v.actualizar(&bueno, 20.0).is_none());
        assert!(v.actualizar(&bueno, 100.0).is_none());
        assert_eq!(v.actualizar(&bueno, 141.0).unwrap()["a"], "LIVIANO"); // sube de a un nivel
        v.fijo = Some("PATATA".into());
        assert!(v.actualizar(&malo, 500.0).is_none());
    }

    #[test]
    fn lee_psi() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("memory"), "some avg10=12.50 avg60=3.00 avg300=1.00 total=1\nfull avg10=0.00").unwrap();
        assert_eq!(psi("memory", d.path().to_str().unwrap()), Some(12.5));
        assert_eq!(psi("cpu", d.path().to_str().unwrap()), None);
    }
}
