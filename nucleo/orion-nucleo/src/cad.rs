//! CAD por código: lo único de Neo Orión que sigue en Python, porque los motores CAD libres (build123d y CadQuery
//! sobre OpenCascade) lo son. El núcleo los trata como a cualquier programa externo (igual que a Blender o KiCad):
//! escribe la pieza, la corre y lee el resultado. No hay ningún Python de fondo.
//!
//!  · cad_construir / cad_modificar: pieza paramétrica en ~/orion/cad/<nombre>.py → STEP + STL a escala real
//!  · forma (foto/idea → 3D → CAD → servos): `python3 -m orion_forma <orden>` (en /opt/orion)

use regex::Regex;
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const PLANTILLA: &str = r#""""Modelo paramétrico generado por Neo Orión. Edita PARAMS o pídelo por voz."""
from build123d import *
PARAMS = {params}

{body}

if __name__ == "__main__":
    import json, sys
    out = sys.argv[1]
    export_step(pieza, out + ".step")
    export_stl(pieza, out + ".stl")
    bb = pieza.bounding_box()
    print(json.dumps({"x": round(bb.size.X, 2), "y": round(bb.size.Y, 2), "z": round(bb.size.Z, 2),
                      "volumen_cm3": round(pieza.volume / 1000, 2)}))
"#;

/// El Python con los motores CAD (el entorno de Neo Orión si existe).
pub fn python() -> String {
    if Path::new("/opt/orion/venv/bin/python3").exists() { "/opt/orion/venv/bin/python3".into() } else { "python3".into() }
}

/// Dónde está orion_forma.
pub fn raiz_forma() -> String {
    std::env::var("ORION_PY").unwrap_or_else(|_| if Path::new("/opt/orion/orion_forma").is_dir() { "/opt/orion".into() } else { "orion".into() })
}

fn json_python(v: &Value) -> String {
    // json.dumps(params, indent=4) de Python: cuatro espacios y «, » / «: »
    let mut b = Vec::new();
    let f = serde_json::ser::PrettyFormatter::with_indent(b"    ");
    let mut s = serde_json::Serializer::with_formatter(&mut b, f);
    serde::Serialize::serialize(v, &mut s).unwrap();
    String::from_utf8(b).unwrap()
}

pub fn construir_desde(trabajo: &Path, nombre: &str) -> Value {
    let src = trabajo.join("cad").join(format!("{nombre}.py"));
    let out = trabajo.join("cad").join(nombre);
    let o = Command::new(python()).arg(&src).arg(&out).stdin(Stdio::null()).output();
    let o = match o {
        Ok(o) => o,
        Err(e) => return json!({"ok": false, "error": format!("no pude correr Python para el CAD: {e}")}),
    };
    if !o.status.success() {
        let e = String::from_utf8_lossy(&o.stderr);
        return json!({"ok": false, "error": e.chars().rev().take(3000).collect::<Vec<_>>().into_iter().rev().collect::<String>()});
    }
    let medidas: Value = String::from_utf8_lossy(&o.stdout).trim().lines().last().and_then(|l| serde_json::from_str(l).ok()).unwrap_or(Value::Null);
    json!({"ok": true, "medidas_mm": medidas, "step": format!("{}.step", out.display()), "stl": format!("{}.stl", out.display()), "fuente": src})
}

/// Crea una pieza 3D por código (build123d) y exporta STEP y STL. `codigo` usa PARAMS[...] y deja `pieza`.
pub fn construir(trabajo: &Path, nombre: &str, codigo: &str, params: &Value) -> Value {
    if !Regex::new(r"^[\w-]+$").unwrap().is_match(nombre) {
        return json!({"ok": false, "error": "nombre de pieza inválido (letras, números, - y _)"});
    }
    let src = trabajo.join("cad").join(format!("{nombre}.py"));
    let _ = std::fs::create_dir_all(src.parent().unwrap());
    if let Err(e) = std::fs::write(&src, PLANTILLA.replace("{params}", &json_python(params)).replace("{body}", codigo)) {
        return json!({"ok": false, "error": e.to_string()});
    }
    construir_desde(trabajo, nombre)
}

