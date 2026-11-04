"""Las 9 ideas: respaldo, remoto, CNC, cámaras, casa, dictado, calculadora RIC, actualizaciones, traductor."""
import json
import math
import subprocess
import time
import sys
import urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import orion_actualizar as act  # noqa: E402
import orion_casa as casa  # noqa: E402
import orion_cnc as cnc  # noqa: E402
import orion_dictado as dic  # noqa: E402
import orion_electrica as el  # noqa: E402
import orion_remoto as rem  # noqa: E402
import orion_respaldo as res  # noqa: E402
import orion_traductor as tr  # noqa: E402
import orion_vision as vis  # noqa: E402


# ---------------------------------------------------------------- 7 · calculadora eléctrica (RIC N°03)
def test_caida_monofasica_y_trifasica():
    # 2·L·I·ρ/S = 2·30·16·0,018/2,5 = 6,912 V
    assert abs(el.caida(16, 30, 2.5, 1) - 6.912) < 1e-9
    assert abs(el.caida(16, 30, 2.5, 3) - math.sqrt(3) * 30 * 16 * 0.018 / 2.5) < 1e-9


def test_seccion_cumple_3_por_ciento_y_minimos():
    r = el.seccion(20, 35, 220, 1, tipo="subalimentador")
    assert r.cumple and r.caida_pct <= 3.0 and r.seccion_mm2 == 4       # 2,5 mm² daría 4,58 %; 4 mm² da 2,86 %
    a = el.seccion(5, 5, 220, 1, tipo="alimentador")
    assert a.seccion_mm2 == 4 and any("mínimo" in x for x in a.avisos)  # RIC N°03: 4 mm² en alimentadores
    assert any("Iz" in x for x in r.avisos)                              # no inventa la capacidad del conductor
    p = el.seccion(20, 35, 220, 1, iz_a=25)
    assert p.proteccion_a == 20 and not any("supera" in x for x in p.avisos)
    total = el.seccion(10, 40, 220, 1, caida_previa_pct=2.5)
    assert total.caida_pct + 2.5 <= 5.0                                  # 5 % total


def test_frase_y_demanda():
    r = el.desde_frase("qué cable necesito para 4500 W a 40 metros en 220")
    assert r and r.corriente_a == round(4500 / 220, 2) and r.cumple
    tri = el.desde_frase("20 amperes trifásico a 60 metros")
    assert tri.fases == 3 and tri.tension_v == 380
    assert el.demanda_vivienda(5) == 3 + 2 * 0.35


# ---------------------------------------------------------------- 1 · respaldo
def test_respaldo_con_fotos_enlazadas(tmp_path, monkeypatch):
    casa_ = tmp_path / "casa"
    (casa_ / "Orion/diario").mkdir(parents=True)
    (casa_ / "Orion/diario/hoy.md").write_text("nota")
    destino = tmp_path / "usb"
    c = {"destino": str(destino), "que": [str(casa_ / "Orion")]}
    import datetime as dt
    r1 = res.respaldar(c, dt.datetime(2026, 11, 3, 21, 30))
    r2 = res.respaldar(c, dt.datetime(2026, 11, 4, 21, 30))
    assert r1["ok"] and r2["ok"]
    a, b = destino / "2026-11-03_2130/Orion/diario/hoy.md", destino / "2026-11-04_2130/Orion/diario/hoy.md"
    assert a.read_text() == "nota" and a.stat().st_ino == b.stat().st_ino     # sin cambios: mismo archivo enlazado
    assert res.listar(c) == ["2026-11-04_2130", "2026-11-03_2130"]
    rr = res.restaurar("2026-11-03_2130", "Orion", str(tmp_path / "vuelta"), c)
    assert rr["ok"] and (tmp_path / "vuelta/diario/hoy.md").exists()


def test_respaldo_conserva_30(tmp_path, monkeypatch):
    monkeypatch.setattr(res, "CONSERVAR", 3)
    (tmp_path / "o").mkdir()
    (tmp_path / "o/a").write_text("x")
    c = {"destino": str(tmp_path / "d"), "que": [str(tmp_path / "o")]}
    import datetime as dt
    for d in range(1, 6):
        res.respaldar(c, dt.datetime(2026, 11, d, 10, 0))
    assert len(res.listar(c)) == 3


