//! Controladores: reconoce el procesador y las gráficas (ID PCI: 10de NVIDIA, 1002 AMD, 8086 Intel) y arma el
//! plan de drivers. Mismo comportamiento que `orion_core.controladores`.

use crate::util::sh;
use regex::Regex;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;

fn fabricante(ven: &str) -> String {
    match ven {
        "10de" => "NVIDIA",
        "1002" | "1022" => "AMD",
        "8086" => "Intel",
        "1234" | "1b36" => "QEMU",
        "15ad" => "VMware",
        "80ee" => "VirtualBox",
        "1af4" => "virtio",
        otro => return otro.to_string(),
    }
    .to_string()
}
const VIRTUALES: [&str; 4] = ["QEMU", "VMware", "VirtualBox", "virtio"];

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct Cpu {
    pub fabricante: String,
    pub modelo: String,
    pub nucleos: Option<usize>,
    pub avx2: bool,
    pub avx512: bool,
    pub microcodigo: Option<String>,
}

pub fn cpu(cpuinfo: &str) -> Cpu {
    let cap = |pat: &str| Regex::new(pat).unwrap().captures(cpuinfo).and_then(|c| c.get(1)).map(|m| m.as_str().trim().to_string()).unwrap_or_default();
    let vendor = cap(r"vendor_id\s*:\s*(\S+)");
    let modelo = Regex::new(r"\s+").unwrap().replace_all(&cap(r"model name\s*:\s*(.+)"), " ").into_owned();
    let fab = if vendor.contains("Intel") { "Intel".into() } else if vendor.contains("AMD") { "AMD".into() } else if vendor.is_empty() { "desconocido".into() } else { vendor };
    let flags = cap(r"(?m)^flags\s*:(.*)$");
    let f: BTreeSet<&str> = flags.split_whitespace().collect();
    let n = cpuinfo.matches("processor\t:").count();
    Cpu {
        microcodigo: match fab.as_str() { "Intel" => Some("intel-microcode".into()), "AMD" => Some("amd64-microcode".into()), _ => None },
        fabricante: fab,
        modelo,
        nucleos: if n > 0 { Some(n) } else { None },
        avx2: f.contains("avx2"),
        avx512: f.iter().any(|x| x.starts_with("avx512")),
    }
}

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct Gpu {
    pub pci: String,
    pub fabricante: String,
    pub id: String,
    pub modelo: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,
}

/// Gráficas desde `lspci -nn` (clase 0300 VGA, 0302 3D, 0380 display).
pub fn gpus(lspci_nn: &str) -> Vec<Gpu> {
    let clase = Regex::new(r"\[(0300|0302|0380)\]").unwrap();
    let ids = Regex::new(r"\[([0-9a-f]{4}):([0-9a-f]{4})\]").unwrap();
    let cola = Regex::new(r"\s*\[[0-9a-f]{4}:[0-9a-f]{4}\].*$").unwrap();
    let rev = Regex::new(r"\s*\(rev [0-9a-f]+\)").unwrap();
    lspci_nn
        .lines()
        .filter(|l| clase.is_match(l))
        .filter_map(|l| {
            let (ven, dev) = ids.captures_iter(l).last().map(|c| (c[1].to_string(), c[2].to_string()))?;
            let tras = l.splitn(2, ": ").last().unwrap_or("");
            let nombre = rev.replace_all(&cola.replace(tras, ""), "").trim().to_string();
            Some(Gpu { pci: l.split_whitespace().next()?.into(), fabricante: fabricante(&ven), id: format!("{ven}:{dev}"), modelo: nombre, driver: None })
        })
        .collect()
}

pub fn driver_en_uso(pci: &str) -> String {
    Regex::new(r"Kernel driver in use:\s*(\S+)").unwrap().captures(&sh(&["lspci", "-k", "-s", pci])).map(|c| c[1].to_string()).unwrap_or_default()
}

/// Driver NVIDIA que Ubuntu recomienda para este equipo.
pub fn recomendado_nvidia(ubuntu_drivers: &str) -> Option<String> {
    if let Some(c) = Regex::new(r"driver\s*:\s*(nvidia-driver-\d+(?:-open|-server(?:-open)?)?)\s.*recommended").unwrap().captures(ubuntu_drivers) {
        return Some(c[1].into());
    }
    let num = Regex::new(r"\d+").unwrap();
    let mut todos: Vec<String> = Regex::new(r"(nvidia-driver-\d+)\b").unwrap().captures_iter(ubuntu_drivers).map(|c| c[1].to_string()).collect::<BTreeSet<_>>().into_iter().collect();
    todos.sort_by_key(|x| num.find(x).and_then(|m| m.as_str().parse::<u32>().ok()).unwrap_or(0));
    todos.pop()
}

