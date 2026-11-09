//! Navegador (antes orion_navegador): Neo Orión abre Chrome o Firefox a la vista y, cuando se lo piden, maneja ESE
//! MISMO navegador: abre pestañas, navega, hace clic y escribe, con una franja que avisa y un cursor de bloque que
//! muestra dónde va a tocar. Sin Playwright ni Selenium: Chrome se maneja por su protocolo DevTools (WebSocket) y
//! Firefox por Marionette, su canal oficial.
//!
//! En los chats de IA (claude.ai, chatgpt.com, gemini…) NO se automatiza nada: sus términos lo prohíben y arriesga la
//! cuenta. Ahí se abre a la vista con el encargo copiado (puente) o se usa la extensión oficial «Claude in Chrome».

use crate::util::{correr, existe_programa};
use base64::Engine;
use regex::Regex;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

pub const PUERTO: u16 = 9222;
pub const PUERTO_FF: u16 = 2828;
const NAVEGADORES: [&str; 6] = ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser", "flatpak:org.chromium.Chromium", "firefox"];
const FIREFOXES: [&str; 3] = ["firefox", "firefox-esr", "flatpak:org.mozilla.firefox"];
const ATAJOS: [(&str, &str); 13] = [
    ("youtube", "https://www.youtube.com"), ("gmail", "https://mail.google.com"), ("drive", "https://drive.google.com"), ("github", "https://github.com"),
    ("whatsapp", "https://web.whatsapp.com"), ("maps", "https://maps.google.com"), ("sii", "https://www.sii.cl"), ("sec", "https://www.sec.cl"),
    ("claude", "https://claude.ai/new"), ("chatgpt", "https://chatgpt.com"), ("notebooklm", "https://notebooklm.google.com"),
    ("gemini notebook", "https://notebooklm.google.com"), ("open notebook", "http://127.0.0.1:8502"),
];

pub const MARCA_JS: &str = r#"(() => { if (document.getElementById('orion-piloto')) return;
  const b = document.createElement('div'); b.id = 'orion-piloto';
  b.textContent = '■ NEO ORIÓN MANEJANDO · mueve el mouse o di «yo sigo» para tomar el control';
  b.style.cssText = 'position:fixed;left:0;right:0;top:0;z-index:2147483647;background:#ff2438;color:#111;font:12px monospace;padding:4px 10px;letter-spacing:.05em;pointer-events:none';
  document.documentElement.appendChild(b);
  const c = document.createElement('div'); c.id = 'orion-cursor';
  c.style.cssText = 'position:fixed;width:14px;height:14px;background:#ff2438;z-index:2147483647;pointer-events:none;transition:left .35s steps(6),top .35s steps(6);left:50%;top:50%';
  document.documentElement.appendChild(c); })()"#;
pub const QUITAR_JS: &str = "(() => { ['orion-piloto','orion-cursor'].forEach(i => { const e = document.getElementById(i); if (e) e.remove(); }); })()";

/// JS que encuentra un elemento (selector CSS, XPath «//…» o texto visible), mueve el cursor de bloque hasta él y
/// devuelve el centro; `accion` = apuntar | click | escribir | leer.
fn js_elemento(que: &str, accion: &str, texto: &str) -> String {
    format!(
        r#"(() => {{
  const q = {q}, acc = {a}, txt = {t};
  let el = null;
  if (/^[#.\[]/.test(q)) el = document.querySelector(q);
  else {{
    const xp = q.startsWith('//') ? q : `//*[normalize-space(text())=${{JSON.stringify(q)}}]`;
    el = document.evaluate(xp, document, null, XPathResult.FIRST_ORDERED_NODE_TYPE, null).singleNodeValue;
    if (!el && !q.startsWith('//')) el = [...document.querySelectorAll('a,button,[role=button],input,label,span,div')].find(e => (e.innerText || '').trim().includes(q));
  }}
  if (!el) return {{ok: false, error: 'no encontré: ' + q}};
  if (acc === 'leer') return {{ok: true, texto: [el.innerText]}};
  el.scrollIntoView({{block: 'center'}});
  const r = el.getBoundingClientRect(), c = document.getElementById('orion-cursor');
  if (c) {{ c.style.left = (r.x + r.width / 2 - 7) + 'px'; c.style.top = (r.y + r.height / 2 - 7) + 'px'; }}
  if (acc === 'escribir') {{ el.focus(); if ('value' in el) {{ el.value = ''; el.dispatchEvent(new Event('input', {{bubbles: true}})); }} }}
  return {{ok: true, x: r.x + r.width / 2, y: r.y + r.height / 2}};
}})()"#,
        q = serde_json::to_string(que).unwrap(),
        a = serde_json::to_string(accion).unwrap(),
        t = serde_json::to_string(texto).unwrap()
    )
}

