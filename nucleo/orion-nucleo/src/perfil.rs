//! ¿Dónde estoy corriendo, qué tengo conectado y qué alternativa uso en cada pieza? (antes orion_core.detect y
//! orion_core.perfil).
//!
//!  · HOST — el equipo donde corre Neo Orión (torre, notebook, Raspberry, Jetson, Android con Termux, VM…)
//!  · PERIFÉRICOS — lo que el host ve conectado: ESP32/Arduino por USB, cámaras, teléfonos por adb, micrófonos
//!  · NIVEL — micro, patata, ligero, medio, potente, estacion; para cada pieza (oído, voz, memoria, 3D…) se elige
//!    la opción más capaz que corre fluida. El catálogo está en config/perfil-catalogo.json; la persona puede
//!    forzar cualquier elección en ~/.orion/backends.json.

use crate::util::{existe_programa, leer, sh};
use regex::Regex;
use serde_json::{json, Map, Value};
use std::sync::LazyLock;

static DATOS: LazyLock<Value> = LazyLock::new(|| serde_json::from_str(include_str!("../../../config/perfil-catalogo.json")).expect("perfil-catalogo.json"));

pub fn niveles() -> Vec<&'static str> {
    DATOS["niveles"].as_array().unwrap().iter().filter_map(Value::as_str).collect()
}

fn leer_sys(p: &str) -> String {
    std::fs::read(p).map(|b| String::from_utf8_lossy(&b).trim_matches(|c| c == '\0' || c == ' ' || c == '\n').to_string()).unwrap_or_default()
}

pub fn host() -> Value {
    let modelo = leer_sys("/proc/device-tree/model");
    let kernel = sh(&["uname", "-r"]).trim().to_string();
    let mut h = json!({"arch": std::env::consts::ARCH, "kernel": kernel, "model": modelo, "tipo": "pc"});
    let tipo = if modelo.contains("Raspberry Pi") {
        "raspberry-pi".to_string()
    } else if modelo.contains("Jetson") || std::path::Path::new("/etc/nv_tegra_release").exists() {
        "jetson".into()
    } else if std::env::var_os("TERMUX_VERSION").is_some() || std::path::Path::new("/data/data/com.termux").is_dir() {
        "android-termux".into()
    } else if kernel.to_lowercase().contains("microsoft") {
        "wsl".into()
    } else {
        let virt = sh(&["systemd-detect-virt"]).trim().to_string();
        if !virt.is_empty() && virt != "none" {
            h["virtualizacion"] = json!(virt);
            "maquina-virtual".into()
        } else if std::path::Path::new("/.dockerenv").exists() {
            "contenedor".into()
        } else {
            match leer_sys("/sys/class/dmi/id/chassis_type").as_str() {
                "8" | "9" | "10" | "14" | "31" => "notebook",
                "3" | "4" | "6" | "7" => "torre",
                "13" => "todo-en-uno",
                "35" => "mini-pc",
                _ => "pc",
            }
            .into()
        }
    };
    h["tipo"] = json!(tipo);
    if let Some(c) = Regex::new(r"MemTotal:\s+(\d+)").unwrap().captures(&leer("/proc/meminfo")) {
        h["ram_gb"] = json!(crate::util::redondear(c[1].parse::<f64>().unwrap_or(0.0) / 1024.0 / 1024.0, 1));
    }
    h["gpu_nvidia"] = json!(existe_programa("nvidia-smi") && !sh(&["nvidia-smi", "-L"]).trim().is_empty());
    h["pantalla"] = json!(std::env::var_os("DISPLAY").is_some() || std::env::var_os("WAYLAND_DISPLAY").is_some());
    let cpu = leer("/proc/cpuinfo");
    h["avx2"] = json!(match Regex::new(r"(?m)^flags\s*:(.*)$").unwrap().captures(&cpu) {
        Some(f) => f[1].split_whitespace().any(|x| x == "avx2"),
        None => cpu.is_empty(), // sin datos: no castigar
    });
    let ram = h["ram_gb"].as_f64().unwrap_or(0.0);
    h["rol_sugerido"] = json!(if (tipo == "raspberry-pi" || tipo == "jetson") && ram <= 8.0 { "nodo-robot" } else { "central" });
    h
}

fn listar(patron_dir: &str, prefijo: &str) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(patron_dir).into_iter().flatten().flatten().map(|e| e.path().to_string_lossy().to_string()).filter(|p| p.rsplit('/').next().is_some_and(|n| n.starts_with(prefijo))).collect();
    v.sort();
    v
}

