//! ¿Quién está hablando? Huella de voz (vector) por frase y un registro por persona.
//!
//! Huellas: sherpa-onnx (3D-Speaker/ECAPA, Apache-2.0, por su librería C cargada al vuelo) o la espectral propia
//! (MFCC + tono, sin modelos). Nombres por contexto: «soy Constanza», «me llamo Jorge»; el vocativo «Jorge, …» con
//! respuesta de una voz nueva queda como sugerencia hasta repetirse 2 veces.
//! PRIVACIDAD: la huella es un dato biométrico. Modo «anonimo» (terceros sin huella permanente) por defecto.

use crate::util::sin_tilde;
use crate::voz::RATE;
use regex::Regex;
use rustfft::{num_complex::Complex, FftPlanner};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

pub fn coseno(a: &[f64], b: &[f64]) -> f64 {
    let (n, na, nb) = a.iter().zip(b).fold((0.0, 0.0, 0.0), |(n, x, y), (p, q)| (n + p * q, x + p * p, y + q * q));
    let d = (na * nb).sqrt();
    if d == 0.0 { 0.0 } else { n / d }
}
fn normalizar(v: &mut [f64]) {
    let n = v.iter().map(|x| x * x).sum::<f64>().sqrt() + 1e-9;
    v.iter_mut().for_each(|x| *x /= n);
}

pub trait Huella: Send {
    fn nombre(&self) -> &str;
    fn vector(&mut self, pcm: &[i16]) -> Vec<f64>;
}

/// Sin modelos: estadísticos de MFCC (timbre) + tono fundamental.
pub struct HuellaEspectral {
    fb: Vec<Vec<f64>>,
    dct: Vec<Vec<f64>>,
    nfft: usize,
    fft: std::sync::Arc<dyn rustfft::Fft<f64>>,
}

impl HuellaEspectral {
    pub fn new() -> Self {
        let (n_mfcc, n_mel, nfft) = (20usize, 40usize, 512usize);
        let mel = |f: f64| 2595.0 * (1.0 + f / 700.0).log10();
        let imel = |m: f64| 700.0 * (10f64.powf(m / 2595.0) - 1.0);
        let (a, b) = (mel(60.0), mel(7600.0));
        let bins: Vec<usize> = (0..n_mel + 2).map(|i| ((nfft + 1) as f64 * imel(a + (b - a) * i as f64 / (n_mel + 1) as f64) / RATE as f64).floor() as usize).collect();
        let mut fb = vec![vec![0.0; nfft / 2 + 1]; n_mel];
        for i in 0..n_mel {
            let (a, b, c) = (bins[i], bins[i + 1], bins[i + 2]);
            for k in a..b {
                fb[i][k] = (k - a) as f64 / (b - a).max(1) as f64;
            }
            for k in b..c {
                fb[i][k] = (c - k) as f64 / (c - b).max(1) as f64;
            }
        }
        let dct = (0..n_mfcc).map(|k| (0..n_mel).map(|n| (std::f64::consts::PI / n_mel as f64 * (n as f64 + 0.5) * k as f64).cos()).collect()).collect();
        HuellaEspectral { fb, dct, nfft, fft: FftPlanner::new().plan_fft_forward(nfft) }
    }
}
impl Default for HuellaEspectral {
    fn default() -> Self {
        Self::new()
    }
}

