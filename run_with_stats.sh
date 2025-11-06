#!/bin/bash

# Script para correr el bot MEV y analizar estadísticas al terminar

echo "🚀 Iniciando bot MEV con estadísticas..."
echo "Presiona Ctrl+C para detener y ver estadísticas"

# Crear directorio de logs si no existe
mkdir -p logs

# Generar nombre del log basado en timestamp
LOG_FILE="logs/mev_bot_$(date +%Y%m%d_%H%M%S).log"

echo "📝 Log file: $LOG_FILE"

# Ejecutar el bot y capturar la salida al log
cargo run 2>&1 | tee "$LOG_FILE" &

# PID del proceso
BOT_PID=$!

# Esperar señal de interrupción
trap 'echo; echo "🛑 Recibida señal de interrupción..."; kill $BOT_PID 2>/dev/null; wait $BOT_PID 2>/dev/null; echo "Bot detenido. Generando estadísticas..."; sleep 2; exit 0' INT TERM

# Esperar a que termine el bot (esto no debería ocurrir normalmente)
wait $BOT_PID

# Mostrar estadísticas después de la ejecución
echo
echo "📊 Generando estadísticas de operación..."
echo "Archivo de log: $LOG_FILE"
echo
echo "📈 ESTADÍSTICAS DEL BOT:"
echo "========================"
bash analyze_stats.sh "$LOG_FILE"
echo
echo "========================"
echo "✅ Análisis completado"