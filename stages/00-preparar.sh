# Etapa 00 · preparar: revisa el equipo que compila y deja las carpetas listas.
log "Equipo que compila: $(. /etc/os-release; echo "$PRETTY_NAME") · $(nproc) núcleos · $(df -h "$WORK_DIR" | awk 'NR==2{print $4}') libres"
libre_gb=$(df -BG "$WORK_DIR" | awk 'NR==2{gsub("G","",$4);print $4}')
min=$([ "$EDITION" = full ] && echo 40 || echo 12)
[ "$libre_gb" -ge "$min" ] || die "Hacen falta ~${min} GB libres en $WORK_DIR (hay ${libre_gb} GB)."
mkdir -p "$CHROOT" "$IMAGE" "$OUT_DIR"
