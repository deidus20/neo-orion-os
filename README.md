# Neo Orión OS

**Un computador que se maneja hablando.** Neo Orión es un sistema basado en Ubuntu 24.04 en el que
no hay escritorio ni íconos: dices lo que necesitas y la IA lo hace por detrás. Solo aparece en
pantalla lo que tienes que ver: un robot en 3D, una carta Gantt, un cálculo eléctrico, una cámara.

Autor: **Davis Reyes** · Licencia: **GPL-3.0** (software libre: úsalo, cámbialo y compártelo con la misma licencia).

## Qué trae

- **Voz siempre activa:** oído local (Vosk o Whisper) y voz (Piper, Kokoro o eSpeak). Nada sale del equipo si no quieres.
- **IA local que calza con tu PC:** detecta memoria, gráfica y procesador y recomienda el modelo que corre fluido (Ollama). También puede usar IAs en la nube si tú lo eliges.
- **Llave de voz:** una frase tuya (1 o 3 grabaciones) desbloquea el equipo; también clave normal, o ambas.
- **Formas de iniciar:** frases y palmadas que despiertan el PC, lo saludan y abren lo que quieras («Buenos días…» + dos palmadas). Todo editable.
- **Controladores al día:** reconoce la gráfica (NVIDIA, AMD, Intel) y el procesador e instala lo que corresponde.
- **Herramientas:** robots (ROS 2, LiDAR), taller (ESP32, KiCad, FreeCAD, CNC), proyectos (Gantt, presupuesto, RACI), calculadora eléctrica RIC, respaldo, control remoto, cámaras, casa inteligente, dictado de informes, traductor, cuadernos de estudio.
- **Guardián:** lo físico (motores, máquinas) y lo irreversible siempre esperan tu aprobación.
- **Modo patata:** funciona en PC viejos (2 GB de RAM) sin perder la identidad visual.

La guía de uso, con capturas de cada pantalla, está en
[`docs/Guia_de_uso_Neo_Orion.pdf`](docs/Guia_de_uso_Neo_Orion.pdf).
La arquitectura técnica está en [`ARQUITECTURA.md`](ARQUITECTURA.md).

## Cómo instalarlo

### Opción A · En un PC que ya tiene Ubuntu 24.04 (lo más fácil)

Abre una terminal y pega:

```bash
curl -fsSL https://raw.githubusercontent.com/deidus20/neo-orion-os/main/instalar.sh | sudo bash
```

Tu escritorio queda igual: Neo Orión aparece en el menú y también se abre con `orion hud`.
Si prefieres que el PC entre **directo** a Neo Orión al encender (sin escritorio):

```bash
curl -fsSL https://raw.githubusercontent.com/deidus20/neo-orion-os/main/instalar.sh | sudo bash -s -- --pantalla
```

Otras opciones: `--sin-ia` (no baja modelos ahora), `--quitar` (lo desinstala y conserva tu memoria).

### Opción B · Desde cero con la ISO

1. Baja la ISO desde [Releases](https://github.com/deidus20/neo-orion-os/releases) y revisa su suma `sha256`.
2. Grábala en un pendrive (8 GB o más) con [balenaEtcher](https://etcher.balena.io/) o Rufus (modo DD).
3. Arranca el PC desde el pendrive: aparece el instalador de Neo Orión, que te pregunta idioma, disco,
   IA, voz, color y tu cuenta. Puede borrar el disco o instalarse **junto** a Windows.
4. Al primer arranque con internet baja solo la IA, el oído, la voz y los controladores.

### Opción C · Compilar la ISO tú mismo

En Ubuntu 24.04, con unos 40 GB libres:

```bash
git clone https://github.com/deidus20/neo-orion-os.git && cd neo-orion-os
sudo EDITION=lite ./build.sh        # full | lite | patata | test
```

La ISO queda en `out/`. Ver `config/distro.conf` para cambiar nombre, región y qué se incluye.

## Estructura

| Carpeta | Qué hay |
|---|---|
| `orion/` | el código en Python: servidor local, MCP, voz, memoria, instalador y cada herramienta (`orion_*`) con sus pruebas en `orion/tests` |
| `hud/` | la interfaz (HUD) y el instalador gráfico |
| `stages/`, `build.sh`, `lib/` | la compilación de la ISO por etapas |
| `config/` | paquetes, catálogo de IA, módulos, identidad |
| `overlay/` | archivos que se copian tal cual al sistema (servicios, comandos) |
| `nodos/` | programas para Raspberry/ESP32 (por ejemplo, el nodo que despierta el PC) |
| `docs/` | la guía de uso en PDF y las notas de cada versión |
| `instalar.sh` | instalación directa sobre Ubuntu |

## Pruebas

```bash
cd orion && for t in tests/test_*.py; do python3 -m pytest -q "$t" 2>/dev/null || python3 "$t"; done
```

## Licencia

Copyright (C) 2026 Davis Reyes.

Este programa es software libre: puedes redistribuirlo y/o modificarlo bajo los términos de la
Licencia Pública General GNU (GPL) versión 3, publicada por la Free Software Foundation.
Se distribuye con la esperanza de que sea útil, pero SIN NINGUNA GARANTÍA. Ver [`LICENSE`](LICENSE).

Los programas de terceros que instala (Ubuntu, Ollama, modelos de IA, Piper, etc.) mantienen sus propias licencias;
el instalador muestra la de cada uno antes de elegirlo.
