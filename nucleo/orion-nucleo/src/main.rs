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
        "vigia" => orion_nucleo::rendimiento::vigilar(5),
        "oido" => std::process::exit(orion_nucleo::voz::servicio()),
        "decir" => {
            let motor = opcion(&args, "--motor").unwrap_or_else(|| "auto".into());
            let texto: Vec<&str> = args[1..].iter().map(String::as_str).filter(|a| !a.starts_with("--") && *a != motor).collect();
            orion_nucleo::voz::hablar(&texto.join(" "), &motor, "", true);
        }
        "respaldo" => {
            let r = orion_nucleo::respaldo::accion(&serde_json::json!({}));
            println!("{r}");
            std::process::exit(if r["ok"] == true { 0 } else { 1 });
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
        "instalar" => std::process::exit(orion_nucleo::instalador::principal(&args[1..])),
        "dictado" => {
            // orion-nucleo dictado "título … sección …" [--plantilla informe-duoc|memoria-electrica|carta|acta]
            let pl = opcion(&args, "--plantilla").unwrap_or_else(|| "informe-duoc".into());
            let texto: Vec<&str> = args[1..].iter().map(String::as_str).filter(|a| !a.starts_with("--") && *a != pl).collect();
            imprimir(orion_nucleo::dictado::dictar(&texto.join(" "), &pl, "", &serde_json::json!({}), false));
        }
        "fit" => std::process::exit(orion_nucleo::fit::principal(&args[1..])),
        "robot" => {
            // orion-nucleo robot "hombro a 45 grados" ["onda en el codo" …]: pasa por el traductor y el Guardián (simulado)
            let raiz = std::env::var_os("ORION_ROBOTS").map(PathBuf::from);
            let dist = std::env::var("ORION_DISTANCIA_MM").ok().and_then(|d| d.parse::<f64>().ok());
            let mut o = orion_nucleo::movimiento::Orion::new(orion_nucleo::movimiento::perfil_por_defecto(), None, Some(Box::new(move || dist)), raiz.as_deref());
            for f in &args[1..] {
                println!("IA  › {f}");
                for l in o.decir(f) {
                    println!("     {l}");
                }
            }
            println!("pose {}", o.robot.pose_json());
            println!("efector {:?}", o.robot.efector());
        }
        "lidar" => std::process::exit(orion_nucleo::lidar::principal(&args[1..])),
        "despertador" => {
            // Nodo despertador (Raspberry Pi con micrófono USB): escucha palmadas y despierta al PC central por red.
            //   orion-nucleo despertador <MAC del PC central> [veces=2]
            use orion_nucleo::voz::activacion as act;
            let Some(mac) = args.get(1) else {
                eprintln!("uso: orion-nucleo despertador AA:BB:CC:DD:EE:FF [veces]");
                std::process::exit(2);
            };
            if let Err(e) = act::paquete_magico(mac) {
                eprintln!("{e}");
                std::process::exit(2);
            }
            let veces: usize = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(2);
            let mut p = act::palmadas_de(&act::leer());
            let mut arec = std::process::Command::new("arecord").args(["-q", "-f", "S16_LE", "-r", "16000", "-c", "1", "-t", "raw"]).stdout(std::process::Stdio::piped()).spawn().expect("arecord (alsa-utils)");
            let mut entrada = arec.stdout.take().unwrap();
            let mut b = [0u8; 3200];
            println!("escuchando palmadas · despierto {mac} con {veces}");
            while std::io::Read::read_exact(&mut entrada, &mut b).is_ok() {
                let m: Vec<i16> = b.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
                if let Some(n) = p.alimentar(&m).filter(|n| *n >= veces) {
                    let r = act::despertar_equipo(mac, "255.255.255.255", 9);
                    println!("palmadas: {n} → despertando {mac} {}", if r.is_ok() { "✓" } else { "✗" });
                }
            }
        }
        "importar-html" => std::process::exit(orion_nucleo::importar::principal(&args[1..])),
        "mcp" => std::process::exit(orion_nucleo::mcp::ejecutar()),
        "primer-arranque" => std::process::exit(orion_nucleo::primer::ejecutar()),
        "llave-desde" => {
            // orion-nucleo llave-desde "<frase>" <modo> <wav…>: la llave de voz con lo grabado en el instalador
            let frase = args.get(1).cloned().unwrap_or_default();
            let modo = args.get(2).cloned().unwrap_or_else(|| "voz_o_clave".into());
            let wavs: Vec<PathBuf> = args.iter().skip(3).map(PathBuf::from).collect();
            imprimir(orion_nucleo::voz::llave::configurar_desde_archivos(&frase, &wavs, &modo));
        }
        "config" => {
            // orion-nucleo config <archivo.json> <ruta.con.puntos>: lee un valor (para guiones de consola)
            let v: serde_json::Value = std::fs::read_to_string(args.get(1).map(String::as_str).unwrap_or("/etc/orion/instalacion.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
            let mut x = &v;
            for k in args.get(2).map(String::as_str).unwrap_or("").split('.').filter(|k| !k.is_empty()) { x = &x[k]; }
            match x { serde_json::Value::String(s) => println!("{s}"), serde_json::Value::Array(a) => println!("{}", a.iter().map(|e| e.as_str().map(String::from).unwrap_or(e.to_string())).collect::<Vec<_>>().join(" ")), serde_json::Value::Null => {}, o => println!("{o}") }
        }
        "version" => println!("orion-nucleo {}", env!("CARGO_PKG_VERSION")),
        _ => {
            eprintln!("uso: orion-nucleo [servir|mcp|importar-html|instalar|dictado|fit|robot|lidar|despertador|vigia|oido|decir|respaldo|primer-arranque|llave-desde|config|equipo|controladores|apps|electrica|memoria] …");
            std::process::exit(2);
        }
    }
}