pub fn plan(c: &Cpu, g: &[Gpu], nvidia: Option<&str>, nucleo_nuevo: bool) -> Value {
    let mut paquetes: BTreeSet<String> = BTreeSet::new();
    let (mut pasos, mut notas): (Vec<String>, Vec<String>) = (vec![], vec![]);
    if let Some(m) = &c.microcodigo {
        paquetes.insert(m.clone());
    }
    let fabs: BTreeSet<&str> = g.iter().map(|x| x.fabricante.as_str()).collect();
    if !fabs.is_empty() && fabs.iter().all(|f| VIRTUALES.contains(f)) {
        notas.push("máquina virtual: usa los controladores virtuales del núcleo".into());
    }
    if fabs.contains("NVIDIA") {
        match nvidia {
            Some(n) => {
                paquetes.insert(n.into());
                notas.push(format!("NVIDIA: {n} (propietario, firmado para arranque seguro; habilita CUDA para la IA local)"));
            }
            None => {
                pasos.push("ubuntu-drivers install   # elige el driver NVIDIA recomendado para este modelo".into());
                notas.push("NVIDIA: se elige el driver recomendado al tener internet".into());
            }
        }
    }
    if fabs.contains("AMD") {
        paquetes.extend(["mesa-vulkan-drivers".into(), "mesa-va-drivers".into()]);
        notas.push("AMD: amdgpu del núcleo + Mesa (OpenGL/Vulkan) + firmware; ROCm solo si se pide para IA".into());
    }
    if fabs.contains("Intel") {
        paquetes.extend(["mesa-vulkan-drivers".into(), "intel-media-va-driver".into()]);
        notas.push("Intel: i915/xe del núcleo + Mesa + aceleración de video".into());
    }
    if nucleo_nuevo {
        paquetes.insert("linux-generic-hwe-24.04".into());
        notas.push("núcleo HWE: soporte para hardware más nuevo".into());
    }
    json!({"paquetes": paquetes, "pasos": pasos, "notas": notas})
}

pub fn detectar() -> Value {
    let c = cpu(&std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default());
    let mut g = gpus(&sh(&["lspci", "-nn"]));
    for x in g.iter_mut() {
        x.driver = Some(driver_en_uso(&x.pci));
    }
    let p = plan(&c, &g, None, false);
    json!({"cpu": c, "gpus": g, "plan": p})
}

pub fn revisar_al_dia(ubuntu_drivers: &str, instalado: &str) -> Value {
    let rec = recomendado_nvidia(ubuntu_drivers);
    let num = |s: &str| Regex::new(r"\d+").unwrap().find(s).and_then(|m| m.as_str().parse::<u32>().ok()).unwrap_or(0);
    let nuevo = matches!(&rec, Some(r) if !instalado.is_empty() && num(r) > num(instalado));
    json!({"instalado": if instalado.is_empty() { Value::Null } else { instalado.into() }, "recomendado": rec, "hay_nuevo": nuevo,
           "comando": if nuevo { format!("sudo apt-get install -y {}", rec.clone().unwrap_or_default()) } else { String::new() }})
}

pub fn revisar_al_dia_equipo() -> Value {
    let inst = Regex::new(r"(?m)^ii\s+(nvidia-driver-\d+\S*)").unwrap().captures(&sh(&["dpkg", "-l", "nvidia-driver-*"])).map(|c| c[1].to_string()).unwrap_or_default();
    revisar_al_dia(&crate::util::sh_tiempo(&["ubuntu-drivers", "devices"], 60), &inst)
}

#[cfg(test)]
mod pruebas {
    use super::*;
    const LSPCI: &str = "00:02.0 VGA compatible controller [0300]: Intel Corporation UHD Graphics 630 [8086:3e92] (rev 02)\n\
01:00.0 VGA compatible controller [0300]: NVIDIA Corporation GA106 [GeForce RTX 3060] [10de:2503] (rev a1)\n\
00:1f.3 Audio device [0403]: Intel Corporation Cannon Lake PCH cAVS [8086:a348] (rev 10)";

    #[test]
    fn reconoce_gpus_por_id_pci() {
        let g = gpus(LSPCI);
        assert_eq!(g.len(), 2);
        assert_eq!((g[0].fabricante.as_str(), g[1].fabricante.as_str()), ("Intel", "NVIDIA"));
        assert_eq!(g[1].modelo, "NVIDIA Corporation GA106 [GeForce RTX 3060]");
        assert_eq!(g[1].id, "10de:2503");
    }

    #[test]
    fn cpu_y_plan() {
        let c = cpu("processor\t: 0\nvendor_id\t: GenuineIntel\nmodel name\t: Intel(R) Core(TM) i7-9700  CPU\nflags\t\t: fpu sse avx2\nprocessor\t: 1\n");
        assert_eq!((c.fabricante.as_str(), c.nucleos, c.avx2, c.avx512), ("Intel", Some(2), true, false));
        let p = plan(&c, &gpus(LSPCI), Some("nvidia-driver-570"), false);
        let paq: Vec<&str> = p["paquetes"].as_array().unwrap().iter().map(|x| x.as_str().unwrap()).collect();
        assert!(paq.contains(&"intel-microcode") && paq.contains(&"nvidia-driver-570") && paq.contains(&"intel-media-va-driver"));
        let vm = plan(&c, &gpus("00:01.0 VGA compatible controller [0300]: Device [1234:1111] (rev 02)"), None, false);
        assert!(vm["notas"][0].as_str().unwrap().contains("máquina virtual"));
    }

    #[test]
    fn driver_nvidia_recomendado_y_al_dia() {
        let ud = "driver   : nvidia-driver-535 - distro non-free\ndriver   : nvidia-driver-570 - distro non-free recommended\n";
        assert_eq!(recomendado_nvidia(ud).as_deref(), Some("nvidia-driver-570"));
        assert_eq!(recomendado_nvidia("driver : nvidia-driver-470 - x\ndriver : nvidia-driver-535 - x").as_deref(), Some("nvidia-driver-535"));
        let r = revisar_al_dia(ud, "nvidia-driver-535");
        assert_eq!(r["hay_nuevo"], true);
        assert_eq!(revisar_al_dia(ud, "nvidia-driver-570")["hay_nuevo"], false);
    }
}
