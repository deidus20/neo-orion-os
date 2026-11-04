"""Servidor local de Neo Orión: sirve el HUD y el instalador y les da datos reales.

Escucha solo en 127.0.0.1 (nada sale a la red). Todas las llamadas /api/* exigen la cabecera
`X-Orion: 1`; un sitio web cualquiera no puede ponerla sin permiso CORS, que aquí no se da.

  GET  /                    → /instalador/ en el pendrive, /hud/ ya instalado
  GET  /hud/… /instalador/… → archivos de la interfaz
  GET  /api/estado          → modo, versión, si hay IA local
  GET  /api/equipo          → hardware real (RAM, CPU, GPU, AVX2) + discos
  POST /api/instalar        → (solo pendrive, root) lanza orion_core.instalador
  GET  /api/progreso?desde=n→ eventos de la instalación
  POST /api/reiniciar       → reinicia (solo pendrive o al terminar)
  POST /api/chat            → conversa con la IA local (Ollama) con la identidad y la memoria
  GET  /api/eventos?desde=n → frases y palmadas que activaron rutinas
  GET  /api/rendimiento     → último nivel del vigía (completo / liviano / patata)
  POST /api/accion/<módulo> → activacion | llave | controladores | electrica | respaldo | remoto | cnc | casa | dictado | actualizar | traductor

Uso: python3 -m orion_core.servidor [--puerto 7700] [--modo auto|instalador|hud] [--raiz /usr/share/orion]
"""
from __future__ import annotations

import json
import mimetypes
import os
import re
import subprocess
import sys
import tempfile
import threading
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse

VERSION = "0.9"
OLLAMA = os.environ.get("OLLAMA_HOST", "http://127.0.0.1:11434")
PROG = Path(os.environ.get("ORION_PROGRESO", "/run/orion-instalacion.jsonl"))
ESTADO = {"proceso": None}


def modo_auto() -> str:
    try:
        return "instalador" if "boot=casper" in Path("/proc/cmdline").read_text() else "hud"
    except OSError:
        return "hud"


def raiz_por_defecto() -> Path:
    if Path("/usr/share/orion/hud/index.html").exists():
        return Path("/usr/share/orion")
    return Path(__file__).resolve().parents[2]          # repositorio: hud/ e instalador = hud/instalar.html


def equipo() -> dict:
    from orion_core import detect
    from orion_core.instalador import discos
    h = detect.host()
    gpu, vram = "", 0
    try:
        out = subprocess.run(["nvidia-smi", "--query-gpu=name,memory.total", "--format=csv,noheader,nounits"],
                             capture_output=True, text=True, timeout=5).stdout.strip().splitlines()
        if out:
            n, m = out[0].rsplit(",", 1)
            gpu, vram = n.strip(), round(int(m) / 1024)
    except (OSError, ValueError, subprocess.SubprocessError):
        pass
    if not gpu:
        try:
            lsp = subprocess.run(["lspci"], capture_output=True, text=True, timeout=5).stdout
            m = re.search(r"(VGA compatible controller|3D controller|Display controller): (.+)", lsp)
            gpu = re.sub(r"\(rev .*?\)", "", m.group(2)).strip() if m else ""
        except OSError:
            pass
    cpu = ""
    try:
        cpu = re.search(r"model name\s*:\s*(.+)", Path("/proc/cpuinfo").read_text()).group(1).strip()
    except (OSError, AttributeError):
        pass
    mic = cam = False
    try:
        mic = bool(re.search(r"^card \d", subprocess.run(["arecord", "-l"], capture_output=True, text=True, timeout=5).stdout, re.M))
    except OSError:
        pass
    cam = any(Path("/dev").glob("video*"))
    red = "sin conexión"
    try:
        for d in Path("/sys/class/net").iterdir():
            if d.name != "lo" and (d / "operstate").read_text().strip() == "up":
                red = "wifi" if (d / "wireless").exists() else "ethernet"
                break
    except OSError:
        pass
    try:
        from orion_core import controladores as ctl
        drv = ctl.detectar()
        if drv["gpus"] and not h.get("gpu_nvidia"):
            gpu = " + ".join(f"{x['fabricante']} {x['modelo']}".strip() for x in drv["gpus"])[:80]
    except Exception:
        drv = {}
    return {"ram_gb": h.get("ram_gb", 0), "hilos": os.cpu_count() or 1, "cpu": cpu, "gpu": gpu or "integrada / no informada",
            "controladores": drv,
            "vram_gb": vram, "avx2": h.get("avx2", True), "tipo": h.get("tipo", "pc"), "uefi": os.path.isdir("/sys/firmware/efi"),
            "microfono": mic, "camara": cam, "red": red, "discos": discos()}


