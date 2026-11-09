//! LiDAR 2D para robots (antes orion_lidar): leer, mapear, detectar obstáculos y frenar a tiempo.
//!
//! Fuentes: RPLidar A1/A2/C1 por puerto serie (protocolo propio de Slamtec, sin librerías), el tópico /scan de
//! ROS 2 (con sllidar_ros2) o el simulador (sala con muros y personas que caminan, para pruebas y demo).
//! Procesamiento liviano (corre en Raspberry): mapa de ocupación (log-odds, trazado de rayos), obstáculo más
//! cercano por sector, decisión de frenado (se conecta al Guardián) y puntos móviles (personas) que no calzan con
//! el mapa estático. Para SLAM completo se integra slam_toolbox (ROS 2) o KISS-ICP.

use serde_json::{json, Value};
use std::f64::consts::PI;
use std::io::{Read, Write};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Escaneo {
    /// rad, 0 = frente del robot, sentido antihorario
    pub angulos: Vec<f64>,
    /// mm (0 o NaN = sin retorno)
    pub distancias: Vec<f64>,
    /// x_mm, y_mm, theta_rad del robot en el mapa
    pub pose: (f64, f64, f64),
}

impl Escaneo {
    pub fn puntos_mundo(&self) -> Vec<(f64, f64)> {
        let (x, y, th) = self.pose;
        self.angulos.iter().zip(&self.distancias).filter(|(_, d)| d.is_finite() && **d > 0.0).map(|(a, d)| (x + d * (a + th).cos(), y + d * (a + th).sin())).collect()
    }
}

/// Generador pseudoaleatorio chico (xorshift) con ruido normal: el simulador es reproducible sin dependencias.
pub struct Azar(u64);

impl Azar {
    pub fn new(semilla: u64) -> Self {
        Azar(semilla.max(1))
    }
    fn u(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    pub fn normal(&mut self, media: f64, desv: f64) -> f64 {
        let (u1, u2) = (self.u().max(1e-12), self.u());
        media + desv * (-2.0 * u1.ln()).sqrt() * (2.0 * PI * u2).cos()
    }
}

#[derive(Clone, Debug)]
pub struct Persona {
    pub centro: (f64, f64),
    pub radio: f64,
    pub vel: (f64, f64),
}

/// Sala poligonal + obstáculos + personas que caminan; rayos con intersección exacta.
pub struct Simulador {
    /// segmentos (x1, y1, x2, y2) en mm
    pub muros: Vec<[f64; 4]>,
    pub personas: Vec<Persona>,
    pub rayos: usize,
    pub alcance: f64,
    pub ruido: f64,
    pub t: f64,
    azar: Azar,
}

impl Simulador {
    pub fn new(muros: Vec<[f64; 4]>, personas: Vec<Persona>) -> Self {
        Simulador { muros, personas, rayos: 360, alcance: 8000.0, ruido: 8.0, t: 0.0, azar: Azar::new(3) }
    }

    pub fn avanzar(&mut self, dt: f64) {
        self.t += dt;
        for p in &mut self.personas {
            p.centro = (p.centro.0 + p.vel.0 * dt, p.centro.1 + p.vel.1 * dt);
        }
    }

