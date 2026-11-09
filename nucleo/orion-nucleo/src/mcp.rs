//! orion-linux: el servidor MCP que le da a la IA el control del sistema (antes orion_mcp/server.py).
//!
//!   orion-nucleo mcp            (transporte stdio: una línea JSON-RPC por mensaje)
//!
//! Cualquier IA compatible con MCP (Claude, o un modelo local con un cliente MCP) se conecta aquí y trabaja «por
//! detrás». El catálogo (nombres, descripciones y esquemas) vive en config/mcp-herramientas.json. Todas las
//! herramientas corren en Rust; las de CAD llaman a su motor (build123d/CadQuery) como programa externo.

use crate::politica::{Politica, Riesgo};
use crate::util::{casa, correr, expandir};
use crate::{actualizar, cad, controladores, electrica, iconos, memoria, rendimiento, remoto, respaldo, telefono, voz};
use serde_json::{json, Map, Value};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

const CATALOGO: &str = include_str!("../../../config/mcp-herramientas.json");
pub const PROTOCOLO: &str = "2025-06-18";

/// Todas las herramientas del catálogo (la prueba revisa que no falte ninguna).
pub const NATIVAS: &[&str] = &[
    "sistema_info", "ejecutar", "leer_archivo", "escribir_archivo", "clonar_repo", "instalar", "rendimiento", "controladores",
    "electrica", "respaldo", "remoto", "casa", "actualizar", "apps", "telefono_microfono", "hud_mostrar", "memoria_anotar",
    "memoria_buscar", "memoria_contexto", "modulos", "voz_decir", "identidad", "traductor", "repos_buscar", "repos_verificar", "dictado", "cnc", "proyecto_crear",
    "proyecto_actualizar", "proyecto_exportar", "perfil_equipo", "colaborar", "cuaderno",
    "robot_orden", "plc_escribir", "camaras", "jugar", "navegador",
    "cad_construir", "cad_modificar", "forma_alternativas", "forma_foto_a_3d", "forma_integrar_servos",
];

pub struct Servidor {
    pub hogar: PathBuf,
    pub trabajo: PathBuf,
    pub politica: Politica,
    catalogo: Value,
    /// El motor CAD externo (`python3 -m orion_forma`); las pruebas ponen uno falso.
    pub cad_externo: fn(&str, &Value, &Path) -> Value,
    /// El robot queda cargado entre llamadas (pose, macros), como en Python.
    robot: std::sync::Mutex<Option<crate::movimiento::Orion>>,
}

fn txt<'a>(a: &'a Value, k: &str, d: &'a str) -> &'a str {
    a[k].as_str().unwrap_or(d)
}

fn num(a: &Value, k: &str, d: f64) -> f64 {
    a[k].as_f64().unwrap_or(d)
}

impl Servidor {
    pub fn new(hogar: PathBuf, trabajo: PathBuf) -> Self {
        for d in ["hud/inbox"] {
            let _ = std::fs::create_dir_all(hogar.join(d));
        }
        for d in ["cad", "repos", "forma"] {
            let _ = std::fs::create_dir_all(trabajo.join(d));
        }
        let mut s = Servidor { politica: Politica::new("libre", &trabajo, &hogar), hogar, trabajo, catalogo: serde_json::from_str(CATALOGO).unwrap(), cad_externo: cad::forma, robot: Default::default() };
        s.politica.modo = s.identidad()["permisos"].as_str().unwrap_or("libre").into();
        s
    }

    pub fn desde_entorno() -> Self {
        let hogar = std::env::var_os("ORION_HOME").map(PathBuf::from).unwrap_or_else(|| casa().join(".orion"));
        let trabajo = std::env::var_os("ORION_WORKSPACE").map(PathBuf::from).unwrap_or_else(|| casa().join("orion"));
        Self::new(hogar, trabajo)
    }

    pub fn identidad(&self) -> Value {
        std::fs::read_to_string(self.hogar.join("identidad.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(|| {
            json!({"nombre": "Neo Orión", "tono": "directo", "genero": "neutro", "color": "#cdf564", "permisos": "libre"})
        })
    }

    pub fn instrucciones(&self) -> String {
        let nombre = self.identidad()["nombre"].as_str().unwrap_or("Neo Orión").to_string();
        self.catalogo["instrucciones"].as_str().unwrap_or("").trim().replacen("Eres Neo Orión,", &format!("Eres {nombre},"), 1)
    }