impl Huella for HuellaEspectral {
    fn nombre(&self) -> &str {
        "espectral"
    }
    fn vector(&mut self, pcm: &[i16]) -> Vec<f64> {
        let x0: Vec<f64> = pcm.iter().map(|&v| v as f64 / 32768.0).collect();
        if x0.len() < 800 {
            return vec![0.0; 39];
        }
        let mut x = vec![x0[0]];
        x.extend(x0.windows(2).map(|w| w[1] - 0.97 * w[0]));
        let (hop, win) = (160usize, 400usize);
        let n = 1 + (x.len().saturating_sub(win)) / hop;
        let ham: Vec<f64> = (0..win).map(|i| 0.54 - 0.46 * (2.0 * std::f64::consts::PI * i as f64 / (win - 1) as f64).cos()).collect();
        let frames: Vec<Vec<f64>> = (0..n).map(|i| (0..win).map(|j| x.get(i * hop + j).copied().unwrap_or(0.0) * ham[j]).collect()).collect();
        let energia: Vec<f64> = frames.iter().map(|f| f.iter().map(|v| v * v).sum::<f64>() / win as f64).collect();
        let mut orden = energia.clone();
        orden.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p40 = orden[((orden.len() - 1) as f64 * 0.4) as usize];
        let voz: Vec<&Vec<f64>> = frames.iter().zip(&energia).filter(|(_, e)| **e > p40).map(|(f, _)| f).collect();
        let voz = if voz.is_empty() { frames.iter().collect() } else { voz };
        let mut mfccs: Vec<Vec<f64>> = vec![];
        for f in &voz {
            let mut buf: Vec<Complex<f64>> = (0..self.nfft).map(|i| Complex::new(f.get(i).copied().unwrap_or(0.0), 0.0)).collect();
            self.fft.process(&mut buf);
            let spec: Vec<f64> = buf[..self.nfft / 2 + 1].iter().map(|c| c.norm_sqr()).collect();
            let logmel: Vec<f64> = self.fb.iter().map(|w| (w.iter().zip(&spec).map(|(a, b)| a * b).sum::<f64>() + 1e-9).ln()).collect();
            mfccs.push(self.dct.iter().skip(1).map(|d| d.iter().zip(&logmel).map(|(a, b)| a * b).sum()).collect());
        }
        let k = mfccs[0].len();
        let media: Vec<f64> = (0..k).map(|i| mfccs.iter().map(|m| m[i]).sum::<f64>() / mfccs.len() as f64).collect();
        let desv: Vec<f64> = (0..k).map(|i| (mfccs.iter().map(|m| (m[i] - media[i]).powi(2)).sum::<f64>() / mfccs.len() as f64).sqrt()).collect();
        // tono fundamental por autocorrelación (70–400 Hz)
        let paso = (voz.len() / 60).max(1);
        let mut f0s: Vec<f64> = vec![];
        for f in voz.iter().step_by(paso) {
            let ac = |l: usize| f.iter().zip(f.iter().skip(l)).map(|(a, b)| a * b).sum::<f64>();
            let (lo, hi) = (RATE / 400, RATE / 70);
            let (kmax, vmax) = (lo..hi).map(|l| (l, ac(l))).fold((lo, f64::MIN), |m, x| if x.1 > m.1 { x } else { m });
            if vmax > 0.3 * ac(0) {
                f0s.push(RATE as f64 / kmax as f64);
            }
        }
        f0s.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let f0 = if f0s.is_empty() { 0.0 } else { f0s[f0s.len() / 2] };
        let mut v = media;
        v.extend(desv);
        v.push((f0 + 1.0).ln() * 6.0);
        normalizar(&mut v);
        v
    }
}

/// sherpa-onnx por su API C (libsherpa-onnx-c-api.so), con un modelo de huella ONNX.
pub struct HuellaSherpa {
    _lib: libloading::Library,
    ex: *const std::ffi::c_void,
    crear_stream: unsafe extern "C" fn(*const std::ffi::c_void) -> *const std::ffi::c_void,
    aceptar: unsafe extern "C" fn(*const std::ffi::c_void, i32, *const f32, i32),
    terminar: unsafe extern "C" fn(*const std::ffi::c_void),
    calcular: unsafe extern "C" fn(*const std::ffi::c_void, *const std::ffi::c_void) -> *const f32,
    liberar: unsafe extern "C" fn(*const f32),
    destruir: unsafe extern "C" fn(*const std::ffi::c_void),
    dim: usize,
}
unsafe impl Send for HuellaSherpa {}

#[repr(C)]
struct SherpaConfig {
    model: *const std::ffi::c_char,
    num_threads: i32,
    debug: i32,
    provider: *const std::ffi::c_char,
}

