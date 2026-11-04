"""Teléfono como micrófono (WebSocket → micrófono virtual) e íconos con regla estética."""
import base64
import io
import json
import os
import socket
import struct
import sys
import tempfile
import time
from pathlib import Path
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
os.environ["HOME"] = tempfile.mkdtemp()
import orion_iconos as ic  # noqa: E402
import orion_telefono as tel  # noqa: E402


def trama_cliente(datos: bytes, op: int) -> bytes:
    m = b"\x11\x22\x33\x44"
    n = len(datos)
    cab = bytes([0x80 | op]) + (bytes([0x80 | n]) if n < 126 else bytes([0x80 | 126]) + struct.pack(">H", n))
    return cab + m + bytes(b ^ m[i % 4] for i, b in enumerate(datos))


def test_trama_ida_y_vuelta():
    for n in (5, 300, 70000):
        d = os.urandom(n)
        op, r = tel.leer_trama(io.BytesIO(trama_cliente(d, 2) if n < 65536 else
                                          bytes([0x82, 0xFF]) + struct.pack(">Q", n) + b"\0\0\0\0" + d))
        assert op == 2 and r == d
    assert tel.aceptar_ws("dGhlIHNhbXBsZSBub25jZQ==") == "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="   # ejemplo del RFC 6455


class Pactl:
    def __init__(self):
        self.hechos, self.def_src = [], "alsa_input.pci"

    def __call__(self, cmd, **k):
        self.hechos.append(cmd)
        out = ""
        if cmd[:3] == ["pactl", "list", "short"]:
            out = ""
        elif cmd[:2] == ["pactl", "load-module"]:
            out = "536870913\n"
        elif cmd[:2] == ["pactl", "get-default-source"]:
            out = self.def_src + "\n"
        elif cmd[:2] == ["pactl", "set-default-source"]:
            self.def_src = cmd[2]
        return SimpleNamespace(returncode=0, stdout=out, stderr="")


def test_puente_de_punta_a_punta():
    pa, recibido = Pactl(), {}

    class Rep:
        def __init__(self, tasa):
            recibido["tasa"] = tasa; self.stdin = io.BytesIO(); self.stdin.close = lambda: None
            recibido["rep"] = self

        def terminate(self):
            recibido["cerrado"] = True

    p = tel.Puente(puerto=0, ejecutar=pa, reproductor=Rep)
    tel.ip_local = lambda: "127.0.0.1"
    info = p.iniciar(tls=False)
    puerto = p.srv.server_address[1]
    assert info["ok"] and info["url"].startswith("https://127.0.0.1:") and "?t=" in info["url"]
    assert any(c[:2] == ["pactl", "load-module"] and "media.class=Audio/Source/Virtual" in c for c in pa.hechos)

    # sin la clave del QR: prohibido
    s = socket.create_connection(("127.0.0.1", puerto)); s.sendall(b"GET /mic?t=mala HTTP/1.1\r\nHost: x\r\n\r\n")
    assert b"403" in s.recv(200); s.close()
    # con la clave: la página
    s = socket.create_connection(("127.0.0.1", puerto)); s.sendall(f"GET /mic?t={p.clave} HTTP/1.1\r\nHost: x\r\n\r\n".encode())
    pagina = b""
    while b"</html>" not in pagina:
        pagina += s.recv(65536)
    assert b"USAR COMO MICR" in pagina; s.close()

    # WebSocket: configuración + audio
    s = socket.create_connection(("127.0.0.1", puerto))
    s.sendall((f"GET /ws?t={p.clave} HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
               f"Sec-WebSocket-Key: {base64.b64encode(b'0123456789abcdef').decode()}\r\nSec-WebSocket-Version: 13\r\n\r\n").encode())
    assert b"101" in s.recv(300)
    s.sendall(trama_cliente(json.dumps({"tasa": 44100}).encode(), 1))
    audio = struct.pack("<4h", 0, 16384, -32768, 100)
    s.sendall(trama_cliente(audio, 2))
    time.sleep(.3)
    assert p.estado["conectado"] and pa.def_src == tel.NOMBRE            # el teléfono pasa a ser el micrófono
    assert recibido["tasa"] == 44100 and recibido["rep"].stdin.getvalue() == audio and p.estado["nivel"] == 1.0
    s.sendall(trama_cliente(b"", 8)); s.close(); time.sleep(.3)
    assert not p.estado["conectado"] and pa.def_src == "alsa_input.pci"  # al soltar, vuelve el anterior
    assert recibido.get("cerrado")
    p.parar()
    assert any(c[:2] == ["pactl", "unload-module"] for c in pa.hechos)


def test_regla_de_iconos():
    for k, v in {**ic.CATEGORIAS, **ic.PROPIOS}.items():
        assert ic.validar(v) == [], k
    assert set(ic.PROPIOS) == {a["id"] for a in ic.CONOCIDAS}                 # cada programa conocido con el suyo
    todos = list(ic.CATEGORIAS.values()) + list(ic.PROPIOS.values())
    assert len(set(todos)) == len(todos)                                     # regla 8: ninguno se repite
    assert all(ic.validar(ic.monograma(a + b)) == [] for a in ic.LETRAS for b in ic.LETRAS)
    assert ic.validar("###|###") and ic.validar("#" * 9 + ("|" + "#" * 9) * 8)        # tamaño / demasiado lleno
    suelto = "|".join(["#########", "#.......#", "#.#.....#"] + ["#.......#"] * 5 + ["#########"])
    assert any(e.startswith("4") for e in ic.validar(suelto))
    assert ic.norm_cat("Películas") == "video" and ic.norm_cat("algo raro") == "app"
    assert ic.sigla("GeForce Now") == "GN" and ic.sigla("steam") == "ST"


def test_registrar_listar_abrir():
    r = ic.registrar("jellyfin-casa", "Series de la casa", "series", web="http://192.168.1.5:8096")
    assert r["ok"] and r["icono"] == ic.CATEGORIAS["video"] and r["sigla"] == "SD"
    malo = ic.registrar("raro", "Raro", "app", comando="raro", icono="#########|" * 8 + "#########")
    assert not malo["ok"] and malo["reglas"]
    propio = "|".join([".........", "..#####..", ".#.....#.", ".#.###.#.", ".#.#.#.#.", ".#.###.#.", ".#.....#.", "..#####..", "........."])
    assert ic.registrar("ojo", "Ojo", "foto", comando="ojo", icono=propio)["ok"]
    assert not ic.registrar("x y", "X", "app", comando="x")["ok"]
    otra = ic.registrar("plex", "Mi Plex", "series", comando="plex")             # misma categoría → monograma, no se repite
    assert otra["ok"] and otra["icono"] != r["icono"] and otra["icono"] == ic.monograma("MP")
    gemela = ic.registrar("plex2", "Mi Plex 2", "series", comando="plex2")         # misma sigla → igual distinto
    assert gemela["ok"] and gemela["icono"] not in (otra["icono"], r["icono"])
    assert not ic.registrar("copia", "Copia", "foto", comando="c", icono=ic.PROPIOS["steam"])["ok"]
    lista = {a["id"]: a for a in ic.listar()}
    assert "jellyfin-casa" in lista and lista["ojo"]["icono"] == propio
    hechos = []
    assert ic.abrir("jellyfin-casa", ejecutar=lambda c, **k: hechos.append(c))["ok"] and hechos[-1] == ["xdg-open", "http://192.168.1.5:8096"]
    assert not ic.abrir("no-existe")["ok"]
    assert ic.quitar("ojo")["ok"] and "ojo" not in {a["id"] for a in ic.listar()}
