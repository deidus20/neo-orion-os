//! Oído y voz de Neo Orión — siempre escuchando, sin botón (reemplaza a `orion_voz`).
//!
//! micrófono ─► VAD ─► frase ─► voz a texto ─► memoria (diario) ─► si te hablan («Orión, …») ─► IA ─► voz
//!
//! Privacidad: todo en el equipo; el audio no se guarda, solo el texto. «Orión, pausa» deja de registrar.
//! Motores de voz a texto (todos nativos): Vosk (libvosk, se carga al vuelo) y whisper.cpp. Los nombres antiguos
//! «faster-whisper-*» y «moonshine» (librerías Python) se atienden con whisper.cpp y el modelo equivalente.
//! Voz: Piper, eSpeak NG, Kokoro (`koko`, versión Rust) o ElevenLabs.

pub mod activacion;
pub mod hablantes;
pub mod llave;

use crate::memoria::Memoria;
use crate::util::sin_tilde;
use regex::Regex;
use serde_json::{json, Value};
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const RATE: usize = 16000;
pub const FRAME_MS: usize = 30;
pub const FRAME_MUESTRAS: usize = RATE * FRAME_MS / 1000;

pub fn a_muestras(pcm: &[u8]) -> Vec<i16> {
    pcm.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect()
}

// ============================================================ fuentes de audio
/// Micrófono local por ALSA (`arecord`), en frames de 30 ms.
pub fn fuente_arecord(dispositivo: &str) -> impl Iterator<Item = Vec<i16>> {
    let hijo = Command::new("arecord").args(["-q", "-D", dispositivo, "-f", "S16_LE", "-r", "16000", "-c", "1", "-t", "raw"]).stdout(Stdio::piped()).spawn();
    let mut out = hijo.ok().and_then(|mut h| h.stdout.take());
    std::iter::from_fn(move || {
        let o = out.as_mut()?;
        let mut b = vec![0u8; FRAME_MUESTRAS * 2];
        o.read_exact(&mut b).ok()?;
        Some(a_muestras(&b))
    })
}

/// Archivo WAV 16 kHz mono 16 bit (pruebas, o audio que llega del teléfono).
pub fn fuente_wav(ruta: &str) -> Result<impl Iterator<Item = Vec<i16>>, String> {
    let r = hound::WavReader::open(ruta).map_err(|e| e.to_string())?;
    let s = r.spec();
    if s.sample_rate as usize != RATE || s.channels != 1 || s.bits_per_sample != 16 {
        return Err("WAV 16 kHz mono 16 bit".into());
    }
    let m: Vec<i16> = r.into_samples::<i16>().flatten().collect();
    Ok(m.chunks_exact(FRAME_MUESTRAS).map(|c| c.to_vec()).collect::<Vec<_>>().into_iter())
}

// ============================================================ VAD
pub trait Vad {
    fn es_voz(&mut self, frame: &[i16]) -> bool;
}

/// Sin dependencias: umbral adaptativo de energía (sirve hasta en una Pi Zero).
pub struct VadEnergia {
    ruido: f64,
    factor: f64,
}
impl VadEnergia {
    pub fn new(factor: f64) -> Self {
        VadEnergia { ruido: 200.0, factor }
    }
}
impl Vad for VadEnergia {
    fn es_voz(&mut self, f: &[i16]) -> bool {
        let rms = if f.is_empty() { 0.0 } else { (f.iter().map(|&x| (x as f64).powi(2)).sum::<f64>() / f.len() as f64).sqrt() };
        let voz = rms > self.ruido * self.factor;
        if !voz {
            self.ruido = 0.95 * self.ruido + 0.05 * rms.max(50.0);
        }
        voz
    }
}

pub struct VadWebRtc(webrtc_vad::Vad);
impl VadWebRtc {
    pub fn new(agresividad: u8) -> Self {
        let modo = match agresividad {
            0 => webrtc_vad::VadMode::Quality,
            1 => webrtc_vad::VadMode::LowBitrate,
            2 => webrtc_vad::VadMode::Aggressive,
            _ => webrtc_vad::VadMode::VeryAggressive,
        };
        VadWebRtc(webrtc_vad::Vad::new_with_rate_and_mode(webrtc_vad::SampleRate::Rate16kHz, modo))
    }
}
impl Vad for VadWebRtc {
    fn es_voz(&mut self, f: &[i16]) -> bool {
        self.0.is_voice_segment(f).unwrap_or(false)
    }
}

