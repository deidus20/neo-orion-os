//! De la palabra de la IA al movimiento seguro del robot (antes orion_motion).
//!
//! ```text
//! IA (habla)  →  Traductor (entiende)  →  Guardián (protege)  →  Driver (mueve)
//! ```
//!
//! El traductor es determinista y sin IA: reconoce la frase por aproximación («ombro a 20 grados lento») y la
//! convierte en órdenes exactas. La IA nunca toca los motores: todo pasa por el Guardián, código fijo que recorta
//! a los límites, limita velocidades (también el pico de las ondas), impide bajar del piso o entrar a zonas
//! prohibidas y frena la base antes de chocar. La IA puede PEDIR saltarse una protección, pero no AUTORIZARLO:
//! solo una persona confirma, y solo una vez. El paro de emergencia físico no pasa por software.

use crate::util::{parecido, sin_tilde};
use regex::Regex;
use serde_json::{json, Map, Value};
use std::f64::consts::PI;
use std::path::{Path, PathBuf};

/// Como el formato :g de Python (6 cifras significativas, sin ceros de sobra).
pub fn g(v: f64) -> String {
    if v == 0.0 {
        return "0".into();
    }
    if !v.is_finite() {
        return if v.is_nan() { "nan".into() } else if v > 0.0 { "inf".into() } else { "-inf".into() };
    }
    let e = format!("{v:.5e}");
    let exp: i32 = e.split('e').nth(1).unwrap().parse().unwrap();
    let limpiar = |s: String| if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    if (-4..6).contains(&exp) {
        limpiar(format!("{:.*}", (5 - exp).max(0) as usize, v))
    } else {
        let (m, x) = e.split_once('e').unwrap();
        let x: i32 = x.parse().unwrap();
        format!("{}e{}{:02}", limpiar(m.to_string()), if x < 0 { '-' } else { '+' }, x.abs())
    }
}

// ------------------------------------------------------------------ robot y cinemática

#[derive(Clone, Debug)]
pub struct Articulacion {
    pub id: String,
    pub alias: Vec<String>,
    pub min: f64,
    pub max: f64,
    pub home: f64,
    /// grados por segundo
    pub vel_max: f64,
    /// [a_mm, alpha_deg, d_mm, theta_offset_deg]
    pub dh: Option<[f64; 4]>,
}

#[derive(Clone, Debug)]
pub struct Robot {
    pub nombre: String,
    pub articulaciones: Vec<Articulacion>,
    pub seguridad: Value,
    /// orden de las articulaciones
    pub pose: Vec<(String, f64)>,
}

type M4 = [[f64; 4]; 4];

fn dh(a: f64, alpha: f64, d: f64, theta: f64) -> M4 {
    let (ct, st, ca, sa) = (theta.cos(), theta.sin(), alpha.cos(), alpha.sin());
    [[ct, -st * ca, st * sa, a * ct], [st, ct * ca, -ct * sa, a * st], [0.0, sa, ca, d], [0.0, 0.0, 0.0, 1.0]]
}

