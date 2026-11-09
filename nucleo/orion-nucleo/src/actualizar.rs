//! Actualizaciones por internet, sin reinstalar.
//! 1. El sistema (Ubuntu): parches automáticos (unattended-upgrades) y «actualiza todo» por voz.
//! 2. Neo Orión mismo: paquete `neo-orion-core_<versión>_amd64.deb` en un CANAL (GitHub Releases, sitio propio o
//!    pendrive) con `canal.json` (versión, .deb y SHA-256). Se baja, se verifica la suma y recién ahí se instala.

use crate::util::Ejecutor;
use regex::Regex;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub fn version_actual() -> String {
    Regex::new(r"(\d+\.\d+(?:\.\d+)?)").unwrap().captures(&std::fs::read_to_string("/etc/orion/version").unwrap_or_default()).map(|c| c[1].to_string()).unwrap_or_else(|| "0.0".into())
}

/// ¿a es más nueva que b?
pub fn mayor(a: &str, b: &str) -> bool {
    let n = |v: &str| Regex::new(r"\d+").unwrap().find_iter(v).filter_map(|m| m.as_str().parse::<u64>().ok()).collect::<Vec<_>>();
    let (mut pa, mut pb) = (n(a), n(b));
    let l = pa.len().max(pb.len());
    pa.resize(l, 0);
    pb.resize(l, 0);
    pa > pb
}

pub fn canal() -> String {
    std::fs::read_to_string("/etc/orion/actualizaciones.json").ok().and_then(|s| serde_json::from_str::<Value>(&s).ok()).and_then(|v| v["canal"].as_str().map(String::from)).unwrap_or_default()
}

pub type Lector<'a> = &'a dyn Fn(&str) -> Result<Vec<u8>, String>;

pub fn bajar(url: &str) -> Result<Vec<u8>, String> {
    if let Some(r) = url.strip_prefix("file://") {
        return std::fs::read(r).map_err(|e| e.to_string());
    }
    let r = ureq::get(url).timeout(std::time::Duration::from_secs(120)).call().map_err(|e| e.to_string())?;
    let mut b = vec![];
    std::io::Read::read_to_end(&mut r.into_reader(), &mut b).map_err(|e| e.to_string())?;
    Ok(b)
}

pub fn revisar(url_canal: Option<&str>, leer: Lector) -> Value {
    let url = url_canal.map(String::from).unwrap_or_else(canal);
    if url.is_empty() {
        return json!({"ok": false, "error": "no hay canal de actualizaciones configurado (/etc/orion/actualizaciones.json)"});
    }
    let c: Value = match leer(&format!("{}/canal.json", url.trim_end_matches('/'))).and_then(|b| serde_json::from_slice(&b).map_err(|e| e.to_string())) {
        Ok(c) => c,
        Err(e) => return json!({"ok": false, "error": format!("no pude leer el canal ({e})")}),
    };
    let actual = version_actual();
    let ultima = c["version"].as_str().unwrap_or("0");
    json!({"ok": true, "actual": actual, "ultima": ultima, "hay_nueva": mayor(ultima, &actual), "novedades": c["novedades"].as_array().cloned().unwrap_or_default(),
           "deb": c["deb"], "sha256": c["sha256"], "canal": url})
}

pub fn bajar_y_verificar(info: &Value, leer: Lector, carpeta: &Path) -> Result<PathBuf, String> {
    let deb = info["deb"].as_str().unwrap_or("");
    let url = if Regex::new(r"^(https?|file)://").unwrap().is_match(deb) { deb.to_string() } else { format!("{}/{deb}", info["canal"].as_str().unwrap_or("").trim_end_matches('/')) };
    let datos = leer(&url)?;
    let suma = format!("{:x}", Sha256::digest(&datos));
    let esperada = info["sha256"].as_str().unwrap_or("").to_lowercase();
    if suma != esperada {
        return Err(format!("la suma SHA-256 no calza (esperada {}…, llegó {}…): no se instala", &esperada.get(..12).unwrap_or(""), &suma[..12]));
    }
    let destino = carpeta.join(url.rsplit('/').next().unwrap_or("neo-orion.deb"));
    std::fs::write(&destino, datos).map_err(|e| e.to_string())?;
    Ok(destino)
}

pub fn actualizar_orion(leer: Lector, ejecutar: Ejecutor) -> Value {
    let info = revisar(None, leer);
    if info["ok"] != true {
        return info;
    }
    if info["hay_nueva"] != true {
        return json!({"ok": true, "mensaje": format!("Neo Orión ya está al día ({})", info["actual"].as_str().unwrap_or(""))});
    }
    let deb = match bajar_y_verificar(&info, leer, &std::env::temp_dir()) {
        Ok(d) => d,
        Err(e) => return json!({"ok": false, "error": e}),
    };
    let (c, _, err) = ejecutar(&["sudo", "apt-get", "install", "-y", &deb.to_string_lossy()]);
    json!({"ok": c == 0, "de": info["actual"], "a": info["ultima"], "novedades": info["novedades"], "error": if c != 0 { err } else { String::new() }})
}

pub fn actualizar_sistema(ejecutar: Ejecutor) -> Value {
    for p in [&["sudo", "apt-get", "update"][..], &["sudo", "apt-get", "-y", "full-upgrade"], &["sudo", "apt-get", "-y", "autoremove"]] {
        let (c, _, err) = ejecutar(p);
        if c != 0 {
            return json!({"ok": false, "paso": p.join(" "), "error": err.chars().rev().take(300).collect::<Vec<_>>().into_iter().rev().collect::<String>()});
        }
    }
    json!({"ok": true, "reiniciar": Path::new("/var/run/reboot-required").exists(), "driver_grafica": crate::controladores::revisar_al_dia_equipo()})
}

