//! Gestión de proyectos para ingeniería y empresas (antes orion_proyectos).
//!
//! Un proyecto es un JSON en la bóveda (proyectos/<id>.json + nota .md), con el mismo formato que antes:
//!  · EDT/WBS jerárquica (1, 1.1, 1.1.1…)
//!  · Programación CPM: inicio/fin tempranos y tardíos, holgura, RUTA CRÍTICA
//!  · Carta Gantt en días calendario (SVG con la estética del sistema + Excel)
//!  · Matriz RACI (Responsable, Aprobador, Consultado, Informado)
//!  · Presupuesto chileno: costo directo → gastos generales → utilidad → contingencia → neto → IVA 19 % → total
//!  · Avance real vs. planificado y tareas atrasadas

use rust_xlsxwriter::{Color, Format, Workbook};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

pub const IVA_CL: f64 = 0.19;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Tarea {
    /// "1.2.3" (EDT)
    pub id: String,
    pub nombre: String,
    /// 0 = hito o paquete resumen
    pub dias: f64,
    pub depende: Vec<String>,
    pub responsable: String,
    /// {"Davis": "R", "Cliente": "A", ...}
    pub raci: BTreeMap<String, String>,
    /// costo directo CLP (materiales + mano de obra)
    pub costo: f64,
    /// 0..1
    pub avance: f64,
    // calculados
    pub ei: f64,
    pub ef: f64,
    pub it: f64,
    pub ft: f64,
    pub holgura: f64,
    pub critica: bool,
}

impl Tarea {
    pub fn new(id: &str, nombre: &str, dias: f64, depende: &[&str], responsable: &str, raci: &[(&str, &str)], costo: f64, avance: f64) -> Self {
        Tarea {
            id: id.into(),
            nombre: nombre.into(),
            dias,
            depende: depende.iter().map(|s| s.to_string()).collect(),
            responsable: responsable.into(),
            raci: raci.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(),
            costo,
            avance,
            ..Default::default()
        }
    }
}