fn mul(a: &M4, b: &M4) -> M4 {
    let mut r = [[0.0; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            r[i][j] = (0..4).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    r
}

fn r1(x: f64) -> f64 {
    (x * 10.0).round_ties_even() / 10.0
}

impl Robot {
    pub fn desde_json(d: &Value) -> Result<Self, String> {
        let mut arts = vec![];
        for j in d["articulaciones"].as_array().ok_or("el perfil no tiene articulaciones")? {
            let dh = j["dh"].as_array().map(|a| {
                let v: Vec<f64> = a.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect();
                [v[0], v[1], v[2], v[3]]
            });
            arts.push(Articulacion {
                id: j["id"].as_str().unwrap_or("").into(),
                alias: j["alias"].as_array().into_iter().flatten().filter_map(Value::as_str).map(String::from).collect(),
                min: j["min"].as_f64().unwrap_or(0.0),
                max: j["max"].as_f64().unwrap_or(0.0),
                home: j["home"].as_f64().unwrap_or(0.0),
                vel_max: j["vel_max"].as_f64().unwrap_or(0.0),
                dh,
            });
        }
        let pose = arts.iter().map(|j| (j.id.clone(), j.home)).collect();
        Ok(Robot { nombre: d["nombre"].as_str().unwrap_or("robot").into(), articulaciones: arts, seguridad: d.get("seguridad").cloned().unwrap_or(json!({})), pose })
    }

    pub fn cargar(ruta: &Path) -> Result<Self, String> {
        let t = std::fs::read_to_string(ruta).map_err(|e| format!("{}: {e}", ruta.display()))?;
        Self::desde_json(&serde_json::from_str(&t).map_err(|e| e.to_string())?)
    }

    pub fn art(&self, id: &str) -> &Articulacion {
        self.articulaciones.iter().find(|j| j.id == id).expect("articulación")
    }

    pub fn angulo(&self, id: &str) -> f64 {
        self.pose.iter().find(|p| p.0 == id).map(|p| p.1).unwrap_or(0.0)
    }

    pub fn fijar(pose: &mut [(String, f64)], id: &str, v: f64) {
        if let Some(p) = pose.iter_mut().find(|p| p.0 == id) {
            p.1 = v;
        }
    }

    /// Posición (mm) de cada articulación, de la base al efector (Denavit-Hartenberg).
    pub fn fk(&self, pose: &[(String, f64)]) -> Vec<(f64, f64, f64)> {
        let mut t: M4 = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
        let mut puntos = vec![(0.0, 0.0, 0.0)];
        for j in &self.articulaciones {
            let Some([a, alpha, d, off]) = j.dh else { continue };
            let ang = pose.iter().find(|p| p.0 == j.id).map(|p| p.1).unwrap_or(j.home);
            t = mul(&t, &dh(a, alpha.to_radians(), d, (ang + off).to_radians()));
            puntos.push((r1(t[0][3]), r1(t[1][3]), r1(t[2][3])));
        }
        puntos
    }

    pub fn efector(&self) -> (f64, f64, f64) {
        *self.fk(&self.pose).last().unwrap()
    }

    pub fn pose_json(&self) -> Value {
        Value::Object(self.pose.iter().map(|(k, v)| (k.clone(), json!(v))).collect())
    }
}

// ------------------------------------------------------------------ onda

/// θ(t) = centro + A · sin(2π · f · t + φ). Para ir más rápido se multiplica la FRECUENCIA (no la amplitud);
/// el pico de velocidad es 2π · f · A (°/s) y es lo que se compara con el máximo del servo.
#[derive(Clone, Debug)]
pub struct Onda {
    pub articulacion: String,
    pub amplitud: f64,
    pub frecuencia: f64,
    pub centro: f64,
    pub fase: f64,
    pub duracion: f64,
    pub mult: f64,
}

impl Onda {
    pub fn new(art: &str, amplitud: f64, frecuencia: f64) -> Self {
        Onda { articulacion: art.into(), amplitud, frecuencia, centro: 0.0, fase: 0.0, duracion: 4.0, mult: 1.0 }
    }
    pub fn f(&self) -> f64 {
        self.frecuencia * self.mult
    }
    pub fn angulo(&self, t: f64) -> f64 {
        self.centro + self.amplitud * (2.0 * PI * self.f() * t + self.fase.to_radians()).sin()
    }
    pub fn velocidad(&self, t: f64) -> f64 {
        self.amplitud * 2.0 * PI * self.f() * (2.0 * PI * self.f() * t + self.fase.to_radians()).cos()
    }
    pub fn pico(&self) -> f64 {
        (2.0 * PI * self.f() * self.amplitud).abs()
    }
    /// Frecuencia máxima que el servo aguanta con esta amplitud.
    pub fn frecuencia_max(&self, vel_max: f64) -> f64 {
        if self.amplitud != 0.0 { vel_max / (2.0 * PI * self.amplitud.abs()) } else { f64::INFINITY }
    }
    pub fn muestras(&self, hz: f64) -> Vec<(f64, f64)> {
        let n = (self.duracion * hz) as usize;
        (0..=n).map(|i| ((i as f64 / hz * 1000.0).round() / 1000.0, (self.angulo(i as f64 / hz) * 100.0).round() / 100.0)).collect()
    }
}

// ------------------------------------------------------------------ órdenes y traductor

#[derive(Clone, Debug, PartialEq)]
pub enum Param {
    N(f64),
    T(String),
}

#[derive(Clone, Debug)]
pub struct Orden {
    /// SET, MOVE_REL, WAVE, MACRO, SAVE, DRIVE, TURN, STOP, CALIBRATE, HOME
    pub tipo: String,
    pub art: Option<String>,
    pub valor: Option<f64>,
    /// en orden de inserción, como el dict de Python
    pub params: Vec<(String, Param)>,
    /// la frase original de la IA
    pub fuente: String,
    pub confianza: f64,
}

impl Orden {
    fn new(tipo: &str) -> Self {
        Orden { tipo: tipo.into(), art: None, valor: None, params: vec![], fuente: String::new(), confianza: 1.0 }
    }
    pub fn p(&self, k: &str) -> Option<f64> {
        self.params.iter().find(|x| x.0 == k).and_then(|x| if let Param::N(n) = x.1 { Some(n) } else { None })
    }
    pub fn poner(&mut self, k: &str, v: Param) {
        match self.params.iter_mut().find(|x| x.0 == k) {
            Some(x) => x.1 = v,
            None => self.params.push((k.into(), v)),
        }
    }
}

impl std::fmt::Display for Orden {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut partes = vec![self.tipo.clone()];
        partes.extend(self.art.clone());
        if let Some(v) = self.valor {
            partes.push(g(v));
        }
        for (k, v) in &self.params {
            partes.push(match v {
                Param::N(n) => format!("{k}={}", g(*n)),
                Param::T(t) => format!("{k}={t}"),
            });
        }
        write!(f, "{}", partes.join(" "))
    }
}

const NUM: &str = r"(-?\d+(?:[.,]\d+)?)";

fn num(x: &str) -> f64 {
    x.replace(',', ".").parse().unwrap_or(0.0)
}

fn re(p: &str) -> Regex {
    Regex::new(&p.replace("{NUM}", NUM)).unwrap()
}

pub struct Traductor {
    /// la última articulación (la pinza): id, mínimo y máximo
    pub pinza: (String, f64, f64),
    /// alias normalizado → id, en el orden en que se agregaron (como el dict de Python)
    pub vocab: Vec<(String, String)>,
    pub macros: Map<String, Value>,
}

impl Traductor {
    pub fn new(robot: &Robot, macros: Map<String, Value>) -> Self {
        let mut vocab: Vec<(String, String)> = vec![];
        let mut poner = |k: String, v: &str| match vocab.iter_mut().find(|x| x.0 == k) {
            Some(x) => x.1 = v.into(),
            None => vocab.push((k, v.into())),
        };
        for j in &robot.articulaciones {
            poner(sin_tilde(&j.id), &j.id);
            for a in &j.alias {
                poner(sin_tilde(a), &j.id);
            }
        }
        let pinza = robot.articulaciones.last().map(|j| (j.id.clone(), j.min, j.max)).unwrap_or_default();
        Traductor { pinza, vocab, macros }
    }

    fn id(&self, k: &str) -> Option<&String> {
        self.vocab.iter().find(|x| x.0 == k).map(|x| &x.1)
    }

    /// A qué articulación se refiere (con su confianza).
    pub fn articulacion(&self, texto: &str) -> (Option<String>, f64) {
        let t = sin_tilde(texto);
        if let Some(m) = re(r"\b(?:servo|motor|articulacion|eje)?\s*([a-z]\d)\b").captures(&t) {
            if let Some(i) = self.id(&sin_tilde(&m[1])) {
                return (Some(i.clone()), 1.0);
            }
        }
        // frases de varias palabras primero («rotacion muneca»)
        let mut por_largo: Vec<&(String, String)> = self.vocab.iter().collect();
        por_largo.sort_by_key(|x| std::cmp::Reverse(x.0.chars().count()));
        for (alias, id) in por_largo {
            if alias.chars().count() > 2 && Regex::new(&format!(r"\b{}\b", regex::escape(alias))).unwrap().is_match(&t) {
                return (Some(id.clone()), 1.0);
            }
        }
        // aproximación: tolera errores de dictado («ombro», «codito»)
        let (mut mejor, mut puntaje) = (None, 0.0);
        for w in re(r"[a-z]{3,}").find_iter(&t) {
            // difflib.get_close_matches(n=1, cutoff=0.75): el más parecido (en empate, el mayor alfabéticamente)
            let cand = self.vocab.iter().map(|(k, _)| (parecido(w.as_str(), k), k)).filter(|(s, _)| *s >= 0.75).max_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.cmp(b.1)));
            if let Some((s, k)) = cand {
                if s > puntaje {
                    mejor = self.id(k).cloned();
                    puntaje = s;
                }
            }
        }
        (mejor, puntaje)
    }

    /// Una frase puede traer varias órdenes.
    pub fn leer(&self, texto: &str) -> Vec<Orden> {
        // la coma decimal («12,5 grados») no separa órdenes: se pasa a punto antes de cortar la frase
        let t = re(r"(\d),(\d)").replace_all(&sin_tilde(texto), "$1.$2").to_string();
        // «y» seguido de un verbo separa órdenes (en Python era una búsqueda hacia adelante)
        let t = re(r"\by(\s+)(mueve|gira|haz|lleva|ejecuta|guarda|avanza|retrocede|abre|cierra)").replace_all(&t, ",$1$2");
        re(r"\s*(?:,|;|\by luego\b|\by despues\b|\bdespues\b|\bluego\b)\s*")
            .split(&t)
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .filter_map(|c| {
                let mut o = self.leer_una(c)?;
                o.fuente = c.to_string();
                Some(o)
            })
            .collect()
    }

    pub fn leer_una(&self, t: &str) -> Option<Orden> {
        if re(r"\b(alto|detente|para|stop|emergencia|frena)\b").is_match(t) {
            return Some(Orden::new("STOP"));
        }
        if re(r"\b(inicio|home|reposo|posicion inicial)\b").is_match(t) {
            return Some(Orden::new("HOME"));
        }
        if let Some(m) = re(r"\b(?:guarda|graba|memoriza)\b.*?\b(?:como|en)\s+([a-z0-9]{1,6})\b").captures(t) {
            let mut o = Orden::new("SAVE");
            o.params.push(("name".into(), Param::T(m[1].to_uppercase())));
            return Some(o);
        }
        if let Some(m) = re(r"\b(?:ejecuta|posicion|repite|movimiento|secuencia)\s+([0-9]{1,3}[a-z]|[a-z][0-9]{1,3})\b").captures(t) {
            if self.macros.contains_key(&m[1].to_uppercase()) {
                let mut o = Orden::new("MACRO");
                o.params.push(("name".into(), Param::T(m[1].to_uppercase())));
                return Some(o);
            }
        }
        // base con ruedas
        if let Some(m) = re(r"\b(avanza|retrocede)\s+{NUM}\s*(cm|m|mm)?").captures(t) {
            let factor = match m.get(3).map(|x| x.as_str()) {
                Some("m") => 1000.0,
                Some("mm") => 1.0,
                _ => 10.0,
            };
            let d = num(&m[2]) * factor;
            let mut o = Orden::new("DRIVE");
            o.valor = Some(if &m[1] == "avanza" { d } else { -d });
            o.params.push(("unit".into(), Param::T("mm".into())));
            return Some(o);
        }
        if let Some(m) = re(r"\bgira\s+{NUM}\s*(?:grados|°)?\s+(?:a la\s+)?(izquierda|derecha)").captures(t) {
            let mut o = Orden::new("TURN");
            o.valor = Some(num(&m[1]) * if &m[2] == "izquierda" { 1.0 } else { -1.0 });
            return Some(o);
        }
        let (art, conf) = self.articulacion(t);
        if re(r"\bcalibra").is_match(t) {
            let mut o = Orden::new("CALIBRATE");
            o.art = art;
            o.confianza = conf;
            return Some(o);
        }
        let art = art?;
        let con = |tipo: &str| {
            let mut o = Orden::new(tipo);
            o.art = Some(art.clone());
            o.confianza = conf;
            o
        };
        // onda / oscilación
        if re(r"\b(onda|oscila|ondula|balancea|sinusoidal|senoidal)").is_match(t) {
            let amp = re(r"amplitud\s+(?:de\s+)?{NUM}").captures(t).map(|c| num(&c[1]));
            let frq = re(r"frecuencia\s+(?:de\s+)?{NUM}|{NUM}\s*hz").captures(t).map(|c| num(c.get(1).or(c.get(2)).unwrap().as_str()));
            let dur = re(r"(?:durante|por)\s+{NUM}\s*(?:s|seg)").captures(t).map(|c| num(&c[1]));
            let mul = re(r"(?:x|por|multiplicad[oa] por)\s*{NUM}\s*(?:veces)?\s*(?:mas rapido|la velocidad)").captures(t).map(|c| num(&c[1]));
            let mut o = con("WAVE");
            o.params = vec![
                ("amp".into(), Param::N(amp.unwrap_or(if t.contains("suave") { 10.0 } else { 20.0 }))),
                ("freq".into(), Param::N(frq.unwrap_or(0.5))),
                ("dur".into(), Param::N(dur.unwrap_or(4.0))),
                ("mult".into(), Param::N(mul.unwrap_or(1.0))),
            ];
            return Some(o);
        }
        // pinza
        if art == self.pinza.0 && re(r"\b(abre|cierra)\b").is_match(t) {
            let mut o = con("SET");
            o.valor = Some(if t.contains("abre") { self.pinza.2 } else { self.pinza.1 });
            return Some(o);
        }
        let mut params = vec![];
        if let Some(c) = re(r"(?:a|con)\s+(?:velocidad\s+)?{NUM}\s*(?:grados por segundo|°/s|gps)").captures(t) {
            params.push(("vel".to_string(), Param::N(num(&c[1]))));
        }
        if re(r"\b(lento|despacio|suave)\b").is_match(t) && params.is_empty() {
            params.push(("vel".into(), Param::N(30.0)));
        }
        // relativo: «20 grados mas», «baja 10»
        let rel = re(r"{NUM}\s*(?:grados|°)?\s*(mas|menos)\b").captures(t).map(|c| num(&c[1]) * if &c[2] == "mas" { 1.0 } else { -1.0 }).or_else(|| {
            re(r"\b(sube|baja|abre|cierra)\s+(?:el\s+\w+\s+)?{NUM}").captures(t).map(|c| num(&c[2]) * if &c[1] == "sube" || &c[1] == "abre" { 1.0 } else { -1.0 })
        });
        if let Some(v) = rel {
            let mut o = con("MOVE_REL");
            o.valor = Some(v);
            o.params = params;
            return Some(o);
        }
        let abs = re(r"(?:a|en|hasta)\s+{NUM}\s*(?:grados|°|deg)?").captures(t).or_else(|| re(r"{NUM}\s*(?:grados|°|deg)").captures(t));
        abs.map(|c| {
            let mut o = con("SET");
            o.valor = Some(num(&c[1]));
            o.params = params;
            o
        })
    }
}