def test_respaldo_nube_pide_rclone(monkeypatch, tmp_path):
    (tmp_path / "o").mkdir()
    monkeypatch.setattr(res.shutil, "which", lambda x: None)
    r = res.respaldar({"destino": "rclone:drive:NeoOrion", "que": [str(tmp_path / "o")]})
    assert not r["ok"] and "rclone" in r["error"]


# ---------------------------------------------------------------- 2 · remoto
def test_vnc_con_clave_y_tls(tmp_path):
    hechos = []
    r = rem.preparar_vnc("davis", "Secreta123", tmp_path, ejecutar=lambda c, **k: hechos.append(c))
    cfg = (tmp_path / "config").read_text()
    assert "enable_auth=true" in cfg and "password=Secreta123" in cfg and "certificate_file" in cfg
    assert oct((tmp_path / "config").stat().st_mode)[-3:] == "600"
    assert hechos and hechos[0][0] == "openssl"
    assert len(rem.clave_nueva()) == 10


# ---------------------------------------------------------------- 3 · CNC
GCODE = """G21 G90
(soporte)
M3 S10000
G0 X0 Y0 Z5
G1 Z-2 F200
G1 X50 F800
G1 Y30
G2 X40 Y40 I-10 J0
G1 X0
G1 Y0
G0 Z5
M5
"""


def test_revisar_gcode():
    r = cnc.revisar(GCODE).resumen()
    assert r["listo_para_cortar"] and r["tamano_mm"][:2] == [50.0, 40.0]
    assert r["profundidad_max_mm"] == -2.0 and r["rpm"] == [10000] and 200 in r["avances_mm_min"]
    assert r["minutos"] > 0 and r["corte_m"] > 0.15
    grande = cnc.revisar("G21\nG1 X400 F500\n", cnc.Maquina(x_mm=300)).resumen()
    assert not grande["listo_para_cortar"] and "300" in grande["errores"][0]
    pulgadas = cnc.revisar("G20\nG1 X1 F10\n").resumen()
    assert pulgadas["hasta_mm"][0] == 25.4
    sin_f = cnc.revisar("G1 X10\n").resumen()
    assert not sin_f["listo_para_cortar"]


class SerieFalsa:
    def __init__(self):
        self.escrito, self.salida = [], []

    def write(self, b):
        self.escrito.append(b)
        if b == b"?":
            self.salida.append(b"<Idle|MPos:1.000,2.000,-3.000|FS:0,0>\n")
        elif b.endswith(b"\n"):
            self.salida.append(b"error:2\n" if b.startswith(b"ROMPE") else b"ok\n")

    def readline(self):
        return self.salida.pop(0) if self.salida else b""


def test_grbl_estado_trabajo_y_parada():
    s = SerieFalsa()
    g = cnc.Grbl(serie=s)
    assert g.estado() == {"estado": "Idle", "pos_mm": [1.0, 2.0, -3.0]}
    pct = []
    assert g.trabajo(GCODE, avance=lambda p, n: pct.append(p))["ok"] and pct[-1] == 100.0
    assert b"(soporte)" not in b"".join(s.escrito)            # los comentarios no se mandan
    assert g.trabajo("G1 X1 F100\nROMPE\n")["linea"] == 2
    g.parar()
    assert s.escrito[-1] == b"\x18" and g.detenido
    g.mover(x=10)
    assert s.escrito[-1].startswith(b"$J=G91")


def test_vista_cnc(tmp_path):
    png = cnc.vista(cnc.revisar(GCODE), str(tmp_path / "r.png"))
    assert Path(png).stat().st_size > 5000


