#!/usr/bin/env bash
# Funciones compartidas por todas las etapas del build.
# shellcheck disable=SC2034

C_CYAN=$'\e[38;5;51m'; C_RED=$'\e[38;5;197m'; C_DIM=$'\e[2m'; C_OFF=$'\e[0m'

log()  { printf '%s[ORION]%s %s\n' "$C_CYAN" "$C_OFF" "$*"; }
warn() { printf '%s[ORION]%s %s\n' "$C_RED" "$C_OFF" "$*" >&2; }
die()  { warn "$*"; exit 1; }

CHROOT="${WORK_DIR}/chroot"
IMAGE="${WORK_DIR}/image"

# Monta los sistemas de archivos virtuales necesarios dentro del chroot.
chroot_mount() {
  for d in dev dev/pts proc sys run; do
    mkdir -p "$CHROOT/$d"
    mountpoint -q "$CHROOT/$d" && continue
    case "$d" in
      proc)    mount -t proc proc "$CHROOT/proc" ;;
      sys)     mount -t sysfs sysfs "$CHROOT/sys" ;;
      dev/pts) mount -t devpts devpts "$CHROOT/dev/pts" ;;
      run)     mount -t tmpfs tmpfs "$CHROOT/run" ;;
      dev)     mount --bind /dev "$CHROOT/dev" ;;
    esac
  done
  # DNS para el chroot
  rm -f "$CHROOT/etc/resolv.conf"
  cp -L /etc/resolv.conf "$CHROOT/etc/resolv.conf"
}

chroot_umount() {
  [ -d "$CHROOT" ] || return 0
  for d in run proc sys dev/pts dev; do
    mountpoint -q "$CHROOT/$d" && umount -lf "$CHROOT/$d" || true
  done
}

# Ejecuta un script bash dentro del chroot (lee de stdin).
in_chroot() {
  chroot "$CHROOT" /usr/bin/env -i \
    HOME=/root TERM="${TERM:-xterm}" LANG=C.UTF-8 \
    PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
    DEBIAN_FRONTEND=noninteractive \
    EDITION="$EDITION" DISTRO_NAME="$DISTRO_NAME" DISTRO_ID="$DISTRO_ID" \
    LIVE_USER="$LIVE_USER" \
    /bin/bash -euo pipefail -s
}

# apt-get install sin recomendados salvo que se pida, con reintentos.
apt_install() {
  printf 'apt-get install -y --no-install-recommends %s\n' "$*" | in_chroot
}
apt_install_rec() {
  printf 'apt-get install -y %s\n' "$*" | in_chroot
}

# Lee una lista de paquetes (ignora comentarios y líneas vacías).
pkg_list() {
  sed -e 's/#.*//' -e '/^[[:space:]]*$/d' "$ROOT_DIR/config/packages/$1.list" | tr '\n' ' '
}

# Copia el overlay de un módulo al rootfs respetando permisos.
overlay_copy() {
  local src="$ROOT_DIR/overlay/$1"
  [ -d "$src" ] || return 0
  cp -a "$src/." "$CHROOT/"
}

enabled() { [ "${!1:-no}" = "yes" ]; }
