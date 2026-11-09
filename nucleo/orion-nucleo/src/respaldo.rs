//! Respaldo automático: memoria (~/Orion), proyectos y configuración. Destinos: disco externo (se detecta solo),
//! una carpeta, o la nube con rclone (la cuenta la conecta la persona con `rclone config`; la clave no pasa por aquí).
//! Cada respaldo local es una foto con fecha hecha con rsync --link-dest (lo que no cambió se enlaza): se guardan 30.

use crate::util::{expandir, Ejecutor};
use serde_json::{json, Value};
use std::path::PathBuf;

pub const QUE: [&str; 4] = ["~/Orion", "~/Proyectos", "~/.orion", "/etc/orion"];
pub const EXCLUIR: [&str; 7] = [".orion/indice.db-wal", "*.tmp", "__pycache__", ".cache", "node_modules", "chrome-perfil", "firefox-perfil"];
pub const CONSERVAR: usize = 30;

fn ruta_config() -> PathBuf {
    crate::util::casa().join(".orion/respaldo.json")
}
pub fn configuracion() -> Value {
    std::fs::read_to_string(ruta_config()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(|| json!({"destino": "auto", "que": QUE, "cada": "diario"}))
}

/// Discos extraíbles montados con espacio libre (pendrive, disco USB).
pub fn discos_externos_desde(lsblk: &str) -> Vec<Value> {
    let d: Value = serde_json::from_str(lsblk).unwrap_or_default();
    let mut out = vec![];
    fn rec(n: &Value, ext: bool, out: &mut Vec<Value>) {
        let ext = ext || n["rm"] == true || n["hotplug"] == true || n["tran"] == "usb";
        if let Some(mp) = n["mountpoint"].as_str() {
            if ext && !["/", "/boot", "/boot/efi"].contains(&mp) && !mp.starts_with("/cdrom") && !mp.starts_with("/snap") {
                let libre = n["fsavail"].as_u64().or_else(|| n["fsavail"].as_str().and_then(|s| s.parse().ok())).unwrap_or(0);
                out.push(json!({"montaje": mp, "libre_gb": crate::util::redondear(libre as f64 / 1e9, 1), "etiqueta": n["label"].as_str().or(n["name"].as_str()).unwrap_or("")}));
            }
        }
        for h in n["children"].as_array().into_iter().flatten() {
            rec(h, ext, out);
        }
    }
    for n in d["blockdevices"].as_array().into_iter().flatten() {
        rec(n, false, &mut out);
    }
    out.sort_by(|a, b| b["libre_gb"].as_f64().partial_cmp(&a["libre_gb"].as_f64()).unwrap());
    out
}
pub fn discos_externos() -> Vec<Value> {
    discos_externos_desde(&crate::util::sh(&["lsblk", "-J", "-b", "-o", "NAME,RM,HOTPLUG,MOUNTPOINT,FSAVAIL,LABEL,TRAN"]))
}

pub fn destino_local(c: &Value) -> Option<PathBuf> {
    match c["destino"].as_str().unwrap_or("auto") {
        "auto" => discos_externos().first().and_then(|d| d["montaje"].as_str()).map(|m| PathBuf::from(m).join("NeoOrion-respaldo")),
        d if d.starts_with("rclone:") => None,
        d => Some(expandir(d)),
    }
}

fn fotos(base: &std::path::Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(base).into_iter().flatten().flatten().filter(|e| e.path().is_dir()).map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.len() >= 4 && n[..4].chars().all(|c| c.is_ascii_digit())).collect();
    v.sort();
    v
}

pub fn respaldar(c: &Value, sello: &str, ejecutar: Ejecutor) -> Value {
    let fuentes: Vec<PathBuf> = c["que"].as_array().map(|q| q.iter().filter_map(Value::as_str).map(expandir).collect::<Vec<PathBuf>>()).unwrap_or_else(|| QUE.iter().map(|q| expandir(q)).collect()).into_iter().filter(|f| f.exists()).collect();
    if fuentes.is_empty() {
        return json!({"ok": false, "error": "no hay nada que respaldar todavía"});
    }
    let excluir: Vec<&str> = EXCLUIR.iter().flat_map(|e| ["--exclude", e]).collect();
    let nombre = |f: &PathBuf| f.file_name().unwrap().to_string_lossy().trim_start_matches('.').to_string();
    let d = c["destino"].as_str().unwrap_or("auto");
    if let Some(remoto) = d.strip_prefix("rclone:") {
        if !crate::util::existe_programa("rclone") && ejecutar(&["rclone", "version"]).0 != 0 {
            return json!({"ok": false, "error": "falta rclone: sudo apt install rclone y luego `rclone config` para conectar tu nube"});
        }
        let remoto = remoto.trim_end_matches('/');
        let hechos: Vec<Value> = fuentes.iter().map(|f| {
            let (actual, antes) = (format!("{remoto}/actual/{}", nombre(f)), format!("{remoto}/anteriores/{sello}/{}", nombre(f)));
            let mut cmd = vec!["rclone", "sync", f.to_str().unwrap(), &actual, "--backup-dir", &antes];
            cmd.extend(&excluir);
            json!({"carpeta": f, "ok": ejecutar(&cmd).0 == 0})
        }).collect();
        return json!({"ok": hechos.iter().all(|h| h["ok"] == true), "destino": d, "sello": sello, "carpetas": hechos});
    }
    let Some(base) = destino_local(c) else { return json!({"ok": false, "error": "no hay disco externo conectado; conecta uno o elige una carpeta o tu nube"}) };
    if let Err(e) = std::fs::create_dir_all(base.join(sello)) {
        return json!({"ok": false, "error": e.to_string()});
    }
    let previos: Vec<String> = fotos(&base).into_iter().filter(|x| x != sello).collect();
    let hechos: Vec<Value> = fuentes.iter().map(|f| {
        let destino = base.join(sello).join(nombre(f));
        let link = previos.last().map(|p| base.join(p).join(nombre(f)).to_string_lossy().to_string());
        let origen = format!("{}/", f.display());
        let mut cmd = vec!["rsync", "-a", "--delete"];
        cmd.extend(&excluir);
        if let Some(l) = &link {
            cmd.extend(["--link-dest", l.as_str()]);
        }
        let dest = destino.to_string_lossy().to_string();
        cmd.extend([origen.as_str(), dest.as_str()]);
        let (codigo, _, err) = ejecutar(&cmd);
        json!({"carpeta": f, "ok": codigo == 0, "error": if codigo != 0 { err.chars().rev().take(200).collect::<Vec<_>>().into_iter().rev().collect::<String>() } else { String::new() }})
    }).collect();
    // quedan CONSERVAR fotos contando la nueva
    for viejo in previos.iter().take(previos.len().saturating_sub(CONSERVAR - 1)) {
        let _ = std::fs::remove_dir_all(base.join(viejo));
    }
    let _ = std::fs::write(base.join("ULTIMO.txt"), format!("{sello}\n"));
    json!({"ok": hechos.iter().all(|h| h["ok"] == true), "destino": base.join(sello), "sello": sello, "carpetas": hechos})
}