// ------------------------------------------------------------------ guardián

pub struct Veredicto {
    pub ok: bool,
    pub orden: Orden,
    pub notas: Vec<String>,
    pub necesita_persona: bool,
}

impl std::fmt::Display for Veredicto {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let tag = if self.ok { "OK " } else if self.necesita_persona { "CONFIRMAR" } else { "BLOQUEADO" };
        write!(f, "[{tag}] {}", self.orden)?;
        if !self.notas.is_empty() {
            write!(f, "  · {}", self.notas.join(" · "))?;
        }
        Ok(())
    }
}

pub struct Guardian {
    /// distancia al obstáculo (mm) o None
    pub distancia: Box<dyn Fn() -> Option<f64> + Send>,
    permiso: Option<String>,
}

impl Guardian {
    pub fn new(distancia: Option<Box<dyn Fn() -> Option<f64> + Send>>) -> Self {
        Guardian { distancia: distancia.unwrap_or_else(|| Box::new(|| None)), permiso: None }
    }

    /// Llamado SOLO desde la interfaz de la persona (botón o voz verificada).
    pub fn confirmar_persona(&mut self, regla: &str) {
        self.permiso = Some(regla.into());
    }

    fn usar_permiso(&mut self, regla: &str) -> bool {
        if self.permiso.as_deref() == Some(regla) {
            self.permiso = None;
            return true;
        }
        false
    }

