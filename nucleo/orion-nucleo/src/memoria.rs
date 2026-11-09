//! Memoria de Neo Orión: una carpeta de Markdown (~/Orion) + índice SQLite FTS5, compatible con el índice que
//! ya usa `orion_memoria` (mismas tablas: fts, archivos, enlaces). Cambiar de IA no pierde nada.

use chrono::{Datelike, Local, Timelike};
use regex::Regex;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

const DIAS: [&str; 7] = ["lunes", "martes", "miércoles", "jueves", "viernes", "sábado", "domingo"];
const MESES: [&str; 12] = ["enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre"];
const STOP: &str = " del las los una uno unos unas que con por para como pero sus este esta esto ese esa eso hay muy mas sin sobre entre cuando donde todo toda ";
static LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[\[([^\]|#]+)(?:[#|][^\]]*)?\]\]").unwrap());
static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:^|[^\w/])#([\wáéíóúñ-]+)").unwrap());
static TITULO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^# (.+)$").unwrap());

pub fn slug(s: &str) -> String {
    let t: String = s.trim().to_lowercase().chars().map(|c| match c { 'á' => 'a', 'é' => 'e', 'í' => 'i', 'ó' => 'o', 'ú' | 'ü' => 'u', 'ñ' => 'n', c => c }).collect();
    let r = Regex::new(r"[^a-z0-9]+").unwrap().replace_all(&t, "-").trim_matches('-').chars().take(80).collect::<String>();
    if r.is_empty() { "nota".into() } else { r }
}

/// Separa el frontmatter YAML simple del cuerpo.
pub fn parse(texto: &str) -> (Vec<(String, String)>, String) {
    if !texto.starts_with("---") {
        return (vec![], texto.into());
    }
    let Some(fin) = texto[3..].find("\n---").map(|i| i + 3) else { return (vec![], texto.into()) };
    let meta = texto[3..fin].trim().lines().filter_map(|l| l.split_once(':').map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))).collect();
    (meta, texto[(fin + 4).min(texto.len())..].trim_start_matches('\n').into())
}

pub struct Memoria {
    pub raiz: PathBuf,
    db: Connection,
}

impl Memoria {
    pub fn abrir(raiz: Option<PathBuf>) -> rusqlite::Result<Self> {
        let raiz = raiz.or_else(|| std::env::var_os("ORION_BOVEDA").map(PathBuf::from)).unwrap_or_else(|| crate::util::casa().join("Orion"));
        for d in ["diario", "proyectos", "temas", "personas", "robots", ".orion"] {
            let _ = std::fs::create_dir_all(raiz.join(d));
        }
        let db = Connection::open(raiz.join(".orion/indice.db"))?;
        db.execute_batch(
            "CREATE VIRTUAL TABLE IF NOT EXISTS fts USING fts5(ruta UNINDEXED, titulo, cuerpo, etiquetas, tokenize='unicode61 remove_diacritics 2');
             CREATE TABLE IF NOT EXISTS archivos(ruta TEXT PRIMARY KEY, hash TEXT, vec BLOB);
             CREATE TABLE IF NOT EXISTS enlaces(origen TEXT, destino TEXT);",
        )?;
        Ok(Memoria { raiz, db })
    }

    /// Agrega una línea al diario del día (lo escuchado, lo hecho).
    pub fn diario(&self, texto: &str, quien: &str) -> std::io::Result<PathBuf> {
        let ahora = Local::now();
        let f = self.raiz.join(format!("diario/{}.md", ahora.format("%Y-%m-%d")));
        if !f.exists() {
            let dia = DIAS[ahora.weekday().num_days_from_monday() as usize];
            let cab = format!(
                "---\ntipo: diario\nfecha: {}\netiquetas: [diario]\n---\n# {}{} {} de {} de {}\n\n",
                ahora.format("%Y-%m-%d"), dia[..1].to_uppercase(), &dia[dia.char_indices().nth(1).map(|x| x.0).unwrap_or(1)..],
                ahora.day(), MESES[ahora.month0() as usize], ahora.year()
            );
            std::fs::write(&f, cab)?;
        }
        use std::io::Write;
        let mut fh = std::fs::OpenOptions::new().append(true).open(&f)?;
        writeln!(fh, "- **{:02}:{:02}** _{quien}_: {texto}", ahora.hour(), ahora.minute())?;
        let _ = self.indexar(&f);
        Ok(f)
    }

