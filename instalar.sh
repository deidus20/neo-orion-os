#!/usr/bin/env bash
# =====================================================================================
#  Neo Orión OS · instalación directa en un PC que ya tiene Ubuntu 24.04
#
#  Una línea (versión pública):
#    curl -fsSL https://raw.githubusercontent.com/deidus20/neo-orion-os/main/instalar.sh | sudo bash
#
#  Desde una copia del repositorio (sirve también para el privado):
#    sudo ./instalar.sh [opciones]
#
#  Opciones:
#    --pantalla     al encender, el PC entra directo al HUD de Neo Orión (modo quiosco en tty1).
#                   Sin esta opción, Neo Orión se instala junto a tu escritorio y lo abres con «orion hud».
#    --sin-ia       no instala Ollama ni modelos ahora (se puede hacer después con «sudo orion-primer-arranque»)
#    --capa NOMBRE  aplica la capa personal capas/NOMBRE (solo existe en el repo privado)
#    --repo URL     de dónde bajar Neo Orión (por defecto, el repo público)
#    --rama RAMA    rama a usar (por defecto main)
#
#  Se puede correr de nuevo cuando quieras: actualiza sin borrar tu memoria ni tu configuración.
#  Para quitarlo:  sudo ./instalar.sh --quitar
# =====================================================================================
set -euo pipefail

REPO="https://github.com/deidus20/neo-orion-os.git"
RAMA="main"
PANTALLA=no; IA=si; CAPA=""; QUITAR=no
while [ $# -gt 0 ]; do
  case "$1" in
    --pantalla) PANTALLA=si ;;
    --sin-ia) IA=no ;;
    --capa) CAPA="${2:?falta el nombre de la capa}"; shift ;;
    --repo) REPO="${2:?falta la URL}"; shift ;;
    --rama) RAMA="${2:?falta la rama}"; shift ;;
    --quitar) QUITAR=si ;;
    -h|--help) sed -n '2,24p' "$0" 2>/dev/null || true; exit 0 ;;
    *) echo "Opción desconocida: $1 (usa --help)"; exit 2 ;;
  esac
  shift
done

R=$'\e[38;5;197m'; G=$'\e[2m'; N=$'\e[0m'
paso() { printf '\n%s▌ %s%s\n' "$R" "$*" "$N"; }
info() { printf '%s  %s%s\n' "$G" "$*" "$N"; }
muere() { printf '%s✖ %s%s\n' "$R" "$*" "$N" >&2; exit 1; }

[ "$(id -u)" -eq 0 ] || muere "Hay que correrlo con sudo:  sudo bash instalar.sh"
USUARIO="${SUDO_USER:-}"
[ -n "$USUARIO" ] && [ "$USUARIO" != root ] || muere "Córrelo con sudo desde tu cuenta normal (no como root directo)."
CASA="$(getent passwd "$USUARIO" | cut -d: -f6)"

# ------------------------------------------------------------------------------- quitar
if [ "$QUITAR" = si ]; then
  paso "Quitando Neo Orión (tu memoria en $CASA/Orion y $CASA/.orion se conserva)"
  systemctl disable --now orion-sesion.service orion-primer-arranque.service 2>/dev/null || true
  systemctl enable getty@tty1.service 2>/dev/null || true
  rm -rf /opt/orion /usr/share/orion /etc/systemd/system/orion-sesion.service* /etc/systemd/system/orion-primer-arranque.service \
         /etc/systemd/user/orion-* /usr/local/bin/orion /usr/local/bin/orion-fit /usr/local/bin/orion-pantalla \
         /usr/local/bin/orion-primer-arranque /usr/local/bin/orion-sesion /usr/local/bin/orion-nucleo /usr/share/applications/neo-orion.desktop
  systemctl daemon-reload 2>/dev/null || true
  info "Listo. /etc/orion quedó por si vuelves a instalar; bórralo a mano si no lo quieres."
  exit 0
fi

# ------------------------------------------------------------------------------- 1 · revisar
paso "1/7 · Revisando el equipo"
. /etc/os-release
info "sistema: ${PRETTY_NAME:-desconocido} · arquitectura: $(uname -m) · usuario: $USUARIO"
[ "$(uname -m)" = x86_64 ] || muere "Neo Orión por ahora es para PC de 64 bits (x86_64)."
if [ "${VERSION_ID:-}" != "24.04" ]; then
  info "Aviso: está probado en Ubuntu 24.04. En ${PRETTY_NAME:-este sistema} puede fallar algún paquete."
fi
LIBRE=$(df -BG --output=avail / | tail -1 | tr -dc 0-9)
[ "${LIBRE:-0}" -ge 8 ] || muere "Hacen falta al menos 8 GB libres (hay ${LIBRE} GB)."

