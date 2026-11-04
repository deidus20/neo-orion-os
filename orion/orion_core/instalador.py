"""Instalador real de Neo Orión: lleva la sesión en vivo (pendrive) a un disco.

La interfaz (hud/instalar.html) junta las elecciones y las manda al servidor local; el
servidor lanza este programa como root. Hace lo que antes se le dejaba a Calamares:

  1. revisar la configuración (usuario, disco, modelos, color)
  2. particionar   borrar: GPT nueva (EFI 512 MB + BIOS 1 MB + Neo Orión)
                   junto:  usa el espacio libre más grande y la EFI que ya existe (Windows queda intacto)
  3. formatear y montar en /target
  4. copiar el sistema desde la imagen del pendrive (filesystem.squashfs)
  5. fstab, nombre del equipo, idioma, zona, teclado
  6. crear la cuenta (grupos de hardware, clave), sacar lo que es solo del pendrive
  7. identidad, configuración de la instalación, capa personal si se eligió
  8. cargador de arranque: UEFI (shim firmado, arranque seguro) + BIOS
  9. dejar el primer arranque listo para bajar las IA, oído y voz cuando haya internet

Progreso: una línea JSON por evento en el archivo de progreso
  {"pct": 37, "etapa": "COPIANDO SISTEMA", "detalle": "...", "log": "..."}
El modo `simular` imprime los comandos sin tocar nada (pruebas y vista previa).

Uso:  sudo python3 -m orion_core.instalador config.json [--progreso archivo] [--simular]
      La clave llega por la variable ORION_CLAVE o por la entrada estándar; nunca se escribe en disco.
"""
from __future__ import annotations

import json
import os
import re
import shlex
import subprocess
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path

from orion_core.instalacion import identidad_yaml, validar

OBJ = Path("/target")
MEDIOS = ["/cdrom/casper/filesystem.squashfs", "/run/live/medium/casper/filesystem.squashfs",
          "/media/cdrom/casper/filesystem.squashfs"]
SOLO_PENDRIVE = ["casper", "ubiquity", "calamares", "calamares-settings-neo-orion", "orion-instalador"]
GRUPOS = ["sudo", "adm", "audio", "video", "render", "input", "plugdev", "dialout", "netdev", "lpadmin"]
ZONA_OK = re.compile(r"^[A-Za-z_]+(/[A-Za-z_+-]+){0,2}$")
TECLADOS = {"latam": ("latam", ""), "es": ("es", ""), "us": ("us", "")}
IDIOMAS = {"es-CL": "es_CL.UTF-8", "es-MX": "es_MX.UTF-8", "es-ES": "es_ES.UTF-8", "en-US": "en_US.UTF-8"}


class ErrorInstalacion(RuntimeError):
    pass


@dataclass
class Ejecutor:
    """Corre comandos de verdad o solo los anota (simular)."""
    simular: bool = False
    hechos: list = field(default_factory=list)
    salidas: dict = field(default_factory=dict)     # para simular: prefijo de comando → salida

    def __call__(self, cmd: list[str], entrada: str | None = None, ok=(0,)) -> str:
        self.hechos.append(cmd)
        if self.simular:
            for pref, out in self.salidas.items():
                if " ".join(cmd).startswith(pref):
                    return out
            return ""
        try:
            r = subprocess.run(cmd, input=entrada, capture_output=True, text=True)
        except FileNotFoundError:
            if cmd[0] in ("udevadm", "partprobe", "swapoff"):      # ayudas opcionales: si no están, se sigue
                return ""
            raise ErrorInstalacion(f"falta el programa {cmd[0]}")
        if r.returncode not in ok:
            raise ErrorInstalacion(f"falló: {shlex.join(cmd)}\n{(r.stderr or r.stdout).strip()[-800:]}")
        return r.stdout

    def escribir(self, ruta: Path, texto: str, modo: int = 0o644) -> None:
        self.hechos.append(["escribir", str(ruta)])
        if self.simular:
            return
        ruta.parent.mkdir(parents=True, exist_ok=True)
        ruta.write_text(texto)
        os.chmod(ruta, modo)


