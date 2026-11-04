# Neo Orión OS 0.1 «Polaris» · edición patata

Primera versión pública. Sistema por voz sobre Ubuntu 24.04 (GPL-3.0). Autor: Davis Reyes.

**Descarga directa (un solo archivo, 1,4 GB):** `neo-orion-0.1-patata-amd64.iso`
Revisa la suma: `sha256sum -c neo-orion-0.1-patata-amd64.iso.sha256`

## Cómo usarla
1. Grábala en un pendrive de 8 GB o más con balenaEtcher o Rufus (modo DD).
2. Arranca el PC desde el pendrive: aparece el instalador de Neo Orión.
3. Al primer arranque con internet baja la IA, el oído, la voz y los controladores.

¿Ya tienes Ubuntu 24.04? No necesitas la ISO:
```
curl -fsSL https://raw.githubusercontent.com/deidus20/neo-orion-os/main/instalar.sh | sudo bash
```

## Qué trae
HUD por voz con estética de bloques, IA local según tu equipo, llave de voz, frases y palmadas para despertar el PC,
controladores al día, efecto CRT, el teléfono como micrófono, íconos propios para cada app, herramientas de taller,
proyectos, calculadora eléctrica RIC, respaldo, remoto, cámaras, casa, dictado y traductor.

## Probado
Arranca en máquina virtual (BIOS), el instalador funciona en disco de prueba y el sistema instalado arranca al HUD.
Falta probar en hardware real (UEFI con arranque seguro, NVIDIA): avisa en Issues si algo falla.
