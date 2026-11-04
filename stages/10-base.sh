# Etapa 10 · base: Ubuntu mínimo con debootstrap (se reutiliza si ya existe).
if [ -f "$CHROOT/etc/os-release" ] && [ -x "$CHROOT/usr/bin/apt-get" ]; then
  log "Base ya existe en $CHROOT (usa --clean para empezar de cero)"
else
  log "debootstrap $UBUNTU_SUITE ($ARCH) desde $MIRROR"
  debootstrap --arch="$ARCH" --variant=minbase \
    --include=apt-utils,ca-certificates,gnupg,locales,systemd-sysv,dbus \
    "$UBUNTU_SUITE" "$CHROOT" "$MIRROR"
fi
chroot_mount
# fuentes en formato deb822 (main restricted universe multiverse + updates + security)
cat > "$CHROOT/etc/apt/sources.list.d/ubuntu.sources" <<SRC
Types: deb
URIs: $MIRROR
Suites: $UBUNTU_SUITE $UBUNTU_SUITE-updates $UBUNTU_SUITE-backports
Components: main restricted universe multiverse
Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg

Types: deb
URIs: http://security.ubuntu.com/ubuntu
Suites: $UBUNTU_SUITE-security
Components: main restricted universe multiverse
Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg
SRC
: > "$CHROOT/etc/apt/sources.list"
# que los servicios no arranquen dentro del chroot
printf '#!/bin/sh\nexit 101\n' > "$CHROOT/usr/sbin/policy-rc.d"; chmod 755 "$CHROOT/usr/sbin/policy-rc.d"
echo "$HOSTNAME_DEFAULT" > "$CHROOT/etc/hostname"
printf '127.0.0.1 localhost\n127.0.1.1 %s\n' "$HOSTNAME_DEFAULT" > "$CHROOT/etc/hosts"
cat <<IN | in_chroot
echo 'APT::Install-Recommends "0";' > /etc/apt/apt.conf.d/90-orion-sin-recomendados
echo 'Acquire::Retries "5";' > /etc/apt/apt.conf.d/90-orion-reintentos
apt-get update
apt-get -y dist-upgrade
for l in $LOCALE_MAIN $LOCALES_EXTRA; do sed -i "s/^# *\$l/\$l/" /etc/locale.gen; grep -q "^\$l" /etc/locale.gen || echo "\$l UTF-8" >> /etc/locale.gen; done
locale-gen
update-locale LANG=$LOCALE_MAIN
ln -sf /usr/share/zoneinfo/$TIMEZONE /etc/localtime; echo $TIMEZONE > /etc/timezone
printf 'XKBMODEL="pc105"\nXKBLAYOUT="$KEYBOARD_LAYOUT"\nXKBVARIANT=""\nXKBOPTIONS=""\nBACKSPACE="guess"\n' > /etc/default/keyboard
IN
