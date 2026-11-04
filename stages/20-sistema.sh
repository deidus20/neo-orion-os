# Etapa 20 · sistema: núcleo, firmware, arranque, red, sonido y lo que usa el instalador.
echo "dpkg --configure -a || true; apt-get update; apt-get -f install -y || true" | in_chroot   # por si se retoma (la limpieza borra las listas) o algo quedó a medias
EXTRA="gdisk parted dosfstools e2fsprogs squashfs-tools alsa-utils pciutils curl python3 python3-venv python3-numpy libpam-systemd"
apt_install $(pkg_list base) $(pkg_list herramientas) $EXTRA
if enabled WITH_KIOSK || ! enabled WITH_DESKTOP; then
  log "Quiosco liviano (cage + orion-pantalla) para el HUD y el instalador"
  apt_install $(pkg_list kiosko) mesa-utils libgl1-mesa-dri libegl-mesa0 fonts-dejavu-core
fi
# zram en equipos con poca memoria (también en full: no molesta)
printf 'ALGO=zstd\nPERCENT=50\nPRIORITY=100\n' > "$CHROOT/etc/default/zramswap" 2>/dev/null || true