fn por_defecto_gg() -> f64 {
    0.15
}
fn por_defecto_ut() -> f64 {
    0.10
}
fn por_defecto_cont() -> f64 {
    0.05
}
fn clp() -> String {
    "CLP".into()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Proyecto {
    pub id: String,
    pub nombre: String,
    #[serde(default)]
    pub cliente: String,
    /// AAAA-MM-DD
    #[serde(default)]
    pub inicio: String,
    #[serde(default)]
    pub tareas: Vec<Tarea>,
    #[serde(default = "por_defecto_gg")]
    pub gg: f64,
    #[serde(default = "por_defecto_ut")]
    pub utilidad: f64,
    #[serde(default = "por_defecto_cont")]
    pub contingencia: f64,
    #[serde(default = "clp")]
    pub moneda: String,
}

/// Como el formato :g de Python para cantidades de días (17, 2.5).
pub fn g(x: f64) -> String {
    if x.fract() == 0.0 { format!("{}", x as i64) } else { format!("{}", (x * 1e6).round() / 1e6) }
}

/// 3810975 → "3,810,975" (como {:,.0f} de Python)
pub fn miles(x: f64) -> String {
    let s = format!("{}", x.round_ties_even() as i64);
    let (signo, d) = s.split_at(if s.starts_with('-') { 1 } else { 0 });
    let mut out = String::new();
    for (i, c) in d.chars().enumerate() {
        if i > 0 && (d.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    format!("{signo}{out}")
}

fn pct(x: f64) -> String {
    format!("{}%", (x * 100.0).round_ties_even() as i64)
}

impl Proyecto {
    pub fn new(id: &str, nombre: &str, cliente: &str, inicio: &str, tareas: Vec<Tarea>) -> Self {
        Proyecto { id: id.into(), nombre: nombre.into(), cliente: cliente.into(), inicio: inicio.into(), tareas, gg: 0.15, utilidad: 0.10, contingencia: 0.05, moneda: "CLP".into() }
    }

    fn es_resumen(&self, t: &Tarea) -> bool {
        let pref = format!("{}.", t.id);
        self.tareas.iter().any(|o| o.id.starts_with(&pref))
    }

    fn hojas(&self) -> Vec<usize> {
        (0..self.tareas.len()).filter(|i| !self.es_resumen(&self.tareas[*i])).collect()
    }

    // ---------------------------------------------------- CPM
    pub fn programar(&mut self) -> Result<Value, String> {
        let hojas = self.hojas();
        let por_id: HashMap<String, usize> = hojas.iter().map(|i| (self.tareas[*i].id.clone(), *i)).collect();
        let mut orden: Vec<usize> = vec![];
        let mut visto: BTreeSet<usize> = BTreeSet::new();
        fn dfs(p: &Proyecto, t: usize, pila: &mut Vec<usize>, visto: &mut BTreeSet<usize>, orden: &mut Vec<usize>, por_id: &HashMap<String, usize>) -> Result<(), String> {
            if pila.contains(&t) {
                return Err(format!("dependencia circular en {}", p.tareas[t].id));
            }
            if visto.contains(&t) {
                return Ok(());
            }
            pila.push(t);
            for d in &p.tareas[t].depende {
                let Some(&j) = por_id.get(d) else {
                    return Err(format!("{} depende de {d}, que no existe (o es un paquete resumen)", p.tareas[t].id));
                };
                dfs(p, j, pila, visto, orden, por_id)?;
            }
            pila.pop();
            visto.insert(t);
            orden.push(t);
            Ok(())
        }
        for &h in &hojas {
            dfs(self, h, &mut vec![], &mut visto, &mut orden, &por_id)?;
        }
        for &i in &orden {
            let ei = self.tareas[i].depende.iter().map(|d| self.tareas[por_id[d]].ef).fold(0.0, f64::max);
            let t = &mut self.tareas[i];
            t.ei = ei;
            t.ef = ei + t.dias;
        }
        let fin = orden.iter().map(|i| self.tareas[*i].ef).fold(0.0, f64::max);
        for &i in orden.iter().rev() {
            let id = self.tareas[i].id.clone();
            let ft = orden.iter().filter(|s| self.tareas[**s].depende.contains(&id)).map(|s| self.tareas[*s].it).fold(f64::INFINITY, f64::min);
            let t = &mut self.tareas[i];
            t.ft = if ft.is_finite() { ft } else { fin };
            t.it = t.ft - t.dias;
            t.holgura = ((t.it - t.ei) * 100.0).round() / 100.0;
            t.critica = t.holgura.abs() < 1e-9;
        }
        for p in 0..self.tareas.len() {
            // paquetes resumen = envolvente de sus hijas
            let pref = format!("{}.", self.tareas[p].id);
            let hijas: Vec<&Tarea> = hojas.iter().map(|i| &self.tareas[*i]).filter(|h| h.id.starts_with(&pref)).collect();
            if hijas.is_empty() {
                continue;
            }
            let ei = hijas.iter().map(|h| h.ei).fold(f64::INFINITY, f64::min);
            let ef = hijas.iter().map(|h| h.ef).fold(f64::NEG_INFINITY, f64::max);
            let costo: f64 = hijas.iter().map(|h| h.costo).sum();
            let tot = hijas.iter().map(|h| h.dias).sum::<f64>();
            let avance = hijas.iter().map(|h| h.avance * h.dias).sum::<f64>() / if tot == 0.0 { 1.0 } else { tot };
            let critica = hijas.iter().any(|h| h.critica);
            let t = &mut self.tareas[p];
            (t.ei, t.ef, t.dias, t.costo, t.avance, t.critica) = (ei, ef, ef - ei, costo, avance, critica);
        }
        let ruta: Vec<&str> = orden.iter().filter(|i| self.tareas[**i].critica).map(|i| self.tareas[*i].id.as_str()).collect();
        Ok(json!({"duracion_dias": fin, "ruta_critica": ruta, "termino": if self.inicio.is_empty() { Value::Null } else { json!(self.fecha(fin)) }}))
    }

    pub fn fecha(&self, d: f64) -> String {
        chrono::NaiveDate::parse_from_str(&self.inicio, "%Y-%m-%d").map(|f| (f + chrono::Duration::days(d.round_ties_even() as i64)).to_string()).unwrap_or_default()
    }

    // ---------------------------------------------------- presupuesto (Chile)
    pub fn presupuesto(&self) -> Value {
        let cd: f64 = self.hojas().iter().map(|i| self.tareas[*i].costo).sum();
        let (gg, ut) = (cd * self.gg, cd * self.utilidad);
        let cont = (cd + gg + ut) * self.contingencia;
        let neto = cd + gg + ut + cont;
        let iva = neto * IVA_CL;
        let r = |x: f64| x.round_ties_even() as i64;
        json!({"costo_directo": r(cd), "gastos_generales": r(gg), "utilidad": r(ut), "contingencia": r(cont), "neto": r(neto), "iva_19": r(iva), "total": r(neto + iva),
               "supuestos": format!("GG {} y utilidad {} sobre CD; contingencia {} sobre CD+GG+U", pct(self.gg), pct(self.utilidad), pct(self.contingencia))})
    }

    pub fn raci(&self) -> Value {
        let personas: BTreeSet<String> = self.tareas.iter().flat_map(|t| t.raci.keys().cloned().chain((!t.responsable.is_empty()).then(|| t.responsable.clone()))).collect();
        let mut matriz = Map::new();
        let mut alertas = vec![];
        for t in &self.tareas {
            let k = format!("{} {}", t.id, t.nombre);
            let fila: Map<String, Value> = personas.iter().map(|p| (p.clone(), json!(t.raci.get(p).cloned().unwrap_or_else(|| if *p == t.responsable { "R".into() } else { String::new() })))).collect();
            if fila.values().filter(|v| *v == "A").count() > 1 {
                alertas.push(format!("más de un Aprobador en {k}"));
            }
            matriz.insert(k, Value::Object(fila));
        }
        json!({"personas": personas, "matriz": matriz, "alertas": alertas})
    }

    pub fn avance(&mut self, hoy: Option<&str>) -> Result<Value, String> {
        self.programar()?;
        let hojas: Vec<&Tarea> = self.hojas().into_iter().map(|i| &self.tareas[i]).collect();
        let tot = hojas.iter().map(|t| t.dias).sum::<f64>();
        let tot = if tot == 0.0 { 1.0 } else { tot };
        let real = hojas.iter().map(|t| t.avance * t.dias).sum::<f64>() / tot;
        let inicio = chrono::NaiveDate::parse_from_str(&self.inicio, "%Y-%m-%d").ok();
        let dias = inicio.map(|i| {
            let h = hoy.and_then(|h| chrono::NaiveDate::parse_from_str(h, "%Y-%m-%d").ok()).unwrap_or_else(|| chrono::Local::now().date_naive());
            (h - i).num_days() as f64
        });
        let plan = dias.map(|d| hojas.iter().map(|t| (d - t.ei).max(0.0).min(t.dias)).sum::<f64>() / tot);
        let atrasadas: Vec<&str> = match dias {
            Some(d) => hojas.iter().filter(|t| t.ef <= d && t.avance < 1.0).map(|t| t.id.as_str()).collect(),
            None => vec![],
        };
        let r3 = |x: f64| (x * 1000.0).round() / 1000.0;
        let estado = match plan {
            Some(p) if real < p - 0.02 => "atrasado",
            _ => "al día",
        };
        Ok(json!({"real": r3(real), "planificado": plan.map(r3), "estado": estado, "tareas_atrasadas": atrasadas}))
    }

    // ---------------------------------------------------- salidas
    pub fn gantt_svg(&mut self, ancho: f64) -> Result<String, String> {
        let info = self.programar()?;
        let dur = info["duracion_dias"].as_f64().unwrap_or(0.0);
        let d_max = dur.max(1.0);
        let (fila, izq) = (26.0, 300.0);
        let esc = (ancho - izq - 20.0) / d_max;
        let alto = 60.0 + fila * self.tareas.len() as f64;
        let esc_xml = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        let mut s = vec![
            format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {ancho} {alto}" font-family="JetBrains Mono,monospace" font-size="11">"#),
            format!(r##"<rect width="{ancho}" height="{alto}" fill="#101113"/>"##),
            format!(r##"<text x="12" y="22" fill="#cdf564" font-size="13">{}</text>"##, esc_xml(&self.nombre.to_uppercase())),
            format!(r##"<text x="12" y="40" fill="#8b8e94">{} días · ruta crítica en rojo · {}</text>"##, g(dur), esc_xml(&self.cliente)),
        ];
        let paso = ((d_max / 12.0).round_ties_even() as usize).max(1);
        for d in (0..=d_max as usize).step_by(paso) {
            let x = izq + d as f64 * esc;
            s.push(format!(r##"<line x1="{x:.1}" y1="48" x2="{x:.1}" y2="{alto}" stroke="#2a2c30"/><text x="{:.1}" y="58" fill="#55585e" font-size="9">d{d}</text>"##, x + 2.0));
        }
        for (i, t) in self.tareas.iter().enumerate() {
            let y = 64.0 + i as f64 * fila;
            let nivel = t.id.matches('.').count() as f64;
            let resumen = self.es_resumen(t);
            let col = if t.critica && !resumen { "#ff4a2a" } else if resumen { "#e6e8ea" } else { "#cdf564" };
            let nombre: String = t.nombre.chars().take(30).collect();
            s.push(format!(r#"<text x="{}" y="{}" fill="{}">{} {}</text>"#, 12.0 + nivel * 12.0, y + 13.0, if resumen { "#e6e8ea" } else { "#b9bcc1" }, t.id, esc_xml(&nombre)));
            let (x, w) = (izq + t.ei * esc, (t.dias * esc).max(3.0));
            if t.dias == 0.0 {
                s.push(format!(r##"<rect x="{:.1}" y="{}" width="10" height="10" fill="#e6e8ea" transform="rotate(45 {x:.1} {})"/>"##, x - 5.0, y + 3.0, y + 8.0));
            } else if resumen {
                s.push(format!(r#"<rect x="{x:.1}" y="{}" width="{w:.1}" height="5" fill="{col}"/>"#, y + 6.0));
            } else {
                s.push(format!(r#"<rect x="{x:.1}" y="{}" width="{w:.1}" height="14" fill="none" stroke="{col}"/><rect x="{x:.1}" y="{}" width="{:.1}" height="14" fill="{col}" opacity=".85"/>"#, y + 2.0, y + 2.0, w * t.avance));
                if t.holgura > 0.0 {
                    s.push(format!(r##"<line x1="{:.1}" y1="{}" x2="{:.1}" y2="{}" stroke="#55585e" stroke-dasharray="2 3"/>"##, x + w, y + 9.0, x + w + t.holgura * esc, y + 9.0));
                }
            }
        }
        s.push("</svg>".into());
        Ok(s.concat())
    }

    pub fn a_excel(&mut self, ruta: &Path) -> Result<PathBuf, String> {
        let info = self.programar()?;
        let e = |x: rust_xlsxwriter::XlsxError| x.to_string();
        let negrita = Format::new().set_bold();
        let mut wb = Workbook::new();
        let ws = wb.add_worksheet().set_name("Programa").map_err(e)?;
        let cab = ["EDT", "Tarea", "Días", "Depende de", "Responsable", "Inicio temprano", "Fin temprano", "Holgura", "Crítica", "Avance", "Costo directo"];
        ws.write_row_with_format(0, 0, cab, &negrita).map_err(e)?;
        for (r, t) in self.tareas.iter().enumerate() {
            let r = r as u32 + 1;
            ws.write(r, 0, &t.id).map_err(e)?;
            ws.write(r, 1, &t.nombre).map_err(e)?;
            ws.write(r, 2, t.dias).map_err(e)?;
            ws.write(r, 3, t.depende.join(", ")).map_err(e)?;
            ws.write(r, 4, &t.responsable).map_err(e)?;
            if self.inicio.is_empty() {
                ws.write(r, 5, t.ei).map_err(e)?;
                ws.write(r, 6, t.ef).map_err(e)?;
            } else {
                ws.write(r, 5, self.fecha(t.ei)).map_err(e)?;
                ws.write(r, 6, self.fecha(t.ef)).map_err(e)?;
            }
            ws.write(r, 7, t.holgura).map_err(e)?;
            ws.write(r, 8, if t.critica { "sí" } else { "" }).map_err(e)?;
            ws.write(r, 9, t.avance).map_err(e)?;
            ws.write(r, 10, t.costo).map_err(e)?;
        }
        ws.set_column_width(1, 34).map_err(e)?;
        let dias = info["duracion_dias"].as_f64().unwrap_or(0.0) as u16 + 1;
        let gt = wb.add_worksheet().set_name("Carta Gantt").map_err(e)?;
        gt.write_with_format(0, 0, "EDT", &negrita).map_err(e)?;
        gt.write_with_format(0, 1, "Tarea", &negrita).map_err(e)?;
        for d in 0..dias {
            gt.write_with_format(0, 2 + d, format!("d{d}"), &negrita).map_err(e)?;
        }
        let (lima, rojo) = (Format::new().set_background_color(Color::RGB(0xCDF564)), Format::new().set_background_color(Color::RGB(0xFF4A2A)));
        for (r, t) in self.tareas.iter().enumerate() {
            let r = r as u32 + 1;
            gt.write(r, 0, &t.id).map_err(e)?;
            gt.write(r, 1, &t.nombre).map_err(e)?;
            for d in t.ei as u16..t.ef as u16 {
                gt.write_blank(r, 2 + d, if t.critica { &rojo } else { &lima }).map_err(e)?;
            }
        }
        gt.set_column_width(1, 34).map_err(e)?;
        let pres = self.presupuesto();
        let ps = wb.add_worksheet().set_name("Presupuesto").map_err(e)?;
        let claves = ["costo_directo", "gastos_generales", "utilidad", "contingencia", "neto", "iva_19", "total", "supuestos"];
        for (r, k) in claves.iter().enumerate() {
            let etiqueta = {
                let t = k.replace('_', " ");
                let mut c = t.chars();
                c.next().map(|f| f.to_uppercase().chain(c).collect::<String>()).unwrap_or_default()
            };
            let fmt = if r == 0 { negrita.clone() } else { Format::new() };
            ps.write_with_format(r as u32, 0, etiqueta, &fmt).map_err(e)?;
            match &pres[*k] {
                Value::Number(n) => ps.write_with_format(r as u32, 1, n.as_f64().unwrap_or(0.0), &fmt).map_err(e)?,
                v => ps.write_with_format(r as u32, 1, v.as_str().unwrap_or(""), &fmt).map_err(e)?,
            };
        }
        ps.set_column_width(0, 20).map_err(e)?;
        let rc = self.raci();
        let rs = wb.add_worksheet().set_name("RACI").map_err(e)?;
        let personas: Vec<&str> = rc["personas"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        rs.write_with_format(0, 0, "Tarea", &negrita).map_err(e)?;
        for (c, p) in personas.iter().enumerate() {
            rs.write_with_format(0, c as u16 + 1, *p, &negrita).map_err(e)?;
        }
        for (r, t) in self.tareas.iter().enumerate() {
            let k = format!("{} {}", t.id, t.nombre);
            rs.write(r as u32 + 1, 0, &k).map_err(e)?;
            for (c, p) in personas.iter().enumerate() {
                rs.write(r as u32 + 1, c as u16 + 1, rc["matriz"][&k][*p].as_str().unwrap_or("")).map_err(e)?;
            }
        }
        rs.set_column_width(0, 36).map_err(e)?;
        if let Some(d) = ruta.parent() {
            std::fs::create_dir_all(d).map_err(|x| x.to_string())?;
        }
        wb.save(ruta).map_err(e)?;
        Ok(ruta.to_path_buf())
    }

    // ---------------------------------------------------- persistencia
    pub fn guardar(&mut self, boveda: Option<&Path>) -> Result<PathBuf, String> {
        let raiz = boveda.map(Path::to_path_buf).unwrap_or_else(boveda_por_defecto).join("proyectos");
        std::fs::create_dir_all(&raiz).map_err(|e| e.to_string())?;
        let info = self.programar()?;
        let pres = self.presupuesto();
        let f = raiz.join(format!("{}.json", self.id));
        std::fs::write(&f, serde_json::to_string_pretty(&self).unwrap()).map_err(|e| e.to_string())?;
        let ruta: Vec<&str> = info["ruta_critica"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        let tareas: Vec<String> = self.tareas.iter().map(|t| format!("- [{}] {} {} ({} d)", if t.avance >= 1.0 { "x" } else { " " }, t.id, t.nombre, g(t.dias))).collect();
        let md = format!(
            "---\ntipo: proyecto\ncliente: {}\ninicio: {}\netiquetas: [proyecto]\n---\n# {}\n\n- Duración: {} días (término {})\n- Ruta crítica: {}\n- Total con IVA: ${} {}\n\n## Tareas\n{}\n",
            self.cliente,
            self.inicio,
            self.nombre,
            g(info["duracion_dias"].as_f64().unwrap_or(0.0)),
            info["termino"].as_str().unwrap_or("None"),
            ruta.join(" → "),
            miles(pres["total"].as_f64().unwrap_or(0.0)),
            self.moneda,
            tareas.join("\n")
        );
        std::fs::write(raiz.join(format!("{}.md", self.id)), md).map_err(|e| e.to_string())?;
        Ok(f)
    }

    pub fn cargar(ruta: &Path) -> Result<Self, String> {
        let t = std::fs::read_to_string(ruta).map_err(|e| format!("{}: {e}", ruta.display()))?;
        serde_json::from_str(&t).map_err(|e| e.to_string())
    }
}

pub fn boveda_por_defecto() -> PathBuf {
    std::env::var_os("ORION_BOVEDA").map(PathBuf::from).unwrap_or_else(|| crate::util::casa().join("Orion"))
}

fn orden_edt(id: &str) -> Vec<i64> {
    id.split('.').map(|x| x.parse().unwrap_or(0)).collect()
}

fn tarea_de(v: &Value) -> Result<Tarea, String> {
    serde_json::from_value(v.clone()).map_err(|e| format!("tarea inválida: {e}"))
}

// ---------------------------------------------------- herramientas MCP

pub fn crear(a: &Value) -> Value {
    let r = (|| -> Result<Value, String> {
        let tareas = a["tareas"].as_array().into_iter().flatten().map(tarea_de).collect::<Result<Vec<_>, _>>()?;
        let mut p = Proyecto::new(a["id"].as_str().unwrap_or(""), a["nombre"].as_str().unwrap_or(""), a["cliente"].as_str().unwrap_or(""), a["inicio"].as_str().unwrap_or(""), tareas);
        p.gg = a["gg"].as_f64().unwrap_or(0.15);
        p.utilidad = a["utilidad"].as_f64().unwrap_or(0.10);
        p.contingencia = a["contingencia"].as_f64().unwrap_or(0.05);
        let info = p.programar()?;
        p.guardar(None)?;
        Ok(json!({"programa": info, "presupuesto": p.presupuesto()}))
    })();
    r.unwrap_or_else(|e| json!({"ok": false, "error": e}))
}

fn abrir(id: &str) -> Result<Proyecto, String> {
    Proyecto::cargar(&boveda_por_defecto().join("proyectos").join(format!("{id}.json")))
}

pub fn actualizar(a: &Value) -> Value {
    let r = (|| -> Result<Value, String> {
        let mut p = abrir(a["id"].as_str().unwrap_or(""))?;
        for c in a["cambios"].as_array().into_iter().flatten() {
            let id = c["id"].as_str().unwrap_or("");
            if let Some(t) = p.tareas.iter_mut().find(|t| t.id == id) {
                let mut v = serde_json::to_value(&*t).unwrap();
                for (k, x) in c.as_object().into_iter().flatten() {
                    v[k] = x.clone();
                }
                *t = tarea_de(&v)?;
            } else {
                p.tareas.push(tarea_de(c)?);
            }
        }
        p.tareas.sort_by_key(|t| orden_edt(&t.id));
        let info = p.programar()?;
        p.guardar(None)?;
        Ok(json!({"programa": info, "avance": p.avance(None)?, "presupuesto": p.presupuesto()}))
    })();
    r.unwrap_or_else(|e| json!({"ok": false, "error": e}))
}

pub fn exportar(id: &str, trabajo: &Path) -> Value {
    let r = (|| -> Result<Value, String> {
        let mut p = abrir(id)?;
        let out = trabajo.join("proyectos");
        std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        let svg = out.join(format!("{id}_gantt.svg"));
        std::fs::write(&svg, p.gantt_svg(980.0)?).map_err(|e| e.to_string())?;
        let excel = p.a_excel(&out.join(format!("{id}.xlsx")))?;
        Ok(json!({"gantt_svg": svg, "excel": excel, "raci": p.raci(), "avance": p.avance(None)?}))
    })();
    r.unwrap_or_else(|e| json!({"ok": false, "error": e}))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// Ejemplo real de obra: habilitación de oficina 30 m² (mismo caso que la prueba de Python).
    fn oficina() -> Proyecto {
        let t = Tarea::new;
        Proyecto::new("oficina-30m2", "Habilitación oficina 30 m²", "Cliente particular", "2026-11-09", vec![
            t("1", "Preliminares", 0.0, &[], "", &[], 0.0, 0.0),
            t("1.1", "Levantamiento y medición", 1.0, &[], "Davis", &[("Cliente", "I")], 60000.0, 1.0),
            t("1.2", "Permisos y compra de materiales", 3.0, &["1.1"], "Davis", &[("Cliente", "A")], 40000.0, 0.5),
            t("2", "Obra gruesa", 0.0, &[], "", &[], 0.0, 0.0),
            t("2.1", "Demolición y retiro", 2.0, &["1.1"], "Maestro", &[("Davis", "A")], 180000.0, 0.0),
            t("2.2", "Tabiquería", 4.0, &["2.1", "1.2"], "Maestro", &[("Davis", "A")], 520000.0, 0.0),
            t("3", "Instalaciones", 0.0, &[], "", &[], 0.0, 0.0),
            t("3.1", "Canalización eléctrica", 3.0, &["2.2"], "Davis", &[("Cliente", "I")], 380000.0, 0.0),
            t("3.2", "Tablero y protecciones", 1.0, &["3.1"], "Davis", &[], 290000.0, 0.0),
            t("3.3", "Sanitario", 2.0, &["2.2"], "Gasfíter", &[("Davis", "A")], 240000.0, 0.0),
            t("4", "Terminaciones", 0.0, &[], "", &[], 0.0, 0.0),
            t("4.1", "Pintura", 3.0, &["3.2", "3.3"], "Maestro", &[("Davis", "A")], 310000.0, 0.0),
            t("4.2", "Pisos", 2.0, &["4.1"], "Maestro", &[("Davis", "A")], 420000.0, 0.0),
            t("4.3", "Entrega y TE1", 0.0, &["4.2"], "Davis", &[("Cliente", "A")], 0.0, 0.0),
        ])
    }

    #[test]
    fn mismos_numeros_que_python() {
        let mut p = oficina();
        let info = p.programar().unwrap();
        assert_eq!(info["duracion_dias"], 17.0);
        assert_eq!(info["termino"], "2026-11-26");
        assert_eq!(info["ruta_critica"], json!(["1.1", "1.2", "2.2", "3.1", "3.2", "4.1", "4.2", "4.3"]));
        let t = |id: &str| p.tareas.iter().find(|t| t.id == id).unwrap().clone();
        assert_eq!((t("2.1").ei, t("2.1").ef, t("2.1").holgura, t("2.1").critica), (1.0, 3.0, 1.0, false));
        assert_eq!((t("3.3").holgura, t("2").ei, t("2").ef, t("4").ef), (2.0, 1.0, 8.0, 17.0));
        assert!(t("1").critica);
        let pr = p.presupuesto();
        assert_eq!(pr, json!({"costo_directo": 2440000, "gastos_generales": 366000, "utilidad": 244000, "contingencia": 152500, "neto": 3202500, "iva_19": 608475, "total": 3810975, "supuestos": "GG 15% y utilidad 10% sobre CD; contingencia 5% sobre CD+GG+U"}));
        assert_eq!(p.avance(Some("2026-11-12")).unwrap(), json!({"real": 0.119, "planificado": 0.238, "estado": "atrasado", "tareas_atrasadas": ["2.1"]}));
        let r = p.raci();
        assert_eq!(r["alertas"], json!([]));
        assert_eq!(r["personas"], json!(["Cliente", "Davis", "Gasfíter", "Maestro"]));
        assert_eq!(r["matriz"]["1.1 Levantamiento y medición"]["Davis"], "R");
        // ciclo
        let mut c = Proyecto::new("c", "c", "", "", vec![Tarea::new("1", "a", 1.0, &["2"], "", &[], 0.0, 0.0), Tarea::new("2", "b", 1.0, &["1"], "", &[], 0.0, 0.0)]);
        assert!(c.programar().unwrap_err().contains("circular"));
    }

    #[test]
    fn guarda_carga_y_exporta() {
        let d = tempfile::tempdir().unwrap();
        let mut p = oficina();
        let f = p.guardar(Some(d.path())).unwrap();
        let md = std::fs::read_to_string(d.path().join("proyectos/oficina-30m2.md")).unwrap();
        assert!(md.contains("- Duración: 17 días (término 2026-11-26)") && md.contains("Total con IVA: $3,810,975 CLP") && md.contains("- [x] 1.1 Levantamiento y medición (1 d)"));
        let mut q = Proyecto::cargar(&f).unwrap();
        assert_eq!(q.programar().unwrap()["duracion_dias"], 17.0);
        // un JSON guardado por la versión Python (enteros, sin campos calculados) también se lee
        let py = json!({"id": "x", "nombre": "X", "cliente": "", "inicio": "", "tareas": [{"id": "1", "nombre": "a", "dias": 2, "depende": [], "responsable": "", "raci": {}, "costo": 100, "avance": 0}], "gg": 0.15, "utilidad": 0.1, "contingencia": 0.05, "moneda": "CLP"});
        std::fs::write(d.path().join("py.json"), py.to_string()).unwrap();
        assert_eq!(Proyecto::cargar(&d.path().join("py.json")).unwrap().programar().unwrap()["duracion_dias"], 2.0);
        let svg = p.gantt_svg(980.0).unwrap();
        assert!(svg.contains("HABILITACIÓN OFICINA 30 M²") && svg.contains("17 días") && svg.contains("#ff4a2a"));
        let x = p.a_excel(&d.path().join("o.xlsx")).unwrap();
        let o = std::process::Command::new("unzip").args(["-l", &x.to_string_lossy()]).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&o.stdout).matches("xl/worksheets/sheet").count(), 4);
    }
}