    pub fn nota(&self, titulo: &str, cuerpo: &str, carpeta: &str) -> std::io::Result<PathBuf> {
        self.nota_con(titulo, cuerpo, carpeta, &[], true)
    }

    /// Como `nota`, con campos extra en el encabezado y la opción de reescribir en vez de anexar.
    pub fn nota_con(&self, titulo: &str, cuerpo: &str, carpeta: &str, meta: &[(&str, &str)], anexar: bool) -> std::io::Result<PathBuf> {
        let f = self.raiz.join(carpeta).join(format!("{}.md", slug(titulo)));
        std::fs::create_dir_all(f.parent().unwrap())?;
        if f.exists() && anexar {
            use std::io::Write;
            let mut fh = std::fs::OpenOptions::new().append(true).open(&f)?;
            write!(fh, "\n## {}\n{cuerpo}\n", Local::now().format("%Y-%m-%d %H:%M"))?;
        } else {
            let extra: String = meta.iter().map(|(k, v)| format!("{k}: {v}\n")).collect();
            std::fs::write(&f, format!("---\ntipo: {}\ncreado: {}\n{extra}---\n# {titulo}\n\n{cuerpo}\n", carpeta.trim_end_matches('s'), Local::now().format("%Y-%m-%d")))?;
        }
        let _ = self.indexar(&f);
        Ok(f)
    }

    pub fn indexar(&self, f: &Path) -> rusqlite::Result<()> {
        let texto = std::fs::read_to_string(f).unwrap_or_default();
        let hash = sha1_smol::Sha1::from(texto.as_bytes()).digest().to_string();
        let rel = f.strip_prefix(&self.raiz).unwrap_or(f).to_string_lossy().to_string();
        let viejo: Option<String> = self.db.query_row("SELECT hash FROM archivos WHERE ruta=?", [&rel], |r| r.get(0)).ok();
        if viejo.as_deref() == Some(hash.as_str()) {
            return Ok(());
        }
        let (meta, cuerpo) = parse(&texto);
        let mut tags: std::collections::BTreeSet<String> = meta.iter().filter(|(k, _)| k == "etiquetas").flat_map(|(_, v)| v.trim_matches(['[', ']']).split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect::<Vec<_>>()).collect();
        tags.extend(TAG.captures_iter(&cuerpo).map(|c| c[1].to_string()));
        let stem = f.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let titulo = TITULO.captures(&cuerpo).map(|c| c[1].to_string()).unwrap_or(stem);
        self.db.execute("DELETE FROM fts WHERE ruta=?", [&rel])?;
        self.db.execute("INSERT INTO fts VALUES (?,?,?,?)", params![rel, titulo, cuerpo, tags.into_iter().collect::<Vec<_>>().join(" ")])?;
        self.db.execute("DELETE FROM enlaces WHERE origen=?", [&rel])?;
        for c in LINK.captures_iter(&cuerpo) {
            self.db.execute("INSERT INTO enlaces VALUES (?,?)", params![rel, slug(&c[1])])?;
        }
        self.db.execute("INSERT OR REPLACE INTO archivos VALUES (?,?,NULL)", params![rel, hash])?;
        Ok(())
    }

