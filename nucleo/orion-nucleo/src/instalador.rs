//! Instalador real de Neo Orión (antes orion_core/instalador.py + instalacion.py): lleva la sesión en vivo
//! (pendrive) a un disco. La interfaz (instalador/instalar.html) junta las elecciones y el servidor lanza
//! `orion-nucleo instalar config.json --progreso archivo` como root.
//!
//!  1. revisar la configuración (usuario, disco, modelos, color)
//!  2. particionar   borrar: GPT nueva (EFI 512 MB + BIOS 1 MB + Neo Orión)
//!                   junto:  usa el espacio libre más grande y la EFI que ya existe (Windows queda intacto)
//!  3. formatear y montar en /target
//!  4. copiar el sistema desde la imagen del pendrive (filesystem.squashfs)
//!  5. fstab, nombre del equipo, idioma, zona, teclado
//!  6. crear la cuenta (grupos de hardware, clave), sacar lo que es solo del pendrive
//!  7. identidad, configuración de la instalación, capa personal si se eligió
//!  8. cargador de arranque: UEFI (shim firmado, arranque seguro) + BIOS
//!  9. dejar el primer arranque listo para bajar las IA, oído y voz cuando haya internet
//!
//! Progreso: una línea JSON por evento {"pct", "etapa", "detalle", "log"}. Con --simular solo anota los comandos.
//! La clave llega por la variable ORION_CLAVE o por la entrada estándar; nunca se escribe en disco.

use regex::Regex;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const OBJ: &str = "/target";
const MEDIOS: [&str; 3] = ["/cdrom/casper/filesystem.squashfs", "/run/live/medium/casper/filesystem.squashfs", "/media/cdrom/casper/filesystem.squashfs"];
const SOLO_PENDRIVE: [&str; 5] = ["casper", "ubiquity", "calamares", "calamares-settings-neo-orion", "orion-instalador"];
pub const GRUPOS: [&str; 10] = ["sudo", "adm", "audio", "video", "render", "input", "plugdev", "dialout", "netdev", "lpadmin"];

fn obj(r: &str) -> String {
    if r.is_empty() { OBJ.into() } else { format!("{OBJ}/{r}") }
}

fn prueba_loop() -> bool {
    std::env::var("ORION_PRUEBA_LOOP").as_deref() == Ok("1")
}

/// Corre comandos de verdad o solo los anota (simular).
#[derive(Default)]
pub struct Ejecutor {
    pub simular: bool,
    pub hechos: Vec<Vec<String>>,
    /// para simular: prefijo de comando → salida
    pub salidas: Vec<(String, String)>,
}

impl Ejecutor {
    pub fn new(simular: bool) -> Self {
        Ejecutor { simular, ..Default::default() }
    }

    pub fn ok(&mut self, cmd: &[&str], entrada: Option<&str>, ok: &[i32]) -> Result<String, String> {
        self.hechos.push(cmd.iter().map(|s| s.to_string()).collect());
        if self.simular {
            let plano = cmd.join(" ");
            return Ok(self.salidas.iter().find(|(p, _)| plano.starts_with(p.as_str())).map(|x| x.1.clone()).unwrap_or_default());
        }
        let hijo = Command::new(cmd[0]).args(&cmd[1..]).stdin(if entrada.is_some() { Stdio::piped() } else { Stdio::null() }).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn();
        let mut hijo = match hijo {
            Ok(h) => h,
            Err(_) if ["udevadm", "partprobe", "swapoff"].contains(&cmd[0]) => return Ok(String::new()), // ayudas opcionales
            Err(_) => return Err(format!("falta el programa {}", cmd[0])),
        };
        if let (Some(t), Some(mut e)) = (entrada, hijo.stdin.take()) {
            let _ = e.write_all(t.as_bytes());
        }
        let o = hijo.wait_with_output().map_err(|e| e.to_string())?;
        let codigo = o.status.code().unwrap_or(-1);
        if !ok.contains(&codigo) {
            let err = if o.stderr.is_empty() { &o.stdout } else { &o.stderr };
            let t = String::from_utf8_lossy(err).trim().to_string();
            let cola: String = t.chars().rev().take(800).collect::<Vec<_>>().into_iter().rev().collect();
            return Err(format!("falló: {}\n{cola}", shlex::try_join(cmd.iter().copied()).unwrap_or_else(|_| cmd.join(" "))));
        }
        Ok(String::from_utf8_lossy(&o.stdout).into())
    }

    pub fn x(&mut self, cmd: &[&str]) -> Result<String, String> {
        self.ok(cmd, None, &[0])
    }

    pub fn escribir(&mut self, ruta: &str, texto: &str) -> Result<(), String> {
        self.hechos.push(vec!["escribir".into(), ruta.into()]);
        if self.simular {
            return Ok(());
        }
        if let Some(d) = Path::new(ruta).parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(ruta, texto).map_err(|e| format!("{ruta}: {e}"))
    }

    fn en_objetivo(&mut self, cmd: &[&str], entrada: Option<&str>, ok: &[i32]) -> Result<String, String> {
        let mut c = vec!["chroot", OBJ];
        c.extend(cmd);
        self.ok(&c, entrada, ok)
    }
}

