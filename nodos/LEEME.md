# Nodo despertador

Una Raspberry Pi (o cualquier Linux chico) con micrófono USB escucha palmadas y despierta al PC central con
Wake-on-LAN cuando está suspendido o apagado. Usa el mismo programa de Neo Orión, sin Python:

```bash
sudo apt install alsa-utils
# copia orion-nucleo (la versión aarch64 si es una Raspberry) a /usr/local/bin
orion-nucleo despertador AA:BB:CC:DD:EE:FF      # la MAC del PC central (el HUD la muestra en «prepara el encendido por red»)
orion-nucleo despertador AA:BB:CC:DD:EE:FF 3    # pide 3 palmadas en vez de 2
```
