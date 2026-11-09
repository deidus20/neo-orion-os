//! Cuadernos de investigación (antes orion_cuaderno): Gemini Notebook (antes NotebookLM) y Open Notebook (local).
//!
//! Gemini Notebook no tiene API pública para cuentas personales y sus términos no permiten manejarlo por código.
//! Neo Orión hace lo que sí corresponde: arma las FUENTES (notas de la memoria, textos, archivos) en
//! ~/Orion/exportar/cuaderno/<fecha>-<tema>/, abre Gemini Notebook en el navegador normal, la persona sube los
//! archivos, y lo que el cuaderno responda (pegado por la persona) queda en la memoria.
//!
//! Open Notebook (lfnovo/open-notebook, MIT): la misma idea en el propio PC, con API REST local (puerto 5055);
//! Neo Orión crea el cuaderno y le carga las fuentes sin que la persona toque nada.

use crate::memoria::Memoria;
use regex::Regex;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const WEB: &str = "https://notebooklm.google.com";
pub const OPEN_NB_API: &str = "http://127.0.0.1:5055";
pub const OPEN_NB_WEB: &str = "http://127.0.0.1:8502";
const MAX_FUENTE: usize = 400_000; // caracteres por archivo
const COMPOSE_OLLAMA: &str = "https://raw.githubusercontent.com/lfnovo/open-notebook/main/examples/docker-compose-ollama.yml";

pub fn slug(s: &str) -> String {
    let s = Regex::new(r"[^\w\s-]").unwrap().replace_all(&s.to_lowercase(), "").to_string();
    let s: String = Regex::new(r"[\s_]+").unwrap().replace_all(&s, "-").trim_matches('-').chars().take(60).collect();
    if s.is_empty() { "cuaderno".into() } else { s }
}

/// Junta todo lo que sirve sobre `tema` en archivos listos para subir a un cuaderno.
pub fn preparar_fuentes(tema: &str, memoria: Option<&Memoria>, textos: &[(String, String)], archivos: &[String], raiz: Option<&Path>, n_notas: usize) -> Value {
    let raiz = raiz.map(Path::to_path_buf).unwrap_or_else(|| crate::util::casa().join("Orion"));
    let base = raiz.join("exportar/cuaderno").join(format!("{}-{}", chrono::Local::now().format("%Y-%m-%d"), slug(tema)));
    if let Err(e) = std::fs::create_dir_all(&base) {
        return json!({"ok": false, "error": e.to_string()});
    }
    let mut hechos: Vec<PathBuf> = vec![];
    let escribir = |nombre: &str, titulo: &str, cuerpo: &str, hechos: &mut Vec<PathBuf>| {
        let f = base.join(format!("{:02}-{}.md", hechos.len() + 1, slug(nombre)));
        let c: String = cuerpo.trim().chars().take(MAX_FUENTE).collect();
        if std::fs::write(&f, format!("# {titulo}\n\n{c}\n")).is_ok() {
            hechos.push(f);
        }
    };
    if let Some(m) = memoria {
        for r in m.buscar(tema, n_notas) {
            let mut ruta = PathBuf::from(r["ruta"].as_str().unwrap_or(""));
            if !ruta.is_absolute() {
                ruta = m.raiz.join(ruta);
            }
            let cuerpo = if ruta.is_file() { std::fs::read_to_string(&ruta).unwrap_or_default() } else { r["fragmento"].as_str().unwrap_or("").to_string() };
            let tallo = ruta.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let titulo = r["titulo"].as_str().filter(|t| !t.is_empty()).map(String::from).unwrap_or_else(|| tallo.clone());
            escribir(if tallo.is_empty() { "nota" } else { &tallo }, &titulo, &cuerpo, &mut hechos);
        }
    }
    for (t, x) in textos {
        escribir(t, t, x, &mut hechos);
    }
    for a in archivos {
        let p = crate::util::expandir(a);
        if p.is_file() {
            let destino = base.join(format!("{:02}-{}", hechos.len() + 1, p.file_name().unwrap().to_string_lossy()));
            if std::fs::copy(&p, &destino).is_ok() {
                hechos.push(destino);
            }
        }
    }
    let guia = format!(
        "# Cómo subir estas fuentes\n\nTema: **{tema}**\n\n1. En Gemini Notebook toca **Crear** (cuaderno nuevo).\n2. **Agregar fuentes → Subir** y elige todos los archivos de esta carpeta (menos este).\n3. Pide lo que quieras: resumen, guía de estudio, resumen en audio, preguntas de prueba.\n4. Copia la respuesta y pégala en el chat de Neo Orión (tecla /) diciendo «guarda lo del cuaderno».\n"
    );
    let _ = std::fs::write(base.join("00-LEEME.md"), guia);
    json!({"ok": !hechos.is_empty(), "carpeta": base, "fuentes": hechos, "aviso": if hechos.is_empty() { "No encontré notas sobre ese tema: agrega textos o archivos." } else { "" }})
}

