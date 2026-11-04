import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import orion_navegador as nv  # noqa: E402

assert nv.destino("youtube") == "https://www.youtube.com"
assert nv.destino("sec.cl/normativa") == "https://sec.cl/normativa"
assert nv.destino("cables RIC 18").startswith("https://www.google.com/search?q=cables+RIC+18")
assert not nv.automatizable("https://claude.ai/new") and not nv.automatizable("https://chatgpt.com/")
assert nv.automatizable("https://www.sii.cl/") and nv.automatizable("https://github.com/x")
r = nv.automatizar("https://claude.ai/new", [{"escribir": ["textarea", "hola"]}])
assert not r["ok"] and "Claude in Chrome" in r["motivo"]

lanzado = {}
nv.cual = lambda fam="chrome": ["google-chrome"]
r = nv.abrir("youtube", ejecutar=lambda cmd: lanzado.setdefault("cmd", cmd))
c = lanzado["cmd"]
assert r["ok"] and r["piloto"] and c[0] == "google-chrome" and c[-1] == "https://www.youtube.com"
assert "--remote-debugging-address=127.0.0.1" in c and any(x.startswith("--user-data-dir=") for x in c) and "--kiosk" not in c

# Automatización real en un sitio local (Playwright está en este entorno)
import tempfile
f = Path(tempfile.mkdtemp()) / "p.html"
f.write_text('<input id="q"><button onclick="document.getElementById(\'r\').textContent=document.getElementById(\'q\').value.toUpperCase()">Ir</button><p id="r"></p>')
r = nv.automatizar(f.as_uri(), [{"escribir": ["#q", "neo orión"]}, {"click": "Ir"}, {"leer": "#r"}], visible=False)
assert r["ok"] and r["resultado"][0]["leido"] == ["NEO ORIÓN"], r
print("navegador ok")

# Piloto sobre un Chrome VISIBLE: se conecta al mismo navegador por CDP, abre pestañas y hace clic a la vista
import subprocess, time, urllib.request
from playwright.sync_api import sync_playwright
with sync_playwright() as pw:
    exe = pw.chromium.executable_path
perfil = tempfile.mkdtemp()
srv = subprocess.Popen([exe, "--headless=new", "--no-sandbox", f"--user-data-dir={perfil}", "--remote-debugging-port=9339", "--remote-debugging-address=127.0.0.1", "about:blank"],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
for _ in range(50):
    try:
        urllib.request.urlopen("http://127.0.0.1:9339/json/version", timeout=1); break
    except Exception:
        time.sleep(.2)
pi = nv.Piloto("http://127.0.0.1:9339")
pi.nueva(f.as_uri())
assert len(pi.pestanas()) >= 2
pi.escribir("#q", "pestaña nueva"); pi.click("Ir")
assert pi.leer("#r") == ["PESTAÑA NUEVA"]
assert pi.activa().evaluate("!!document.getElementById('orion-piloto')")       # franja visible mientras maneja
pi.soltar(); assert not pi.activa().evaluate("!!document.getElementById('orion-piloto')")
try:
    pi.ir("https://claude.ai/new"); raise SystemExit("debió negarse")
except PermissionError:
    pass
pi.cerrar_conexion(); srv.terminate()
print("piloto ok")

# Firefox: se abre normal con Marionette y el piloto se conecta al MISMO Firefox
lanzado.clear(); nv.cual = lambda fam="chrome": ["firefox"] if fam == "firefox" else ["google-chrome"]
r = nv.abrir("youtube", ejecutar=lambda cmd: lanzado.setdefault("cmd", cmd), familia="firefox")
c = lanzado["cmd"]
assert r["ok"] and r["navegador"] == "firefox" and "--marionette" in c and "--profile" in c and c[-1] == "https://www.youtube.com"

class FalsoDriver:   # mismo contrato que selenium.webdriver.Firefox
    def __init__(self): self.h, self.cur, self.urls, self.js = ["t1"], "t1", {"t1": "about:blank"}, []
    window_handles = property(lambda s: s.h); current_window_handle = property(lambda s: s.cur)
    title = property(lambda s: "Título " + s.urls[s.cur]); current_url = property(lambda s: s.urls[s.cur])
    class _S:
        def __init__(s, d): s.d = d
        def window(s, h): s.d.cur = h
        def new_window(s, k): n = f"t{len(s.d.h)+1}"; s.d.h.append(n); s.d.urls[n] = "about:blank"; s.d.cur = n
    switch_to = property(lambda s: FalsoDriver._S(s))
    def get(self, u): self.urls[self.cur] = u
    def execute_script(self, js, *a): self.js.append(js)
fd = FalsoDriver()
pf = nv.PilotoFirefox(crear=lambda puerto: fd)
pf.nueva("github.com")
assert len(pf.pestanas()) == 2 and fd.urls[fd.cur] == "https://github.com"
assert any("orion-piloto" in j for j in fd.js)
try:
    pf.ir("chatgpt.com"); raise SystemExit("debió negarse")
except PermissionError:
    pass
print("firefox ok")