    fn pose_segura(r: &Robot, pose: &[(String, f64)]) -> Result<(), String> {
        let s = &r.seguridad;
        let piso = s["altura_minima_mm"].as_f64().unwrap_or(0.0);
        for (i, (x, y, z)) in r.fk(pose).iter().enumerate().skip(1) {
            if *z < piso {
                return Err(format!("punto {i} bajaría a z={z:.0} mm (< {} mm del piso)", g(piso)));
            }
            for zona in s["zonas_prohibidas"].as_array().into_iter().flatten() {
                let dentro = |k: &str, v: f64| zona[k][0].as_f64().unwrap_or(0.0) <= v && v <= zona[k][1].as_f64().unwrap_or(0.0);
                if dentro("x", *x) && dentro("y", *y) && dentro("z", *z) {
                    return Err(format!("punto {i} entraría a la zona prohibida '{}'", zona["nombre"].as_str().unwrap_or("")));
                }
            }
        }
        Ok(())
    }

    pub fn revisar(&mut self, r: &Robot, mut o: Orden) -> Veredicto {
        let mut notas: Vec<String> = vec![];
        match o.tipo.as_str() {
            "STOP" | "HOME" | "SAVE" | "MACRO" | "CALIBRATE" => {}
            "SET" | "MOVE_REL" => {
                let j = r.art(o.art.as_deref().unwrap()).clone();
                let v = o.valor.unwrap_or(0.0);
                let mut objetivo = if o.tipo == "SET" { v } else { r.angulo(&j.id) + v };
                if objetivo < j.min || objetivo > j.max {
                    let rec = objetivo.clamp(j.min, j.max);
                    notas.push(format!("{} {}° fuera de rango [{},{}] → recortado a {}°", j.id, g(objetivo), g(j.min), g(j.max), g(rec)));
                    objetivo = rec;
                }
                let mut vel = o.p("vel").unwrap_or(j.vel_max);
                if vel > j.vel_max {
                    notas.push(format!("velocidad {}°/s > máx {} → limitada", g(vel), g(j.vel_max)));
                    vel = j.vel_max;
                }
                let mut pose = r.pose.clone();
                Robot::fijar(&mut pose, &j.id, objetivo);
                if let Err(por) = Self::pose_segura(r, &pose) {
                    if self.usar_permiso("zona") {
                        notas.push(format!("{por} — AUTORIZADO por la persona"));
                    } else {
                        notas.push(por);
                        return Veredicto { ok: false, orden: o, notas, necesita_persona: true };
                    }
                }
                o.tipo = "SET".into();
                o.valor = Some(objetivo);
                o.poner("vel", Param::N(vel));
            }
            "WAVE" => {
                let j = r.art(o.art.as_deref().unwrap()).clone();
                let mut w = Onda { articulacion: j.id.clone(), amplitud: o.p("amp").unwrap(), frecuencia: o.p("freq").unwrap(), centro: r.angulo(&j.id), fase: 0.0, duracion: o.p("dur").unwrap(), mult: o.p("mult").unwrap() };
                if w.centro - w.amplitud.abs() < j.min || w.centro + w.amplitud.abs() > j.max {
                    let lugar = (w.centro - j.min).min(j.max - w.centro);
                    notas.push(format!("amplitud {}° excede el rango → {}°", g(w.amplitud), g(lugar)));
                    w.amplitud = lugar;
                    o.poner("amp", Param::N(lugar));
                }
                if w.pico() > j.vel_max {
                    let fmax = w.frecuencia_max(j.vel_max);
                    notas.push(format!("pico {:.0}°/s > {}°/s → frecuencia efectiva {fmax:.2} Hz", w.pico(), g(j.vel_max)));
                    o.poner("mult", Param::N(fmax / o.p("freq").unwrap()));
                }
                for tt in [0.25 / w.f(), 0.75 / w.f()] {
                    // extremos de la onda
                    let mut pose = r.pose.clone();
                    Robot::fijar(&mut pose, &j.id, w.angulo(tt));
                    if let Err(por) = Self::pose_segura(r, &pose) {
                        notas.push(por);
                        return Veredicto { ok: false, orden: o, notas, necesita_persona: true };
                    }
                }
            }
            "DRIVE" => {
                let freno = r.seguridad["distancia_frenado_mm"].as_f64().unwrap_or(150.0);
                let v = o.valor.unwrap_or(0.0);
                if let Some(dist) = (self.distancia)() {
                    if v > 0.0 && v > dist - freno {
                        if self.usar_permiso("choque") {
                            notas.push("avance hasta contacto AUTORIZADO por la persona".into());
                        } else {
                            let permitido = (dist - freno).max(0.0);
                            notas.push(format!("obstáculo a {} mm → avance limitado a {} mm", g(dist), g(permitido)));
                            o.valor = Some(permitido);
                        }
                    }
                }
            }
            _ => {}
        }
        Veredicto { ok: true, orden: o, notas, necesita_persona: false }
    }
}

