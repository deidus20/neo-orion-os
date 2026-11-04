"""Navegador: Neo Orión abre Chrome y trabaja en la web.

- abrir(url) / buscar(texto): abre Chrome (o Chromium) a la vista, como lo harías tú.
- automatizar(url, pasos): para sitios que lo permiten (formularios, descargar un PDF, leer una
  tabla), Neo Orión maneja el navegador por código con Playwright y muestra el resultado.
- En los chats de IA (claude.ai, chatgpt.com, gemini…) NO se automatiza el teclado: sus términos
  lo prohíben y arriesga tu cuenta. Ahí se abre a la vista con el encargo copiado (orion_puente),
  o se usa la extensión oficial "Claude in Chrome", que es la forma permitida de que Claude
  maneje tu Chrome.
"""
from __future__ import annotations

import json
import re
import shutil
import subprocess
import urllib.parse

NAVEGADORES = ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser", "flatpak:org.chromium.Chromium", "firefox"]
FIREFOXES = ["firefox", "firefox-esr", "flatpak:org.mozilla.firefox"]
SIN_AUTOMATIZAR = re.compile(r"(^|\.)(claude\.ai|anthropic\.com|chatgpt\.com|openai\.com|gemini\.google\.com|grok\.com|x\.ai|"
                             r"chat\.deepseek\.com|copilot\.microsoft\.com|perplexity\.ai|notebooklm\.google(\.com)?|notebook\.google\.com)$")
ATAJOS = {"youtube": "https://www.youtube.com", "gmail": "https://mail.google.com", "drive": "https://drive.google.com",
          "github": "https://github.com", "whatsapp": "https://web.whatsapp.com", "maps": "https://maps.google.com",
          "sii": "https://www.sii.cl", "sec": "https://www.sec.cl", "claude": "https://claude.ai/new", "chatgpt": "https://chatgpt.com",
          "notebooklm": "https://notebooklm.google.com", "gemini notebook": "https://notebooklm.google.com",
          "open notebook": "http://127.0.0.1:8502"}


def cual(familia: str = "chrome") -> list[str] | None:
    for n in (FIREFOXES if familia == "firefox" else NAVEGADORES):
        if n.startswith("flatpak:"):
            if shutil.which("flatpak"):
                r = subprocess.run(["flatpak", "info", n[8:]], capture_output=True)
                if r.returncode == 0:
                    return ["flatpak", "run", n[8:]]
        elif shutil.which(n):
            return [n]
    return None


def destino(texto: str) -> str:
    """'abre youtube' → URL; 'busca cables RIC 18' → búsqueda; una URL → tal cual."""
    t = texto.strip()
    if re.match(r"^(https?|file)://|^about:", t):
        return t
    if re.match(r"^[\w-]+(\.[\w-]+)+(/.*)?$", t):
        return "https://" + t
    clave = t.lower().strip()
    if clave in ATAJOS:
        return ATAJOS[clave]
    return "https://www.google.com/search?q=" + urllib.parse.quote_plus(t)


def automatizable(url: str) -> bool:
    host = urllib.parse.urlparse(url).hostname or ""
    return not SIN_AUTOMATIZAR.search(host)


PUERTO = 9222
PERFIL = "~/.orion/chrome-perfil"   # perfil propio: Chrome solo permite control remoto fuera del perfil por defecto


PUERTO_FF = 2828          # Marionette, el canal oficial de Firefox para manejarlo por código
PERFIL_FF = "~/.orion/firefox-perfil"


def abrir(texto: str = "", ejecutar=subprocess.Popen, familia: str = "chrome") -> dict:
    """Abre el navegador NORMAL (con pestañas, barra, extensiones) a la vista. Tú lo usas como siempre;
    además queda listo para que Neo Orión lo maneje cuando se lo pidas (control solo desde este PC).
    familia: chrome | firefox."""
    import os
    url = destino(texto) if texto else "https://www.google.com"
    nav = cual(familia)
    if not nav:
        falta = ("Instala Firefox: orion install firefox (repositorio oficial de Mozilla)" if familia == "firefox"
                 else "Instala Chrome: orion install chrome (o flatpak install flathub org.chromium.Chromium)")
        return {"ok": False, "url": url, "falta": falta}
    if "firefox" in nav[-1]:
        perfil = os.path.expanduser(PERFIL_FF); os.makedirs(perfil, exist_ok=True)
        ejecutar([*nav, "--marionette", "--profile", perfil, "--new-window", url])
        return {"ok": True, "url": url, "navegador": "firefox", "piloto": True}
    perfil = os.path.expanduser(PERFIL)
    ejecutar([*nav, f"--user-data-dir={perfil}", f"--remote-debugging-port={PUERTO}", "--remote-debugging-address=127.0.0.1",
              "--start-maximized", "--no-first-run", url])
    return {"ok": True, "url": url, "navegador": nav[-1], "piloto": True}