    pub fn reindexar(&self) -> usize {
        fn recorrer(d: &Path, out: &mut Vec<PathBuf>) {
            for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() && p.file_name().is_some_and(|n| n != ".orion") {
                    recorrer(&p, out)
                } else if p.extension().is_some_and(|x| x == "md") {
                    out.push(p)
                }
            }
        }
        let mut v = vec![];
        recorrer(&self.raiz, &mut v);
        v.iter().filter(|f| self.indexar(f).is_ok()).count()
    }

    /// Búsqueda por palabras (FTS5 + BM25), como `Memoria.buscar` sin embeddings.
    pub fn buscar(&self, consulta: &str, n: usize) -> Vec<Value> {
        let palabra = Regex::new(r"\w+").unwrap();
        let terms: Vec<String> = palabra.find_iter(&consulta.to_lowercase()).map(|m| m.as_str().to_string()).filter(|t| t.chars().count() > 2 && !STOP.contains(&format!(" {t} "))).collect();
        if terms.is_empty() {
            return vec![];
        }
        let q = terms.iter().map(|t| format!("\"{t}\"*")).collect::<Vec<_>>().join(" OR ");
        let Ok(mut st) = self.db.prepare("SELECT ruta, titulo, snippet(fts, 2, '«', '»', '…', 14), bm25(fts) FROM fts WHERE fts MATCH ? ORDER BY bm25(fts) LIMIT ?") else { return vec![] };
        st.query_map(params![q, (n * 2) as i64], |r| {
            Ok(json!({"ruta": r.get::<_, String>(0)?, "titulo": r.get::<_, String>(1)?, "fragmento": r.get::<_, String>(2)?,
                      "puntaje": crate::util::redondear(-r.get::<_, f64>(3)?, 3), "via": "palabras"}))
        })
        .map(|it| it.flatten().take(n).collect())
        .unwrap_or_default()
    }

    /// Lo que la IA lee antes de responder: notas relevantes, hasta max caracteres.
    pub fn contexto(&self, consulta: &str, max: usize) -> String {
        let mut out = String::new();
        for r in self.buscar(consulta, 6) {
            let ruta = r["ruta"].as_str().unwrap_or("");
            let cuerpo = parse(&std::fs::read_to_string(self.raiz.join(ruta)).unwrap_or_default()).1;
            let trozo = format!("### {ruta}\n{}\n", cuerpo.chars().take(1800).collect::<String>());
            if out.len() + trozo.len() > max {
                break;
            }
            out += &trozo;
        }
        if out.is_empty() { "(sin notas relacionadas)".into() } else { out }
    }

    /// Documentos para la vista «Archivo»: notas e investigaciones guardadas.
    pub fn documentos(&self, tema: &str, n: usize) -> Vec<Value> {
        let filas: Vec<(String, String)> = if tema.trim().is_empty() {
            self.db.prepare("SELECT ruta, titulo FROM fts LIMIT ?").and_then(|mut s| s.query_map([n as i64], |r| Ok((r.get(0)?, r.get(1)?))).map(|i| i.flatten().collect())).unwrap_or_default()
        } else {
            self.buscar(tema, n).iter().map(|r| (r["ruta"].as_str().unwrap_or("").into(), r["titulo"].as_str().unwrap_or("").into())).collect()
        };
        filas.into_iter().enumerate().map(|(i, (ruta, titulo))| {
            let carpeta = ruta.split('/').next().unwrap_or("");
            let meta = std::fs::metadata(self.raiz.join(&ruta)).ok();
            let fecha = meta.as_ref().and_then(|m| m.modified().ok()).map(|t| chrono::DateTime::<Local>::from(t).format("%Y-%m-%d %H:%M").to_string()).unwrap_or_default();
            json!({"id": format!("D-{:04}", 100 + i * 37), "codigo": format!("NO-{}-{:02}", 1000 + (i * 391) % 9000, (i * 7) % 97),
                   "nombre": titulo.to_uppercase(), "tipo": if carpeta == "diario" { "DIARIO" } else { "NOTA" },
                   "origen": "MEMORIA", "prioridad": "MEDIA", "estado": "LEÍDO", "tema": if tema.is_empty() { "MEMORIA".to_string() } else { tema.to_uppercase() },
                   "peso": format!("{} KB", meta.map(|m| m.len() / 1024 + 1).unwrap_or(1)), "fecha": fecha,
                   "ruta": self.raiz.join(&ruta).to_string_lossy()})
        }).collect()
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn escribe_indexa_y_encuentra() {
        let d = tempfile::tempdir().unwrap();
        let m = Memoria::abrir(Some(d.path().to_path_buf())).unwrap();
        m.nota("Brazo robótico", "Pinza por ventosa frontal y base sobre riel. Ver [[servo hombro]] #robotica", "proyectos").unwrap();
        m.diario("probé el lidar en el pasillo", "voz").unwrap();
        let r = m.buscar("brazo ventosa", 5);
        assert_eq!(r[0]["ruta"], "proyectos/brazo-robotico.md");
        assert!(r[0]["fragmento"].as_str().unwrap().contains('«'));
        assert!(m.contexto("lidar", 2000).contains("pasillo"));
        assert_eq!(m.reindexar(), 2);
        let docs = m.documentos("", 10);
        assert_eq!(docs.len(), 2);
        assert_eq!(slug("Árbol Ñandú!"), "arbol-nandu");
    }

    #[test]
    fn frontmatter() {
        let (meta, cuerpo) = parse("---\ntipo: nota\netiquetas: [a, b]\n---\n# Hola\ntexto");
        assert_eq!(meta[0], ("tipo".into(), "nota".into()));
        assert!(cuerpo.starts_with("# Hola"));
    }
}
