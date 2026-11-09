//! Íconos y lista de aplicaciones con la REGLA ESTÉTICA de Neo Orión (9×9 bloques, un color, ninguno se repite).
//! Los datos (categorías, íconos propios, programas conocidos) vienen de `config/iconos.json`, fuente única.

use crate::util::{casa, existe_programa, sin_tilde};
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::LazyLock;

pub const LADO: usize = 9;
static DATOS: LazyLock<Value> = LazyLock::new(|| serde_json::from_str(include_str!("../../../config/iconos.json")).expect("iconos.json"));

fn mapa(k: &str) -> &'static Map<String, Value> {
    DATOS[k].as_object().expect("objeto")
}
pub fn regla() -> &'static str {
    DATOS["regla"].as_str().unwrap_or("")
}
pub fn categoria(c: &str) -> &'static str {
    mapa("categorias").get(c).and_then(Value::as_str).unwrap_or_else(|| mapa("categorias")["app"].as_str().unwrap())
}
pub fn propio(id: &str) -> Option<&'static str> {
    mapa("propios").get(id).and_then(Value::as_str)
}
pub fn categorias() -> Vec<&'static str> {
    mapa("categorias").keys().map(String::as_str).collect()
}
pub fn conocidas() -> &'static Vec<Value> {
    DATOS["conocidas"].as_array().expect("lista")
}

pub fn norm_cat(c: &str) -> String {
    let c = sin_tilde(c.trim());
    if mapa("categorias").contains_key(&c) {
        return c;
    }
    mapa("sinonimos").get(&c).and_then(Value::as_str).unwrap_or("app").to_string()
}

/// Reglas que NO cumple el ícono (vacía = válido).
pub fn validar(icono: &str) -> Vec<String> {
    let filas: Vec<Vec<char>> = icono.split('|').map(|f| f.chars().collect()).collect();
    if filas.len() != LADO || filas.iter().any(|f| f.len() != LADO) {
        return vec![format!("1 · debe ser de {LADO}×{LADO} (filas de {LADO} caracteres separadas por «|»)")];
    }
    if filas.iter().flatten().any(|c| *c != '#' && *c != '.') {
        return vec!["1 · solo «#» (bloque) y «.» (vacío)".into()];
    }
    let mut errores = vec![];
    let llenos: BTreeSet<(i32, i32)> =
        filas.iter().enumerate().flat_map(|(y, f)| f.iter().enumerate().filter(|(_, c)| **c == '#').map(move |(x, _)| (y as i32, x as i32))).collect();
    let p = llenos.len() as f64 / (LADO * LADO) as f64;
    if !(0.20..=0.65).contains(&p) {
        errores.push(format!("3 · lleno {:.0}%: debe estar entre 20 % y 65 %", p * 100.0));
    }
    let sueltos: Vec<_> = llenos
        .iter()
        .filter(|(y, x)| !(-1..=1).any(|dy| (-1..=1).any(|dx| (dy != 0 || dx != 0) && llenos.contains(&(y + dy, x + dx)))))
        .collect();
    if let Some((y, x)) = sueltos.first() {
        errores.push(format!("4 · {} bloque(s) suelto(s), p. ej. fila {} columna {}", sueltos.len(), y + 1, x + 1));
    }
    let filas_usadas: BTreeSet<_> = llenos.iter().map(|(y, _)| y).collect();
    let cols_usadas: BTreeSet<_> = llenos.iter().map(|(_, x)| x).collect();
    if filas_usadas.len() < 6 || cols_usadas.len() < 6 {
        errores.push("5 · debe ocupar al menos 6 filas y 6 columnas".into());
    }
    errores
}

pub fn sigla(nombre: &str) -> String {
    let limpio: String = nombre.chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect();
    let p: Vec<&str> = limpio.split_whitespace().collect();
    let s: String = match p.as_slice() {
        [] => "AP".into(),
        [uno] => uno.chars().take(2).collect(),
        [a, b, ..] => format!("{}{}", a.chars().next().unwrap(), b.chars().next().unwrap()),
    };
    s.to_uppercase()
}

/// Ícono de respaldo: la sigla en bloques entre dos barras.
pub fn monograma(sg: &str) -> String {
    let letras = mapa("letras");
    let base = format!("{sg}AP");
    let mut it = base.chars().take(2).map(|c| letras.get(&c.to_string()).or_else(|| letras.get("A")).and_then(Value::as_str).unwrap().split('|').collect::<Vec<_>>());
    let (a, b) = (it.next().unwrap(), it.next().unwrap());
    let mut f = vec!["#########".to_string(), ".........".to_string()];
    f.extend((0..5).map(|i| format!("{}.{}", a[i], b[i])));
    f.extend([".........".to_string(), "#########".to_string()]);
    f.join("|")
}