# ---------------------------------------------------------------- 4 · cámaras (Frigate)
def test_frigate_sin_claves_en_texto(tmp_path):
    cams = [{"nombre": "Taller CNC", "fuente": "usb"}, {"nombre": "Patio", "zonas": {"robot": [[0, 0], [10, 0], [10, 10]]}}]
    y = vis.configuracion(cams)
    import yaml
    d = yaml.safe_load(y)
    assert d["cameras"]["patio"]["ffmpeg"]["inputs"][0]["path"] == "{FRIGATE_RTSP_PATIO}"
    assert d["cameras"]["taller_cnc"]["ffmpeg"]["inputs"][0]["path"] == "/dev/video0"
    env = vis.variables_entorno(cams, {"Patio": "rtsp://u:clave@10.0.0.5/s"})
    assert env == {"FRIGATE_RTSP_PATIO": "rtsp://u:clave@10.0.0.5/s"} and "u:clave@" not in y


def test_evento_frena_guardian():
    ev = {"after": {"label": "person", "current_zones": ["robot"], "camera": "patio", "top_score": 0.91}}
    assert vis.evento_guardian(ev)["accion"] == "frenar"
    assert vis.evento_guardian({"after": {"label": "person", "current_zones": ["jardin"]}}) is None
    assert vis.evento_guardian({"after": {"label": "dog", "current_zones": ["robot"]}}) is None


# ---------------------------------------------------------------- 5 · casa (Home Assistant)
ESTADOS = [{"entity_id": "light.taller", "state": "on", "attributes": {"friendly_name": "Luz del taller"}},
           {"entity_id": "light.living", "state": "off", "attributes": {"friendly_name": "Luz living"}},
           {"entity_id": "lock.puerta", "state": "locked", "attributes": {"friendly_name": "Cerradura puerta"}},
           {"entity_id": "sensor.temp_pieza", "state": "21.5", "attributes": {"friendly_name": "Temperatura pieza", "unit_of_measurement": "°C"}}]


class Resp:
    def __init__(self, d):
        self.d = d

    def __enter__(self):
        return self

    def __exit__(self, *a):
        pass

    def read(self):
        return json.dumps(self.d).encode()


def test_casa_por_voz():
    llamadas = []

    def abrir(req, timeout=10):
        llamadas.append((req.get_method(), req.full_url, req.data))
        assert req.headers["Authorization"] == "Bearer tok"
        return Resp(ESTADOS if req.get_method() == "GET" else [])
    h = casa.Casa("http://ha:8123", "tok", abrir)
    r = h.ordenar("apaga la luz del taller")
    assert r["ok"] and r["entidad"] == "light.taller" and r["servicio"] == "turn_off"
    assert llamadas[-1][1].endswith("/api/services/light/turn_off")
    q = h.ordenar("qué temperatura hay en la pieza")
    assert q["valor"] == "21.5" and q["unidad"] == "°C"
    c = h.ordenar("desbloquea la cerradura de la puerta")
    assert c.get("necesita_confirmar") and c["entidad"] == "lock.puerta"
    assert h.ordenar("desbloquea la cerradura de la puerta", confirmado=True)["servicio"] == "unlock"


def test_casa_sin_token():
    try:
        casa.Casa("http://ha", "").estados()
        assert False
    except PermissionError as e:
        assert "token" in str(e)


# ---------------------------------------------------------------- 6 · dictado
def test_dictado_a_word(tmp_path):
    t = ("título informe de procesos industriales sección introducción punto aparte eh este trabajo describe el proceso de tortillas "
         "punto aparte o sea se estudian las etapas sección desarrollo punto amasado punto laminado "
         "tabla etapa, tiempo tabla amasado, 10 min")
    b = dic.ordenar(t)
    assert b[0] == ("titulo", "Informe de procesos industriales")
    assert ("seccion", "Introducción") in b and ("punto", "Amasado.") in b
    assert not any(x.startswith("Eh") for _, x in b) and ("parrafo", "Este trabajo describe el proceso de tortillas.") in b and ("tabla", "Etapa, tiempo") in b
    ruta = dic.documento(b, str(tmp_path / "inf.docx"), "informe-duoc", {"asignatura": "EAI 6122", "integrantes": ["Davis", "Uwe"]})
    import docx
    d = docx.Document(ruta)
    textos = [p.text for p in d.paragraphs]
    assert "INFORME DE PROCESOS INDUSTRIALES" in textos and "1. Introducción" in textos and "2. Desarrollo" in textos
    assert d.paragraphs[0].alignment == 2                       # logo DUOC arriba a la derecha
    assert len(d.tables) == 1 and d.tables[0].rows[1].cells[0].text == "Amasado"


