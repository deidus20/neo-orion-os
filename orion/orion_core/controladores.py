"""Controladores: reconoce el procesador y la(s) gráfica(s) y deja sus drivers instalados y al día.

Cómo decide (por el fabricante del chip, con los ID PCI oficiales: 10de NVIDIA, 1002 AMD, 8086 Intel):
  · Procesador Intel o AMD → microcódigo (intel-microcode / amd64-microcode): corrige fallas y seguridad del CPU.
  · NVIDIA → el driver propietario que recomienda Ubuntu para ESE modelo (`ubuntu-drivers`), firmado para el
    arranque seguro; con él funciona CUDA para la IA local. Si la tarjeta es muy antigua, queda nouveau (libre).
  · AMD (Radeon) → amdgpu, que ya viene en el núcleo + Mesa (OpenGL y Vulkan) + firmware: no hay nada que bajar
    aparte; se agrega ROCm solo si se pide para IA.
  · Intel (iGPU / Arc) → i915/xe del núcleo + Mesa + aceleración de video (VA-API).
  · Máquina virtual → los controladores virtuales del núcleo; nada que instalar.
  · Equipos muy nuevos (núcleo sin soporte) → núcleo HWE de Ubuntu 24.04 (más reciente, mismo sistema).

Mantenerlos al día:
  · los drivers vienen de los repositorios de Ubuntu y se actualizan con el sistema (parches automáticos de
    seguridad + «actualiza todo»); `revisar_al_dia()` avisa si Ubuntu recomienda un driver NVIDIA más nuevo;
  · el firmware de placa, BIOS/UEFI, SSD y periféricos compatibles se actualiza con fwupd (LVFS), solo si la
    persona lo aprueba: tocar el BIOS es irreversible.
"""
from __future__ import annotations

import json
import re
import subprocess
from pathlib import Path

VENDOR = {"10de": "NVIDIA", "1002": "AMD", "1022": "AMD", "8086": "Intel", "1234": "QEMU", "15ad": "VMware",
          "80ee": "VirtualBox", "1af4": "virtio", "1b36": "QEMU"}
VIRTUALES = {"QEMU", "VMware", "VirtualBox", "virtio"}
CLASE_GPU = re.compile(r"\[(0300|0302|0380)\]")


def _sh(cmd: list[str], timeout: int = 20) -> str:
    try:
        return subprocess.run(cmd, capture_output=True, text=True, timeout=timeout).stdout
    except (OSError, subprocess.SubprocessError):
        return ""


def cpu(cpuinfo: str | None = None) -> dict:
    t = cpuinfo if cpuinfo is not None else (Path("/proc/cpuinfo").read_text() if Path("/proc/cpuinfo").exists() else "")
    vendor = (re.search(r"vendor_id\s*:\s*(\S+)", t) or [None, ""])[1]
    modelo = (re.search(r"model name\s*:\s*(.+)", t) or [None, ""])[1].strip()
    fab = "Intel" if "Intel" in vendor else "AMD" if "AMD" in vendor else vendor or "desconocido"
    flags = set((re.search(r"^flags\s*:(.*)$", t, re.M) or [None, ""])[1].split())
    return {"fabricante": fab, "modelo": re.sub(r"\s+", " ", modelo), "nucleos": t.count("processor\t:") or None,
            "avx2": "avx2" in flags, "avx512": any(f.startswith("avx512") for f in flags),
            "microcodigo": {"Intel": "intel-microcode", "AMD": "amd64-microcode"}.get(fab)}


def gpus(lspci_nn: str | None = None) -> list[dict]:
    """Gráficas desde `lspci -nn` (clase 0300 VGA, 0302 3D, 0380 display)."""
    t = lspci_nn if lspci_nn is not None else _sh(["lspci", "-nn"])
    out = []
    for l in t.splitlines():
        if not CLASE_GPU.search(l):
            continue
        ids = re.findall(r"\[([0-9a-f]{4}):([0-9a-f]{4})\]", l)
        if not ids:
            continue
        ven, dev = ids[-1]
        nombre = re.sub(r"\s*\[[0-9a-f]{4}:[0-9a-f]{4}\].*$", "", l.split(": ", 1)[-1]).strip()
        nombre = re.sub(r"\s*\(rev [0-9a-f]+\)", "", nombre)
        out.append({"pci": l.split()[0], "fabricante": VENDOR.get(ven, ven), "id": f"{ven}:{dev}", "modelo": nombre})
    return out