/// «abre youtube» → URL; «busca cables RIC 18» → búsqueda; una URL → tal cual.
pub fn destino(texto: &str) -> String {
    let t = texto.trim();
    if Regex::new(r"^(https?|file)://|^about:").unwrap().is_match(t) {
        return t.into();
    }
    if Regex::new(r"^[\w-]+(\.[\w-]+)+(/.*)?$").unwrap().is_match(t) {
        return format!("https://{t}");
    }
    let clave = t.to_lowercase();
    if let Some((_, u)) = ATAJOS.iter().find(|(k, _)| *k == clave) {
        return u.to_string();
    }
    let q: String = t
        .bytes()
        .map(|b| match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'.' | b'-' | b'~' => (b as char).to_string(),
            b' ' => "+".into(),
            o => format!("%{o:02X}"),
        })
        .collect();
    format!("https://www.google.com/search?q={q}")
}

pub fn anfitrion(url: &str) -> String {
    Regex::new(r"^[a-zA-Z][a-zA-Z0-9+.-]*://([^/:?#]*)").unwrap().captures(url).map(|c| c[1].to_lowercase()).unwrap_or_default()
}

/// Los chats de IA y sus servicios no se manejan por código.
pub fn automatizable(url: &str) -> bool {
    !Regex::new(r"(^|\.)(claude\.ai|anthropic\.com|chatgpt\.com|openai\.com|gemini\.google\.com|grok\.com|x\.ai|chat\.deepseek\.com|copilot\.microsoft\.com|perplexity\.ai|notebooklm\.google(\.com)?|notebook\.google\.com)$")
        .unwrap()
        .is_match(&anfitrion(url))
}

pub fn cual(familia: &str) -> Option<Vec<String>> {
    let lista: &[&str] = if familia == "firefox" { &FIREFOXES } else { &NAVEGADORES };
    for n in lista {
        if let Some(fp) = n.strip_prefix("flatpak:") {
            if existe_programa("flatpak") && correr(&["flatpak", "info", fp]).0 == 0 {
                return Some(vec!["flatpak".into(), "run".into(), fp.into()]);
            }
        } else if existe_programa(n) {
            return Some(vec![n.to_string()]);
        }
    }
    None
}

/// Abre el navegador NORMAL a la vista, listo para que Neo Orión lo maneje (control solo desde este PC).
pub fn abrir(texto: &str, familia: &str) -> Value {
    let url = if texto.is_empty() { "https://www.google.com".to_string() } else { destino(texto) };
    let Some(nav) = cual(familia) else {
        let falta = if familia == "firefox" { "Instala Firefox: orion install firefox (repositorio oficial de Mozilla)" } else { "Instala Chrome: orion install chrome (o flatpak install flathub org.chromium.Chromium)" };
        return json!({"ok": false, "url": url, "falta": falta});
    };
    let casa = crate::util::casa();
    let mut args: Vec<String> = nav[1..].to_vec();
    let es_ff = nav.last().unwrap().contains("firefox");
    if es_ff {
        let perfil = casa.join(".orion/firefox-perfil");
        let _ = std::fs::create_dir_all(&perfil);
        args.extend(["--marionette".into(), "--profile".into(), perfil.to_string_lossy().into(), "--new-window".into(), url.clone()]);
    } else {
        // perfil propio: Chrome solo permite control remoto fuera del perfil por defecto
        args.extend([format!("--user-data-dir={}", casa.join(".orion/chrome-perfil").display()), format!("--remote-debugging-port={PUERTO}"), "--remote-debugging-address=127.0.0.1".into(), "--start-maximized".into(), "--no-first-run".into(), url.clone()]);
    }
    match std::process::Command::new(&nav[0]).args(&args).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn() {
        Ok(_) => json!({"ok": true, "url": url, "navegador": if es_ff { "firefox".into() } else { nav.last().unwrap().clone() }, "piloto": true}),
        Err(e) => json!({"ok": false, "url": url, "error": e.to_string()}),
    }
}

