# Lenguaje visual de Neo Orión (reglas para toda interfaz que cree la IA)

Referencia: O.R.C.A. (danneuenhaus) y la "Graphic Realism" de Marathon (Bungie).
La IA lee este archivo antes de generar cualquier panel, vista de lentes, app o pantalla de robot.

## Principio
Nada es fijo en pantalla. Mientras escucha: fondo vacío + indicador de voz mínimo.
Cada interfaz aparece solo cuando se pide y desaparece cuando ya no sirve.

## Color
| token | valor | uso |
|---|---|---|
| fondo | #101113 + grilla de puntos #25272b cada 22 px | siempre |
| panel | #17181b, borde #2a2c30, esquinas cortadas a 45° | contenedores |
| lima | #cdf564 | dato activo, líneas de radar, botón principal |
| rojo | #ff4a2a | alerta, personas en radar, cambios, bloqueos |
| blanco | #e6e8ea | obstáculos, texto principal |
| gris | #8b8e94 | etiquetas secundarias |

## Tipografía
- Etiquetas y títulos: pixel (Silkscreen), MAYÚSCULAS, espaciado amplio.
- Números y datos: monoespaciada (JetBrains Mono). **Los datos críticos siempre legibles**
  (ángulos, tensiones, distancias): la estética nunca tapa un número.

## Componentes
- **Radar arco**: círculos tangentes a la base + guías en V a ±45° y 0°, regla inferior con 4 marcas.
- **Radar de esquina**: círculo con barrido cónico, campo de visión punteado.
  Rojo = personas · lima = aliados/robots · blanco = obstáculos.
- **Alineación**: filas `ALGN 01…` con barra que se llena por pasos (`steps()`), luego bloque lima con ✓ pixelado.
- **Acceso**: 8 bloques tipo tablero que se encienden; blanco → lima al validar.
- **Etiqueta de objetivo**: recuadro lima + rótulo pegado (`ESTRUCTURA · 248 m`).
- **Mira**: cruces `+` en las 4 esquinas del marco.
- **Animación**: aparición en 4–6 pasos (scaleY + brillo), nunca transiciones suaves largas.
