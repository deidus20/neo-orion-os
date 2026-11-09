//! El teléfono como micrófono REAL del PC (iPhone o Android, sin instalar nada en el teléfono).
//!
//! 1. Se crea un micrófono virtual en PipeWire («Micrófono del teléfono»): para el sistema es un micrófono más.
//! 2. Se levanta una página segura (HTTPS, certificado propio) en la red de la casa; el HUD muestra el QR.
//! 3. El teléfono la abre, toca «USAR COMO MICRÓFONO» y manda el sonido por WebSocket (PCM 16 bits).
//! 4. Mientras está conectado es el micrófono predeterminado; al soltarlo vuelve el anterior.
//! Seguridad: clave de un solo uso en el QR, solo red local, se apaga sola tras 30 minutos sin teléfono.

use crate::util::{ip_local, Ejecutor};
use base64::Engine;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
#[cfg(test)]
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const PUERTO: u16 = 7743;
pub const NOMBRE: &str = "orion-telefono";
pub const DESCRIPCION: &str = "Micrófono del teléfono";
const INACTIVO: Duration = Duration::from_secs(30 * 60);
const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
const PAGINA: &str = include_str!("telefono.html");

fn dir() -> PathBuf {
    let d = crate::util::casa().join(".orion/telefono");
    let _ = std::fs::create_dir_all(&d);
    d
}

/// Certificado propio del equipo (el micrófono del navegador exige HTTPS).
pub fn certificado(ip: &str, ejecutar: Ejecutor) -> Result<(PathBuf, PathBuf), String> {
    let d = dir();
    let (crt, key, marca) = (d.join("cert.pem"), d.join("key.pem"), d.join("ip"));
    if crt.exists() && key.exists() && std::fs::read_to_string(&marca).ok().as_deref() == Some(ip) {
        return Ok((crt, key));
    }
    let host = crate::util::sh(&["hostname"]).trim().to_string();
    let san = format!("subjectAltName=IP:{ip},DNS:{host}.local");
    let (c, _, e) = ejecutar(&["openssl", "req", "-x509", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:prime256v1", "-nodes", "-keyout", &key.to_string_lossy(), "-out", &crt.to_string_lossy(), "-days", "825", "-subj", "/CN=Neo Orion", "-addext", &san]);
    if c != 0 {
        return Err(format!("openssl: {e}"));
    }
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600));
    let _ = std::fs::write(&marca, ip);
    Ok((crt, key))
}

// ----------------------------------------------------------------------------------- micrófono virtual
pub fn crear_mic(ejecutar: Ejecutor) -> Value {
    if ejecutar(&["pactl", "list", "short", "sources"]).1.contains(NOMBRE) {
        return json!({"ok": true, "ya_estaba": true});
    }
    let desc = format!("sink_properties=device.description=\"{DESCRIPCION}\"");
    let (c, out, err) = ejecutar(&["pactl", "load-module", "module-null-sink", "media.class=Audio/Source/Virtual", &format!("sink_name={NOMBRE}"), "channel_map=mono", &desc]);
    if c != 0 {
        return json!({"ok": false, "error": if err.trim().is_empty() { "pactl falló".to_string() } else { err.trim().chars().take(200).collect() }});
    }
    let _ = std::fs::write(dir().join("modulo"), out.trim());
    json!({"ok": true, "modulo": out.trim()})
}

pub fn quitar_mic(ejecutar: Ejecutor) {
    let m = dir().join("modulo");
    if let Ok(id) = std::fs::read_to_string(&m) {
        ejecutar(&["pactl", "unload-module", id.trim()]);
        let _ = std::fs::remove_file(m);
    }
}

// ----------------------------------------------------------------------------------- WebSocket mínimo
pub fn aceptar_ws(clave: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(sha1_smol::Sha1::from(format!("{clave}{GUID}")).digest().bytes())
}

