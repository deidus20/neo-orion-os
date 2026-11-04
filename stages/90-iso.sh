# Etapa 90 · ISO híbrida (pendrive o DVD) que arranca en UEFI con arranque seguro y en BIOS.
rm -rf "$IMAGE"; mkdir -p "$IMAGE"/{casper,boot/grub,EFI/boot,.disk,isolinux}
K=$(ls "$CHROOT"/boot/vmlinuz-* | sort -V | tail -1); I=$(ls "$CHROOT"/boot/initrd.img-* | sort -V | tail -1)
[ -n "$K" ] && [ -n "$I" ] || die "No hay núcleo en el chroot (¿se instaló linux-generic?)"
cp "$K" "$IMAGE/casper/vmlinuz"; cp "$I" "$IMAGE/casper/initrd"
cp "$WORK_DIR/filesystem.manifest" "$IMAGE/casper/"
printf 'casper\ncalamares\n' > "$IMAGE/casper/filesystem.manifest-remove"
log "Comprimiendo el sistema ($SQUASHFS_COMP)…"
mksquashfs "$CHROOT" "$IMAGE/casper/filesystem.squashfs" -noappend -comp "$SQUASHFS_COMP" \
  $([ "$SQUASHFS_COMP" = xz ] && echo "-Xbcj x86 -b 1M") -wildcards -e 'proc/*' 'sys/*' 'dev/*' 'run/*' 'tmp/*' 
printf '%s' "$(du -sx --block-size=1 "$CHROOT" | cut -f1)" > "$IMAGE/casper/filesystem.size"
echo "$DISTRO_NAME $DISTRO_VERSION \"$DISTRO_CODENAME\" - $EDITION $ARCH ($(date +%Y%m%d))" > "$IMAGE/.disk/info"
touch "$IMAGE/ubuntu"   # marcador que busca el grub firmado

# menú de arranque
cp /usr/share/grub/unicode.pf2 "$IMAGE/boot/grub/font.pf2" 2>/dev/null || cp "$CHROOT/usr/share/grub/unicode.pf2" "$IMAGE/boot/grub/font.pf2" 2>/dev/null || true
FONDO=""
[ -f "$ROOT_DIR/config/arranque/fondo.png" ] && cp "$ROOT_DIR/config/arranque/fondo.png" "$IMAGE/boot/grub/" && FONDO=1
cat > "$IMAGE/boot/grub/grub.cfg" <<GRUB
search --no-floppy --set=root --file /ubuntu
insmod all_video
insmod gfxterm
insmod gfxterm_background
insmod png
insmod font
# el modo gráfico necesita una letra cargada; si no está, el menú queda en texto (pero visible)
if loadfont /boot/grub/font.pf2 ; then
  set gfxmode=1024x768,auto
  terminal_output gfxterm
  $( [ -n "$FONDO" ] && echo 'background_image /boot/grub/fondo.png' )
fi
set color_normal=light-gray/black
set color_highlight=black/red
set menu_color_normal=light-gray/black
set menu_color_highlight=black/light-red
set default=0
set timeout=8
set timeout_style=menu

menuentry "Neo Orión · probar o instalar" {
  linux /casper/vmlinuz boot=casper quiet splash loglevel=3 console=tty2 vt.global_cursor_default=0 ---
  initrd /casper/initrd
}
menuentry "Neo Orión · modo patata (equipo lento o con poca RAM)" {
  linux /casper/vmlinuz boot=casper quiet splash loglevel=3 console=tty2 vt.global_cursor_default=0 orion.patata=1 ---
  initrd /casper/initrd
}
menuentry "Neo Orión · modo seguro (gráficos básicos)" {
  linux /casper/vmlinuz boot=casper nomodeset ---
  initrd /casper/initrd
}
menuentry "Neo Orión · cargar todo en la RAM (sacar el pendrive después)" {
  linux /casper/vmlinuz boot=casper toram quiet splash loglevel=3 console=tty2 ---
  initrd /casper/initrd
}
if [ "\$grub_platform" = "efi" ]; then
menuentry "Configuración del firmware (BIOS/UEFI)" { fwsetup }
fi
GRUB
cp "$IMAGE/boot/grub/grub.cfg" "$IMAGE/isolinux/grub.cfg"

