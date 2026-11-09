//! Puente entre IAs (antes orion_puente): Neo Orión le pasa trabajo a otra IA con contexto y trae la respuesta.
//!
//! Ida (Neo Orión → otra IA): «usa Claude para crear el documento con lo que hemos hablado». Se arma un PAQUETE DE
//! CONTEXTO (memoria + conversación reciente + tarea) y se envía por la vía OFICIAL de cada IA, con la cuenta de la
//! persona: claude (`claude -p`, su plan Pro/Max), gemini (`gemini -p`), codex (`codex exec`), opencode
//! (`opencode run`), local (Ollama). O la vía web: abre el chat con el paquete copiado y la PERSONA aprieta enviar
//! (automatizar ese clic va contra los términos de varios servicios y arriesga la cuenta).
//! La respuesta queda en la memoria, en colaboraciones/.
//!
//! Vuelta (otra IA → Neo Orión): el servidor MCP del núcleo se conecta a Claude (escritorio o Claude Code). Ver
//! `config_vuelta()`.

use crate::memoria::Memoria;
use crate::util::{correr, existe_programa};
use regex::Regex;
use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

pub struct Via {
    pub id: &'static str,
    pub nombre: &'static str,
    pub bin: &'static str,
    pub instalar: &'static str,
    pub web: Option<&'static str>,
}

pub const VIAS: [Via; 5] = [
    Via { id: "claude", nombre: "Claude", bin: "claude", instalar: "npm install -g @anthropic-ai/claude-code && claude   # entra con tu cuenta (Pro/Max)", web: Some("https://claude.ai/new") },
    Via { id: "gemini", nombre: "Gemini", bin: "gemini", instalar: "npm install -g @google/gemini-cli && gemini   # entra con tu cuenta de Google", web: Some("https://gemini.google.com/app") },
    Via { id: "codex", nombre: "ChatGPT / Codex", bin: "codex", instalar: "npm install -g @openai/codex && codex   # entra con tu cuenta de ChatGPT", web: Some("https://chatgpt.com/") },
    Via { id: "opencode", nombre: "OpenCode", bin: "opencode", instalar: "curl -fsSL https://opencode.ai/install | bash", web: None },
    Via { id: "local", nombre: "IA local", bin: "ollama", instalar: "orion-fit --install", web: None },
];
const ALIAS: [(&str, &str); 7] = [("anthropic", "claude"), ("claude code", "claude"), ("google", "gemini"), ("chatgpt", "codex"), ("openai", "codex"), ("gpt", "codex"), ("ollama", "local")];

pub fn via(id: &str) -> &'static Via {
    VIAS.iter().find(|v| v.id == id).unwrap_or(&VIAS[0])
}

/// Qué IA se nombra en la tarea (si ninguna, Claude).
pub fn via_de(texto: &str) -> &'static str {
    let t = texto.to_lowercase();
    let nombra = |k: &str| Regex::new(&format!(r"\b{}\b", regex::escape(k))).unwrap().is_match(&t);
    if let Some(v) = VIAS.iter().find(|v| nombra(v.id)) {
        return v.id;
    }
    ALIAS.iter().find(|(k, _)| nombra(k)).map(|x| x.1).unwrap_or("claude")
}

pub fn disponible(id: &str) -> bool {
    existe_programa(via(id).bin)
}

/// Lo que la otra IA necesita para trabajar sin preguntar todo de nuevo.
pub fn paquete(tarea: &str, memoria: Option<&Memoria>, conversacion: &[String], max: usize) -> String {
    let mut p = vec![
        "# Encargo de Neo Orión".to_string(),
        String::new(),
        "Trabajas para la misma persona que usa Neo Orión (su asistente en el PC). Responde en español de Chile, claro y completo.".into(),
        "Si entregas un documento, entrégalo completo en Markdown, listo para guardar.".into(),
        String::new(),
        "## Tarea".into(),
        tarea.trim().into(),
        String::new(),
    ];
    if !conversacion.is_empty() {
        p.push("## Lo que se ha conversado (más reciente al final)".into());
        p.extend(conversacion.iter().skip(conversacion.len().saturating_sub(12)).map(|c| format!("- {c}")));
        p.push(String::new());
    }
    if let Some(m) = memoria {
        p.extend(["## Notas de su memoria relacionadas".into(), m.contexto(tarea, max / 2), String::new()]);
    }
    p.extend(["## Al terminar".into(), "Devuelve solo el resultado. Si te falta un dato que no puedes suponer, dilo en una línea al final.".into()]);
    p.join("\n").chars().take(max).collect()
}

