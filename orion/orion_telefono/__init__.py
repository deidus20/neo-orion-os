"""El teléfono como micrófono REAL del PC (iPhone o Android, sin instalar nada en el teléfono).

Cómo funciona:
  1. Neo Orión crea un micrófono virtual en PipeWire («Micrófono del teléfono»). Para el sistema es un
     micrófono más: lo usan el oído de Neo Orión, los juegos en la nube (GeForce Now…), Discord, Meet, OBS.
  2. Levanta una página segura (HTTPS) en la red de la casa y muestra un código QR en el HUD.
  3. El teléfono escanea el QR, abre la página en Safari o Chrome y toca «USAR COMO MICRÓFONO».
     El navegador del teléfono envía el sonido al PC (WebSocket, PCM 16 bits, ~20 ms por paquete).
  4. Mientras el teléfono está conectado, ese micrófono queda como predeterminado; al soltarlo vuelve el anterior.

Seguridad: solo funciona con la clave de un solo uso del QR, solo en la red local y se apaga sola tras
30 minutos sin teléfono. El certificado es propio del equipo (autofirmado): el teléfono avisa la primera vez.
"""
from __future__ import annotations

import base64
import hashlib
import json
import os
import secrets
import socket
import ssl
import struct
import subprocess
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

PUERTO = 7743
NOMBRE = "orion-telefono"
DESCRIPCION = "Micrófono del teléfono"
INACTIVO_S = 30 * 60
GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"


def _dir() -> Path:
    d = Path.home() / ".orion/telefono"
    d.mkdir(parents=True, exist_ok=True)
    return d


def ip_local() -> str:
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    try:
        s.connect(("10.255.255.255", 1))     # no envía nada: solo elige la interfaz de la red local
        return s.getsockname()[0]
    except OSError:
        return "127.0.0.1"
    finally:
        s.close()


def certificado(ip: str, ejecutar=subprocess.run) -> tuple[str, str]:
    """Certificado propio del equipo para HTTPS en la red local (el micrófono del navegador exige HTTPS)."""
    d = _dir()
    crt, key, marca = d / "cert.pem", d / "key.pem", d / "ip"
    if crt.exists() and key.exists() and marca.exists() and marca.read_text() == ip:
        return str(crt), str(key)
    ejecutar(["openssl", "req", "-x509", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:prime256v1", "-nodes",
              "-keyout", str(key), "-out", str(crt), "-days", "825", "-subj", "/CN=Neo Orion",
              "-addext", f"subjectAltName=IP:{ip},DNS:{socket.gethostname()}.local"], check=True, capture_output=True)
    os.chmod(key, 0o600)
    marca.write_text(ip)
    return str(crt), str(key)


# ----------------------------------------------------------------------------------- micrófono virtual
def _pactl(*a, ejecutar=subprocess.run):
    return ejecutar(["pactl", *a], capture_output=True, text=True, timeout=10)


def crear_mic(ejecutar=subprocess.run) -> dict:
    """Crea (si no existe) la fuente virtual de PipeWire donde se escribe el audio del teléfono."""
    fuentes = _pactl("list", "short", "sources", ejecutar=ejecutar).stdout
    if NOMBRE in fuentes:
        return {"ok": True, "ya_estaba": True}
    r = _pactl("load-module", "module-null-sink", "media.class=Audio/Source/Virtual", f"sink_name={NOMBRE}",
               "channel_map=mono", f'sink_properties=device.description="{DESCRIPCION}"', ejecutar=ejecutar)
    if r.returncode != 0:
        return {"ok": False, "error": (r.stderr or "pactl falló").strip()[:200]}
    (_dir() / "modulo").write_text(r.stdout.strip())
    return {"ok": True, "modulo": r.stdout.strip()}


def quitar_mic(ejecutar=subprocess.run) -> None:
    m = _dir() / "modulo"
    if m.exists():
        _pactl("unload-module", m.read_text().strip(), ejecutar=ejecutar)
        m.unlink()


def fuente_predeterminada(ejecutar=subprocess.run) -> str:
    return _pactl("get-default-source", ejecutar=ejecutar).stdout.strip()


