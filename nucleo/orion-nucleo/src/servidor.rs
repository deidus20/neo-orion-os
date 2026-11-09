//! Servidor local de Neo Orión (reemplaza a `orion_core.servidor`): sirve el HUD y el instalador y les da datos.
//!
//! Escucha solo en 127.0.0.1. Todas las llamadas /api/* exigen la cabecera `X-Orion: 1` (un sitio web no puede
//! ponerla sin permiso CORS, que aquí no se da). Misma API que la versión Python, para que el HUD no cambie.

use crate::{archivo, controladores, electrica, equipo, ia, iconos, legado, util};
use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use tiny_http::{Header, Method, Request, Response, Server};

pub const VERSION: &str = "0.9";

pub struct Estado {
    pub raiz: PathBuf,
    pub modo: String,
    pub progreso: PathBuf,
    pub instalador: Mutex<Option<Child>>,
}

pub fn modo_auto() -> String {
    if std::fs::read_to_string("/proc/cmdline").unwrap_or_default().contains("boot=casper") { "instalador".into() } else { "hud".into() }
}

pub fn raiz_por_defecto() -> PathBuf {
    if Path::new("/usr/share/orion/hud/index.html").exists() { "/usr/share/orion".into() } else { PathBuf::from(".") }
}

fn cabecera(k: &str, v: &str) -> Header {
    Header::from_bytes(k.as_bytes(), v.as_bytes()).unwrap()
}

fn responder_json(req: Request, v: &Value, codigo: u16) {
    let r = Response::from_string(v.to_string())
        .with_status_code(codigo)
        .with_header(cabecera("Content-Type", "application/json; charset=utf-8"))
        .with_header(cabecera("Cache-Control", "no-store"))
        .with_header(cabecera("Server", &format!("NeoOrion/{VERSION}")));
    let _ = req.respond(r);
}

fn archivo_estatico(est: &Estado, req: Request, ruta: &str) {
    if ruta.is_empty() || ruta == "/" {
        let destino = if est.modo == "instalador" { "/instalador/" } else { "/hud/" };
        let _ = req.respond(Response::empty(302).with_header(cabecera("Location", destino)));
        return;
    }
    let limpio = ruta.trim_matches('/');
    let (base, resto) = limpio.split_once('/').unwrap_or((limpio, ""));
    let (carpeta, resto) = match base {
        "hud" => (est.raiz.join("hud"), if resto.is_empty() { "index.html" } else { resto }),
        "instalador" => {
            let c = if est.raiz.join("instalador").exists() { est.raiz.join("instalador") } else { est.raiz.join("hud") };
            let r = if !resto.is_empty() { resto } else if c.join("instalar.html").exists() { "instalar.html" } else { "index.html" };
            (c, r)
        }
        _ => return responder_json(req, &json!({"error": "no existe"}), 404),
    };
    let (Ok(c), Ok(f)) = (carpeta.canonicalize(), carpeta.join(resto).canonicalize()) else {
        return responder_json(req, &json!({"error": "no existe"}), 404);
    };
    if !f.starts_with(&c) || !f.is_file() {
        return responder_json(req, &json!({"error": "no existe"}), 404);
    }
    let Ok(datos) = std::fs::read(&f) else { return responder_json(req, &json!({"error": "no existe"}), 404) };
    let mut tipo = mime_guess::from_path(&f).first_or_octet_stream().essence_str().to_string();
    if matches!(f.extension().and_then(|e| e.to_str()), Some("html" | "js" | "css" | "json")) {
        tipo += "; charset=utf-8";
    }
    let _ = req.respond(Response::from_data(datos).with_header(cabecera("Content-Type", &tipo)));
}

fn consulta(url: &str, clave: &str) -> Option<String> {
    url.split_once('?')?.1.split('&').find_map(|p| p.split_once('=').filter(|(k, _)| *k == clave).map(|(_, v)| v.to_string()))
}

fn eventos(desde: i64) -> Value {
    let t = std::fs::read_to_string(util::casa().join(".orion/activaciones.jsonl")).unwrap_or_default();
    let lineas: Vec<&str> = t.lines().collect();
    let ev: Vec<Value> = if desde >= 0 { lineas.iter().skip(desde as usize).filter_map(|l| serde_json::from_str(l).ok()).collect() } else { vec![] };
    json!({"total": lineas.len(), "eventos": ev})
}

fn progreso(est: &Estado, desde: usize) -> Value {
    let t = std::fs::read_to_string(&est.progreso).unwrap_or_default();
    let lineas: Vec<&str> = t.lines().collect();
    let ev: Vec<Value> = lineas.iter().skip(desde).filter(|l| !l.trim().is_empty()).filter_map(|l| serde_json::from_str(l).ok()).collect();
    let mut g = est.instalador.lock().unwrap();
    let (vivo, codigo) = match g.as_mut() {
        None => (false, Value::Null),
        Some(h) => match h.try_wait() {
            Ok(None) => (true, Value::Null),
            Ok(Some(s)) => (false, json!(s.code().unwrap_or(-1))),
            Err(_) => (false, Value::Null),
        },
    };
    json!({"eventos": ev, "total": lineas.len(), "corriendo": vivo, "codigo": codigo})
}