pub struct Progreso {
    archivo: Option<std::fs::File>,
    pub pct: i64,
    /// para pruebas: los eventos en memoria
    pub eventos: Option<Vec<Value>>,
}

impl Progreso {
    pub fn new(archivo: Option<&Path>) -> Self {
        Progreso { archivo: archivo.and_then(|a| std::fs::OpenOptions::new().create(true).append(true).open(a).ok()), pct: 0, eventos: None }
    }

    pub fn memoria() -> Self {
        Progreso { archivo: None, pct: 0, eventos: Some(vec![]) }
    }

    pub fn ev(&mut self, pct: Option<i64>, etapa: &str, detalle: &str, log: &str, extra: Value) {
        if let Some(p) = pct {
            self.pct = p;
        }
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| (d.as_secs_f64() * 10.0).round() / 10.0).unwrap_or(0.0);
        let mut e = json!({"t": t, "pct": self.pct, "etapa": etapa, "detalle": detalle, "log": log});
        for (k, v) in extra.as_object().into_iter().flatten() {
            e[k] = v.clone();
        }
        if let Some(v) = self.eventos.as_mut() {
            v.push(e);
        } else if let Some(f) = self.archivo.as_mut() {
            let _ = writeln!(f, "{e}");
        } else {
            println!("{e}");
        }
    }

    pub fn p(&mut self, pct: i64, etapa: &str, detalle: &str) {
        self.ev(Some(pct), etapa, detalle, "", json!({}));
    }
}

/// /dev/sda + 2 → /dev/sda2 ; /dev/nvme0n1 + 2 → /dev/nvme0n1p2 ; /dev/mmcblk0 → p2.
pub fn particion(disco: &str, n: u32) -> String {
    if disco.ends_with(|c: char| c.is_ascii_digit()) { format!("{disco}p{n}") } else { format!("{disco}{n}") }
}

// ------------------------------------------------------------------ revisión (antes instalacion.py)

pub fn validar(c: &Value) -> Vec<String> {
    let mut err = vec![];
    let modelo = Regex::new(r"^[a-z0-9._-]+(:[a-z0-9._-]+)?$").unwrap();
    for m in c["ia"]["locales"].as_array().into_iter().flatten() {
        let s = m.as_str().unwrap_or("");
        if !modelo.is_match(s) {
            err.push(format!("modelo con nombre inválido: {s:?}"));
        }
    }
    let col = c["identidad"]["color"].as_str().unwrap_or("");
    if !col.is_empty() && !Regex::new(r"^#[0-9a-fA-F]{6}$").unwrap().is_match(col) {
        err.push(format!("color inválido: {col:?}"));
    }
    if !Regex::new(r"^[a-z_][a-z0-9_-]{0,31}$").unwrap().is_match(&c["cuenta"]["usuario"].as_str().unwrap_or("").to_lowercase()) {
        err.push("nombre de usuario inválido para Linux".into());
    }
    err
}

pub fn revisar(c: &Value) -> Vec<String> {
    let mut err = validar(c);
    let d = c["disco"]["dispositivo"].as_str().unwrap_or("");
    let lp = if prueba_loop() { r"|loop\d+" } else { "" }; // solo pruebas: disco en un archivo
    if !Regex::new(&format!(r"^/dev/(sd[a-z]+|vd[a-z]+|hd[a-z]+|xvd[a-z]+|nvme\d+n\d+|mmcblk\d+{lp})$")).unwrap().is_match(d) {
        err.push(format!("disco inválido: {d:?}"));
    }
    let modo = c["disco"]["modo"].as_str().unwrap_or("");
    if modo != "borrar" && modo != "junto" {
        err.push("modo de disco no soportado por el instalador rápido (usa borrar o junto; manual abre Calamares)".into());
    }
    if modo == "borrar" && c["disco"]["confirmado"] != true {
        err.push("falta confirmar el borrado del disco".into());
    }
    let eq = c["cuenta"].get("equipo").and_then(Value::as_str).unwrap_or("neo-orion");
    if !Regex::new(r"^[a-z0-9][a-z0-9-]{0,62}$").unwrap().is_match(eq) {
        err.push(format!("nombre de equipo inválido: {eq:?}"));
    }
    if let Some(z) = c["zona"].as_str().filter(|z| !z.is_empty()) {
        if !Regex::new(r"^[A-Za-z_]+(/[A-Za-z_+-]+){0,2}$").unwrap().is_match(z) {
            err.push("zona horaria inválida".into());
        }
    }
    err
}

