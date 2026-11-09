//! Importa páginas HTML con fichas (las bitácoras que arman sus pestañas con JavaScript) a la memoria Markdown.
//!
//!   orion-nucleo importar-html pagina.html [más.html…] [--carpeta bitacora] [--boveda ~/Orion] [--meta clave=valor…]
//!
//! Saca los arreglos de datos de la página (LOG, SECTIONS, TABS…) evaluándolos en Node sin DOM ni red, y deja una
//! nota por pestaña en <bóveda>/<carpeta>/ y una por cada entrada fechada en <bóveda>/<carpeta>/diario/. Si la página
//! no tiene arreglos reconocibles, guarda el texto visible completo.

use crate::memoria::Memoria;
use regex::Regex;
use serde_json::{Map, Value};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Ejecuta solo las declaraciones de datos en un sandbox de Node (sin DOM, sin red).
const NODE: &str = r#"
const vm=require('vm'),fs=require('fs');
const src=fs.readFileSync(0,'utf8');
const out={};
const re=/(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*([\[{])/g; let m;
while((m=re.exec(src))){
  let i=m.index+m[0].length-1, d=0, q=null, esc=false;
  for(let j=i;j<src.length;j++){
    const c=src[j];
    if(q){ if(esc)esc=false; else if(c==='\\')esc=true; else if(c===q)q=null; continue;}
    if(c==='"'||c==="'"||c==='`'){q=c;continue;}
    if(c==='['||c==='{')d++;
    else if(c===']'||c==='}'){d--; if(d===0){
      try{ out[m[1]]=vm.runInNewContext('('+src.slice(i,j+1)+')',{}, {timeout:500}); }catch(e){}
      break;}}
  }
}
process.stdout.write(JSON.stringify(out));
"#;

fn estado(s: &str) -> String {
    match s {
        "exists" => "Existe hoy".into(),
        "design" => "En diseño".into(),
        "concept" => "Concepto".into(),
        "study" => "Investigar".into(),
        o => limpio(&Value::String(o.into())),
    }
}

/// Sin etiquetas HTML ni entidades.
pub fn limpio(v: &Value) -> String {
    let s = match v {
        Value::String(s) => s.clone(),
        o => o.to_string(),
    };
    let s = Regex::new(r"<[^>]+>").unwrap().replace_all(&s, "");
    let mut t = s.replace("&nbsp;", " ").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&amp;", "&");
    t = Regex::new(r"&#(\d+);").unwrap().replace_all(&t, |c: &regex::Captures| char::from_u32(c[1].parse().unwrap_or(32)).map(String::from).unwrap_or_default()).into();
    t.trim().into()
}

pub fn datos_js(html: &str) -> Map<String, Value> {
    let scripts: Vec<String> = Regex::new(r"(?s)<script[^>]*>(.*?)</script>").unwrap().captures_iter(html).map(|c| c[1].to_string()).collect();
    let Ok(mut h) = std::process::Command::new("node").args(["-e", NODE]).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null()).spawn() else {
        return Map::new();
    };
    if let Some(mut e) = h.stdin.take() {
        let _ = e.write_all(scripts.join("\n").as_bytes());
    }
    h.wait_with_output().ok().and_then(|o| serde_json::from_slice(&o.stdout).ok()).unwrap_or_default()
}

fn primero<'a>(f: &'a Value, claves: &[&str]) -> Option<&'a Value> {
    claves.iter().find_map(|k| f.get(*k).filter(|v| !v.is_null() && *v != "" && *v != false))
}