fn archivo() -> PathBuf {
    casa().join(".orion/apps.json")
}
pub fn propias() -> Vec<Value> {
    std::fs::read_to_string(archivo()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}
fn guardar(lista: &[Value]) -> std::io::Result<()> {
    std::fs::create_dir_all(archivo().parent().unwrap())?;
    std::fs::write(archivo(), serde_json::to_string_pretty(lista)? + "\n")
}

fn usados(excepto: &str) -> BTreeSet<String> {
    let mut u: BTreeSet<String> = mapa("propios").values().filter_map(|v| v.as_str().map(String::from)).collect();
    u.extend(propias().iter().filter(|a| a["id"] != excepto).filter_map(|a| a["icono"].as_str().map(String::from)));
    u
}

pub fn registrar(id: &str, nombre: &str, cat: &str, comando: &str, web: &str, icono: &str) -> Value {
    let id_ok = !id.is_empty() && id.len() <= 41 && id.chars().next().is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "._-".contains(c));
    if !id_ok {
        return json!({"ok": false, "error": "id: minúsculas, números, punto, guion (máx. 41)"});
    }
    if comando.is_empty() && web.is_empty() {
        return json!({"ok": false, "error": "falta cómo abrirlo: comando o web"});
    }
    if !web.is_empty() && !(web.starts_with("http://") || web.starts_with("https://")) {
        return json!({"ok": false, "error": "la web debe empezar con http:// o https://"});
    }
    let cat = norm_cat(cat);
    let ocupados = usados(id);
    let icono = if !icono.is_empty() {
        let mal = validar(icono);
        if !mal.is_empty() {
            return json!({"ok": false, "error": "el ícono no cumple la regla estética", "reglas": mal, "regla": regla()});
        }
        if ocupados.contains(icono) {
            return json!({"ok": false, "error": "8 · ese ícono ya lo usa otro programa: dibuja uno distinto", "regla": regla()});
        }
        icono.to_string()
    } else {
        let mut ic = if ocupados.contains(categoria(&cat)) { monograma(&sigla(if nombre.is_empty() { id } else { nombre })) } else { categoria(&cat).to_string() };
        let mut n = 0;
        while ocupados.contains(&ic) && n < 40 {
            n += 1;
            let mut f: Vec<String> = ic.split('|').map(String::from).collect();
            f[8] = "#".repeat(9 - n % 9) + &".".repeat(n % 9);
            ic = f.join("|");
        }
        ic
    };
    let nom: String = nombre.trim().chars().take(24).collect();
    let mut reg = json!({"id": id, "nombre": if nom.is_empty() { id.to_string() } else { nom }, "categoria": cat,
                         "sigla": sigla(if nombre.is_empty() { id } else { nombre }), "icono": icono});
    if !comando.is_empty() {
        reg["comando"] = comando.into();
    }
    if !web.is_empty() {
        reg["web"] = web.into();
    }
    let mut lista: Vec<Value> = propias().into_iter().filter(|a| a["id"] != id).collect();
    lista.push(reg.clone());
    if let Err(e) = guardar(&lista) {
        return json!({"ok": false, "error": e.to_string()});
    }
    reg["ok"] = true.into();
    reg
}

pub fn quitar(id: &str) -> Value {
    let lista = propias();
    let nueva: Vec<Value> = lista.iter().filter(|a| a["id"] != id).cloned().collect();
    let ok = nueva.len() < lista.len();
    let _ = guardar(&nueva);
    json!({"ok": ok})
}

fn instalada(a: &Value) -> bool {
    if let Some(fp) = a["flatpak"].as_str() {
        return std::process::Command::new("flatpak").args(["info", fp]).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().map(|s| s.success()).unwrap_or(false);
    }
    let clave = a["cuando"].as_str().map(String::from).or_else(|| a["comando"].as_str().and_then(|c| shlex::split(c)).and_then(|v| v.into_iter().next())).unwrap_or_default();
    if clave.is_empty() { a["web"].is_string() } else { existe_programa(&clave) }
}