# ------------------------------------------------------------------------------- 2 · código
paso "2/7 · Bajando Neo Orión"
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq git ca-certificates curl >/dev/null
AQUI="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" 2>/dev/null && pwd || echo /nada)"
if [ -d "$AQUI/orion/orion_core" ] && [ -d "$AQUI/hud" ]; then
  SRC="$AQUI"; info "usando esta copia: $SRC"
else
  SRC=/opt/orion-src
  if [ -d "$SRC/.git" ]; then git -C "$SRC" fetch -q --depth 1 origin "$RAMA" && git -C "$SRC" reset -q --hard FETCH_HEAD
  else rm -rf "$SRC"; git clone -q --depth 1 -b "$RAMA" "$REPO" "$SRC"; fi
  info "desde $REPO ($RAMA) → $SRC"
fi
lista() { sed -e 's/#.*//' -e '/^[[:space:]]*$/d' "$SRC/config/packages/$1.list" | tr '\n' ' '; }

# ------------------------------------------------------------------------------- 3 · paquetes
paso "3/7 · Instalando lo que necesita (puede tardar unos minutos)"
PAQ="$(lista herramientas) $(lista kiosko) python3 python3-venv python3-numpy python3-gi gir1.2-gtk-3.0 alsa-utils pciutils curl zram-tools"
# se instalan uno por uno los que fallen, para que un paquete ausente no frene todo
if ! apt-get install -y -qq --no-install-recommends $PAQ >/dev/null 2>&1; then
  for p in $PAQ; do apt-get install -y -qq --no-install-recommends "$p" >/dev/null 2>&1 || info "no se pudo instalar: $p (se sigue)"; done
fi

# ------------------------------------------------------------------------------- 4 · archivos
paso "4/7 · Copiando Neo Orión al sistema"
D=/opt/orion
mkdir -p "$D" /usr/share/orion/hud /usr/share/orion/instalador /etc/orion
for m in "$SRC"/orion/orion_* "$SRC"/orion/profiles "$SRC"/orion/modulos; do
  [ -e "$m" ] && { rm -rf "$D/$(basename "$m")"; cp -a "$m" "$D/"; }