/// Ajusta config/identidad.yaml con lo elegido en el instalador.
pub fn identidad_yaml(c: &Value, base: &str) -> String {
    let i = &c["identidad"];
    let genero = match i["voz"].as_str() {
        Some("femenina") => "femenino",
        Some("masculina") => "masculino",
        _ => "neutro",
    };
    let tono = match i["tono"].as_str() {
        Some("tecnico") => "técnico",
        Some(t) => t,
        None => "directo",
    };
    let mut out = vec![];
    for linea in base.lines() {
        let k = linea.split(':').next().unwrap_or("").trim();
        let l = if linea.starts_with("nombre:") {
            format!("nombre: {}", i["nombre"].as_str().unwrap_or("Neo Orión"))
        } else if linea.starts_with("idioma:") {
            format!("idioma: {}", c["idioma"].as_str().unwrap_or("es-CL"))
        } else if linea.starts_with("color:") && i["color"].as_str().is_some_and(|x| !x.is_empty()) {
            Regex::new(r##""#[0-9a-fA-F]{6}""##).unwrap().replace_all(linea, format!("\"{}\"", i["color"].as_str().unwrap()).as_str()).into()
        } else if linea.starts_with("tono:") {
            Regex::new(r"^tono:\s*\S+").unwrap().replace(linea, format!("tono: {tono}").as_str()).into()
        } else if k == "genero" {
            Regex::new(r"genero:\s*\S+").unwrap().replace(linea, format!("genero: {genero}").as_str()).into()
        } else if k == "motor" && c["voz"].as_str().is_some_and(|v| !v.is_empty()) {
            Regex::new(r"motor:\s*\S+").unwrap().replace(linea, format!("motor: {}", c["voz"].as_str().unwrap()).as_str()).into()
        } else {
            linea.to_string()
        };
        out.push(l);
    }
    out.join("\n") + "\n"
}

// ------------------------------------------------------------------ pasos

pub fn medio_vivo() -> Result<&'static str, String> {
    MEDIOS.iter().copied().find(|m| Path::new(m).exists()).ok_or_else(|| "no encuentro la imagen del sistema (filesystem.squashfs): ¿arrancaste desde el pendrive?".into())
}

pub fn es_uefi() -> bool {
    !prueba_loop() && Path::new("/sys/firmware/efi").is_dir() // en pruebas nunca se tocan las entradas del equipo
}

pub struct Particiones {
    pub efi: String,
    pub raiz: String,
    pub formatear_efi: bool,
}

fn numeros(tabla: &str) -> std::collections::BTreeSet<u32> {
    Regex::new(r"(?m)^\s*(\d+)\s").unwrap().captures_iter(tabla).filter_map(|c| c[1].parse().ok()).collect()
}

pub fn particionar(x: &mut Ejecutor, disco: &str, modo: &str, p: &mut Progreso) -> Result<Particiones, String> {
    for parte in x.x(&["lsblk", "-lnpo", "NAME,MOUNTPOINT", disco])?.lines() {
        let cols: Vec<&str> = parte.split_whitespace().collect();
        if cols.len() > 1 {
            x.ok(&["umount", "-l", cols[0]], None, &[0, 1, 32])?;
        }
    }
    x.ok(&["swapoff", "-a"], None, &[0, 1, 255])?;
    if modo == "borrar" {
        p.ev(Some(4), "PARTICIONANDO", &format!("{disco} · tabla GPT nueva"), "se borra todo el disco", json!({}));
        x.x(&["wipefs", "-a", disco])?;
        x.ok(&["sgdisk", "-Z", disco], None, &[0, 2])?;
        x.x(&["sgdisk", "-n", "1:0:+512M", "-t", "1:ef00", "-c", "1:EFI", "-n", "2:0:+1M", "-t", "2:ef02", "-c", "2:BIOS", "-n", "3:0:0", "-t", "3:8304", "-c", "3:NEO-ORION", disco])?;
        x.ok(&["partprobe", disco], None, &[0, 1])?;
        x.ok(&["udevadm", "settle"], None, &[0, 1])?;
        return Ok(Particiones { efi: particion(disco, 1), raiz: particion(disco, 3), formatear_efi: true });
    }
    // junto: espacio libre más grande + EFI existente
    p.ev(Some(4), "PARTICIONANDO", &format!("{disco} · junto al sistema actual"), "se usa el espacio libre; lo demás queda intacto", json!({}));
    let info = x.x(&["sgdisk", "-p", disco])?;
    let re_efi = Regex::new(r"^\s*(\d+)\s+\d+\s+\d+\s+\S+\s+\S+\s+EF00").unwrap();
    let mut efi = info.lines().filter_map(|l| re_efi.captures(l)).last().map(|c| particion(disco, c[1].parse().unwrap_or(1)));
    let ultimo = |s: String| s.trim().lines().last().and_then(|l| l.trim().parse::<i64>().ok());
    let libre = ultimo(x.x(&["sgdisk", "-F", disco])?);
    let fin = ultimo(x.x(&["sgdisk", "-E", disco])?);
    match (libre, fin) {
        (Some(l), Some(f)) if f - l >= 20 * 1024 * 1024 * 2 => {} // sectores de 512 B: 20 GB
        _ => return Err("no hay al menos 20 GB libres en el disco: achica Windows primero o elige borrar".into()),
    }
    let antes = numeros(&info);
    if efi.is_none() {
        x.x(&["sgdisk", "-n", "0:0:+512M", "-t", "0:ef00", "-c", "0:EFI", disco])?;
    }
    x.x(&["sgdisk", "-n", "0:0:0", "-t", "0:8304", "-c", "0:NEO-ORION", disco])?;
    x.ok(&["partprobe", disco], None, &[0, 1])?;
    x.ok(&["udevadm", "settle"], None, &[0, 1])?;
    let despues = x.x(&["sgdisk", "-p", disco])?;
    let mut nuevos: Vec<u32> = numeros(&despues).difference(&antes).copied().collect();
    if nuevos.is_empty() {
        if !x.simular {
            return Err("no se pudo crear la partición nueva".into());
        }
        nuevos = if efi.is_some() { vec![9] } else { vec![8, 9] };
    }
    let nueva_efi = efi.is_none();
    if efi.is_none() {
        efi = Some(particion(disco, nuevos[0]));
    }
    Ok(Particiones { efi: efi.unwrap(), raiz: particion(disco, *nuevos.last().unwrap()), formatear_efi: nueva_efi })
}

