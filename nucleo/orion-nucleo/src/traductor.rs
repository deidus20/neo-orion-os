//! Traductor en vivo (antes orion_traductor): dos personas que no hablan el mismo idioma conversan a través de
//! Neo Orión. Oído (voz → texto) → traducción → voz en el otro idioma + subtítulos en el HUD.
//!
//! Motores de traducción, en orden, todos nativos:
//!   1. translateLocally (Bergamot, el motor sin internet de Firefox, C++; modelos ~20 MB por par)
//!   2. la IA local (Ollama) con un pedido de traducción estricto
//!   3. cualquier función que se le pase (p. ej. una IA en la nube elegida por la persona)

use crate::util::{correr, existe_programa};
use regex::Regex;
use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

pub fn nombre(c: &str) -> &str {
    match c {
        "es" => "español",
        "en" => "inglés",
        "pt" => "portugués",
        "fr" => "francés",
        "de" => "alemán",
        "it" => "italiano",
        "zh" => "chino",
        "ja" => "japonés",
        "ko" => "coreano",
        "ru" => "ruso",
        "ar" => "árabe",
        "hi" => "hindi",
        o => o,
    }
}

pub fn voz_espeak(c: &str) -> &str {
    match c {
        "es" => "es-419",
        "en" => "en-us",
        "pt" => "pt-br",
        "fr" => "fr-fr",
        "zh" => "cmn",
        o => o,
    }
}

/// Detección liviana entre los dos idiomas de la conversación (Whisper ya entrega el idioma cuando se usa).
pub fn detectar_idioma(texto: &str, opciones: (&str, &str)) -> String {
    let t = texto.to_lowercase();
    let pista = |k: &str| match k {
        "es" => r"\b(el|la|los|las|que|de|y|en|un|una|es|por|para|con|no|qué|cómo|dónde|hola|gracias|ñ)\b|[ñáéíóú¿¡]",
        "en" => r"\b(the|is|are|you|what|how|where|and|to|of|in|it|that|hello|thanks|please|i'm|don't)\b",
        "pt" => r"\b(não|você|obrigad[oa]|olá|é|são|com|uma|isso|onde)\b|[ãõç]",
        "fr" => r"\b(le|les|est|vous|je|nous|bonjour|merci|où|c'est|pas)\b|[èêàç]",
        "de" => r"\b(der|die|das|ist|und|ich|nicht|danke|hallo|wo|sie)\b|[äöüß]",
        _ => r"$^",
    };
    let puntos = |k: &str| Regex::new(pista(k)).unwrap().find_iter(&t).count();
    let (a, b) = opciones;
    if puntos(b) > puntos(a) { b.into() } else { a.into() }
}

pub type Motor = fn(&str, &str, &str) -> Option<String>;

