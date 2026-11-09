//! Cámaras y visión (antes orion_vision): Frigate (MIT) detecta personas, autos y mascotas en cámaras IP o USB y
//! avisa a Neo Orión.
//!
//! Neo Orión arma la configuración de Frigate desde una lista simple de cámaras, la levanta con Docker y escucha sus
//! eventos por MQTT. Una «persona» en una zona marcada «robot», «cnc», «taller» o «maquina» le avisa al Guardián:
//! la máquina se frena igual que cuando el LiDAR ve a alguien. Los videos quedan en el equipo.
//!
//! Las direcciones RTSP llevan usuario y clave: la persona las escribe en ~/.orion/claves/camaras.json (permiso
//! 600); en la configuración van como variables {FRIGATE_RTSP_...}, nunca la clave en texto.

use regex::Regex;
use serde_json::{json, Map, Value};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};

pub const OBJETOS: [&str; 4] = ["person", "car", "dog", "cat"];
pub const ZONAS_GUARDIAN: [&str; 4] = ["robot", "cnc", "taller", "maquina"];

pub fn claves_ruta() -> PathBuf {
    crate::util::casa().join(".orion/claves/camaras.json")
}

pub fn carpeta() -> PathBuf {
    crate::util::casa().join(".orion/frigate")
}

pub fn id(nombre: &str) -> String {
    let s = Regex::new(r"[^a-z0-9]+").unwrap().replace_all(&crate::util::sin_tilde(nombre), "_").trim_matches('_').to_string();
    if s.is_empty() { "camara".into() } else { s }
}

fn es_usb(c: &Value) -> bool {
    c["fuente"].as_str().unwrap_or("rtsp") == "usb"
}

/// El YAML de Frigate (sin claves: las URLs RTSP salen de variables de entorno).
pub fn configuracion(camaras: &[Value], detector: &str, mqtt: &str, grabar_dias: u32) -> String {
    let tipo = match detector {
        "intel" => "openvino",
        "coral" => "edgetpu",
        _ => "cpu",
    };
    let mut l: Vec<String> = vec![
        "# Generado por Neo Orión (orion_vision). Las claves RTSP vienen de variables FRIGATE_RTSP_*.".into(),
        "mqtt:".into(),
        format!("  host: {mqtt}"),
        "  topic_prefix: frigate".into(),
        "detectors:".into(),
        "  principal:".into(),
        format!("    type: {tipo}"),
    ];
    if detector == "coral" {
        l.push("    device: usb".into());
    }
    if detector == "intel" {
        l.push("    device: GPU".into());
    }
    l.extend(["record:", "  enabled: true", "  retain:"].map(String::from));
    l.push(format!("    days: {grabar_dias}"));
    l.extend(["    mode: motion", "snapshots:", "  enabled: true", "objects:"].map(String::from));
    l.push(format!("  track: [{}]", OBJETOS.join(", ")));
    l.push("cameras:".into());
    for c in camaras {
        let cid = id(c["nombre"].as_str().unwrap_or(""));
        if es_usb(c) {
            l.extend([format!("  {cid}:"), "    ffmpeg:".into(), "      input_args: -f v4l2 -input_format mjpeg -video_size 1280x720".into(), "      inputs:".into()]);
            l.push(format!("        - path: {}", c["dispositivo"].as_str().unwrap_or("/dev/video0")));
        } else {
            l.extend([format!("  {cid}:"), "    ffmpeg:".into(), "      inputs:".into(), format!("        - path: \"{{FRIGATE_RTSP_{}}}\"", cid.to_uppercase())]);
        }
        l.extend(["          roles: [detect, record]", "    detect:", "      width: 1280", "      height: 720", "      fps: 5"].map(String::from));
        if let Some(z) = c["zonas"].as_object().filter(|z| !z.is_empty()) {
            l.push("    zones:".into());
            for (zn, pts) in z {
                let coords: Vec<String> = pts.as_array().into_iter().flatten().map(|p| format!("{},{}", p[0], p[1])).collect();
                l.extend([format!("      {}:", id(zn)), format!("        coordinates: {}", coords.join(",")), "        objects: [person]".into()]);
            }
        }
    }
    l.join("\n") + "\n"
}