fn guardar(memoria: Option<&Memoria>, id: &str, tarea: &str, respuesta: &str) -> Value {
    let Some(m) = memoria else { return Value::Null };
    let v = via(id);
    let titulo = format!("{} {} — {}", chrono::Local::now().format("%Y-%m-%d"), v.nombre, tarea.chars().take(60).collect::<String>());
    m.nota_con(&titulo, &format!("**Tarea:** {tarea}\n\n**Respuesta de {}:**\n\n{respuesta}", v.nombre), "colaboraciones", &[("ia", id)], false).map(|f| json!(f)).unwrap_or(Value::Null)
}

/// La orden para cada vía (la tarea va como argumento, nunca por una shell).
pub fn orden(id: &str, p: &str, modelo_local: &str) -> Vec<String> {
    let v: Vec<&str> = match id {
        "claude" => vec!["claude", "-p", p, "--output-format", "json", "--max-turns", "6"],
        "gemini" => vec!["gemini", "-p", p],
        "codex" => vec!["codex", "exec", p],
        "opencode" => vec!["opencode", "run", p],
        _ => vec!["ollama", "run", modelo_local, p],
    };
    v.into_iter().map(String::from).collect()
}

pub fn limpiar(id: &str, salida: &str) -> String {
    if id == "claude" {
        if let Ok(v) = serde_json::from_str::<Value>(salida) {
            if let Some(r) = v["result"].as_str() {
                return r.to_string();
            }
        }
    }
    salida.to_string()
}

/// Envía la tarea con contexto. Si la vía no está instalada, dice cómo dejarla lista y ofrece la vía web.
pub fn delegar(tarea: &str, id: Option<&str>, memoria: Option<&Memoria>, conversacion: &[String], modelo_local: &str) -> Value {
    let id = id.filter(|x| !x.is_empty()).unwrap_or_else(|| via_de(tarea));
    let v = via(id);
    let p = paquete(tarea, memoria, conversacion, 9000);
    if !disponible(id) {
        return if id == "local" { json!({"ok": false, "via": id, "falta": v.instalar}) } else { json!({"ok": false, "via": id, "falta": v.instalar, "alternativa_web": v.web, "paquete": p}) };
    }
    let c = orden(id, &p, modelo_local);
    let cmd: Vec<&str> = c.iter().map(String::as_str).collect();
    let (codigo, sal, err) = correr(&cmd);
    if codigo != 0 {
        let e = if err.is_empty() { sal } else { err };
        return json!({"ok": false, "via": id, "error": e.chars().rev().take(600).collect::<Vec<_>>().into_iter().rev().collect::<String>()});
    }
    let resp = limpiar(id, &sal).trim().to_string();
    json!({"ok": true, "via": id, "ia": v.nombre, "respuesta": resp, "guardado_en": guardar(memoria, id, tarea, &resp)})
}

