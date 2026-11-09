//! orion-fit (antes un guion Python): escanea el hardware y dice cuál es la IA local más grande que este equipo
//! puede correr bien (LLM + reconocimiento de voz).
//!
//!   orion-nucleo fit              informe con la estética del HUD
//!   orion-nucleo fit --json       para que la IA / el HUD lo consuman
//!   orion-nucleo fit --ram 16     simular otro equipo (GB); --vram 8 (0 = sin GPU)
//!   orion-nucleo fit --install    descarga con Ollama el modelo recomendado
//!   orion-nucleo fit --sin-filtro recomienda entre los modelos sin filtro
//!   orion-nucleo fit --actualizar revisa el catálogo en vivo de Ollama y avisa de familias nuevas
//!
//! Fuente principal: llmfit, si está instalado (catálogo al día). Respaldo: models-fallback.json.

use crate::util::{correr, existe_programa, leer, redondear, sh};
use regex::Regex;
use serde_json::{json, Value};

const OS_RESERVA_GB: f64 = 2.0; // sistema + HUD
const VOZ_RESERVA_GB: f64 = 1.0; // micrófono siempre activo + voz
const SEGURIDAD: f64 = 0.90; // no llenar la memoria al 100 %
const CPU_TOPE_GB: f64 = 6.5; // en CPU, sobre esto ya no es conversación fluida
const CATALOGO_INCLUIDO: &str = include_str!("../../../overlay/usr/share/orion/models-fallback.json");