/// Apps para el HUD: las conocidas que estén instaladas + las registradas.
pub fn listar(todas: bool) -> Vec<Value> {
    let prop = propias();
    let ids: BTreeSet<String> = prop.iter().filter_map(|a| a["id"].as_str().map(String::from)).collect();
    let mut salida = vec![];
    for a in conocidas() {
        let id = a["id"].as_str().unwrap_or("");
        if ids.contains(id) {
            continue;
        }
        let inst = instalada(a);
        if todas || inst {
            let mut x = a.clone();
            x["sigla"] = sigla(a["nombre"].as_str().unwrap_or(id)).into();
            x["icono"] = propio(id).unwrap_or_else(|| categoria(a["categoria"].as_str().unwrap_or("app"))).into();
            x["instalada"] = inst.into();
            salida.push(x);
        }
    }
    for mut a in prop {
        let ok = a["icono"].as_str().is_some_and(|i| validar(i).is_empty());
        if !ok {
            a["icono"] = categoria(&norm_cat(a["categoria"].as_str().unwrap_or(""))).into();
        }
        a["instalada"] = true.into();
        salida.push(a);
    }
    salida
}

/// Abre una app de la lista (solo de la lista: nada arbitrario).
pub fn abrir(id: &str) -> Value {
    let Some(a) = listar(true).into_iter().find(|x| x["id"] == id) else {
        return json!({"ok": false, "error": format!("no conozco la app «{id}»")});
    };
    let cmd: Vec<String> = match (a["comando"].as_str(), a["web"].as_str()) {
        (None, Some(w)) => vec!["xdg-open".into(), w.into()],
        (Some(c), _) => shlex::split(&c.replace("{casa}", &casa().to_string_lossy())).unwrap_or_default(),
        _ => vec![],
    };
    if id == "nube" {
        let _ = std::fs::create_dir_all(casa().join("Nube"));
    }
    let Some((p, args)) = cmd.split_first() else { return json!({"ok": false, "error": "sin comando"}) };
    match std::process::Command::new(p).args(args).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn() {
        Ok(_) => json!({"ok": true, "abierta": a["nombre"]}),
        Err(e) => json!({"ok": false, "error": format!("no se pudo abrir {}: {e}", a["nombre"].as_str().unwrap_or(id))}),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn todos_los_iconos_cumplen_y_ninguno_se_repite() {
        let mut todos: Vec<&str> = mapa("categorias").values().filter_map(Value::as_str).collect();
        todos.extend(mapa("propios").values().filter_map(Value::as_str));
        for i in &todos {
            assert!(validar(i).is_empty(), "{i}: {:?}", validar(i));
        }
        assert_eq!(todos.iter().collect::<BTreeSet<_>>().len(), todos.len());
        let ids: BTreeSet<&str> = conocidas().iter().filter_map(|a| a["id"].as_str()).collect();
        assert_eq!(ids, mapa("propios").keys().map(String::as_str).collect());
        for a in mapa("letras").keys() {
            for b in mapa("letras").keys() {
                assert!(validar(&monograma(&format!("{a}{b}"))).is_empty());
            }
        }
    }

    #[test]
    fn reglas_y_siglas() {
        assert!(!validar("###|###").is_empty());
        let suelto = ["#########", "#.......#", "#.#.....#", "#.......#", "#.......#", "#.......#", "#.......#", "#.......#", "#########"].join("|");
        assert!(validar(&suelto).iter().any(|e| e.starts_with('4')));
        assert_eq!(norm_cat("Películas"), "video");
        assert_eq!(norm_cat("algo raro"), "app");
        assert_eq!((sigla("GeForce Now"), sigla("steam")), ("GN".to_string(), "ST".to_string()));
    }

    #[test]
    fn registrar_sin_repetir() {
        let d = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", d.path());
        let r = registrar("jellyfin-casa", "Series de la casa", "series", "", "http://192.168.1.5:8096", "");
        assert_eq!(r["ok"], true);
        assert_eq!(r["icono"], categoria("video"));
        let otra = registrar("plex", "Mi Plex", "series", "plex", "", "");
        assert_eq!(otra["icono"], monograma("MP"));
        let gemela = registrar("plex2", "Mi Plex 2", "series", "plex2", "", "");
        assert!(gemela["icono"] != otra["icono"] && gemela["icono"] != r["icono"]);
        assert_eq!(registrar("copia", "Copia", "foto", "c", "", propio("steam").unwrap())["ok"], false);
        assert_eq!(registrar("x y", "X", "app", "x", "", "")["ok"], false);
        assert!(listar(false).iter().any(|a| a["id"] == "jellyfin-casa"));
        assert_eq!(quitar("plex")["ok"], true);
        assert_eq!(abrir("no-existe")["ok"], false);
    }
}
