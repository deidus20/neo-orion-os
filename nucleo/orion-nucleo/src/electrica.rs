//! Calculadora eléctrica (Chile, pliegos RIC de la SEC), igual que `orion_electrica` en Python.
//!
//! - RIC N°03 5.1.2: caída ≤ 3 % en alimentadores y subalimentadores; secciones mínimas 4 y 2,5 mm².
//! - RIC N°03 5.1.3: caída total ≤ 5 % en el punto más desfavorable.
//! - ΔV monofásica = 2·L·I·ρ/S · trifásica = √3·L·I·ρ/S (cos φ = 1, conservador en BT).
//! - Protección: In ≥ Ib e In ≤ Iz. Iz no se inventa: si falta, se avisa.

use regex::Regex;
use serde::Serialize;

pub const SECCIONES: [f64; 16] = [1.5, 2.5, 4.0, 6.0, 10.0, 16.0, 25.0, 35.0, 50.0, 70.0, 95.0, 120.0, 150.0, 185.0, 240.0, 300.0];
pub const PROTECCIONES: [u32; 15] = [6, 10, 16, 20, 25, 32, 40, 50, 63, 80, 100, 125, 160, 200, 250];
pub const LIMITE_TOTAL: f64 = 5.0;
pub const FUENTE: &str = "RIC N°03 (SEC) 5.1.2 y 5.1.3";