/// Lee una trama (cliente → servidor, enmascarada). Devuelve (opcode, datos) o None si se cerró.
pub fn leer_trama(f: &mut impl Read) -> Option<(u8, Vec<u8>)> {
    let mut cab = [0u8; 2];
    f.read_exact(&mut cab).ok()?;
    let (op, mut n) = (cab[0] & 0x0F, (cab[1] & 0x7F) as u64);
    if n == 126 {
        let mut b = [0u8; 2];
        f.read_exact(&mut b).ok()?;
        n = u16::from_be_bytes(b) as u64;
    } else if n == 127 {
        let mut b = [0u8; 8];
        f.read_exact(&mut b).ok()?;
        n = u64::from_be_bytes(b);
    }
    if n > 1 << 20 {
        return None;
    }
    let mut mascara = [0u8; 4];
    if cab[1] & 0x80 != 0 {
        f.read_exact(&mut mascara).ok()?;
    }
    let mut d = vec![0u8; n as usize];
    f.read_exact(&mut d).ok()?;
    d.iter_mut().enumerate().for_each(|(i, b)| *b ^= mascara[i % 4]);
    Some((op, d))
}

pub fn trama(datos: &[u8], op: u8) -> Vec<u8> {
    let n = datos.len();
    let mut v = vec![0x80 | op];
    if n < 126 {
        v.push(n as u8);
    } else if n < 65536 {
        v.push(126);
        v.extend((n as u16).to_be_bytes());
    } else {
        v.push(127);
        v.extend((n as u64).to_be_bytes());
    }
    v.extend(datos);
    v
}

// ----------------------------------------------------------------------------------- el puente
pub type Reproductor = Box<dyn Fn(u32) -> Option<Box<dyn Write + Send>> + Send + Sync>;

pub struct Estado {
    pub activo: bool,
    pub conectado: bool,
    pub telefono: String,
    pub nivel: f64,
    pub bytes: u64,
    anterior: String,
    ultimo: Instant,
}

pub struct Puente {
    pub puerto: u16,
    pub clave: String,
    pub ip: String,
    pub estado: Arc<Mutex<Estado>>,
    ejecutar: fn(&[&str]) -> crate::util::Salida,
    reproductor: Arc<Reproductor>,
    parar: Arc<AtomicBool>,
    puerto_real: u16,
}

fn reproductor_pacat() -> Reproductor {
    Box::new(|tasa| {
        let h: Child = Command::new("pacat")
            .args(["--playback", "--raw", "-d", NOMBRE, "--format=s16le", &format!("--rate={tasa}"), "--channels=1", "--latency-msec=40", "--client-name=Neo Orion telefono"])
            .stdin(Stdio::piped())
            .spawn()
            .ok()?;
        h.stdin.map(|s| Box::new(s) as Box<dyn Write + Send>)
    })
}

fn clave_azar() -> String {
    let mut b = [0u8; 6];
    let _ = std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b));
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

impl Puente {
    pub fn new(puerto: u16, ejecutar: fn(&[&str]) -> crate::util::Salida, reproductor: Option<Reproductor>) -> Self {
        Puente {
            puerto,
            clave: clave_azar(),
            ip: String::new(),
            estado: Arc::new(Mutex::new(Estado { activo: false, conectado: false, telefono: String::new(), nivel: 0.0, bytes: 0, anterior: String::new(), ultimo: Instant::now() })),
            ejecutar,
            reproductor: Arc::new(reproductor.unwrap_or_else(reproductor_pacat)),
            parar: Arc::new(AtomicBool::new(false)),
            puerto_real: puerto,
        }
    }

    pub fn url(&self) -> String {
        format!("https://{}:{}/mic?t={}", self.ip, self.puerto_real, self.clave)
    }

    pub fn info(&self) -> Value {
        let e = self.estado.lock().unwrap();
        json!({"ok": true, "url": self.url(), "activo": e.activo, "conectado": e.conectado, "telefono": e.telefono, "nivel": e.nivel, "bytes": e.bytes, "nombre_mic": DESCRIPCION})
    }