# ----------------------------------------------------------------------------------- WebSocket mínimo
def aceptar_ws(clave: str) -> str:
    return base64.b64encode(hashlib.sha1((clave + GUID).encode()).digest()).decode()


def leer_trama(f) -> tuple[int, bytes] | None:
    """Lee una trama WebSocket (cliente → servidor, enmascarada). Devuelve (opcode, datos) o None si se cerró."""
    cab = f.read(2)
    if len(cab) < 2:
        return None
    op, n = cab[0] & 0x0F, cab[1] & 0x7F
    if n == 126:
        n = struct.unpack(">H", f.read(2))[0]
    elif n == 127:
        n = struct.unpack(">Q", f.read(8))[0]
    if n > 1 << 20:
        return None
    mascara = f.read(4) if cab[1] & 0x80 else b"\0\0\0\0"
    datos = bytearray(f.read(n))
    for i in range(len(datos)):
        datos[i] ^= mascara[i % 4]
    return op, bytes(datos)


def trama(datos: bytes, op: int = 1) -> bytes:
    n = len(datos)
    cab = bytes([0x80 | op]) + (bytes([n]) if n < 126 else bytes([126]) + struct.pack(">H", n) if n < 65536
                                 else bytes([127]) + struct.pack(">Q", n))
    return cab + datos