# ---------------------------------------------------------------- 8 · actualizaciones
def test_versiones():
    assert act.mayor("0.10", "0.9") and act.mayor("1.0.1", "1.0") and not act.mayor("0.9", "0.9.0")


def test_deb_canal_y_suma(tmp_path):
    repo = Path(__file__).resolve().parents[2]
    deb = act.construir_deb(repo, "0.9.1", tmp_path / "out")
    info = subprocess.run(["dpkg-deb", "-I", str(deb)], capture_output=True, text=True).stdout
    assert "neo-orion-core" in info and "0.9.1" in info
    lista = subprocess.run(["dpkg-deb", "-c", str(deb)], capture_output=True, text=True).stdout
    assert "./opt/orion/orion_core/servidor.py" in lista and "./usr/share/orion/hud/index.html" in lista
    canal = act.publicar_canal(deb, "0.9.1", ["Calculadora RIC"], tmp_path / "canal")
    leer = lambda u: Path(u.replace("file://", "")).read_bytes()  # noqa: E731
    r = act.revisar("file://" + str(canal.parent), leer)
    assert r["ok"] and r["hay_nueva"] and r["ultima"] == "0.9.1"
    assert act.bajar_y_verificar(r, leer, tmp_path).exists()
    malo = {**r, "sha256": "0" * 64}
    try:
        act.bajar_y_verificar(malo, leer, tmp_path)
        assert False
    except ValueError as e:
        assert "no se instala" in str(e)


# ---------------------------------------------------------------- 9 · traductor
def test_traductor_conversacion():
    dichos = []
    falso = lambda t, de, a: {"hola, ¿cómo estás?": "hi, how are you?", "i am fine, thanks": "estoy bien, gracias"}.get(t)  # noqa: E731
    falso.__name__ = "falso"
    c = tr.Conversacion("es", "en", motores=[falso], hablar=lambda t, a: dichos.append((t, a)))
    e1 = c.turno("hola, ¿cómo estás?")
    e2 = c.turno("i am fine, thanks")
    assert (e1["de"], e1["a"], e1["traduccion"]) == ("es", "en", "hi, how are you?")
    assert (e2["de"], e2["a"]) == ("en", "es") and dichos[-1] == ("estoy bien, gracias", "es")
    sin = tr.Conversacion(motores=[lambda *a: None]).turno("hola")
    assert "error" in sin


def test_detectar_idioma():
    assert tr.detectar_idioma("where is the station?") == "en"
    assert tr.detectar_idioma("¿dónde está la estación?") == "es"
    assert tr.detectar_idioma("obrigado, você é muito gentil", ("es", "pt")) == "pt"


# ---------------------------------------------------------------- controladores (gráfica y procesador)
from orion_core import controladores as ctl  # noqa: E402

LSPCI = """00:02.0 VGA compatible controller [0300]: Intel Corporation Alder Lake-P GT2 [Iris Xe Graphics] [8086:46a6] (rev 0c)
01:00.0 VGA compatible controller [0300]: NVIDIA Corporation GA106 [GeForce RTX 3060 Lite Hash Rate] [10de:2504] (rev a1)
02:00.0 Ethernet controller [0200]: Realtek Semiconductor Co., Ltd. RTL8111 [10ec:8168] (rev 15)
03:00.0 Display controller [0380]: Advanced Micro Devices, Inc. [AMD/ATI] Navi 23 [1002:73ff] (rev c7)"""
UBUNTU_DRIVERS = """== /sys/devices/pci0000:00/0000:00:01.0/0000:01:00.0 ==
modalias : pci:v000010DEd00002504sv
vendor   : NVIDIA Corporation
driver   : nvidia-driver-550 - distro non-free
driver   : nvidia-driver-570 - distro non-free recommended
driver   : nvidia-driver-570-open - distro non-free
driver   : xserver-xorg-video-nouveau - distro free builtin"""