fn abrir_externo(que: &str) -> Value {
    match std::process::Command::new("xdg-open").arg(que).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn() {
        Ok(_) => json!({"ok": true, "url": que}),
        Err(e) => json!({"ok": false, "error": e.to_string()}),
    }
}

/// Prepara las fuentes y abre Gemini Notebook en el navegador normal (lo usa la persona).
pub fn abrir_gemini_notebook(tema: &str, memoria: Option<&Memoria>, textos: &[(String, String)], archivos: &[String]) -> Value {
    let mut r = preparar_fuentes(tema, memoria, textos, archivos, None, 12);
    let nav = crate::navegador::abrir(WEB, "chrome");
    r["navegador"] = if nav["ok"] == true { nav } else { abrir_externo(WEB) };
    if let Some(c) = r["carpeta"].as_str() {
        let _ = abrir_externo(c); // carpeta a la vista para arrastrar los archivos
    }
    r["modo"] = json!("manejas tú");
    r["pasos"] = json!(["Crea un cuaderno", format!("Sube los archivos de {}", r["carpeta"].as_str().unwrap_or("")), "Pídele lo que necesites", "Pega la respuesta en Neo Orión para guardarla"]);
    r["por_que"] = json!("Gemini Notebook no tiene API para cuentas personales y sus términos no permiten manejarlo por código.");
    r
}

/// Guarda en la memoria lo que devolvió el cuaderno (pegado por la persona).
pub fn recibir(tema: &str, respuesta: &str, memoria: Option<&Memoria>) -> Value {
    if respuesta.trim().is_empty() {
        return json!({"ok": false, "error": "respuesta vacía"});
    }
    let Some(m) = memoria else { return json!({"ok": true, "guardado": null}) };
    let cuerpo = format!("_Desde Gemini Notebook, {}_\n\n{}", chrono::Local::now().format("%Y-%m-%d %H:%M"), respuesta.trim());
    match m.nota_con(&format!("Cuaderno · {tema}"), &cuerpo, "colaboraciones", &[("fuente", "gemini-notebook"), ("tema", tema)], true) {
        Ok(f) => json!({"ok": true, "guardado": f}),
        Err(e) => json!({"ok": false, "error": e.to_string()}),
    }
}

/// Cliente mínimo de la API REST local de Open Notebook.
pub struct OpenNotebook {
    pub base: String,
}

impl OpenNotebook {
    pub fn new(base: &str) -> Self {
        OpenNotebook { base: base.trim_end_matches('/').into() }
    }

    fn pedir(&self, metodo: &str, ruta: &str, datos: Option<Value>) -> Result<Value, String> {
        let r = ureq::request(metodo, &format!("{}{ruta}", self.base)).timeout(Duration::from_secs(30));
        let res = match datos {
            Some(d) => r.send_json(d),
            None => r.call(),
        }
        .map_err(|e| e.to_string())?;
        let t = res.into_string().map_err(|e| e.to_string())?;
        if t.trim().is_empty() { Ok(json!({})) } else { serde_json::from_str(&t).map_err(|e| e.to_string()) }
    }

    pub fn disponible(&self) -> bool {
        self.pedir("GET", "/api/notebooks", None).is_ok()
    }
    pub fn crear(&self, nombre: &str, descripcion: &str) -> Result<Value, String> {
        self.pedir("POST", "/api/notebooks", Some(json!({"name": nombre, "description": descripcion})))
    }
    pub fn agregar_texto(&self, id: &str, titulo: &str, texto: &str) -> Result<Value, String> {
        self.pedir("POST", "/api/sources", Some(json!({"type": "text", "notebook_id": id, "title": titulo, "content": texto, "embed": true})))
    }
}

/// Todo automático: crea el cuaderno en Open Notebook y le carga las fuentes.
pub fn enviar_open_notebook(tema: &str, memoria: Option<&Memoria>, textos: &[(String, String)], c: &OpenNotebook, raiz: Option<&Path>) -> Value {
    if !c.disponible() {
        return json!({"ok": false, "error": "Open Notebook no está corriendo", "instalar": instalar_open_notebook()});
    }
    let r = preparar_fuentes(tema, memoria, textos, &[], raiz, 12);
    let nb = match c.crear(tema, "Creado por Neo Orión") {
        Ok(v) => v,
        Err(e) => return json!({"ok": false, "error": e}),
    };
    let nid = nb["id"].as_str().or(nb["notebook_id"].as_str()).unwrap_or("").to_string();
    let mut cargadas = 0;
    for f in r["fuentes"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        let p = Path::new(f);
        if p.extension().is_some_and(|e| e == "md") && c.agregar_texto(&nid, &p.file_stem().unwrap().to_string_lossy(), &std::fs::read_to_string(p).unwrap_or_default()).is_ok() {
            cargadas += 1;
        }
    }
    json!({"ok": true, "cuaderno": nid, "fuentes": cargadas, "abrir": OPEN_NB_WEB, "carpeta": r["carpeta"]})
}