# ----------------------------------------------------------------------------------- servidor
class Puente:
    """Servidor HTTPS + WebSocket que recibe el audio del teléfono y lo escribe en el micrófono virtual."""

    def __init__(self, puerto: int = PUERTO, ejecutar=subprocess.run, reproductor=None):
        self.puerto, self.ejecutar = puerto, ejecutar
        self.reproductor = reproductor or (lambda tasa: subprocess.Popen(
            ["pacat", "--playback", "--raw", "-d", NOMBRE, "--format=s16le", f"--rate={tasa}", "--channels=1",
             "--latency-msec=40", "--client-name=Neo Orion telefono"], stdin=subprocess.PIPE))
        self.clave = secrets.token_urlsafe(6)
        self.srv = None
        self.estado = {"activo": False, "conectado": False, "telefono": "", "nivel": 0.0, "desde": 0, "bytes": 0}
        self._anterior = ""
        self._ultimo = time.time()

    @property
    def url(self) -> str:
        return f"https://{self.ip}:{self.puerto}/mic?t={self.clave}"

    def iniciar(self, tls: bool = True) -> dict:
        if self.srv:
            return self.info()
        self.ip = ip_local()
        mic = crear_mic(self.ejecutar)
        if not mic.get("ok"):
            return {"ok": False, "error": "no se pudo crear el micrófono virtual: " + mic.get("error", "")}
        puente = self

        class H(Manejador):
            p = puente
        self.srv = ThreadingHTTPServer(("0.0.0.0", self.puerto), H)
        if tls:
            crt, key = certificado(self.ip, self.ejecutar)
            ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
            ctx.load_cert_chain(crt, key)
            self.srv.socket = ctx.wrap_socket(self.srv.socket, server_side=True)
        threading.Thread(target=self.srv.serve_forever, daemon=True).start()
        threading.Thread(target=self._vigilar, daemon=True).start()
        self.estado.update(activo=True)
        self._ultimo = time.time()
        return self.info()

    def info(self) -> dict:
        return {"ok": True, "url": self.url, **self.estado, "nombre_mic": DESCRIPCION}

    def parar(self) -> dict:
        if self.srv:
            self.srv.shutdown(); self.srv.server_close(); self.srv = None
        self._soltar()
        quitar_mic(self.ejecutar)
        self.estado.update(activo=False, conectado=False)
        return {"ok": True}

    def _vigilar(self):
        while self.srv:
            time.sleep(15)
            if not self.estado["conectado"] and time.time() - self._ultimo > INACTIVO_S:
                self.parar()

    def _tomar(self, quien: str):
        self._anterior = fuente_predeterminada(self.ejecutar)
        _pactl("set-default-source", NOMBRE, ejecutar=self.ejecutar)
        self.estado.update(conectado=True, telefono=quien, desde=time.time(), bytes=0)

    def _soltar(self):
        if self.estado.get("conectado") and self._anterior and self._anterior != NOMBRE:
            _pactl("set-default-source", self._anterior, ejecutar=self.ejecutar)
        self.estado.update(conectado=False, nivel=0.0)
        self._ultimo = time.time()

    def recibir(self, entrada, salida, quien: str):
        """Atiende una conexión WebSocket ya aceptada hasta que el teléfono la cierra."""
        if self.estado["conectado"]:
            salida.write(trama(json.dumps({"error": "ya hay un teléfono conectado"}).encode())); salida.flush()
            return
        self._tomar(quien)
        rep, tasa = None, 48000
        try:
            while True:
                t = leer_trama(entrada)
                if t is None or t[0] == 8:
                    break
                op, datos = t
                if op == 1:                                   # texto: configuración del teléfono
                    try:
                        tasa = int(json.loads(datos).get("tasa", tasa))
                    except (ValueError, AttributeError):
                        pass
                    if rep is None and 8000 <= tasa <= 96000:
                        rep = self.reproductor(tasa)
                elif op == 2:                                 # binario: audio PCM 16 bits mono
                    if rep is None:
                        rep = self.reproductor(tasa)
                    rep.stdin.write(datos)
                    self.estado["bytes"] += len(datos)
                    if datos:
                        m = memoryview(datos[: len(datos) // 2 * 2]).cast("h")
                        self.estado["nivel"] = round(max(abs(x) for x in m) / 32768, 3) if len(m) else 0.0
                elif op == 9:                                 # ping → pong
                    salida.write(trama(datos, 10)); salida.flush()
        except (OSError, BrokenPipeError, ValueError):
            pass
        finally:
            if rep:
                try:
                    rep.stdin.close(); rep.terminate()
                except (OSError, AttributeError):
                    pass
            self._soltar()


class Manejador(BaseHTTPRequestHandler):
    p: Puente = None

    def log_message(self, *a):
        pass

    def do_GET(self):
        from urllib.parse import parse_qs, urlparse
        u = urlparse(self.path)
        if parse_qs(u.query).get("t", [""])[0] != self.p.clave:
            self.send_error(403, "Escanea el código QR del PC: la clave cambia cada vez.")
            return
        if u.path == "/mic":
            cuerpo = PAGINA.replace("__ACC__", _acento()).encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Cache-Control", "no-store")
            self.send_header("Content-Length", str(len(cuerpo)))
            self.end_headers()
            self.wfile.write(cuerpo)
            return
        if u.path == "/ws" and self.headers.get("Upgrade", "").lower() == "websocket":
            self.send_response(101)
            self.send_header("Upgrade", "websocket")
            self.send_header("Connection", "Upgrade")
            self.send_header("Sec-WebSocket-Accept", aceptar_ws(self.headers.get("Sec-WebSocket-Key", "")))
            self.end_headers()
            self.wfile.flush()
            self.close_connection = True
            self.p.recibir(self.rfile, self.wfile, self.headers.get("User-Agent", "teléfono")[:120])
            return
        self.send_error(404)


def _acento() -> str:
    try:
        c = (Path.home() / ".orion/acento").read_text().strip()
        return c if len(c) == 7 and c.startswith("#") else "#ff2438"
    except OSError:
        return "#ff2438"


_PUENTE: Puente | None = None


def puente() -> Puente:
    global _PUENTE
    if _PUENTE is None:
        _PUENTE = Puente()
    return _PUENTE


def ordenar(accion: str) -> dict:
    """iniciar | estado | parar (lo usan el HUD y la IA)."""
    p = puente()
    if accion == "iniciar":
        return p.iniciar()
    if accion == "parar":
        return p.parar()
    return p.info() if p.srv else {"ok": True, "activo": False}


# Página que abre el teléfono: estética Marathon, un solo botón, medidor de nivel. Sin instalar nada.
PAGINA = r"""<!doctype html><html lang="es"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover"><meta name="theme-color" content="#0b0c0d">
<title>Neo Orión · micrófono</title><style>
:root{--acc:__ACC__}*{box-sizing:border-box}html,body{margin:0;height:100%;background:#0b0c0d;color:#e6e8ea;font:15px ui-monospace,Menlo,monospace}
body{display:flex;flex-direction:column;justify-content:center;gap:22px;padding:28px max(22px,env(safe-area-inset-left))}
.k{font-size:11px;letter-spacing:.18em;color:#8b8e94}.t{font-size:28px;font-weight:700;letter-spacing:.06em;color:var(--acc)}
button{font:inherit;font-weight:700;letter-spacing:.14em;font-size:17px;padding:22px;border:0;background:var(--acc);color:#111;width:100%}
button.off{background:none;border:1px solid var(--acc);color:var(--acc)}
#vu{display:flex;gap:4px;height:46px;align-items:flex-end}#vu i{flex:1;background:#24272b;height:100%}#vu i.on{background:var(--acc)}
.d{font-size:12px;color:#8b8e94;line-height:1.6}.e{color:#ffd400}</style></head><body>
<div><div class="k">NEO ORIÓN // TELÉFONO</div><div class="t">MICRÓFONO</div></div>
<div class="d" id="est">Toca el botón y acepta el permiso del micrófono. Deja esta página abierta y la pantalla encendida mientras hablas.</div>
<div id="vu"></div><button id="b">USAR COMO MICRÓFONO</button>
<div class="d">Funciona como un micrófono real del PC: Neo Orión, juegos en la nube, Discord, Meet. Usa audífonos en el PC para que no haya eco.</div>
<script>
const vu=document.getElementById('vu');for(let i=0;i<24;i++)vu.appendChild(document.createElement('i'));
const est=document.getElementById('est'),b=document.getElementById('b');let ws,ctx,src,nodo,stream,lock;
function nivel(v){const n=Math.min(24,Math.round(v*60));[...vu.children].forEach((x,i)=>x.classList.toggle('on',i<n))}
async function parar(){try{ws&&ws.close()}catch(e){}try{stream&&stream.getTracks().forEach(t=>t.stop())}catch(e){}try{ctx&&ctx.close()}catch(e){}
 try{lock&&lock.release()}catch(e){}ws=ctx=stream=null;b.textContent='USAR COMO MICRÓFONO';b.className='';nivel(0)}
b.onclick=async()=>{if(ws){parar();est.textContent='Micrófono soltado: el PC volvió a su micrófono anterior.';return}
 try{stream=await navigator.mediaDevices.getUserMedia({audio:{echoCancellation:true,noiseSuppression:true,autoGainControl:true,channelCount:1}})}
 catch(e){est.innerHTML='<span class="e">No hay permiso de micrófono.</span> Revisa los permisos del navegador para esta página.';return}
 ctx=new (window.AudioContext||window.webkitAudioContext)();await ctx.resume();
 ws=new WebSocket('wss://'+location.host+'/ws'+location.search);ws.binaryType='arraybuffer';
 ws.onopen=()=>{ws.send(JSON.stringify({tasa:ctx.sampleRate}));est.textContent='Conectado: el PC te está escuchando.';b.textContent='SOLTAR';b.className='off'};
 ws.onmessage=m=>{try{const r=JSON.parse(m.data);if(r.error){est.innerHTML='<span class="e">'+r.error+'</span>';parar()}}catch(e){}};
 ws.onclose=()=>{if(ws){est.textContent='Se cortó la conexión. Toca el botón para volver a conectar.';parar()}};
 src=ctx.createMediaStreamSource(stream);nodo=ctx.createScriptProcessor(1024,1,1);
 nodo.onaudioprocess=e=>{const f=e.inputBuffer.getChannelData(0);let m=0;const s=new Int16Array(f.length);
  for(let i=0;i<f.length;i++){const v=Math.max(-1,Math.min(1,f[i]));s[i]=v*32767;const a=Math.abs(v);if(a>m)m=a}
  nivel(m);if(ws&&ws.readyState===1&&ws.bufferedAmount<64000)ws.send(s.buffer)};
 const mudo=ctx.createGain();mudo.gain.value=0;src.connect(nodo);nodo.connect(mudo);mudo.connect(ctx.destination);
 try{lock=await navigator.wakeLock.request('screen')}catch(e){}};
document.addEventListener('visibilitychange',()=>{if(document.hidden&&ws)est.innerHTML='<span class="e">La página quedó atrás: algunos teléfonos cortan el micrófono. Vuelve a esta pestaña.</span>'});
</script></body></html>"""