/// FRIGATE_RTSP_<ID> desde ~/.orion/claves/camaras.json ({"Taller": "rtsp://usuario:clave@ip:554/..."}).
pub fn variables_entorno(camaras: &[Value], claves: Option<&Map<String, Value>>) -> Vec<(String, String)> {
    let leidas: Map<String, Value> = match claves {
        Some(c) => c.clone(),
        None => std::fs::read_to_string(claves_ruta()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default(),
    };
    camaras
        .iter()
        .filter(|c| !es_usb(c))
        .filter_map(|c| {
            let n = c["nombre"].as_str()?;
            leidas.get(n).and_then(Value::as_str).map(|u| (format!("FRIGATE_RTSP_{}", id(n).to_uppercase()), u.to_string()))
        })
        .collect()
}

pub fn compose(detector: &str) -> String {
    let disp = match detector {
        "coral" => "      - /dev/bus/usb:/dev/bus/usb",
        "intel" => "      - /dev/dri:/dev/dri",
        _ => "",
    };
    let dispositivos = if disp.is_empty() { String::new() } else { format!("    devices:\n{disp}") };
    format!(
        "services:\n  mqtt:\n    image: eclipse-mosquitto:2\n    restart: unless-stopped\n    ports: [\"127.0.0.1:1883:1883\"]\n    command: mosquitto -c /mosquitto-no-auth.conf\n  frigate:\n    image: ghcr.io/blakeblackshear/frigate:stable\n    restart: unless-stopped\n    shm_size: \"256mb\"\n    env_file: [.env]\n    volumes:\n      - ./config:/config\n      - ./media:/media/frigate\n      - /etc/localtime:/etc/localtime:ro\n{dispositivos}\n    ports: [\"127.0.0.1:5000:5000\"]\n"
    )
}

pub fn preparar(camaras: &[Value], detector: &str, dir: &Path) -> Value {
    let r = (|| -> std::io::Result<Value> {
        std::fs::create_dir_all(dir.join("config"))?;
        std::fs::write(dir.join("config/config.yml"), configuracion(camaras, detector, "mqtt", 7))?;
        std::fs::write(dir.join("docker-compose.yml"), compose(detector))?;
        let vars = variables_entorno(camaras, None);
        let env = dir.join(".env");
        std::fs::write(&env, vars.iter().map(|(k, v)| format!("{k}={v}\n")).collect::<String>())?;
        std::fs::set_permissions(&env, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
        let faltan: Vec<&str> = camaras.iter().filter(|c| !es_usb(c)).filter_map(|c| c["nombre"].as_str()).filter(|n| !vars.iter().any(|(k, _)| *k == format!("FRIGATE_RTSP_{}", id(n).to_uppercase()))).collect();
        Ok(json!({"ok": true, "carpeta": dir, "levantar": format!("cd {} && sudo docker compose up -d", dir.display()), "ver": "http://127.0.0.1:5000", "faltan_claves": faltan}))
    })();
    r.unwrap_or_else(|e| json!({"ok": false, "error": e.to_string()}))
}

/// Mensaje MQTT frigate/events → orden para el Guardián si hay una persona en una zona de máquina.
pub fn evento_guardian(ev: &Value) -> Option<Value> {
    let d = if ev["after"].is_object() { &ev["after"] } else { ev };
    if d["label"] != "person" {
        return None;
    }
    let zona = d["current_zones"].as_array()?.iter().filter_map(Value::as_str).find(|z| ZONAS_GUARDIAN.iter().any(|k| z.contains(k)))?;
    let conf = d["top_score"].as_f64().filter(|x| *x != 0.0).or(d["score"].as_f64()).unwrap_or(0.0);
    Some(json!({"accion": "frenar", "motivo": format!("persona en la zona {zona} (cámara {})", d["camera"].as_str().unwrap_or("?")), "confianza": (conf * 100.0).round() / 100.0}))
}

// ------------------------------------------------------------------ MQTT mínimo (3.1.1, QoS 0) para escuchar a Frigate

fn largo_restante(mut n: usize) -> Vec<u8> {
    let mut v = vec![];
    loop {
        let mut b = (n % 128) as u8;
        n /= 128;
        if n > 0 {
            b |= 0x80;
        }
        v.push(b);
        if n == 0 {
            return v;
        }
    }
}

fn texto(s: &str) -> Vec<u8> {
    let mut v = (s.len() as u16).to_be_bytes().to_vec();
    v.extend(s.as_bytes());
    v
}

pub fn paquete_connect(cliente: &str) -> Vec<u8> {
    let mut cuerpo = texto("MQTT");
    cuerpo.extend([4, 0x02, 0, 60]); // nivel 3.1.1, sesión limpia, keepalive 60 s
    cuerpo.extend(texto(cliente));
    let mut p = vec![0x10];
    p.extend(largo_restante(cuerpo.len()));
    p.extend(cuerpo);
    p
}

pub fn paquete_subscribe(tema: &str) -> Vec<u8> {
    let mut cuerpo = vec![0, 1];
    cuerpo.extend(texto(tema));
    cuerpo.push(0);
    let mut p = vec![0x82];
    p.extend(largo_restante(cuerpo.len()));
    p.extend(cuerpo);
    p
}

/// Lee un paquete: (tipo, cuerpo).
pub fn leer_paquete(r: &mut impl Read) -> std::io::Result<(u8, Vec<u8>)> {
    let mut b = [0u8; 1];
    r.read_exact(&mut b)?;
    let tipo = b[0];
    let (mut n, mut mult) = (0usize, 1usize);
    loop {
        let mut x = [0u8; 1];
        r.read_exact(&mut x)?;
        n += (x[0] & 0x7f) as usize * mult;
        mult *= 128;
        if x[0] & 0x80 == 0 {
            break;
        }
    }
    let mut c = vec![0u8; n];
    r.read_exact(&mut c)?;
    Ok((tipo, c))
}

/// De un PUBLISH (QoS 0): (tema, mensaje).
pub fn publicacion(cuerpo: &[u8]) -> Option<(String, Vec<u8>)> {
    let n = u16::from_be_bytes([*cuerpo.first()?, *cuerpo.get(1)?]) as usize;
    Some((String::from_utf8_lossy(cuerpo.get(2..2 + n)?).into(), cuerpo.get(2 + n..)?.to_vec()))
}

/// Bucle MQTT: al_frenar(orden) con cada persona en zona de máquina.
pub fn escuchar(host: &str, mut al_frenar: impl FnMut(Value)) -> std::io::Result<()> {
    let mut s = TcpStream::connect((host, 1883))?;
    s.write_all(&paquete_connect("neo-orion"))?;
    leer_paquete(&mut s)?; // CONNACK
    s.write_all(&paquete_subscribe("frigate/events"))?;
    s.set_read_timeout(Some(std::time::Duration::from_secs(50)))?;
    loop {
        match leer_paquete(&mut s) {
            Ok((t, c)) if t >> 4 == 3 => {
                if let Some(o) = publicacion(&c).and_then(|(_, m)| serde_json::from_slice::<Value>(&m).ok()).and_then(|e| evento_guardian(&e)) {
                    al_frenar(o);
                }
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => s.write_all(&[0xC0, 0])?, // PINGREQ
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn frigate_sin_claves_en_texto() {
        let cams = vec![json!({"nombre": "Taller CNC", "fuente": "usb"}), json!({"nombre": "Patio", "zonas": {"robot": [[0, 0], [10, 0], [10, 10]]}})];
        let y = configuracion(&cams, "cpu", "127.0.0.1", 7);
        assert!(y.contains("  patio:\n    ffmpeg:\n      inputs:\n        - path: \"{FRIGATE_RTSP_PATIO}\""));
        assert!(y.contains("  taller_cnc:") && y.contains("        - path: /dev/video0") && y.contains("        coordinates: 0,0,10,0,10,10"));
        let mut cl = Map::new();
        cl.insert("Patio".into(), json!("rtsp://u:clave@10.0.0.5/s"));
        assert_eq!(variables_entorno(&cams, Some(&cl)), vec![("FRIGATE_RTSP_PATIO".to_string(), "rtsp://u:clave@10.0.0.5/s".to_string())]);
        assert!(!y.contains("u:clave@"));
        let d = tempfile::tempdir().unwrap();
        let r = preparar(&cams, "coral", d.path());
        assert_eq!(r["faltan_claves"], json!(["Patio"]));
        assert!(std::fs::read_to_string(d.path().join("docker-compose.yml")).unwrap().contains("    devices:\n      - /dev/bus/usb:/dev/bus/usb"));
    }

    #[test]
    fn evento_frena_guardian_y_mqtt() {
        let ev = json!({"after": {"label": "person", "current_zones": ["robot"], "camera": "patio", "top_score": 0.91}});
        assert_eq!(evento_guardian(&ev).unwrap()["accion"], "frenar");
        assert!(evento_guardian(&json!({"after": {"label": "person", "current_zones": ["jardin"]}})).is_none());
        assert!(evento_guardian(&json!({"after": {"label": "dog", "current_zones": ["robot"]}})).is_none());
        // un PUBLISH armado a mano se lee igual que lo mandaría mosquitto
        let msg = ev.to_string();
        let mut cuerpo = texto("frigate/events");
        cuerpo.extend(msg.as_bytes());
        let mut p = vec![0x30];
        p.extend(largo_restante(cuerpo.len()));
        p.extend(&cuerpo);
        let (t, c) = leer_paquete(&mut &p[..]).unwrap();
        assert_eq!(t >> 4, 3);
        let (tema, m) = publicacion(&c).unwrap();
        assert_eq!(tema, "frigate/events");
        assert_eq!(serde_json::from_slice::<Value>(&m).unwrap(), ev);
        assert_eq!(largo_restante(321), vec![0xC1, 0x02]);
        assert_eq!(&paquete_connect("x")[..2], &[0x10, 13]);
    }
}