/// WebRTC + energía: los dos deben estar de acuerdo (ventiladores y motores del taller no cuentan como voz).
pub struct VadCombinado(VadWebRtc, VadEnergia);
impl Vad for VadCombinado {
    fn es_voz(&mut self, f: &[i16]) -> bool {
        self.1.es_voz(f) && self.0.es_voz(f)
    }
}

pub fn crear_vad(nombre: &str) -> Box<dyn Vad> {
    match nombre {
        "webrtcvad" => Box::new(VadWebRtc::new(2)),
        "energia" => Box::new(VadEnergia::new(3.0)),
        _ => Box::new(VadCombinado(VadWebRtc::new(3), VadEnergia::new(2.5))),
    }
}

/// Agrupa frames en frases: empieza con 3 frames de voz, termina tras `silencio_ms` de silencio.
pub fn segmentos<'a>(frames: impl Iterator<Item = Vec<i16>> + 'a, mut vad: Box<dyn Vad + 'a>, silencio_ms: usize) -> impl Iterator<Item = Vec<i16>> + 'a {
    let (min, max, fin) = (300 * RATE / 1000, 20 * RATE, silencio_ms / FRAME_MS);
    let (mut buf, mut previos, mut activos, mut silencio): (Vec<i16>, Vec<Vec<i16>>, usize, usize) = (vec![], vec![], 0, 0);
    let mut frames = frames.fuse();
    let mut terminado = false;
    std::iter::from_fn(move || loop {
        if terminado {
            return None;
        }
        let Some(f) = frames.next() else {
            terminado = true;
            let b = std::mem::take(&mut buf);
            return if b.len() >= min { Some(b) } else { None };
        };
        let voz = vad.es_voz(&f);
        if buf.is_empty() {
            previos.push(f);
            if previos.len() > 10 {
                previos.remove(0);
            }
            activos = if voz { activos + 1 } else { 0 };
            if activos >= 3 {
                buf = previos.concat();
                silencio = 0;
            }
            continue;
        }
        buf.extend_from_slice(&f);
        silencio = if voz { 0 } else { silencio + 1 };
        if silencio >= fin || buf.len() >= max {
            let b = std::mem::take(&mut buf);
            previos.clear();
            activos = 0;
            if b.len() >= min {
                return Some(b);
            }
        }
    })
}

pub fn wav_bytes(m: &[i16], tasa: u32) -> Vec<u8> {
    let mut c = std::io::Cursor::new(Vec::new());
    {
        let mut w = hound::WavWriter::new(&mut c, hound::WavSpec { channels: 1, sample_rate: tasa, bits_per_sample: 16, sample_format: hound::SampleFormat::Int }).unwrap();
        for &x in m {
            let _ = w.write_sample(x);
        }
        let _ = w.finalize();
    }
    c.into_inner()
}

// ============================================================ voz a texto
pub trait Stt {
    fn nombre(&self) -> String;
    fn transcribir(&mut self, pcm: &[i16]) -> String;
}

/// whisper.cpp (`whisper-cli`): el motor por defecto y el reemplazo de faster-whisper y moonshine.
pub struct SttWhisperCpp {
    pub modelo: String,
    pub binario: String,
}
impl Stt for SttWhisperCpp {
    fn nombre(&self) -> String {
        "whisper.cpp".into()
    }
    fn transcribir(&mut self, pcm: &[i16]) -> String {
        let tmp = std::env::temp_dir().join(format!("orion-{}.wav", std::process::id()));
        if std::fs::write(&tmp, wav_bytes(pcm, RATE as u32)).is_err() {
            return String::new();
        }
        let r = Command::new(&self.binario).args(["-m", &self.modelo, "-l", "es", "-nt", "-np", "-f"]).arg(&tmp).output();
        let _ = std::fs::remove_file(&tmp);
        r.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
    }
}