# UEFI: shim firmado + grub firmado (arranque seguro) en una partición EFI dentro de la ISO
SHIM="$CHROOT/usr/lib/shim/shimx64.efi.signed.latest"; [ -f "$SHIM" ] || SHIM="$CHROOT/usr/lib/shim/shimx64.efi.signed"
GRUBEFI="$CHROOT/usr/lib/grub/x86_64-efi-signed/gcdx64.efi.signed"
MM="$CHROOT/usr/lib/shim/mmx64.efi"
if [ "$SECURE_BOOT" = yes ] && [ -f "$SHIM" ] && [ -f "$GRUBEFI" ]; then
  cp "$SHIM" "$IMAGE/EFI/boot/bootx64.efi"; cp "$GRUBEFI" "$IMAGE/EFI/boot/grubx64.efi"; [ -f "$MM" ] && cp "$MM" "$IMAGE/EFI/boot/mmx64.efi"
  log "UEFI con arranque seguro (shim + grub firmados por Canonical)"
else
  warn "Sin shim firmado: UEFI sin arranque seguro"
  grub-mkstandalone --format=x86_64-efi --output="$IMAGE/EFI/boot/bootx64.efi" --locales="" --fonts="" \
    "boot/grub/grub.cfg=$IMAGE/isolinux/grub.cfg"
fi
EFIIMG="$WORK_DIR/efiboot.img"; rm -f "$EFIIMG"
mkfs.vfat -C -n NEO_EFI "$EFIIMG" 8192 >/dev/null
mmd -i "$EFIIMG" ::/EFI ::/EFI/boot ::/boot ::/boot/grub
mcopy -i "$EFIIMG" "$IMAGE"/EFI/boot/*.efi ::/EFI/boot/
mcopy -i "$EFIIMG" "$IMAGE/boot/grub/grub.cfg" ::/boot/grub/

# BIOS: grub El Torito (CD) + MBR híbrido (pendrive)
grub-mkstandalone --format=i386-pc --output="$WORK_DIR/core.img" \
  --install-modules="linux16 linux normal iso9660 biosdisk memdisk search tar ls all_video gfxterm gfxterm_background png font configfile echo test" \
  --modules="linux16 linux normal iso9660 biosdisk search" --locales="" --fonts="" \
  "boot/grub/grub.cfg=$IMAGE/isolinux/grub.cfg"
cat /usr/lib/grub/i386-pc/cdboot.img "$WORK_DIR/core.img" > "$IMAGE/isolinux/bios.img"

(cd "$IMAGE" && find . -type f ! -name md5sum.txt ! -path './isolinux/*' -print0 | xargs -0 md5sum > md5sum.txt)
ISO="$OUT_DIR/${DISTRO_ID}-${DISTRO_VERSION}-${EDITION}-${ARCH}.iso"
xorriso -as mkisofs -iso-level 3 -full-iso9660-filenames -J -joliet-long -volid "$ISO_LABEL" \
  -output "$ISO" \
  --grub2-mbr /usr/lib/grub/i386-pc/boot_hybrid.img \
  -partition_offset 16 --mbr-force-bootable \
  -append_partition 2 28732ac11ff8d211ba4b00a0c93ec93b "$EFIIMG" -appended_part_as_gpt \
  -iso_mbr_part_type a2a0d0ebe5b9334487c068b6b72699c7 \
  -c '/boot.catalog' \
  -b '/isolinux/bios.img' -no-emul-boot -boot-load-size 4 -boot-info-table --grub2-boot-info \
  -eltorito-alt-boot -e '--interval:appended_partition_2:::' -no-emul-boot \
  "$IMAGE"
(cd "$OUT_DIR" && sha256sum "$(basename "$ISO")" > "$(basename "$ISO").sha256")
log "ISO lista: $ISO ($(du -h "$ISO" | cut -f1))"