// ------------------------------------------------------------------ WebSocket mínimo (cliente) para DevTools

pub struct Ws {
    s: TcpStream,
}

impl Ws {
    pub fn conectar(url: &str) -> Result<Self, String> {
        let c = Regex::new(r"^ws://([^/]+)(/.*)$").unwrap().captures(url).ok_or("dirección WebSocket inválida")?;
        let (host, ruta) = (c[1].to_string(), c[2].to_string());
        let mut s = TcpStream::connect(&host).map_err(|e| e.to_string())?;
        s.set_read_timeout(Some(Duration::from_secs(30))).ok();
        let clave = base64::engine::general_purpose::STANDARD.encode(std::array::from_fn::<u8, 16, _>(|i| (i as u8).wrapping_mul(37).wrapping_add(crate::util::ahora_s() as u8)));
        write!(s, "GET {ruta} HTTP/1.1\r\nHost: {host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {clave}\r\nSec-WebSocket-Version: 13\r\n\r\n").map_err(|e| e.to_string())?;
        let mut r = BufReader::new(s.try_clone().map_err(|e| e.to_string())?);
        let mut l = String::new();
        r.read_line(&mut l).map_err(|e| e.to_string())?;
        if !l.contains(" 101 ") {
            return Err(format!("el navegador rechazó la conexión: {}", l.trim()));
        }
        loop {
            l.clear();
            r.read_line(&mut l).map_err(|e| e.to_string())?;
            if l.trim().is_empty() {
                break;
            }
        }
        Ok(Ws { s })
    }

    pub fn enviar(&mut self, texto: &str) -> std::io::Result<()> {
        let datos = texto.as_bytes();
        let mut f = vec![0x81u8];
        match datos.len() {
            n if n < 126 => f.push(0x80 | n as u8),
            n if n < 65536 => {
                f.push(0x80 | 126);
                f.extend((n as u16).to_be_bytes());
            }
            n => {
                f.push(0x80 | 127);
                f.extend((n as u64).to_be_bytes());
            }
        }
        let m: [u8; 4] = [0x4f, 0x52, 0x49, 0x4e];
        f.extend(m);
        f.extend(datos.iter().enumerate().map(|(i, b)| b ^ m[i % 4]));
        self.s.write_all(&f)
    }

    /// Un mensaje completo (junta fragmentos; contesta los ping).
    pub fn recibir(&mut self) -> std::io::Result<String> {
        let mut msg = vec![];
        loop {
            let mut h = [0u8; 2];
            self.s.read_exact(&mut h)?;
            let (fin, op) = (h[0] & 0x80 != 0, h[0] & 0x0f);
            let mut n = (h[1] & 0x7f) as u64;
            if n == 126 {
                let mut b = [0u8; 2];
                self.s.read_exact(&mut b)?;
                n = u16::from_be_bytes(b) as u64;
            } else if n == 127 {
                let mut b = [0u8; 8];
                self.s.read_exact(&mut b)?;
                n = u64::from_be_bytes(b);
            }
            let mut d = vec![0u8; n as usize];
            self.s.read_exact(&mut d)?;
            match op {
                0x9 => {
                    let mut p = vec![0x8A, 0x80 | d.len() as u8, 0, 0, 0, 0];
                    p.extend(&d);
                    self.s.write_all(&p)?;
                }
                0x8 => return Err(std::io::Error::new(std::io::ErrorKind::ConnectionAborted, "cerrado")),
                _ => {
                    msg.extend(d);
                    if fin {
                        return Ok(String::from_utf8_lossy(&msg).into());
                    }
                }
            }
        }
    }
}

/// Una pestaña de Chrome por DevTools.
pub struct Cdp {
    ws: Ws,
    id: u64,
}

impl Cdp {
    pub fn conectar(ws_url: &str) -> Result<Self, String> {
        Ok(Cdp { ws: Ws::conectar(ws_url)?, id: 0 })
    }