    /// Enciende el puente. `tls=false` solo para pruebas.
    pub fn iniciar(&mut self, tls: bool, ip: Option<String>) -> Value {
        if self.estado.lock().unwrap().activo {
            return self.info();
        }
        self.ip = ip.unwrap_or_else(ip_local);
        let mic = crear_mic(&self.ejecutar);
        if mic["ok"] != true {
            return json!({"ok": false, "error": format!("no se pudo crear el micrófono virtual: {}", mic["error"].as_str().unwrap_or(""))});
        }
        let tls_cfg = if tls {
            let (crt, key) = match certificado(&self.ip, &crate::util::correr) {
                Ok(x) => x,
                Err(e) => return json!({"ok": false, "error": e}),
            };
            match config_tls(&crt, &key) {
                Ok(c) => Some(Arc::new(c)),
                Err(e) => return json!({"ok": false, "error": e}),
            }
        } else {
            None
        };
        let l = match TcpListener::bind(("0.0.0.0", self.puerto)) {
            Ok(l) => l,
            Err(e) => return json!({"ok": false, "error": e.to_string()}),
        };
        self.puerto_real = l.local_addr().map(|a| a.port()).unwrap_or(self.puerto);
        let _ = l.set_nonblocking(true);
        self.parar.store(false, Ordering::SeqCst);
        {
            let mut e = self.estado.lock().unwrap();
            e.activo = true;
            e.ultimo = Instant::now();
        }
        let (estado, parar, clave, ejecutar, rep) = (self.estado.clone(), self.parar.clone(), self.clave.clone(), self.ejecutar, self.reproductor.clone());
        std::thread::spawn(move || {
            while !parar.load(Ordering::SeqCst) {
                match l.accept() {
                    Ok((s, _)) => {
                        let (estado, clave, rep, tls) = (estado.clone(), clave.clone(), rep.clone(), tls_cfg.clone());
                        std::thread::spawn(move || {
                            let _ = s.set_nonblocking(false);
                            let _ = s.set_read_timeout(Some(Duration::from_secs(120)));
                            match tls {
                                Some(cfg) => {
                                    if let Ok(con) = rustls::ServerConnection::new(cfg) {
                                        atender(rustls::StreamOwned::new(con, s), &estado, &clave, &rep, ejecutar);
                                    }
                                }
                                None => atender(s, &estado, &clave, &rep, ejecutar),
                            }
                        });
                    }
                    Err(_) => {
                        std::thread::sleep(Duration::from_millis(80));
                        let e = estado.lock().unwrap();
                        if !e.conectado && e.ultimo.elapsed() > INACTIVO {
                            parar.store(true, Ordering::SeqCst);
                        }
                    }
                }
            }
            estado.lock().unwrap().activo = false;
            quitar_mic(&ejecutar);
        });
        self.info()
    }

    pub fn parar(&mut self) -> Value {
        self.parar.store(true, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(150));
        let mut e = self.estado.lock().unwrap();
        e.activo = false;
        e.conectado = false;
        json!({"ok": true})
    }
}

fn config_tls(crt: &std::path::Path, key: &std::path::Path) -> Result<rustls::ServerConfig, String> {
    let certs: Vec<_> = rustls_pemfile::certs(&mut BufReader::new(std::fs::File::open(crt).map_err(|e| e.to_string())?)).flatten().collect();
    let llave = rustls_pemfile::private_key(&mut BufReader::new(std::fs::File::open(key).map_err(|e| e.to_string())?)).map_err(|e| e.to_string())?.ok_or("sin llave")?;
    rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_no_client_auth()
        .with_single_cert(certs, llave)
        .map_err(|e| e.to_string())
}

fn responder(s: &mut impl Write, codigo: &str, tipo: &str, cuerpo: &[u8]) {
    let _ = write!(s, "HTTP/1.1 {codigo}\r\nContent-Type: {tipo}\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", cuerpo.len());
    let _ = s.write_all(cuerpo);
    let _ = s.flush();
}

fn acento() -> String {
    let c = crate::util::leer(&crate::util::casa().join(".orion/acento").to_string_lossy());
    if c.len() == 7 && c.starts_with('#') { c } else { "#ff2438".into() }
}

