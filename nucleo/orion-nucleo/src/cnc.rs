//! Modo taller (antes orion_cnc): la fresadora CNC (GRBL) desde Neo Orión, con el mismo guardián que los robots.
//!
//!  · revisar un G-code ANTES de cortar: recorrido, límites X/Y/Z, tiempo aproximado, herramientas, avances,
//!    profundidad máxima; lo compara con el área útil de la máquina y avisa si se sale
//!  · vista del recorrido (SVG con la estética del HUD)
//!  · hablar con GRBL por USB: estado (?), desbloquear ($X), casa ($H), mover a mano ($J=), cero de pieza (G10 L20),
//!    pausa (!), seguir (~), parada inmediata (Ctrl-X)
//!  · mandar el trabajo línea por línea esperando el «ok» de cada una, con avance en %
//!
//! Seguridad: mover la máquina y cortar son acciones FÍSICAS: el MCP pide confirmación humana (ticket) antes de
//! `iniciar`, `mover`, `casa` o `cero`. Pausa y parada no piden permiso.

use regex::Regex;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::time::{Duration, Instant};

pub const TIEMPO_RAPIDO: f64 = 1500.0; // mm/min si la máquina no informa su G0

#[derive(Clone)]
pub struct Maquina {
    pub nombre: String,
    pub x_mm: f64,
    pub y_mm: f64,
    pub z_mm: f64,
    pub g0_mm_min: f64,
    pub puerto: String,
    pub baudios: u32,
}

impl Default for Maquina {
    fn default() -> Self {
        Maquina { nombre: "CNC 3018".into(), x_mm: 300.0, y_mm: 180.0, z_mm: 45.0, g0_mm_min: TIEMPO_RAPIDO, puerto: "/dev/ttyUSB0".into(), baudios: 115200 }
    }
}

pub struct Revision {
    pub lineas: usize,
    pub movimientos: usize,
    pub minimo: [f64; 3],
    pub maximo: [f64; 3],
    pub corte_mm: f64,
    pub rapido_mm: f64,
    pub minutos: f64,
    pub avances: BTreeSet<i64>,
    pub herramientas: BTreeSet<i64>,
    pub rpm: BTreeSet<i64>,
    /// (x, y, z, corte?)
    pub puntos: Vec<(f64, f64, f64, bool)>,
    pub avisos: Vec<String>,
    pub errores: Vec<String>,
}

fn r2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

impl Revision {
    pub fn resumen(&self) -> Value {
        let ok = self.minimo[0].is_finite();
        let v = |a: [f64; 3]| if ok { json!(a.map(r2)) } else { Value::Null };
        json!({
            "lineas": self.lineas, "movimientos": self.movimientos,
            "desde_mm": v(self.minimo), "hasta_mm": v(self.maximo),
            "tamano_mm": if ok { json!([0, 1, 2].map(|i| r2(self.maximo[i] - self.minimo[i]))) } else { Value::Null },
            "profundidad_max_mm": if ok { json!(r2(self.minimo[2])) } else { Value::Null },
            "corte_m": r2(self.corte_mm / 1000.0), "minutos": (self.minutos * 10.0).round() / 10.0,
            "avances_mm_min": self.avances, "herramientas": self.herramientas, "rpm": self.rpm,
            "avisos": self.avisos, "errores": self.errores, "listo_para_cortar": self.errores.is_empty(),
        })
    }
}

fn sin_comentarios(l: &str) -> String {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\(.*?\)|;.*$").unwrap()).replace_all(l, "").trim().to_string()
}