    pub fn llamar(&mut self, metodo: &str, params: Value) -> Result<Value, String> {
        self.id += 1;
        let id = self.id;
        self.ws.enviar(&json!({"id": id, "method": metodo, "params": params}).to_string()).map_err(|e| e.to_string())?;
        loop {
            let m: Value = serde_json::from_str(&self.ws.recibir().map_err(|e| e.to_string())?).unwrap_or_default();
            if m["id"] == id {
                if let Some(e) = m.get("error") {
                    return Err(e["message"].as_str().unwrap_or("error de DevTools").into());
                }
                return Ok(m["result"].clone());
            }
        }
    }

    pub fn evaluar(&mut self, js: &str) -> Result<Value, String> {
        let r = self.llamar("Runtime.evaluate", json!({"expression": js, "returnByValue": true, "awaitPromise": true}))?;
        if let Some(e) = r.get("exceptionDetails") {
            return Err(e["text"].as_str().unwrap_or("error de JavaScript").into());
        }
        Ok(r["result"]["value"].clone())
    }
}

fn http_json(puerto: u16, ruta: &str, metodo: &str) -> Result<Value, String> {
    ureq::request(metodo, &format!("http://127.0.0.1:{puerto}{ruta}")).timeout(Duration::from_secs(5)).call().map_err(|e| e.to_string())?.into_json().map_err(|e| e.to_string())
}

// ------------------------------------------------------------------ Marionette (Firefox)

pub struct Marionette {
    s: TcpStream,
    id: u64,
}

impl Marionette {
    pub fn conectar(puerto: u16) -> Result<Self, String> {
        let s = TcpStream::connect(("127.0.0.1", puerto)).map_err(|e| e.to_string())?;
        s.set_read_timeout(Some(Duration::from_secs(30))).ok();
        let mut m = Marionette { s, id: 0 };
        m.leer()?; // saludo
        m.orden("WebDriver:NewSession", json!({"capabilities": {}}))?;
        Ok(m)
    }

    fn leer(&mut self) -> Result<Value, String> {
        let mut largo = vec![];
        loop {
            let mut b = [0u8; 1];
            self.s.read_exact(&mut b).map_err(|e| e.to_string())?;
            if b[0] == b':' {
                break;
            }
            largo.push(b[0]);
        }
        let n: usize = String::from_utf8_lossy(&largo).parse().map_err(|_| "Marionette respondió algo raro")?;
        let mut d = vec![0u8; n];
        self.s.read_exact(&mut d).map_err(|e| e.to_string())?;
        serde_json::from_slice(&d).map_err(|e| e.to_string())
    }

    pub fn orden(&mut self, nombre: &str, params: Value) -> Result<Value, String> {
        self.id += 1;
        let p = json!([0, self.id, nombre, params]).to_string();
        write!(self.s, "{}:{p}", p.len()).map_err(|e| e.to_string())?;
        loop {
            let r = self.leer()?;
            if r[1] == self.id {
                if !r[2].is_null() {
                    return Err(r[2]["message"].as_str().unwrap_or("error de Firefox").into());
                }
                return Ok(r[3].clone());
            }
        }
    }

    pub fn evaluar(&mut self, js: &str) -> Result<Value, String> {
        Ok(self.orden("WebDriver:ExecuteScript", json!({"script": format!("return {js}"), "args": []}))?["value"].clone())
    }
}

// ------------------------------------------------------------------ el piloto

/// Neo Orión maneja el navegador que la persona está viendo.
pub enum Piloto {
    Chrome,
    Firefox(Marionette),
}

impl Piloto {
    pub fn conectar(familia: &str) -> Result<Self, String> {
        if familia == "firefox" {
            return Marionette::conectar(PUERTO_FF).map(Piloto::Firefox);
        }
        http_json(PUERTO, "/json/version", "GET")?;
        Ok(Piloto::Chrome)
    }

    fn pestanas_chrome() -> Result<Vec<Value>, String> {
        Ok(http_json(PUERTO, "/json/list", "GET")?.as_array().cloned().unwrap_or_default().into_iter().filter(|t| t["type"] == "page").collect())
    }

    fn activa_chrome() -> Result<Cdp, String> {
        let p = Self::pestanas_chrome()?;
        let t = p.first().ok_or("no hay pestañas abiertas")?;
        Cdp::conectar(t["webSocketDebuggerUrl"].as_str().ok_or("pestaña sin DevTools")?)
    }