MARCA_JS = """(() => { if (document.getElementById('orion-piloto')) return;
  const b = document.createElement('div'); b.id = 'orion-piloto';
  b.textContent = '■ NEO ORIÓN MANEJANDO · mueve el mouse o di «yo sigo» para tomar el control';
  b.style.cssText = 'position:fixed;left:0;right:0;top:0;z-index:2147483647;background:#ff2438;color:#111;font:12px monospace;padding:4px 10px;letter-spacing:.05em;pointer-events:none';
  document.documentElement.appendChild(b);
  const c = document.createElement('div'); c.id = 'orion-cursor';
  c.style.cssText = 'position:fixed;width:14px;height:14px;background:#ff2438;z-index:2147483647;pointer-events:none;transition:left .35s steps(6),top .35s steps(6);left:50%;top:50%';
  document.documentElement.appendChild(c); })()"""
QUITAR_JS = "(() => { ['orion-piloto','orion-cursor'].forEach(i => { const e = document.getElementById(i); if (e) e.remove(); }); })()"


def piloto(familia: str = "chrome"):
    """El piloto que corresponde al navegador abierto."""
    return PilotoFirefox() if familia == "firefox" else Piloto()


class Piloto:
    """Neo Orión maneja el MISMO Chrome que estás viendo: abre pestañas, navega, hace clic y escribe
    a la vista, con una franja que avisa y un cursor de bloque que muestra dónde va a tocar."""

    def __init__(self, endpoint: str = f"http://127.0.0.1:{PUERTO}"):
        from playwright.sync_api import sync_playwright
        self._pw = sync_playwright().start()
        self.nav = self._pw.chromium.connect_over_cdp(endpoint)
        self.ctx = self.nav.contexts[0] if self.nav.contexts else self.nav.new_context()

    # --- pestañas
    def pestanas(self) -> list[dict]:
        return [{"n": i, "titulo": p.title(), "url": p.url} for i, p in enumerate(self.ctx.pages)]

    def nueva(self, texto: str = ""):
        pg = self.ctx.new_page()
        if texto:
            self.ir(texto, pg)
        return pg

    def activa(self):
        return self.ctx.pages[-1] if self.ctx.pages else self.nueva()

    def cerrar(self, n: int):
        self.ctx.pages[n].close()

    # --- acciones a la vista
    def _marca(self, pg):
        try:
            pg.evaluate(MARCA_JS)
        except Exception:
            pass

    def _apuntar(self, pg, loc):
        try:
            bb = loc.bounding_box()
            if bb:
                pg.evaluate(f"(() => {{ const c = document.getElementById('orion-cursor'); if (c) {{ c.style.left = '{bb['x'] + bb['width'] / 2 - 7}px'; c.style.top = '{bb['y'] + bb['height'] / 2 - 7}px'; }} }})()")
                pg.wait_for_timeout(420)
        except Exception:
            pass

    def ir(self, texto: str, pg=None):
        url = destino(texto)
        pg = pg or self.activa()
        if not automatizable(url):
            raise PermissionError("Ese sitio no se maneja por código; ábrelo y úsalo tú (o con Claude in Chrome).")
        pg.goto(url); pg.bring_to_front(); self._marca(pg)
        return pg.title()

    def _loc(self, pg, que: str):
        return pg.locator(que).first if que.startswith(("#", ".", "[", "//")) else pg.get_by_text(que).first

    def click(self, que: str):
        pg = self.activa(); self._marca(pg)
        if not automatizable(pg.url):
            raise PermissionError("Este sitio no se maneja por código.")
        loc = self._loc(pg, que); self._apuntar(pg, loc); loc.click()

    def escribir(self, que: str, texto: str):
        pg = self.activa(); self._marca(pg)
        if not automatizable(pg.url):
            raise PermissionError("Este sitio no se maneja por código.")
        loc = self._loc(pg, que); self._apuntar(pg, loc); loc.fill(texto)

    def leer(self, que: str = "body") -> list[str]:
        return self._loc(self.activa(), que).all_inner_texts()

    def soltar(self):
        """La persona toma el control: se quitan la franja y el cursor."""
        for pg in self.ctx.pages:
            try:
                pg.evaluate(QUITAR_JS)
            except Exception:
                pass

    def cerrar_conexion(self):
        self.soltar(); self._pw.stop()