impl HuellaSherpa {
    pub fn new(modelo: &str) -> Result<Self, String> {
        if !std::path::Path::new(modelo).exists() {
            return Err("falta el modelo de huella".into());
        }
        unsafe {
            let lib = ["libsherpa-onnx-c-api.so", "/usr/local/lib/libsherpa-onnx-c-api.so", "/opt/orion/lib/libsherpa-onnx-c-api.so"].iter().find_map(|p| libloading::Library::new(p).ok()).ok_or("falta libsherpa-onnx-c-api.so")?;
            let crear: libloading::Symbol<unsafe extern "C" fn(*const SherpaConfig) -> *const std::ffi::c_void> = lib.get(b"SherpaOnnxCreateSpeakerEmbeddingExtractor").map_err(|e| e.to_string())?;
            let dimf: libloading::Symbol<unsafe extern "C" fn(*const std::ffi::c_void) -> i32> = lib.get(b"SherpaOnnxSpeakerEmbeddingExtractorDim").map_err(|e| e.to_string())?;
            let (m, p) = (std::ffi::CString::new(modelo).unwrap(), std::ffi::CString::new("cpu").unwrap());
            let cfg = SherpaConfig { model: m.as_ptr(), num_threads: 1, debug: 0, provider: p.as_ptr() };
            let ex = crear(&cfg);
            if ex.is_null() {
                return Err("sherpa-onnx no abrió el modelo".into());
            }
            let dim = dimf(ex) as usize;
            Ok(HuellaSherpa {
                crear_stream: *lib.get(b"SherpaOnnxSpeakerEmbeddingExtractorCreateStream").map_err(|e| e.to_string())?,
                aceptar: *lib.get(b"SherpaOnnxOnlineStreamAcceptWaveform").map_err(|e| e.to_string())?,
                terminar: *lib.get(b"SherpaOnnxOnlineStreamInputFinished").map_err(|e| e.to_string())?,
                calcular: *lib.get(b"SherpaOnnxSpeakerEmbeddingExtractorComputeEmbedding").map_err(|e| e.to_string())?,
                liberar: *lib.get(b"SherpaOnnxSpeakerEmbeddingExtractorDestroyEmbedding").map_err(|e| e.to_string())?,
                destruir: *lib.get(b"SherpaOnnxDestroyOnlineStream").map_err(|e| e.to_string())?,
                _lib: lib,
                ex,
                dim,
            })
        }
    }
}
impl Huella for HuellaSherpa {
    fn nombre(&self) -> &str {
        "sherpa-onnx"
    }
    fn vector(&mut self, pcm: &[i16]) -> Vec<f64> {
        let x: Vec<f32> = pcm.iter().map(|&v| v as f32 / 32768.0).collect();
        unsafe {
            let s = (self.crear_stream)(self.ex);
            (self.aceptar)(s, RATE as i32, x.as_ptr(), x.len() as i32);
            (self.terminar)(s);
            let e = (self.calcular)(self.ex, s);
            let mut v: Vec<f64> = std::slice::from_raw_parts(e, self.dim).iter().map(|&f| f as f64).collect();
            (self.liberar)(e);
            (self.destruir)(s);
            normalizar(&mut v);
            v
        }
    }
}

pub fn crear_huella(nombre: &str) -> Box<dyn Huella> {
    if nombre == "auto" || nombre == "sherpa-onnx" {
        if let Ok(h) = HuellaSherpa::new(&crate::util::casa().join(".orion/modelos/3dspeaker_eres2net_base.onnx").to_string_lossy()) {
            return Box::new(h);
        }
    }
    Box::new(HuellaEspectral::new())
}

// ================================================================ registro de personas
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Persona {
    pub id: String,
    #[serde(default)]
    pub nombre: String,
    #[serde(default)]
    pub centro: Vec<f64>,
    #[serde(default)]
    pub frases: i64,
    #[serde(default)]
    pub acepto: bool,
    #[serde(default)]
    pub sugerencias: BTreeMap<String, i64>,
}
impl Persona {
    pub fn etiqueta(&self) -> &str {
        if self.nombre.is_empty() { &self.id } else { &self.nombre }
    }
}

pub struct Hablantes {
    pub h: Box<dyn Huella>,
    pub umbral: f64,
    pub ruta: PathBuf,
    pub modo: String,
    pub personas: Vec<Persona>,
    hist: Vec<Vec<f64>>,
    ultimo: Option<usize>,
    ultimo_v: Vec<f64>,
    vocativo: Option<(String, Instant)>,
}

impl Hablantes {
    pub fn new(h: Box<dyn Huella>, ruta: Option<PathBuf>, modo: &str) -> Self {
        let umbral = match h.nombre() { "espectral" => 0.25, "sherpa-onnx" => 0.55, _ => 0.6 };
        let ruta = ruta.unwrap_or_else(|| crate::util::casa().join(".orion/voces/perfiles.json"));
        let personas = std::fs::read_to_string(&ruta).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        Hablantes { h, umbral, ruta, modo: modo.into(), personas, hist: vec![], ultimo: None, ultimo_v: vec![], vocativo: None }
    }