// ------------------------------------------------------------------ memoria del robot

/// Posiciones guardadas (macros tipo «1A»), calibración y bitácora, en JSON legible en ~/.orion/robots/<nombre>/.
pub struct MemoriaRobot {
    pub dir: PathBuf,
}

fn leer_json(f: &Path, defecto: Value) -> Value {
    std::fs::read_to_string(f).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(defecto)
}

fn red(x: f64, n: i32) -> f64 {
    let k = 10f64.powi(n);
    (x * k).round_ties_even() / k
}

impl MemoriaRobot {
    pub fn new(nombre: &str, raiz: Option<&Path>) -> Self {
        let seguro: String = nombre.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).collect::<String>().trim_matches('-').to_string();
        let dir = raiz.map(Path::to_path_buf).unwrap_or_else(|| crate::util::casa().join(".orion/robots")).join(seguro);
        let _ = std::fs::create_dir_all(&dir);
        MemoriaRobot { dir }
    }
    pub fn macros(&self) -> Map<String, Value> {
        leer_json(&self.dir.join("posiciones.json"), json!({})).as_object().cloned().unwrap_or_default()
    }
    pub fn guardar_macro(&self, nombre: &str, pose: Value, descripcion: &str) {
        let mut m = self.macros();
        m.insert(nombre.to_uppercase(), json!({"pose": pose, "descripcion": descripcion, "creado": chrono::Local::now().format("%F %T").to_string()}));
        let _ = std::fs::write(self.dir.join("posiciones.json"), serde_json::to_string_pretty(&m).unwrap());
    }
    pub fn calibracion(&self) -> Value {
        leer_json(&self.dir.join("calibracion.json"), json!({"foto": null, "servos": {}}))
    }
    pub fn marcar_servo(&self, art: &str, px: (i64, i64), vista: &str, foto: &str) {
        let mut c = self.calibracion();
        c["foto"] = json!(foto);
        let s = &mut c["servos"][art];
        if !s.is_object() {
            *s = json!({});
        }
        s["px"] = json!([px.0, px.1]);
        s["vista"] = json!(vista);
        let _ = std::fs::write(self.dir.join("calibracion.json"), serde_json::to_string_pretty(&c).unwrap());
    }
    /// Cada prueba de calibración: ángulo mandado vs. medido por la cámara → ajuste lineal (dirección, escala, offset).
    pub fn prueba_calibracion(&self, art: &str, mandado: f64, medido: f64) -> Value {
        let mut c = self.calibracion();
        if !c["servos"][art].is_object() {
            c["servos"][art] = json!({});
        }
        let s = &mut c["servos"][art];
        if !s["pruebas"].is_array() {
            s["pruebas"] = json!([]);
        }
        s["pruebas"].as_array_mut().unwrap().push(json!([mandado, medido]));
        let pts: Vec<(f64, f64)> = s["pruebas"].as_array().unwrap().iter().map(|p| (p[0].as_f64().unwrap_or(0.0), p[1].as_f64().unwrap_or(0.0))).collect();
        if pts.len() >= 2 {
            let n = pts.len() as f64;
            let mx = pts.iter().map(|p| p.0).sum::<f64>() / n;
            let my = pts.iter().map(|p| p.1).sum::<f64>() / n;
            let sxx = pts.iter().map(|p| (p.0 - mx).powi(2)).sum::<f64>();
            let sxx = if sxx == 0.0 { 1e-9 } else { sxx };
            let pend = pts.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum::<f64>() / sxx;
            s["direccion"] = json!(if pend >= 0.0 { 1 } else { -1 });
            s["escala"] = json!(red(pend.abs(), 4));
            s["offset"] = json!(red(my - pend * mx, 2));
        }
        let r = s.clone();
        let _ = std::fs::write(self.dir.join("calibracion.json"), serde_json::to_string_pretty(&c).unwrap());
        r
    }
    pub fn anotar(&self, ev: Value) {
        use std::io::Write;
        let mut l = json!({"t": chrono::Local::now().format("%F %T").to_string()});
        for (k, v) in ev.as_object().into_iter().flatten() {
            l[k] = v.clone();
        }
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(self.dir.join("bitacora.jsonl")) {
            let _ = writeln!(f, "{l}");
        }
    }
}