/// Vosk por su librería C (libvosk.so), cargada al vuelo: no hace falta para compilar.
pub struct SttVosk {
    _lib: libloading::Library,
    rec: *mut std::ffi::c_void,
    acepta: unsafe extern "C" fn(*mut std::ffi::c_void, *const i16, i32) -> i32,
    final_: unsafe extern "C" fn(*mut std::ffi::c_void) -> *const std::ffi::c_char,
}
unsafe impl Send for SttVosk {}
impl SttVosk {
    pub fn new(modelo: &str) -> Result<Self, String> {
        unsafe {
            let lib = ["libvosk.so", "/usr/local/lib/libvosk.so", "/opt/orion/lib/libvosk.so"].iter().find_map(|p| libloading::Library::new(p).ok()).ok_or("falta libvosk.so")?;
            let nuevo_modelo: libloading::Symbol<unsafe extern "C" fn(*const std::ffi::c_char) -> *mut std::ffi::c_void> = lib.get(b"vosk_model_new").map_err(|e| e.to_string())?;
            let nuevo_rec: libloading::Symbol<unsafe extern "C" fn(*mut std::ffi::c_void, f32) -> *mut std::ffi::c_void> = lib.get(b"vosk_recognizer_new").map_err(|e| e.to_string())?;
            let ruta = std::ffi::CString::new(modelo).map_err(|e| e.to_string())?;
            let m = nuevo_modelo(ruta.as_ptr());
            if m.is_null() {
                return Err(format!("no se pudo abrir el modelo Vosk {modelo}"));
            }
            let rec = nuevo_rec(m, RATE as f32);
            let acepta = *lib.get(b"vosk_recognizer_accept_waveform_s").map_err(|e| e.to_string())?;
            let final_ = *lib.get(b"vosk_recognizer_final_result").map_err(|e| e.to_string())?;
            Ok(SttVosk { _lib: lib, rec, acepta, final_ })
        }
    }
}
impl Stt for SttVosk {
    fn nombre(&self) -> String {
        "vosk".into()
    }
    fn transcribir(&mut self, pcm: &[i16]) -> String {
        unsafe {
            (self.acepta)(self.rec, pcm.as_ptr(), pcm.len() as i32);
            let p = (self.final_)(self.rec);
            if p.is_null() {
                return String::new();
            }
            let s = std::ffi::CStr::from_ptr(p).to_string_lossy().to_string();
            serde_json::from_str::<Value>(&s).ok().and_then(|v| v["text"].as_str().map(String::from)).unwrap_or_default()
        }
    }
}

pub fn modelos() -> std::path::PathBuf {
    crate::util::casa().join(".orion/modelos")
}

/// Elige el motor según la instalación: vosk | whisper.cpp | faster-whisper-<tamaño> | moonshine.
pub fn crear_stt(eleccion: &str) -> Result<Box<dyn Stt>, String> {
    let whisper = |modelo: &str| -> Result<Box<dyn Stt>, String> {
        let m = modelos().join(format!("ggml-{modelo}.bin"));
        let alt = modelos().join("ggml-base.bin");
        let ruta = if m.exists() { m } else if alt.exists() { alt } else { return Err(format!("falta el modelo whisper {modelo}")) };
        let bin = ["whisper-cli", "whisper-cpp", "main"].iter().find(|b| crate::util::existe_programa(b)).ok_or("falta whisper-cli (whisper.cpp)")?;
        Ok(Box::new(SttWhisperCpp { modelo: ruta.to_string_lossy().into(), binario: bin.to_string() }))
    };
    let e = if eleccion == "auto" { if crate::util::existe_programa("whisper-cli") { "whisper.cpp" } else { "vosk" } } else { eleccion };
    if e.starts_with("vosk") {
        return Ok(Box::new(SttVosk::new(&modelos().join("vosk-small-es").to_string_lossy())?));
    }
    if let Some(t) = e.strip_prefix("faster-whisper") {
        let t = t.trim_start_matches('-');
        return whisper(if t.is_empty() { "small" } else { t });
    }
    if e.starts_with("moonshine") {
        return whisper("tiny");
    }
    if e.starts_with("whisper") {
        return whisper("base");
    }
    Err(format!("STT desconocido: {eleccion}"))
}