fn corriendo(est: &Estado) -> bool {
    est.instalador.lock().unwrap().as_mut().is_some_and(|h| matches!(h.try_wait(), Ok(None)))
}

fn lanzar_instalacion(est: &Estado, mut cfg: Value) -> Value {
    if corriendo(est) {
        return json!({"ok": false, "error": "ya hay una instalación en curso"});
    }
    if !util::sh(&["id", "-u"]).trim().eq("0") {
        return json!({"ok": false, "error": "el servidor del instalador no corre como administrador"});
    }
    let clave = cfg.as_object_mut().and_then(|o| o.remove("clave")).and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
    let rev = legado::llamar("instalar", &json!({"accion": "revisar", "config": cfg}));
    if let Some(e) = rev["errores"].as_array().filter(|e| !e.is_empty()) {
        return json!({"ok": false, "error": e.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("; ")});
    }
    let ruta = std::env::temp_dir().join(format!("orion-inst-{}.json", std::process::id()));
    if std::fs::write(&ruta, cfg.to_string()).is_err() || std::fs::write(&est.progreso, "").is_err() {
        return json!({"ok": false, "error": "no se pudo preparar la instalación"});
    }
    let err = std::fs::File::create("/run/orion-instalacion.err").map(Stdio::from).unwrap_or_else(|_| Stdio::null());
    match Command::new(legado::python())
        .args(["-m", "orion_core.instalador", &ruta.to_string_lossy(), "--progreso", &est.progreso.to_string_lossy()])
        .env("ORION_CLAVE", clave)
        .env("PYTHONPATH", legado::raiz_python())
        .stdout(Stdio::null())
        .stderr(err)
        .spawn()
    {
        Ok(h) => {
            *est.instalador.lock().unwrap() = Some(h);
            json!({"ok": true})
        }
        Err(e) => json!({"ok": false, "error": e.to_string()}),
    }
}

/// Graba 3 s del micrófono para la llave de voz.
pub fn grabar_frase(n: i64, instalador: bool) -> Value {
    if !(0..=3).contains(&n) {
        return json!({"ok": false, "error": "número de frase inválido"});
    }
    let destino = if instalador {
        PathBuf::from(format!("/run/orion-voz-{n}.wav"))
    } else {
        let d = util::casa().join(".orion/voz");
        let _ = std::fs::create_dir_all(&d);
        d.join(if n > 0 { format!("nueva-{n}.wav") } else { "verificar.wav".into() })
    };
    match Command::new("arecord").args(["-q", "-d", "3", "-f", "S16_LE", "-r", "16000", "-c", "1"]).arg(&destino).output() {
        Ok(o) if o.status.success() => json!({"ok": true, "bytes": std::fs::metadata(&destino).map(|m| m.len()).unwrap_or(0)}),
        Ok(o) => json!({"ok": false, "error": String::from_utf8_lossy(&o.stderr).trim().chars().take(160).collect::<String>()}),
        Err(e) => json!({"ok": false, "error": e.to_string()}),
    }
}

/// Acciones de las herramientas (lista cerrada). Lo ya migrado se atiende en Rust; el resto, por el puente.
pub fn accion(modulo: &str, datos: &Value) -> Value {
    let a = datos["accion"].as_str().unwrap_or("");
    let txt = |k: &str| datos[k].as_str().unwrap_or("").to_string();
    match modulo {
        "electrica" => match electrica::desde_frase(&txt("texto")) {
            Some(r) => {
                let mut v = serde_json::to_value(&r).unwrap();
                v["ok"] = true.into();
                v["resumen"] = r.texto().into();
                v
            }
            None => json!({"ok": false, "error": "di la corriente o la potencia y el largo"}),
        },
        "controladores" => {
            let mut d = controladores::detectar();
            if d["gpus"].as_array().is_some_and(|g| g.iter().any(|x| x["fabricante"] == "NVIDIA")) {
                d["al_dia"] = controladores::revisar_al_dia_equipo();
            }
            d["ok"] = true.into();
            d
        }
        "apps" => match a {
            "abrir" => iconos::abrir(&txt("id")),
            "registrar" => iconos::registrar(&txt("id"), &txt("nombre"), &txt("categoria"), &txt("comando"), &txt("web"), &txt("icono")),
            "quitar" => iconos::quitar(&txt("id")),
            _ => json!({"ok": true, "apps": iconos::listar(false), "regla": iconos::regla()}),
        },
        "archivo" => match a {
            "abrir" => archivo::abrir(&txt("ruta")),
            _ => archivo::listar(&txt("tema")),
        },
        "activacion" => crate::voz::activacion::accion(datos),
        "respaldo" => crate::respaldo::accion(datos),
        "remoto" => crate::remoto::accion(datos),
        "actualizar" => crate::actualizar::accion(datos),
        "telefono" => crate::telefono::ordenar(if a == "iniciar" || a == "parar" { a } else { "estado" }),
        "llave" => crate::voz::llave::accion(datos),
        "casa" => crate::casa::Casa::cargar().ordenar(&txt("texto"), datos["confirmado"] == true),
        "memoria" => match crate::memoria::Memoria::abrir(None) {
            Ok(m) => match a {
                "buscar" => json!({"ok": true, "resultados": m.buscar(&txt("texto"), 8)}),
                "anotar" => json!({"ok": m.diario(&txt("texto"), "hud").is_ok()}),
                _ => json!({"ok": true, "indexadas": m.reindexar()}),
            },
            Err(e) => json!({"ok": false, "error": e.to_string()}),
        },
        m if legado::PENDIENTES.contains(&m) && m != "instalar" => legado::llamar(m, datos),
        _ => json!({"ok": false, "error": "módulo desconocido"}),
    }
}