def automatizar(url: str, pasos: list[dict], visible: bool = True) -> dict:
    """pasos: [{"ir": url}, {"click": "texto o selector"}, {"escribir": ["selector", "texto"]},
    {"leer": "selector"}, {"captura": "archivo.png"}]. Solo sitios que lo permiten."""
    if not automatizable(url):
        return {"ok": False, "motivo": "Este sitio no permite manejarlo por código. Lo abro a la vista para que lo uses tú, "
                                       "o usa la extensión oficial Claude in Chrome.", "url": url}
    try:
        from playwright.sync_api import sync_playwright
    except ImportError:
        return {"ok": False, "falta": "pip install playwright && playwright install chromium"}
    salida = []
    with sync_playwright() as p:
        b = p.chromium.launch(headless=not visible)
        pg = b.new_page()
        pg.goto(url)
        for paso in pasos:
            (k, v), = paso.items()
            if k == "ir":
                if not automatizable(v):
                    salida.append({"saltado": v}); continue
                pg.goto(v)
            elif k == "click":
                (pg.get_by_text(v).first if not v.startswith(("#", ".", "[")) else pg.locator(v).first).click()
            elif k == "escribir":
                pg.locator(v[0]).fill(v[1])
            elif k == "leer":
                salida.append({"leido": pg.locator(v).all_inner_texts()})
            elif k == "captura":
                pg.screenshot(path=v); salida.append({"captura": v})
        b.close()
    return {"ok": True, "resultado": salida}



class PilotoFirefox:
    """Lo mismo que Piloto, pero en Firefox: se conecta al Firefox que ya está abierto (Marionette)
    con geckodriver --connect-existing. Mismas acciones, misma franja y mismo cursor de bloque."""

    def __init__(self, puerto: int = PUERTO_FF, crear=None):
        crear = crear or self._crear
        self.d = crear(puerto)

    @staticmethod
    def _crear(puerto):
        from selenium import webdriver
        from selenium.webdriver.firefox.service import Service
        srv = Service(service_args=["--connect-existing", "--marionette-host", "127.0.0.1", "--marionette-port", str(puerto)])
        return webdriver.Firefox(service=srv)

    def pestanas(self) -> list[dict]:
        actual, out = self.d.current_window_handle, []
        for i, h in enumerate(self.d.window_handles):
            self.d.switch_to.window(h); out.append({"n": i, "titulo": self.d.title, "url": self.d.current_url})
        self.d.switch_to.window(actual)
        return out

    def nueva(self, texto: str = ""):
        self.d.switch_to.new_window("tab")
        if texto:
            self.ir(texto)

    def _marca(self):
        try:
            self.d.execute_script("return " + MARCA_JS)
        except Exception:
            pass

    def ir(self, texto: str):
        url = destino(texto)
        if not automatizable(url):
            raise PermissionError("Ese sitio no se maneja por código; ábrelo y úsalo tú.")
        self.d.get(url); self._marca()
        return self.d.title

    def _el(self, que: str):
        from selenium.webdriver.common.by import By
        if que.startswith(("#", ".", "[")):
            return self.d.find_element(By.CSS_SELECTOR, que)
        if que.startswith("//"):
            return self.d.find_element(By.XPATH, que)
        return self.d.find_element(By.XPATH, f"//*[normalize-space(text())={json.dumps(que)}]")

    def _apuntar(self, el):
        try:
            self.d.execute_script("""const r = arguments[0].getBoundingClientRect(), c = document.getElementById('orion-cursor');
              if (c) { c.style.left = (r.x + r.width / 2 - 7) + 'px'; c.style.top = (r.y + r.height / 2 - 7) + 'px'; }""", el)
            import time; time.sleep(.42)
        except Exception:
            pass

    def click(self, que: str):
        if not automatizable(self.d.current_url):
            raise PermissionError("Este sitio no se maneja por código.")
        self._marca(); el = self._el(que); self._apuntar(el); el.click()

    def escribir(self, que: str, texto: str):
        if not automatizable(self.d.current_url):
            raise PermissionError("Este sitio no se maneja por código.")
        self._marca(); el = self._el(que); self._apuntar(el); el.clear(); el.send_keys(texto)

    def leer(self, que: str = "body") -> list[str]:
        return [self._el(que).text]

    def soltar(self):
        actual = self.d.current_window_handle
        for h in self.d.window_handles:
            self.d.switch_to.window(h)
            try:
                self.d.execute_script(QUITAR_JS)
            except Exception:
                pass
        self.d.switch_to.window(actual)

    def cerrar_conexion(self):
        self.soltar()   # no se llama quit(): el Firefox es de la persona y sigue abierto