    pub fn herramientas(&self) -> &Vec<Value> {
        self.catalogo["herramientas"].as_array().unwrap()
    }

    // ------------------------------------------------------------------ JSON-RPC

    /// Atiende un mensaje; devuelve la respuesta (None para notificaciones).
    pub fn atender(&self, m: &Value) -> Option<Value> {
        let id = m.get("id").cloned();
        let metodo = m["method"].as_str().unwrap_or("");
        let p = &m["params"];
        let r: Result<Value, (i64, String)> = match metodo {
            "initialize" => Ok(json!({
                "protocolVersion": p["protocolVersion"].as_str().unwrap_or(PROTOCOLO),
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "orion-linux", "version": env!("CARGO_PKG_VERSION")},
                "instructions": self.instrucciones(),
            })),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({"tools": self.herramientas()})),
            "tools/call" => {
                let nombre = p["name"].as_str().unwrap_or("");
                let args = if p["arguments"].is_object() { p["arguments"].clone() } else { json!({}) };
                match self.herramientas().iter().find(|h| h["name"] == nombre) {
                    None => Err((-32602, format!("herramienta desconocida: {nombre}"))),
                    Some(h) => {
                        let args = con_defectos(h, args);
                        let falta: Vec<&str> = h["inputSchema"]["required"].as_array().into_iter().flatten().filter_map(Value::as_str).filter(|k| args.get(*k).is_none()).collect();
                        if !falta.is_empty() {
                            Ok(json!({"content": [{"type": "text", "text": format!("faltan argumentos: {}", falta.join(", "))}], "isError": true}))
                        } else {
                            let v = self.llamar(nombre, &args);
                            let error = v["ok"] == false && v.get("error").is_some();
                            let texto = match &v { Value::String(s) => s.clone(), o => serde_json::to_string(o).unwrap() };
                            let mut res = json!({"content": [{"type": "text", "text": texto}], "isError": error});
                            if v.is_object() {
                                res["structuredContent"] = v;
                            }
                            Ok(res)
                        }
                    }
                }
            }
            "resources/list" => Ok(json!({"resources": []})),
            "prompts/list" => Ok(json!({"prompts": []})),
            m if m.starts_with("notifications/") => return None,
            _ => Err((-32601, format!("método desconocido: {metodo}"))),
        };
        let id = id?;
        Some(match r {
            Ok(v) => json!({"jsonrpc": "2.0", "id": id, "result": v}),
            Err((c, e)) => json!({"jsonrpc": "2.0", "id": id, "error": {"code": c, "message": e}}),
        })
    }

    pub fn servir_stdio(&self) {
        let entrada = std::io::stdin();
        let mut salida = std::io::stdout();
        for linea in entrada.lock().lines() {
            let Ok(linea) = linea else { break };
            if linea.trim().is_empty() {
                continue;
            }
            let resp = match serde_json::from_str::<Value>(&linea) {
                Ok(Value::Array(lote)) => {
                    let r: Vec<Value> = lote.iter().filter_map(|m| self.atender(m)).collect();
                    if r.is_empty() { None } else { Some(Value::Array(r)) }
                }
                Ok(m) => self.atender(&m),
                Err(_) => Some(json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": "JSON inválido"}})),
            };
            if let Some(r) = resp {
                let _ = writeln!(salida, "{r}");
                let _ = salida.flush();
            }
        }
    }

    // ------------------------------------------------------------------ herramientas

    pub fn llamar(&self, nombre: &str, a: &Value) -> Value {
        match nombre {
            "cad_construir" => {
                let r = cad::construir(&self.trabajo, txt(a, "nombre", ""), txt(a, "codigo", ""), &a["params"]);
                if r["ok"] == true {
                    self.politica.anotar("cad", &json!({"pieza": txt(a, "nombre", "")}), Riesgo::Escribir, "ok", json!({}));
                }
                r
            }
            "cad_modificar" => cad::modificar(&self.trabajo, txt(a, "nombre", ""), &a["cambios"]),
            "forma_alternativas" => (self.cad_externo)("alternativas", a, &self.trabajo),
            "forma_foto_a_3d" => (self.cad_externo)("foto_a_3d", a, &self.trabajo),
            "forma_integrar_servos" => (self.cad_externo)("integrar_servos", &json!({"step": a["step"], "servos": a["servos"]}), &self.trabajo),
            "sistema_info" => {
                let mut info = crate::perfil::escanear();
                let hw = crate::fit::detectar(None, None);
                info["ia_local"] = crate::fit::recomendar(&hw, &crate::fit::catalogo(), false);
                info
            }
            "ejecutar" => self.ejecutar(txt(a, "comando", ""), txt(a, "carpeta", ""), a["segundos"].as_u64().unwrap_or(300)),
            "leer_archivo" => {
                let max = a["max_bytes"].as_u64().unwrap_or(200_000) as usize;
                match std::fs::read(expandir(txt(a, "ruta", ""))) {
                    Ok(b) => Value::String(String::from_utf8_lossy(&b).chars().take(max).collect()),
                    Err(e) => json!({"ok": false, "error": e.to_string()}),
                }
            }
            "escribir_archivo" => self.escribir(txt(a, "ruta", ""), txt(a, "contenido", "")),
            "clonar_repo" => self.clonar(txt(a, "url", ""), txt(a, "nombre", "")),
            "instalar" => {
                let seguro = regex::Regex::new(r"^[\w.+:@/=-]+$").unwrap();
                let p: Vec<&str> = a["paquetes"].as_array().into_iter().flatten().filter_map(Value::as_str).filter(|x| seguro.is_match(x)).collect();
                let j = p.join(" ");
                let cmd = match txt(a, "gestor", "apt") {
                    "apt" => format!("sudo apt-get install -y {j}"),
                    "pip" => format!("pip install --user {j}"),
                    "flatpak" => format!("flatpak install -y flathub {j}"),
                    "npm" => format!("npm install -g {j}"),
                    "cargo" => format!("cargo install {j}"),
                    "ollama" => p.iter().map(|x| format!("ollama pull {x}")).collect::<Vec<_>>().join(" && "),
                    g => return json!({"ok": false, "error": format!("gestor desconocido: {g}")}),
                };
                self.ejecutar(&cmd, "", 3600)
            }
            "rendimiento" => {
                let mut extra = Map::new();
                if let Some(t) = a["tok_s"].as_f64() {
                    extra.insert("tok_s".into(), json!(t));
                }
                let m = rendimiento::medir(&extra);
                let (n, razones) = rendimiento::nivel_de(&m);
                json!({"nivel": n, "razones": razones, "acciones": rendimiento::acciones(n), "medicion": m})
            }
            "controladores" => match txt(a, "accion", "ver") {
                "al_dia" => controladores::revisar_al_dia_equipo(),
                "firmware" => controladores::firmware(),
                "instalar" => {
                    let d = controladores::detectar();
                    if let Some(g) = self.politica.compuerta("controladores", json!({"paquetes": d["plan"]["paquetes"], "pasos": d["plan"]["pasos"]}), Riesgo::Sistema, "instala drivers") {
                        return g;
                    }
                    let paq: Vec<&str> = d["plan"]["paquetes"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
                    let ok = paq.is_empty() || {
                        let mut c = vec!["sudo", "apt-get", "install", "-y"];
                        c.extend(&paq);
                        correr(&c).0 == 0
                    };
                    let nvidia = d["gpus"].as_array().is_some_and(|g| g.iter().any(|x| x["fabricante"] == "NVIDIA"));
                    if nvidia {
                        correr(&["sudo", "ubuntu-drivers", "install"]);
                    }
                    json!({"ok": ok, "plan": d["plan"], "reiniciar": nvidia})
                }
                _ => controladores::detectar(),
            },
            "electrica" => {
                let r = if !txt(a, "pregunta", "").is_empty() {
                    electrica::desde_frase(txt(a, "pregunta", "")).ok_or_else(String::new)
                } else {
                    electrica::seccion(&electrica::Pedido {
                        corriente_a: num(a, "corriente_a", 0.0),
                        largo_m: num(a, "largo_m", 0.0),
                        tension_v: num(a, "tension_v", 220.0),
                        fases: a["fases"].as_u64().unwrap_or(1) as u8,
                        material: txt(a, "material", "cobre"),
                        tipo: txt(a, "tipo", "subalimentador"),
                        limite_pct: None,
                        iz_a: a["iz_a"].as_f64(),
                        caida_previa_pct: num(a, "caida_previa_pct", 0.0),
                    })
                };
                match r {
                    Ok(r) => {
                        let mut v = serde_json::to_value(&r).unwrap();
                        v["ok"] = json!(true);
                        v["resumen"] = json!(r.texto());
                        v
                    }
                    Err(e) if !e.is_empty() => json!({"ok": false, "error": e}),
                    Err(_) => json!({"ok": false, "error": "di la corriente (A) o la potencia (W) y el largo en metros"}),
                }
            }
            "respaldo" => respaldo::accion(a),
            "remoto" => {
                let mut r = remoto::accion(a);
                if let Some(m) = r.as_object_mut() {
                    m.remove("clave");
                }
                r
            }
            "casa" => {
                let orden = txt(a, "orden", "");
                let r = crate::casa::Casa::cargar().ordenar(orden, false);
                if r["necesita_confirmar"] == true {
                    return self.politica.compuerta("casa", json!({"orden": orden, "entidad": r["entidad"], "servicio": r["servicio"]}), Riesgo::Fisico, r["motivo"].as_str().unwrap_or("")).unwrap_or(r);
                }
                r
            }
            "actualizar" => actualizar::accion(&json!({"accion": txt(a, "que", "revisar")})),
            "apps" => match txt(a, "accion", "listar") {
                "registrar" => iconos::registrar(txt(a, "id", ""), txt(a, "nombre", ""), txt(a, "categoria", ""), txt(a, "comando", ""), txt(a, "web", ""), txt(a, "icono", "")),
                "quitar" => iconos::quitar(txt(a, "id", "")),
                "abrir" => iconos::abrir(txt(a, "id", "")),
                "regla" => json!({"regla": iconos::regla(), "categorias": iconos::categorias()}),
                _ => json!({"apps": iconos::listar(false)}),
            },
            "telefono_microfono" => telefono::ordenar(txt(a, "accion", "iniciar")),
            "hud_mostrar" => self.hud_mostrar(txt(a, "tipo", "texto"), txt(a, "titulo", ""), &a["datos"]),
            "memoria_anotar" | "memoria_buscar" | "memoria_contexto" => {
                let m = match memoria::Memoria::abrir(None) {
                    Ok(m) => m,
                    Err(e) => return json!({"ok": false, "error": e.to_string()}),
                };
                match nombre {
                    "memoria_buscar" => json!(m.buscar(txt(a, "consulta", ""), a["n"].as_u64().unwrap_or(8) as usize)),
                    "memoria_contexto" => json!(m.contexto(txt(a, "consulta", ""), 6000)),
                    _ => {
                        let titulo = txt(a, "titulo", "");
                        let r = if titulo.is_empty() {
                            let tags: Vec<String> = a["etiquetas"].as_array().into_iter().flatten().filter_map(Value::as_str).map(|t| format!("#{t}")).collect();
                            let texto = if tags.is_empty() { txt(a, "texto", "").to_string() } else { format!("{} {}", txt(a, "texto", ""), tags.join(" ")) };
                            m.diario(&texto, "IA")
                        } else {
                            m.nota(titulo, txt(a, "texto", ""), txt(a, "carpeta", "temas"))
                        };
                        match r {
                            Ok(f) => json!({"ok": true, "archivo": f}),
                            Err(e) => json!({"ok": false, "error": e.to_string()}),
                        }
                    }
                }
            }
            "modulos" => self.modulos(),
            "traductor" => {
                let mut c = crate::traductor::Conversacion::new(txt(a, "idioma_a", "es"), txt(a, "idioma_b", "en"));
                c.decir = a["decir"].as_bool().unwrap_or(true);
                c.turno(txt(a, "texto", ""), None)
            }
            "repos_buscar" => match crate::repos::buscar(txt(a, "consulta", ""), a["n"].as_u64().unwrap_or(8) as usize) {
                Ok(l) => json!(l),
                Err(e) => json!({"ok": false, "error": e}),
            },
            "dictado" => {
                let datos = json!({"asignatura": a["asignatura"], "docente": a["docente"], "integrantes": a["integrantes"]});
                let mut r = crate::dictado::dictar(txt(a, "texto", ""), txt(a, "plantilla", "informe-duoc"), txt(a, "nombre", ""), &datos, a["pulir"] != false);
                if let Some(b) = r["bloques"].as_array().map(Vec::len) {
                    r["bloques"] = json!(b);
                }
                r
            }
            "cnc" => {
                let ac = txt(a, "accion", "");
                if ["mover", "casa", "iniciar", "cero"].contains(&ac) {
                    // FÍSICAS: la persona aprueba el ticket y la interfaz ejecuta
                    if ac == "iniciar" {
                        let r = crate::cnc::herramienta(&json!({"accion": "revisar", "archivo": a["archivo"]}), false);
                        if r["listo_para_cortar"] != true {
                            return json!({"ok": false, "errores": r["errores"].clone(), "error": r["error"].clone()});
                        }
                    }
                    let det = json!({"accion": ac, "archivo": a["archivo"], "x": a["x"], "y": a["y"], "z": a["z"], "puerto": a["puerto"]});
                    return self.politica.compuerta("cnc", det, Riesgo::Fisico, &format!("la CNC va a {ac}")).unwrap_or(json!({"ok": false}));
                }
                crate::cnc::herramienta(a, false)
            }
            "perfil_equipo" => crate::perfil::perfil(),
            "cuaderno" => crate::cuaderno::herramienta(a),
            "navegador" => crate::navegador::herramienta(a),
            "jugar" => crate::juegos::jugar(txt(a, "pedido", ""), a["hacerlo"] == true, |paso| self.ejecutar(paso, "", 3600)),
            "camaras" => {
                if txt(a, "accion", "preparar") != "preparar" {
                    return json!({"ok": false, "error": "accion: preparar"});
                }
                let cams = a["camaras"].as_array().cloned().filter(|c| !c.is_empty()).unwrap_or_else(|| vec![json!({"nombre": "Taller", "fuente": "usb"})]);
                crate::vision::preparar(&cams, txt(a, "detector", "cpu"), &crate::vision::carpeta())
            }
            "robot_orden" => {
                let mut g = self.robot.lock().unwrap();
                if g.is_none() {
                    let perfil = txt(a, "perfil", "");
                    let robot = if perfil.is_empty() { Ok(crate::movimiento::perfil_por_defecto()) } else { crate::movimiento::Robot::cargar(&expandir(perfil)) };
                    match robot {
                        Ok(r) => *g = Some(crate::movimiento::Orion::new(r, None, None, Some(&self.hogar.join("robots")))),
                        Err(e) => return json!({"ok": false, "error": e}),
                    }
                }
                let o = g.as_mut().unwrap();
                let lineas = o.decir(txt(a, "texto", ""));
                self.politica.anotar("robot", &json!({"orden": txt(a, "texto", "")}), Riesgo::Fisico, &lineas.join("; "), json!({}));
                json!({"resultado": lineas, "pose": o.robot.pose_json()})
            }
            "plc_escribir" => {
                // SIEMPRE espera confirmación humana: una línea real puede lastimar a alguien si arranca sola
                let det = json!({"ip": a["ip"], "area": a["area"], "direccion": a["direccion"], "valor": a["valor"]});
                self.politica.compuerta("plc_escribir", det, Riesgo::Fisico, txt(a, "motivo", "")).unwrap_or(json!({}))
            }
            "colaborar" => crate::puente::colaborar(txt(a, "tarea", ""), txt(a, "ia", ""), txt(a, "modo", "auto")),
            "proyecto_crear" => crate::proyectos::crear(a),
            "proyecto_actualizar" => crate::proyectos::actualizar(a),
            "proyecto_exportar" => crate::proyectos::exportar(txt(a, "id", ""), &self.trabajo),
            "repos_verificar" => {
                let r: Vec<String> = a["repos"].as_array().into_iter().flatten().filter_map(Value::as_str).map(String::from).collect();
                json!(crate::repos::verificar(&r))
            }
            "voz_decir" => {
                let a = voz::hablar(txt(a, "texto", ""), txt(a, "motor", "auto"), "", true);
                json!({"ok": a.is_some(), "bytes": a.map(|b| b.len()).unwrap_or(0)})
            }
            "identidad" => {
                let mut id = self.identidad();
                for (k, v) in a["cambios"].as_object().into_iter().flatten() {
                    if ["nombre", "tono", "genero", "color", "voz", "idioma", "saludo"].contains(&k.as_str()) {
                        id[k] = v.clone();
                    }
                }
                let _ = std::fs::write(self.hogar.join("identidad.json"), serde_json::to_string_pretty(&id).unwrap());
                id
            }
            otra => json!({"ok": false, "error": format!("herramienta sin implementar: {otra}")}),
        }
    }

    pub fn ejecutar(&self, comando: &str, carpeta: &str, segundos: u64) -> Value {
        let (r, motivo) = self.politica.clasificar_orden(comando);
        if let Some(g) = self.politica.compuerta("ejecutar", json!({"comando": comando}), r, motivo) {
            return g;
        }
        let cwd = if carpeta.is_empty() { self.trabajo.clone() } else { expandir(carpeta) };
        let t0 = std::time::Instant::now();
        let o = std::process::Command::new("timeout").arg(segundos.to_string()).args(["sh", "-c", comando]).current_dir(&cwd).stdin(std::process::Stdio::null()).output();
        let (codigo, sal, err) = match o {
            Ok(o) => (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stdout).to_string(), String::from_utf8_lossy(&o.stderr).to_string()),
            Err(e) => (-1, String::new(), e.to_string()),
        };
        self.politica.anotar("ejecutar", &json!({"comando": comando}), r, &format!("código {codigo}"), json!({}));
        let cola = |s: &str, n: usize| s.chars().rev().take(n).collect::<Vec<_>>().into_iter().rev().collect::<String>();
        json!({"codigo": codigo, "ok": codigo == 0, "salida": cola(&sal, 8000), "errores": cola(&err, 4000), "segundos": (t0.elapsed().as_secs_f64() * 10.0).round() / 10.0})
    }

    pub fn escribir(&self, ruta: &str, contenido: &str) -> Value {
        let p = expandir(ruta);
        let r = self.politica.clasificar_escritura(&p);
        if let Some(g) = self.politica.compuerta("escribir_archivo", json!({"ruta": p}), r, "") {
            return g;
        }
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if p.exists() {
            let mut bak = p.clone().into_os_string();
            bak.push(".bak");
            let _ = std::fs::copy(&p, bak);
        }
        if let Err(e) = std::fs::write(&p, contenido) {
            return json!({"ok": false, "error": e.to_string()});
        }
        self.politica.anotar("escribir_archivo", &json!({"ruta": p}), r, "ok", json!({"bytes": contenido.len()}));
        json!({"ok": true, "ruta": p, "bytes": contenido.len()})
    }

    pub fn clonar(&self, url: &str, nombre: &str) -> Value {
        let url = if regex::Regex::new(r"^(https?://|git@)").unwrap().is_match(url) { url.to_string() } else { format!("https://{}", url.trim_start_matches('/')) };
        let base = url.trim_end_matches('/').rsplit('/').next().unwrap_or("repo").trim_end_matches(".git").to_string();
        let dest = self.trabajo.join("repos").join(if nombre.is_empty() { &base } else { nombre });
        let d = dest.to_string_lossy().to_string();
        let o = if dest.exists() { correr(&["git", "-C", &d, "pull", "--ff-only"]) } else { correr(&["git", "clone", "--depth", "50", &url, &d]) };
        self.politica.anotar("clonar_repo", &json!({"url": url}), Riesgo::Sistema, &format!("código {}", o.0), json!({}));
        let archivos = contar(&dest);
        let log = format!("{}{}", o.1, o.2);
        json!({"ok": o.0 == 0, "ruta": dest, "archivos": archivos, "log": log.chars().rev().take(2000).collect::<Vec<_>>().into_iter().rev().collect::<String>()})
    }

    pub fn hud_mostrar(&self, tipo: &str, titulo: &str, datos: &Value) -> Value {
        let slug: String = regex::Regex::new(r"[^a-z0-9]+").unwrap().replace_all(&titulo.to_lowercase(), "-").chars().take(30).collect();
        let pid = format!("{}-{slug}", chrono::Local::now().format("%H%M%S"));
        let f = self.hogar.join("hud/inbox").join(format!("{pid}.json"));
        let _ = std::fs::create_dir_all(f.parent().unwrap());
        match std::fs::write(&f, json!({"id": pid, "tipo": tipo, "titulo": titulo, "datos": datos}).to_string()) {
            Ok(_) => json!({"ok": true, "panel": pid}),
            Err(e) => json!({"ok": false, "error": e.to_string()}),
        }
    }

    pub fn modulos(&self) -> Value {
        let candidatos = [PathBuf::from("/usr/share/orion/modulos.json"), PathBuf::from("config/modulos.json"), PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/modulos.json"))];
        let Some(m) = candidatos.iter().find_map(|p| std::fs::read_to_string(p).ok()).and_then(|s| serde_json::from_str::<Value>(&s).ok()) else {
            return json!({"ok": false, "error": "no encontré modulos.json"});
        };
        let mut out = Map::new();
        for (k, v) in m.as_object().into_iter().flatten().filter(|(k, _)| !k.starts_with('_')) {
            let instalado = if v["tipo"] == "mcp" && k == "orion-linux" {
                true
            } else {
                v["comando"].as_array().and_then(|c| c.last()).and_then(Value::as_str).map(|c| expandir(c).exists()).unwrap_or(false)
            };
            out.insert(k.clone(), json!({"nombre": v["nombre_visible"], "que_hace": v["descripcion_ia"].as_str().unwrap_or(""), "instalado": instalado}));
        }
        Value::Object(out)
    }
}

fn contar(d: &Path) -> usize {
    let mut n = 0;
    let mut pila = vec![d.to_path_buf()];
    while let Some(x) = pila.pop() {
        for e in std::fs::read_dir(&x).into_iter().flatten().flatten() {
            match e.file_type() {
                Ok(t) if t.is_dir() => pila.push(e.path()),
                Ok(t) if t.is_file() => n += 1,
                _ => {}
            }
        }
    }
    n
}

/// Completa los argumentos que faltan con los valores por defecto del esquema (como hacía Python).
fn con_defectos(h: &Value, mut args: Value) -> Value {
    for (k, p) in h["inputSchema"]["properties"].as_object().into_iter().flatten() {
        if args.get(k).is_none() {
            if let Some(d) = p.get("default") {
                args[k] = d.clone();
            }
        }
    }
    args
}

pub fn ejecutar() -> i32 {
    Servidor::desde_entorno().servir_stdio();
    0
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn cad_falso(orden: &str, a: &Value, _: &Path) -> Value {
        json!({"ok": true, "via": "cad", "orden": orden, "args": a})
    }

    fn servidor() -> (tempfile::TempDir, Servidor) {
        let d = tempfile::tempdir().unwrap();
        let mut s = Servidor::new(d.path().join(".orion"), d.path().join("orion"));
        s.cad_externo = cad_falso;
        (d, s)
    }

    fn rpc(s: &Servidor, id: i64, metodo: &str, p: Value) -> Value {
        s.atender(&json!({"jsonrpc": "2.0", "id": id, "method": metodo, "params": p})).unwrap()
    }

    #[test]
    fn catalogo_igual_al_de_python() {
        let (_d, s) = servidor();
        let h = s.herramientas();
        assert_eq!(h.len(), 43);
        for n in NATIVAS {
            assert!(h.iter().any(|x| x["name"] == *n), "{n} no está en el catálogo");
        }
        for x in h {
            assert!(NATIVAS.contains(&x["name"].as_str().unwrap()), "{} sin implementar", x["name"]);
        }
        let e = h.iter().find(|x| x["name"] == "ejecutar").unwrap();
        assert_eq!(e["inputSchema"]["required"], json!(["comando"]));
        assert_eq!(e["inputSchema"]["properties"]["segundos"]["default"], 300);
    }

    #[test]
    fn protocolo_mcp() {
        let (_d, s) = servidor();
        let i = rpc(&s, 1, "initialize", json!({"protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "prueba"}}));
        assert_eq!(i["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(i["result"]["serverInfo"]["name"], "orion-linux");
        assert!(i["result"]["instructions"].as_str().unwrap().contains("REGLA DE CÓDIGO"));
        assert!(s.atender(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).is_none());
        assert_eq!(rpc(&s, 2, "tools/list", json!({}))["result"]["tools"].as_array().unwrap().len(), 43);
        assert_eq!(rpc(&s, 3, "nada", json!({}))["error"]["code"], -32601);
        let r = rpc(&s, 4, "tools/call", json!({"name": "no_existe"}));
        assert_eq!(r["error"]["code"], -32602);
        let r = rpc(&s, 5, "tools/call", json!({"name": "ejecutar", "arguments": {}}));
        assert_eq!(r["result"]["isError"], true);
    }

    #[test]
    fn herramientas_nativas() {
        let (d, s) = servidor();
        // ejecutar en el espacio de trabajo
        let r = rpc(&s, 1, "tools/call", json!({"name": "ejecutar", "arguments": {"comando": "echo hola; pwd"}}));
        let v = &r["result"]["structuredContent"];
        assert_eq!(v["codigo"], 0);
        assert!(v["salida"].as_str().unwrap().starts_with("hola\n") && v["salida"].as_str().unwrap().trim_end().ends_with("/orion"));
        // lo irreversible queda en espera, no se ejecuta
        let r = s.llamar("ejecutar", &json!({"comando": "rm -rf / "}));
        assert_eq!(r["estado"], "en-espera");
        // escribir: con copia .bak
        let f = d.path().join("orion/nota.txt");
        let fs = f.to_string_lossy();
        assert_eq!(s.llamar("escribir_archivo", &json!({"ruta": fs, "contenido": "uno"}))["ok"], true);
        s.llamar("escribir_archivo", &json!({"ruta": fs, "contenido": "dos"}));
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "dos");
        assert_eq!(std::fs::read_to_string(d.path().join("orion/nota.txt.bak")).unwrap(), "uno");
        assert_eq!(s.llamar("leer_archivo", &json!({"ruta": fs, "max_bytes": 1})), json!("d"));
        // panel en el HUD
        let p = s.llamar("hud_mostrar", &json!({"tipo": "tabla", "titulo": "Presupuesto Taller", "datos": {"a": 1}}));
        let pid = p["panel"].as_str().unwrap();
        assert!(pid.ends_with("-presupuesto-taller"));
        assert!(d.path().join(format!(".orion/hud/inbox/{pid}.json")).exists());
        // identidad: solo los campos permitidos
        let id = s.llamar("identidad", &json!({"cambios": {"nombre": "Vega", "permisos": "asistido"}}));
        assert_eq!(id["nombre"], "Vega");
        assert_eq!(id["permisos"], "libre");
        assert!(s.instrucciones().starts_with("Eres Vega,"));
        // calculadora eléctrica
        let e = s.llamar("electrica", &json!({"pregunta": "20 A a 35 m"}));
        assert_eq!(e["ok"], true);
        assert!(e["resumen"].as_str().unwrap().contains("mm²"));
        let e = s.llamar("electrica", &json!({"corriente_a": 20.0, "largo_m": 35.0, "tension_v": 220.0, "fases": 1, "tipo": "subalimentador", "material": "cobre", "caida_previa_pct": 0}));
        assert_eq!(e["ok"], true);
        // apps: la regla
        assert!(s.llamar("apps", &json!({"accion": "regla"}))["regla"].as_str().unwrap().contains("9"));
    }

    #[test]
    fn cad_externo_y_ordenes_fisicas() {
        let (_d, s) = servidor();
        let r = rpc(&s, 1, "tools/call", json!({"name": "forma_alternativas", "arguments": {"descripcion": "soporte"}}));
        assert_eq!(r["result"]["structuredContent"]["orden"], "alternativas");
        assert_eq!(r["result"]["structuredContent"]["args"]["n"], 3);
        // la CNC: mover siempre deja un ticket para la persona
        assert_eq!(s.llamar("cnc", &json!({"accion": "mover", "x": 10.0, "puerto": "/dev/ttyUSB0"}))["estado"], "en-espera");
        // el robot recuerda la pose entre llamadas; el PLC siempre espera a la persona
        s.llamar("robot_orden", &json!({"texto": "hombro a 45 grados"}));
        let r = s.llamar("robot_orden", &json!({"texto": "codo a 10 grados"}));
        assert_eq!(r["pose"]["A2"], 45.0);
        assert_eq!(r["resultado"][0], "[OK ] SET A3 10 vel=150");
        assert_eq!(s.llamar("plc_escribir", &json!({"ip": "10.0.0.5", "area": "DB", "direccion": "1.0", "valor": "1", "motivo": "arrancar cinta"}))["estado"], "en-espera");
    }
}