    pub fn escanear(&mut self, pose: (f64, f64, f64)) -> Escaneo {
        let (x, y, th) = pose;
        let n = self.rayos;
        let mut angulos = Vec::with_capacity(n);
        let mut dist = Vec::with_capacity(n);
        for i in 0..n {
            let a = -PI + 2.0 * PI * i as f64 / n as f64;
            let (dx, dy) = ((a + th).cos(), (a + th).sin());
            let mut d = f64::INFINITY;
            for &[x1, y1, x2, y2] in &self.muros {
                let (ex, ey) = (x2 - x1, y2 - y1);
                let den = dx * ey - dy * ex;
                if den.abs() <= 1e-9 {
                    continue;
                }
                let t = ((x1 - x) * ey - (y1 - y) * ex) / den;
                let u = ((x1 - x) * dy - (y1 - y) * dx) / den;
                if t > 0.0 && (0.0..=1.0).contains(&u) && t < d {
                    d = t;
                }
            }
            for p in &self.personas {
                let (cx, cy, r) = (p.centro.0 - x, p.centro.1 - y, p.radio);
                let b = dx * cx + dy * cy;
                let disc = b * b - (cx * cx + cy * cy - r * r);
                let t = b - disc.max(0.0).sqrt();
                if disc > 0.0 && t > 0.0 && t < d {
                    d = t;
                }
            }
            let ruido = self.azar.normal(0.0, self.ruido);
            angulos.push(a);
            dist.push(if d > self.alcance { f64::NAN } else { d + ruido });
        }
        Escaneo { angulos, distancias: dist, pose }
    }
}

// ------------------------------------------------------------------ RPLidar (Slamtec) por puerto serie

/// Un paquete de medición del modo «scan» estándar (5 bytes) → (nueva vuelta, calidad, ángulo °, distancia mm).
pub fn paquete_rplidar(b: &[u8; 5]) -> Option<(bool, u8, f64, f64)> {
    let nuevo = b[0] & 0x1 == 1;
    if nuevo == (b[0] & 0x2 == 2) || b[1] & 0x1 != 1 {
        return None; // bits de control inconsistentes: se descarta
    }
    let angulo = ((b[1] as u16 >> 1) | ((b[2] as u16) << 7)) as f64 / 64.0;
    let distancia = ((b[3] as u16) | ((b[4] as u16) << 8)) as f64 / 4.0;
    Some((nuevo, b[0] >> 2, angulo, distancia))
}

/// Lee vueltas completas de un RPLidar (A1/A2/C1). El RPLidar gira en sentido horario: se invierte el ángulo.
pub fn leer_rplidar(puerto: &str, vueltas: usize, mut cada: impl FnMut(Escaneo)) -> Result<(), String> {
    let mut p = serialport::new(puerto, 115200).timeout(Duration::from_millis(500)).open().map_err(|e| format!("no pude abrir {puerto}: {e}"))?;
    let _ = p.write_data_terminal_ready(false); // A1: DTR bajo enciende el motor
    p.write_all(&[0xA5, 0x20]).map_err(|e| e.to_string())?;
    let mut cab = [0u8; 7];
    p.read_exact(&mut cab).map_err(|e| format!("el LiDAR no respondió: {e}"))?;
    if cab[0..2] != [0xA5, 0x5A] {
        return Err("respuesta desconocida del LiDAR".into());
    }
    let (mut ang, mut dist) = (vec![], vec![]);
    let mut hechas = 0;
    let mut b = [0u8; 5];
    while hechas < vueltas {
        p.read_exact(&mut b).map_err(|e| e.to_string())?;
        let Some((nuevo, calidad, a, d)) = paquete_rplidar(&b) else { continue };
        if nuevo && !ang.is_empty() {
            cada(Escaneo { angulos: std::mem::take(&mut ang), distancias: std::mem::take(&mut dist), pose: (0.0, 0.0, 0.0) });
            hechas += 1;
        }
        if calidad > 0 && d > 0.0 {
            ang.push(-a.to_radians());
            dist.push(d);
        }
    }
    let _ = p.write_all(&[0xA5, 0x25]); // detener
    let _ = p.write_data_terminal_ready(true);
    Ok(())
}

// ------------------------------------------------------------------ mapa de ocupación

pub struct Mapa {
    pub ancho_mm: f64,
    pub alto_mm: f64,
    pub celda_mm: f64,
    pub origen: (f64, f64),
    pub w: usize,
    pub h: usize,
    pub logodds: Vec<f32>,
}

impl Mapa {
    pub fn new(ancho_mm: f64, alto_mm: f64, celda_mm: f64, origen: (f64, f64)) -> Self {
        let (w, h) = ((ancho_mm / celda_mm) as usize, (alto_mm / celda_mm) as usize);
        Mapa { ancho_mm, alto_mm, celda_mm, origen, w, h, logodds: vec![0.0; w * h] }
    }

    fn celda(&self, p: (f64, f64)) -> (i64, i64) {
        (((p.0 - self.origen.0) / self.celda_mm) as i64, ((p.1 - self.origen.1) / self.celda_mm) as i64)
    }

    fn dentro(&self, c: (i64, i64)) -> bool {
        c.0 >= 0 && (c.0 as usize) < self.w && c.1 >= 0 && (c.1 as usize) < self.h
    }

    fn i(&self, c: (i64, i64)) -> usize {
        c.1 as usize * self.w + c.0 as usize
    }

