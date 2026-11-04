"""Instalador real (modo simular) y servidor local."""
import json
import sys
import threading
import urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from orion_core import instalador as ins  # noqa: E402
from orion_core import servidor as srv  # noqa: E402

BASE = {"idioma": "es-CL", "teclado": "latam", "zona": "America/Santiago", "disco": {"dispositivo": "/dev/sda", "modo": "borrar", "confirmado": True},
        "ia": {"locales": ["gemma3:1b"], "nube": []}, "identidad": {"nombre": "Neo Orión", "color": "#ff2438", "voz": "femenina", "tono": "cercano"},
        "modulos": [], "capa": "base", "cuenta": {"usuario": "davis", "equipo": "taller"}}


def correr(cfg, clave="secreta1"):
    x = ins.Ejecutor(simular=True)
    eventos = []
    p = ins.Progreso(None)
    p.__class__ = type("P", (ins.Progreso,), {"__call__": lambda s, pct=None, etapa="", detalle="", log="", **e: eventos.append((pct, etapa, detalle, e))})
    r = ins.instalar(json.loads(json.dumps(cfg)), clave, x, p, Path(__file__).resolve().parents[2])
    return r, x.hechos, eventos


def test_particion_nombres():
    assert ins.particion("/dev/sda", 3) == "/dev/sda3"
    assert ins.particion("/dev/nvme0n1", 1) == "/dev/nvme0n1p1"
    assert ins.particion("/dev/mmcblk0", 2) == "/dev/mmcblk0p2"


def test_revisar_rechaza():
    malo = json.loads(json.dumps(BASE))
    malo["disco"] = {"dispositivo": "/dev/sda; rm -rf /", "modo": "borrar", "confirmado": True}
    assert any("disco" in e for e in ins.revisar(malo))
    sin = json.loads(json.dumps(BASE)); sin["disco"]["confirmado"] = False
    assert any("confirmar" in e for e in ins.revisar(sin))
    m = json.loads(json.dumps(BASE)); m["disco"]["modo"] = "manual"
    assert any("manual" in e for e in ins.revisar(m))
    u = json.loads(json.dumps(BASE)); u["cuenta"]["usuario"] = "Root Kit"
    assert ins.revisar(u)
    assert ins.revisar(BASE) == []


def test_borrar_completo_en_orden():
    r, cmds, ev = correr(BASE)
    assert r["ok"] and r["raiz"] == "/dev/sda3" and r["efi"] == "/dev/sda1"
    planos = [" ".join(map(str, c)) for c in cmds]
    def idx(pref):
        return next(i for i, c in enumerate(planos) if c.startswith(pref) or pref in c)
    assert idx("sgdisk -Z /dev/sda") < idx("mkfs.ext4") < idx("unsquashfs") < idx("useradd") < idx("grub-install") < idx("update-grub")
    assert any("--target=i386-pc /dev/sda" in c for c in planos)            # BIOS también
    assert any("--uefi-secure-boot" in c and "--removable" in c for c in planos)
    assert any(c.startswith("chroot /target apt-get purge") and "casper" in c for c in planos)
    assert not any("secreta1" in c for c in planos)                          # la clave no aparece en comandos
    assert ev[-1][0] == 100 and ev[-1][3].get("terminado")
    pcts = [e[0] for e in ev if e[0] is not None]
    assert pcts == sorted(pcts)


def test_nvme_y_junto():
    cfg = json.loads(json.dumps(BASE))
    cfg["disco"] = {"dispositivo": "/dev/nvme0n1", "modo": "junto"}
    x = ins.Ejecutor(simular=True, salidas={
        "sgdisk -p": "Number  Start (sector)    End (sector)  Size       Code  Name\n   1  2048  1050623  512.0 MiB  EF00  EFI\n   2  1050624  400000000  190 GiB  0700  Windows\n",
        "sgdisk -F": "400002048\n", "sgdisk -E": "1000215182\n"})
    p = ins.Progreso.__new__(ins.Progreso); p.f = None; p.pct = 0
    p.__class__ = type("P", (ins.Progreso,), {"__call__": lambda s, *a, **k: None})
    r = ins.instalar(cfg, "secreta1", x, p)
    planos = [" ".join(map(str, c)) for c in x.hechos]
    assert r["efi"] == "/dev/nvme0n1p1"                                       # reusa la EFI de Windows
    assert not any(c.startswith("mkfs.vfat") for c in planos)                # y no la formatea
    assert not any("sgdisk -Z" in c or c.startswith("wipefs") for c in planos)
    assert not any("i386-pc" in c for c in planos)


def test_junto_sin_espacio():
    cfg = json.loads(json.dumps(BASE)); cfg["disco"] = {"dispositivo": "/dev/sda", "modo": "junto"}
    x = ins.Ejecutor(simular=True, salidas={"sgdisk -F": "1000\n", "sgdisk -E": "2000\n"})
    p = ins.Progreso.__new__(ins.Progreso); p.f = None; p.pct = 0
    p.__class__ = type("P", (ins.Progreso,), {"__call__": lambda s, *a, **k: None})
    try:
        ins.instalar(cfg, "secreta1", x, p)
        assert False
    except ins.ErrorInstalacion as e:
        assert "20 GB" in str(e)


def test_clave_corta():
    try:
        correr(BASE, clave="123")
        assert False
    except ins.ErrorInstalacion as e:
        assert "clave" in str(e)


def test_servidor_api_y_seguridad(tmp_path):
    (tmp_path / "hud").mkdir()
    (tmp_path / "hud/index.html").write_text("<h1>hud</h1>")
    (tmp_path / "instalador").mkdir()
    (tmp_path / "instalador/instalar.html").write_text("<h1>inst</h1>")
    s = srv.servir(0, "hud", tmp_path)
    port = s.server_address[1]
    threading.Thread(target=s.serve_forever, daemon=True).start()
    base = f"http://127.0.0.1:{port}"
    try:
        assert b"hud" in urllib.request.urlopen(base + "/hud/").read()
        # sin cabecera X-Orion: rechazado (protege de páginas web que llamen al servidor)
        try:
            urllib.request.urlopen(base + "/api/estado"); assert False
        except urllib.error.HTTPError as e:
            assert e.code == 403
        req = lambda ruta, datos=None: json.loads(urllib.request.urlopen(urllib.request.Request(
            base + ruta, data=json.dumps(datos).encode() if datos is not None else None, headers={"X-Orion": "1"})).read())
        assert req("/api/estado")["modo"] == "hud"
        # instalar solo desde el pendrive
        try:
            req("/api/instalar", BASE); assert False
        except urllib.error.HTTPError as e:
            assert e.code == 403
        # no sale de la carpeta
        try:
            urllib.request.urlopen(base + "/hud/../../etc/passwd"); assert False
        except urllib.error.HTTPError as e:
            assert e.code == 404
        eq = req("/api/equipo")
        assert eq["ram_gb"] > 0 and "discos" in eq
        r = req("/api/chat", {"texto": "hola"})
        assert "texto" in r
    finally:
        s.shutdown()