/// Para el que publica: neo-orion-core_<versión>_amd64.deb con el núcleo, la interfaz y los servicios.
pub fn construir_deb(repo: &Path, version: &str, salida: &Path, binarios: &[PathBuf]) -> Result<PathBuf, String> {
    let raiz = std::env::temp_dir().join(format!("neo-orion-core_{version}_amd64"));
    let _ = std::fs::remove_dir_all(&raiz);
    let copiar = |o: &Path, d: &Path| -> Result<(), String> {
        std::fs::create_dir_all(d.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::copy(o, d).map(|_| ()).map_err(|e| format!("{}: {e}", o.display()))
    };
    fn recorrer(d: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
            if e.path().is_dir() { recorrer(&e.path(), out) } else { out.push(e.path()) }
        }
    }
    let mut ov = vec![];
    recorrer(&repo.join("overlay"), &mut ov);
    for f in ov {
        copiar(&f, &raiz.join(f.strip_prefix(repo.join("overlay")).unwrap()))?;
    }
    for b in binarios {
        copiar(b, &raiz.join("usr/local/bin").join(b.file_name().unwrap()))?;
    }
    for (carpeta, archivos) in [("usr/share/orion/hud", ["index.html", "marathon.js"]), ("usr/share/orion/instalador", ["instalar.html", "marathon.js"])] {
        for a in archivos {
            copiar(&repo.join("hud").join(a), &raiz.join(carpeta).join(a))?;
        }
    }
    for a in ["ia-catalogo.json", "modulos.json", "iconos.json"] {
        copiar(&repo.join("config").join(a), &raiz.join("usr/share/orion").join(a))?;
    }
    std::fs::create_dir_all(raiz.join("DEBIAN")).map_err(|e| e.to_string())?;
    std::fs::write(raiz.join("DEBIAN/control"), format!("Package: neo-orion-core\nVersion: {version}\nArchitecture: amd64\nMaintainer: Neo Orión <hola@neo-orion.local>\nDepends: cage, alsa-utils\nSection: misc\nPriority: optional\nDescription: Neo Orión OS - núcleo nativo, HUD e instalador\n Interfaz por voz, servidor MCP y herramientas de Neo Orión (Rust).\n")).map_err(|e| e.to_string())?;
    std::fs::write(raiz.join("DEBIAN/postinst"), format!("#!/bin/sh\nset -e\nmkdir -p /etc/orion\nsed -i 's/^\\(.*OS \\)[0-9.]*/\\1{version}/' /etc/orion/version 2>/dev/null || true\nsystemctl daemon-reload 2>/dev/null || true\n")).map_err(|e| e.to_string())?;
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(raiz.join("DEBIAN/postinst"), std::fs::Permissions::from_mode(0o755));
    std::fs::create_dir_all(salida).map_err(|e| e.to_string())?;
    let deb = salida.join(format!("neo-orion-core_{version}_amd64.deb"));
    let (c, _, e) = crate::util::correr(&["dpkg-deb", "--root-owner-group", "--build", &raiz.to_string_lossy(), &deb.to_string_lossy()]);
    if c != 0 {
        return Err(format!("dpkg-deb: {e}"));
    }
    Ok(deb)
}

pub fn publicar_canal(deb: &Path, version: &str, novedades: &[String], carpeta: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(carpeta).map_err(|e| e.to_string())?;
    let nombre = deb.file_name().unwrap();
    if deb.parent() != Some(carpeta) {
        std::fs::copy(deb, carpeta.join(nombre)).map_err(|e| e.to_string())?;
    }
    let datos = std::fs::read(deb).map_err(|e| e.to_string())?;
    let c = json!({"version": version, "deb": nombre.to_string_lossy(), "sha256": format!("{:x}", Sha256::digest(&datos)), "novedades": novedades});
    std::fs::write(carpeta.join("canal.json"), serde_json::to_string_pretty(&c).unwrap()).map_err(|e| e.to_string())?;
    Ok(carpeta.join("canal.json"))
}

pub fn accion(d: &Value) -> Value {
    match d["accion"].as_str().unwrap_or("revisar") {
        "sistema" => actualizar_sistema(&crate::util::correr),
        "orion" => actualizar_orion(&bajar, &crate::util::correr),
        _ => revisar(None, &bajar),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    #[test]
    fn versiones_y_suma() {
        assert!(mayor("0.10", "0.9") && mayor("1.0.1", "1.0") && !mayor("0.9", "0.9.0"));
        let d = tempfile::tempdir().unwrap();
        let deb = d.path().join("x.deb");
        std::fs::write(&deb, b"paquete").unwrap();
        let canal = d.path().join("canal");
        publicar_canal(&deb, "9.9", &["nuevo".into()], &canal).unwrap();
        let url = format!("file://{}", canal.display());
        let info = revisar(Some(&url), &bajar);
        assert_eq!((info["ok"].as_bool(), info["hay_nueva"].as_bool()), (Some(true), Some(true)));
        assert!(bajar_y_verificar(&info, &bajar, d.path()).is_ok());
        let mut malo = info.clone();
        malo["sha256"] = "00".repeat(32).into();
        assert!(bajar_y_verificar(&malo, &bajar, d.path()).unwrap_err().contains("no calza"));
        assert_eq!(revisar(Some("file:///no/hay"), &bajar)["ok"], false);
    }
}
