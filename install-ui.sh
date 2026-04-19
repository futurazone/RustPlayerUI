#!/bin/bash
# install-ui.sh - Script de instalación para PiPlayer Rust UI

echo "🚀 Migrando a PiPlayer Rust UI..."

# 1. Copiar el nuevo archivo de servicio
if [ -f "music-player-ui.service" ]; then
    sudo cp music-player-ui.service /etc/systemd/system/music-player-ui.service
    echo "✔ Archivo de servicio copiado a /etc/systemd/system/"
else
    echo "✖ Error: No se encuentra music-player-ui.service en la carpeta actual."
    exit 1
fi

# 2. Asegurar permisos de ejecución al binario
if [ -f "ui" ]; then
    chmod +x ui
    echo "✔ Permisos de ejecución concedidos al binario 'ui'"
else
    echo "⚠ Aviso: No se encuentra el binario 'ui'. Recuerda subirlo antes de arrancar el servicio."
fi

# 3. Recargar y reiniciar el servicio
echo "⚙ Recargando systemd y reiniciando servicio..."
sudo systemctl daemon-reload
sudo systemctl restart music-player-ui.service

echo "✅ ¡Listo! PiPlayer Rust UI está ahora configurado."
echo "Puedes ver los logs en vivo con: sudo journalctl -u music-player-ui.service -f"