// ============================================================ texto a voz
/// Devuelve el audio (WAV, o MP3 en ElevenLabs) y lo reproduce.
pub fn hablar(texto: &str, motor: &str, voz: &str, reproducir: bool) -> Option<Vec<u8>> {
    let motor = match motor {
        "auto" if std::env::var("ELEVENLABS_API_KEY").is_ok() => "elevenlabs",
        "auto" if crate::util::existe_programa("piper") => "piper",
        "auto" => "espeak-ng",
        m => m,
    };
    let con_entrada = |cmd: &mut Command, entrada: &str| -> Option<Vec<u8>> {
        let mut h = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
        use std::io::Write;
        h.stdin.take()?.write_all(entrada.as_bytes()).ok()?;
        Some(h.wait_with_output().ok()?.stdout)
    };
    let audio = match motor {
        "piper" => {
            let modelo = if voz.is_empty() { crate::util::casa().join(".orion/voces/es_MX-claude-high.onnx").to_string_lossy().to_string() } else { voz.into() };
            con_entrada(Command::new("piper").args(["--model", &modelo, "--output_file", "-"]), texto)
        }
        "kokoro" => con_entrada(Command::new("koko").args(["text", "--lan", "es", "--style", if voz.is_empty() { "ef_dora" } else { voz }, "--output", "/dev/stdout"]), texto),
        "elevenlabs" => {
            let vid = if voz.is_empty() { std::env::var("ELEVENLABS_VOICE_ID").unwrap_or_default() } else { voz.into() };
            let clave = std::env::var("ELEVENLABS_API_KEY").ok()?;
            let r = ureq::post(&format!("https://api.elevenlabs.io/v1/text-to-speech/{vid}"))
                .set("xi-api-key", &clave)
                .timeout(Duration::from_secs(30))
                .send_json(json!({"text": texto, "model_id": "eleven_multilingual_v2"}))
                .ok()?;
            let mut b = vec![];
            r.into_reader().read_to_end(&mut b).ok()?;
            Some(b)
        }
        _ => Command::new("espeak-ng").args(["-v", if voz.is_empty() { "es-419" } else { voz }, "-s", "165", "--stdout", texto]).output().ok().map(|o| o.stdout),
    }?;
    if reproducir && !audio.is_empty() {
        let jugador: &[&str] = if motor == "elevenlabs" { &["mpg123", "-q", "-"] } else { &["aplay", "-q"] };
        if crate::util::existe_programa(jugador[0]) {
            let _ = con_entrada_bytes(jugador, &audio);
        }
    }
    Some(audio)
}

fn con_entrada_bytes(cmd: &[&str], datos: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut h = Command::new(cmd[0]).args(&cmd[1..]).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
    h.stdin.take().unwrap().write_all(datos)?;
    h.wait().map(|_| ())
}

// ============================================================ el oído
pub struct Oido<'a> {
    pub memoria: Option<&'a Memoria>,
    pub al_hablarme: Option<Box<dyn FnMut(&str) -> String + 'a>>,
    pub nombre: String,
    pub hablante: String,
    pub ventana_s: f64,
    pub pausado: bool,
    pub solo_dueno_ordena: bool,
    pub dueno: String,
    hasta: Option<Instant>,
}

impl<'a> Oido<'a> {
    pub fn new(nombre: &str, hablante: &str, memoria: Option<&'a Memoria>, al_hablarme: Option<Box<dyn FnMut(&str) -> String + 'a>>) -> Self {
        Oido { memoria, al_hablarme, nombre: nombre.into(), hablante: hablante.into(), ventana_s: 25.0, pausado: false, solo_dueno_ordena: true, dueno: String::new(), hasta: None }
    }

    /// ¿Me hablan a mí? Devuelve (dirigido, orden sin el nombre).
    pub fn me_hablan(&self, texto: &str) -> (bool, String) {
        let t = sin_tilde(texto);
        let n = sin_tilde(&self.nombre).replace("neo ", "");
        let re = Regex::new(&format!(r"^\s*(?:oye\s+|hey\s+|ey\s+)?(?:neo\s+)?{}\b[\s,.:]*(.*)", regex::escape(&n))).unwrap();
        if let Some(c) = re.captures(&t) {
            let g = c.get(1).unwrap();
            // misma cantidad de caracteres → se corta el original, con sus tildes
            if t.chars().count() == texto.chars().count() {
                let inicio = t[..g.start()].chars().count();
                return (true, texto.chars().skip(inicio).collect::<String>().trim().to_string());
            }
            return (true, g.as_str().to_string());
        }
        (self.hasta.is_some_and(|h| Instant::now() < h), texto.to_string())
    }