class Progreso:
    def __init__(self, archivo: str | None):
        self.f = open(archivo, "a", buffering=1) if archivo else None
        self.pct = 0

    def __call__(self, pct: int | None = None, etapa: str = "", detalle: str = "", log: str = "", **extra) -> None:
        if pct is not None:
            self.pct = pct
        ev = {"t": round(time.time(), 1), "pct": self.pct, "etapa": etapa, "detalle": detalle, "log": log, **extra}
        line = json.dumps(ev, ensure_ascii=False)
        if self.f:
            self.f.write(line + "\n")
        else:
            print(line, flush=True)


# ---------------------------------------------------------------------------------------------
def particion(disco: str, n: int) -> str:
    """/dev/sda + 2 → /dev/sda2 ; /dev/nvme0n1 + 2 → /dev/nvme0n1p2 ; /dev/mmcblk0 → p2."""
    return f"{disco}p{n}" if re.search(r"\d$", disco) else f"{disco}{n}"


def revisar(c: dict) -> list[str]:
    err = validar(c)
    d = (c.get("disco") or {}).get("dispositivo", "")
    loop = r"|loop\d+" if os.environ.get("ORION_PRUEBA_LOOP") == "1" else ""      # solo pruebas: disco en un archivo
    if not re.fullmatch(r"/dev/(sd[a-z]+|vd[a-z]+|hd[a-z]+|xvd[a-z]+|nvme\d+n\d+|mmcblk\d+" + loop + ")", d):
        err.append(f"disco inválido: {d!r}")
    if (c.get("disco") or {}).get("modo") not in ("borrar", "junto"):
        err.append("modo de disco no soportado por el instalador rápido (usa borrar o junto; manual abre Calamares)")
    if (c.get("disco") or {}).get("modo") == "borrar" and not (c.get("disco") or {}).get("confirmado"):
        err.append("falta confirmar el borrado del disco")
    eq = (c.get("cuenta") or {}).get("equipo", "neo-orion")
    if not re.fullmatch(r"[a-z0-9][a-z0-9-]{0,62}", eq or ""):
        err.append(f"nombre de equipo inválido: {eq!r}")
    if c.get("zona") and not ZONA_OK.match(c["zona"]):
        err.append("zona horaria inválida")
    return err


def medio_vivo() -> str:
    for m in MEDIOS:
        if os.path.exists(m):
            return m
    raise ErrorInstalacion("no encuentro la imagen del sistema (filesystem.squashfs): ¿arrancaste desde el pendrive?")


def es_uefi() -> bool:
    if os.environ.get("ORION_PRUEBA_LOOP") == "1":     # en pruebas nunca se tocan las entradas de arranque del equipo
        return False
    return os.path.isdir("/sys/firmware/efi")