    pub fn pestanas(&mut self) -> Result<Vec<Value>, String> {
        match self {
            Piloto::Chrome => Ok(Self::pestanas_chrome()?.iter().enumerate().map(|(i, t)| json!({"n": i, "titulo": t["title"], "url": t["url"]})).collect()),
            Piloto::Firefox(m) => {
                let actual = m.orden("WebDriver:GetWindowHandle", json!({}))?["value"].clone();
                let lista = m.orden("WebDriver:GetWindowHandles", json!({}))?;
                let mut out = vec![];
                for (i, h) in lista.as_array().cloned().unwrap_or_default().iter().enumerate() {
                    m.orden("WebDriver:SwitchToWindow", json!({"handle": h}))?;
                    out.push(json!({"n": i, "titulo": m.orden("WebDriver:GetTitle", json!({}))?["value"], "url": m.orden("WebDriver:GetCurrentURL", json!({}))?["value"]}));
                }
                m.orden("WebDriver:SwitchToWindow", json!({"handle": actual}))?;
                Ok(out)
            }
        }
    }

    fn url_actual(&mut self) -> Result<String, String> {
        Ok(match self {
            Piloto::Chrome => Self::pestanas_chrome()?.first().and_then(|t| t["url"].as_str().map(String::from)).unwrap_or_default(),
            Piloto::Firefox(m) => m.orden("WebDriver:GetCurrentURL", json!({}))?["value"].as_str().unwrap_or("").into(),
        })
    }

    fn evaluar(&mut self, js: &str) -> Result<Value, String> {
        match self {
            Piloto::Chrome => Self::activa_chrome()?.evaluar(js),
            Piloto::Firefox(m) => m.evaluar(js),
        }
    }

    pub fn ir(&mut self, texto: &str) -> Result<String, String> {
        let url = destino(texto);
        if !automatizable(&url) {
            return Err("Ese sitio no se maneja por código; ábrelo y úsalo tú (o con Claude in Chrome).".into());
        }
        match self {
            Piloto::Chrome => {
                let mut c = Self::activa_chrome()?;
                c.llamar("Page.enable", json!({}))?;
                c.llamar("Page.navigate", json!({"url": url}))?;
                for _ in 0..50 {
                    std::thread::sleep(Duration::from_millis(200));
                    if c.evaluar("document.readyState")? == "complete" {
                        break;
                    }
                }
                let _ = c.evaluar(MARCA_JS);
                Ok(c.evaluar("document.title")?.as_str().unwrap_or("").into())
            }
            Piloto::Firefox(m) => {
                m.orden("WebDriver:Navigate", json!({"url": url}))?;
                let _ = m.evaluar(MARCA_JS);
                Ok(m.orden("WebDriver:GetTitle", json!({}))?["value"].as_str().unwrap_or("").into())
            }
        }
    }

    pub fn nueva(&mut self, texto: &str) -> Result<(), String> {
        match self {
            Piloto::Chrome => {
                http_json(PUERTO, "/json/new?about:blank", "PUT")?;
            }
            Piloto::Firefox(m) => {
                let h = m.orden("WebDriver:NewWindow", json!({"type": "tab"}))?;
                m.orden("WebDriver:SwitchToWindow", json!({"handle": h["handle"]}))?;
            }
        }
        if !texto.is_empty() {
            self.ir(texto)?;
        }
        Ok(())
    }

    fn sobre_elemento(&mut self, que: &str, accion: &str, texto: &str) -> Result<Value, String> {
        if !automatizable(&self.url_actual()?) {
            return Err("Este sitio no se maneja por código.".into());
        }
        let _ = self.evaluar(MARCA_JS);
        let r = self.evaluar(&js_elemento(que, accion, texto))?;
        if r["ok"] != true {
            return Err(r["error"].as_str().unwrap_or("no encontré el elemento").into());
        }
        if accion != "leer" {
            std::thread::sleep(Duration::from_millis(420)); // que se vea el cursor llegar
        }
        Ok(r)
    }