def test_controladores_detecta_y_planifica():
    g = ctl.gpus(LSPCI)
    assert [x["fabricante"] for x in g] == ["Intel", "NVIDIA", "AMD"]          # la tarjeta de red no cuenta
    assert g[1]["id"] == "10de:2504" and "RTX 3060" in g[1]["modelo"]
    c = ctl.cpu("vendor_id\t: AuthenticAMD\nmodel name\t: AMD Ryzen 5 5600X\nflags\t\t: sse avx2\nprocessor\t: 0\n")
    assert c["fabricante"] == "AMD" and c["microcodigo"] == "amd64-microcode" and c["avx2"]
    assert ctl.recomendado_nvidia(UBUNTU_DRIVERS) == "nvidia-driver-570"
    pl = ctl.plan(c, g, "nvidia-driver-570")
    assert {"amd64-microcode", "nvidia-driver-570", "mesa-vulkan-drivers", "intel-media-va-driver", "mesa-va-drivers"} <= set(pl["paquetes"])
    sin = ctl.plan(c, [g[1]])                                                    # sin consultar: ubuntu-drivers en el primer arranque
    assert any("ubuntu-drivers install" in x for x in sin["pasos"])
    vm = ctl.plan(c, ctl.gpus("00:02.0 VGA compatible controller [0300]: Device [1234:1111] (rev 02)"))
    assert any("virtual" in n for n in vm["notas"]) and vm["paquetes"] == ["amd64-microcode"]


def test_controladores_al_dia():
    r = ctl.revisar_al_dia(UBUNTU_DRIVERS, "nvidia-driver-550")
    assert r["hay_nuevo"] and r["recomendado"] == "nvidia-driver-570" and "570" in r["comando"]
    assert not ctl.revisar_al_dia(UBUNTU_DRIVERS, "nvidia-driver-570")["hay_nuevo"]


# ---------------------------------------------------------------- llave de voz (frase elegida + huella)
from orion_voz import llave as lv  # noqa: E402
import numpy as np  # noqa: E402


class HuellaFalsa:
    """Cada «voz» es un vector base + un poco de ruido según los bytes."""
    def __call__(self, pcm):
        base = {b"D": [1, 0.2, 0.1, 0.0], b"X": [0.0, 0.1, 1, 0.3]}[pcm[:1]]
        r = np.random.default_rng(len(pcm)).normal(0, 0.03, 4)
        return np.array(base) + r


def test_frase_calza():
    assert lv.frase_calza("eh neo orion soy yo", "Neo Orión, soy yo") == 1.0
    assert lv.frase_calza("neo orio soy yo", "Neo Orión, soy yo") >= 0.7          # el oído entendió mal una letra
    assert lv.frase_calza("abre el taller", "Neo Orión, soy yo") < 0.6


def test_llave_configura_verifica_y_cambia(tmp_path):
    k = lv.LlaveVoz(huella=HuellaFalsa(), archivo=tmp_path / "llave.json")
    dav = [b"D" * (3200 + i) for i in range(3)]
    assert not k.configurar("hola", dav)["ok"]                                    # 1 palabra: muy corta
    r = k.configurar("Orión abre el taller", dav, ["orion abre el taller"] * 3)
    assert r["ok"] and lv.UMBRAL_MIN <= r["umbral"] <= lv.UMBRAL_MAX
    assert oct((tmp_path / "llave.json").stat().st_mode)[-3:] == "600"           # dato biométrico: solo el dueño
    assert k.verificar(b"D" * 4000, "orión abre el taller")["ok"]                 # su frase y su voz
    otra = k.verificar(b"X" * 4000, "orión abre el taller")
    assert not otra["ok"] and otra["frase_ok"] and not otra["voz_ok"]             # la frase, pero otra voz
    mala = k.verificar(b"D" * 4000, "apaga la luz")
    assert not mala["ok"] and not mala["frase_ok"] and mala["voz_ok"]             # su voz, pero no la frase
    assert k.cambiar_frase("Neo Orión soy yo", dav, ["neo orion soy yo"] * 3)["ok"]
    assert lv.LlaveVoz(huella=HuellaFalsa(), archivo=tmp_path / "llave.json").estado()["frase"] == "Neo Orión soy yo"
    assert not k.configurar("Neo Orión soy yo", dav, ["neo orion soy yo", "otra cosa", "neo orion soy yo"])["ok"]
    k.olvidar()
    assert not k.configurada and k.modo()["modo"] == "clave" and not k.verificar(b"D" * 4000, "x")["ok"]