def driver_en_uso(pci: str) -> str:
    m = re.search(r"Kernel driver in use:\s*(\S+)", _sh(["lspci", "-k", "-s", pci]))
    return m.group(1) if m else ""


def recomendado_nvidia(ubuntu_drivers: str | None = None) -> str | None:
    """Driver NVIDIA que Ubuntu recomienda para este equipo («... nvidia-driver-570 - distro non-free recommended»)."""
    t = ubuntu_drivers if ubuntu_drivers is not None else _sh(["ubuntu-drivers", "devices"], 60)
    m = re.search(r"driver\s*:\s*(nvidia-driver-\d+(?:-open|-server(?:-open)?)?)\s.*recommended", t)
    if m:
        return m.group(1)
    todos = sorted(set(re.findall(r"(nvidia-driver-\d+)\b", t)), key=lambda x: int(re.search(r"\d+", x).group()))
    return todos[-1] if todos else None


def plan(c: dict, g: list[dict], nvidia: str | None = None, nucleo_nuevo: bool = False) -> dict:
    """Qué instalar. nvidia: el driver recomendado (si ya se consultó)."""
    paquetes, pasos, notas = [], [], []
    if c.get("microcodigo"):
        paquetes.append(c["microcodigo"])
    fabs = {x["fabricante"] for x in g}
    if fabs and fabs <= VIRTUALES:
        notas.append("máquina virtual: usa los controladores virtuales del núcleo")
    if "NVIDIA" in fabs:
        if nvidia:
            paquetes.append(nvidia)
            notas.append(f"NVIDIA: {nvidia} (propietario, firmado para arranque seguro; habilita CUDA para la IA local)")
        else:
            pasos.append("ubuntu-drivers install   # elige el driver NVIDIA recomendado para este modelo")
            notas.append("NVIDIA: se elige el driver recomendado al tener internet")
    if "AMD" in fabs:
        paquetes += ["mesa-vulkan-drivers", "mesa-va-drivers"]
        notas.append("AMD: amdgpu del núcleo + Mesa (OpenGL/Vulkan) + firmware; ROCm solo si se pide para IA")
    if "Intel" in fabs:
        paquetes += ["mesa-vulkan-drivers", "intel-media-va-driver"]
        notas.append("Intel: i915/xe del núcleo + Mesa + aceleración de video")
    if nucleo_nuevo:
        paquetes.append("linux-generic-hwe-24.04")
        notas.append("núcleo HWE: soporte para hardware más nuevo")
    return {"paquetes": sorted(set(paquetes)), "pasos": pasos, "notas": notas}


def detectar() -> dict:
    c, g = cpu(), gpus()
    for x in g:
        x["driver"] = driver_en_uso(x["pci"])
    return {"cpu": c, "gpus": g, "plan": plan(c, g)}


def revisar_al_dia(ubuntu_drivers: str | None = None, instalado: str | None = None) -> dict:
    """¿Hay un driver NVIDIA más nuevo recomendado que el instalado?"""
    rec = recomendado_nvidia(ubuntu_drivers)
    if instalado is None:
        m = re.search(r"^ii\s+(nvidia-driver-\d+\S*)", _sh(["dpkg", "-l", "nvidia-driver-*"]), re.M)
        instalado = m.group(1) if m else ""
    num = lambda s: int((re.search(r"\d+", s or "") or [0])[0]) if s else 0  # noqa: E731
    nuevo = bool(rec and instalado and num(rec) > num(instalado))
    return {"instalado": instalado or None, "recomendado": rec, "hay_nuevo": nuevo,
            "comando": f"sudo apt-get install -y {rec}" if nuevo else ""}


def firmware(ejecutar=subprocess.run) -> dict:
    """Actualizaciones de firmware disponibles (fwupd/LVFS). Solo informa: instalar pide confirmación."""
    r = ejecutar(["fwupdmgr", "get-updates", "--json"], capture_output=True, text=True, timeout=120)
    try:
        d = json.loads(r.stdout or "{}")
    except ValueError:
        return {"ok": False, "error": (r.stderr or "fwupd no respondió")[-200:]}
    disp = [{"equipo": x.get("Name"), "version": x.get("Version"), "nueva": (x.get("Releases") or [{}])[0].get("Version")}
            for x in d.get("Devices", []) if x.get("Releases")]
    return {"ok": True, "actualizaciones": disp}


if __name__ == "__main__":
    print(json.dumps(detectar(), ensure_ascii=False, indent=2))