    pub fn procesar(&mut self, texto: &str) -> Value {
        let texto = texto.trim();
        if texto.is_empty() {
            return json!({});
        }
        let (mut dirigido, orden) = self.me_hablan(texto);
        let mut ev = json!({"texto": texto, "dirigido": dirigido, "quien": self.hablante});
        if dirigido && self.solo_dueno_ordena && !self.dueno.is_empty() && self.hablante != self.dueno {
            dirigido = false;
            ev["dirigido"] = false.into();
            ev["ignorado"] = "orden de una voz que no es la del dueño".into();
        }
        let o = sin_tilde(&orden);
        if dirigido && Regex::new(r"\b(pausa|deja de escuchar|no registres|privado)\b").unwrap().is_match(&o) {
            self.pausado = true;
            ev["accion"] = "pausa".into();
        } else if dirigido && Regex::new(r"\b(sigue|vuelve a escuchar|registra de nuevo|reanuda)\b").unwrap().is_match(&o) {
            self.pausado = false;
            ev["accion"] = "reanuda".into();
        }
        let es_pausa = ev["accion"] == "pausa";
        if self.pausado && !es_pausa {
            ev["registrado"] = false.into();
            return ev;
        }
        if let (Some(m), false) = (self.memoria, es_pausa) {
            let _ = m.diario(&format!("{texto}{}", if dirigido { " #orden" } else { "" }), &self.hablante);
            ev["registrado"] = true.into();
        }
        if dirigido && ev.get("accion").is_none() {
            if let Some(f) = self.al_hablarme.as_mut() {
                self.hasta = Some(Instant::now() + Duration::from_secs_f64(self.ventana_s));
                let r = f(&orden);
                if let (Some(m), false) = (self.memoria, r.is_empty()) {
                    let _ = m.diario(&r, &self.nombre);
                }
                ev["respuesta"] = r.into();
            }
        }
        ev
    }
}

fn config() -> Value {
    crate::ia::instalacion()
}

