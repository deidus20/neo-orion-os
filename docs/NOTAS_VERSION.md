# Neo Orión OS 0.3 · edición patata

**Todo Neo Orión corre en Rust.** Sistema por voz sobre Ubuntu 24.04 (GPL-3.0). Autor: Davis Reyes.

**Descarga directa (un solo archivo):** `neo-orion-0.3-patata-amd64.iso`
Revisa la suma: `sha256sum -c neo-orion-0.3-patata-amd64.iso.sha256`

## Cómo usarla
1. Grábala en un pendrive de 8 GB o más con balenaEtcher o Rufus (modo DD).
2. Arranca el PC desde el pendrive: aparece el instalador de Neo Orión.
3. Al primer arranque con internet baja la IA, el oído (Vosk o whisper.cpp), la voz (Piper) y los controladores.

¿Ya tienes Ubuntu 24.04? No necesitas la ISO:
```
curl -fsSL https://raw.githubusercontent.com/deidus20/neo-orion-os/main/instalar.sh | sudo bash
```

## Qué cambió
- **Un solo programa nativo** (`orion-nucleo`) hace todo: servidor del HUD, servidor MCP para la IA (43 herramientas,
  5 MB de RAM), oído y llave de voz, instalador, robots con su guardián, LiDAR, cámaras, CNC, navegador, juegos,
  proyectos, dictado a Word, traductor y primer arranque. Sin intérprete ni pausas de recolección de basura.
- **La ventana del HUD** (`orion-pantalla`, 360 KB) también es Rust con WebKit.
- Python queda solo para el CAD por código (build123d/CadQuery), como programa externo.
- Arreglos: la coma decimal («12,5 grados») ya no corta la orden del robot; la sala de juegos usa la gráfica real.

## Archivos
- `neo-orion-0.3-patata-amd64.iso` y su suma SHA-256
- `orion-nucleo-x86_64-linux` y `orion-pantalla-x86_64-linux`: los binarios sueltos (los usa `instalar.sh`)

## Probado
71 pruebas automáticas; cada módulo comparado con su versión anterior. Navegador probado con Chrome real y CAD con
build123d. Falta probar en hardware real (UEFI con arranque seguro, NVIDIA): avisa en Issues si algo falla.
