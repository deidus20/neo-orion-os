#!/usr/bin/env python3
"""Nodo despertador (Raspberry Pi con micrófono USB): escucha palmadas o la frase y despierta al PC central.
Instalar: sudo apt install python3-numpy alsa-utils && pip install vosk ; copiar orion_voz/activacion.py al lado."""
import subprocess, sys
from activacion import Palmadas, despertar_equipo
MAC = sys.argv[1]                      # MAC del PC central (la muestra «prepara el encendido por red» en el HUD)
p = Palmadas()
arec = subprocess.Popen(["arecord", "-q", "-f", "S16_LE", "-r", "16000", "-c", "1", "-t", "raw"], stdout=subprocess.PIPE)
while True:
    n = p.alimentar(arec.stdout.read(3200))
    if n and n >= 2:
        despertar_equipo(MAC)
        print("palmadas:", n, "→ despertando", MAC, flush=True)
