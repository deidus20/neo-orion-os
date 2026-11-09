//! Sala de juegos y medios (antes orion_juegos): juegos locales (Steam + Proton, Heroic, Lutris, emuladores), en la
//! nube (GeForce NOW, Xbox Cloud, Boosteroid, Shadow…), desde otro PC de la casa (Sunshine → Moonlight) y series y
//! películas (Jellyfin, Kodi, plataformas oficiales como app web). La persona dice «quiero jugar X» o «pon la serie»
//! y esto decide dónde correrlo según el equipo, lo instala si falta y lo lanza. Teclados, mouse y mandos (Xbox,
//! PlayStation, Nintendo, 8BitDo, Steam, genéricos) se detectan solos. Los datos están en config/juegos.json.

use crate::util::{correr, existe_programa};
use regex::Regex;
use serde_json::{json, Value};
use std::sync::LazyLock;

static DATOS: LazyLock<Value> = LazyLock::new(|| serde_json::from_str(include_str!("../../../config/juegos.json")).expect("juegos.json"));

pub fn plataformas() -> &'static Vec<Value> {
    DATOS["plataformas"].as_array().unwrap()
}

pub fn plataforma(clave: &str) -> Option<&'static Value> {
    plataformas().iter().find(|p| p["clave"] == clave)
}

const MANDOS: [(&str, &str); 6] = [
    (r"xbox|x-box|microsoft.*controller|045e:", "Xbox"),
    (r"dualsense|dualshock|sony.*controller|wireless controller|054c:", "PlayStation"),
    (r"nintendo|pro controller|joy-con|057e:", "Nintendo"),
    (r"8bitdo|2dc8:", "8BitDo"),
    (r"steam controller|28de:", "Steam"),
    (r"gamepad|joystick|controller", "genérico"),
];

