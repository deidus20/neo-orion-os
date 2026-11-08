//! Vista «Archivo»: los documentos de la persona (memoria, Documentos, Descargas) como un manifiesto.

use crate::util::casa;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn tipo(ext: &str) -> &'static str {
    match ext.to_lowercase().as_str() {
        "pdf" => "PDF",
        "md" | "txt" | "odt" | "docx" | "doc" => "NOTA",
        "xlsx" | "ods" | "csv" => "TABLA",
        "py" | "rs" | "js" | "ino" | "c" | "cpp" | "sh" => "CÓDIGO",
        "dxf" | "dwg" | "step" | "stp" | "stl" | "fcstd" | "svg" => "PLANO",
        "mp4" | "mkv" | "webm" => "VIDEO",
        "png" | "jpg" | "jpeg" | "webp" => "IMAGEN",
        _ => "ARCHIVO",
    }
}

/// Archivos sueltos de las carpetas de la persona, del más nuevo al más viejo.
pub fn sueltos(carpetas: &[PathBuf], tema: &str, n: usize) -> Vec<Value> {
    let tema_l = crate::util::sin_tilde(tema);
    let mut v: Vec<(std::time::SystemTime, PathBuf, u64)> = vec![];
    for c in carpetas {
        for e in std::fs::read_dir(c).into_iter().flatten().flatten() {
            let p = e.path();
            let Ok(m) = e.metadata() else { continue };
            if !m.is_file() || p.file_name().is_some_and(|f| f.to_string_lossy().starts_with('.')) {
                continue;
            }
            if !tema_l.is_empty() && !crate::util::sin_tilde(&p.file_name().unwrap().to_string_lossy()).contains(&tema_l) {
                continue;
            }
            v.push((m.modified().unwrap_or(std::time::UNIX_EPOCH), p, m.len()));
        }
    }
    v.sort_by(|a, b| b.0.cmp(&a.0));
    v.into_iter().take(n).enumerate().map(|(i, (t, p, peso))| {
        let ext = p.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
        let origen = if p.to_string_lossy().contains("/Descargas") || p.to_string_lossy().contains("/Downloads") { "DESCARGAS" } else { "DISCO" };
        json!({"id": format!("D-{:04}", 500 + i * 37), "codigo": format!("NO-{}-{:02}", 2000 + (i * 613) % 7000, (i * 11) % 97),
               "nombre": p.file_stem().unwrap_or_default().to_string_lossy().to_uppercase().chars().take(40).collect::<String>(),
               "tipo": tipo(&ext), "origen": origen, "prioridad": "MEDIA", "estado": "EN DISCO",
               "tema": if tema.is_empty() { "ARCHIVOS".to_string() } else { tema.to_uppercase() },
               "peso": format!("{} KB", peso / 1024 + 1), "fecha": chrono::DateTime::<chrono::Local>::from(t).format("%Y-%m-%d %H:%M").to_string(),
               "ruta": p.to_string_lossy()})
    }).collect()
}

pub fn listar(tema: &str) -> Value {
    let mut docs = crate::memoria::Memoria::abrir(None).map(|m| m.documentos(tema, 30)).unwrap_or_default();
    let h = casa();
    let carpetas = ["Documentos", "Documents", "Descargas", "Downloads", "Orion/archivos"].iter().map(|c| h.join(c)).collect::<Vec<_>>();
    docs.extend(sueltos(&carpetas, tema, 40usize.saturating_sub(docs.len())));
    json!({"ok": true, "docs": docs})
}

/// Abre un documento con su programa (solo dentro de la carpeta de la persona).
pub fn abrir(ruta: &str) -> Value {
    let p = Path::new(ruta);
    let dentro = p.canonicalize().map(|c| c.starts_with(casa())).unwrap_or(false);
    if !dentro {
        return json!({"ok": false, "error": "solo abro documentos de tu carpeta personal"});
    }
    match std::process::Command::new("xdg-open").arg(p).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn() {
        Ok(_) => json!({"ok": true}),
        Err(e) => json!({"ok": false, "error": e.to_string()}),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    #[test]
    fn manifiesto_de_archivos() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("plano-lidar.pdf"), b"x").unwrap();
        std::fs::write(d.path().join("notas.md"), b"y").unwrap();
        std::fs::write(d.path().join(".oculto"), b"z").unwrap();
        let v = sueltos(&[d.path().to_path_buf()], "", 10);
        assert_eq!(v.len(), 2);
        let solo = sueltos(&[d.path().to_path_buf()], "lídar", 10);
        assert_eq!(solo.len(), 1);
        assert_eq!(solo[0]["tipo"], "PDF");
        assert_eq!(abrir("/etc/passwd")["ok"], false);
    }
}