done
find "$D" -name __pycache__ -prune -exec rm -rf {} +
# scripts y servicios (sin pisar la configuración que ya tengas en /etc/orion)
cp -a "$SRC/overlay/usr/." /usr/
cp -a "$SRC/overlay/etc/systemd/." /etc/systemd/
for f in "$SRC"/overlay/etc/orion/*; do [ -e "/etc/orion/$(basename "$f")" ] || cp -a "$f" /etc/orion/; done
[ -f /etc/orion/identidad.yaml ] || cp "$SRC/config/identidad.yaml" /etc/orion/identidad.yaml
cp "$SRC"/hud/index.html "$SRC"/hud/marathon.js /usr/share/orion/hud/
cp "$SRC"/hud/instalar.html "$SRC"/hud/marathon.js /usr/share/orion/instalador/
[ -d "$SRC/hud/lib" ] && cp -a "$SRC/hud/lib" /usr/share/orion/hud/
for d in hud instalador; do cp -a "$SRC/hud/fuentes" "/usr/share/orion/$d/"; done
cp "$SRC/config/ia-catalogo.json" "$SRC/config/modulos.json" "$SRC/config/capas.json" /usr/share/orion/
cp -a "$SRC/catalogo" /usr/share/orion/
chmod +x /usr/local/bin/orion* 2>/dev/null || true
printf 'Neo Orión OS · instalado sobre %s · %s\n' "${PRETTY_NAME:-Linux}" "$(date +%F)" > /etc/orion/version

# entorno Python propio
python3 -m venv --system-site-packages "$D/venv"
"$D/venv/bin/pip" install -q --upgrade pip >/dev/null
"$D/venv/bin/pip" install -q mcp >/dev/null || info "mcp no se pudo instalar ahora (se reintenta después)"
echo "$D" > "$("$D/venv/bin/python3" -c 'import site;print(site.getsitepackages()[0])')/orion.pth"
"$D/venv/bin/python3" -c "import orion_core.servidor" || muere "El código de Neo Orión no carga; revisa el mensaje de arriba."

# núcleo nativo en Rust (servidor del HUD): el ya compilado de la versión, o se compila si hay Rust
NUCLEO_URL="https://github.com/deidus20/neo-orion-os/releases/latest/download/orion-nucleo-x86_64-linux"
if [ -x "$SRC/nucleo/target/release/orion-nucleo" ]; then
  install -m755 "$SRC/nucleo/target/release/orion-nucleo" /usr/local/bin/orion-nucleo
elif curl -fsSL "$NUCLEO_URL" -o /tmp/orion-nucleo 2>/dev/null && chmod +x /tmp/orion-nucleo && /tmp/orion-nucleo --version >/dev/null 2>&1; then
  install -m755 /tmp/orion-nucleo /usr/local/bin/orion-nucleo
elif command -v cargo >/dev/null 2>&1; then
  ( cd "$SRC/nucleo" && cargo build --release --locked -q ) && install -m755 "$SRC/nucleo/target/release/orion-nucleo" /usr/local/bin/orion-nucleo
fi
if [ -x /usr/local/bin/orion-nucleo ]; then info "núcleo Rust: $(/usr/local/bin/orion-nucleo --version)"
else info "sin núcleo Rust por ahora: se usa el servidor Python de respaldo"; fi

# ------------------------------------------------------------------------------- 5 · tu cuenta
paso "5/7 · Configurando tu cuenta ($USUARIO)"
echo "$USUARIO" > /etc/orion/usuario
mkdir -p /etc/systemd/system/orion-sesion.service.d
printf '[Service]\nUser=%s\n' "$USUARIO" > /etc/systemd/system/orion-sesion.service.d/usuario.conf
for g in audio video input render dialout plugdev; do getent group "$g" >/dev/null && usermod -aG "$g" "$USUARIO"; done
sudo -u "$USUARIO" mkdir -p "$CASA/Orion" "$CASA/.orion"
[ -f "$CASA/.orion/acento" ] || echo "#ff2438" | sudo -u "$USUARIO" tee "$CASA/.orion/acento" >/dev/null
# controladores de este equipo (gráfica y procesador)
PYTHONPATH=$D "$D/venv/bin/python3" -c "
import json; from orion_core.controladores import detectar
open('/etc/orion/controladores.json','w').write(json.dumps(detectar(), ensure_ascii=False, indent=2))" 2>/dev/null \
  && info "gráfica y procesador detectados → /etc/orion/controladores.json" || info "no se pudo detectar la gráfica (se sigue)"
# IA recomendada para este equipo
if [ ! -f /etc/orion/instalacion.json ]; then
  MODELO=$(python3 /usr/local/bin/orion-fit --json 2>/dev/null | python3 -c "import json,sys;print(json.load(sys.stdin)['recommendation']['best']['tag'])" 2>/dev/null || echo "gemma3:1b")
  python3 - "$USUARIO" "$MODELO" <<'PY'
import json, sys
u, m = sys.argv[1], sys.argv[2]
json.dump({"modo": "directo", "ia": {"locales": [m]}, "oido": "vosk", "voz": "espeak-ng", "capa": "base",
           "cuenta": {"usuario": u, "frase": "", "desbloqueo": "clave"}},
          open("/etc/orion/instalacion.json", "w"), ensure_ascii=False, indent=2)
PY
  info "IA local recomendada para este equipo: $MODELO"
fi
# capa personal (solo repo privado)
if [ -n "$CAPA" ]; then
  if [ -f "$SRC/capas/$CAPA/aplicar.sh" ]; then
    mkdir -p /usr/share/orion/capas; rm -rf "/usr/share/orion/capas/$CAPA"; cp -a "$SRC/capas/$CAPA" /usr/share/orion/capas/
    sudo -u "$USUARIO" env HOME="$CASA" bash "/usr/share/orion/capas/$CAPA/aplicar.sh" || info "la capa $CAPA dio un aviso"
  else
    info "No existe capas/$CAPA en esta copia (las capas personales solo están en el repo privado)."
  fi
fi
chown -R "$USUARIO:$USUARIO" "$CASA/Orion" "$CASA/.orion"

# acceso desde el escritorio
cat > /usr/share/applications/neo-orion.desktop <<'DSK'
[Desktop Entry]
Type=Application
Name=Neo Orión
Comment=Interfaz por voz de Neo Orión
Exec=orion hud
Terminal=false
Categories=Utility;
DSK

# ------------------------------------------------------------------------------- 6 · arranque
paso "6/7 · Servicios"
systemctl daemon-reload 2>/dev/null || true
systemctl enable orion-primer-arranque.service >/dev/null 2>&1 || true
if [ "$PANTALLA" = si ]; then
  systemctl enable orion-sesion.service >/dev/null
  systemctl disable getty@tty1.service >/dev/null 2>&1 || true
  info "Al reiniciar entra directo al HUD (para volver al escritorio: sudo systemctl disable orion-sesion && sudo systemctl set-default graphical.target)"
  systemctl set-default multi-user.target >/dev/null
else
  info "Tu escritorio queda igual. Abre Neo Orión desde el menú o con:  orion hud"
fi

# ------------------------------------------------------------------------------- 7 · IA
paso "7/7 · IA local, oído y voz"
if [ "$IA" = si ]; then
  /usr/local/bin/orion-primer-arranque || info "Algo quedó pendiente (¿sin internet?). Se reintenta solo al reiniciar."
else
  info "Saltado (--sin-ia). Se hará solo en el próximo arranque, o ahora con:  sudo orion-primer-arranque"
fi

printf '\n%s▌ Neo Orión quedó instalado.%s\n' "$R" "$N"
info "Cierra sesión y vuelve a entrar (o reinicia) para que tomen efecto los permisos de micrófono y cámara."