pub fn catalogo() -> Value {
    ["/usr/share/orion/models-fallback.json"].iter().find_map(|p| std::fs::read_to_string(p).ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(|| serde_json::from_str(CATALOGO_INCLUIDO).unwrap())
}

fn libre_gb(ruta: &std::path::Path) -> f64 {
    let mut p = ruta.to_path_buf();
    while !p.exists() {
        if !p.pop() {
            break;
        }
    }
    let o = sh(&["df", "-B1", "--output=avail", &p.to_string_lossy()]);
    o.lines().nth(1).and_then(|l| l.trim().parse::<f64>().ok()).map(|b| redondear(b / 1024f64.powi(3), 1)).unwrap_or(0.0)
}

pub fn detectar(ram: Option<f64>, vram: Option<f64>) -> Value {
    let arch = std::env::consts::ARCH;
    let nucleos = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    let mem = Regex::new(r"MemTotal:\s+(\d+)").unwrap().captures(&leer("/proc/meminfo")).and_then(|c| c[1].parse::<f64>().ok()).map(|k| k / 1024.0 / 1024.0);
    let modelo = ["/proc/device-tree/model", "/sys/firmware/devicetree/base/model"].iter().find_map(|p| std::fs::read(p).ok()).map(|b| String::from_utf8_lossy(&b).trim_matches(|c| c == '\0' || c == ' ' || c == '\n').to_string());
    let cpuinfo = leer("/proc/cpuinfo");
    let cpu = Regex::new(r"model name\s*:\s*(.+)").unwrap().captures(&cpuinfo).map(|c| c[1].trim().to_string()).unwrap_or_else(|| arch.into());
    let avx2 = Regex::new(r"(?m)^flags\s*:(.*)$").unwrap().captures(&cpuinfo).is_some_and(|f| f[1].split_whitespace().any(|x| x == "avx2"));
    let mut gpus: Vec<Value> = vec![];
    for l in sh(&["nvidia-smi", "--query-gpu=name,memory.total", "--format=csv,noheader,nounits"]).trim().lines() {
        if let Some((n, m)) = l.rsplit_once(',') {
            gpus.push(json!({"vendor": "nvidia", "name": n.trim(), "vram_gb": redondear(m.trim().parse::<f64>().unwrap_or(0.0) / 1024.0, 1)}));
        }
    }
    if gpus.is_empty() {
        // AMD vía sysfs
        let mut cartas: Vec<String> = std::fs::read_dir("/sys/class/drm").into_iter().flatten().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
        cartas.sort();
        for c in cartas.iter().filter(|c| Regex::new(r"^card\d+$").unwrap().is_match(c)) {
            if let Ok(t) = std::fs::read_to_string(format!("/sys/class/drm/{c}/device/mem_info_vram_total")) {
                gpus.push(json!({"vendor": "amd", "name": c, "vram_gb": redondear(t.trim().parse::<f64>().unwrap_or(0.0) / 1024f64.powi(3), 1)}));
            }
        }
    }
    if let Some(v) = vram {
        gpus = if v > 0.0 { vec![json!({"vendor": "sim", "name": "simulada", "vram_gb": v})] } else { vec![] };
    }
    let vram_gb = gpus.iter().filter_map(|g| g["vram_gb"].as_f64()).fold(0.0, f64::max);
    let placa = modelo.filter(|m| !m.is_empty()).unwrap_or_else(|| sh(&["uname", "-n"]).trim().to_string());
    json!({"arch": arch, "cpu_cores": nucleos, "ram_gb": redondear(ram.or(mem).unwrap_or(0.0), 1), "board": placa, "cpu": cpu, "avx2": avx2,
           "gpus": gpus, "vram_gb": vram_gb, "disk_free_gb": libre_gb(&crate::util::casa().join(".ollama"))})
}

/// Memoria realmente usable por el LLM: (rápida, máxima, modo).
pub fn presupuesto(hw: &Value) -> (f64, f64, &'static str) {
    let ram = hw["ram_gb"].as_f64().unwrap_or(0.0);
    let vram = hw["vram_gb"].as_f64().unwrap_or(0.0);
    if vram >= 4.0 {
        let rapida = vram * SEGURIDAD;
        let lenta = rapida + (ram - OS_RESERVA_GB - VOZ_RESERVA_GB).max(0.0) * 0.6;
        return (redondear(rapida, 1), redondear(lenta, 1), "GPU");
    }
    let reserva = if ram > 4.5 { OS_RESERVA_GB + VOZ_RESERVA_GB } else { 0.9 };
    let usable = (ram - reserva).max(0.0) * SEGURIDAD;
    (redondear(usable.min(CPU_TOPE_GB), 1), redondear(usable, 1), "CPU")
}

pub fn recomendar(hw: &Value, cat: &Value, sin_filtro: bool) -> Value {
    let (rapida, lenta, modo) = presupuesto(hw);
    let quiere = |m: &Value| if sin_filtro { m["filtro"] == "sin" } else { m["filtro"] != "sin" };
    let libre = hw["disk_free_gb"].as_f64().unwrap_or(0.0);
    let en_disco: Vec<&Value> = cat["llm"].as_array().into_iter().flatten().filter(|m| m["disk_gb"].as_f64().unwrap_or(0.0) <= libre && !m["verificar"].as_bool().unwrap_or(false) && quiere(m)).collect();
    let moe = Regex::new(r"-a\d+b").unwrap();
    let es_moe = |m: &Value| m["moe"].as_bool().unwrap_or(false) || moe.is_match(m["tag"].as_str().unwrap_or(""));
    let necesita = |m: &Value| m["need_gb"].as_f64().unwrap_or(f64::MAX);
    let mut caben: Vec<&Value> = en_disco.iter().copied().filter(|m| necesita(m) <= rapida || (modo == "CPU" && es_moe(m) && necesita(m) <= lenta)).collect();
    caben.sort_by(|a, b| {
        let k = |m: &Value| (m["params_b"].as_f64().unwrap_or(0.0), m["calidad"].as_f64().unwrap_or(3.0));
        k(a).partial_cmp(&k(b)).unwrap()
    });
    let estirado: Vec<&Value> = en_disco.iter().copied().filter(|m| !caben.contains(m) && necesita(m) <= lenta).collect();
    let ram = hw["ram_gb"].as_f64().unwrap_or(0.0);
    let arch = hw["arch"].as_str().unwrap_or("");
    let voz = if modo == "GPU" { hw["vram_gb"].as_f64().unwrap_or(0.0) } else { ram / 8.0 };
    let stt_todos: Vec<&Value> = cat["stt"].as_array().into_iter().flatten().collect();
    let mut stt: Vec<&Value> = stt_todos.iter().copied().filter(|s| s["need_gb"].as_f64().unwrap_or(0.0) <= voz.max(0.3)).collect();
    if (arch == "aarch64" || arch == "armv7l") && ram <= 8.0 {
        stt = stt_todos.iter().take(1).copied().collect();
    }
    json!({
        "mode": modo, "budget_fast_gb": rapida, "budget_max_gb": lenta,
        "best": caben.last().copied().cloned().unwrap_or(Value::Null),
        "stretch": estirado.last().copied().cloned().unwrap_or(Value::Null),
        "all_fit": caben,
        "stt": stt.last().or(stt_todos.first()).copied().cloned().unwrap_or(Value::Null),
        "patata": arch == "x86_64" && (ram <= 4.5 || hw["avx2"] == false),
        "role": if (arch.starts_with("aarch64") || arch.starts_with("arm")) && ram <= 8.0 { "nodo-robot" } else { "central" },
    })
}

fn llmfit(args: &[String]) -> Value {
    if !existe_programa("llmfit") {
        return Value::Null;
    }
    let mut c: Vec<&str> = vec!["llmfit"];
    c.extend(args.iter().map(String::as_str));
    c.extend(["recommend", "--json", "--limit", "5"]);
    serde_json::from_str(&correr(&c).1).unwrap_or(Value::Null)
}

fn barra(v: f64, total: f64, ancho: usize) -> String {
    let lleno = if total <= 0.0 { 0 } else { ((ancho as f64 * v / total) as usize).min(ancho) };
    "█".repeat(lleno) + &"░".repeat(ancho - lleno)
}

/// Informe en la terminal con el color de acento de la persona.
pub fn informe(hw: &Value, rec: &Value, hay_llmfit: bool) -> String {
    let acento = std::fs::read_to_string(crate::util::casa().join(".orion/acento")).ok().map(|s| s.trim().trim_start_matches('#').to_string()).filter(|s| s.len() == 6).unwrap_or_else(|| "ff2438".into());
    let rgb = |i: usize| u8::from_str_radix(&acento[i..i + 2], 16).unwrap_or(0);
    let (a, d, o) = (format!("\x1b[38;2;{};{};{}m", rgb(0), rgb(2), rgb(4)), "\x1b[2m", "\x1b[0m");
    let c = |k: &str| hw[k].to_string().trim_matches('"').to_string();
    let corto = |s: String, n: usize| s.chars().take(n).collect::<String>();
    let mut l = vec![
        format!("{a}╔══ NEO ORIÓN  ·  ESCANEO DE HARDWARE ══════════════════════╗{o}"),
        format!("  CPU    {}  ({} núcleos)", corto(c("cpu"), 44), c("cpu_cores")),
        format!("  PLACA  {}", corto(c("board"), 52)),
        format!("  RAM    {a}{}{o} {} GB", barra(hw["ram_gb"].as_f64().unwrap_or(0.0), 128.0, 28), c("ram_gb")),
    ];
    let gpus = hw["gpus"].as_array().cloned().unwrap_or_default();
    for g in &gpus {
        l.push(format!("  GPU    {a}{}{o} {} GB  {d}{}{o}", barra(g["vram_gb"].as_f64().unwrap_or(0.0), 48.0, 28), g["vram_gb"], g["name"].as_str().unwrap_or("")));
    }
    if gpus.is_empty() {
        l.push(format!("  GPU    {d}sin GPU dedicada → inferencia en CPU{o}"));
    }
    l.push(format!("  DISCO  {} GB libres", c("disk_free_gb")));
    l.push(format!("  ROL    {}   ·   MODO {}   ·   presupuesto {} GB", rec["role"].as_str().unwrap_or("").to_uppercase(), rec["mode"].as_str().unwrap_or(""), rec["budget_fast_gb"]));
    l.push(format!("{a}╠══ RESULTADO ═══════════════════════════════════════════════╣{o}"));
    match rec["best"].as_object() {
        Some(b) => l.push(format!("  TOPE RÁPIDO   {a}{}{o}  ({}B · {} GB) — {}", b["tag"].as_str().unwrap_or(""), b["params_b"], b["need_gb"], b["rol"].as_str().unwrap_or(""))),
        None => l.push(format!("  {a}Ningún LLM local cabe con holgura. Usa la nube (Claude) como cerebro.{o}")),
    }
    if rec["patata"] == true {
        l.push(format!("  {a}PC PATATA{o}  {d}poca RAM o CPU sin AVX2: modo liviano (HUD sin 3D, voz Vosk + espeak,{o}"));
        l.push(format!("  {d}IA local mínima para órdenes; lo pesado va a la nube si hay internet){o}"));
    }
    if let Some(s) = rec["stretch"].as_object() {
        let por = if rec["mode"] == "GPU" { "repartido GPU+RAM" } else { "en CPU" };
        l.push(format!("  TOPE ABSOLUTO {}  {d}(corre, pero {por}: lento para conversar){o}", s["tag"].as_str().unwrap_or("")));
    }
    l.push(format!("  OÍDO (STT)    {}", rec["stt"]["modelo"].as_str().unwrap_or("")));
    l.push(if hay_llmfit { format!("  {d}llmfit detectado: catálogo actualizado disponible (orion-fit --json){o}") } else { format!("  {d}Consejo: instala llmfit para un catálogo de modelos al día.{o}") });
    l.push(format!("{a}╚═══════════════════════════════════════════════════════════╝{o}"));
    l.join("\n")
}

/// Familias del catálogo en vivo de Ollama que no conocemos (para anunciarlas).
pub fn familias_nuevas(html: &str, cat: &Value, antes: &[String]) -> (Vec<String>, Vec<String>) {
    let mut vivos: Vec<String> = Regex::new(r#"href="/library/([a-z0-9._-]+)""#).unwrap().captures_iter(html).map(|c| c[1].to_string()).collect();
    vivos.sort();
    vivos.dedup();
    let conocidos: Vec<String> = cat["llm"].as_array().into_iter().flatten().filter_map(|m| m["tag"].as_str()).map(|t| t.split(':').next().unwrap().to_string()).collect();
    let nuevos = vivos.iter().filter(|v| !conocidos.contains(v) && !antes.contains(v)).cloned().collect();
    (vivos, nuevos)
}

fn actualizar(cat: &Value) -> i32 {
    let cache = crate::util::casa().join(".orion/catalogo-vivo.json");
    let html = match ureq::get("https://ollama.com/library?sort=newest").timeout(std::time::Duration::from_secs(20)).call().and_then(|r| r.into_string().map_err(|e| ureq::Error::from(std::io::Error::other(e)))) {
        Ok(h) => h,
        Err(e) => {
            println!("Sin conexión al catálogo ({e}). Sigo con el catálogo guardado.");
            return 1;
        }
    };
    let antes: Vec<String> = std::fs::read_to_string(&cache).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok()).and_then(|v| v["familias"].as_array().map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())).unwrap_or_default();
    let (vivos, nuevos) = familias_nuevas(&html, cat, &antes);
    let _ = std::fs::create_dir_all(cache.parent().unwrap());
    let _ = std::fs::write(&cache, json!({"familias": vivos}).to_string());
    println!("Catálogo en vivo: {} familias.", vivos.len());
    if !nuevos.is_empty() {
        println!("NUEVAS desde la última revisión: {}", nuevos.iter().take(40).cloned().collect::<Vec<_>>().join(", "));
        println!("Neo Orión las anuncia y calcula si caben con llmfit antes de ofrecerlas.");
    }
    0
}