/// Cambia parámetros de una pieza existente y la regenera. Devuelve también qué cambió (antes → ahora).
pub fn modificar(trabajo: &Path, nombre: &str, cambios: &Value) -> Value {
    let src = trabajo.join("cad").join(format!("{nombre}.py"));
    let Ok(texto) = std::fs::read_to_string(&src) else { return json!({"ok": false, "error": format!("no existe la pieza {nombre}")}) };
    let Some(m) = Regex::new(r"(?s)PARAMS = (\{.*?\n\})").unwrap().captures(&texto).map(|c| c[1].to_string()) else {
        return json!({"ok": false, "error": "la pieza no tiene PARAMS"});
    };
    let Ok(mut params) = serde_json::from_str::<Value>(&m) else { return json!({"ok": false, "error": "PARAMS no es JSON válido"}) };
    let antes = params.clone();
    for (k, v) in cambios.as_object().into_iter().flatten() {
        params[k] = v.clone();
    }
    if let Err(e) = std::fs::write(&src, texto.replace(&m, &json_python(&params))) {
        return json!({"ok": false, "error": e.to_string()});
    }
    let mut r = construir_desde(trabajo, nombre);
    r["cambios"] = Value::Object(cambios.as_object().into_iter().flatten().map(|(k, v)| (k.clone(), json!([antes.get(k), v]))).collect());
    r
}

/// La cadena foto/idea → 3D → CAD → servos (`python3 -m orion_forma <orden>`).
pub fn forma(orden: &str, args: &Value, trabajo: &Path) -> Value {
    let hijo = Command::new(python())
        .args(["-m", "orion_forma", orden])
        .env("PYTHONPATH", raiz_forma())
        .env("ORION_WORKSPACE", trabajo)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let Ok(mut h) = hijo else { return json!({"ok": false, "error": "falta Python para el CAD (sudo apt install python3)"}) };
    if let Some(mut e) = h.stdin.take() {
        let _ = e.write_all(args.to_string().as_bytes());
    }
    match h.wait_with_output() {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().lines().last().and_then(|l| serde_json::from_str(l).ok()).unwrap_or_else(|| json!({"ok": false, "error": "respuesta inválida del CAD"})),
        Ok(o) => {
            let e = String::from_utf8_lossy(&o.stderr);
            let ultima = e.lines().last().unwrap_or("el CAD falló");
            let instalar = if ultima.contains("No module named") { "Instala el CAD: /opt/orion/venv/bin/pip install build123d" } else { "" };
            json!({"ok": false, "error": ultima.chars().take(300).collect::<String>(), "instalar": instalar})
        }
        Err(e) => json!({"ok": false, "error": e.to_string()}),
    }
}

pub fn trabajo_por_defecto() -> PathBuf {
    std::env::var_os("ORION_WORKSPACE").map(PathBuf::from).unwrap_or_else(|| crate::util::casa().join("orion"))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn plantilla_y_cambios() {
        let d = tempfile::tempdir().unwrap();
        // sin build123d instalado igual se escribe la pieza y se informa el error de Python
        let r = construir(d.path(), "soporte_servo", "pieza = Box(PARAMS['largo'], 20, PARAMS['grosor'])", &json!({"largo": 40, "grosor": 5}));
        let src = std::fs::read_to_string(d.path().join("cad/soporte_servo.py")).unwrap();
        assert!(src.contains("PARAMS = {\n    \"largo\": 40,\n    \"grosor\": 5\n}") && src.contains("pieza = Box("));
        assert!(r["ok"] == true || r["error"].as_str().unwrap().contains("build123d"), "{r}");
        let m = modificar(d.path(), "soporte_servo", &json!({"grosor": 3}));
        assert_eq!(m["cambios"]["grosor"], json!([5, 3]));
        assert!(std::fs::read_to_string(d.path().join("cad/soporte_servo.py")).unwrap().contains("\"grosor\": 3"));
        assert_eq!(construir(d.path(), "../fuera", "", &json!({}))["ok"], false);
        assert_eq!(modificar(d.path(), "no_existe", &json!({}))["ok"], false);
    }
}