fn atender<S: Read + Write>(mut s: S, estado: &Arc<Mutex<Estado>>, clave: &str, rep: &Arc<Reproductor>, ejecutar: fn(&[&str]) -> crate::util::Salida) {
    // cabecera HTTP
    let mut cab = Vec::new();
    let mut b = [0u8; 1];
    while !cab.ends_with(b"\r\n\r\n") && cab.len() < 16384 {
        if s.read_exact(&mut b).is_err() {
            return;
        }
        cab.push(b[0]);
    }
    let texto = String::from_utf8_lossy(&cab).to_string();
    let mut lineas = BufReader::new(texto.as_bytes()).lines().map_while(Result::ok);
    let primera = lineas.next().unwrap_or_default();
    let ruta = primera.split_whitespace().nth(1).unwrap_or("/").to_string();
    let heads: Vec<(String, String)> = lineas.filter_map(|l| l.split_once(':').map(|(k, v)| (k.trim().to_lowercase(), v.trim().to_string()))).collect();
    let h = |k: &str| heads.iter().find(|(x, _)| x == k).map(|x| x.1.clone()).unwrap_or_default();
    let (camino, consulta) = ruta.split_once('?').unwrap_or((&ruta, ""));
    let t = consulta.split('&').find_map(|p| p.strip_prefix("t=")).unwrap_or("");
    if t != clave {
        return responder(&mut s, "403 Forbidden", "text/plain; charset=utf-8", "Escanea el código QR del PC: la clave cambia cada vez.".as_bytes());
    }
    if camino == "/mic" {
        return responder(&mut s, "200 OK", "text/html; charset=utf-8", PAGINA.replace("__ACC__", &acento()).as_bytes());
    }
    if camino != "/ws" || !h("upgrade").eq_ignore_ascii_case("websocket") {
        return responder(&mut s, "404 Not Found", "text/plain", b"no existe");
    }
    let _ = write!(s, "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n", aceptar_ws(&h("sec-websocket-key")));
    let _ = s.flush();
    {
        let mut e = estado.lock().unwrap();
        if e.conectado {
            drop(e);
            let _ = s.write_all(&trama(json!({"error": "ya hay un teléfono conectado"}).to_string().as_bytes(), 1));
            return;
        }
        e.anterior = ejecutar(&["pactl", "get-default-source"]).1.trim().to_string();
        e.conectado = true;
        e.bytes = 0;
        e.telefono = h("user-agent").chars().take(120).collect();
    }
    ejecutar(&["pactl", "set-default-source", NOMBRE]);
    let (mut salida, mut tasa): (Option<Box<dyn Write + Send>>, u32) = (None, 48000);
    while let Some((op, datos)) = leer_trama(&mut s) {
        match op {
            8 => break,
            1 => {
                if let Some(t) = serde_json::from_slice::<Value>(&datos).ok().and_then(|v| v["tasa"].as_u64()) {
                    tasa = t as u32;
                }
                if salida.is_none() && (8000..=96000).contains(&tasa) {
                    salida = rep(tasa);
                }
            }
            2 => {
                if salida.is_none() {
                    salida = rep(tasa);
                }
                if let Some(o) = salida.as_mut() {
                    if o.write_all(&datos).is_err() {
                        break;
                    }
                }
                let m = crate::voz::a_muestras(&datos);
                let mut e = estado.lock().unwrap();
                e.bytes += datos.len() as u64;
                e.nivel = crate::util::redondear(m.iter().map(|x| (*x as f64).abs()).fold(0.0, f64::max) / 32768.0, 3);
            }
            9 => {
                let _ = s.write_all(&trama(&datos, 10));
            }
            _ => {}
        }
    }
    drop(salida);
    let mut e = estado.lock().unwrap();
    if !e.anterior.is_empty() && e.anterior != NOMBRE {
        ejecutar(&["pactl", "set-default-source", &e.anterior]);
    }
    e.conectado = false;
    e.nivel = 0.0;
    e.ultimo = Instant::now();
}

static PUENTE: Mutex<Option<Puente>> = Mutex::new(None);