def _ollama(ruta: str, datos: dict | None = None, timeout: float = 120) -> dict:
    req = urllib.request.Request(OLLAMA + ruta, data=json.dumps(datos).encode() if datos is not None else None,
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.loads(r.read())


def modelos_locales() -> list[str]:
    try:
        return [m["name"] for m in _ollama("/api/tags", timeout=2).get("models", [])]
    except Exception:
        return []


def _instalacion() -> dict:
    try:
        return json.loads(Path("/etc/orion/instalacion.json").read_text())
    except (OSError, ValueError):
        return {}


def charlar(texto: str, historial: list | None = None) -> dict:
    disp = modelos_locales()
    if not disp:
        return {"sin_ia": True, "texto": "Aún no tengo una IA local descargada. Conéctame a internet y la bajo sola, "
                                          "o di «abre el instalador de IA»."}
    pref = (_instalacion().get("ia") or {}).get("locales", [])
    modelo = next((m for m in pref if m in disp or m + ":latest" in disp), disp[0])
    ident = _instalacion().get("identidad") or {}
    nombre = ident.get("nombre", "Neo Orión")
    sistema = (f"Eres {nombre}, el asistente de voz de Neo Orión OS, un Linux que se maneja hablando. "
               "Responde en español de Chile, corto y claro (2–4 frases), porque se lee en un HUD y se dice en voz alta. "
               "Si te piden una acción física o irreversible, explica que necesita la confirmación de la persona.")
    try:
        from orion_memoria import Memoria   # contexto de la memoria, si existe
        mem = Memoria(str(Path.home() / "Orion"), embeddings=None)
        ctx = mem.contexto(texto, max_chars=1500)
        mem.diario(texto, quien="chat")
        if ctx:
            sistema += "\n\nNotas de la memoria que pueden servir:\n" + ctx
    except Exception:
        pass
    msgs = [{"role": "system", "content": sistema}] + (historial or [])[-8:] + [{"role": "user", "content": texto}]
    try:
        r = _ollama("/api/chat", {"model": modelo, "messages": msgs, "stream": False, "think": False, "options": {"num_ctx": 4096}})
        txt = re.sub(r"<think>.*?</think>", "", r.get("message", {}).get("content", ""), flags=re.S).strip()
        return {"texto": txt, "modelo": modelo}
    except Exception as e:
        return {"error": True, "texto": f"La IA local no respondió ({e.__class__.__name__})."}


def lanzar_instalacion(cfg: dict) -> dict:
    if ESTADO["proceso"] and ESTADO["proceso"].poll() is None:
        return {"ok": False, "error": "ya hay una instalación en curso"}
    if os.geteuid() != 0:
        return {"ok": False, "error": "el servidor del instalador no corre como administrador"}
    clave = cfg.pop("clave", "")
    from orion_core.instalador import revisar
    err = revisar(cfg)
    if err:
        return {"ok": False, "error": "; ".join(err)}
    fd, ruta = tempfile.mkstemp(prefix="orion-inst-", suffix=".json")
    with os.fdopen(fd, "w") as f:
        json.dump(cfg, f)
    PROG.write_text("")
    env = {**os.environ, "ORION_CLAVE": clave, "PYTHONPATH": str(Path(__file__).resolve().parents[1])}
    ESTADO["proceso"] = subprocess.Popen([sys.executable, "-m", "orion_core.instalador", ruta, "--progreso", str(PROG)],
                                         env=env, stdout=subprocess.DEVNULL, stderr=open("/run/orion-instalacion.err", "w"))
    return {"ok": True}


def grabar_frase(n: int, instalador: bool = True) -> dict:
    """Graba 3 s del micrófono para la llave de voz (en el pendrive: /run, el instalador la copia a la cuenta nueva;
    ya instalado: ~/.orion/voz/nueva-n.wav para cambiar la frase o verificar)."""
    if not 0 <= n <= 3:
        return {"ok": False, "error": "número de frase inválido"}
    if instalador:
        destino = Path(f"/run/orion-voz-{n}.wav")
    else:
        destino = Path.home() / ".orion/voz" / (f"nueva-{n}.wav" if n else "verificar.wav")
        destino.parent.mkdir(parents=True, exist_ok=True)
    try:
        r = subprocess.run(["arecord", "-q", "-d", "3", "-f", "S16_LE", "-r", "16000", "-c", "1", str(destino)],
                           capture_output=True, text=True, timeout=10)
    except (OSError, subprocess.SubprocessError) as e:
        return {"ok": False, "error": str(e)}
    if r.returncode != 0:
        return {"ok": False, "error": (r.stderr or "arecord falló").strip()[:160]}
    return {"ok": True, "bytes": destino.stat().st_size}


def accion(modulo: str, datos: dict) -> dict:
    """Acciones de las herramientas de Neo Orión que el HUD puede pedir (lista cerrada; lo físico va por ticket)."""
    a = str(datos.get("accion", ""))
    try:
        if modulo == "electrica":
            import orion_electrica as el
            from dataclasses import asdict
            r = el.desde_frase(str(datos.get("texto", "")))
            return {"ok": True, **asdict(r), "resumen": r.texto()} if r else {"ok": False, "error": "di la corriente o la potencia y el largo"}
        if modulo == "respaldo":
            import orion_respaldo as rs
            return {"ok": True, "respaldos": rs.listar(), "discos": rs.discos_externos(), "config": rs.configuracion()} if a == "ver" else rs.respaldar()
        if modulo == "remoto":
            import orion_remoto as rm
            if a == "casa":
                r = rm.encender_vnc()
                r["clave"] = next((l.split("=", 1)[1] for l in (rm.CONF / "config").read_text().splitlines() if l.startswith("password=")), "") if r.get("ok") else ""
                return r
            return rm.apagar() if a == "apagar" else rm.rustdesk_id() if a == "afuera" else rm.estado()
        if modulo == "cnc":
            import orion_cnc as cn
            if a == "revisar":
                rev = cn.revisar(str(datos.get("gcode", ""))[:5_000_000])
                return {"ok": True, **rev.resumen(), "trazo": [(round(p[0], 2), round(p[1], 2), p[3]) for p in rev.puntos[:4000]]}
            if a in ("parar", "pausa", "seguir", "estado"):
                g = cn.Grbl(cn.Maquina(puerto=str(datos.get("puerto", "/dev/ttyUSB0"))))
                return {"ok": True, **g.estado()} if a == "estado" else (getattr(g, a)() or {"ok": True})
            return {"ok": False, "error": "mover y cortar se aprueban desde la IA con ticket"}
        if modulo == "casa":
            import orion_casa as ca
            return ca.Casa().ordenar(str(datos.get("texto", "")), confirmado=bool(datos.get("confirmado")))
        if modulo == "dictado":
            import orion_dictado as dc
            b = dc.ordenar(dc.pulir_con_ia(str(datos.get("texto", ""))) if datos.get("pulir") else str(datos.get("texto", "")))
            titulo = next((x for k, x in b if k == "titulo"), "documento")
            ruta = Path.home() / "Documentos" / (re.sub(r"[^\w-]+", "_", titulo)[:60] + ".docx")
            dc.documento(b, str(ruta), str(datos.get("plantilla", "informe-duoc")), datos.get("datos") or {})
            return {"ok": True, "archivo": str(ruta), "bloques": b}
        if modulo == "activacion":
            from orion_voz import activacion as act
            c = act.leer()
            if a == "agregar":
                act.guardar(act.agregar(c, str(datos.get("tipo", "frase")), list(datos.get("rutina") or ["despertar", "saludo"]),
                                        str(datos.get("frase", "")), int(datos.get("veces", 2)), bool(datos.get("solo_dueno"))))
            elif a == "quitar":
                c["activadores"] = [x for x in c["activadores"] if x["id"] != datos.get("id")]
                act.guardar(c)
            elif a == "editar":
                act.guardar(act.editar(c, str(datos.get("id", "")), dict(datos.get("cambios") or {})))
            elif a == "ajustes":                    # saludo y sensibilidad de las palmadas
                if "saludo" in datos:
                    c["saludo"] = str(datos["saludo"])[:200]
                if datos.get("sensibilidad") in act.SENSIBILIDAD:
                    c["palmadas"] = {"sensibilidad": datos["sensibilidad"]}
                act.guardar(c)
            elif a == "reposo":
                c["reposo"] = {"tras_minutos": int(datos.get("tras_minutos", 20)), "nivel": str(datos.get("nivel", "atento"))}
                act.guardar(c)
            elif a == "wol":
                r = act.preparar_wol()
                if r.get("ok"):
                    c["wol"] = {"mac": r["mac"], "interfaz": r["interfaz"]}
                    act.guardar(c)
                return r
            elif a == "ejecutar":                   # probar una rutina desde el HUD
                x = next((v for v in c["activadores"] if v["id"] == datos.get("id")), None)
                return {"ok": bool(x), "hechos": act.ejecutar_rutina([r for r in (x or {}).get("rutina", []) if r in ("despertar", "reposo", "luces")])}
            return {"ok": True, **act.leer(), "acciones": act.ACCIONES}
        if modulo == "controladores":
            from orion_core import controladores as ctl
            d = ctl.detectar()
            if any(g["fabricante"] == "NVIDIA" for g in d["gpus"]):
                d["al_dia"] = ctl.revisar_al_dia()
            return {"ok": True, **d}
        if modulo == "llave":
            from orion_voz.llave import LlaveVoz, leer_wav
            k = LlaveVoz()
            voz = Path.home() / ".orion/voz"

            def clave_ok() -> bool:   # la clave de la cuenta (la revisa unix_chkpwd, como la pantalla de entrada)
                c = str(datos.get("clave", ""))
                if not c:
                    return False
                import getpass
                r = subprocess.run(["/usr/sbin/unix_chkpwd", getpass.getuser(), "nullok"], input=c + "\0",
                                   capture_output=True, text=True, timeout=10)
                return r.returncode == 0
            if a == "configurar":            # 1 o 3 grabaciones de la frase nueva
                veces = 1 if int(datos.get("veces", 3)) == 1 else 3
                wavs = [voz / f"nueva-{n}.wav" for n in range(1, veces + 1)]
                if not all(w.exists() for w in wavs):
                    return {"ok": False, "error": f"faltan grabaciones ({veces})"}
                return k.configurar(str(datos.get("frase", "")), [leer_wav(w) for w in wavs])
            if a == "modo":
                return k.modo(str(datos.get("modo", "")))
            if a == "desbloquear":           # según el modo: voz, clave, voz o clave, voz y clave
                modo = k.modo()["modo"]
                v = {"ok": False}
                if modo != "clave" and datos.get("con_voz"):
                    g = grabar_frase(0, instalador=False)
                    v = k.verificar(leer_wav(voz / "verificar.wav")) if g.get("ok") else g
                c = clave_ok() if modo != "voz" else False
                return {"ok": k.abrir(bool(v.get("ok")), c), "modo": modo, "voz": v, "clave_ok": c}
            if a == "olvidar":
                return k.olvidar()
            return {"ok": True, **k.estado()}
        if modulo == "actualizar":
            import orion_actualizar as ac
            return ac.revisar()
        if modulo == "traductor":
            import orion_traductor as tr
            c = tr.Conversacion(str(datos.get("a", "es")), str(datos.get("b", "en")), hablar=lambda *x: None)
            return c.turno(str(datos.get("texto", "")))
    except (PermissionError, ValueError) as e:
        return {"ok": False, "error": str(e)}
    except Exception as e:                       # la interfaz muestra el error en una línea
        return {"ok": False, "error": f"{e.__class__.__name__}: {e}"[:240]}
    return {"ok": False, "error": "módulo desconocido"}


def progreso(desde: int = 0) -> dict:
    try:
        lineas = PROG.read_text().splitlines()
    except OSError:
        lineas = []
    ev = [json.loads(l) for l in lineas[desde:] if l.strip()]
    p = ESTADO["proceso"]
    vivo = bool(p and p.poll() is None)
    return {"eventos": ev, "total": len(lineas), "corriendo": vivo,
            "codigo": None if (p is None or vivo) else p.returncode}


class Manejador(BaseHTTPRequestHandler):
    server_version = "NeoOrion/" + VERSION
    raiz: Path = Path("/usr/share/orion")
    modo: str = "hud"

    def log_message(self, fmt, *a):   # silencioso
        pass

    def _json(self, obj, codigo=200):
        b = json.dumps(obj, ensure_ascii=False).encode()
        self.send_response(codigo)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(b)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(b)

    def _api_ok(self) -> bool:
        if self.headers.get("X-Orion") != "1":
            self._json({"error": "falta la cabecera X-Orion"}, 403)
            return False
        return True

    def _archivo(self, ruta: str):
        if ruta in ("", "/"):
            self.send_response(302)
            self.send_header("Location", "/instalador/" if self.modo == "instalador" else "/hud/")
            self.end_headers()
            return
        partes = ruta.strip("/").split("/", 1)
        base, resto = partes[0], (partes[1] if len(partes) > 1 else "")
        if base == "hud":
            carpeta = self.raiz / "hud"
            resto = resto or "index.html"
        elif base == "instalador":
            carpeta = self.raiz / "instalador" if (self.raiz / "instalador").exists() else self.raiz / "hud"
            resto = resto or ("instalar.html" if (carpeta / "instalar.html").exists() else "index.html")
        else:
            return self._json({"error": "no existe"}, 404)
        f = (carpeta / resto).resolve()
        if not str(f).startswith(str(carpeta.resolve())) or not f.is_file():
            return self._json({"error": "no existe"}, 404)
        b = f.read_bytes()
        self.send_response(200)
        self.send_header("Content-Type", (mimetypes.guess_type(f.name)[0] or "application/octet-stream") +
                         ("; charset=utf-8" if f.suffix in (".html", ".js", ".css", ".json") else ""))
        self.send_header("Content-Length", str(len(b)))
        self.end_headers()
        self.wfile.write(b)

    def do_GET(self):
        u = urlparse(self.path)
        if not u.path.startswith("/api/"):
            return self._archivo(u.path)
        if not self._api_ok():
            return
        q = parse_qs(u.query)
        if u.path == "/api/estado":
            return self._json({"modo": self.modo, "version": VERSION, "ia_local": modelos_locales(),
                               "acento": _acento(), "instalacion": {k: v for k, v in _instalacion().items() if k in ("identidad", "idioma")}})
        if u.path == "/api/equipo":
            return self._json(equipo())
        if u.path == "/api/progreso":
            return self._json(progreso(int(q.get("desde", ["0"])[0] or 0)))
        if u.path == "/api/eventos":             # activaciones (frases, palmadas) que el HUD debe mostrar
            try:
                lineas = (Path.home() / ".orion/activaciones.jsonl").read_text().splitlines()
            except OSError:
                lineas = []
            d = int(q.get("desde", ["-1"])[0])
            return self._json({"total": len(lineas), "eventos": [json.loads(l) for l in lineas[d:]] if d >= 0 else []})
        if u.path == "/api/rendimiento":
            try:
                return self._json(json.loads((Path.home() / ".orion/rendimiento.json").read_text()))
            except (OSError, ValueError):
                return self._json({"nivel": "completo"})
        self._json({"error": "no existe"}, 404)

    def do_POST(self):
        u = urlparse(self.path)
        if not self._api_ok():
            return
        n = int(self.headers.get("Content-Length") or 0)
        if n > 1_000_000:
            return self._json({"error": "muy grande"}, 413)
        try:
            datos = json.loads(self.rfile.read(n) or b"{}")
        except ValueError:
            return self._json({"error": "JSON inválido"}, 400)
        if u.path.startswith("/api/accion/"):
            return self._json(accion(u.path.rsplit("/", 1)[1], datos))
        if u.path == "/api/chat":
            return self._json(charlar(str(datos.get("texto", ""))[:4000], datos.get("historial")))
        if u.path == "/api/instalar":
            if self.modo != "instalador":
                return self._json({"ok": False, "error": "solo desde el pendrive"}, 403)
            return self._json(lanzar_instalacion(datos))
        if u.path == "/api/voz/grabar":
            return self._json(grabar_frase(int(datos.get("n", 1)), self.modo == "instalador"))
        if u.path == "/api/reiniciar":
            p = ESTADO["proceso"]
            if self.modo != "instalador" or (p and p.poll() is None):
                return self._json({"ok": False}, 403)
            self._json({"ok": True})
            threading.Timer(1.0, lambda: subprocess.run(["systemctl", "reboot"])).start()
            return
        self._json({"error": "no existe"}, 404)


def _acento() -> str:
    try:
        return (Path.home() / ".orion/acento").read_text().strip()
    except OSError:
        return ""


def servir(puerto: int = 7700, modo: str = "auto", raiz: Path | None = None) -> ThreadingHTTPServer:
    Manejador.raiz = raiz or raiz_por_defecto()
    Manejador.modo = modo_auto() if modo == "auto" else modo
    return ThreadingHTTPServer(("127.0.0.1", puerto), Manejador)


if __name__ == "__main__":
    import argparse
    ap = argparse.ArgumentParser()
    ap.add_argument("--puerto", type=int, default=7700)
    ap.add_argument("--modo", default="auto")
    ap.add_argument("--raiz")
    a = ap.parse_args()
    s = servir(a.puerto, a.modo, Path(a.raiz) if a.raiz else None)
    print(f"Neo Orión · servidor local en http://127.0.0.1:{a.puerto}/ ({Manejador.modo})", flush=True)
    s.serve_forever()