pub fn rho(material: &str) -> Option<f64> {
    match material {
        "cobre" => Some(0.018),
        "aluminio" => Some(0.029),
        _ => None,
    }
}
fn limite(tipo: &str) -> f64 {
    match tipo {
        "alimentador" | "subalimentador" => 3.0,
        "total" => 5.0,
        _ => 3.0,
    }
}
fn seccion_min(tipo: &str) -> Option<f64> {
    match tipo {
        "alimentador" => Some(4.0),
        "subalimentador" => Some(2.5),
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Resultado {
    pub tension_v: f64,
    pub fases: u8,
    pub corriente_a: f64,
    pub largo_m: f64,
    pub material: String,
    pub tipo: String,
    pub seccion_mm2: f64,
    pub caida_v: f64,
    pub caida_pct: f64,
    pub limite_pct: f64,
    pub cumple: bool,
    pub proteccion_a: Option<u32>,
    pub iz_a: Option<f64>,
    pub avisos: Vec<String>,
}

fn g(x: f64) -> String {
    // como el formato :g de Python para secciones (2.5, 4, 10)
    if x.fract() == 0.0 { format!("{}", x as i64) } else { format!("{x}") }
}

impl Resultado {
    pub fn texto(&self) -> String {
        let mut s = format!(
            "{} mm² de {}: caída {:.2} V ({:.2} %), límite {} % ({}) → {}.",
            g(self.seccion_mm2), self.material, self.caida_v, self.caida_pct, g(self.limite_pct), FUENTE,
            if self.cumple { "cumple" } else { "NO cumple" }
        );
        if let Some(p) = self.proteccion_a {
            s += &format!(" Protección {p} A.");
        }
        if !self.avisos.is_empty() {
            s += " ";
            s += &self.avisos.join(" ");
        }
        s
    }
}

/// Caída de tensión en volts (largo = distancia de ida, en metros).
pub fn caida(corriente_a: f64, largo_m: f64, seccion_mm2: f64, fases: u8, material: &str) -> Result<f64, String> {
    if corriente_a < 0.0 || largo_m < 0.0 || seccion_mm2 <= 0.0 {
        return Err("corriente y largo no pueden ser negativos; la sección debe ser mayor que 0".into());
    }
    let k = if fases == 1 { 2.0 } else { 3f64.sqrt() };
    let r = rho(material).ok_or("material: cobre o aluminio")?;
    Ok(k * largo_m * corriente_a * r / seccion_mm2)
}

pub fn corriente(potencia_w: f64, tension_v: f64, fases: u8) -> f64 {
    if fases == 1 { potencia_w / tension_v } else { potencia_w / (3f64.sqrt() * tension_v) }
}

/// Demanda de alumbrado en viviendas: 3 kW al 100 %, el exceso al 35 %.
pub fn demanda_vivienda(kw: f64) -> f64 {
    let p = kw.max(0.0);
    p.min(3.0) + (p - 3.0).max(0.0) * 0.35
}

pub struct Pedido<'a> {
    pub corriente_a: f64,
    pub largo_m: f64,
    pub tension_v: f64,
    pub fases: u8,
    pub material: &'a str,
    pub tipo: &'a str,
    pub limite_pct: Option<f64>,
    pub iz_a: Option<f64>,
    pub caida_previa_pct: f64,
}

/// La sección comercial más chica que cumple la caída y el mínimo del RIC N°03.
pub fn seccion(p: &Pedido) -> Result<Resultado, String> {
    if p.fases != 1 && p.fases != 3 {
        return Err("fases: 1 o 3".into());
    }
    rho(p.material).ok_or("material: cobre o aluminio")?;
    let lim = p.limite_pct.unwrap_or_else(|| limite(p.tipo));
    let minimo = seccion_min(p.tipo).unwrap_or(1.5);
    let mut avisos = Vec::new();
    let mut elegida = None;
    for &s in SECCIONES.iter().filter(|&&s| s >= minimo) {
        let pct = caida(p.corriente_a, p.largo_m, s, p.fases, p.material)? / p.tension_v * 100.0;
        if pct <= lim && pct + p.caida_previa_pct <= LIMITE_TOTAL {
            elegida = Some(s);
            break;
        }
    }
    let elegida = elegida.unwrap_or_else(|| {
        avisos.push("Ni la sección más grande de la tabla cumple: acorta el recorrido, sube la tensión o divide la carga.".into());
        SECCIONES[SECCIONES.len() - 1]
    });
    let v = caida(p.corriente_a, p.largo_m, elegida, p.fases, p.material)?;
    let pct = v / p.tension_v * 100.0;
    if elegida == minimo && seccion_min(p.tipo).is_some() {
        avisos.push(format!("Se usa el mínimo del RIC N°03 para {}: {} mm².", p.tipo, g(minimo)));
    }
    if p.caida_previa_pct != 0.0 {
        avisos.push(format!("Caída acumulada {:.2} % (máx. 5 % total).", pct + p.caida_previa_pct));
    }
    let prot = PROTECCIONES.iter().copied().find(|&x| x as f64 >= p.corriente_a);
    match (p.iz_a, prot) {
        (None, _) => avisos.push("Falta Iz del conductor (tabla del RIC N°04 o del fabricante) para verificar In ≤ Iz.".into()),
        (Some(iz), Some(pr)) if pr as f64 > iz => {
            avisos.push(format!("La protección de {pr} A supera la capacidad del conductor ({} A): sube la sección.", g(iz)))
        }
        _ => {}
    }
    Ok(Resultado {
        tension_v: p.tension_v,
        fases: p.fases,
        corriente_a: crate::util::redondear(p.corriente_a, 2),
        largo_m: p.largo_m,
        material: p.material.into(),
        tipo: p.tipo.into(),
        seccion_mm2: elegida,
        caida_v: crate::util::redondear(v, 2),
        caida_pct: crate::util::redondear(pct, 2),
        limite_pct: lim,
        cumple: pct <= lim && pct + p.caida_previa_pct <= LIMITE_TOTAL,
        proteccion_a: prot,
        iz_a: p.iz_a,
        avisos,
    })
}

/// «qué cable para 20 amperes a 35 metros trifásico» / «4500 W a 40 m en 220» → cálculo.
pub fn desde_frase(texto: &str) -> Option<Resultado> {
    let t = texto.to_lowercase().replace(',', ".");
    let num = |pat: &str| Regex::new(pat).ok()?.captures(&t)?.get(1)?.as_str().parse::<f64>().ok();
    let mut i = num(r"(\d+(?:\.\d+)?)\s*(?:a\b|amp)");
    let w = num(r"(\d+(?:\.\d+)?)\s*(?:w\b|watts?|vatios)");
    let kw = num(r"(\d+(?:\.\d+)?)\s*kw");
    let largo = num(r"(\d+(?:\.\d+)?)\s*(?:m\b|metros?)")?;
    let tri = Regex::new(r"trif|380|400\s*v").ok()?.is_match(&t);
    let v = num(r"(\d{3})\s*v").unwrap_or(if tri { 380.0 } else { 220.0 });
    let fases = if tri { 3 } else { 1 };
    if i.is_none() && (w.is_some() || kw.is_some()) {
        i = Some(corriente(w.unwrap_or(0.0) + kw.unwrap_or(0.0) * 1000.0, v, fases));
    }
    let tipo = if t.contains("alimentador") && !t.contains("sub") { "alimentador" } else { "subalimentador" };
    seccion(&Pedido {
        corriente_a: i?,
        largo_m: largo,
        tension_v: v,
        fases,
        material: if t.contains("alumin") { "aluminio" } else { "cobre" },
        tipo,
        limite_pct: None,
        iz_a: None,
        caida_previa_pct: 0.0,
    })
    .ok()
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn pedir(i: f64, l: f64, fases: u8, tipo: &str) -> Resultado {
        seccion(&Pedido { corriente_a: i, largo_m: l, tension_v: if fases == 1 { 220.0 } else { 380.0 }, fases, material: "cobre", tipo, limite_pct: None, iz_a: None, caida_previa_pct: 0.0 }).unwrap()
    }

    #[test]
    fn caida_monofasica_y_trifasica() {
        // 2·35·20·0,018/4 = 6,3 V
        assert!((caida(20.0, 35.0, 4.0, 1, "cobre").unwrap() - 6.3).abs() < 1e-9);
        assert!((caida(20.0, 35.0, 4.0, 3, "cobre").unwrap() - 3f64.sqrt() * 35.0 * 20.0 * 0.018 / 4.0).abs() < 1e-9);
        assert!(caida(-1.0, 1.0, 1.0, 1, "cobre").is_err());
    }

    #[test]
    fn elige_la_seccion_minima_que_cumple() {
        let r = pedir(20.0, 35.0, 1, "subalimentador"); // 4 mm² → 2,86 %
        assert_eq!(r.seccion_mm2, 4.0);
        assert!(r.cumple && r.caida_pct <= 3.0);
        assert_eq!(r.proteccion_a, Some(20));
        assert!(r.avisos.iter().any(|a| a.contains("Falta Iz")));
        // alimentador: nunca bajo 4 mm²
        assert_eq!(pedir(2.0, 5.0, 1, "alimentador").seccion_mm2, 4.0);
    }

    #[test]
    fn frase_hablada() {
        let r = desde_frase("qué cable para 20 amperes a 35 metros").unwrap();
        assert_eq!((r.seccion_mm2, r.fases), (4.0, 1));
        let r = desde_frase("4500 W a 40 m en 220").unwrap();
        assert!((r.corriente_a - 20.45).abs() < 0.01);
        assert_eq!(desde_frase("trifásico 30 a 60 metros").unwrap().fases, 3);
        assert!(desde_frase("hola").is_none());
        assert!(r.texto().contains("RIC N°03"));
    }

    #[test]
    fn demanda() {
        assert_eq!(demanda_vivienda(2.0), 2.0);
        assert!((demanda_vivienda(5.0) - 3.7).abs() < 1e-9);
    }
}