def test_llave_con_huella_real(tmp_path):
    """La huella espectral real distingue un tono grave de uno agudo (sin modelos descargados)."""
    from orion_voz.hablantes import HuellaEspectral
    t = np.arange(16000 * 2) / 16000
    voz = lambda f: (np.sin(2 * np.pi * f * t) * 0.3 + np.sin(2 * np.pi * 2 * f * t) * 0.15 + np.random.default_rng(int(f)).normal(0, .01, t.size))  # noqa: E731
    pcm = lambda f: (voz(f) * 32767).astype("<i2").tobytes()  # noqa: E731
    k = lv.LlaveVoz(huella=HuellaEspectral(), archivo=tmp_path / "l.json")
    assert k.configurar("neo orion soy yo", [pcm(120), pcm(122), pcm(118)], ["neo orion soy yo"] * 3)["ok"]
    assert k.verificar(pcm(121), "neo orion soy yo")["voz_ok"]


def test_llave_una_vez_y_modos(tmp_path):
    k = lv.LlaveVoz(huella=HuellaFalsa(), archivo=tmp_path / "ll.json")
    assert k.modo()["modo"] == "clave"                                            # sin frase: solo clave
    assert not k.modo("voz")["ok"]                                                # no se puede exigir voz sin frase
    r = k.configurar("buenos días señoritas", [b"D" * 3300], ["buenos dias señoritas"])
    assert r["ok"] and r["umbral"] == lv.UMBRAL_UNA and "aviso" in r             # una sola vez: más exigente y avisa
    assert k.modo()["modo"] == "voz_o_clave"
    assert k.abrir(voz_ok=False, clave_ok=True) and k.abrir(True, False)
    k.modo("voz_y_clave")
    assert not k.abrir(True, False) and not k.abrir(False, True) and k.abrir(True, True)
    k.modo("voz"); assert not k.abrir(False, True)
    k.modo("clave"); assert not k.abrir(True, False) and k.abrir(False, True)
    assert not k.modo("cualquiera")["ok"]
    k.olvidar(); assert k.modo()["modo"] == "clave"


# ---------------------------------------------------------------- formas de iniciar (frases, palmadas, rutinas)
from orion_voz import activacion as acti  # noqa: E402


def audio_palmadas(tiempos, dur=3.0, fuerza=0.6):
    t = np.zeros(int(16000 * dur), dtype=np.float32)
    t += np.random.default_rng(1).normal(0, 0.002, t.size).astype(np.float32)   # ruido de fondo de la pieza
    for s in tiempos:
        i = int(s * 16000)
        n = int(0.03 * 16000)
        t[i:i + n] += (np.random.default_rng(i).normal(0, 1, n) * np.exp(-np.arange(n) / 150) * fuerza).astype(np.float32)
    return (np.clip(t, -1, 1) * 32767).astype("<i2").tobytes()


def test_palmadas():
    def contar(pcm):
        p, res = acti.Palmadas(), []
        for i in range(0, len(pcm), 3200):
            n = p.alimentar(pcm[i:i + 3200])
            if n:
                res.append(n)
        return res
    assert contar(audio_palmadas([0.5, 0.9])) == [2]
    assert contar(audio_palmadas([0.4, 0.75, 1.1], dur=3.5)) == [3]
    assert contar(audio_palmadas([0.5])) == []                                   # una sola no cuenta
    assert contar(audio_palmadas([0.5, 2.2], dur=4)) == []                       # muy separadas: no es serie
    assert contar(audio_palmadas([0.5, 0.9], fuerza=0.01)) == []                 # muy suaves: ruido