pub fn ficha_md(f: &Value) -> String {
    if !f.is_object() {
        return format!("- {}", limpio(f));
    }
    let mut p = vec![format!("### {}", primero(f, &["title", "titulo", "name"]).map(limpio).unwrap_or_else(|| "Ficha".into()))];
    if let Some(e) = primero(f, &["status", "estado", "st"]) {
        p.push(format!("*Estado:* {}", estado(e.as_str().unwrap_or(""))));
    }
    for k in ["concept", "body", "text", "desc", "cuerpo", "txt", "p"] {
        if let Some(v) = primero(f, &[k]) {
            p.push(limpio(v));
        }
    }
    for k in ["ref", "referencia"] {
        if let Some(v) = primero(f, &[k]) {
            p.push(format!("*Referencia real:* {}", limpio(v)));
        }
    }
    for k in ["fix", "correccion", "warn", "nota"] {
        if let Some(v) = primero(f, &[k]) {
            p.push(format!("> **Corrección:** {}", limpio(v)));
        }
    }
    for k in ["list", "items", "puntos"] {
        if let Some(l) = f[k].as_array() {
            p.extend(l.iter().map(|x| format!("- {}", limpio(x))));
        }
    }
    for k in ["terms", "terminos", "tags"] {
        if let Some(l) = f[k].as_array() {
            p.push(format!("*Para estudiar:* {}", l.iter().map(limpio).collect::<Vec<_>>().join(", ")));
        }
    }
    p.join("\n")
}

pub fn importar(archivo: &Path, mem: &Memoria, carpeta: &str, meta: &[(String, String)]) -> Result<Vec<PathBuf>, String> {
    let texto = String::from_utf8_lossy(&std::fs::read(archivo).map_err(|e| format!("{}: {e}", archivo.display()))?).to_string();
    let mut base: Vec<(String, String)> = meta.to_vec();
    base.push(("fuente".into(), archivo.file_name().unwrap_or_default().to_string_lossy().into()));
    let m = |extra: &[(&str, String)]| -> Vec<(String, String)> {
        let mut v = base.clone();
        v.extend(extra.iter().map(|(k, x)| (k.to_string(), x.clone())));
        v
    };
    let nota = |t: &str, c: &str, carp: &str, mm: Vec<(String, String)>| {
        let r: Vec<(&str, &str)> = mm.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        mem.nota_con(t, c, carp, &r, false).map_err(|e| e.to_string())
    };
    let mut hechas = vec![];
    for (nombre, valor) in datos_js(&texto) {
        let mut valor = valor;
        if let Some(o) = valor.as_object() {
            if o.values().any(|v| v.get("cards").is_some()) {
                valor = Value::Array(o.iter().filter(|(_, v)| v.is_object()).map(|(k, v)| {
                    let mut x = v.clone();
                    x["k"] = Value::String(k.clone());
                    x
                }).collect());
            }
        }
        let Some(lista) = valor.as_array().filter(|l| !l.is_empty()) else { continue };
        // entradas fechadas
        if lista.iter().all(|e| e.is_object() && (e.get("date").is_some() || e.get("fecha").is_some())) {
            for e in lista {
                let fecha = limpio(&Value::String(format!("{} {}", limpio(e.get("date").or(e.get("fecha")).unwrap_or(&Value::Null)).replace("null", ""), e.get("year").map(limpio).unwrap_or_default())));
                let t = limpio(e.get("title").or(e.get("titulo")).unwrap_or(&Value::String("Entrada".into())));
                hechas.push(nota(&format!("{fecha} {t}"), &ficha_md(e), &format!("{carpeta}/diario"), m(&[("fecha", fecha.clone())]))?);
            }
            continue;
        }
        // pestañas con fichas
        for sec in lista.iter().filter(|s| s.is_object()) {
            let Some(fichas) = ["cards", "fichas", "items", "c"].iter().find_map(|k| sec[*k].as_array()) else { continue };
            let t = primero(sec, &["title", "label", "name", "k"]).map(limpio).unwrap_or_else(|| nombre.clone());
            let intro: Vec<String> = ["lead", "intro", "desc", "note"].iter().filter_map(|k| primero(sec, &[k]).map(limpio)).collect();
            let cuerpo = format!("{}{}", if intro.is_empty() { String::new() } else { intro.join("\n\n") + "\n\n" }, fichas.iter().map(ficha_md).collect::<Vec<_>>().join("\n\n"));
            hechas.push(nota(&t, &cuerpo, carpeta, m(&[("pestana", t.clone()), ("fichas", fichas.len().to_string())]))?);
        }
    }
    if hechas.is_empty() {
        // respaldo: todo el texto visible
        let c = Regex::new(r"(?s)<script[^>]*>.*?</script>|<style[^>]*>.*?</style>").unwrap().replace_all(&texto, "");
        let c = Regex::new(r"<(br|/p|/div|/li|/h\d)>").unwrap().replace_all(&c, "\n");
        let c = Regex::new(r"\n\s*\n+").unwrap().replace_all(&limpio(&Value::String(c.into())), "\n\n").to_string();
        hechas.push(nota(&archivo.file_stem().unwrap_or_default().to_string_lossy(), &c, carpeta, m(&[]))?);
    }
    Ok(hechas)
}