    fn centrar(&self) -> bool {
        self.h.nombre() == "espectral"
    }

    fn cmp(&self, a: &[f64], b: &[f64]) -> f64 {
        if self.centrar() && self.hist.len() > 2 {
            let ult = &self.hist[self.hist.len().saturating_sub(300)..];
            let mu: Vec<f64> = (0..a.len()).map(|i| ult.iter().map(|v| v[i]).sum::<f64>() / ult.len() as f64).collect();
            let (x, y): (Vec<f64>, Vec<f64>) = (a.iter().zip(&mu).map(|(p, m)| p - m).collect(), b.iter().zip(&mu).map(|(p, m)| p - m).collect());
            return coseno(&x, &y);
        }
        coseno(a, b)
    }

    /// Registra al dueño (o a quien acepte) con unas frases.
    pub fn enrolar(&mut self, nombre: &str, muestras: &[Vec<i16>], acepto: bool) -> usize {
        let vs: Vec<Vec<f64>> = muestras.iter().map(|m| self.h.vector(m)).collect();
        self.hist.extend(vs.iter().cloned());
        let mut c: Vec<f64> = (0..vs[0].len()).map(|i| vs.iter().map(|v| v[i]).sum::<f64>() / vs.len() as f64).collect();
        normalizar(&mut c);
        self.personas.push(Persona { id: format!("P{}", self.personas.len() + 1), nombre: nombre.into(), centro: c, frases: muestras.len() as i64, acepto, ..Default::default() });
        self.guardar();
        self.personas.len() - 1
    }

    /// Asigna la frase a una persona (nueva si no se parece a nadie). Devuelve (índice, parecido).
    pub fn identificar(&mut self, pcm: &[i16]) -> (usize, f64) {
        let v = self.h.vector(pcm);
        self.hist.push(v.clone());
        self.ultimo_v = v.clone();
        let mejor = self.personas.iter().enumerate().map(|(i, p)| (i, self.cmp(&v, &p.centro))).fold(None, |m: Option<(usize, f64)>, x| if m.map_or(true, |m| x.1 > m.1) { Some(x) } else { m });
        let (i, sim) = match mejor {
            Some((i, s)) if s >= self.umbral => {
                let p = &mut self.personas[i];
                let mut c: Vec<f64> = p.centro.iter().zip(&v).map(|(a, b)| a * 0.8 + b * 0.2).collect();
                normalizar(&mut c);
                p.centro = c;
                (i, s)
            }
            _ => {
                self.personas.push(Persona { id: format!("Persona {}", self.personas.len() + 1), centro: v, ..Default::default() });
                (self.personas.len() - 1, 1.0)
            }
        };
        self.personas[i].frases += 1;
        (i, sim)
    }

    /// Aprende nombres por contexto. Devuelve el índice de la persona (puede cambiar si se fusiona o se separa).
    pub fn aprender(&mut self, mut i: usize, texto: &str) -> usize {
        let t = texto.trim();
        let presenta = Regex::new(r"\b(?:soy|me llamo|mi nombre es|habla)\s+([A-ZÁÉÍÓÚÑ][a-záéíóúñ]+)").unwrap().captures(t).map(|c| c[1].to_string());
        if let Some(n) = &presenta {
            if !self.personas[i].nombre.is_empty() && &self.personas[i].nombre != n {
                // una voz conocida «se presenta» con otro nombre: la identificación falló → perfil nuevo
                self.personas[i].frases -= 1;
                self.personas.push(Persona { id: format!("Persona {}", self.personas.len() + 1), nombre: n.clone(), centro: self.ultimo_v.clone(), frases: 1, ..Default::default() });
                i = self.personas.len() - 1;
            } else {
                i = self.nombrar(i, n, true);
            }
        } else if let Some((nom, cuando)) = self.vocativo.clone() {
            if Some(i) != self.ultimo && cuando.elapsed().as_secs() < 30 && self.personas[i].nombre.is_empty() {
                let veces = {
                    let s = self.personas[i].sugerencias.entry(nom.clone()).or_insert(0);
                    *s += 1;
                    *s
                };
                if veces >= 2 {
                    i = self.nombrar(i, &nom, false);
                }
            }
        }
        if let Some(c) = Regex::new(r"^\s*(?:oye\s+|hola\s+)?([A-ZÁÉÍÓÚÑ][a-záéíóúñ]+)\s*[,!]").unwrap().captures(t) {
            let v = c[1].to_string();
            if !["orion", "neo", "hola", "oye", "bueno", "ya", "si", "no", "mira"].contains(&sin_tilde(&v).as_str()) && self.personas[i].nombre != v {
                self.vocativo = Some((v, Instant::now()));
            }
        }
        self.ultimo = Some(i);
        i
    }

