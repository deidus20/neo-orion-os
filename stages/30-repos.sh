# Etapa 30 · repositorios oficiales de terceros. Si uno no responde, se avisa y se sigue
# (ORION_ESTRICTO=yes para que falle). Lo que no entra aquí se instala después con `orion install`.
repo_fallo() { if [ "${ORION_ESTRICTO:-no}" = yes ]; then die "$1"; else warn "$1 · se omite"; fi; }
llave() { # url destino
  in_chroot <<IN || return 1
mkdir -p "$(dirname "$2")"
curl -fsSL -m 30 "$1" -o /tmp/llave
if head -c 40 /tmp/llave | grep -q "BEGIN PGP"; then gpg --dearmor < /tmp/llave > "$2"; else cp /tmp/llave "$2"; fi
IN
}
NUEVOS=0
if enabled WITH_DESKTOP; then
  if llave https://dl.google.com/linux/linux_signing_key.pub /usr/share/keyrings/google-chrome.gpg; then
    cp "$ROOT_DIR/config/repos/google-chrome.sources" "$CHROOT/etc/apt/sources.list.d/"; NUEVOS=1
  else repo_fallo "No se pudo bajar la llave de Google Chrome"; fi
  if llave https://packages.mozilla.org/apt/repo-signing-key.gpg /etc/apt/keyrings/packages.mozilla.org.asc; then
    cp "$ROOT_DIR/config/repos/mozilla.sources" "$CHROOT/etc/apt/sources.list.d/"
    printf 'Package: *\nPin: origin packages.mozilla.org\nPin-Priority: 1000\n' > "$CHROOT/etc/apt/preferences.d/mozilla"; NUEVOS=1
  else repo_fallo "No se pudo bajar la llave de Mozilla"; fi
fi
if enabled WITH_ROBOTICS; then
  if llave https://raw.githubusercontent.com/ros/rosdistro/master/ros.key /usr/share/keyrings/ros-archive-keyring.gpg; then
    printf 'Types: deb\nURIs: http://packages.ros.org/ros2/ubuntu\nSuites: %s\nComponents: main\nSigned-By: /usr/share/keyrings/ros-archive-keyring.gpg\n' "$UBUNTU_SUITE" \
      > "$CHROOT/etc/apt/sources.list.d/ros2.sources"; NUEVOS=1
  else repo_fallo "No se pudo bajar la llave de ROS 2"; fi
fi
if [ "$NUEVOS" = 1 ]; then
  echo 'apt-get update' | in_chroot || { warn "apt update con repos nuevos falló; se quitan"; rm -f "$CHROOT"/etc/apt/sources.list.d/{google-chrome,mozilla,ros2}.sources; echo 'apt-get update' | in_chroot; }
fi
