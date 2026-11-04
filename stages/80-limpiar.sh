# Etapa 80 · limpiar: lo que no debe ir en la imagen.
cat <<'IN' | in_chroot
apt-get -y autoremove --purge
apt-get clean
rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/* /root/.cache /var/cache/debconf/*-old
truncate -s 0 /etc/machine-id; rm -f /var/lib/dbus/machine-id
find /var/log -type f -exec truncate -s 0 {} +
IN
rm -f "$CHROOT/usr/sbin/policy-rc.d"
rm -f "$CHROOT/etc/resolv.conf"; ln -s ../run/systemd/resolve/stub-resolv.conf "$CHROOT/etc/resolv.conf"
chroot "$CHROOT" dpkg-query -W --showformat='${Package} ${Version}\n' > "$WORK_DIR/filesystem.manifest"
chroot_umount