def particionar(x: Ejecutor, disco: str, modo: str, p: Progreso) -> dict:
    """Devuelve {'efi': dev, 'raiz': dev, 'formatear_efi': bool}."""
    for parte in x(["lsblk", "-lnpo", "NAME,MOUNTPOINT", disco]).splitlines():
        cols = parte.split()
        if len(cols) > 1 and cols[1]:
            x(["umount", "-l", cols[0]], ok=(0, 1, 32))
    x(["swapoff", "-a"], ok=(0, 1, 255))
    if modo == "borrar":
        p(4, "PARTICIONANDO", f"{disco} · tabla GPT nueva", "se borra todo el disco")
        x(["wipefs", "-a", disco])
        x(["sgdisk", "-Z", disco], ok=(0, 2))
        x(["sgdisk", "-n", "1:0:+512M", "-t", "1:ef00", "-c", "1:EFI",
           "-n", "2:0:+1M", "-t", "2:ef02", "-c", "2:BIOS",
           "-n", "3:0:0", "-t", "3:8304", "-c", "3:NEO-ORION", disco])
        x(["partprobe", disco], ok=(0, 1))
        x(["udevadm", "settle"], ok=(0, 1))
        return {"efi": particion(disco, 1), "raiz": particion(disco, 3), "formatear_efi": True}
    # junto: espacio libre más grande + EFI existente
    p(4, "PARTICIONANDO", f"{disco} · junto al sistema actual", "se usa el espacio libre; lo demás queda intacto")
    info = x(["sgdisk", "-p", disco])
    efi = None
    for ln in info.splitlines():
        m = re.match(r"\s*(\d+)\s+\d+\s+\d+\s+\S+\s+\S+\s+EF00", ln)
        if m:
            efi = particion(disco, int(m.group(1)))
    libre = x(["sgdisk", "-F", disco]).strip().splitlines()
    fin = x(["sgdisk", "-E", disco]).strip().splitlines()
    if not libre or not fin or int(fin[-1]) - int(libre[-1]) < 20 * 1024 * 1024 * 2:   # sectores de 512 B: 20 GB
        raise ErrorInstalacion("no hay al menos 20 GB libres en el disco: achica Windows primero o elige borrar")
    antes = set(re.findall(r"^\s*(\d+)\s", info, re.M))
    nuevos = []
    if not efi:
        x(["sgdisk", "-n", "0:0:+512M", "-t", "0:ef00", "-c", "0:EFI", disco])
    x(["sgdisk", "-n", "0:0:0", "-t", "0:8304", "-c", "0:NEO-ORION", disco])
    x(["partprobe", disco], ok=(0, 1))
    x(["udevadm", "settle"], ok=(0, 1))
    despues = x(["sgdisk", "-p", disco])
    nuevos = sorted(set(re.findall(r"^\s*(\d+)\s", despues, re.M)) - antes, key=int)
    if not nuevos:
        if x.simular:
            nuevos = ["9"] if efi else ["8", "9"]
        else:
            raise ErrorInstalacion("no se pudo crear la partición nueva")
    nueva_efi = not efi
    if not efi:
        efi = particion(disco, int(nuevos[0]))
    return {"efi": efi, "raiz": particion(disco, int(nuevos[-1])), "formatear_efi": nueva_efi}


def uuid(x: Ejecutor, dev: str) -> str:
    return x(["blkid", "-s", "UUID", "-o", "value", dev]).strip() or "SIMULADO"


def en_objetivo(x: Ejecutor, *cmd: str, entrada: str | None = None, ok=(0,)) -> str:
    return x(["chroot", str(OBJ), *cmd], entrada=entrada, ok=ok)


def montar_virtuales(x: Ejecutor) -> None:
    for fs, dest, extra in [("proc", "proc", ["-t", "proc"]), ("sysfs", "sys", ["-t", "sysfs"]),
                            ("/dev", "dev", ["--bind"]), ("devpts", "dev/pts", ["-t", "devpts"]), ("tmpfs", "run", ["-t", "tmpfs"])]:
        x(["mkdir", "-p", str(OBJ / dest)])
        x(["mount", *extra, fs, str(OBJ / dest)])
    if es_uefi() and os.path.isdir("/sys/firmware/efi/efivars"):
        x(["mount", "--bind", "/sys/firmware/efi/efivars", str(OBJ / "sys/firmware/efi/efivars")], ok=(0, 32))


def desmontar(x: Ejecutor) -> None:
    for d in ["sys/firmware/efi/efivars", "run", "dev/pts", "dev", "sys", "proc", "boot/efi", ""]:
        x(["umount", "-l", str(OBJ / d) if d else str(OBJ)], ok=(0, 1, 32))