pub fn perifericos() -> Vec<Value> {
    let usb = &DATOS["usb"];
    let mut out = vec![];
    for dev in listar("/sys/bus/usb/devices", "") {
        let (vid, pid) = (leer_sys(&format!("{dev}/idVendor")), leer_sys(&format!("{dev}/idProduct")));
        if vid.is_empty() {
            continue;
        }
        let k = format!("{vid}:{pid}");
        let nombre = usb[&k].as_str().map(String::from).or_else(|| (vid == "303a").then(|| "placa Espressif (ESP32)".to_string()));
        if let Some(n) = nombre {
            out.push(json!({"tipo": "usb", "id": k, "que_es": n, "producto": leer_sys(&format!("{dev}/product"))}));
        }
    }
    let mut ttys = listar("/dev", "ttyUSB");
    ttys.extend(listar("/dev", "ttyACM"));
    ttys.sort();
    for t in ttys {
        out.push(json!({"tipo": "serie", "dispositivo": t, "que_es": "puerto serie (microcontrolador / servos)"}));
    }
    for cam in listar("/dev", "video") {
        let n = leer_sys(&format!("/sys/class/video4linux/{}/name", cam.rsplit('/').next().unwrap()));
        out.push(json!({"tipo": "camara", "dispositivo": cam, "que_es": if n.is_empty() { "cámara".to_string() } else { n }}));
    }
    if existe_programa("adb") {
        for l in sh(&["adb", "devices", "-l"]).lines().skip(1).filter(|l| l.contains("device")) {
            out.push(json!({"tipo": "android", "que_es": "teléfono Android", "detalle": l.trim()}));
        }
    }
    if existe_programa("arecord") {
        for l in sh(&["arecord", "-l"]).lines().filter(|l| l.starts_with("card")) {
            out.push(json!({"tipo": "microfono", "que_es": l.split_once(':').map(|x| x.1.trim()).unwrap_or("")}));
        }
    }
    out
}

pub fn escanear() -> Value {
    json!({"host": host(), "perifericos": perifericos()})
}

pub fn nivel(h: &Value) -> String {
    let ram = h["ram_gb"].as_f64().unwrap_or(0.0);
    let tipo = h["tipo"].as_str().unwrap_or("");
    let vram = if h["gpu_nvidia"] == true {
        sh(&["nvidia-smi", "--query-gpu=memory.total", "--format=csv,noheader,nounits"]).split_whitespace().filter_map(|x| x.parse::<f64>().ok()).fold(0.0, f64::max) / 1024.0
    } else {
        0.0
    };
    if vram >= 20.0 {
        "estacion"
    } else if vram >= 8.0 {
        "potente"
    } else if (tipo == "raspberry-pi" || tipo == "android-termux") && ram <= 4.5 {
        "micro"
    } else if ram <= 4.5 || h["avx2"] == false {
        "patata"
    } else if ram <= 8.5 || tipo == "raspberry-pi" || tipo == "jetson" {
        "ligero"
    } else {
        "medio"
    }
    .into()
}

pub fn elegir(pieza: &str, nv: &str, forzado: &Map<String, Value>) -> Value {
    if let Some(f) = forzado.get(pieza) {
        return json!({"id": f, "lic": "?", "nota": "elegido por la persona"});
    }
    let cat = &DATOS["catalogo"][pieza];
    if let Some(t) = cat["_todos"].as_array() {
        return t.first().cloned().unwrap_or(Value::Null);
    }
    let n = niveles();
    let idx = n.iter().position(|x| *x == nv).unwrap_or(0);
    // si el nivel no tiene opción, baja un nivel
    for x in n[..=idx].iter().rev() {
        if let Some(o) = cat[*x].as_array().and_then(|a| a.first()) {
            return o.clone();
        }
    }
    cat["_nube"].as_array().and_then(|a| a.first()).cloned().unwrap_or(Value::Null)
}

pub fn perfil_con(forzado: &Map<String, Value>, h: &Value) -> Value {
    let nv = forzado.get("nivel").and_then(Value::as_str).map(String::from).unwrap_or_else(|| nivel(h));
    let mut eleccion = Map::new();
    let mut alternativas = Map::new();
    for p in DATOS["piezas"].as_array().unwrap().iter().filter_map(Value::as_str) {
        eleccion.insert(p.into(), elegir(p, &nv, forzado));
        let c = &DATOS["catalogo"][p];
        alternativas.insert(p.into(), c.get(&nv).or_else(|| c.get("_todos")).cloned().unwrap_or(json!([])));
    }
    json!({"nivel": nv, "host": h["tipo"], "ram_gb": h["ram_gb"], "eleccion": eleccion, "alternativas": alternativas})
}

pub fn perfil() -> Value {
    let forzado: Map<String, Value> = std::fs::read_to_string(crate::util::casa().join(".orion/backends.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    perfil_con(&forzado, &host())
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn nivel_y_eleccion_como_python() {
        let pc = |ram: f64, tipo: &str| json!({"ram_gb": ram, "tipo": tipo, "gpu_nvidia": false, "avx2": true});
        assert_eq!(nivel(&pc(2.0, "raspberry-pi")), "micro");
        assert_eq!(nivel(&pc(4.0, "notebook")), "patata");
        assert_eq!(nivel(&json!({"ram_gb": 16.0, "tipo": "torre", "avx2": false})), "patata");
        assert_eq!(nivel(&pc(8.0, "notebook")), "ligero");
        assert_eq!(nivel(&pc(16.0, "torre")), "medio");
        let vacio = Map::new();
        assert_eq!(elegir("oido", "micro", &vacio)["id"], "vosk-small-es");
        // patata no tiene oído propio: baja a micro
        assert_eq!(elegir("oido", "patata", &vacio)["id"], "vosk-small-es");
        assert_eq!(elegir("memoria_app", "medio", &vacio)["id"], "carpeta-markdown");
        let mut f = Map::new();
        f.insert("voz".into(), json!("piper"));
        f.insert("nivel".into(), json!("potente"));
        let p = perfil_con(&f, &pc(16.0, "torre"));
        assert_eq!(p["nivel"], "potente");
        assert_eq!(p["eleccion"]["voz"]["nota"], "elegido por la persona");
        assert_eq!(p["eleccion"].as_object().unwrap().len(), 10);
        assert!(host()["ram_gb"].as_f64().unwrap() > 0.0);
    }
}
