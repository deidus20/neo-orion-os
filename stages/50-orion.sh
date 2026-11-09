# Etapa 50 · Neo Orión: código, interfaz, servicios, catálogo, identidad y (opcional) capa personal.
D=/opt/orion
rm -rf "$CHROOT$D"; mkdir -p "$CHROOT$D" "$CHROOT/usr/share/orion" "$CHROOT/etc/orion"
# código Python (sin pruebas ni cachés)
for m in "$ROOT_DIR"/orion/orion_* "$ROOT_DIR"/orion/profiles "$ROOT_DIR"/orion/modulos; do
  [ -e "$m" ] && cp -a "$m" "$CHROOT$D/"
done
find "$CHROOT$D" -name __pycache__ -prune -exec rm -rf {} +
# overlay (servicios, scripts, catálogo) y la interfaz más reciente desde hud/
overlay_copy .
mkdir -p "$CHROOT/usr/share/orion/hud" "$CHROOT/usr/share/orion/instalador"
cp "$ROOT_DIR"/hud/index.html "$ROOT_DIR"/hud/marathon.js "$CHROOT/usr/share/orion/hud/"
cp "$ROOT_DIR"/hud/instalar.html "$ROOT_DIR"/hud/marathon.js "$CHROOT/usr/share/orion/instalador/"
[ -d "$ROOT_DIR/hud/lib" ] && cp -a "$ROOT_DIR/hud/lib" "$CHROOT/usr/share/orion/hud/"
for d in hud instalador; do cp -a "$ROOT_DIR/hud/fuentes" "$CHROOT/usr/share/orion/$d/"; done   # letras sin internet
cp "$ROOT_DIR/config/ia-catalogo.json" "$ROOT_DIR/config/modulos.json" "$ROOT_DIR/config/capas.json" "$CHROOT/usr/share/orion/"
cp "$ROOT_DIR/config/identidad.yaml" "$CHROOT/etc/orion/identidad.yaml"
cp -a "$ROOT_DIR/catalogo" "$CHROOT/usr/share/orion/"
printf '%s %s "%s" · edición %s · %s\n' "$DISTRO_NAME" "$DISTRO_VERSION" "$DISTRO_CODENAME" "$EDITION" "$(date +%F)" > "$CHROOT/etc/orion/version"
# capa personal: solo si se pide (CAPA=<nombre>, carpeta capas/<nombre>); la ISO limpia no la lleva
if [ -n "${CAPA:-}" ] && [ -d "$ROOT_DIR/capas/$CAPA" ]; then
  log "Capa personal $CAPA incluida (ISO privada)"
  mkdir -p "$CHROOT/usr/share/orion/capas"; cp -a "$ROOT_DIR/capas/$CAPA" "$CHROOT/usr/share/orion/capas/"
fi
# entorno Python propio (usa numpy del sistema) + MCP
# ORION_CA=/ruta/ca.crt: certificado extra si el equipo que compila sale a internet por un proxy con su propia CA
PIPCERT=""
if [ -n "${ORION_CA:-}" ] && [ -f "$ORION_CA" ]; then cp "$ORION_CA" "$CHROOT/tmp/orion-ca.crt"; PIPCERT="--cert /tmp/orion-ca.crt"; fi
cat <<IN | in_chroot
python3 -m venv --system-site-packages $D/venv
echo "$D" > \$($D/venv/bin/python3 -c 'import site;print(site.getsitepackages()[0])')/orion.pth
python3 -c "import sys; sys.path.insert(0,'$D'); import orion_core.legado" && echo "puente de Neo Orión: OK"
systemctl enable orion-primer-arranque.service
if command -v cage >/dev/null; then systemctl enable orion-sesion.service; systemctl disable getty@tty1.service 2>/dev/null || true; fi
systemctl set-default multi-user.target
rm -f /tmp/orion-ca.crt
IN
# nombre del sistema
sed -i "s/^PRETTY_NAME=.*/PRETTY_NAME=\"$DISTRO_NAME $DISTRO_VERSION ($DISTRO_CODENAME)\"/" "$CHROOT/usr/lib/os-release"
