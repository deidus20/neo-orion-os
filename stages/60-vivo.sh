# Etapa 60 · sesión en vivo (pendrive): usuario «orion» con sudo, sin clave, directo al instalador.
cat > "$CHROOT/etc/casper.conf" <<CONF
export USERNAME="$LIVE_USER"
export USERFULLNAME="Neo Orion"
export HOST="$HOSTNAME_DEFAULT"
export BUILD_SYSTEM="Ubuntu"
# FLAVOUR con valor: si queda vacío, casper renombra usuario y equipo con la primera palabra de .disk/info («neo»)
export FLAVOUR="orion"
CONF
# el instalador real particiona: el usuario del pendrive necesita sudo sin clave (solo en el pendrive;
# el instalador borra este archivo en el equipo instalado)
printf '%s ALL=(ALL) NOPASSWD: ALL\n' "$LIVE_USER" > "$CHROOT/etc/sudoers.d/orion-live"; chmod 440 "$CHROOT/etc/sudoers.d/orion-live"
# casper lee casper.conf desde el initrd: hay que regenerarlo para que el usuario del pendrive sea «orion»
echo "update-initramfs -u -k all" | in_chroot
# consola de rescate en tty2
echo 'systemctl enable getty@tty2.service' | in_chroot
# Calamares como instalador avanzado (particiones a mano) en la edición con escritorio
if enabled WITH_INSTALLER && enabled WITH_DESKTOP; then
  apt_install calamares || warn "calamares no disponible"
  mkdir -p "$CHROOT/etc/calamares"
  cp -a "$ROOT_DIR/config/calamares/." "$CHROOT/etc/calamares/" 2>/dev/null || true
fi