pub fn listar(c: &Value) -> Vec<String> {
    destino_local(c).filter(|b| b.exists()).map(|b| fotos(&b).into_iter().rev().collect()).unwrap_or_default()
}

/// Trae de vuelta una carpeta de un respaldo, por defecto a ~/Restaurado/<sello>/ (no pisa lo actual).
pub fn restaurar(sello: &str, carpeta: &str, a: Option<&str>, c: &Value, ejecutar: Ejecutor) -> Value {
    let Some(base) = destino_local(c) else { return json!({"ok": false, "error": "no encuentro el disco del respaldo"}) };
    let origen = base.join(sello).join(carpeta.trim_start_matches('.'));
    if !origen.exists() {
        return json!({"ok": false, "error": format!("no existe {}", origen.display())});
    }
    let destino = a.map(expandir).unwrap_or_else(|| crate::util::casa().join("Restaurado").join(sello).join(carpeta.trim_start_matches('.')));
    let _ = std::fs::create_dir_all(&destino);
    let (o, d) = (format!("{}/", origen.display()), destino.to_string_lossy().to_string());
    json!({"ok": ejecutar(&["rsync", "-a", &o, &d]).0 == 0, "en": destino})
}

pub fn ahora() -> String {
    chrono::Local::now().format("%Y-%m-%d_%H%M").to_string()
}

pub fn accion(d: &Value) -> Value {
    let c = configuracion();
    match d["accion"].as_str().unwrap_or("") {
        "ver" => json!({"ok": true, "respaldos": listar(&c), "discos": discos_externos(), "config": c}),
        "restaurar" => restaurar(d["sello"].as_str().unwrap_or(""), d["carpeta"].as_str().unwrap_or("Orion"), d["a"].as_str(), &c, &crate::util::correr),
        _ => respaldar(&c, &ahora(), &crate::util::correr),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn detecta_discos_externos() {
        let l = r#"{"blockdevices":[{"name":"sda","rm":false,"hotplug":false,"tran":"sata","children":[{"name":"sda1","mountpoint":"/","fsavail":"100"}]},
            {"name":"sdb","rm":true,"tran":"usb","children":[{"name":"sdb1","mountpoint":"/media/ana/USB","fsavail":"58200000000","label":"USB"}]}]}"#;
        let d = discos_externos_desde(l);
        assert_eq!(d.len(), 1);
        assert_eq!((d[0]["montaje"].as_str(), d[0]["libre_gb"].as_f64()), (Some("/media/ana/USB"), Some(58.2)));
    }

    #[test]
    fn fotos_con_link_dest_y_retencion() {
        let t = tempfile::tempdir().unwrap();
        let fuente = t.path().join("Orion");
        std::fs::create_dir_all(&fuente).unwrap();
        let destino = t.path().join("disco");
        let c = json!({"destino": destino, "que": [fuente]});
        for i in 0..32 {
            std::fs::create_dir_all(destino.join(format!("2026-01-01_{i:04}"))).ok();
        }
        let usados = std::cell::RefCell::new(vec![]);
        let falso = |cmd: &[&str]| {
            usados.borrow_mut().push(cmd.join(" "));
            (0, String::new(), String::new())
        };
        let r = respaldar(&c, "2026-11-08_2100", &falso);
        assert_eq!(r["ok"], true, "{r}");
        assert!(usados.borrow()[0].contains("--link-dest") && usados.borrow()[0].contains("--exclude"));
        assert_eq!(fotos(&destino).len(), CONSERVAR);
        assert_eq!(listar(&c)[0], "2026-11-08_2100");
        let nube = respaldar(&json!({"destino": "rclone:drive:Respaldo", "que": [fuente]}), "S", &falso);
        assert!(usados.borrow().last().unwrap().starts_with("rclone sync") && nube["ok"] == true);
        assert_eq!(respaldar(&json!({"destino": destino, "que": ["/no/existe"]}), "S", &falso)["ok"], false);
    }
}