    pub fn click(&mut self, que: &str) -> Result<(), String> {
        let r = self.sobre_elemento(que, "click", "")?;
        match self {
            Piloto::Chrome => {
                // clic de verdad (eventos del mouse), igual que si la persona apretara
                let mut c = Self::activa_chrome()?;
                for tipo in ["mousePressed", "mouseReleased"] {
                    c.llamar("Input.dispatchMouseEvent", json!({"type": tipo, "x": r["x"], "y": r["y"], "button": "left", "clickCount": 1}))?;
                }
            }
            Piloto::Firefox(m) => {
                let el = m.orden("WebDriver:FindElement", json!({"using": if que.starts_with(['#', '.', '[']) { "css selector" } else { "xpath" }, "value": if que.starts_with(['#', '.', '[']) || que.starts_with("//") { que.to_string() } else { format!("//*[normalize-space(text())={}]", serde_json::to_string(que).unwrap()) }}))?;
                let id = el["value"].as_object().and_then(|o| o.values().next()).cloned().unwrap_or_default();
                m.orden("WebDriver:ElementClick", json!({"id": id}))?;
            }
        }
        Ok(())
    }

    pub fn escribir(&mut self, que: &str, texto: &str) -> Result<(), String> {
        self.sobre_elemento(que, "escribir", texto)?;
        match self {
            Piloto::Chrome => {
                Self::activa_chrome()?.llamar("Input.insertText", json!({"text": texto}))?;
            }
            Piloto::Firefox(m) => {
                m.evaluar(&format!("(() => {{ const e = document.activeElement; e.value = {t}; e.dispatchEvent(new Event('input', {{bubbles: true}})); return true; }})()", t = serde_json::to_string(texto).unwrap()))?;
            }
        }
        Ok(())
    }

    pub fn leer(&mut self, que: &str) -> Result<Value, String> {
        Ok(self.sobre_elemento(que, "leer", "")?["texto"].clone())
    }

    /// La persona toma el control: se quitan la franja y el cursor.
    pub fn soltar(&mut self) {
        let _ = self.evaluar(QUITAR_JS);
    }
}

/// En segundo plano (Chromium sin ventana): pasos [{"ir": url}, {"click": …}, {"escribir": [sel, texto]},
/// {"leer": sel}, {"captura": "archivo.png"}]. Solo sitios que lo permiten.
pub fn automatizar(url: &str, pasos: &[Value]) -> Value {
    if !automatizable(url) {
        return json!({"ok": false, "motivo": "Este sitio no permite manejarlo por código. Lo abro a la vista para que lo uses tú, o usa la extensión oficial Claude in Chrome.", "url": url});
    }
    let Some(nav) = cual("chrome").filter(|n| !n.last().unwrap().contains("firefox")) else {
        return json!({"ok": false, "falta": "Instala Chrome o Chromium: orion install chrome"});
    };
    let puerto = 9333;
    let perfil = std::env::temp_dir().join(format!("orion-auto-{}", std::process::id()));
    let mut args: Vec<String> = nav[1..].to_vec();
    args.extend(["--headless=new".into(), format!("--remote-debugging-port={puerto}"), format!("--user-data-dir={}", perfil.display()), "--no-first-run".into(), "about:blank".into()]);
    let Ok(mut hijo) = std::process::Command::new(&nav[0]).args(&args).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn() else {
        return json!({"ok": false, "error": "no pude abrir el navegador"});
    };
    let r = (|| -> Result<Vec<Value>, String> {
        let mut lista = Err(String::new());
        for _ in 0..50 {
            std::thread::sleep(Duration::from_millis(200));
            lista = http_json(puerto, "/json/list", "GET");
            if lista.is_ok() {
                break;
            }
        }
        let t = lista?.as_array().cloned().unwrap_or_default().into_iter().find(|t| t["type"] == "page").ok_or("sin pestaña")?;
        let mut c = Cdp::conectar(t["webSocketDebuggerUrl"].as_str().unwrap_or(""))?;
        c.llamar("Page.enable", json!({}))?;
        let ir = |c: &mut Cdp, u: &str| -> Result<(), String> {
            c.llamar("Page.navigate", json!({"url": u}))?;
            for _ in 0..75 {
                std::thread::sleep(Duration::from_millis(200));
                if c.evaluar("document.readyState")? == "complete" {
                    break;
                }
            }
            Ok(())
        };
        ir(&mut c, url)?;
        let mut salida = vec![];
        for paso in pasos {
            let Some((k, v)) = paso.as_object().and_then(|o| o.iter().next()) else { continue };
            match k.as_str() {
                "ir" => {
                    let u = v.as_str().unwrap_or("");
                    if automatizable(u) { ir(&mut c, u)? } else { salida.push(json!({"saltado": u})) }
                }
                "click" => {
                    let e = c.evaluar(&js_elemento(v.as_str().unwrap_or(""), "click", ""))?;
                    for tipo in ["mousePressed", "mouseReleased"] {
                        c.llamar("Input.dispatchMouseEvent", json!({"type": tipo, "x": e["x"], "y": e["y"], "button": "left", "clickCount": 1}))?;
                    }
                }
                "escribir" => {
                    c.evaluar(&js_elemento(v[0].as_str().unwrap_or(""), "escribir", ""))?;
                    c.llamar("Input.insertText", json!({"text": v[1]}))?;
                }
                "leer" => salida.push(json!({"leido": c.evaluar(&js_elemento(v.as_str().unwrap_or("body"), "leer", ""))?["texto"]})),
                "captura" => {
                    let png = c.llamar("Page.captureScreenshot", json!({"format": "png"}))?;
                    let b = base64::engine::general_purpose::STANDARD.decode(png["data"].as_str().unwrap_or("")).map_err(|e| e.to_string())?;
                    std::fs::write(crate::util::expandir(v.as_str().unwrap_or("captura.png")), b).map_err(|e| e.to_string())?;
                    salida.push(json!({"captura": v}));
                }
                _ => {}
            }
        }
        Ok(salida)
    })();
    let _ = hijo.kill();
    let _ = hijo.wait();
    let _ = std::fs::remove_dir_all(&perfil);
    match r {
        Ok(s) => json!({"ok": true, "resultado": s}),
        Err(e) => json!({"ok": false, "error": e}),
    }
}

