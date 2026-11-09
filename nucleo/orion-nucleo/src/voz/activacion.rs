//! Formas de iniciar Neo Orión: frases, palmadas y rutinas («Buenos días, señoritas» 👏👏 y todo despierta).
//! Configuración en ~/.orion/activacion.json (mismo formato que antes). Reposo atento, suspensión y Wake-on-LAN.

use crate::voz::llave::frase_calza;
use crate::voz::RATE;
use chrono::Timelike;
use regex::Regex;
use serde_json::{json, Value};
use std::path::PathBuf;

pub const ACCIONES: [(&str, &str); 9] = [
    ("despertar", "enciende la pantalla y vuelve a cargar la IA"),
    ("saludo", "te saluda por tu nombre y la hora del día"),
    ("luces", "enciende las luces (Home Assistant)"),
    ("musica", "pone tu música"),
    ("resumen", "resumen del día: pendientes, proyectos y memoria de ayer"),
    ("proyecto", "abre el último proyecto"),
    ("taller", "modo taller: CNC, cámaras y robots a la vista"),
    ("reposo", "apaga la pantalla y descarga la IA (reposo atento)"),
    ("suspender", "suspende el equipo (lo despierta el nodo, el teclado o el teléfono)"),
];
pub const SENSIBILIDAD: [(&str, f64); 3] = [("sensible", 5.0), ("normal", 8.0), ("firme", 12.0)];

pub fn por_defecto() -> Value {
    json!({
        "activadores": [
            {"id": "buenos-dias", "tipo": "frase", "frase": "Buenos días, señoritas", "solo_dueno": false, "rutina": ["despertar", "saludo", "luces", "resumen"]},
            {"id": "palmadas", "tipo": "palmadas", "veces": 2, "rutina": ["despertar", "saludo"]},
            {"id": "a-dormir", "tipo": "frase", "frase": "Buenas noches", "solo_dueno": true, "rutina": ["reposo"]}
        ],
        "reposo": {"tras_minutos": 20, "nivel": "atento"},
        "saludo": "{saludo}, {nombre}. Todo listo.",
        "palmadas": {"sensibilidad": "normal"},
        "wol": {"mac": "", "interfaz": ""}
    })
}