/// Lee /proc/bus/input/devices y clasifica teclados, mouse y mandos.
pub fn dispositivos(texto: Option<&str>) -> Value {
    let leido;
    let texto = match texto {
        Some(t) => t,
        None => {
            leido = std::fs::read_to_string("/proc/bus/input/devices").unwrap_or_default();
            &leido
        }
    };
    let (mut teclados, mut mouse, mut mandos) = (vec![], vec![], vec![]);
    let re_n = Regex::new(r#"N: Name="([^"]*)""#).unwrap();
    let re_id = Regex::new(r"Vendor=([0-9a-f]{4}) Product=([0-9a-f]{4})").unwrap();
    let re_h = Regex::new(r"H: Handlers=(.*)").unwrap();
    let r = |p: &str| Regex::new(p).unwrap();
    for bloque in texto.trim().split("\n\n") {
        let Some(n) = re_n.captures(bloque).map(|c| c[1].to_string()) else { continue };
        let vid = re_id.captures(bloque).map(|c| format!("{}:{}", &c[1], &c[2])).unwrap_or_default();
        let hand = re_h.captures(bloque).map(|c| c[1].to_string()).unwrap_or_default();
        let low = format!("{} {vid}", n.to_lowercase());
        if hand.contains("js") || r(r"gamepad|controller|joystick").is_match(&low) {
            if r(r"motion sensors|touchpad").is_match(&low) {
                continue; // sub-dispositivos del mismo mando (giroscopio, panel táctil)
            }
            let marca = MANDOS.iter().find(|(p, _)| r(p).is_match(&low)).map(|x| x.1).unwrap_or("genérico");
            mandos.push(json!({"nombre": n, "marca": marca, "id": vid}));
        } else if (hand.contains("mouse") && !r(r"touchpad|trackpad").is_match(&low)) || r(r"\bmouse\b").is_match(&low) {
            mouse.push(json!({"nombre": n, "id": vid}));
        } else if hand.contains("kbd") && r(r"keyboard|teclado").is_match(&low) {
            teclados.push(json!({"nombre": n, "id": vid}));
        }
    }
    json!({"teclados": teclados, "mouse": mouse, "mandos": mandos})
}

fn flatpaks() -> Vec<String> {
    if !existe_programa("flatpak") {
        return vec![];
    }
    correr(&["flatpak", "list", "--columns=application"]).1.split_whitespace().map(String::from).collect()
}

pub fn instalada(p: &Value, fp: &[String]) -> bool {
    let d: Vec<&str> = p["detecta"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    d.is_empty() || d.iter().any(|x| existe_programa(x) || fp.iter().any(|f| f == x))
}

/// Dónde conviene jugar en este equipo.
pub fn recomendar(gpu: Option<&str>, vram_gb: f64, red_mbps: Option<f64>) -> Value {
    let potente = gpu.is_some_and(|g| !g.is_empty()) && vram_gb >= 6.0;
    let (mut prin, mut razon) = if potente {
        (vec!["steam", "heroic"], "Tienes GPU dedicada: los juegos corren aquí mismo con Proton.".to_string())
    } else {
        (vec!["geforcenow", "xboxcloud"], "Sin GPU fuerte: el juego corre en la nube y aquí solo llega el video.".to_string())
    };
    if red_mbps.is_some_and(|r| r < 15.0) && !potente {
        razon += " Ojo: tu red mide menos de 15 Mb/s, la nube se va a ver mal; mejor Moonlight desde otro PC de la casa.";
        prin.insert(0, "moonlight");
    }
    let mut extras: Vec<String> = DATOS["extras"].as_object().unwrap().keys().cloned().collect();
    if potente {
        extras.push("sunshine".into());
    }
    json!({"principal": prin, "razon": razon, "siempre": ["retroarch", "jellyfin", "kodi"], "extras": extras})
}

fn plataforma_de(pedido: &str) -> Option<&'static str> {
    let t = pedido.to_lowercase();
    let reglas = [
        ("boosteroid", "boosteroid"), ("shadow", "shadow"), ("luna", "luna"), ("blacknut", "blacknut"), ("steam link|remote play", "steamlink"),
        ("playstation plus|ps plus|psn", "psplus"), ("geforce|nvidia|nube", "geforcenow"), ("xbox|game pass", "xboxcloud"),
        ("epic|gog|amazon games|heroic", "heroic"), ("steam|proton", "steam"), ("lutris", "lutris"),
        ("emulador|retro|nes|snes|playstation 1|ps1|n64|gba", "retroarch"), ("moonlight|desde la torre|desde el otro pc", "moonlight"),
        ("sunshine|servidor de juegos", "sunshine"), ("jellyfin|mis series|mis pel", "jellyfin"), ("kodi", "kodi"),
    ];
    if let Some((_, c)) = reglas.iter().find(|(p, _)| Regex::new(p).unwrap().is_match(&t)) {
        return Some(c);
    }
    DATOS["servicios"].as_object().unwrap().keys().any(|s| t.contains(s.as_str())).then_some("servicios")
}

/// Todas las nubes de juego, primero las que funcionan en Chile.
pub fn nubes() -> Vec<Value> {
    let orden = |p: &Value| {
        let c = p["chile"].as_str().unwrap_or("");
        if c.starts_with("sí") { 0 } else if c.starts_with("verificar") { 1 } else { 2 }
    };
    let mut v: Vec<&Value> = plataformas().iter().filter(|p| p["tipo"] == "nube").collect();
    v.sort_by_key(|p| orden(p));
    v.iter().map(|p| json!({"clave": p["clave"], "nombre": p["nombre"], "modelo": p["modelo"], "chile": p["chile"], "nota": p["nota"]})).collect()
}

/// Convierte «quiero jugar en la nube» / «pon Netflix» en pasos concretos.
pub fn plan(pedido: &str, gpu: Option<&str>, vram_gb: f64, red_mbps: Option<f64>, mandos: Value) -> Value {
    let mut clave = plataforma_de(pedido);
    if clave == Some("psplus") {
        return json!({"plataforma": "PlayStation Plus (nube)", "clave": "psplus", "tipo": "nube", "pasos": [],
                      "aviso": "Sony no tiene app de PlayStation Plus para Linux ni versión web. Alternativas: jugarlo en Shadow PC (Windows en la nube) o Remote Play desde tu PS5 con Chiaki-ng.", "mandos": mandos});
    }
    if clave.is_none() {
        let rec = recomendar(gpu, vram_gb, red_mbps);
        let c = if Regex::new(r"jug|juego").unwrap().is_match(&pedido.to_lowercase()) { rec["principal"][0].as_str().unwrap().to_string() } else { "jellyfin".into() };
        clave = plataformas().iter().find(|p| p["clave"] == c.as_str()).and_then(|p| p["clave"].as_str());
    }
    let clave = clave.unwrap_or("jellyfin");
    let p = plataforma(clave).unwrap();
    let mut lanzar: Vec<String> = p["lanzar"].as_array().into_iter().flatten().filter_map(Value::as_str).map(String::from).collect();
    if clave == "servicios" {
        let serv = DATOS["servicios"].as_object().unwrap();
        let s = serv.keys().find(|s| pedido.to_lowercase().contains(s.as_str())).map(String::as_str).unwrap_or("netflix");
        lanzar = vec!["xdg-open".into(), serv[s].as_str().unwrap().into()];
    }
    let ya = instalada(p, &flatpaks());
    let mut pasos: Vec<String> = if ya { vec![] } else { p["instalar"].as_array().into_iter().flatten().filter_map(Value::as_str).map(String::from).collect() };
    pasos.push(lanzar.join(" "));
    let aviso = if p["necesita_gpu"] == true && gpu.is_none_or(|g| g.is_empty()) { "Este equipo no tiene GPU dedicada; para juegos pesados usa GeForce NOW." } else { "" };
    json!({"plataforma": p["nombre"], "clave": clave, "tipo": p["tipo"], "instalada": ya, "pasos": pasos, "licencia": p["licencia"], "nota": p["nota"], "aviso": aviso, "mandos": mandos})
}

/// La herramienta MCP `jugar`: usa la gráfica real del equipo (la versión Python siempre creía que no había GPU).
pub fn jugar(pedido: &str, hacerlo: bool, ejecutar: impl Fn(&str) -> Value) -> Value {
    let hw = crate::fit::detectar(None, None);
    let gpu = hw["gpus"][0]["name"].as_str().map(String::from);
    let mut pl = plan(pedido, gpu.as_deref(), hw["vram_gb"].as_f64().unwrap_or(0.0), None, dispositivos(None)["mandos"].clone());
    if hacerlo {
        let mut res = vec![];
        for paso in pl["pasos"].as_array().cloned().unwrap_or_default() {
            let r = ejecutar(paso.as_str().unwrap_or(""));
            let ok = r["ok"] == true;
            res.push(r);
            if !ok {
                break;
            }
        }
        pl["resultado"] = json!(res);
    }
    pl
}

#[cfg(test)]
mod pruebas {
    use super::*;

    const PROC: &str = "I: Bus=0003 Vendor=045e Product=0b12 Version=0110\nN: Name=\"Microsoft Xbox Series S|X Controller\"\nH: Handlers=event20 js0\n\n\
I: Bus=0005 Vendor=054c Product=0ce6 Version=8100\nN: Name=\"DualSense Wireless Controller\"\nH: Handlers=event21 js1\n\n\
I: Bus=0005 Vendor=054c Product=0ce6 Version=8100\nN: Name=\"DualSense Wireless Controller Motion Sensors\"\nH: Handlers=event22\n\n\
I: Bus=0003 Vendor=046d Product=c52b Version=0111\nN: Name=\"Logitech USB Receiver Mouse\"\nH: Handlers=mouse0 event5\n\n\
I: Bus=0003 Vendor=046d Product=c52b Version=0111\nN: Name=\"Logitech USB Receiver Keyboard\"\nH: Handlers=sysrq kbd leds event4\n\n\
I: Bus=0018 Vendor=06cb Product=7e7e Version=0100\nN: Name=\"SYNA Touchpad\"\nH: Handlers=mouse1 event6\n";

    #[test]
    fn como_la_prueba_de_python() {
        let d = dispositivos(Some(PROC));
        let marcas: Vec<&str> = d["mandos"].as_array().unwrap().iter().filter_map(|m| m["marca"].as_str()).collect();
        assert_eq!(marcas, ["Xbox", "PlayStation"]);
        assert_eq!((d["mouse"].as_array().unwrap().len(), d["teclados"].as_array().unwrap().len()), (1, 1));
        assert_eq!(recomendar(None, 0.0, None)["principal"][0], "geforcenow");
        let r = recomendar(Some("RTX 3060"), 12.0, None);
        assert!(r["principal"][0] == "steam" && r["extras"].as_array().unwrap().contains(&json!("sunshine")));
        assert_eq!(recomendar(None, 0.0, Some(8.0))["principal"][0], "moonlight");
        let p = |t: &str| plan(t, None, 0.0, None, json!([]));
        assert!(p("quiero jugar en geforce now")["pasos"].as_array().unwrap().last().unwrap().as_str().unwrap().contains("com.nvidia.geforcenow"));
        assert_eq!(p("pon netflix")["pasos"].as_array().unwrap().last().unwrap(), "xdg-open https://www.netflix.com");
        assert_eq!(p("quiero jugar")["clave"], "geforcenow");
        assert_eq!(plan("quiero jugar", Some("RTX 4070"), 12.0, None, json!([]))["clave"], "steam");
        assert_eq!(p("abre mis series")["clave"], "jellyfin");
        assert_eq!(p("emulador de snes")["clave"], "retroarch");
        let n = nubes();
        assert!(n[0]["chile"].as_str().unwrap().starts_with("sí") && n.last().unwrap()["clave"] == "luna" && n.len() == 6);
        assert_eq!(p("quiero jugar en boosteroid")["clave"], "boosteroid");
        assert!(p("abre shadow")["pasos"].to_string().contains("shadow-amd64.deb"));
        assert!(p("juega playstation plus")["aviso"].as_str().unwrap().contains("Linux"));
        assert!(plataformas().iter().all(|x| !x["licencia"].as_str().unwrap_or("").is_empty()));
        assert_eq!(plataformas().len(), 16);
    }

    #[test]
    fn hacerlo_sigue_todos_los_pasos() {
        // la versión Python se detenía después del primer paso (esperaba «ok» y recibía «codigo»)
        let hechos = std::sync::Mutex::new(vec![]);
        let r = jugar("pon netflix", true, |c| {
            hechos.lock().unwrap().push(c.to_string());
            json!({"ok": true})
        });
        assert_eq!(r["resultado"].as_array().unwrap().len(), r["pasos"].as_array().unwrap().len());
    }
}
