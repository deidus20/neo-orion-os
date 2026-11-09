//! Primer arranque: baja lo que no cabe en el pendrive (Ollama y los modelos elegidos, el oído, la voz, los
//! controladores) y deja la llave de voz y los servicios de la cuenta. Sin internet devuelve error y systemd reintenta.
//! Todo nativo: Ollama, Vosk (libvosk), whisper.cpp, Piper; nada de Python.

use crate::util::{correr, existe_programa};
use serde_json::Value;
use std::path::{Path, PathBuf};

const VOSK_LIB: &str = "https://github.com/alphacep/vosk-api/releases/download/v0.3.45/vosk-linux-x86_64-0.3.45.zip";
const VOSK_MODELO: &str = "https://alphacephei.com/vosk/models/vosk-model-small-es-0.42.zip";
const PIPER: &str = "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_linux_x86_64.tar.gz";
const PIPER_VOZ: &str = "https://huggingface.co/rhasspy/piper-voices/resolve/main/es/es_MX/claude/high/es_MX-claude-high.onnx";
const WHISPER_MODELO: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-";
const WHISPER_SRC: &str = "https://github.com/ggml-org/whisper.cpp";

fn log(m: &str) {
    println!("[orion-primer-arranque] {m}");
}

fn bajar(url: &str, destino: &Path) -> bool {
    if destino.exists() {
        return true;
    }
    let _ = std::fs::create_dir_all(destino.parent().unwrap());
    log(&format!("bajando {url}"));
    let tmp = destino.with_extension("parcial");
    let ok = correr(&["curl", "-fsSL", "--retry", "3", "-o", &tmp.to_string_lossy(), url]).0 == 0;
    ok && std::fs::rename(&tmp, destino).is_ok()
}

fn hay_internet() -> bool {
    ["https://ollama.com", "http://archive.ubuntu.com"].iter().any(|u| correr(&["curl", "-fsI", "-m", "10", u]).0 == 0)
}

/// Vosk: la librería nativa en /opt/orion/lib y el modelo español en la carpeta de la persona.
pub fn instalar_vosk(casa: &Path) -> bool {
    let lib = Path::new("/opt/orion/lib/libvosk.so");
    let tmp = std::env::temp_dir();
    let mut ok = true;
    if !lib.exists() {
        let z = tmp.join("vosk.zip");
        ok &= bajar(VOSK_LIB, &z) && correr(&["unzip", "-oj", &z.to_string_lossy(), "*/libvosk.so", "-d", "/opt/orion/lib"]).0 == 0;
    }
    let modelo = casa.join(".orion/modelos/vosk-small-es");
    if !modelo.exists() {
        let z = tmp.join("vosk-es.zip");
        let dir = casa.join(".orion/modelos");
        ok &= bajar(VOSK_MODELO, &z) && correr(&["unzip", "-oq", &z.to_string_lossy(), "-d", &dir.to_string_lossy()]).0 == 0 && std::fs::rename(dir.join("vosk-model-small-es-0.42"), &modelo).is_ok();
    }
    ok
}

/// whisper.cpp: se compila una vez desde su código (C++), y se baja el modelo pedido.
pub fn instalar_whisper(casa: &Path, tamano: &str) -> bool {
    let mut ok = true;
    if !existe_programa("whisper-cli") {
        log("compilando whisper.cpp (una sola vez)");
        let src = std::env::temp_dir().join("whisper.cpp");
        let s = src.to_string_lossy().to_string();
        ok &= correr(&["apt-get", "install", "-y", "build-essential", "cmake", "git"]).0 == 0
            && (src.exists() || correr(&["git", "clone", "--depth", "1", WHISPER_SRC, &s]).0 == 0)
            && correr(&["cmake", "-S", &s, "-B", &format!("{s}/build"), "-DCMAKE_BUILD_TYPE=Release", "-DBUILD_SHARED_LIBS=OFF"]).0 == 0
            && correr(&["cmake", "--build", &format!("{s}/build"), "-j", "--config", "Release", "--target", "whisper-cli"]).0 == 0
            && correr(&["install", "-m755", &format!("{s}/build/bin/whisper-cli"), "/usr/local/bin/whisper-cli"]).0 == 0;
    }
    ok & bajar(&format!("{WHISPER_MODELO}{tamano}.bin"), &casa.join(format!(".orion/modelos/ggml-{tamano}.bin")))
}

pub fn instalar_piper(casa: &Path) -> bool {
    let mut ok = true;
    if !existe_programa("piper") {
        let t = std::env::temp_dir().join("piper.tgz");
        ok &= bajar(PIPER, &t) && correr(&["tar", "-xzf", &t.to_string_lossy(), "-C", "/opt/orion"]).0 == 0 && correr(&["ln", "-sf", "/opt/orion/piper/piper", "/usr/local/bin/piper"]).0 == 0;
    }
    let voz = casa.join(".orion/voces/es_MX-claude-high.onnx");
    ok & bajar(PIPER_VOZ, &voz) & bajar(&format!("{PIPER_VOZ}.json"), &voz.with_extension("onnx.json"))
}