/// La herramienta MCP `navegador`.
pub fn herramienta(a: &Value) -> Value {
    let t = |k: &str| a[k].as_str().unwrap_or("").to_string();
    let (accion, que, en) = (t("accion"), t("que"), if t("en").is_empty() { "chrome".to_string() } else { t("en") });
    match accion.as_str() {
        "abrir" => return abrir(&que, &en),
        "automatizar" => return automatizar(&destino(&que), a["pasos"].as_array().map(Vec::as_slice).unwrap_or(&[])),
        _ => {}
    }
    let mut pi = match Piloto::conectar(&en) {
        Ok(p) => p,
        Err(e) => return json!({"ok": false, "error": format!("Chrome no está abierto en modo piloto ({e}). Primero: navegador('abrir').")}),
    };
    let r = match accion.as_str() {
        "pestanas" => pi.pestanas().map(|p| json!({"ok": true, "pestanas": p})),
        "nueva" => pi.nueva(&que).and_then(|_| pi.pestanas()).map(|p| json!({"ok": true, "pestanas": p})),
        "ir" => pi.ir(&que).map(|ti| json!({"ok": true, "titulo": ti})),
        "click" => pi.click(&que).map(|_| json!({"ok": true})),
        "escribir" => pi.escribir(&que, &t("texto")).map(|_| json!({"ok": true})),
        "leer" => pi.leer(if que.is_empty() { "body" } else { &que }).map(|x| json!({"ok": true, "texto": x})),
        "soltar" => Ok(json!({"ok": true, "control": "persona"})),
        o => Ok(json!({"ok": false, "error": format!("acción desconocida: {o}")})),
    };
    pi.soltar();
    r.unwrap_or_else(|e| json!({"ok": false, "error": e}))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn destinos_y_sitios_prohibidos() {
        assert_eq!(destino("youtube"), "https://www.youtube.com");
        assert_eq!(destino("sii.cl/servicios"), "https://sii.cl/servicios");
        assert_eq!(destino("https://ejemplo.cl/a?b=1"), "https://ejemplo.cl/a?b=1");
        assert_eq!(destino("normativa RIC 18"), "https://www.google.com/search?q=normativa+RIC+18");
        assert_eq!(destino("cañería ½\""), "https://www.google.com/search?q=ca%C3%B1er%C3%ADa+%C2%BD%22");
        assert!(!automatizable("https://claude.ai/new"));
        assert!(!automatizable("https://gemini.google.com/app"));
        assert!(!automatizable("https://notebooklm.google.com"));
        assert!(automatizable("https://www.sii.cl"));
        assert!(automatizable("https://google.com/search?q=claude.ai"));
        assert!(js_elemento("Enviar", "click", "").contains("normalize-space"));
    }
}
