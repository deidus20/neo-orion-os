//! orion-nucleo — servidor local de Neo Orión y herramientas por consola.
//!
//!   orion-nucleo [servir] [--puerto 7700] [--modo auto|instalador|hud] [--raiz /usr/share/orion]
//!   orion-nucleo equipo | controladores | apps | electrica "20 A a 35 m" | memoria buscar "texto"

use orion_nucleo::{controladores, electrica, equipo, iconos, memoria, servidor};
use std::path::PathBuf;

fn opcion(args: &[String], nombre: &str) -> Option<String> {
    args.iter().position(|a| a == nombre).and_then(|i| args.get(i + 1)).cloned()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let orden = match args.first().map(String::as_str) {
        Some("--version" | "-V") => "version",
        Some("--help" | "-h") => "ayuda",
        Some(a) if !a.starts_with("--") => a,
        _ => "servir",
    };
    let imprimir = |v: serde_json::Value| println!("{}", serde_json::to_string_pretty(&v).unwrap());
    match orden {
        "servir" => {
            let puerto = opcion(&args, "--puerto").and_then(|p| p.parse().ok()).unwrap_or(7700);
            let modo = opcion(&args, "--modo").filter(|m| m != "auto").unwrap_or_else(servidor::modo_auto);
            let raiz = opcion(&args, "--raiz").map(PathBuf::from).unwrap_or_else(servidor::raiz_por_defecto);
            let progreso = std::env::var("ORION_PROGRESO").map(PathBuf::from).unwrap_or_else(|_| "/run/orion-instalacion.jsonl".into());
            println!("Neo Orión · núcleo Rust en http://127.0.0.1:{puerto}/ ({modo})");
            let est = servidor::Estado { raiz, modo, progreso, instalador: Default::default() };
            if let Err(e) = servidor::servir(puerto, est, 4) {
                eprintln!("no se pudo abrir el puerto {puerto}: {e}");
                std::process::exit(1);
            }
        }
        "equipo" => imprimir(equipo::equipo()),
        "controladores" => imprimir(controladores::detectar()),
        "apps" => imprimir(serde_json::json!(iconos::listar(args.iter().any(|a| a == "--todas")))),
        "electrica" => match electrica::desde_frase(&args[1..].join(" ")) {
            Some(r) => println!("{}", r.texto()),
            None => println!("Di la corriente o la potencia y el largo: «20 A a 35 m», «4500 W a 40 m trifásico»."),
        },
        "memoria" => {
            let m = memoria::Memoria::abrir(None).expect("memoria");
            match args.get(1).map(String::as_str) {
                Some("buscar") => imprimir(serde_json::json!(m.buscar(&args[2..].join(" "), 8))),
                Some("anotar") => println!("{}", m.diario(&args[2..].join(" "), "consola").map(|p| p.display().to_string()).unwrap_or_default()),
                _ => println!("{} notas indexadas", m.reindexar()),
            }
        }
        "version" => println!("orion-nucleo {}", env!("CARGO_PKG_VERSION")),
        _ => {
            eprintln!("uso: orion-nucleo [servir|equipo|controladores|apps|electrica|memoria] …");
            std::process::exit(2);
        }
    }
}