/// Servicio del oído (systemd --user orion-oido): micrófono → texto → memoria → IA → voz, con palmadas y frases.
pub fn servicio() -> i32 {
    let c = config();
    if !crate::util::existe_programa("arecord") {
        eprintln!("oído: falta arecord (alsa-utils)");
        return 1;
    }
    let mut stt = match crear_stt(c["oido"].as_str().unwrap_or("auto")) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("oído: motor de voz no disponible todavía ({e}); reintento en 5 min");
            std::thread::sleep(Duration::from_secs(300));
            return 1;
        }
    };
    let mem = Memoria::abrir(None).ok();
    let nombre = c["identidad"]["nombre"].as_str().unwrap_or("Neo Orión").to_string();
    let motor = c["voz"].as_str().unwrap_or("auto").to_string();
    let usuario = c["cuenta"]["usuario"].as_str().unwrap_or("").to_string();
    let base = crate::util::casa().join(".orion");
    let _ = std::fs::create_dir_all(&base);
    let mut palmadas = activacion::palmadas_de(&activacion::leer());
    let (eventos, salida) = (base.join("activaciones.jsonl"), base.join("oido.jsonl"));
    let anexar = |ruta: &std::path::Path, v: &Value| {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(ruta) {
            let _ = writeln!(f, "{v}");
        }
    };
    let disparar = |a: &Value, como: &str| {
        let rutina: Vec<String> = a["rutina"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect();
        let sistema: Vec<String> = rutina.iter().filter(|x| ["despertar", "reposo", "luces", "suspender"].contains(&x.as_str())).cloned().collect();
        let hechos = activacion::ejecutar_rutina(&sistema, &usuario);
        for p in &rutina {
            if let Some(d) = p.strip_prefix("di:") {
                hablar(d.trim(), &motor, "", true);
            }
        }
        anexar(&eventos, &json!({"t": crate::util::ahora_s(), "activador": a["id"], "como": como, "rutina": rutina, "hechos": hechos}));
        if rutina.iter().any(|x| x == "saludo") {
            let mut nom = usuario.clone();
            if let Some(p) = nom.get_mut(0..1) {
                p.make_ascii_uppercase();
            }
            hablar(&activacion::saludo(&nom, None, activacion::leer()["saludo"].as_str()), &motor, "", true);
        }
    };
    let corto = nombre.split_whitespace().last().unwrap_or("Orión").to_string();
    let responder: Box<dyn FnMut(&str) -> String> = Box::new(|orden: &str| {
        let r = crate::ia::charlar(orden, &[])["texto"].as_str().unwrap_or("").to_string();
        if !r.is_empty() {
            hablar(&r, &motor, "", true);
        }
        r
    });
    let mut oido = Oido::new(&corto, "voz", mem.as_ref(), Some(responder));
    let frames = fuente_arecord("default").inspect(|fr| {
        if let Some(n) = palmadas.alimentar(fr) {
            if let Some(a) = activacion::elegir(&activacion::leer(), None, Some(n), None) {
                disparar(&a, &format!("{n} palmadas"));
            }
        }
    });
    for seg in segmentos(frames, crear_vad("auto"), 700) {
        let texto = stt.transcribir(&seg);
        let ev = oido.procesar(&texto);
        if let Some(a) = activacion::elegir(&activacion::leer(), ev["texto"].as_str(), None, None) {
            disparar(&a, &format!("«{}»", a["frase"].as_str().unwrap_or("")));
        }
        if ev.get("texto").is_some() {
            let mut e = ev.clone();
            e["t"] = crate::util::ahora_s().into();
            anexar(&salida, &e);
        }
    }
    0
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn tono(segundos: f64, amp: f64) -> Vec<i16> {
        (0..(segundos * RATE as f64) as usize).map(|i| (amp * (i as f64 * 2.0 * std::f64::consts::PI * 220.0 / RATE as f64).sin()) as i16).collect()
    }

    #[test]
    fn segmenta_frases_por_energia() {
        let mut audio = vec![0i16; RATE];
        for _ in 0..3 {
            audio.extend(tono(0.8, 8000.0));
            audio.extend(vec![0i16; RATE]);
        }
        let frames: Vec<Vec<i16>> = audio.chunks_exact(FRAME_MUESTRAS).map(|c| c.to_vec()).collect();
        let segs: Vec<_> = segmentos(frames.into_iter(), Box::new(VadEnergia::new(3.0)), 700).collect();
        assert_eq!(segs.len(), 3);
        assert!(segs.iter().all(|s| s.len() > RATE / 2));
    }

    #[test]
    fn me_hablan_pausa_y_memoria() {
        let d = tempfile::tempdir().unwrap();
        let m = Memoria::abrir(Some(d.path().to_path_buf())).unwrap();
        let mut o = Oido::new("Neo Orión", "Davis", Some(&m), Some(Box::new(|x: &str| format!("Hecho: «{x}»."))));
        assert_eq!(o.me_hablan("Orión, mueve el hombro"), (true, "mueve el hombro".to_string()));
        assert_eq!(o.me_hablan("hoy terminé el tablero").0, false);
        let e = o.procesar("Orión, mueve el hombro a cuarenta y cinco grados");
        assert_eq!(e["respuesta"], "Hecho: «mueve el hombro a cuarenta y cinco grados».");
        assert!(o.procesar("y después abre la pinza")["respuesta"].is_string()); // ventana de conversación
        assert_eq!(o.procesar("Orión, pausa")["accion"], "pausa");
        assert_eq!(o.procesar("esto es una conversación privada")["registrado"], false);
        assert_eq!(o.procesar("Orión, sigue")["accion"], "reanuda");
        let diario = std::fs::read_to_string(std::fs::read_dir(d.path().join("diario")).unwrap().next().unwrap().unwrap().path()).unwrap();
        assert!(diario.contains("mueve el hombro") && !diario.contains("conversación privada"));
        o.dueno = "Davis".into();
        o.hablante = "Persona 2".into();
        assert!(o.procesar("Orión, apaga todo")["ignorado"].is_string());
    }

    #[test]
    fn espeak_genera_audio() {
        if crate::util::existe_programa("espeak-ng") {
            assert!(hablar("Sistema en línea.", "espeak-ng", "", false).unwrap().len() > 1000);
        }
    }
}
