import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from orion_juegos import PLATAFORMAS, dispositivos, plan, recomendar  # noqa: E402

PROC = '''I: Bus=0003 Vendor=045e Product=0b12 Version=0110
N: Name="Microsoft Xbox Series S|X Controller"
H: Handlers=event20 js0

I: Bus=0005 Vendor=054c Product=0ce6 Version=8100
N: Name="DualSense Wireless Controller"
H: Handlers=event21 js1

I: Bus=0005 Vendor=054c Product=0ce6 Version=8100
N: Name="DualSense Wireless Controller Motion Sensors"
H: Handlers=event22

I: Bus=0003 Vendor=046d Product=c52b Version=0111
N: Name="Logitech USB Receiver Mouse"
H: Handlers=mouse0 event5

I: Bus=0003 Vendor=046d Product=c52b Version=0111
N: Name="Logitech USB Receiver Keyboard"
H: Handlers=sysrq kbd leds event4

I: Bus=0018 Vendor=06cb Product=7e7e Version=0100
N: Name="SYNA Touchpad"
H: Handlers=mouse1 event6
'''

d = dispositivos(PROC)
assert [m["marca"] for m in d["mandos"]] == ["Xbox", "PlayStation"], d
assert len(d["mouse"]) == 1 and len(d["teclados"]) == 1, d

r = recomendar(gpu=None)
assert r["principal"][0] == "geforcenow"
r = recomendar(gpu="RTX 3060", vram_gb=12)
assert r["principal"][0] == "steam" and "sunshine" in r["extras"]
r = recomendar(gpu=None, red_mbps=8)
assert r["principal"][0] == "moonlight"

p = plan("quiero jugar en geforce now")
assert p["clave"] == "geforcenow" and "com.nvidia.geforcenow" in p["pasos"][-1]
p = plan("pon netflix")
assert p["pasos"][-1] == "xdg-open https://www.netflix.com"
p = plan("quiero jugar", {"gpu": None})
assert p["clave"] == "geforcenow"
p = plan("abre mis series")
assert p["clave"] == "jellyfin"
p = plan("emulador de snes")
assert p["clave"] == "retroarch"
from orion_juegos import nubes
n = nubes()
assert n[0]["chile"].startswith("sí") and n[-1]["clave"] == "luna", [i["clave"] for i in n]
assert len(n) == 6
assert plan("quiero jugar en boosteroid")["clave"] == "boosteroid"
assert "shadow-amd64.deb" in " ".join(plan("abre shadow")["pasos"])
assert "Linux" in plan("juega playstation plus")["aviso"]
assert all(x.licencia for x in PLATAFORMAS.values())
print("juegos ok:", len(PLATAFORMAS), "plataformas")
