//! ¿En qué equipo corre y qué tiene? RAM, CPU, GPU, micrófono, cámara, red y discos para instalar.

use crate::util::{leer, sh, sh_tiempo};
use regex::Regex;
use serde_json::{json, Value};
use std::path::Path;

pub fn tipo_equipo() -> String {
    let modelo = leer("/proc/device-tree/model");
    if modelo.contains("Raspberry Pi") {
        return "raspberry-pi".into();
    }
    if modelo.contains("Jetson") || Path::new("/etc/nv_tegra_release").exists() {
        return "jetson".into();
    }
    let virt = sh(&["systemd-detect-virt"]).trim().to_string();
    if !virt.is_empty() && virt != "none" {
        return "maquina-virtual".into();
    }
    match leer("/sys/class/dmi/id/chassis_type").as_str() {
        "8" | "9" | "10" | "14" | "31" => "notebook",
        "3" | "4" | "6" | "7" => "torre",
        "13" => "todo-en-uno",
        "35" => "mini-pc",
        _ => "pc",
    }
    .into()
}

pub fn ram_gb() -> f64 {
    let t = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    Regex::new(r"MemTotal:\s+(\d+)").unwrap().captures(&t).and_then(|c| c[1].parse::<f64>().ok()).map(|k| crate::util::redondear(k / 1024.0 / 1024.0, 1)).unwrap_or(0.0)
}

/// Discos donde se puede instalar (sin el pendrive del que se arrancó, ni loops ni CD, ≥ 16 GB).
pub fn discos_desde(lsblk_json: &str, medio: &str) -> Vec<Value> {
    let Ok(d) = serde_json::from_str::<Value>(lsblk_json) else { return vec![] };
    let mut out = vec![];
    for x in d["blockdevices"].as_array().into_iter().flatten() {
        let tam = x["size"].as_u64().unwrap_or(0);
        if x["type"] != "disk" || x["path"] == medio || tam < 16_000_000_000 {
            continue;
        }
        let mut partes: Vec<Value> = x["children"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|c| c["type"] == "part")
            .map(|c| {
                let nombre = c["label"].as_str().or(c["fstype"].as_str()).or(c["name"].as_str()).unwrap_or("");
                json!({"nombre": nombre, "gb": (c["size"].as_u64().unwrap_or(0) as f64 / 1e9).round() as u64, "fs": c["fstype"].as_str().unwrap_or("")})
            })
            .collect();
        let usado: u64 = partes.iter().map(|p| p["gb"].as_u64().unwrap_or(0)).sum();
        let total = (tam as f64 / 1e9).round() as u64;
        if total.saturating_sub(usado) > 1 {
            partes.push(json!({"nombre": "libre", "gb": total - usado, "fs": ""}));
        }
        let windows = partes.iter().any(|p| p["fs"] == "ntfs");
        out.push(json!({"nombre": x["name"], "ruta": x["path"], "gb": total, "modelo": x["model"].as_str().unwrap_or("").trim(),
                        "bus": x["tran"].as_str().unwrap_or(""), "extraible": x["rm"].as_bool().unwrap_or(false), "partes": partes, "windows": windows}));
    }
    out
}

pub fn discos() -> Vec<Value> {
    let mut medio = String::new();
    for m in ["/cdrom", "/run/live/medium"] {
        let src = sh(&["findmnt", "-no", "SOURCE", m]).trim().to_string();
        if !src.is_empty() {
            medio = if src.starts_with("/dev/sr") { src } else { Regex::new(r"p?\d+$").unwrap().replace(&src, "").into_owned() };
        }
    }
    discos_desde(&sh(&["lsblk", "-J", "-b", "-o", "NAME,PATH,SIZE,TYPE,MODEL,TRAN,RM,FSTYPE,LABEL,MOUNTPOINT,PTTYPE"]), &medio)
}

pub fn equipo() -> Value {
    let (mut gpu, mut vram) = (String::new(), 0u64);
    if let Some(l) = sh_tiempo(&["nvidia-smi", "--query-gpu=name,memory.total", "--format=csv,noheader,nounits"], 5).lines().next() {
        if let Some((n, m)) = l.rsplit_once(',') {
            gpu = n.trim().into();
            vram = (m.trim().parse::<f64>().unwrap_or(0.0) / 1024.0).round() as u64;
        }
    }
    let drv = crate::controladores::detectar();
    if gpu.is_empty() {
        gpu = drv["gpus"].as_array().into_iter().flatten().map(|x| format!("{} {}", x["fabricante"].as_str().unwrap_or(""), x["modelo"].as_str().unwrap_or("")).trim().to_string()).collect::<Vec<_>>().join(" + ").chars().take(80).collect();
    }
    let mic = Regex::new(r"(?m)^card \d").unwrap().is_match(&sh_tiempo(&["arecord", "-l"], 5));
    let cam = std::fs::read_dir("/dev").map(|d| d.flatten().any(|e| e.file_name().to_string_lossy().starts_with("video"))).unwrap_or(false);
    let mut red = "sin conexión".to_string();
    for e in std::fs::read_dir("/sys/class/net").into_iter().flatten().flatten() {
        if e.file_name() != "lo" && leer(&format!("{}/operstate", e.path().display())) == "up" {
            red = if e.path().join("wireless").exists() { "wifi".into() } else { "ethernet".into() };
            break;
        }
    }
    json!({"ram_gb": ram_gb(), "hilos": std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1),
           "cpu": drv["cpu"]["modelo"], "gpu": if gpu.is_empty() { "integrada / no informada".to_string() } else { gpu },
           "controladores": drv, "vram_gb": vram, "avx2": drv["cpu"]["avx2"], "tipo": tipo_equipo(),
           "uefi": Path::new("/sys/firmware/efi").is_dir(), "microfono": mic, "camara": cam, "red": red, "discos": discos()})
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn discos_utiles_y_espacio_libre() {
        let lsblk = r#"{"blockdevices":[
          {"name":"sda","path":"/dev/sda","size":512110190592,"type":"disk","model":"Samsung SSD ","tran":"sata","rm":false,
           "children":[{"name":"sda1","size":104857600,"type":"part","fstype":"vfat","label":null},
                       {"name":"sda2","size":300000000000,"type":"part","fstype":"ntfs","label":"Windows"}]},
          {"name":"sdb","path":"/dev/sdb","size":16000000000,"type":"disk","model":"pendrive","rm":true},
          {"name":"loop0","path":"/dev/loop0","size":20000000000,"type":"loop"}]}"#;
        let d = discos_desde(lsblk, "/dev/sdb");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0]["gb"], 512);
        assert_eq!(d[0]["windows"], true);
        assert_eq!(d[0]["modelo"], "Samsung SSD");
        let partes = d[0]["partes"].as_array().unwrap();
        assert_eq!(partes.last().unwrap()["nombre"], "libre");
        assert_eq!(partes[1]["nombre"], "Windows");
    }
}