    pub fn actualizar(&mut self, e: &Escaneo) {
        let (libre, ocupado) = (-0.4f32, 0.9f32);
        let r0 = self.celda((e.pose.0, e.pose.1));
        for p in e.puntos_mundo() {
            let c = self.celda(p);
            let n = (c.0 - r0.0).abs().max((c.1 - r0.1).abs());
            if n == 0 {
                continue;
            }
            for k in 0..n {
                // como np.linspace(r0, c, n, endpoint=False).astype(int)
                let f = k as f64 / n as f64;
                let q = ((r0.0 as f64 + (c.0 - r0.0) as f64 * f) as i64, (r0.1 as f64 + (c.1 - r0.1) as f64 * f) as i64);
                if self.dentro(q) {
                    let i = self.i(q);
                    self.logodds[i] += libre;
                }
            }
            if self.dentro(c) {
                let i = self.i(c);
                self.logodds[i] += ocupado;
            }
        }
        for v in &mut self.logodds {
            *v = v.clamp(-4.0, 6.0);
        }
    }

    pub fn ocupado(&self, c: (i64, i64)) -> bool {
        self.dentro(c) && self.logodds[self.i(c)] > 1.5
    }

    pub fn ocupadas(&self) -> usize {
        self.logodds.iter().filter(|v| **v > 1.5).count()
    }

    pub fn exploradas(&self) -> usize {
        self.logodds.iter().filter(|v| v.abs() > 0.3).count()
    }

    /// Puntos del escaneo que no calzan con el mapa estático → personas u objetos que se mueven.
    pub fn moviles(&self, e: &Escaneo, radio: i64) -> Vec<(f64, f64)> {
        e.puntos_mundo()
            .into_iter()
            .filter(|p| {
                let (cx, cy) = self.celda(*p);
                !(cy - radio..=cy + radio).any(|y| (cx - radio..=cx + radio).any(|x| self.ocupado((x, y))))
            })
            .collect()
    }