/// iniciar | estado | parar (lo usan el HUD y la IA).
pub fn ordenar(accion: &str) -> Value {
    let mut g = PUENTE.lock().unwrap();
    let p = g.get_or_insert_with(|| Puente::new(PUERTO, crate::util::correr, None));
    match accion {
        "iniciar" => p.iniciar(true, None),
        "parar" => p.parar(),
        _ if p.estado.lock().unwrap().activo => p.info(),
        _ => json!({"ok": true, "activo": false}),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    fn trama_cliente(d: &[u8], op: u8) -> Vec<u8> {
        let m = [0x11u8, 0x22, 0x33, 0x44];
        let mut v = vec![0x80 | op];
        if d.len() < 126 {
            v.push(0x80 | d.len() as u8);
        } else {
            v.push(0x80 | 126);
            v.extend((d.len() as u16).to_be_bytes());
        }
        v.extend(m);
        v.extend(d.iter().enumerate().map(|(i, b)| b ^ m[i % 4]));
        v
    }

    static FUENTE: Mutex<String> = Mutex::new(String::new());
    static LLAMADAS: AtomicUsize = AtomicUsize::new(0);
    fn pactl_falso(c: &[&str]) -> crate::util::Salida {
        LLAMADAS.fetch_add(1, Ordering::SeqCst);
        match c.get(1).copied() {
            Some("load-module") => (0, "536870913\n".into(), String::new()),
            Some("get-default-source") => (0, format!("{}\n", FUENTE.lock().unwrap()), String::new()),
            Some("set-default-source") => {
                *FUENTE.lock().unwrap() = c[2].into();
                (0, String::new(), String::new())
            }
            _ => (0, String::new(), String::new()),
        }
    }

    #[test]
    fn trama_y_aceptacion() {
        assert_eq!(aceptar_ws("dGhlIHNhbXBsZSBub25jZQ=="), "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="); // RFC 6455
        for n in [5usize, 300] {
            let d: Vec<u8> = (0..n).map(|i| i as u8).collect();
            assert_eq!(leer_trama(&mut std::io::Cursor::new(trama_cliente(&d, 2))), Some((2, d)));
        }
    }

    #[test]
    fn puente_de_punta_a_punta() {
        *FUENTE.lock().unwrap() = "alsa_input.pci".into();
        let recibido = Arc::new(Mutex::new((0u32, Vec::<u8>::new())));
        let r2 = recibido.clone();
        struct Caja(Arc<Mutex<(u32, Vec<u8>)>>);
        impl Write for Caja {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().1.extend(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let rep: Reproductor = Box::new(move |t| {
            r2.lock().unwrap().0 = t;
            Some(Box::new(Caja(r2.clone())))
        });
        let mut p = Puente::new(0, pactl_falso, Some(rep));
        let info = p.iniciar(false, Some("127.0.0.1".into()));
        assert_eq!(info["ok"], true, "{info}");
        assert!(info["url"].as_str().unwrap().contains("?t="));
        let puerto = p.puerto_real;
        // sin clave: prohibido
        let mut s = TcpStream::connect(("127.0.0.1", puerto)).unwrap();
        s.write_all(b"GET /mic?t=mala HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        let mut r = String::new();
        let _ = s.read_to_string(&mut r);
        assert!(r.contains("403"));
        // la página
        let mut s = TcpStream::connect(("127.0.0.1", puerto)).unwrap();
        write!(s, "GET /mic?t={} HTTP/1.1\r\nHost: x\r\n\r\n", p.clave).unwrap();
        let mut r = String::new();
        let _ = s.read_to_string(&mut r);
        assert!(r.contains("USAR COMO MICR"));
        // WebSocket con audio
        let mut s = TcpStream::connect(("127.0.0.1", puerto)).unwrap();
        write!(s, "GET /ws?t={} HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: MDEyMzQ1Njc4OWFiY2RlZg==\r\n\r\n", p.clave).unwrap();
        let mut b = [0u8; 200];
        let n = s.read(&mut b).unwrap();
        assert!(String::from_utf8_lossy(&b[..n]).contains("101"));
        s.write_all(&trama_cliente(br#"{"tasa":44100}"#, 1)).unwrap();
        let audio: Vec<u8> = [0i16, 16384, -32768, 100].iter().flat_map(|x| x.to_le_bytes()).collect();
        s.write_all(&trama_cliente(&audio, 2)).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert!(p.estado.lock().unwrap().conectado);
        assert_eq!(*FUENTE.lock().unwrap(), NOMBRE); // el teléfono es el micrófono
        assert_eq!(recibido.lock().unwrap().0, 44100);
        assert_eq!(recibido.lock().unwrap().1, audio);
        assert_eq!(p.estado.lock().unwrap().nivel, 1.0);
        s.write_all(&trama_cliente(b"", 8)).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert!(!p.estado.lock().unwrap().conectado);
        assert_eq!(*FUENTE.lock().unwrap(), "alsa_input.pci"); // vuelve el anterior
        p.parar();
    }

    #[test]
    fn pagina_por_https() {
        if !crate::util::existe_programa("openssl") || !crate::util::existe_programa("curl") {
            return;
        }
        let mut p = Puente::new(0, pactl_falso, Some(Box::new(|_| None)));
        assert_eq!(p.iniciar(true, Some("127.0.0.1".into()))["ok"], true);
        let url = p.url().replace("127.0.0.1", "localhost");
        let (c, out, err) = crate::util::correr(&["curl", "-sk", "--noproxy", "*", &url]);
        assert!(c == 0 && out.contains("USAR COMO MICR"), "{err}");
        p.parar();
    }
}