def test_activadores_y_rutina(tmp_path):
    c = acti.leer(tmp_path / "no-existe.json")
    assert acti.elegir(c, texto="eh buenos dias señoritas")["id"] == "buenos-dias"
    assert acti.elegir(c, palmadas=2)["rutina"][0] == "despertar"
    assert acti.elegir(c, texto="buenas noches", es_dueno=False) is None          # «solo el dueño»
    assert acti.elegir(c, texto="buenas noches", es_dueno=True)["rutina"] == ["reposo"]
    acti.agregar(c, "frase", ["despertar", "taller"], "Jarvis, a trabajar")
    acti.agregar(c, "palmadas", ["reposo"], veces=3)
    acti.guardar(c, tmp_path / "a.json")
    c2 = acti.leer(tmp_path / "a.json")
    assert acti.elegir(c2, texto="jarvis a trabajar")["rutina"] == ["despertar", "taller"]
    assert acti.elegir(c2, palmadas=3)["rutina"] == ["reposo"]
    for malo in [("frase", ["volar"], "hola mundo"), ("frase", ["saludo"], "hola")]:
        try:
            acti.agregar(c2, *malo); assert False
        except ValueError:
            pass
    hechas = []
    r = acti.ejecutar_rutina(["despertar", "saludo"], hacer=lambda a: hechas.append(a) or True)
    assert hechas == ["despertar", "saludo"] and all(x["ok"] for x in r)
    assert "Buenos días, Davis" in acti.saludo("Davis", time.struct_time((2026, 11, 4, 8, 0, 0, 2, 308, 0)))


def test_wake_on_lan():
    pk = acti.paquete_magico("9c:6b:00:12:ab:cd")
    assert len(pk) == 102 and pk[:6] == b"\xff" * 6 and pk[6:12] == bytes.fromhex("9c6b0012abcd")
    try:
        acti.paquete_magico("zz"); assert False
    except ValueError:
        pass
    assert "despertar_equipo" in acti.NODO_DESPERTADOR


def test_todo_editable(tmp_path):
    c = acti.leer(tmp_path / "x.json")
    acti.editar(c, "buenos-dias", {"frase": "Buenos días, equipo", "rutina": ["di:Arriba el ánimo", "despertar", "abre:music.youtube.com", "orden:muestra el gantt"]})
    a = next(x for x in c["activadores"] if x["id"] == "buenos-dias")
    assert a["frase"] == "Buenos días, equipo" and a["rutina"][0] == "di:Arriba el ánimo"
    assert acti.elegir(c, texto="buenos dias equipo")["id"] == "buenos-dias"
    acti.editar(c, "palmadas", {"veces": 3, "activo": False})
    assert acti.elegir(c, palmadas=3) is None                                   # en pausa no se dispara
    acti.editar(c, "palmadas", {"activo": True})
    assert acti.elegir(c, palmadas=3)["id"] == "palmadas"
    for malo in [{"rutina": ["abre:no es web"]}, {"rutina": []}, {"frase": "hola"}, {"rutina": ["di:"]}]:
        try:
            acti.editar(c, "buenos-dias", malo); assert False, malo
        except ValueError:
            pass
    r = acti.ejecutar_rutina(["di:Bienvenido"])
    assert r[0]["resultado"] == "Bienvenido"
    t = time.struct_time((2026, 11, 4, 21, 0, 0, 2, 308, 0))
    assert acti.saludo("Davis", t, "{saludo}, jefe {nombre}. ¿Qué construimos?") == "Buenas noches, jefe Davis. ¿Qué construimos?"
    assert acti.saludo("", t, "{saludo}, {nombre}. Todo listo.") == "Buenas noches. Todo listo."
    assert acti.palmadas_de({"palmadas": {"sensibilidad": "firme"}}).salto == 12.0