fn uuid(x: &mut Ejecutor, dev: &str) -> Result<String, String> {
    let u = x.x(&["blkid", "-s", "UUID", "-o", "value", dev])?.trim().to_string();
    Ok(if u.is_empty() { "SIMULADO".into() } else { u })
}

fn montar_virtuales(x: &mut Ejecutor) -> Result<(), String> {
    for (fs, dest, extra) in [("proc", "proc", ["-t", "proc"]), ("sysfs", "sys", ["-t", "sysfs"]), ("/dev", "dev", ["--bind", ""]), ("devpts", "dev/pts", ["-t", "devpts"]), ("tmpfs", "run", ["-t", "tmpfs"])] {
        let d = obj(dest);
        x.x(&["mkdir", "-p", &d])?;
        let mut c = vec!["mount"];
        c.extend(extra.iter().filter(|e| !e.is_empty()));
        c.extend([fs, d.as_str()]);
        x.x(&c)?;
    }
    if es_uefi() && Path::new("/sys/firmware/efi/efivars").is_dir() {
        x.ok(&["mount", "--bind", "/sys/firmware/efi/efivars", &obj("sys/firmware/efi/efivars")], None, &[0, 32])?;
    }
    Ok(())
}

pub fn desmontar(x: &mut Ejecutor) {
    for d in ["sys/firmware/efi/efivars", "run", "dev/pts", "dev", "sys", "proc", "boot/efi", ""] {
        let _ = x.ok(&["umount", "-l", &obj(d)], None, &[0, 1, 32]);
    }
}

fn ram_gb() -> f64 {
    let m = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    Regex::new(r"MemTotal:\s+(\d+)").unwrap().captures(&m).and_then(|c| c[1].parse::<f64>().ok()).map(|k| k / 1024.0 / 1024.0).unwrap_or(8.0)
}

fn grupo_existe(g: &str) -> bool {
    std::fs::read_to_string(obj("etc/group")).unwrap_or_default().lines().any(|l| l.split(':').next() == Some(g))
}

fn copiar_con_avance(medio: &str, p: &mut Progreso) -> Result<(), String> {
    let nproc = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).to_string();
    let mut h = Command::new("unsquashfs").args(["-f", "-p", &nproc, "-d", OBJ, medio]).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|_| "falta el programa unsquashfs".to_string())?;
    let mut sal = h.stdout.take().unwrap();
    let re = Regex::new(r"(\d+)%").unwrap();
    let mut buf = String::new();
    let mut ultimo = -1;
    let mut trozo = [0u8; 256];
    while let Ok(n) = sal.read(&mut trozo) {
        if n == 0 {
            break;
        }
        buf.push_str(&String::from_utf8_lossy(&trozo[..n]));
        if buf.len() > 400 {
            buf = buf[buf.len() - 400..].to_string();
            while !buf.is_char_boundary(0) {
                buf.remove(0);
            }
        }
        if let Some(v) = re.captures_iter(&buf).last().and_then(|c| c[1].parse::<i64>().ok()) {
            if v / 5 != ultimo {
                ultimo = v / 5;
                p.p(10 + v / 2, "COPIANDO SISTEMA", &format!("{v}% copiado"));
            }
        }
    }
    let mut err = String::new();
    if let Some(mut e) = h.stderr.take() {
        let _ = e.read_to_string(&mut err);
    }
    if !h.wait().map(|s| s.success()).unwrap_or(false) {
        let cola: String = format!("{buf}{err}").chars().rev().take(300).collect::<Vec<_>>().into_iter().rev().collect();
        return Err(format!("falló la copia del sistema (unsquashfs): {cola}"));
    }
    Ok(())
}

