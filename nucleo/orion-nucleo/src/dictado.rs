//! Dictado de documentos (antes orion_dictado): hablar y que salga un Word con formato (informe DUOC, memoria
//! explicativa, carta, acta). Se arma con docx-rs, sin Python ni LibreOffice.
//!
//! Cómo se dicta (frases de control, en cualquier parte del dictado):
//!   «título …»                 → título del documento
//!   «sección …» / «capítulo …» → título de sección (se numera solo)
//!   «subtítulo …»              → subsección
//!   «punto …» / «viñeta …»     → elemento de lista
//!   «punto aparte»             → párrafo nuevo
//!   «tabla: a, b, c»           → fila de tabla (la primera es la cabecera)
//! Si hay una IA local, primero ordena el dictado (quita muletillas, arregla puntuación) sin inventar contenido.

use docx_rs::*;
use regex::Regex;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub fn plantilla(n: &str) -> &'static [&'static str] {
    match n {
        "informe-duoc" => &["Introducción", "Desarrollo", "Conclusiones", "Bibliografía"],
        "memoria-electrica" => &["Generalidades", "Descripción de la instalación", "Cálculos justificativos", "Especificaciones técnicas", "Cubicación de materiales", "Planos"],
        "acta" => &["Asistentes", "Temas tratados", "Acuerdos", "Próximos pasos"],
        _ => &[],
    }
}

pub fn logo_duoc() -> PathBuf {
    crate::util::casa().join(".orion/plantillas/duoc-logo.png")
}

pub type Bloque = (String, String);

