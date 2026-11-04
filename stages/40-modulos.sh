# Etapa 40 · módulos según la edición (desktop, dev, robótica, taller, IA, juegos, navegador).
instalar_lista() { # nombre-lista
  local pk disp=() falta=()
  for pk in $(pkg_list "$1"); do
    if echo "apt-cache show $pk >/dev/null 2>&1" | in_chroot 2>/dev/null; then disp+=("$pk"); else falta+=("$pk"); fi
  done
  [ ${#falta[@]} -gt 0 ] && warn "$1: no disponibles en los repos configurados: ${falta[*]}"
  [ ${#disp[@]} -gt 0 ] && apt_install "${disp[@]}"
}
enabled WITH_DESKTOP  && { log "Escritorio de respaldo (KDE)"; instalar_lista desktop; instalar_lista navegador; }
enabled WITH_DEV      && { log "Desarrollo"; instalar_lista dev; }
enabled WITH_ROBOTICS && { log "Robótica (ROS 2 Jazzy)"; instalar_lista robotics; }
enabled WITH_MAKER    && { log "Taller (placas, CAD, impresión 3D)"; instalar_lista maker; }
enabled WITH_AI       && { log "IA (visión, audio; Ollama y modelos se bajan en el primer arranque)"; instalar_lista ai; }
if enabled WITH_GAMING; then
  log "Juegos y medios"
  echo 'dpkg --add-architecture i386 && apt-get update' | in_chroot
  echo 'echo steam steam/question select "I AGREE" | debconf-set-selections; echo steam steam/license note "" | debconf-set-selections' | in_chroot
  instalar_lista gaming
fi
true
