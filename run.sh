#!/bin/bash
# Script para iniciar el bot MEV con logging automático

# Crear directorio para logs si no existe
mkdir -p logs

# Fecha y hora actual
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

echo "Iniciando bot MEV: $TIMESTAMP"
echo "Balance inicial: $(solana balance)"

# Iniciar bot y guardar logs
cargo run > "logs/mev_bot_$TIMESTAMP.log" 2>&1 &

# Guardar PID para poder detenerlo después
echo $! > "logs/mev_bot_$TIMESTAMP.pid"
echo "Bot corriendo con PID: $(cat logs/mev_bot_$TIMESTAMP.pid)"

echo "Puedes detener el bot con: kill -INT \$(cat logs/mev_bot_$TIMESTAMP.pid)"
echo "Para seguir los logs: tail -f logs/mev_bot_$TIMESTAMP.log"