/// Simula el G-code (G0/G1/G2/G3, G20/G21, G90/G91) sin mover nada.
pub fn revisar(gcode: &str, m: &Maquina) -> Revision {
    let palabra = Regex::new(r"([A-Z])\s*(-?\d*\.?\d+)").unwrap();
    let mut r = Revision { lineas: 0, movimientos: 0, minimo: [f64::INFINITY; 3], maximo: [f64::NEG_INFINITY; 3], corte_mm: 0.0, rapido_mm: 0.0, minutos: 0.0, avances: BTreeSet::new(), herramientas: BTreeSet::new(), rpm: BTreeSet::new(), puntos: vec![], avisos: vec![], errores: vec![] };
    let mut pos = [0.0f64; 3];
    let (mut absoluto, mut escala, mut f, mut modo) = (true, 1.0, 0.0, 0);
    for (n, cruda) in gcode.lines().enumerate() {
        let linea = sin_comentarios(cruda).to_uppercase();
        if linea.is_empty() {
            continue;
        }
        r.lineas += 1;
        let mut pal = std::collections::HashMap::new();
        for c in palabra.captures_iter(&linea) {
            let k = c[1].chars().next().unwrap();
            let v: f64 = c[2].parse().unwrap_or(0.0);
            match k {
                'G' => match v as i64 {
                    g @ 0..=3 if v.fract() == 0.0 => modo = g,
                    20 => escala = 25.4,
                    21 => escala = 1.0,
                    90 => absoluto = true,
                    91 => absoluto = false,
                    _ => {}
                },
                'M' => {}
                _ => {
                    pal.insert(k, v);
                }
            }
        }
        if let Some(v) = pal.get(&'F') {
            f = v * escala;
            r.avances.insert(f.round() as i64);
        }
        if let Some(v) = pal.get(&'S') {
            r.rpm.insert(*v as i64);
        }
        if let Some(v) = pal.get(&'T') {
            r.herramientas.insert(*v as i64);
        }
        if !['X', 'Y', 'Z'].iter().any(|k| pal.contains_key(k)) {
            continue;
        }
        let mut nuevo = pos;
        for (i, k) in ['X', 'Y', 'Z'].iter().enumerate() {
            if let Some(v) = pal.get(k) {
                nuevo[i] = if absoluto { v * escala } else { pos[i] + v * escala };
            }
        }
        let mut d = ((0..3).map(|i| (nuevo[i] - pos[i]).powi(2)).sum::<f64>()).sqrt();
        if (modo == 2 || modo == 3) && (pal.contains_key(&'I') || pal.contains_key(&'J')) {
            let radio = pal.get(&'I').unwrap_or(&0.0).hypot(*pal.get(&'J').unwrap_or(&0.0)) * escala;
            d = d.max(radio * 0.5 * std::f64::consts::PI); // aproximación conservadora del arco
        }
        if modo == 0 {
            r.rapido_mm += d;
            r.minutos += d / m.g0_mm_min;
        } else {
            if f <= 0.0 {
                r.errores.push(format!("línea {}: corte sin avance F definido", n + 1));
                f = 100.0;
            }
            r.corte_mm += d;
            r.minutos += d / f;
        }
        r.movimientos += 1;
        for p in [pos, nuevo] {
            // el recorrido incluye desde dónde parte
            for i in 0..3 {
                r.minimo[i] = r.minimo[i].min(p[i]);
                r.maximo[i] = r.maximo[i].max(p[i]);
            }
        }
        pos = nuevo;
        if r.puntos.len() < 20000 {
            r.puntos.push((pos[0], pos[1], pos[2], modo != 0));
        }
    }
    if r.movimientos > 0 {
        for (i, (eje, lim)) in [("X", m.x_mm), ("Y", m.y_mm), ("Z", m.z_mm)].iter().enumerate() {
            let t = r.maximo[i] - r.minimo[i];
            if t > *lim {
                r.errores.push(format!("el trabajo mide {t:.1} mm en {eje} y la máquina tiene {lim} mm"));
            }
        }
        if r.minimo[2] < -m.z_mm {
            r.errores.push(format!("baja hasta Z {:.1} mm: más que el recorrido de la máquina", r.minimo[2]));
        }
        if r.minimo[2] < -10.0 {
            r.avisos.push(format!("profundidad máxima {:.1} mm: revisa el largo de la fresa", r.minimo[2]));
        }
        if let Some(mx) = r.avances.iter().max().filter(|x| **x > 3000) {
            r.avisos.push(format!("avance de {mx} mm/min: alto para una CNC de escritorio"));
        }
        if r.rpm.is_empty() {
            r.avisos.push("el archivo no enciende el husillo (no hay S/M3): enciéndelo a mano".into());
        }
    } else {
        r.errores.push("el archivo no tiene movimientos".into());
    }
    r
}

