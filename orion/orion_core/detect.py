"""¿Dónde estoy corriendo y qué tengo conectado?

Distingue dos cosas que suelen mezclarse:
  · HOST  — el equipo donde corre Neo Orión (torre, notebook, Raspberry, Jetson,
            Android con Termux, máquina virtual…). Solo hosts Linux ejecutan el sistema.
  · PERIFÉRICOS — lo que el host ve conectado: ESP32/Arduino por USB, cámaras,
            teléfonos por adb, lentes por el teléfono, PLC por red. Esos no
            ejecutan Neo Orión: son ojos, oídos o manos del sistema.
"""
from __future__ import annotations
import glob, json, os, platform, re, shutil, subprocess

USB_IDS = {   # VID:PID → qué es
    "10c4:ea60": "ESP32/placa con CP210x", "1a86:7523": "ESP32/Arduino con CH340",
    "1a86:55d4": "ESP32 con CH9102", "303a:1001": "ESP32-S3/C3 (USB nativo)",
    "303a:0002": "ESP32-S2 (USB nativo)", "2341:0043": "Arduino Uno", "2341:0042": "Arduino Mega",
    "2e8a:0005": "Raspberry Pi Pico (MicroPython)", "2e8a:0003": "Raspberry Pi Pico (bootloader)",
    "0483:3748": "ST-Link v2 (STM32)", "0483:374b": "ST-Link v2.1 (STM32)",
    "0403:6001": "adaptador FTDI (servos/serie)", "0403:6014": "FTDI FT232H",
    "18d1:4ee7": "teléfono Android (adb)", "05ac:12a8": "iPhone",
}


def _read(p):
    try:
        return open(p, "rb").read().decode(errors="ignore").strip("\x00 \n")
    except OSError:
        return ""


def _sh(cmd):
    try:
        return subprocess.run(cmd, capture_output=True, text=True, timeout=5).stdout
    except Exception:
        return ""


def host() -> dict:
    model = _read("/proc/device-tree/model")
    arch = platform.machine()
    h = {"arch": arch, "kernel": platform.release(), "model": model or "", "tipo": "pc"}
    if "Raspberry Pi" in model:
        h["tipo"] = "raspberry-pi"
    elif "Jetson" in model or os.path.exists("/etc/nv_tegra_release"):
        h["tipo"] = "jetson"
    elif os.environ.get("TERMUX_VERSION") or os.path.isdir("/data/data/com.termux"):
        h["tipo"] = "android-termux"
    elif "microsoft" in platform.release().lower():
        h["tipo"] = "wsl"
    else:
        virt = _sh(["systemd-detect-virt"]).strip()
        if virt and virt != "none":
            h["tipo"], h["virtualizacion"] = "maquina-virtual", virt
        elif os.path.exists("/.dockerenv"):
            h["tipo"] = "contenedor"
        else:
            chassis = _read("/sys/class/dmi/id/chassis_type")
            h["tipo"] = {"8": "notebook", "9": "notebook", "10": "notebook", "14": "notebook",
                         "31": "notebook", "3": "torre", "4": "torre", "6": "torre", "7": "torre",
                         "13": "todo-en-uno", "35": "mini-pc"}.get(chassis, "pc")
    try:
        mem = int(re.search(r"MemTotal:\s+(\d+)", open("/proc/meminfo").read()).group(1))
        h["ram_gb"] = round(mem / 1024 / 1024, 1)
    except Exception:
        pass
    h["gpu_nvidia"] = bool(shutil.which("nvidia-smi") and _sh(["nvidia-smi", "-L"]).strip())
    h["pantalla"] = bool(os.environ.get("DISPLAY") or os.environ.get("WAYLAND_DISPLAY"))
    try:
        fl = re.search(r"^flags\s*:(.*)$", open("/proc/cpuinfo").read(), re.M)
        h["avx2"] = bool(fl) and "avx2" in fl.group(1).split()
    except (OSError, AttributeError):
        h["avx2"] = True   # sin datos: no castigar
    h["rol_sugerido"] = ("nodo-robot" if h["tipo"] in ("raspberry-pi", "jetson") and h.get("ram_gb", 0) <= 8
                         else "central")
    return h


def peripherals() -> list[dict]:
    out = []
    for dev in glob.glob("/sys/bus/usb/devices/*"):
        vid, pid = _read(f"{dev}/idVendor"), _read(f"{dev}/idProduct")
        if not vid:
            continue
        key = f"{vid}:{pid}"
        name = USB_IDS.get(key) or (USB_IDS.get(f"{vid}:0000") if vid == "303a" else None)
        if vid == "303a" and not name:
            name = "placa Espressif (ESP32)"
        if name:
            out.append({"tipo": "usb", "id": key, "que_es": name, "producto": _read(f"{dev}/product")})
    for tty in sorted(glob.glob("/dev/ttyUSB*") + glob.glob("/dev/ttyACM*")):
        out.append({"tipo": "serie", "dispositivo": tty, "que_es": "puerto serie (microcontrolador / servos)"})
    for cam in sorted(glob.glob("/dev/video*")):
        name = _read(f"/sys/class/video4linux/{os.path.basename(cam)}/name")
        out.append({"tipo": "camara", "dispositivo": cam, "que_es": name or "cámara"})
    if shutil.which("adb"):
        for line in _sh(["adb", "devices", "-l"]).splitlines()[1:]:
            if "device" in line:
                out.append({"tipo": "android", "que_es": "teléfono Android", "detalle": line.strip()})
    if shutil.which("arecord"):
        for line in _sh(["arecord", "-l"]).splitlines():
            if line.startswith("card"):
                out.append({"tipo": "microfono", "que_es": line.split(":", 1)[1].strip()})
    return out


def scan() -> dict:
    return {"host": host(), "perifericos": peripherals()}


if __name__ == "__main__":
    print(json.dumps(scan(), ensure_ascii=False, indent=2))
