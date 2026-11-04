#!/usr/bin/env bash
# ============================================================
#  Neo Orión OS — orquestador del build de la ISO
#
#  Uso:
#    sudo ./build.sh                 # edición full, todas las etapas
#    sudo EDITION=lite ./build.sh    # edición liviana
#    sudo ./build.sh --from 40       # retoma desde la etapa 40
#    sudo ./build.sh --only 99       # solo regenera la ISO
#    sudo ./build.sh --clean         # borra work/ y empieza de cero
#
#  Requisitos del equipo que compila: Ubuntu 24.04 (o Debian 12+),
#  ~40 GB libres, conexión a internet y permisos de root.
# ============================================================
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export ROOT_DIR
# shellcheck source=config/distro.conf
source "$ROOT_DIR/config/distro.conf"

# Presets por edición (las variables explícitas del usuario mandan)
case "$EDITION" in
  full) ;;
  lite)
    WITH_ROBOTICS=no; WITH_AI=no; WITH_MAKER=no; WITH_GAMING=no ;;   # se instalan después con `orion install`
  patata)   # PC viejo: 2–4 GB, sin escritorio, HUD en quiosco liviano, IA mínima + nube
    WITH_DESKTOP=no; WITH_DEV=no; WITH_ROBOTICS=no; WITH_MAKER=no; WITH_GAMING=no; WITH_AI=no; WITH_KIOSK=yes ;;   # Ollama y modelos: primer arranque
  test)
    WITH_DESKTOP=no; WITH_DEV=no; WITH_ROBOTICS=no; WITH_MAKER=no
    WITH_AI=no; WITH_GAMING=no; WITH_INSTALLER=no; WITH_KIOSK=yes; SQUASHFS_COMP=zstd ;;
  *) echo "EDITION desconocida: $EDITION (full|lite|patata|test)"; exit 1 ;;
esac
WITH_KIOSK="${WITH_KIOSK:-no}"
export WITH_KIOSK WITH_DESKTOP WITH_DEV WITH_ROBOTICS WITH_MAKER WITH_AI WITH_GAMING WITH_INSTALLER \
       SECURE_BOOT SQUASHFS_COMP EDITION

# shellcheck source=lib/common.sh
source "$ROOT_DIR/lib/common.sh"

FROM=0; ONLY=""
while [ $# -gt 0 ]; do
  case "$1" in
    --from)  FROM="$2"; shift 2 ;;
    --only)  ONLY="$2"; shift 2 ;;
    --clean) log "Limpiando $WORK_DIR"; CHROOT="$WORK_DIR/chroot"; chroot_umount
             rm -rf "$WORK_DIR"; shift ;;
    -h|--help) sed -n '2,16p' "$0"; exit 0 ;;
    *) die "Opción desconocida: $1" ;;
  esac
done

[ "$(id -u)" -eq 0 ] || die "Ejecuta con sudo (debootstrap y chroot necesitan root)."
for t in debootstrap mksquashfs xorriso mkfs.vfat mcopy; do
  command -v "$t" >/dev/null || die "Falta '$t'. Instala: sudo apt install debootstrap squashfs-tools xorriso grub-pc-bin grub-efi-amd64-bin mtools dosfstools"
done

mkdir -p "$WORK_DIR" "$OUT_DIR"
trap chroot_umount EXIT
# si se retoma con --from, el chroot necesita /proc, /sys, /dev y DNS igual
[ -x "$CHROOT/usr/bin/apt-get" ] && chroot_mount

cat <<EOF
${C_CYAN}
   ███╗   ██╗███████╗ ██████╗      ██████╗ ██████╗ ██╗ ██████╗ ███╗   ██╗
   ████╗  ██║██╔════╝██╔═══██╗    ██╔═══██╗██╔══██╗██║██╔═══██╗████╗  ██║
   ██╔██╗ ██║█████╗  ██║   ██║    ██║   ██║██████╔╝██║██║   ██║██╔██╗ ██║
   ██║╚██╗██║██╔══╝  ██║   ██║    ██║   ██║██╔══██╗██║██║   ██║██║╚██╗██║
   ██║ ╚████║███████╗╚██████╔╝    ╚██████╔╝██║  ██║██║╚██████╔╝██║ ╚████║
   ╚═╝  ╚═══╝╚══════╝ ╚═════╝      ╚═════╝ ╚═╝  ╚═╝╚═╝ ╚═════╝ ╚═╝  ╚═══╝${C_OFF}
   ${DISTRO_NAME} ${DISTRO_VERSION} "${DISTRO_CODENAME}"  ·  edición: ${EDITION}
   desktop=${WITH_DESKTOP} dev=${WITH_DEV} robotics=${WITH_ROBOTICS} maker=${WITH_MAKER} ai=${WITH_AI} gaming=${WITH_GAMING} installer=${WITH_INSTALLER}
EOF

START=$(date +%s)
for stage in "$ROOT_DIR"/stages/[0-9][0-9]-*.sh; do
  num="$(basename "$stage" | cut -c1-2)"
  [ -n "$ONLY" ] && [ "$num" != "$ONLY" ] && continue
  [ "$((10#$num))" -lt "$((10#$FROM))" ] && continue
  log "──────── Etapa $(basename "$stage" .sh) ────────"
  # shellcheck source=/dev/null
  source "$stage"
done

log "Listo en $(( ($(date +%s) - START) / 60 )) min."
ls -lh "$OUT_DIR"/*.iso 2>/dev/null || true