/// translateLocally: `-m es-en-tiny` lee de la entrada estándar y escribe la traducción.
pub fn bergamot(texto: &str, de: &str, a: &str) -> Option<String> {
    if !existe_programa("translateLocally") {
        return None;
    }
    let modelo = format!("{de}-{a}-tiny");
    let mut h = Command::new("translateLocally").args(["-m", &modelo]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    h.stdin.take()?.write_all(texto.as_bytes()).ok()?;
    let o = h.wait_with_output().ok()?;
    let t = String::from_utf8_lossy(&o.stdout).trim().to_string();
    (o.status.success() && !t.is_empty()).then_some(t)
}

pub fn ia_local(texto: &str, de: &str, a: &str) -> Option<String> {
    let r = crate::ia::charlar(&format!("Traduce del {} al {}. Responde SOLO la traducción, sin comillas ni explicaciones:\n{texto}", nombre(de), nombre(a)), &[]);
    if r["sin_ia"] == true || r.get("error").is_some_and(|e| !e.is_null()) {
        return None;
    }
    let t = r["texto"].as_str()?.trim().trim_matches(|c| c == '"' || c == '«' || c == '»').to_string();
    (!t.is_empty()).then_some(t)
}

pub struct Conversacion {
    pub idioma_a: String,
    pub idioma_b: String,
    pub motores: Vec<(&'static str, Motor)>,
    /// sink de PipeWire (audífonos óseos, parlante)
    pub salida: String,
    pub decir: bool,
    pub historial: Vec<Value>,
}

impl Conversacion {
    pub fn new(a: &str, b: &str) -> Self {
        Conversacion { idioma_a: a.into(), idioma_b: b.into(), motores: vec![("bergamot", bergamot), ("ia_local", ia_local)], salida: String::new(), decir: true, historial: vec![] }
    }

    pub fn traducir(&self, texto: &str, de: &str, a: &str) -> (String, &'static str) {
        self.motores.iter().find_map(|(n, m)| m(texto, de, a).map(|r| (r, *n))).unwrap_or((String::new(), "ninguno"))
    }

    /// Una frase de cualquiera de los dos: se traduce al idioma del otro y se dice.
    pub fn turno(&mut self, texto: &str, idioma: Option<&str>) -> Value {
        let de = idioma.map(String::from).unwrap_or_else(|| detectar_idioma(texto, (&self.idioma_a, &self.idioma_b)));
        let a = if de == self.idioma_a { self.idioma_b.clone() } else { self.idioma_a.clone() };
        let (tr, motor) = self.traducir(texto, &de, &a);
        let mut ev = json!({"original": texto, "de": de, "a": a, "traduccion": tr, "motor": motor});
        if tr.is_empty() {
            ev["error"] = json!("no hay motor de traducción: instala translateLocally o una IA local");
        } else if self.decir {
            decir(&tr, &a, &self.salida);
        }
        self.historial.push(ev.clone());
        ev
    }
}

pub fn decir(texto: &str, idioma: &str, salida: &str) {
    let mut c = Command::new("espeak-ng");
    c.args(["-v", voz_espeak(idioma), texto]).stdout(Stdio::null()).stderr(Stdio::null());
    if !salida.is_empty() {
        c.env("PULSE_SINK", salida);
    }
    let _ = c.status();
}

/// Salidas de PipeWire/Pulse disponibles (para elegir los audífonos).
pub fn salidas_audio() -> Vec<String> {
    correr(&["pactl", "list", "short", "sinks"]).1.lines().filter_map(|l| l.split('\t').nth(1).map(String::from)).collect()
}

/// Lo que se instala para un par de idiomas sin internet.
pub fn instalar_par(de: &str, a: &str) -> Vec<String> {
    vec!["sudo apt-get install -y translatelocally || flatpak install -y flathub com.translatelocally.translateLocally".into(), format!("translateLocally -d {de}-{a}-tiny"), format!("translateLocally -d {a}-{de}-tiny")]
}

/// accion("traductor", {texto, a, b, decir})
pub fn accion(d: &Value) -> Value {
    let mut c = Conversacion::new(d["a"].as_str().unwrap_or("es"), d["b"].as_str().unwrap_or("en"));
    c.decir = d["decir"].as_bool().unwrap_or(false);
    c.turno(d["texto"].as_str().unwrap_or(""), d["idioma"].as_str())
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn detecta_y_traduce_en_ambos_sentidos() {
        assert_eq!(detectar_idioma("¿Dónde está el baño?", ("es", "en")), "es");
        assert_eq!(detectar_idioma("Where is the bathroom?", ("es", "en")), "en");
        assert_eq!(detectar_idioma("xyz", ("es", "en")), "es");
        assert_eq!(detectar_idioma("Bonjour, merci", ("es", "fr")), "fr");
        let mut c = Conversacion::new("es", "en");
        c.decir = false;
        c.motores = vec![("prueba", |t, de, a| Some(format!("[{de}>{a}] {t}")))];
        let r = c.turno("hola, ¿cómo estás?", None);
        assert_eq!(r["traduccion"], "[es>en] hola, ¿cómo estás?");
        assert_eq!(c.turno("thanks, I'm fine", None)["a"], "es");
        assert_eq!(c.historial.len(), 2);
        c.motores = vec![("nada", |_, _, _| None)];
        assert!(c.turno("hola", None)["error"].as_str().unwrap().contains("translateLocally"));
    }
}