// ------------------------------------------------------------------ todo junto

/// Lo que recibe el driver: servos (Feetech/Dynamixel/PCA9685), un publicador ROS 2 o la simulación.
pub trait Driver: Send {
    fn articulacion(&mut self, art: &str, grados: f64, vel: Option<f64>);
    fn trayectoria(&mut self, art: &str, muestras: &[(f64, f64)]);
    fn avanzar(&mut self, mm: f64);
    fn parar(&mut self);
}

/// Driver de simulación: anota lo que habría mandado.
#[derive(Default)]
pub struct DriverSim {
    pub enviado: Vec<String>,
}

impl Driver for DriverSim {
    fn articulacion(&mut self, art: &str, grados: f64, vel: Option<f64>) {
        self.enviado.push(format!("SET {art} {} {}", (grados * 100.0).round() / 100.0, vel.map(g).unwrap_or_else(|| "None".into())));
    }
    fn trayectoria(&mut self, art: &str, m: &[(f64, f64)]) {
        self.enviado.push(format!("TRAJ {art} {}", m.len()));
    }
    fn avanzar(&mut self, mm: f64) {
        self.enviado.push(format!("DRIVE {}", (mm * 10.0).round() / 10.0));
    }
    fn parar(&mut self) {
        self.enviado.push("STOP".into());
    }
}