pub fn ejecutar() -> i32 {
    let cfg: Value = std::fs::read_to_string("/etc/orion/instalacion.json").ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let usuario = crate::util::leer("/etc/orion/usuario");
    let usuario = if usuario.is_empty() { "orion".to_string() } else { usuario };
    let casa = PathBuf::from(crate::util::sh(&["getent", "passwd", &usuario]).split(':').nth(5).unwrap_or("/home/orion").trim());
    if !hay_internet() {
        log("sin internet, reintento después");
        return 1;
    }
    let mut fallas = 0;
    let mut paso = |ok: bool, que: &str| {
        if !ok {
            log(&format!("falló: {que}"));
            fallas += 1;
        }
    };
    // IA local
    if !existe_programa("ollama") {
        log("instalando Ollama");
        paso(correr(&["sh", "-c", "curl -fsSL https://ollama.com/install.sh | sh"]).0 == 0, "Ollama");
    }
    if existe_programa("ollama") {
        correr(&["systemctl", "enable", "--now", "ollama.service"]);
        for _ in 0..30 {
            if correr(&["curl", "-fs", "http://127.0.0.1:11434/api/tags"]).0 == 0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        for m in cfg["ia"]["locales"].as_array().into_iter().flatten().filter_map(Value::as_str) {
            log(&format!("bajando IA {m}"));
            paso(correr(&["ollama", "pull", m]).0 == 0, m);
        }
    }
    // controladores de ESTE equipo
    if let Some(c) = std::fs::read_to_string("/etc/orion/controladores.json").ok().and_then(|s| serde_json::from_str::<Value>(&s).ok()) {
        let paq: Vec<&str> = c["plan"]["paquetes"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        if !paq.is_empty() {
            log(&format!("controladores: {}", paq.join(" ")));
            let mut cmd = vec!["apt-get", "install", "-y"];
            cmd.extend(&paq);
            paso(correr(&cmd).0 == 0, "controladores");
        }
        if c["gpus"].as_array().is_some_and(|g| g.iter().any(|x| x["fabricante"] == "NVIDIA")) && existe_programa("ubuntu-drivers") {
            log("controlador NVIDIA recomendado");
            paso(correr(&["ubuntu-drivers", "install"]).0 == 0, "NVIDIA");
        }
    }
    // oído y voz (todo nativo)
    let oido = cfg["oido"].as_str().unwrap_or("vosk");
    if oido.starts_with("vosk") {
        paso(instalar_vosk(&casa), "Vosk");
    } else {
        let t = oido.rsplit('-').next().filter(|t| ["tiny", "base", "small", "medium"].contains(t)).unwrap_or(if oido.starts_with("moonshine") { "tiny" } else { "base" });
        paso(instalar_whisper(&casa, t), "whisper.cpp");
    }
    // puente temporal (etapa 3 lo quita): el servidor MCP todavía es Python
    if Path::new("/opt/orion/venv/bin/pip").exists() && correr(&["/opt/orion/venv/bin/python3", "-c", "import mcp"]).0 != 0 {
        paso(correr(&["/opt/orion/venv/bin/pip", "install", "-q", "mcp"]).0 == 0, "mcp");
    }
    // kokoro → Piper mientras koko (kokoros) no venga empaquetado
    if matches!(cfg["voz"].as_str().unwrap_or("espeak-ng"), "piper" | "kokoro") {
        paso(instalar_piper(&casa), "Piper");
    }
    correr(&["chown", "-R", &format!("{usuario}:{usuario}"), &casa.join(".orion").to_string_lossy()]);
    // llave de voz con las frases grabadas en el instalador
    if !casa.join(".orion/llave-voz.json").exists() {
        let frase = cfg["cuenta"]["frase"].as_str().unwrap_or("");
        let modo = cfg["cuenta"]["desbloqueo"].as_str().unwrap_or("voz_o_clave");
        let wavs: Vec<String> = if frase.is_empty() { vec![] } else {
            let mut v: Vec<String> = std::fs::read_dir(casa.join(".orion/voz")).into_iter().flatten().flatten().map(|e| e.path().to_string_lossy().to_string()).filter(|p| p.contains("/frase-") && p.ends_with(".wav")).collect();
            v.sort();
            v
        };
        let mut cmd = vec!["sudo", "-u", &usuario, "env"];
        let h = format!("HOME={}", casa.display());
        cmd.extend([h.as_str(), "/usr/local/bin/orion-nucleo", "llave-desde", frase, modo]);
        cmd.extend(wavs.iter().map(String::as_str));
        correr(&cmd);
    }
    // servicios de la cuenta del dueño
    correr(&["loginctl", "enable-linger", &usuario]);
    let uid = crate::util::sh(&["id", "-u", &usuario]).trim().to_string();
    correr(&["sudo", "-u", &usuario, &format!("XDG_RUNTIME_DIR=/run/user/{uid}"), "systemctl", "--user", "enable", "orion-oido.service", "orion-vigia.service", "orion-respaldo.timer"]);
    if fallas == 0 {
        let _ = std::fs::create_dir_all("/var/lib/orion");
        let _ = std::fs::write("/var/lib/orion/primer-arranque.hecho", chrono::Local::now().to_rfc3339());
        log("listo");
        return 0;
    }
    log(&format!("{fallas} paso(s) fallaron; reintento después"));
    1
}