def instalar(c: dict, clave: str, x: Ejecutor, p: Progreso, raiz_repo: Path | None = None) -> dict:
    err = revisar(c)
    if not clave or len(clave) < 6:
        err.append("la clave debe tener al menos 6 caracteres")
    if err:
        raise ErrorInstalacion("; ".join(err))
    disco, modo = c["disco"]["dispositivo"], c["disco"]["modo"]
    usuario = c["cuenta"]["usuario"].lower()
    equipo = c["cuenta"].get("equipo") or "neo-orion"
    p(1, "REVISANDO", f"{usuario}@{equipo} · {disco} · {modo}", "configuración válida")
    medio = "/cdrom/casper/filesystem.squashfs" if x.simular else medio_vivo()

    # 2-3 · particiones, formato y montaje -------------------------------------------------
    parts = particionar(x, disco, modo, p)
    p(8, "FORMATEANDO", f"raíz {parts['raiz']} · EFI {parts['efi']}")
    x(["mkfs.ext4", "-F", "-L", "neo-orion", parts["raiz"]])
    if parts["formatear_efi"]:
        x(["mkfs.vfat", "-F", "32", "-n", "EFI", parts["efi"]])
    x(["mkdir", "-p", str(OBJ)])
    x(["mount", parts["raiz"], str(OBJ)])

    # 4 · copia ------------------------------------------------------------------------------
    p(10, "COPIANDO SISTEMA", "Ubuntu 24.04 + Neo Orión desde el pendrive", "unsquashfs " + medio)
    if x.simular:
        x(["unsquashfs", "-f", "-d", str(OBJ), medio])
    else:
        _copiar_con_avance(medio, p)
    x(["mkdir", "-p", str(OBJ / "boot/efi")])
    sin_fat = (os.environ.get("ORION_PRUEBA_LOOP") == "1" and not x.simular
               and "vfat" not in Path("/proc/filesystems").read_text())          # solo el equipo de pruebas
    if not sin_fat:
        x(["mount", parts["efi"], str(OBJ / "boot/efi")])
    # el kernel no queda dentro del squashfs en algunas imágenes: se toma del pendrive
    for base in ("/cdrom/casper", "/run/live/medium/casper"):
        if not x.simular and os.path.exists(f"{base}/vmlinuz") and not list((OBJ / "boot").glob("vmlinuz-*")):
            ver = (OBJ / "usr/lib/modules").iterdir().__next__().name
            x(["cp", f"{base}/vmlinuz", str(OBJ / f"boot/vmlinuz-{ver}")])

    # 5 · sistema base -----------------------------------------------------------------------
    p(62, "CONFIGURANDO", "fstab, nombre del equipo, idioma, zona y teclado")
    u_raiz, u_efi = uuid(x, parts["raiz"]), uuid(x, parts["efi"])
    x.escribir(OBJ / "etc/fstab",
               "# Neo Orión · generado por el instalador\n"
               f"UUID={u_raiz}  /          ext4  errors=remount-ro  0 1\n"
               f"UUID={u_efi}  /boot/efi  vfat  umask=0077,nofail  0 1\n"
               "/swapfile  none  swap  sw  0 0\n")
    x.escribir(OBJ / "etc/hostname", equipo + "\n")
    x.escribir(OBJ / "etc/hosts", f"127.0.0.1 localhost\n127.0.1.1 {equipo}\n::1 localhost ip6-localhost ip6-loopback\n")
    loc = IDIOMAS.get(c.get("idioma", "es-CL"), "es_CL.UTF-8")
    x.escribir(OBJ / "etc/default/locale", f"LANG={loc}\nLANGUAGE={loc.split('.')[0]}:es\n")
    zona = c.get("zona") or "America/Santiago"
    x(["ln", "-sf", f"/usr/share/zoneinfo/{zona}", str(OBJ / "etc/localtime")])
    x.escribir(OBJ / "etc/timezone", zona + "\n")
    lay, var = TECLADOS.get(c.get("teclado", "latam"), ("latam", ""))
    x.escribir(OBJ / "etc/default/keyboard", f'XKBMODEL="pc105"\nXKBLAYOUT="{lay}"\nXKBVARIANT="{var}"\nXKBOPTIONS=""\nBACKSPACE="guess"\n')
    montar_virtuales(x)
    en_objetivo(x, "locale-gen", loc, ok=(0, 1))

    # swap: 2 GB (4 GB si hay ≤ 8 GB de RAM, ayuda al modo patata)
    p(66, "MEMORIA VIRTUAL", "archivo de intercambio + zram si está instalado")
    ram_gb = _ram_gb()
    x(["fallocate", "-l", "4G" if ram_gb <= 8 else "2G", str(OBJ / "swapfile")])
    x(["chmod", "600", str(OBJ / "swapfile")])
    x(["mkswap", str(OBJ / "swapfile")])

    # 6 · cuenta y limpieza del pendrive -------------------------------------------------------
    p(70, "CUENTA", f"{usuario} @ {equipo}", "grupos: " + ", ".join(GRUPOS))
    en_objetivo(x, "userdel", "-r", "orion", ok=(0, 6, 12))   # el usuario de la sesión en vivo, si quedó
    grupos = ",".join(g for g in GRUPOS if x.simular or _grupo_existe(g))
    en_objetivo(x, "useradd", "-m", "-s", "/bin/bash", "-G", grupos, "-c", usuario.capitalize(), usuario)
    en_objetivo(x, "chpasswd", entrada=f"{usuario}:{clave}\n")
    p(74, "LIMPIANDO", "se quitan los programas que solo sirven en el pendrive")
    en_objetivo(x, "apt-get", "purge", "-y", "--allow-remove-essential", *SOLO_PENDRIVE, ok=(0, 100))
    en_objetivo(x, "apt-get", "autoremove", "-y", "--purge", ok=(0, 100))
    for f in ("etc/casper.conf", "etc/sudoers.d/orion-live"):
        x(["rm", "-f", str(OBJ / f)])

    # 7 · Neo Orión: identidad, sesión, capa ------------------------------------------------------
    p(78, "IDENTIDAD", f"{c.get('identidad', {}).get('nombre', 'Neo Orión')} · {c.get('identidad', {}).get('color', '')}")
    pub = {k: v for k, v in c.items() if k != "clave"}
    x.escribir(OBJ / "etc/orion/instalacion.json", json.dumps(pub, ensure_ascii=False, indent=2) + "\n")
    base_ident = OBJ / "etc/orion/identidad.yaml"
    plantilla = base_ident.read_text() if base_ident.exists() else (
        (raiz_repo / "config/identidad.yaml").read_text() if raiz_repo else "nombre: Neo Orión\nidioma: es-CL\n")
    x.escribir(base_ident, identidad_yaml(c, plantilla))
    x.escribir(OBJ / "etc/orion/usuario", usuario + "\n")
    try:   # gráfica y procesador de ESTE equipo → el primer arranque deja sus drivers instalados
        from orion_core import controladores as ctl
        det = {} if x.simular else ctl.detectar()
        x.escribir(OBJ / "etc/orion/controladores.json", json.dumps(det, ensure_ascii=False, indent=2) + "\n")
        if det:
            p(79, "CONTROLADORES", " · ".join(det["plan"]["notas"]) or "sin controladores extra")
    except Exception:
        pass
    # la sesión a pantalla completa corre con la cuenta nueva (ya no con el usuario del pendrive)
    x.escribir(OBJ / "etc/systemd/system/orion-sesion.service.d/usuario.conf", f"[Service]\nUser={usuario}\n")
    casa = OBJ / "home" / usuario
    x(["mkdir", "-p", str(casa / "Orion"), str(casa / ".orion")])
    for n in (1, 2, 3):   # frases grabadas en el instalador → huella de voz en el primer arranque
        if os.path.exists(f"/run/orion-voz-{n}.wav"):
            x(["mkdir", "-p", str(casa / ".orion/voz")])
            x(["cp", f"/run/orion-voz-{n}.wav", str(casa / f".orion/voz/frase-{n}.wav")])
    x.escribir(casa / ".orion/acento", (c.get("identidad") or {}).get("color", "#ff2438") + "\n")
    if c.get("capa") not in (None, "", "base"):   # capa personal: la que venga en la ISO (CAPA=<nombre>)
        for aplicar in sorted((OBJ / "usr/share/orion/capas").glob("*/aplicar.sh")):
            p(80, "CAPA PERSONAL", aplicar.parent.name)
            en_objetivo(x, "sudo", "-u", usuario, "env", f"HOME=/home/{usuario}", "bash",
                        "/" + str(aplicar.relative_to(OBJ)), ok=(0, 1))
    en_objetivo(x, "chown", "-R", f"{usuario}:{usuario}", f"/home/{usuario}")
    en_objetivo(x, "systemctl", "enable", "orion-sesion.service", "orion-primer-arranque.service", ok=(0, 1))

    # 8 · arranque ---------------------------------------------------------------------------------
    p(84, "ARRANQUE", "GRUB · UEFI con arranque seguro + BIOS")
    x.escribir(OBJ / "etc/default/grub.d/neo-orion.cfg",
               'GRUB_DISTRIBUTOR="Neo Orión"\nGRUB_TIMEOUT_STYLE=hidden\nGRUB_TIMEOUT=2\n'
               'GRUB_CMDLINE_LINUX_DEFAULT="quiet splash loglevel=3"\nGRUB_DISABLE_OS_PROBER=false\n')
    # GRUB escribe root=UUID solo si existe /dev/disk/by-uuid/<UUID>; sin udev (algunos entornos) pondría
    # el nombre del dispositivo (/dev/sda3), que cambia entre equipos. Se asegura el enlace.
    for dev, u in ((parts["raiz"], u_raiz), (parts["efi"], u_efi)):
        if u and u != "SIMULADO" and not x.simular and not Path(f"/dev/disk/by-uuid/{u}").exists():
            Path("/dev/disk/by-uuid").mkdir(parents=True, exist_ok=True)
            os.symlink(dev, f"/dev/disk/by-uuid/{u}")
    x.escribir(OBJ / "etc/default/grub.d/10-uuid.cfg", "GRUB_DISABLE_LINUX_UUID=false\n")
    en_objetivo(x, "update-initramfs", "-c", "-k", "all", ok=(0, 1))
    efi = ["grub-install", "--target=x86_64-efi", "--efi-directory=/boot/efi", "--uefi-secure-boot"]
    if sin_fat:
        p(86, "ARRANQUE", "prueba sin FAT: solo BIOS")
    else:
        if es_uefi():   # entrada con nombre en la BIOS
            en_objetivo(x, *efi, "--bootloader-id=ubuntu")
        # siempre también la ruta de respaldo EFI/BOOT/BOOTX64.EFI (BIOS que olvidan entradas, o se instaló desde modo BIOS)
        en_objetivo(x, *efi, "--removable", "--no-nvram", ok=(0,) if not es_uefi() else (0, 1))
    if modo == "borrar":
        en_objetivo(x, "grub-install", "--target=i386-pc", disco, ok=(0, 1))
    en_objetivo(x, "update-grub")

    # 9 · primer arranque ---------------------------------------------------------------------------
    p(94, "PRIMER ARRANQUE", "IA, oído y voz se descargan apenas haya internet",
      "pendiente: " + ", ".join((c.get("ia") or {}).get("locales", [])))
    x(["sync"])
    desmontar(x)
    p(100, "LISTO", f"Neo Orión quedó en {disco}", "puedes reiniciar y sacar el pendrive", terminado=True)
    return {"ok": True, "raiz": parts["raiz"], "efi": parts["efi"], "comandos": len(x.hechos)}