pub fn instalar(c: &Value, clave: &str, x: &mut Ejecutor, p: &mut Progreso, raiz_repo: Option<&Path>) -> Result<Value, String> {
    let mut err = revisar(c);
    if clave.chars().count() < 6 {
        err.push("la clave debe tener al menos 6 caracteres".into());
    }
    if !err.is_empty() {
        return Err(err.join("; "));
    }
    let disco = c["disco"]["dispositivo"].as_str().unwrap();
    let modo = c["disco"]["modo"].as_str().unwrap();
    let usuario = c["cuenta"]["usuario"].as_str().unwrap().to_lowercase();
    let equipo = c["cuenta"]["equipo"].as_str().filter(|e| !e.is_empty()).unwrap_or("neo-orion").to_string();
    p.ev(Some(1), "REVISANDO", &format!("{usuario}@{equipo} · {disco} · {modo}"), "configuración válida", json!({}));
    let medio = if x.simular { MEDIOS[0] } else { medio_vivo()? };

    // 2-3 · particiones, formato y montaje
    let parts = particionar(x, disco, modo, p)?;
    p.p(8, "FORMATEANDO", &format!("raíz {} · EFI {}", parts.raiz, parts.efi));
    x.x(&["mkfs.ext4", "-F", "-L", "neo-orion", &parts.raiz])?;
    if parts.formatear_efi {
        x.x(&["mkfs.vfat", "-F", "32", "-n", "EFI", &parts.efi])?;
    }
    x.x(&["mkdir", "-p", OBJ])?;
    x.x(&["mount", &parts.raiz, OBJ])?;

    // 4 · copia
    p.ev(Some(10), "COPIANDO SISTEMA", "Ubuntu 24.04 + Neo Orión desde el pendrive", &format!("unsquashfs {medio}"), json!({}));
    if x.simular {
        x.x(&["unsquashfs", "-f", "-d", OBJ, medio])?;
    } else {
        copiar_con_avance(medio, p)?;
    }
    x.x(&["mkdir", "-p", &obj("boot/efi")])?;
    let sin_fat = prueba_loop() && !x.simular && !std::fs::read_to_string("/proc/filesystems").unwrap_or_default().contains("vfat"); // solo el equipo de pruebas
    if !sin_fat {
        x.x(&["mount", &parts.efi, &obj("boot/efi")])?;
    }
    // el kernel no queda dentro del squashfs en algunas imágenes: se toma del pendrive
    for base in ["/cdrom/casper", "/run/live/medium/casper"] {
        let sin_kernel = !std::fs::read_dir(obj("boot")).into_iter().flatten().flatten().any(|e| e.file_name().to_string_lossy().starts_with("vmlinuz-"));
        if !x.simular && Path::new(&format!("{base}/vmlinuz")).exists() && sin_kernel {
            if let Some(ver) = std::fs::read_dir(obj("usr/lib/modules")).ok().and_then(|mut d| d.next()).and_then(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()) {
                x.x(&["cp", &format!("{base}/vmlinuz"), &obj(&format!("boot/vmlinuz-{ver}"))])?;
            }
        }
    }

    // 5 · sistema base
    p.p(62, "CONFIGURANDO", "fstab, nombre del equipo, idioma, zona y teclado");
    let (u_raiz, u_efi) = (uuid(x, &parts.raiz)?, uuid(x, &parts.efi)?);
    x.escribir(&obj("etc/fstab"), &format!("# Neo Orión · generado por el instalador\nUUID={u_raiz}  /          ext4  errors=remount-ro  0 1\nUUID={u_efi}  /boot/efi  vfat  umask=0077,nofail  0 1\n/swapfile  none  swap  sw  0 0\n"))?;
    x.escribir(&obj("etc/hostname"), &format!("{equipo}\n"))?;
    x.escribir(&obj("etc/hosts"), &format!("127.0.0.1 localhost\n127.0.1.1 {equipo}\n::1 localhost ip6-localhost ip6-loopback\n"))?;
    let loc = match c["idioma"].as_str().unwrap_or("es-CL") {
        "es-MX" => "es_MX.UTF-8",
        "es-ES" => "es_ES.UTF-8",
        "en-US" => "en_US.UTF-8",
        _ => "es_CL.UTF-8",
    };
    x.escribir(&obj("etc/default/locale"), &format!("LANG={loc}\nLANGUAGE={}:es\n", loc.split('.').next().unwrap()))?;
    let zona = c["zona"].as_str().filter(|z| !z.is_empty()).unwrap_or("America/Santiago");
    x.x(&["ln", "-sf", &format!("/usr/share/zoneinfo/{zona}"), &obj("etc/localtime")])?;
    x.escribir(&obj("etc/timezone"), &format!("{zona}\n"))?;
    let lay = match c["teclado"].as_str().unwrap_or("latam") {
        t @ ("es" | "us") => t,
        _ => "latam",
    };
    x.escribir(&obj("etc/default/keyboard"), &format!("XKBMODEL=\"pc105\"\nXKBLAYOUT=\"{lay}\"\nXKBVARIANT=\"\"\nXKBOPTIONS=\"\"\nBACKSPACE=\"guess\"\n"))?;
    montar_virtuales(x)?;
    x.en_objetivo(&["locale-gen", loc], None, &[0, 1])?;

    // swap: 2 GB (4 GB si hay ≤ 8 GB de RAM, ayuda al modo patata)
    p.p(66, "MEMORIA VIRTUAL", "archivo de intercambio + zram si está instalado");
    let swap = obj("swapfile");
    x.x(&["fallocate", "-l", if ram_gb() <= 8.0 { "4G" } else { "2G" }, &swap])?;
    x.x(&["chmod", "600", &swap])?;
    x.x(&["mkswap", &swap])?;

    // 6 · cuenta y limpieza del pendrive
    p.ev(Some(70), "CUENTA", &format!("{usuario} @ {equipo}"), &format!("grupos: {}", GRUPOS.join(", ")), json!({}));
    x.en_objetivo(&["userdel", "-r", "orion"], None, &[0, 6, 12])?; // el usuario de la sesión en vivo, si quedó
    let grupos: Vec<&str> = GRUPOS.iter().copied().filter(|g| x.simular || grupo_existe(g)).collect();
    let nombre = {
        let mut ch = usuario.chars();
        ch.next().map(|f| f.to_uppercase().chain(ch).collect::<String>()).unwrap_or_default()
    };
    x.en_objetivo(&["useradd", "-m", "-s", "/bin/bash", "-G", &grupos.join(","), "-c", &nombre, &usuario], None, &[0])?;
    x.en_objetivo(&["chpasswd"], Some(&format!("{usuario}:{clave}\n")), &[0])?;
    p.p(74, "LIMPIANDO", "se quitan los programas que solo sirven en el pendrive");
    let mut purga = vec!["apt-get", "purge", "-y", "--allow-remove-essential"];
    purga.extend(SOLO_PENDRIVE);
    x.en_objetivo(&purga, None, &[0, 100])?;
    x.en_objetivo(&["apt-get", "autoremove", "-y", "--purge"], None, &[0, 100])?;
    for f in ["etc/casper.conf", "etc/sudoers.d/orion-live"] {
        x.x(&["rm", "-f", &obj(f)])?;
    }

    // 7 · Neo Orión: identidad, sesión, capa
    p.p(78, "IDENTIDAD", &format!("{} · {}", c["identidad"]["nombre"].as_str().unwrap_or("Neo Orión"), c["identidad"]["color"].as_str().unwrap_or("")));
    let mut publico = c.clone();
    if let Some(o) = publico.as_object_mut() {
        o.remove("clave");
    }
    x.escribir(&obj("etc/orion/instalacion.json"), &(serde_json::to_string_pretty(&publico).unwrap() + "\n"))?;
    let base_ident = obj("etc/orion/identidad.yaml");
    let plantilla = std::fs::read_to_string(&base_ident)
        .ok()
        .or_else(|| raiz_repo.and_then(|r| std::fs::read_to_string(r.join("config/identidad.yaml")).ok()))
        .unwrap_or_else(|| "nombre: Neo Orión\nidioma: es-CL\n".into());
    x.escribir(&base_ident, &identidad_yaml(c, &plantilla))?;
    x.escribir(&obj("etc/orion/usuario"), &format!("{usuario}\n"))?;
    // gráfica y procesador de ESTE equipo → el primer arranque deja sus drivers instalados
    let det = if x.simular { json!({}) } else { crate::controladores::detectar() };
    x.escribir(&obj("etc/orion/controladores.json"), &(serde_json::to_string_pretty(&det).unwrap() + "\n"))?;
    if let Some(notas) = det["plan"]["notas"].as_array() {
        let n: Vec<&str> = notas.iter().filter_map(Value::as_str).collect();
        let t = if n.is_empty() { "sin controladores extra".to_string() } else { n.join(" · ") };
        p.p(79, "CONTROLADORES", &t);
    }
    // la sesión a pantalla completa corre con la cuenta nueva (ya no con el usuario del pendrive)
    x.escribir(&obj("etc/systemd/system/orion-sesion.service.d/usuario.conf"), &format!("[Service]\nUser={usuario}\n"))?;
    let casa = obj(&format!("home/{usuario}"));
    x.x(&["mkdir", "-p", &format!("{casa}/Orion"), &format!("{casa}/.orion")])?;
    for n in 1..=3 {
        // frases grabadas en el instalador → huella de voz en el primer arranque
        let w = format!("/run/orion-voz-{n}.wav");
        if Path::new(&w).exists() {
            x.x(&["mkdir", "-p", &format!("{casa}/.orion/voz")])?;
            x.x(&["cp", &w, &format!("{casa}/.orion/voz/frase-{n}.wav")])?;
        }
    }
    x.escribir(&format!("{casa}/.orion/acento"), &format!("{}\n", c["identidad"]["color"].as_str().unwrap_or("#ff2438")))?;
    if !matches!(c["capa"].as_str(), None | Some("") | Some("base")) {
        // capa personal: la que venga en la ISO (CAPA=<nombre>)
        let mut capas: Vec<PathBuf> = std::fs::read_dir(obj("usr/share/orion/capas")).into_iter().flatten().flatten().map(|e| e.path().join("aplicar.sh")).filter(|p| p.exists()).collect();
        capas.sort();
        for a in capas {
            p.p(80, "CAPA PERSONAL", &a.parent().unwrap().file_name().unwrap().to_string_lossy());
            let dentro = format!("/{}", a.strip_prefix(OBJ).unwrap().display());
            x.en_objetivo(&["sudo", "-u", &usuario, "env", &format!("HOME=/home/{usuario}"), "bash", &dentro], None, &[0, 1])?;
        }
    }
    x.en_objetivo(&["chown", "-R", &format!("{usuario}:{usuario}"), &format!("/home/{usuario}")], None, &[0])?;
    x.en_objetivo(&["systemctl", "enable", "orion-sesion.service", "orion-primer-arranque.service"], None, &[0, 1])?;

    // 8 · arranque
    p.p(84, "ARRANQUE", "GRUB · UEFI con arranque seguro + BIOS");
    x.escribir(&obj("etc/default/grub.d/neo-orion.cfg"), "GRUB_DISTRIBUTOR=\"Neo Orión\"\nGRUB_TIMEOUT_STYLE=hidden\nGRUB_TIMEOUT=2\nGRUB_CMDLINE_LINUX_DEFAULT=\"quiet splash loglevel=3\"\nGRUB_DISABLE_OS_PROBER=false\n")?;
    // GRUB escribe root=UUID solo si existe /dev/disk/by-uuid/<UUID>; sin udev pondría /dev/sda3, que cambia
    for (dev, u) in [(&parts.raiz, &u_raiz), (&parts.efi, &u_efi)] {
        let enlace = format!("/dev/disk/by-uuid/{u}");
        if u != "SIMULADO" && !x.simular && !Path::new(&enlace).exists() {
            let _ = std::fs::create_dir_all("/dev/disk/by-uuid");
            let _ = std::os::unix::fs::symlink(dev, &enlace);
        }
    }
    x.escribir(&obj("etc/default/grub.d/10-uuid.cfg"), "GRUB_DISABLE_LINUX_UUID=false\n")?;
    x.en_objetivo(&["update-initramfs", "-c", "-k", "all"], None, &[0, 1])?;
    let efi = ["grub-install", "--target=x86_64-efi", "--efi-directory=/boot/efi", "--uefi-secure-boot"];
    if sin_fat {
        p.p(86, "ARRANQUE", "prueba sin FAT: solo BIOS");
    } else {
        if es_uefi() {
            let mut c = efi.to_vec();
            c.push("--bootloader-id=ubuntu");
            x.en_objetivo(&c, None, &[0])?;
        }
        // siempre también la ruta de respaldo EFI/BOOT/BOOTX64.EFI
        let mut c = efi.to_vec();
        c.extend(["--removable", "--no-nvram"]);
        x.en_objetivo(&c, None, if es_uefi() { &[0, 1] } else { &[0] })?;
    }
    if modo == "borrar" {
        x.en_objetivo(&["grub-install", "--target=i386-pc", disco], None, &[0, 1])?;
    }
    x.en_objetivo(&["update-grub"], None, &[0])?;

    // 9 · primer arranque
    let ias: Vec<&str> = c["ia"]["locales"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    p.ev(Some(94), "PRIMER ARRANQUE", "IA, oído y voz se descargan apenas haya internet", &format!("pendiente: {}", ias.join(", ")), json!({}));
    x.x(&["sync"])?;
    desmontar(x);
    p.ev(Some(100), "LISTO", &format!("Neo Orión quedó en {disco}"), "puedes reiniciar y sacar el pendrive", json!({"terminado": true}));
    Ok(json!({"ok": true, "raiz": parts.raiz, "efi": parts.efi, "comandos": x.hechos.len()}))
}

/// orion-nucleo instalar config.json [--progreso archivo] [--simular]
pub fn principal(args: &[String]) -> i32 {
    let progreso = args.iter().position(|x| x == "--progreso").and_then(|i| args.get(i + 1)).map(PathBuf::from);
    let posicionales: Vec<&String> = args.iter().enumerate().filter(|(i, a)| !a.starts_with("--") && (*i == 0 || args[i - 1] != "--progreso")).map(|x| x.1).collect();
    let Some(cfg) = posicionales.first() else {
        eprintln!("uso: orion-nucleo instalar config.json [--progreso archivo] [--simular]");
        return 2;
    };
    let simular = args.iter().any(|a| a == "--simular");
    let c: Value = match std::fs::read_to_string(cfg).ok().and_then(|s| serde_json::from_str(&s).ok()) {
        Some(c) => c,
        None => {
            eprintln!("no pude leer {cfg}");
            return 2;
        }
    };
    let clave = std::env::var("ORION_CLAVE").ok().filter(|c| !c.is_empty()).unwrap_or_else(|| {
        let mut l = String::new();
        if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
            let _ = std::io::stdin().read_line(&mut l);
        }
        l.trim_end_matches('\n').to_string()
    });
    std::env::remove_var("ORION_CLAVE");
    let mut p = Progreso::new(progreso.as_deref());
    let mut x = Ejecutor::new(simular);
    if !simular && crate::util::sh(&["id", "-u"]).trim() != "0" {
        p.ev(Some(0), "ERROR", "el instalador necesita permisos de administrador", "", json!({"error": true, "terminado": true}));
        return 2;
    }
    let repo = std::env::current_exe().ok().and_then(|e| e.ancestors().nth(4).map(Path::to_path_buf)).filter(|r| r.join("config/identidad.yaml").exists());
    match instalar(&c, &clave, &mut x, &mut p, repo.as_deref()) {
        Ok(_) => {
            if simular {
                for cmd in x.hechos.clone() {
                    p.ev(None, "", "", &format!("$ {}", cmd.join(" ")), json!({}));
                }
            }
            0
        }
        Err(e) => {
            p.ev(None, "ERROR", &e.lines().next().unwrap_or("").chars().take(200).collect::<String>(), &e, json!({"error": true, "terminado": true}));
            if !simular {
                desmontar(&mut x);
            }
            1
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn base() -> Value {
        json!({"idioma": "es-CL", "teclado": "latam", "zona": "America/Santiago", "disco": {"dispositivo": "/dev/sda", "modo": "borrar", "confirmado": true},
               "ia": {"locales": ["gemma3:1b"], "nube": []}, "identidad": {"nombre": "Neo Orión", "color": "#ff2438", "voz": "femenina", "tono": "cercano"},
               "modulos": [], "capa": "base", "cuenta": {"usuario": "davis", "equipo": "taller"}})
    }

    fn planos(x: &Ejecutor) -> Vec<String> {
        x.hechos.iter().map(|c| c.join(" ")).collect()
    }

    #[test]
    fn nombres_de_particion() {
        assert_eq!(particion("/dev/sda", 3), "/dev/sda3");
        assert_eq!(particion("/dev/nvme0n1", 1), "/dev/nvme0n1p1");
        assert_eq!(particion("/dev/mmcblk0", 2), "/dev/mmcblk0p2");
    }

    #[test]
    fn revisar_rechaza() {
        let mut m = base();
        m["disco"] = json!({"dispositivo": "/dev/sda; rm -rf /", "modo": "borrar", "confirmado": true});
        assert!(revisar(&m).iter().any(|e| e.contains("disco")));
        let mut m = base();
        m["disco"]["confirmado"] = json!(false);
        assert!(revisar(&m).iter().any(|e| e.contains("confirmar")));
        let mut m = base();
        m["disco"]["modo"] = json!("manual");
        assert!(revisar(&m).iter().any(|e| e.contains("manual")));
        let mut m = base();
        m["cuenta"]["usuario"] = json!("Root Kit");
        assert!(!revisar(&m).is_empty());
        let mut m = base();
        m["ia"]["locales"] = json!(["gemma3:1b; reboot"]);
        m["identidad"]["color"] = json!("rojo");
        assert_eq!(validar(&m).len(), 2);
        assert!(revisar(&base()).is_empty());
    }

    #[test]
    fn borrar_completo_en_orden() {
        let mut x = Ejecutor::new(true);
        let mut p = Progreso::memoria();
        let r = instalar(&base(), "secreta1", &mut x, &mut p, None).unwrap();
        assert_eq!(r["raiz"], "/dev/sda3");
        assert_eq!(r["efi"], "/dev/sda1");
        let pl = planos(&x);
        let idx = |pref: &str| pl.iter().position(|c| c.starts_with(pref) || c.contains(pref)).unwrap_or_else(|| panic!("falta {pref}"));
        assert!(idx("sgdisk -Z /dev/sda") < idx("mkfs.ext4") && idx("mkfs.ext4") < idx("unsquashfs") && idx("unsquashfs") < idx("useradd") && idx("useradd") < idx("grub-install") && idx("grub-install") < idx("update-grub"));
        assert!(pl.iter().any(|c| c.contains("--target=i386-pc /dev/sda")));
        assert!(pl.iter().any(|c| c.contains("--uefi-secure-boot") && c.contains("--removable")));
        assert!(pl.iter().any(|c| c.starts_with("chroot /target apt-get purge") && c.contains("casper")));
        assert!(!pl.iter().any(|c| c.contains("secreta1"))); // la clave no aparece en comandos
        let ev = p.eventos.unwrap();
        assert_eq!(ev.last().unwrap()["pct"], 100);
        assert_eq!(ev.last().unwrap()["terminado"], true);
        let pcts: Vec<i64> = ev.iter().map(|e| e["pct"].as_i64().unwrap()).collect();
        assert!(pcts.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn nvme_junto_a_windows() {
        let mut c = base();
        c["disco"] = json!({"dispositivo": "/dev/nvme0n1", "modo": "junto"});
        let mut x = Ejecutor::new(true);
        x.salidas = vec![
            ("sgdisk -p".into(), "Number  Start (sector)    End (sector)  Size       Code  Name\n   1  2048  1050623  512.0 MiB  EF00  EFI\n   2  1050624  400000000  190 GiB  0700  Windows\n".into()),
            ("sgdisk -F".into(), "400002048\n".into()),
            ("sgdisk -E".into(), "1000215182\n".into()),
        ];
        let r = instalar(&c, "secreta1", &mut x, &mut Progreso::memoria(), None).unwrap();
        let pl = planos(&x);
        assert_eq!(r["efi"], "/dev/nvme0n1p1"); // reusa la EFI de Windows
        assert!(!pl.iter().any(|c| c.starts_with("mkfs.vfat"))); // y no la formatea
        assert!(!pl.iter().any(|c| c.contains("sgdisk -Z") || c.starts_with("wipefs")));
        assert!(!pl.iter().any(|c| c.contains("i386-pc")));
    }

    #[test]
    fn junto_sin_espacio_y_clave_corta() {
        let mut c = base();
        c["disco"] = json!({"dispositivo": "/dev/sda", "modo": "junto"});
        let mut x = Ejecutor::new(true);
        x.salidas = vec![("sgdisk -F".into(), "1000\n".into()), ("sgdisk -E".into(), "2000\n".into())];
        assert!(instalar(&c, "secreta1", &mut x, &mut Progreso::memoria(), None).unwrap_err().contains("20 GB"));
        assert!(instalar(&base(), "123", &mut Ejecutor::new(true), &mut Progreso::memoria(), None).unwrap_err().contains("clave"));
    }

    #[test]
    fn identidad_igual_que_python() {
        let base_y = "nombre: X\nidioma: en\ncolor: \"#cdf564\"\ntono: directo\nvoz:\n  motor: espeak-ng\n  genero: neutro\n";
        let mut c = base();
        c["voz"] = json!("piper");
        c["identidad"]["tono"] = json!("tecnico");
        let y = identidad_yaml(&c, base_y);
        assert_eq!(y, "nombre: Neo Orión\nidioma: es-CL\ncolor: \"#ff2438\"\ntono: técnico\nvoz:\n  motor: piper\n  genero: femenino\n");
    }
}