pub fn ruta() -> PathBuf {
    crate::util::casa().join(".orion/activacion.json")
}
pub fn leer() -> Value {
    std::fs::read_to_string(ruta()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(por_defecto)
}
pub fn guardar(c: &Value) -> std::io::Result<()> {
    std::fs::create_dir_all(ruta().parent().unwrap())?;
    std::fs::write(ruta(), serde_json::to_string_pretty(c)?)
}
pub fn acciones() -> Value {
    ACCIONES.iter().map(|(k, v)| (k.to_string(), json!(v))).collect::<serde_json::Map<_, _>>().into()
}

/// Un paso de rutina: acción conocida, o libre: «di:…», «abre:sitio», «orden:…».
pub fn paso_valido(a: &str) -> bool {
    if ACCIONES.iter().any(|(k, _)| *k == a) {
        return true;
    }
    let Some(c) = Regex::new(r"(?s)^(di|abre|orden):(.{1,300})$").unwrap().captures(a) else { return false };
    let resto = c[2].trim();
    if &c[1] == "abre" {
        return Regex::new(r"^(https?://|[\w-]+(\.[\w-]+)+)").unwrap().is_match(resto);
    }
    !resto.is_empty()
}

fn validar(a: &Value) -> Result<(), String> {
    if a["tipo"] == "frase" && a["frase"].as_str().unwrap_or("").split_whitespace().count() < 2 {
        return Err("la frase necesita al menos 2 palabras".into());
    }
    if a["tipo"] == "palmadas" && !matches!(a["veces"].as_i64(), Some(2..=4)) {
        return Err("palmadas: 2, 3 o 4".into());
    }
    let rutina: Vec<&str> = a["rutina"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    let malas: Vec<&str> = rutina.iter().copied().filter(|x| !paso_valido(x)).collect();
    if !malas.is_empty() {
        return Err(format!("pasos inválidos: {}", malas.join(", ")));
    }
    if rutina.is_empty() {
        return Err("la rutina necesita al menos un paso".into());
    }
    Ok(())
}

pub fn agregar(mut c: Value, tipo: &str, rutina: Vec<String>, frase: &str, veces: i64, solo_dueno: bool) -> Result<Value, String> {
    let base = if frase.is_empty() { format!("{veces}-palmadas") } else { frase.to_lowercase() };
    let id: String = Regex::new(r"[^a-z0-9]+").unwrap().replace_all(&base, "-").trim_matches('-').chars().take(40).collect();
    let mut a = json!({"id": id, "tipo": tipo, "rutina": rutina});
    if tipo == "frase" {
        a["frase"] = frase.into();
        a["solo_dueno"] = solo_dueno.into();
    } else {
        a["veces"] = veces.into();
    }
    validar(&a)?;
    let lista = c["activadores"].as_array_mut().ok_or("configuración inválida")?;
    lista.retain(|x| x["id"] != id);
    lista.push(a);
    Ok(c)
}

pub fn editar(mut c: Value, id: &str, cambios: &Value) -> Result<Value, String> {
    let lista = c["activadores"].as_array_mut().ok_or("configuración inválida")?;
    let a = lista.iter_mut().find(|x| x["id"] == id).ok_or("no existe esa forma de inicio")?;
    let mut nuevo = a.clone();
    for k in ["frase", "veces", "solo_dueno", "rutina", "nombre", "activo"] {
        if let Some(v) = cambios.get(k) {
            nuevo[k] = v.clone();
        }
    }
    validar(&nuevo)?;
    *a = nuevo;
    Ok(c)
}

pub fn quitar(mut c: Value, id: &str) -> Value {
    if let Some(l) = c["activadores"].as_array_mut() {
        l.retain(|x| x["id"] != id);
    }
    c
}

// ------------------------------------------------------------------ palmadas
pub fn palmadas_de(c: &Value) -> Palmadas {
    let s = c["palmadas"]["sensibilidad"].as_str().unwrap_or("normal");
    Palmadas::new(SENSIBILIDAD.iter().find(|(k, _)| *k == s).map(|x| x.1).unwrap_or(8.0))
}

/// Detecta golpes cortos y fuertes (16 kHz mono) y cuenta series de 2–4: la energía en 10 ms salta > `salto`× sobre
/// el fondo (con un piso) y cae en < 120 ms; golpes separados 0,15–0,8 s; la serie se cierra tras 0,9 s.
pub struct Palmadas {
    pub salto: f64,
    pub piso: f64,
    pub min_sep: f64,
    pub max_sep: f64,
    pub fondo: f64,
    t: f64,
    golpes: Vec<f64>,
    alto_desde: Option<f64>,
}

impl Palmadas {
    pub fn new(salto: f64) -> Self {
        Palmadas { salto, piso: 0.06, min_sep: 0.15, max_sep: 0.8, fondo: 0.003, t: 0.0, golpes: vec![], alto_desde: None }
    }

    /// Devuelve cuántas palmadas cuando se completa una serie.
    pub fn alimentar(&mut self, m: &[i16]) -> Option<usize> {
        let n = RATE / 100;
        let mut hecho = None;
        for v in m.chunks_exact(n) {
            let rms = (v.iter().map(|&x| (x as f64 / 32768.0).powi(2)).sum::<f64>() / n as f64).sqrt() + 1e-9;
            let alto = rms > self.piso.max(self.fondo * self.salto);
            match (alto, self.alto_desde) {
                (true, None) => self.alto_desde = Some(self.t),
                (false, Some(d)) => {
                    let ultimo = self.golpes.last().copied();
                    if self.t - d <= 0.12 && ultimo.map_or(true, |u| self.t - u >= self.min_sep) {
                        if ultimo.is_some_and(|u| self.t - u > self.max_sep) {
                            self.golpes.clear();
                        }
                        self.golpes.push(self.t);
                    }
                    self.alto_desde = None;
                }
                _ => {}
            }
            if !alto {
                self.fondo = 0.995 * self.fondo + 0.005 * rms;
            }
            if self.golpes.last().is_some_and(|u| self.t - u > 0.9) {
                if self.golpes.len() >= 2 {
                    hecho = Some(self.golpes.len());
                }
                self.golpes.clear();
            }
            self.t += 0.01;
        }
        hecho
    }
}

/// ¿Qué activador calza con lo oído?
pub fn elegir(c: &Value, texto: Option<&str>, palmadas: Option<usize>, es_dueno: Option<bool>) -> Option<Value> {
    for a in c["activadores"].as_array().into_iter().flatten() {
        if a["activo"] == false {
            continue;
        }
        if a["tipo"] == "palmadas" && palmadas.is_some_and(|p| p as i64 == a["veces"].as_i64().unwrap_or(2)) {
            return Some(a.clone());
        }
        if let (true, Some(t)) = (a["tipo"] == "frase", texto) {
            if frase_calza(t, a["frase"].as_str().unwrap_or("")) >= 0.75 {
                if a["solo_dueno"] == true && es_dueno == Some(false) {
                    continue;
                }
                return Some(a.clone());
            }
        }
    }
    None
}

// ------------------------------------------------------------------ acciones
pub fn saludo(nombre: &str, hora: Option<u32>, plantilla: Option<&str>) -> String {
    let h = hora.unwrap_or_else(|| chrono::Local::now().hour());
    let parte = if (5..12).contains(&h) { "Buenos días" } else if h < 20 && h >= 12 { "Buenas tardes" } else { "Buenas noches" };
    match plantilla {
        Some(p) if !p.is_empty() => {
            let t = p.replace("{saludo}", parte).replace("{nombre}", nombre);
            let t = Regex::new(r",\s*,").unwrap().replace_all(&t, ",");
            Regex::new(r"\s*,\s*\.").unwrap().replace_all(&t, ".").trim().to_string()
        }
        _ => format!("{parte}{}. Todo listo.", if nombre.is_empty() { String::new() } else { format!(", {nombre}") }),
    }
}

pub fn pantalla(encender: bool) -> bool {
    std::process::Command::new("wlopm").args([if encender { "--on" } else { "--off" }, "*"]).output().map(|o| o.status.success()).unwrap_or(false)
}

/// Saca los modelos de la memoria (Ollama keep_alive=0); vuelven solos con la próxima pregunta.
pub fn descargar_ia() {
    for m in crate::ia::modelos_locales() {
        let _ = ureq::post(&format!("{}/api/generate", crate::ia::ollama())).timeout(std::time::Duration::from_secs(10)).send_json(json!({"model": m, "keep_alive": 0}));
    }
}

pub fn ejecutar_rutina(rutina: &[String], nombre: &str) -> Vec<Value> {
    rutina
        .iter()
        .map(|a| {
            let r: Value = match a.as_str() {
                "despertar" => pantalla(true).into(),
                "reposo" => {
                    pantalla(false);
                    descargar_ia();
                    true.into()
                }
                "suspender" => std::process::Command::new("systemctl").arg("suspend").output().map(|o| o.status.success()).unwrap_or(false).into(),
                "saludo" => saludo(nombre, None, None).into(),
                "luces" => crate::casa::Casa::cargar().ordenar("enciende las luces", false),
                x if x.starts_with("di:") => x[3..].trim().into(),
                _ => "pendiente en el HUD".into(),
            };
            json!({"accion": a, "ok": !(r == false || r.is_null()), "resultado": r})
        })
        .collect()
}

// ------------------------------------------------------------------ Wake-on-LAN
pub fn paquete_magico(mac: &str) -> Result<Vec<u8>, String> {
    let h: String = mac.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if h.len() != 12 {
        return Err("MAC inválida".into());
    }
    let b: Vec<u8> = (0..6).map(|i| u8::from_str_radix(&h[i * 2..i * 2 + 2], 16).unwrap()).collect();
    let mut p = vec![0xffu8; 6];
    for _ in 0..16 {
        p.extend(&b);
    }
    Ok(p)
}

/// Lo usa el nodo despertador (o el teléfono) para encender/despertar el PC central por la red.
pub fn despertar_equipo(mac: &str, broadcast: &str, puerto: u16) -> Result<(), String> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    s.set_broadcast(true).map_err(|e| e.to_string())?;
    s.send_to(&paquete_magico(mac)?, (broadcast, puerto)).map_err(|e| e.to_string())?;
    Ok(())
}

/// Deja la red cableada lista para Wake-on-LAN y devuelve su MAC.
pub fn preparar_wol() -> Value {
    let mut ifs: Vec<_> = std::fs::read_dir("/sys/class/net").into_iter().flatten().flatten().map(|e| e.path()).collect();
    ifs.sort();
    for d in ifs {
        let nombre = d.file_name().unwrap().to_string_lossy().to_string();
        if nombre == "lo" || d.join("wireless").exists() || !d.join("device").exists() {
            continue;
        }
        let mac = crate::util::leer(&d.join("address").to_string_lossy());
        let ok = std::process::Command::new("sudo").args(["ethtool", "-s", &nombre, "wol", "g"]).output().map(|o| o.status.success()).unwrap_or(false);
        return json!({"ok": ok, "interfaz": nombre, "mac": mac, "bios": "activa «Wake on LAN» o «Power on by PCI-E» en el BIOS para encender desde apagado"});
    }
    json!({"ok": false, "error": "no hay tarjeta de red cableada (Wake-on-LAN no funciona por Wi-Fi en la mayoría de los PC)"})
}

/// Las acciones que el HUD pide (/api/accion/activacion).
pub fn accion(d: &Value) -> Value {
    let a = d["accion"].as_str().unwrap_or("");
    let c = leer();
    let r: Result<Option<Value>, String> = match a {
        "agregar" => {
            let rutina = d["rutina"].as_array().map(|x| x.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_else(|| vec!["despertar".into(), "saludo".into()]);
            agregar(c, d["tipo"].as_str().unwrap_or("frase"), rutina, d["frase"].as_str().unwrap_or(""), d["veces"].as_i64().unwrap_or(2), d["solo_dueno"] == true).and_then(|c| guardar(&c).map_err(|e| e.to_string())).map(|_| None)
        }
        "quitar" => guardar(&quitar(c, d["id"].as_str().unwrap_or(""))).map_err(|e| e.to_string()).map(|_| None),
        "editar" => editar(c, d["id"].as_str().unwrap_or(""), &d["cambios"]).and_then(|c| guardar(&c).map_err(|e| e.to_string())).map(|_| None),
        "ajustes" => {
            let mut c = c;
            if let Some(s) = d["saludo"].as_str() {
                c["saludo"] = s.chars().take(200).collect::<String>().into();
            }
            if let Some(s) = d["sensibilidad"].as_str().filter(|s| SENSIBILIDAD.iter().any(|(k, _)| k == s)) {
                c["palmadas"] = json!({"sensibilidad": s});
            }
            guardar(&c).map_err(|e| e.to_string()).map(|_| None)
        }
        "reposo" => {
            let mut c = c;
            c["reposo"] = json!({"tras_minutos": d["tras_minutos"].as_i64().unwrap_or(20), "nivel": d["nivel"].as_str().unwrap_or("atento")});
            guardar(&c).map_err(|e| e.to_string()).map(|_| None)
        }
        "wol" => {
            let r = preparar_wol();
            if r["ok"] == true {
                let mut c = c;
                c["wol"] = json!({"mac": r["mac"], "interfaz": r["interfaz"]});
                let _ = guardar(&c);
            }
            Ok(Some(r))
        }
        "ejecutar" => {
            let x = c["activadores"].as_array().and_then(|l| l.iter().find(|v| v["id"] == d["id"]).cloned());
            let pasos: Vec<String> = x.as_ref().and_then(|x| x["rutina"].as_array()).into_iter().flatten().filter_map(|v| v.as_str()).filter(|p| ["despertar", "reposo", "luces"].contains(p)).map(String::from).collect();
            Ok(Some(json!({"ok": x.is_some(), "hechos": ejecutar_rutina(&pasos, "")})))
        }
        _ => Ok(None),
    };
    match r {
        Ok(Some(v)) => v,
        Ok(None) => {
            let mut v = leer();
            v["ok"] = true.into();
            v["acciones"] = acciones();
            v
        }
        Err(e) => json!({"ok": false, "error": e}),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn palmada(m: &mut Vec<i16>) {
        m.extend((0..800).map(|i| if i % 2 == 0 { 20000 } else { -20000 }));
    }
    fn silencio(m: &mut Vec<i16>, s: f64) {
        m.extend((0..(s * RATE as f64) as usize).map(|i| ((i * 7919) % 41) as i16 - 20));
    }

    #[test]
    fn cuenta_palmadas() {
        let mut p = Palmadas::new(8.0);
        let mut m = vec![];
        silencio(&mut m, 1.0);
        for _ in 0..3 {
            palmada(&mut m);
            silencio(&mut m, 0.35);
        }
        silencio(&mut m, 1.5);
        let mut total = None;
        for c in m.chunks(480) {
            if let Some(n) = p.alimentar(c) {
                total = Some(n);
            }
        }
        assert_eq!(total, Some(3));
        // una sola palmada no dispara nada
        let mut p = Palmadas::new(8.0);
        let mut m = vec![];
        silencio(&mut m, 1.0);
        palmada(&mut m);
        silencio(&mut m, 2.0);
        assert!(m.chunks(480).filter_map(|c| p.alimentar(c)).next().is_none());
    }

    #[test]
    fn activadores_editables() {
        let c = por_defecto();
        let c = agregar(c, "frase", vec!["despertar".into(), "di:Hola jefe".into(), "abre:youtube.com".into()], "Jarvis a trabajar", 2, false).unwrap();
        assert!(c["activadores"].as_array().unwrap().iter().any(|a| a["id"] == "jarvis-a-trabajar"));
        assert!(agregar(por_defecto(), "frase", vec!["volar".into()], "dos palabras", 2, false).is_err());
        assert!(agregar(por_defecto(), "palmadas", vec!["despertar".into()], "", 7, false).is_err());
        let c = editar(c, "palmadas", &json!({"veces": 3})).unwrap();
        assert_eq!(elegir(&c, None, Some(3), None).unwrap()["id"], "palmadas");
        assert_eq!(elegir(&c, Some("buenos días señoritas"), None, None).unwrap()["id"], "buenos-dias");
        assert!(elegir(&c, Some("buenas noches"), None, Some(false)).is_none()); // solo el dueño
        assert!(!paso_valido("abre:no es sitio"));
    }

    #[test]
    fn saludo_y_wol() {
        assert_eq!(saludo("Davis", Some(9), None), "Buenos días, Davis. Todo listo.");
        assert_eq!(saludo("", Some(22), Some("{saludo}, {nombre}. ¿Qué construimos?")), "Buenas noches. ¿Qué construimos?");
        assert_eq!(saludo("Ana", Some(15), Some("{saludo}, {nombre}.")), "Buenas tardes, Ana.");
        let p = paquete_magico("aa:bb:cc:dd:ee:ff").unwrap();
        assert_eq!((p.len(), &p[..7]), (102, &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xaa][..]));
        assert!(paquete_magico("zz").is_err());
    }
}