    /// El mapa en la estética del sistema (SVG): libre en gris, ocupado en el acento, móviles en rojo.
    pub fn svg(&self, e: Option<&Escaneo>, moviles: &[(f64, f64)], acento: &str) -> String {
        let esc = 900.0 / self.w as f64;
        let (ancho, alto) = (900.0, self.h as f64 * esc + 34.0);
        let mut s = format!(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {ancho} {alto:.0}" shape-rendering="crispEdges"><rect width="100%" height="100%" fill="#101113"/><text x="10" y="22" fill="{acento}" font-family="monospace" font-size="13" letter-spacing="2">LIDAR · MAPA DE OCUPACIÓN</text><g transform="translate(0,34)">"##);
        for y in 0..self.h {
            for x in 0..self.w {
                let v = self.logodds[y * self.w + x];
                let color = if v > 1.5 { acento } else if v < -0.3 { "#1c1e21" } else { continue };
                s += &format!(r#"<rect x="{:.1}" y="{:.1}" width="{:.2}" height="{:.2}" fill="{color}"/>"#, x as f64 * esc, (self.h - 1 - y) as f64 * esc, esc + 0.05, esc + 0.05);
            }
        }
        let a_px = |p: (f64, f64)| ((p.0 - self.origen.0) / self.celda_mm * esc, (self.h as f64 - (p.1 - self.origen.1) / self.celda_mm) * esc);
        if let Some(e) = e {
            for p in e.puntos_mundo().iter().step_by(2) {
                let (x, y) = a_px(*p);
                s += &format!(r##"<rect x="{:.1}" y="{:.1}" width="2" height="2" fill="#e6e8ea"/>"##, x - 1.0, y - 1.0);
            }
            let (x, y) = a_px((e.pose.0, e.pose.1));
            s += &format!(r#"<rect x="{:.1}" y="{:.1}" width="8" height="8" fill="{acento}"/>"#, x - 4.0, y - 4.0);
        }
        for p in moviles {
            let (x, y) = a_px(*p);
            s += &format!(r##"<rect x="{:.1}" y="{:.1}" width="5" height="5" fill="#ff4a2a"/>"##, x - 2.5, y - 2.5);
        }
        s + "</g></svg>"
    }
}

// ------------------------------------------------------------------ seguridad

/// Distancia mínima por sector (frente, frente-izq, izq, …).
pub fn sectores(e: &Escaneo, n: usize) -> Vec<Value> {
    let nombres = ["frente", "frente-izq", "izquierda", "atrás-izq", "atrás", "atrás-der", "derecha", "frente-der"];
    (0..n)
        .map(|k| {
            let min = e
                .angulos
                .iter()
                .zip(&e.distancias)
                .filter(|(a, d)| d.is_finite() && ((((*a + PI / n as f64).rem_euclid(2.0 * PI)) / (2.0 * PI / n as f64)) as usize % n) == k)
                .map(|(_, d)| *d)
                .fold(f64::INFINITY, f64::min);
            json!({"sector": if n == 8 { nombres[k].to_string() } else { k.to_string() }, "min_mm": if min.is_finite() { json!(min) } else { Value::Null }})
        })
        .collect()
}

/// ¿Puede seguir avanzando? Mira el corredor del ancho del robot hacia adelante.
pub fn decision_avance(e: &Escaneo, vel_mm_s: f64, frenado_mm: f64, reaccion_s: f64, ancho_mm: f64) -> Value {
    let libre = e
        .angulos
        .iter()
        .zip(&e.distancias)
        .filter(|(a, d)| d.is_finite() && (a.sin() * **d).abs() < ancho_mm / 2.0 && a.cos() > 0.0)
        .map(|(a, d)| a.cos() * d)
        .fold(f64::INFINITY, f64::min);
    let necesaria = frenado_mm + vel_mm_s * reaccion_s;
    if libre < necesaria {
        let v = ((libre - frenado_mm) / reaccion_s).max(0.0);
        return json!({"accion": if v < 1.0 { "FRENAR" } else { "REDUCIR" }, "libre_mm": libre.round(), "vel_max_mm_s": v.round()});
    }
    json!({"accion": "SEGUIR", "libre_mm": if libre.is_finite() { json!(libre.round()) } else { json!(null) }, "vel_max_mm_s": vel_mm_s})
}

/// orion-nucleo lidar demo [mapa.svg] | orion-nucleo lidar /dev/ttyUSB0 [vueltas]
pub fn principal(args: &[String]) -> i32 {
    let acento = std::fs::read_to_string(crate::util::casa().join(".orion/acento")).ok().map(|s| s.trim().to_string()).filter(|s| s.starts_with('#')).unwrap_or_else(|| "#ff2438".into());
    match args.first().map(String::as_str) {
        Some("demo") | None => {
            let muros = vec![
                [0.0, 0.0, 8000.0, 0.0], [8000.0, 0.0, 8000.0, 6000.0], [8000.0, 6000.0, 0.0, 6000.0], [0.0, 6000.0, 0.0, 0.0],
                [3000.0, 2500.0, 4200.0, 2500.0], [4200.0, 2500.0, 4200.0, 3300.0], [4200.0, 3300.0, 3000.0, 3300.0], [3000.0, 3300.0, 3000.0, 2500.0], [5500.0, 0.0, 5500.0, 1800.0],
            ];
            let mut sim = Simulador::new(muros, vec![Persona { centro: (6500.0, 4500.0), radio: 200.0, vel: (-600.0, -150.0) }]);
            let mut mapa = Mapa::new(10000.0, 8000.0, 50.0, (-1000.0, -1000.0));
            let mut e = sim.escanear((1000.0, 1200.0, 0.15));
            for k in 0..40 {
                e = sim.escanear((1000.0 + 120.0 * k as f64, 1200.0 + 25.0 * k as f64, 0.15));
                mapa.actualizar(&e);
                sim.avanzar(0.1);
            }
            let mov = mapa.moviles(&e, 2);
            println!("celdas ocupadas: {} · exploradas: {} · puntos móviles (persona): {}", mapa.ocupadas(), mapa.exploradas(), mov.len());
            println!("sectores: {}", Value::from(sectores(&e, 8)));
            let salida = args.get(1).cloned().unwrap_or_else(|| "lidar-mapa.svg".into());
            match std::fs::write(&salida, mapa.svg(Some(&e), &mov, &acento)) {
                Ok(_) => println!("mapa: {salida}"),
                Err(e) => eprintln!("{salida}: {e}"),
            }
            0
        }
        Some(puerto) => {
            let vueltas = args.get(1).and_then(|v| v.parse().ok()).unwrap_or(3);
            match leer_rplidar(puerto, vueltas, |e| println!("{} · {}", Value::from(sectores(&e, 8)), decision_avance(&e, 300.0, 150.0, 0.3, 400.0))) {
                Ok(_) => 0,
                Err(e) => {
                    eprintln!("{e}");
                    1
                }
            }
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// La misma sala de la prueba de Python: 8×6 m, una mesa, un tabique y una persona caminando.
    fn sala() -> Simulador {
        let muros = vec![
            [0.0, 0.0, 8000.0, 0.0], [8000.0, 0.0, 8000.0, 6000.0], [8000.0, 6000.0, 0.0, 6000.0], [0.0, 6000.0, 0.0, 0.0],
            [3000.0, 2500.0, 4200.0, 2500.0], [4200.0, 2500.0, 4200.0, 3300.0], [4200.0, 3300.0, 3000.0, 3300.0], [3000.0, 3300.0, 3000.0, 2500.0],
            [5500.0, 0.0, 5500.0, 1800.0],
        ];
        Simulador::new(muros, vec![Persona { centro: (6500.0, 4500.0), radio: 200.0, vel: (-600.0, -150.0) }])
    }

    #[test]
    fn mapa_moviles_y_frenado() {
        let mut sim = sala();
        let mut mapa = Mapa::new(10000.0, 8000.0, 50.0, (-1000.0, -1000.0));
        let mut ultimo = None;
        for k in 0..40 {
            let e = sim.escanear((1000.0 + 120.0 * k as f64, 1200.0 + 25.0 * k as f64, 0.15));
            mapa.actualizar(&e);
            sim.avanzar(0.1);
            ultimo = Some(e);
        }
        let e = ultimo.unwrap();
        // Python (numpy, otra semilla de ruido) da 1025 ocupadas y 19260 exploradas: mismo orden de magnitud
        assert!((800..1300).contains(&mapa.ocupadas()), "ocupadas {}", mapa.ocupadas());
        assert!((17000..21000).contains(&mapa.exploradas()), "exploradas {}", mapa.exploradas());
        // la persona aparece como móvil cerca de donde está de verdad
        let mov = mapa.moviles(&e, 2);
        assert!(mov.len() >= 5, "móviles {}", mov.len());
        let (cx, cy) = (mov.iter().map(|p| p.0).sum::<f64>() / mov.len() as f64, mov.iter().map(|p| p.1).sum::<f64>() / mov.len() as f64);
        let real = sim.personas[0].centro;
        assert!((cx - real.0).hypot(cy - real.1) < 900.0, "centro {cx},{cy} real {real:?}");
        let s = sectores(&e, 8);
        assert_eq!(s.len(), 8);
        assert_eq!(s[0]["sector"], "frente");
        // muro a 600 mm a 400 mm/s: necesita 150 + 120 = 270 mm → sigue; a 2000 mm/s debe frenar
        let muro = sim.escanear((7400.0, 3000.0, 0.0));
        assert_eq!(decision_avance(&muro, 400.0, 150.0, 0.3, 400.0)["accion"], "SEGUIR");
        assert_eq!(decision_avance(&muro, 2000.0, 150.0, 0.3, 400.0)["accion"], "REDUCIR");
        let pegado = sim.escanear((7900.0, 3000.0, 0.0));
        assert_eq!(decision_avance(&pegado, 400.0, 150.0, 0.3, 400.0)["accion"], "FRENAR");
        let pasillo = decision_avance(&sim.escanear((2000.0, 3000.0, 0.0)), 400.0, 150.0, 0.3, 400.0);
        assert!((960.0..1010.0).contains(&pasillo["libre_mm"].as_f64().unwrap()), "{pasillo}");
        assert!(mapa.svg(Some(&e), &mov, "#ff2438").contains("LIDAR · MAPA DE OCUPACIÓN"));
    }

    #[test]
    fn protocolo_rplidar() {
        // nueva vuelta, calidad 15, 90°, 1000 mm
        let a = (90.0 * 64.0) as u16;
        let d = (1000.0 * 4.0) as u16;
        let b = [(15 << 2) | 0b01, ((a << 1) as u8) | 1, (a >> 7) as u8, d as u8, (d >> 8) as u8];
        assert_eq!(paquete_rplidar(&b), Some((true, 15, 90.0, 1000.0)));
        let mut malo = b;
        malo[0] |= 0b10; // nuevo y no-nuevo a la vez
        assert_eq!(paquete_rplidar(&malo), None);
    }
}