    fn nombrar(&mut self, i: usize, nombre: &str, seguro: bool) -> usize {
        if let Some(j) = self.personas.iter().position(|q| q.nombre == nombre).filter(|&j| j != i) {
            // misma persona con dos perfiles → se fusionan
            let (p, o) = (self.personas[i].clone(), &mut self.personas[j]);
            let mut c: Vec<f64> = o.centro.iter().zip(&p.centro).map(|(a, b)| a * o.frases as f64 + b * p.frases as f64).collect();
            normalizar(&mut c);
            o.centro = c;
            o.frases += p.frases;
            self.personas.remove(i);
            return if j > i { j - 1 } else { j };
        }
        self.personas[i].nombre = nombre.into();
        if seguro {
            self.personas[i].sugerencias.clear();
        }
        i
    }

    pub fn olvidar(&mut self, nombre: &str) -> bool {
        let antes = self.personas.len();
        self.personas.retain(|p| sin_tilde(p.etiqueta()) != sin_tilde(nombre));
        self.guardar();
        self.personas.len() < antes
    }

    pub fn guardar(&self) {
        let guardables: Vec<&Persona> = self.personas.iter().filter(|p| p.acepto || self.modo == "con_nombres").collect();
        let _ = std::fs::create_dir_all(self.ruta.parent().unwrap());
        let _ = std::fs::write(&self.ruta, serde_json::to_string(&guardables).unwrap_or_default());
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn voz(f0: f64, formantes: &[f64], semilla: u64, segundos: f64) -> Vec<i16> {
        let mut s = semilla;
        (0..(segundos * RATE as f64) as usize)
            .map(|i| {
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let t = i as f64 / RATE as f64;
                let vib = 1.0 + 0.02 * (2.0 * std::f64::consts::PI * 5.0 * t).sin();
                let mut v = 0.0;
                for h in 1..12 {
                    let f = f0 * vib * h as f64;
                    let g: f64 = formantes.iter().map(|fm| (-((f - fm) / 180.0).powi(2)).exp()).sum::<f64>() + 0.05;
                    v += g * (2.0 * std::f64::consts::PI * f * t).sin() / h as f64;
                }
                ((v * 0.3 + ((s >> 33) as f64 / 4.3e9 - 0.5) * 0.01) * 20000.0).clamp(-32000.0, 32000.0) as i16
            })
            .collect()
    }

    #[test]
    fn distingue_voces_y_aprende_nombres() {
        let d = tempfile::tempdir().unwrap();
        let mut h = Hablantes::new(Box::new(HuellaEspectral::new()), Some(d.path().join("v.json")), "anonimo");
        let davis = [500.0, 1500.0];
        let ana = [850.0, 2300.0];
        h.enrolar("Davis", &[voz(110.0, &davis, 1, 1.5), voz(112.0, &davis, 2, 1.5), voz(109.0, &davis, 3, 1.5)], true);
        let (j, _) = h.identificar(&voz(230.0, &ana, 8, 1.5));
        assert_ne!(h.personas[j].etiqueta(), "Davis");
        let (_, _) = h.identificar(&voz(160.0, &[650.0, 1100.0], 9, 1.5));
        let (i, _) = h.identificar(&voz(111.0, &davis, 7, 1.5));
        assert_eq!(h.personas[i].etiqueta(), "Davis");
        let j = h.aprender(j, "Hola, me llamo Constanza, traje los materiales");
        assert_eq!(h.personas[j].nombre, "Constanza");
        assert!(h.olvidar("constanza"));
        // solo se guarda quien aceptó
        let g: Vec<Persona> = serde_json::from_str(&std::fs::read_to_string(d.path().join("v.json")).unwrap()).unwrap();
        assert_eq!(g.len(), 1);
    }
}