pub struct Orion {
    pub robot: Robot,
    pub mem: MemoriaRobot,
    pub traductor: Traductor,
    pub guardian: Guardian,
    pub driver: Box<dyn Driver>,
}

impl Orion {
    pub fn new(robot: Robot, driver: Option<Box<dyn Driver>>, distancia: Option<Box<dyn Fn() -> Option<f64> + Send>>, raiz_memoria: Option<&Path>) -> Self {
        let mem = MemoriaRobot::new(&robot.nombre, raiz_memoria);
        let traductor = Traductor::new(&robot, mem.macros());
        Orion { robot, mem, traductor, guardian: Guardian::new(distancia), driver: driver.unwrap_or_else(|| Box::new(DriverSim::default())) }
    }

    /// La IA «habla»; esto devuelve lo que pasó, línea a línea.
    pub fn decir(&mut self, texto: &str) -> Vec<String> {
        self.traductor.macros = self.mem.macros();
        let ordenes = self.traductor.leer(texto);
        if ordenes.is_empty() {
            return vec![format!("? no entendí ninguna orden en: «{texto}»")];
        }
        let mut out = vec![];
        for o in ordenes {
            let fuente = o.fuente.clone();
            let v = self.guardian.revisar(&self.robot, o);
            out.push(v.to_string());
            self.mem.anotar(json!({"frase": fuente, "orden": v.orden.to_string(), "ok": v.ok, "notas": v.notas}));
            if v.ok {
                self.ejecutar(&v.orden);
            }
        }
        out
    }

    fn ejecutar(&mut self, o: &Orden) {
        match o.tipo.as_str() {
            "SET" => {
                let (a, v) = (o.art.clone().unwrap(), o.valor.unwrap());
                Robot::fijar(&mut self.robot.pose, &a, v);
                self.driver.articulacion(&a, v, o.p("vel"));
            }
            "HOME" => {
                for j in self.robot.articulaciones.clone() {
                    Robot::fijar(&mut self.robot.pose, &j.id, j.home);
                    self.driver.articulacion(&j.id, j.home, Some(j.vel_max * 0.3));
                }
            }
            "WAVE" => {
                let a = o.art.clone().unwrap();
                let w = Onda { articulacion: a.clone(), amplitud: o.p("amp").unwrap(), frecuencia: o.p("freq").unwrap(), centro: self.robot.angulo(&a), fase: 0.0, duracion: o.p("dur").unwrap(), mult: o.p("mult").unwrap() };
                self.driver.trayectoria(&a, &w.muestras(50.0));
            }
            "SAVE" => {
                if let Some((_, Param::T(n))) = o.params.iter().find(|x| x.0 == "name") {
                    self.mem.guardar_macro(n, self.robot.pose_json(), "");
                }
            }
            "MACRO" => {
                if let Some((_, Param::T(n))) = o.params.iter().find(|x| x.0 == "name") {
                    let pose = self.mem.macros()[n]["pose"].clone();
                    for (id, deg) in pose.as_object().into_iter().flatten() {
                        self.decir(&format!("{id} a {} grados", deg.as_f64().unwrap_or(0.0)));
                    }
                }
            }
            "DRIVE" | "TURN" => self.driver.avanzar(o.valor.unwrap_or(0.0)),
            "STOP" => self.driver.parar(),
            _ => {}
        }
    }
}

/// El perfil del brazo que viene con Neo Orión.
pub const PERFIL_BRAZO: &str = include_str!("../../../config/robots/brazo-6dof.json");

