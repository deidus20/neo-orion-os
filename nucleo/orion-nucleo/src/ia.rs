//! Conversación con la IA local (Ollama), con la identidad elegida en el instalador y la memoria como contexto.

use serde_json::{json, Value};
use std::time::Duration;

pub fn ollama() -> String {
    std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://127.0.0.1:11434".into())
}

fn agente(segundos: u64) -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(Duration::from_secs(segundos)).build()
}

pub fn modelos_locales() -> Vec<String> {
    agente(2)
        .get(&format!("{}/api/tags", ollama()))
        .call()
        .ok()
        .and_then(|r| r.into_json::<Value>().ok())
        .and_then(|v| v["models"].as_array().map(|m| m.iter().filter_map(|x| x["name"].as_str().map(String::from)).collect()))
        .unwrap_or_default()
}

pub fn instalacion() -> Value {
    std::fs::read_to_string("/etc/orion/instalacion.json").ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(|| json!({}))
}

/// Elige el modelo: el preferido en la instalación si está descargado; si no, el primero disponible.
pub fn elegir(preferidos: &[String], disponibles: &[String]) -> Option<String> {
    preferidos.iter().find(|m| disponibles.contains(m) || disponibles.contains(&format!("{m}:latest"))).cloned().or_else(|| disponibles.first().cloned())
}

pub fn quitar_pensamiento(t: &str) -> String {
    regex::Regex::new(r"(?s)<think>.*?</think>").unwrap().replace_all(t, "").trim().to_string()
}

pub fn charlar(texto: &str, historial: &[Value]) -> Value {
    let disp = modelos_locales();
    let inst = instalacion();
    let pref: Vec<String> = inst["ia"]["locales"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect();
    let Some(modelo) = elegir(&pref, &disp) else {
        return json!({"sin_ia": true, "texto": "Aún no tengo una IA local descargada. Conéctame a internet y la bajo sola, o di «abre el instalador de IA»."});
    };
    let nombre = inst["identidad"]["nombre"].as_str().unwrap_or("Neo Orión");
    let mut sistema = format!(
        "Eres {nombre}, el asistente de voz de Neo Orión OS, un Linux que se maneja hablando. Responde en español de Chile, corto y claro (2–4 frases), \
         porque se lee en un HUD y se dice en voz alta. Si te piden una acción física o irreversible, explica que necesita la confirmación de la persona."
    );
    if let Ok(m) = crate::memoria::Memoria::abrir(None) {
        let ctx = m.contexto(texto, 1500);
        let _ = m.diario(texto, "chat");
        if !ctx.is_empty() {
            sistema += "\n\nNotas de la memoria que pueden servir:\n";
            sistema += &ctx;
        }
    }
    let mut msgs = vec![json!({"role": "system", "content": sistema})];
    msgs.extend(historial.iter().rev().take(8).rev().cloned());
    msgs.push(json!({"role": "user", "content": texto}));
    let r = agente(120).post(&format!("{}/api/chat", ollama())).send_json(json!({"model": modelo, "messages": msgs, "stream": false, "think": false, "options": {"num_ctx": 4096}}));
    let v = match r {
        Ok(r) => r.into_json::<Value>().map_err(|e| e.to_string()),
        Err(e) => Err(e.kind().to_string()),
    };
    match v {
        Ok(v) => json!({"texto": quitar_pensamiento(v["message"]["content"].as_str().unwrap_or("")), "modelo": modelo}),
        Err(e) => json!({"error": true, "texto": format!("La IA local no respondió ({e}).")}),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    #[test]
    fn elige_modelo_y_limpia() {
        let d = vec!["qwen3:8b".to_string(), "gemma3:4b".to_string()];
        assert_eq!(elegir(&["gemma3:4b".into()], &d).as_deref(), Some("gemma3:4b"));
        assert_eq!(elegir(&["llama".into()], &d).as_deref(), Some("qwen3:8b"));
        assert_eq!(elegir(&[], &[]), None);
        assert_eq!(quitar_pensamiento("<think>hmm\nx</think> Hola."), "Hola.");
    }
}