/// Recorrido visto desde arriba, en SVG con la estética del HUD: cortes en el color de acento, rápidos en gris
/// punteado, grilla de 10 mm y rótulo monoespaciado.
pub fn vista_svg(rev: &Revision, acento: &str) -> String {
    let (w, h, m) = (600.0, 420.0, 34.0);
    let (x0, y0) = (rev.minimo[0].min(0.0), rev.minimo[1].min(0.0));
    let (x1, y1) = (rev.maximo[0].max(0.0), rev.maximo[1].max(0.0));
    let esc = ((w - 2.0 * m) / (x1 - x0).max(1.0)).min((h - 2.0 * m - 18.0) / (y1 - y0).max(1.0));
    let px = |x: f64| m + (x - x0) * esc;
    let py = |y: f64| h - m - (y - y0) * esc;
    let mut s = format!(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}" shape-rendering="crispEdges"><rect width="100%" height="100%" fill="#0d0e10"/>"##);
    let paso = if (x1 - x0).max(y1 - y0) > 200.0 { 50.0 } else { 10.0 };
    let mut g = (x0 / paso).ceil() * paso;
    while g <= x1 {
        s += &format!(r##"<line x1="{0:.1}" y1="{1:.1}" x2="{0:.1}" y2="{2:.1}" stroke="#1c1e22"/>"##, px(g), py(y0), py(y1));
        g += paso;
    }
    let mut g = (y0 / paso).ceil() * paso;
    while g <= y1 {
        s += &format!(r##"<line x1="{1:.1}" y1="{0:.1}" x2="{2:.1}" y2="{0:.1}" stroke="#1c1e22"/>"##, py(g), px(x0), px(x1));
        g += paso;
    }
    let mut prev = (0.0, 0.0);
    let tramo = |corte: bool, puntos: &mut Vec<(f64, f64)>, s: &mut String| {
        if puntos.len() > 1 {
            let d: Vec<String> = puntos.iter().map(|(x, y)| format!("{:.1},{:.1}", px(*x), py(*y))).collect();
            *s += &if corte {
                format!(r##"<polyline points="{}" fill="none" stroke="{acento}" stroke-width="1.6" shape-rendering="geometricPrecision"/>"##, d.join(" "))
            } else {
                format!(r##"<polyline points="{}" fill="none" stroke="#55585e" stroke-width="0.8" stroke-dasharray="2 3"/>"##, d.join(" "))
            };
        }
        puntos.clear();
    };
    let mut actual: Vec<(f64, f64)> = vec![];
    let mut corte_actual = false;
    for p in &rev.puntos {
        if p.3 != corte_actual || actual.is_empty() {
            tramo(corte_actual, &mut actual, &mut s);
            actual.push(prev);
            corte_actual = p.3;
        }
        actual.push((p.0, p.1));
        prev = (p.0, p.1);
    }
    tramo(corte_actual, &mut actual, &mut s);
    s += &format!(r##"<rect x="{:.1}" y="{:.1}" width="5" height="5" fill="{acento}"/>"##, px(0.0) - 2.5, py(0.0) - 2.5);
    let tam = rev.resumen()["tamano_mm"].clone();
    s += &format!(
        r##"<text x="{m}" y="22" fill="#e6e8ea" font-family="monospace" font-size="12" letter-spacing="2">RECORRIDO · VISTA SUPERIOR</text><text x="{}" y="22" fill="#8b8e94" font-family="monospace" font-size="11" text-anchor="end">{} × {} mm · {} min</text></svg>"##,
        w - m,
        tam[0].as_f64().unwrap_or(0.0),
        tam[1].as_f64().unwrap_or(0.0),
        (rev.minutos * 10.0).round() / 10.0
    );
    s
}

// ------------------------------------------------------------------ GRBL

/// Lo mínimo de un puerto serie (el real o uno de prueba).
pub trait Serie {
    fn escribir(&mut self, b: &[u8]);
    /// Una línea (sin el salto) o vacío si no llegó nada en ~1 s.
    fn leer_linea(&mut self) -> String;
}

pub struct SerieReal {
    p: Box<dyn serialport::SerialPort>,
    resto: Vec<u8>,
}

impl SerieReal {
    pub fn abrir(m: &Maquina) -> Result<Self, String> {
        let p = serialport::new(&m.puerto, m.baudios).timeout(Duration::from_secs(1)).open().map_err(|e| format!("no pude abrir {}: {e}", m.puerto))?;
        std::thread::sleep(Duration::from_secs(2)); // GRBL se reinicia al abrir el puerto
        let _ = p.clear(serialport::ClearBuffer::Input);
        Ok(SerieReal { p, resto: vec![] })
    }
}

impl Serie for SerieReal {
    fn escribir(&mut self, b: &[u8]) {
        let _ = self.p.write_all(b);
        let _ = self.p.flush();
    }
    fn leer_linea(&mut self) -> String {
        let fin = Instant::now() + Duration::from_secs(1);
        let mut b = [0u8; 64];
        while Instant::now() < fin {
            if let Some(i) = self.resto.iter().position(|c| *c == b'\n') {
                let l: Vec<u8> = self.resto.drain(..=i).collect();
                return String::from_utf8_lossy(&l).trim().to_string();
            }
            match self.p.read(&mut b) {
                Ok(n) if n > 0 => self.resto.extend_from_slice(&b[..n]),
                _ => {}
            }
        }
        String::new()
    }
}

pub struct Grbl<S: Serie> {
    pub s: S,
    pub detenido: bool,
}

impl<S: Serie> Grbl<S> {
    pub fn new(s: S) -> Self {
        Grbl { s, detenido: false }
    }

    fn respuesta(&mut self, espera: Duration) -> String {
        let fin = Instant::now() + espera;
        loop {
            let l = self.s.leer_linea();
            if !l.is_empty() || Instant::now() >= fin {
                return l;
            }
        }
    }

    pub fn enviar(&mut self, txt: &str) -> String {
        self.s.escribir(format!("{}\n", txt.trim()).as_bytes());
        loop {
            let r = self.respuesta(Duration::from_secs(10));
            if r.is_empty() || r.starts_with("ok") || r.starts_with("error") || r.starts_with("ALARM") {
                return r;
            }
        }
    }

    pub fn estado(&mut self) -> Value {
        self.s.escribir(b"?");
        let r = self.respuesta(Duration::from_secs(2));
        match Regex::new(r"<(\w+)[|,](?:MPos|WPos):(-?[\d.]+),(-?[\d.]+),(-?[\d.]+)").unwrap().captures(&r) {
            Some(c) => json!({"estado": &c[1], "pos_mm": ([2, 3, 4].map(|i| c[i].parse::<f64>().unwrap_or(0.0)))}),
            None => json!({"estado": "?", "crudo": r}),
        }
    }

    pub fn desbloquear(&mut self) -> String {
        self.enviar("$X")
    }
    pub fn casa(&mut self) -> String {
        self.enviar("$H")
    }
    pub fn cero_pieza(&mut self) -> String {
        self.enviar("G10 L20 P1 X0 Y0 Z0")
    }
    /// Jog relativo en mm (GRBL ≥ 1.1).
    pub fn mover(&mut self, x: f64, y: f64, z: f64, avance: f64) -> String {
        self.enviar(&format!("$J=G91 G21 X{x} Y{y} Z{z} F{avance}"))
    }
    pub fn pausa(&mut self) {
        self.s.escribir(b"!");
    }
    pub fn seguir(&mut self) {
        self.s.escribir(b"~");
    }
    /// Parada inmediata (reset suave): la máquina queda en alarma hasta $X.
    pub fn parar(&mut self) {
        self.detenido = true;
        self.s.escribir(&[0x18]);
    }

    /// Manda el programa línea por línea esperando cada «ok». avance(pct, línea) informa el progreso.
    pub fn trabajo(&mut self, gcode: &str, mut avance: impl FnMut(f64, usize)) -> Value {
        let lineas: Vec<String> = gcode.lines().map(sin_comentarios).filter(|l| !l.is_empty()).collect();
        self.detenido = false;
        for (i, l) in lineas.iter().enumerate() {
            if self.detenido {
                return json!({"ok": false, "detenido_en": i + 1});
            }
            let r = self.enviar(l);
            if r.starts_with("error") || r.starts_with("ALARM") {
                return json!({"ok": false, "linea": i + 1, "gcode": l, "respuesta": r});
            }
            avance(((i + 1) as f64 / lineas.len() as f64 * 1000.0).round() / 10.0, i + 1);
        }
        json!({"ok": true, "lineas": lineas.len()})
    }
}

fn conectar(puerto: &str) -> Result<Grbl<SerieReal>, Value> {
    let m = Maquina { puerto: puerto.into(), ..Default::default() };
    SerieReal::abrir(&m).map(Grbl::new).map_err(|e| json!({"ok": false, "error": e}))
}

/// Lo que puede hacer el HUD sin ticket: revisar y las órdenes que detienen o informan.
pub fn accion(d: &Value) -> Value {
    let a = d["accion"].as_str().unwrap_or("");
    let puerto = d["puerto"].as_str().unwrap_or("/dev/ttyUSB0");
    if a == "revisar" {
        let g: String = d["gcode"].as_str().unwrap_or("").chars().take(5_000_000).collect();
        let rev = revisar(&g, &Maquina::default());
        let mut r = rev.resumen();
        r["ok"] = json!(true);
        r["trazo"] = json!(rev.puntos.iter().take(4000).map(|p| json!([r2(p.0), r2(p.1), p.3])).collect::<Vec<_>>());
        return r;
    }
    if !["parar", "pausa", "seguir", "estado"].contains(&a) {
        return json!({"ok": false, "error": "mover y cortar se aprueban desde la IA con ticket"});
    }
    let mut g = match conectar(puerto) {
        Ok(g) => g,
        Err(e) => return e,
    };
    match a {
        "estado" => {
            let mut e = g.estado();
            e["ok"] = json!(true);
            e
        }
        "parar" => {
            g.parar();
            json!({"ok": true})
        }
        "pausa" => {
            g.pausa();
            json!({"ok": true})
        }
        _ => {
            g.seguir();
            json!({"ok": true})
        }
    }
}

/// La herramienta MCP `cnc` (las órdenes físicas pasan antes por la compuerta del servidor MCP).
pub fn herramienta(a: &Value, aprobado: bool) -> Value {
    let accion = a["accion"].as_str().unwrap_or("");
    let archivo = crate::util::expandir(a["archivo"].as_str().unwrap_or(""));
    let puerto = a["puerto"].as_str().unwrap_or("/dev/ttyUSB0");
    let leer = || std::fs::read_to_string(&archivo).map_err(|e| json!({"ok": false, "error": format!("{}: {e}", archivo.display())}));
    if accion == "revisar" {
        let g = match leer() {
            Ok(g) => g,
            Err(e) => return e,
        };
        let rev = revisar(&g, &Maquina::default());
        let acento = std::fs::read_to_string(crate::util::casa().join(".orion/acento")).ok().map(|s| s.trim().to_string()).filter(|s| s.starts_with('#')).unwrap_or_else(|| "#ff2438".into());
        let destino = crate::util::casa().join(".orion/cnc").join(format!("{}_recorrido.svg", archivo.file_stem().unwrap_or_default().to_string_lossy()));
        let _ = std::fs::create_dir_all(destino.parent().unwrap());
        let _ = std::fs::write(&destino, vista_svg(&rev, &acento));
        let mut r = rev.resumen();
        r["ok"] = json!(true);
        r["vista"] = json!(destino);
        return r;
    }
    if ["mover", "casa", "iniciar", "cero"].contains(&accion) && !aprobado {
        return json!({"ok": false, "error": "orden física sin aprobar"});
    }
    let gcode = if accion == "iniciar" {
        let g = match leer() {
            Ok(g) => g,
            Err(e) => return e,
        };
        let rev = revisar(&g, &Maquina::default());
        if !rev.errores.is_empty() {
            return json!({"ok": false, "errores": rev.errores});
        }
        g
    } else {
        String::new()
    };
    let mut g = match conectar(puerto) {
        Ok(g) => g,
        Err(e) => return e,
    };
    let n = |k: &str| a[k].as_f64().unwrap_or(0.0);
    match accion {
        "estado" => {
            let mut e = g.estado();
            e["ok"] = json!(true);
            e
        }
        "desbloquear" => json!({"ok": true, "respuesta": g.desbloquear()}),
        "casa" => json!({"ok": true, "respuesta": g.casa()}),
        "cero" => json!({"ok": true, "respuesta": g.cero_pieza()}),
        "mover" => json!({"ok": true, "respuesta": g.mover(n("x"), n("y"), n("z"), 600.0)}),
        "pausa" => {
            g.pausa();
            json!({"ok": true})
        }
        "seguir" => {
            g.seguir();
            json!({"ok": true})
        }
        "parar" => {
            g.parar();
            json!({"ok": true, "mensaje": "parada inmediata; desbloquea con $X cuando sea seguro"})
        }
        "iniciar" => g.trabajo(&gcode, |_, _| {}),
        _ => json!({"ok": false, "error": "accion desconocida"}),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    const GCODE: &str = "G21 G90\n(soporte)\nM3 S10000\nG0 X0 Y0 Z5\nG1 Z-2 F200\nG1 X50 F800\nG1 Y30\nG2 X40 Y40 I-10 J0\nG1 X0\nG1 Y0\nG0 Z5\nM5\n";

    #[test]
    fn revisar_gcode() {
        let r = revisar(GCODE, &Maquina::default()).resumen();
        assert_eq!(r["listo_para_cortar"], true);
        assert_eq!(r["tamano_mm"][0], 50.0);
        assert_eq!(r["tamano_mm"][1], 40.0);
        assert_eq!(r["profundidad_max_mm"], -2.0);
        assert_eq!(r["rpm"], json!([10000]));
        assert!(r["avances_mm_min"].as_array().unwrap().contains(&json!(200)));
        assert!(r["minutos"].as_f64().unwrap() > 0.0 && r["corte_m"].as_f64().unwrap() > 0.15);
        let grande = revisar("G21\nG1 X400 F500\n", &Maquina { x_mm: 300.0, ..Default::default() }).resumen();
        assert_eq!(grande["listo_para_cortar"], false);
        assert!(grande["errores"][0].as_str().unwrap().contains("300"));
        assert_eq!(revisar("G20\nG1 X1 F10\n", &Maquina::default()).resumen()["hasta_mm"][0], 25.4);
        assert_eq!(revisar("G1 X10\n", &Maquina::default()).resumen()["listo_para_cortar"], false);
        let svg = vista_svg(&revisar(GCODE, &Maquina::default()), "#ff2438");
        assert!(svg.starts_with("<svg") && svg.contains("stroke=\"#ff2438\"") && svg.contains("50 × 40 mm"));
    }

    struct Falsa {
        escrito: Vec<Vec<u8>>,
        salida: Vec<String>,
    }

    impl Serie for Falsa {
        fn escribir(&mut self, b: &[u8]) {
            self.escrito.push(b.to_vec());
            if b == b"?" {
                self.salida.push("<Idle|MPos:1.000,2.000,-3.000|FS:0,0>".into());
            } else if b.ends_with(b"\n") {
                self.salida.push(if b.starts_with(b"ROMPE") { "error:2" } else { "ok" }.into());
            }
        }
        fn leer_linea(&mut self) -> String {
            if self.salida.is_empty() { String::new() } else { self.salida.remove(0) }
        }
    }

    #[test]
    fn grbl_estado_trabajo_y_parada() {
        let mut g = Grbl::new(Falsa { escrito: vec![], salida: vec![] });
        assert_eq!(g.estado(), json!({"estado": "Idle", "pos_mm": [1.0, 2.0, -3.0]}));
        let mut pct = vec![];
        assert_eq!(g.trabajo(GCODE, |p, _| pct.push(p))["ok"], true);
        assert_eq!(*pct.last().unwrap(), 100.0);
        assert!(!g.s.escrito.concat().windows(9).any(|w| w == b"(soporte)")); // los comentarios no se mandan
        assert_eq!(g.trabajo("G1 X1 F100\nROMPE\n", |_, _| {})["linea"], 2);
        g.parar();
        assert_eq!(g.s.escrito.last().unwrap(), &vec![0x18]);
        assert!(g.detenido);
        g.mover(10.0, 0.0, 0.0, 600.0);
        assert!(g.s.escrito.last().unwrap().starts_with(b"$J=G91"));
    }
}
