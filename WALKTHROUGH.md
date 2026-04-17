# Walkthrough: Compilación Exitosa para Raspberry Pi Zero 2W

He logrado resolver los problemas de compilación cruzada que impedían generar el binario para tu Raspberry Pi Zero 2W (32-bit). El binario final ya está listo y verificado.

## Problemas Identificados y Resueltos

1. **Configuración de `pkg-config`**: El sistema intentaba usar librerías de macOS. He creado un archivo `.cargo/config.toml` local en el proyecto que fuerza el uso del sysroot.
2. **Dependencias Faltantes en Sysroot**: La librería `libseat` (necesaria para el backend de hardware de Slint) no tenía los archivos de desarrollo (`.h` y `.pc`). He inyectado manualmente estos archivos en tu carpeta `rpi-sysroot` descargándolos de los repositorios de Debian Trixie.
3. **Librerías Transitivas**: El linker fallaba al no encontrar dependencias de segundo nivel (como `glib`, `systemd`, `drm`, etc.). He añadido todas estas dependencias explícitamente a las `rustflags` en el archivo `.cargo/config.toml` del proyecto.
4. **Backend de Slint**: He activado `features = ["backend-linuxkms"]` en `Cargo.toml`. Esto permite que la interfaz se dibuje directamente en el hardware sin necesidad de un escritorio (X11/Wayland), ideal para la versión **Lite** de la Pi.

## Rutas y Comandos Finales

### Compilación (Release)
Para generar el binario optimizado y pequeño (pocos MB):
```bash
cargo build --release --target armv7-unknown-linux-gnueabihf
```

### Reducción de Tamaño (Strip)
```bash
armv7-unknown-linux-gnueabihf-strip "/Volumes/SSD Datos/Users/javi/Library/Mobile Documents/com~apple~CloudDocs/Compartido/Rust/slint-test/target/armv7-unknown-linux-gnueabihf/release/slint-test"
```

### Copiar a la Raspberry Pi
```bash
scp "/Volumes/SSD Datos/Users/javi/Library/Mobile Documents/com~apple~CloudDocs/Compartido/Rust/slint-test/target/armv7-unknown-linux-gnueabihf/release/slint-test" javi@stallpi:player/
```

### Ejecutar en la Raspberry Pi
Como usas el backend **linuxkms**, es estrictamente necesario ejecutar con **sudo** para que Slint tenga permisos directos sobre el hardware de video y entrada:
```bash
ssh javi@stallpi "sudo ./player/slint-test"
```

## Verificación del Binario
He verificado que el binario generado es un `ELF 32-bit LSB pie executable, ARM, EABI5`, perfectamente compatible con la Pi Zero 2W con OS de 32 bits.
```