/// Pasos (Docker + Ollama local). La clave de cifrado se genera en el equipo; no pasa por ningún chat.
pub fn instalar_open_notebook() -> Vec<String> {
    vec![
        "sudo apt-get install -y docker.io docker-compose-v2".into(),
        "mkdir -p ~/.orion/open-notebook && cd ~/.orion/open-notebook".into(),
        format!("curl -fsSL {COMPOSE_OLLAMA} -o docker-compose.yml"),
        "sed -i \"s/change-me-to-a-secret-string/$(openssl rand -hex 32)/\" docker-compose.yml".into(),
        "sudo docker compose up -d".into(),
        format!("# luego abre {OPEN_NB_WEB}"),
    ]
}

/// La herramienta MCP `cuaderno`.
pub fn herramienta(a: &Value) -> Value {
    let accion = a["accion"].as_str().unwrap_or("");
    let tema = a["tema"].as_str().unwrap_or("");
    let texto = a["texto"].as_str().unwrap_or("");
    let archivos: Vec<String> = a["archivos"].as_array().into_iter().flatten().filter_map(Value::as_str).map(String::from).collect();
    let m = Memoria::abrir(None).ok();
    let extra: Vec<(String, String)> = if !texto.is_empty() && accion != "recibir" { vec![("Encargo".into(), texto.into())] } else { vec![] };
    match accion {
        "preparar" => preparar_fuentes(tema, m.as_ref(), &extra, &archivos, None, 12),
        "gemini" => abrir_gemini_notebook(tema, m.as_ref(), &extra, &archivos),
        "recibir" => recibir(tema, texto, m.as_ref()),
        "local" => enviar_open_notebook(tema, m.as_ref(), &extra, &OpenNotebook::new(OPEN_NB_API), None),
        "instalar_local" => json!({"ok": true, "pasos": instalar_open_notebook()}),
        _ => json!({"ok": false, "error": "accion: preparar | gemini | recibir | local | instalar_local"}),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn fuentes_recibir_y_open_notebook() {
        let d = tempfile::tempdir().unwrap();
        let m = Memoria::abrir(Some(d.path().join("Orion"))).unwrap();
        m.nota("Tortillas industriales", "Amasado, laminado y horneado de tortillas.", "temas").unwrap();
        let r = preparar_fuentes("tortillas", Some(&m), &[("Encargo".into(), "resumen del proceso".into())], &[], Some(d.path()), 12);
        assert_eq!(r["ok"], true);
        let f: Vec<&str> = r["fuentes"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
        assert_eq!(f.len(), 2);
        assert!(std::fs::read_to_string(f[0]).unwrap().contains("Amasado"));
        assert!(f[1].ends_with("02-encargo.md"));
        assert!(Path::new(r["carpeta"].as_str().unwrap()).join("00-LEEME.md").exists());
        assert_eq!(slug("¿Qué es la Tortilla?"), "qué-es-la-tortilla");
        assert_eq!(recibir("x", "  ", Some(&m))["ok"], false);
        let g = recibir("tortillas", "Resumen: tres etapas.", Some(&m));
        let t = std::fs::read_to_string(g["guardado"].as_str().unwrap()).unwrap();
        assert!(t.contains("fuente: gemini-notebook") && t.contains("Resumen: tres etapas."));
        // Open Notebook falso en un puerto local
        let srv = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let puerto = srv.local_addr().unwrap().port();
        let hilo = std::thread::spawn(move || {
            let mut vistas = vec![];
            for _ in 0..4 {
                let (mut s, _) = srv.accept().unwrap();
                let mut b = [0u8; 8192];
                let n = s.read(&mut b).unwrap();
                let pedido = String::from_utf8_lossy(&b[..n]).to_string();
                let cuerpo = if pedido.starts_with("POST /api/notebooks") { r#"{"id":"nb1"}"# } else if pedido.starts_with("GET") { "[]" } else { "{}" };
                let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{cuerpo}", cuerpo.len());
                vistas.push(pedido.lines().next().unwrap_or("").to_string());
            }
            vistas
        });
        let r = enviar_open_notebook("tortillas", Some(&m), &[], &OpenNotebook::new(&format!("http://127.0.0.1:{puerto}")), Some(d.path()));
        assert_eq!(r["cuaderno"], "nb1");
        assert_eq!(r["fuentes"], 2); // la nota de tortillas + lo recibido del cuaderno
        let vistas = hilo.join().unwrap();
        assert!(vistas[0].starts_with("GET /api/notebooks") && vistas[1].starts_with("POST /api/notebooks") && vistas[2].starts_with("POST /api/sources"));
        assert_eq!(enviar_open_notebook("x", None, &[], &OpenNotebook::new("http://127.0.0.1:9"), None)["error"], "Open Notebook no está corriendo");
    }
}
