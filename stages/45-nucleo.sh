# Etapa 45 · núcleo nativo en Rust (orion-nucleo + orion-pantalla): se compila en el equipo que arma la ISO y se
# copia al sistema. orion-pantalla necesita libwebkit2gtk-4.1-dev para compilar (la ISO trae solo la librería).
# Usa el binario ya compilado (nucleo/target/release) si existe; si no, compila con cargo. Sin Rust en el equipo,
# la ISO funciona igual con los servidores Python de respaldo (y se avisa).
NUC="$ROOT_DIR/nucleo"
BIN="$NUC/target/release/orion-nucleo"
PAN="$NUC/target/release/orion-pantalla"
if { [ ! -x "$BIN" ] || [ ! -x "$PAN" ]; } && command -v cargo >/dev/null 2>&1; then
  log "Compilando el núcleo Rust (release)"
  ( cd "$NUC" && cargo build --release --locked ) || warn "No compiló el núcleo Rust: se usará el respaldo Python"
fi
if [ -x "$BIN" ]; then
  install -Dm755 "$BIN" "$CHROOT/usr/local/bin/orion-nucleo"
  log "Núcleo Rust: $("$BIN" --version) · $(du -h "$BIN" | cut -f1)"
  if [ -x "$PAN" ]; then
    install -Dm755 "$PAN" "$CHROOT/usr/local/bin/orion-pantalla"
    log "Pantalla Rust: $(du -h "$PAN" | cut -f1)"
  else
    warn "Sin orion-pantalla (falta libwebkit2gtk-4.1-dev al compilar): la sesión abrirá el HUD en Chrome/Firefox en modo quiosco"
  fi
else
  warn "Sin núcleo Rust (falta cargo: https://rustup.rs): la ISO usará el servidor Python de respaldo"
fi