fn atender(est: &Estado, mut req: Request) {
    let url = req.url().to_string();
    let ruta = url.split('?').next().unwrap_or("").to_string();
    if !ruta.starts_with("/api/") {
        return archivo_estatico(est, req, &ruta);
    }
    if !req.headers().iter().any(|h| h.field.equiv("X-Orion") && h.value.as_str() == "1") {
        return responder_json(req, &json!({"error": "falta la cabecera X-Orion"}), 403);
    }
    if *req.method() == Method::Get {
        let v = match ruta.as_str() {
            "/api/estado" => {
                let inst = ia::instalacion();
                json!({"modo": est.modo, "version": VERSION, "nucleo": "rust", "ia_local": ia::modelos_locales(),
                       "acento": util::leer(&util::casa().join(".orion/acento").to_string_lossy()),
                       "instalacion": {"identidad": inst.get("identidad"), "idioma": inst.get("idioma")}})
            }
            "/api/equipo" => equipo::equipo(),
            "/api/progreso" => progreso(est, consulta(&url, "desde").and_then(|d| d.parse().ok()).unwrap_or(0)),
            "/api/eventos" => eventos(consulta(&url, "desde").and_then(|d| d.parse().ok()).unwrap_or(-1)),
            "/api/rendimiento" => std::fs::read_to_string(util::casa().join(".orion/rendimiento.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(|| json!({"nivel": "completo"})),
            _ => return responder_json(req, &json!({"error": "no existe"}), 404),
        };
        return responder_json(req, &v, 200);
    }
    let largo = req.body_length().unwrap_or(0);
    if largo > 1_000_000 {
        return responder_json(req, &json!({"error": "muy grande"}), 413);
    }
    let mut cuerpo = String::new();
    let _ = req.as_reader().take(1_000_001).read_to_string(&mut cuerpo);
    let Ok(datos) = serde_json::from_str::<Value>(if cuerpo.trim().is_empty() { "{}" } else { &cuerpo }) else {
        return responder_json(req, &json!({"error": "JSON inválido"}), 400);
    };
    if let Some(m) = ruta.strip_prefix("/api/accion/") {
        return responder_json(req, &accion(m, &datos), 200);
    }
    match ruta.as_str() {
        "/api/chat" => {
            let t: String = datos["texto"].as_str().unwrap_or("").chars().take(4000).collect();
            let h = datos["historial"].as_array().cloned().unwrap_or_default();
            responder_json(req, &ia::charlar(&t, &h), 200)
        }
        "/api/instalar" if est.modo != "instalador" => responder_json(req, &json!({"ok": false, "error": "solo desde el pendrive"}), 403),
        "/api/instalar" => responder_json(req, &lanzar_instalacion(est, datos), 200),
        "/api/voz/grabar" => responder_json(req, &grabar_frase(datos["n"].as_i64().unwrap_or(1), est.modo == "instalador"), 200),
        "/api/reiniciar" => {
            if est.modo != "instalador" || corriendo(est) {
                return responder_json(req, &json!({"ok": false}), 403);
            }
            responder_json(req, &json!({"ok": true}), 200);
            std::thread::spawn(|| {
                std::thread::sleep(std::time::Duration::from_secs(1));
                let _ = Command::new("systemctl").arg("reboot").status();
            });
        }
        _ => responder_json(req, &json!({"error": "no existe"}), 404),
    }
}

/// Atiende con unos pocos hilos fijos (nada de hilos por conexión): poca memoria, sin recolector de basura.
pub fn servir(puerto: u16, est: Estado, hilos: usize) -> std::io::Result<()> {
    let server = Arc::new(Server::http(("127.0.0.1", puerto)).map_err(|e| std::io::Error::other(e.to_string()))?);
    let est = Arc::new(est);
    let mut hs = vec![];
    for _ in 0..hilos.max(1) {
        let (s, e) = (server.clone(), est.clone());
        hs.push(std::thread::spawn(move || {
            for req in s.incoming_requests() {
                atender(&e, req);
            }
        }));
    }
    for h in hs {
        let _ = h.join();
    }
    Ok(())
}