pub fn principal(args: &[String]) -> i32 {
    let num = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).and_then(|v| v.parse::<f64>().ok());
    let tiene = |k: &str| args.iter().any(|a| a == k);
    let cat = catalogo();
    if tiene("--actualizar") {
        return actualizar(&cat);
    }
    let hw = detectar(num("--ram"), num("--vram"));
    let rec = recomendar(&hw, &cat, tiene("--sin-filtro"));
    let extra: Vec<String> = if num("--ram").is_some() { vec![format!("--memory={}G", hw["ram_gb"].as_f64().unwrap_or(0.0) as i64)] } else { vec![] };
    let lf = llmfit(&extra);
    if tiene("--json") {
        println!("{}", serde_json::to_string_pretty(&json!({"hardware": hw, "recommendation": rec, "llmfit": lf})).unwrap());
    } else {
        println!("{}", informe(&hw, &rec, !lf.is_null()));
    }
    if tiene("--install") {
        let Some(tag) = rec["best"]["tag"].as_str() else {
            eprintln!("No hay modelo local recomendado para este equipo.");
            return 1;
        };
        if !existe_programa("ollama") {
            eprintln!("Ollama no está instalado: orion install ollama");
            return 1;
        }
        return std::process::Command::new("ollama").args(["pull", tag]).status().map(|s| s.code().unwrap_or(1)).unwrap_or(1);
    }
    0
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn eq(ram: f64, vram: f64, arch: &str) -> Value {
        json!({"arch": arch, "ram_gb": ram, "vram_gb": vram, "avx2": true, "disk_free_gb": 500.0, "gpus": []})
    }

    #[test]
    fn presupuesto_y_recomendacion() {
        assert_eq!(presupuesto(&eq(16.0, 0.0, "x86_64")), (6.5, 11.7, "CPU"));
        assert_eq!(presupuesto(&eq(4.0, 0.0, "x86_64")), (2.8, 2.8, "CPU"));
        assert_eq!(presupuesto(&eq(32.0, 12.0, "x86_64")), (10.8, 28.2, "GPU"));
        let cat = serde_json::from_str::<Value>(CATALOGO_INCLUIDO).unwrap();
        let chico = recomendar(&eq(4.0, 0.0, "x86_64"), &cat, false);
        let grande = recomendar(&eq(64.0, 24.0, "x86_64"), &cat, false);
        assert_eq!(chico["patata"], true);
        assert!(chico["best"]["need_gb"].as_f64().unwrap() <= 2.8);
        assert!(grande["best"]["params_b"].as_f64().unwrap() > chico["best"]["params_b"].as_f64().unwrap());
        assert_eq!(recomendar(&eq(4.0, 0.0, "aarch64"), &cat, false)["role"], "nodo-robot");
        let (vivos, nuevos) = familias_nuevas(r#"<a href="/library/gemma3"></a><a href="/library/zzz-nuevo"></a><a href="/library/gemma3">"#, &cat, &[]);
        assert_eq!(vivos.len(), 2);
        assert_eq!(nuevos, vec!["zzz-nuevo"]);
        assert!(informe(&eq(4.0, 0.0, "x86_64"), &chico, false).contains("PC PATATA"));
    }
}