pub fn perfil_por_defecto() -> Robot {
    ["/usr/share/orion/robots/brazo-6dof.json"].iter().find_map(|p| Robot::cargar(Path::new(p)).ok()).unwrap_or_else(|| Robot::desde_json(&serde_json::from_str(PERFIL_BRAZO).unwrap()).unwrap())
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn formato_g_como_python() {
        for (v, s) in [(120.0, "120"), (0.265258238, "0.265258"), (-30.0, "-30"), (2.5, "2.5"), (1e-5, "1e-05"), (1234567.0, "1.23457e+06"), (0.0001, "0.0001")] {
            assert_eq!(g(v), s);
        }
    }

    struct Compartido(Arc<Mutex<Vec<String>>>);
    impl Driver for Compartido {
        fn articulacion(&mut self, a: &str, d: f64, v: Option<f64>) {
            self.0.lock().unwrap().push(format!("SET {a} {d} {v:?}"));
        }
        fn trayectoria(&mut self, a: &str, m: &[(f64, f64)]) {
            self.0.lock().unwrap().push(format!("TRAJ {a} {}", m.len()));
        }
        fn avanzar(&mut self, mm: f64) {
            self.0.lock().unwrap().push(format!("DRIVE {mm}"));
        }
        fn parar(&mut self) {
            self.0.lock().unwrap().push("STOP".into());
        }
    }

    /// La misma demostración que tests/demo.py de Python, línea por línea.
    #[test]
    fn misma_salida_que_python() {
        let d = tempfile::tempdir().unwrap();
        let enviado = Arc::new(Mutex::new(vec![]));
        let mut o = Orion::new(perfil_por_defecto(), Some(Box::new(Compartido(enviado.clone()))), Some(Box::new(|| Some(400.0))), Some(d.path()));
        let casos: [(&str, &[&str]); 11] = [
            ("Mueve el hombro a 45 grados", &["[OK ] SET A2 45 vel=120"]),
            ("baja el codo 30 y luego abre la pinza", &["[OK ] SET A3 -30 vel=150", "[OK ] SET A6 100 vel=300"]),
            ("servo A1 a 200 grados", &["[OK ] SET A1 150 vel=180  · A1 200° fuera de rango [-150,150] → recortado a 150°"]),
            ("ombro a 20 grados lento", &["[OK ] SET A2 20 vel=30"]),
            ("guarda esta posición como 1A", &["[OK ] SAVE name=1A"]),
            ("haz una onda en el codo amplitud 25 frecuencia 0.5 hz durante 6 s", &["[OK ] WAVE A3 amp=25 freq=0.5 dur=6 mult=1"]),
            ("onda en la muñeca amplitud 60 frecuencia 2 hz", &["[OK ] WAVE A4 amp=60 freq=2 dur=4 mult=0.265258  · pico 754°/s > 200°/s → frecuencia efectiva 0.53 Hz"]),
            ("posicion inicial", &["[OK ] HOME"]),
            ("ejecuta 1A", &["[OK ] MACRO name=1A"]),
            ("avanza 50 cm", &["[OK ] DRIVE 250 unit=mm  · obstáculo a 400 mm → avance limitado a 250 mm"]),
            ("detente", &["[OK ] STOP"]),
        ];
        for (frase, esperado) in casos {
            assert_eq!(o.decir(frase), esperado.iter().map(|s| s.to_string()).collect::<Vec<_>>(), "frase: {frase}");
        }
        assert_eq!(o.robot.pose_json(), json!({"A1": 150.0, "A2": 20.0, "A3": -30.0, "A4": 0.0, "A5": 0.0, "A6": 100.0}));
        assert_eq!(o.robot.efector(), (-1.0, 0.6, 460.4));
        assert_eq!(enviado.lock().unwrap().len(), 21);
        let w = Onda::new("A3", 25.0, 0.5);
        assert_eq!(format!("{:.1}", w.pico()), "78.5");
        let mut s = json!({});
        for (m, x) in [(0.0, 3.0), (45.0, 47.5), (90.0, 92.0)] {
            s = o.mem.prueba_calibracion("A2", m, x);
        }
        assert_eq!((s["direccion"].clone(), s["escala"].clone(), s["offset"].clone()), (json!(1), json!(0.9889), json!(3.0)));
    }

    #[test]
    fn el_guardian_protege() {
        let d = tempfile::tempdir().unwrap();
        let mut o = Orion::new(perfil_por_defecto(), None, Some(Box::new(|| Some(400.0))), Some(d.path()));
        // el brazo estirado hacia el monitor entra a la zona prohibida: lo pide la IA, lo autoriza solo la persona
        o.decir("hombro a 90 grados");
        let r = o.decir("muñeca a 0 grados y luego base a 0 grados");
        assert!(r.iter().all(|l| l.starts_with("[OK ]")));
        let r = o.decir("hombro a -90 grados");
        assert!(r[0].starts_with("[CONFIRMAR]") || r[0].starts_with("[OK ]"), "{r:?}");
        if r[0].starts_with("[CONFIRMAR]") {
            assert_eq!(o.robot.angulo("A2"), 90.0); // no se movió
            o.guardian.confirmar_persona("zona");
            assert!(o.decir("hombro a -90 grados")[0].contains("AUTORIZADO por la persona"));
            assert!(o.decir("hombro a -89 grados")[0].starts_with("[CONFIRMAR]")); // el permiso sirve una sola vez
        }
        assert_eq!(o.decir("hola qué tal")[0], "? no entendí ninguna orden en: «hola qué tal»");
        // la coma decimal chilena no corta la orden (en la versión Python «12,5» quedaba en 12)
        o.decir("posicion inicial");
        assert_eq!(o.decir("codo a 12,5 grados"), vec!["[OK ] SET A3 12.5 vel=150".to_string()]);
        assert_eq!(o.decir("codo a 10, luego muñeca a 5").len(), 2);
        // avance autorizado hasta contacto: una sola vez
        o.guardian.confirmar_persona("choque");
        assert!(o.decir("avanza 50 cm")[0].contains("AUTORIZADO"));
        assert!(o.decir("avanza 50 cm")[0].contains("limitado a 250 mm"));
    }
}