fn mayuscula(t: &str) -> String {
    let mut c = t.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// Dictado → bloques (tipo, texto). tipo: titulo | seccion | subseccion | parrafo | punto | tabla.
pub fn ordenar(dictado: &str) -> Vec<Bloque> {
    let t = Regex::new(r"(?i)\b(eh+|em+|mmm+|o sea|cachai|ya po|entonces eh)\b[,]?\s*").unwrap().replace_all(dictado, "");
    let t = Regex::new(r"\s+").unwrap().replace_all(&t, " ").trim().to_string();
    let marca = Regex::new(r"(?i)\b(t[ií]tulo|secci[oó]n|cap[ií]tulo|subt[ií]tulo|punto aparte|punto|viñeta|tabla)\b[:,]?").unwrap();
    // como re.split con grupo de Python: texto, marca, texto, marca…
    let mut piezas: Vec<(bool, String)> = vec![];
    let mut desde = 0;
    for m in marca.captures_iter(&t) {
        let todo = m.get(0).unwrap();
        piezas.push((false, t[desde..todo.start()].to_string()));
        piezas.push((true, m[1].to_lowercase()));
        desde = todo.end();
    }
    piezas.push((false, t[desde..].to_string()));
    let mut bloques = vec![];
    let mut actual = "parrafo";
    for (es_marca, p) in piezas {
        if es_marca {
            actual = match p.as_str() {
                "titulo" | "título" => "titulo",
                "seccion" | "sección" | "capitulo" | "capítulo" => "seccion",
                "subtitulo" | "subtítulo" => "subseccion",
                "punto" | "viñeta" => "punto",
                "tabla" => "tabla",
                _ => "parrafo",
            };
            continue;
        }
        let txt = p.trim_matches(|c| " .,:;".contains(c));
        if txt.is_empty() {
            continue;
        }
        let mut txt = mayuscula(txt);
        if (actual == "parrafo" || actual == "punto") && !txt.ends_with(['.', '?', '!']) {
            txt.push('.');
        }
        bloques.push((actual.to_string(), txt));
        if ["titulo", "seccion", "subseccion", "tabla"].contains(&actual) {
            actual = "parrafo";
        }
    }
    bloques
}

/// Secciones vacías de la plantilla, para dictar encima.
pub fn esqueleto(n: &str) -> Vec<Bloque> {
    plantilla(n).iter().map(|s| ("seccion".to_string(), s.to_string())).collect()
}

/// Pasa el dictado por la IA local para ordenar la redacción SIN agregar contenido. Si no hay IA, igual.
pub fn pulir_con_ia(dictado: &str) -> String {
    let r = crate::ia::charlar(&format!("Corrige solo puntuación, mayúsculas y muletillas de este dictado. No agregues ni quites ideas, conserva las palabras «título», «sección», «subtítulo», «punto» y «tabla». Devuelve solo el texto:\n\n{dictado}"), &[]);
    if r["sin_ia"] == true || r.get("error").is_some_and(|e| !e.is_null()) {
        return dictado.into();
    }
    r["texto"].as_str().filter(|t| !t.trim().is_empty()).unwrap_or(dictado).to_string()
}

fn estilos(d: Docx) -> Docx {
    let h = |id: &str, nombre: &str, tam: usize| Style::new(id, StyleType::Paragraph).name(nombre).size(tam).bold().color("1F3864").line_spacing(LineSpacing::new().before(280).after(120));
    d.add_style(h("Title", "Title", 52))
        .add_style(h("Heading1", "Heading 1", 32))
        .add_style(h("Heading2", "Heading 2", 26))
        .add_style(Style::new("ListBullet", StyleType::Paragraph).name("List Bullet").indent(Some(720), Some(SpecialIndentType::Hanging(360)), None, None))
}

fn parrafo(texto: &str) -> Paragraph {
    Paragraph::new().add_run(Run::new().add_text(texto)).line_spacing(LineSpacing::new().after(120).line(276))
}

/// Ancho útil de una hoja A4 con 2,5 cm por lado, en twips.
const ANCHO_UTIL: usize = 11906 - 2 * 1417;

/// Ancho y alto de un PNG (cabecera IHDR), para escalar el logo sin deformarlo.
fn medida_png(b: &[u8]) -> Option<(u32, u32)> {
    (b.len() > 24 && &b[1..4] == b"PNG").then(|| (u32::from_be_bytes(b[16..20].try_into().unwrap()), u32::from_be_bytes(b[20..24].try_into().unwrap())))
}

fn celda(t: &str, negrita: bool) -> TableCell {
    let r = Run::new().add_text(t);
    TableCell::new().add_paragraph(Paragraph::new().add_run(if negrita { r.bold() } else { r }))
}

/// Arma el .docx. datos: asignatura, docente, integrantes, seccion, fecha, destinatario, lugar, firma, titulo.
pub fn documento(bloques: &[Bloque], ruta: &Path, plantilla_: &str, datos: &Value) -> Result<PathBuf, String> {
    let margen = 1417; // 2,5 cm en twips
    let mut d = estilos(Docx::new().page_size(11906, 16838).page_margin(PageMargin::new().top(margen).bottom(margen).left(margen).right(margen)))
        .default_fonts(RunFonts::new().ascii("Arial").hi_ansi("Arial").cs("Arial"))
        .default_size(22);
    let titulo = bloques.iter().find(|b| b.0 == "titulo").map(|b| b.1.clone()).unwrap_or_else(|| datos["titulo"].as_str().unwrap_or("Documento").to_string());
    let hoy = chrono::Local::now().format("%d-%m-%Y").to_string();
    match plantilla_ {
        "informe-duoc" => {
            let logo = std::fs::read(logo_duoc()).ok();
            let cab = match logo.as_ref().and_then(|b| medida_png(b).map(|m| (b, m))) {
                Some((b, (w, h))) => {
                    let ancho = 1_620_000u32; // 4,5 cm en EMU
                    Paragraph::new().add_run(Run::new().add_image(Pic::new(b).size(ancho, (ancho as u64 * h as u64 / w.max(1) as u64) as u32)))
                }
                None => Paragraph::new().add_run(Run::new().add_text("[logo DUOC UC]").color("999999")),
            };
            d = d.add_paragraph(cab.align(AlignmentType::Right));
            for _ in 0..6 {
                d = d.add_paragraph(Paragraph::new());
            }
            d = d.add_paragraph(Paragraph::new().align(AlignmentType::Center).add_run(Run::new().add_text(titulo.to_uppercase()).bold().size(40)));
            for (etiqueta, k) in [("Asignatura", "asignatura"), ("Sección", "seccion"), ("Docente", "docente"), ("Integrantes", "integrantes"), ("Fecha", "fecha")] {
                let v = match &datos[k] {
                    Value::Array(a) => a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", "),
                    Value::String(s) => s.clone(),
                    Value::Null if k == "fecha" => hoy.clone(),
                    Value::Null => String::new(),
                    o => o.to_string(),
                };
                if !v.is_empty() {
                    d = d.add_paragraph(Paragraph::new().align(AlignmentType::Center).add_run(Run::new().add_text(format!("{etiqueta}: ")).bold()).add_run(Run::new().add_text(v)));
                }
            }
            d = d.add_paragraph(Paragraph::new().add_run(Run::new().add_break(BreakType::Page)));
        }
        "carta" => {
            d = d.add_paragraph(parrafo(&format!("{}, {hoy}", datos["lugar"].as_str().unwrap_or("Santiago"))).align(AlignmentType::Right));
            if let Some(dest) = datos["destinatario"].as_str() {
                d = d.add_paragraph(parrafo(dest));
            }
        }
        _ => d = d.add_paragraph(parrafo(&titulo).style("Title")),
    }
    let (mut n_sec, mut n_sub) = (0, 0);
    let mut filas: Vec<TableRow> = vec![];
    let mut columnas = 0usize;
    let cerrar = |d: Docx, filas: &mut Vec<TableRow>, cols: usize| {
        if filas.is_empty() {
            return d;
        }
        let n = cols.max(1);
        d.add_table(Table::new(std::mem::take(filas)).set_grid(vec![ANCHO_UTIL / n; n]).width(ANCHO_UTIL, WidthType::Dxa)).add_paragraph(Paragraph::new())
    };
    for (k, t) in bloques {
        if k == "titulo" {
            continue;
        }
        if k != "tabla" {
            d = cerrar(d, &mut filas, columnas);
        }
        match k.as_str() {
            "seccion" => {
                n_sec += 1;
                n_sub = 0;
                d = d.add_paragraph(parrafo(&format!("{n_sec}. {t}")).style("Heading1"));
            }
            "subseccion" => {
                n_sub += 1;
                d = d.add_paragraph(parrafo(&format!("{n_sec}.{n_sub} {t}")).style("Heading2"));
            }
            "punto" => d = d.add_paragraph(parrafo(&format!("•\t{t}")).style("ListBullet")),
            "tabla" => {
                let celdas: Vec<&str> = t.split([',', ';']).map(str::trim).filter(|c| !c.is_empty()).collect();
                let cab = filas.is_empty();
                if cab {
                    columnas = celdas.len();
                }
                let ancho = ANCHO_UTIL / columnas.max(1);
                filas.push(TableRow::new(celdas.iter().map(|c| celda(c, cab).width(ancho, WidthType::Dxa)).collect()));
            }
            _ => d = d.add_paragraph(parrafo(t)),
        }
    }
    d = cerrar(d, &mut filas, columnas);
    if plantilla_ == "carta" {
        if let Some(f) = datos["firma"].as_str() {
            d = d.add_paragraph(Paragraph::new()).add_paragraph(parrafo(f));
        }
    }
    if let Some(p) = ruta.parent() {
        std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    let f = std::fs::File::create(ruta).map_err(|e| e.to_string())?;
    d.build().pack(f).map_err(|e| e.to_string())?;
    Ok(ruta.to_path_buf())
}

fn nombre_archivo(base: &str) -> String {
    Regex::new(r"[^\w-]+").unwrap().replace_all(base, "_").chars().take(60).collect::<String>() + ".docx"
}

/// Lo que piden el HUD (accion "dictado") y la IA (herramienta dictado).
pub fn dictar(texto: &str, plantilla_: &str, nombre: &str, datos: &Value, pulir: bool) -> Value {
    let t = if pulir { pulir_con_ia(texto) } else { texto.to_string() };
    let b = ordenar(&t);
    let titulo = b.iter().find(|x| x.0 == "titulo").map(|x| x.1.as_str()).unwrap_or(if nombre.is_empty() { "documento" } else { nombre });
    let ruta = crate::util::casa().join("Documentos").join(nombre_archivo(if nombre.is_empty() { titulo } else { nombre }));
    match documento(&b, &ruta, plantilla_, datos) {
        Ok(r) => json!({"ok": true, "archivo": r, "bloques": b}),
        Err(e) => json!({"ok": false, "error": e}),
    }
}

pub fn accion(d: &Value) -> Value {
    let datos = if d["datos"].is_object() { d["datos"].clone() } else { json!({}) };
    dictar(d["texto"].as_str().unwrap_or(""), d["plantilla"].as_str().unwrap_or("informe-duoc"), "", &datos, d["pulir"] == true)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn b(k: &str, t: &str) -> Bloque {
        (k.into(), t.into())
    }

    #[test]
    fn dictado_a_word() {
        let t = "título informe de procesos industriales sección introducción punto aparte eh este trabajo describe el proceso de tortillas \
                 punto aparte o sea se estudian las etapas sección desarrollo punto amasado punto laminado \
                 tabla etapa, tiempo tabla amasado, 10 min";
        let bl = ordenar(t);
        assert_eq!(bl[0], b("titulo", "Informe de procesos industriales"));
        assert!(bl.contains(&b("seccion", "Introducción")) && bl.contains(&b("punto", "Amasado.")));
        assert!(!bl.iter().any(|x| x.1.starts_with("Eh")));
        assert!(bl.contains(&b("parrafo", "Este trabajo describe el proceso de tortillas.")) && bl.contains(&b("tabla", "Etapa, tiempo")));
        let d = tempfile::tempdir().unwrap();
        let r = documento(&bl, &d.path().join("inf.docx"), "informe-duoc", &json!({"asignatura": "EAI 6122", "integrantes": ["Davis", "Uwe"]})).unwrap();
        // el .docx es un zip: se revisa el XML del cuerpo
        let o = std::process::Command::new("unzip").args(["-p", &r.to_string_lossy(), "word/document.xml"]).output().unwrap();
        let xml = String::from_utf8_lossy(&o.stdout);
        for s in ["INFORME DE PROCESOS INDUSTRIALES", "1. Introducción", "2. Desarrollo", "Davis, Uwe", "EAI 6122", "Amasado", "10 min", "w:tbl>"] {
            assert!(xml.contains(s), "falta {s}");
        }
        assert!(xml.find("w:jc w:val=\"right\"").unwrap() < xml.find("INFORME").unwrap()); // logo arriba a la derecha
        assert_eq!(xml.matches("<w:tbl>").count(), 1);
        assert_eq!(esqueleto("acta").len(), 4);
    }
}