/// Vía manual: copia el paquete al portapapeles y abre el chat web. La persona aprieta enviar.
pub fn abrir_web(tarea: &str, id: &str, memoria: Option<&Memoria>, conversacion: &[String]) -> Value {
    let p = paquete(tarea, memoria, conversacion, 9000);
    let url = via(id).web;
    let mut copiado = false;
    for c in [vec!["wl-copy"], vec!["xclip", "-selection", "clipboard"]] {
        if existe_programa(c[0]) {
            if let Ok(mut h) = Command::new(c[0]).args(&c[1..]).stdin(Stdio::piped()).spawn() {
                if let Some(mut e) = h.stdin.take() {
                    let _ = e.write_all(p.as_bytes());
                }
                copiado = h.wait().map(|s| s.success()).unwrap_or(false);
            }
            break;
        }
    }
    if let (Some(u), true) = (url, existe_programa("xdg-open")) {
        let _ = Command::new("xdg-open").arg(u).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
    }
    json!({"ok": true, "via": id, "url": url, "copiado": copiado, "paquete": p,
           "siguiente": "Pega con Ctrl+V, envía, y cuando responda dime «pega la respuesta» o pégala en el chat de Neo Orión."})
}

/// Guarda lo que la persona trae de vuelta desde la web.
pub fn recibir(id: &str, tarea: &str, respuesta: &str, memoria: Option<&Memoria>) -> Value {
    json!({"ok": true, "guardado_en": guardar(memoria, id, tarea, respuesta)})
}

/// Cómo conectar Neo Orión a Claude para que Claude le pida cosas al PC.
pub fn config_vuelta() -> Value {
    let escritorio = crate::util::casa().join(".config/Claude/claude_desktop_config.json");
    json!({
        "claude_code": "claude mcp add orion-linux -- /usr/local/bin/orion-nucleo mcp",
        "claude_escritorio": {"archivo": escritorio, "agregar": {"mcpServers": {"orion-linux": {"command": "/usr/local/bin/orion-nucleo", "args": ["mcp"]}}}},
        "nota": "Con esto, desde el chat de Claude puedes decir «pídele a mi PC que…» y Neo Orión lo hace. Lo físico (robots, PLC) y lo irreversible siguen pidiendo tu confirmación en el PC.",
    })
}

/// La herramienta MCP `colaborar`.
pub fn colaborar(tarea: &str, ia: &str, modo: &str) -> Value {
    let m = Memoria::abrir(None).ok();
    let id = if ia.is_empty() { via_de(tarea) } else { via(ia).id };
    if modo == "web" {
        return abrir_web(tarea, id, m.as_ref(), &[]);
    }
    delegar(tarea, Some(id), m.as_ref(), &[], "qwen3:8b")
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn elige_via_arma_paquete_y_guarda() {
        assert_eq!(via_de("usa Claude para crear el documento"), "claude");
        assert_eq!(via_de("pásale esto a Gemini"), "gemini");
        assert_eq!(via_de("que lo haga ChatGPT"), "codex");
        assert_eq!(via_de("con ollama sin internet"), "local");
        assert_eq!(via_de("hazlo"), "claude");
        let d = tempfile::tempdir().unwrap();
        let m = Memoria::abrir(Some(d.path().to_path_buf())).unwrap();
        m.nota("Oficina 30 m2", "Cliente pide tablero nuevo y canalización a la vista.", "temas").unwrap();
        let conv: Vec<String> = (0..20).map(|i| format!("frase {i}")).collect();
        let p = paquete("Redacta la propuesta del tablero de la oficina", Some(&m), &conv, 9000);
        assert!(p.contains("## Tarea") && p.contains("tablero nuevo") && p.contains("- frase 19") && !p.contains("- frase 7\n"));
        assert!(paquete(&"x".repeat(20000), None, &[], 9000).chars().count() == 9000);
        assert_eq!(orden("claude", "hola", "")[0..2], ["claude", "-p"]);
        assert_eq!(limpiar("claude", r#"{"result": "listo"}"#), "listo");
        let r = recibir("gemini", "resumen del proyecto", "Aquí va el resumen.", Some(&m));
        let f = std::fs::read_to_string(r["guardado_en"].as_str().unwrap()).unwrap();
        assert!(f.contains("ia: gemini") && f.contains("**Respuesta de Gemini:**") && f.contains("Aquí va el resumen."));
        assert!(config_vuelta()["claude_code"].as_str().unwrap().ends_with("orion-nucleo mcp"));
    }
}