pub fn principal(args: &[String]) -> i32 {
    let opcion = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let carpeta = opcion("--carpeta").unwrap_or_else(|| "importado".into());
    let boveda = opcion("--boveda").map(|b| crate::util::expandir(&b));
    let mut meta = vec![];
    let mut archivos = vec![];
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--carpeta" | "--boveda" => i += 1,
            "--meta" => {
                if let Some((k, v)) = args.get(i + 1).and_then(|x| x.split_once('=')) {
                    meta.push((k.to_string(), v.to_string()));
                }
                i += 1;
            }
            a => archivos.push(PathBuf::from(a)),
        }
        i += 1;
    }
    if archivos.is_empty() {
        eprintln!("uso: orion-nucleo importar-html pagina.html [--carpeta bitacora] [--boveda ~/Orion] [--meta clave=valor]");
        return 2;
    }
    let mem = match Memoria::abrir(boveda) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("memoria: {e}");
            return 1;
        }
    };
    let mut codigo = 0;
    for a in &archivos {
        match importar(a, &mem, &carpeta, &meta) {
            Ok(n) => println!("{}: {} notas en {carpeta}", a.display(), n.len()),
            Err(e) => {
                eprintln!("{e}");
                codigo = 1;
            }
        }
    }
    codigo
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn fichas_y_entradas() {
        let d = tempfile::tempdir().unwrap();
        let mem = Memoria::abrir(Some(d.path().to_path_buf())).unwrap();
        let html = d.path().join("pagina.html");
        std::fs::write(&html, r#"<html><body><h1>Proyecto</h1><script>
const TABS = [{title: "Brazo", lead: "Diseño del brazo", cards: [{title: "Pinza", status: "design", body: "Ventosa <b>frontal</b>", terms: ["vacío", "Venturi"]}]}];
const LOG = [{date: "3 nov", year: 2026, title: "Primera prueba", body: "Funcionó &amp; bien"}];
</script></body></html>"#).unwrap();
        let n = importar(&html, &mem, "notas", &[("capa".into(), "base".into())]).unwrap();
        if n.len() == 1 {
            // sin Node en el equipo: queda el texto visible
            assert!(std::fs::read_to_string(&n[0]).unwrap().contains("Proyecto"));
            return;
        }
        assert_eq!(n.len(), 2);
        let pestana = std::fs::read_to_string(d.path().join("notas/brazo.md")).unwrap();
        assert!(pestana.contains("*Estado:* En diseño") && pestana.contains("Ventosa frontal") && pestana.contains("*Para estudiar:* vacío, Venturi") && pestana.contains("capa: base"));
        let entrada = std::fs::read_dir(d.path().join("notas/diario")).unwrap().next().unwrap().unwrap().path();
        assert!(std::fs::read_to_string(entrada).unwrap().contains("Funcionó & bien"));
        assert_eq!(limpio(&Value::String("<i>a</i> &lt;b&gt; &#241;".into())), "a <b> ñ");
    }
}