def _copiar_con_avance(medio: str, p: Progreso) -> None:
    """unsquashfs con su barra de avance → pct 10..60."""
    nproc = str(max(1, (os.cpu_count() or 2)))
    proc = subprocess.Popen(["unsquashfs", "-f", "-p", nproc, "-d", str(OBJ), medio],
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, bufsize=1)
    ultimo = -1
    buf = ""
    while True:
        ch = proc.stdout.read(256)
        if not ch:
            break
        buf = (buf + ch)[-400:]
        m = re.findall(r"(\d+)%", buf)
        if m:
            v = int(m[-1])
            if v // 5 != ultimo:
                ultimo = v // 5
                p(10 + v // 2, "COPIANDO SISTEMA", f"{v}% copiado")
    if proc.wait() != 0:
        raise ErrorInstalacion("falló la copia del sistema (unsquashfs): " + buf[-300:])


def _ram_gb() -> float:
    try:
        return int(re.search(r"MemTotal:\s+(\d+)", open("/proc/meminfo").read()).group(1)) / 1024 / 1024
    except Exception:
        return 8.0


def _grupo_existe(g: str) -> bool:
    try:
        return any(l.split(":")[0] == g for l in (OBJ / "etc/group").read_text().splitlines())
    except OSError:
        return False


def discos(x: Ejecutor | None = None) -> list[dict]:
    """Discos donde se puede instalar (sin el pendrive del que se arrancó ni loops/cd)."""
    x = x or Ejecutor()
    try:
        data = json.loads(x(["lsblk", "-J", "-b", "-o", "NAME,PATH,SIZE,TYPE,MODEL,TRAN,RM,FSTYPE,LABEL,MOUNTPOINT,PTTYPE"]))
    except Exception:
        return []
    medio = ""
    for m in ("/cdrom", "/run/live/medium"):
        try:
            src = subprocess.run(["findmnt", "-no", "SOURCE", m], capture_output=True, text=True).stdout.strip()
            if src:
                medio = re.sub(r"p?\d+$", "", src) if not src.startswith("/dev/sr") else src
        except OSError:
            pass
    out = []
    for d in data.get("blockdevices", []):
        if d.get("type") != "disk" or d.get("path") == medio or (d.get("size") or 0) < 16 * 10**9:
            continue
        partes = [{"nombre": (ch.get("label") or ch.get("fstype") or ch["name"]), "gb": round((ch.get("size") or 0) / 10**9),
                   "fs": ch.get("fstype") or ""} for ch in d.get("children", []) if ch.get("type") == "part"]
        usado = sum(pa["gb"] for pa in partes)
        total = round(d["size"] / 10**9)
        if total - usado > 1:
            partes.append({"nombre": "libre", "gb": total - usado, "fs": ""})
        out.append({"nombre": d["name"], "ruta": d["path"], "gb": total, "modelo": (d.get("model") or "").strip(),
                    "bus": d.get("tran") or "", "extraible": bool(d.get("rm")), "partes": partes,
                    "windows": any(pa["fs"] == "ntfs" for pa in partes)})
    return out


def main(argv: list[str]) -> int:
    import argparse
    ap = argparse.ArgumentParser(description="Instala Neo Orión en un disco")
    ap.add_argument("config")
    ap.add_argument("--progreso")
    ap.add_argument("--simular", action="store_true")
    a = ap.parse_args(argv)
    c = json.loads(Path(a.config).read_text())
    clave = os.environ.pop("ORION_CLAVE", "") or (sys.stdin.readline().rstrip("\n") if not sys.stdin.isatty() else "")
    p = Progreso(a.progreso)
    x = Ejecutor(simular=a.simular)
    if not a.simular and os.geteuid() != 0:
        p(0, "ERROR", "el instalador necesita permisos de administrador", error=True, terminado=True)
        return 2
    try:
        r = instalar(c, clave, x, p, Path(__file__).resolve().parents[2])
        if a.simular:
            for cmd in x.hechos:
                p(log="$ " + " ".join(map(str, cmd)))
        return 0 if r["ok"] else 1
    except ErrorInstalacion as e:
        p(etapa="ERROR", detalle=str(e).splitlines()[0][:200], log=str(e), error=True, terminado=True)
        if not a.simular:
            desmontar(x)
        return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
