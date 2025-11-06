#!/bin/bash

# Persistent MEV Bot Script with auto-reconnect

# Function to get balance from the Solana keypair file
get_balance() {
    if [ -f "solana-keypair.json" ]; then
        # Use solana CLI to get the balance
        # Since we need to get the public key from the keypair file, we'll use the solana CLI with the keypair
        balance_output=$(solana balance --keypair solana-keypair.json 2>/dev/null)
        if [ $? -eq 0 ]; then
            # Extract the SOL value from the output (e.g., "1.234567 SOL" -> "1.234567")
            echo "$balance_output" | cut -d' ' -f1
        else
            echo "0.000000"
        fi
    else
        echo "0.000000"
    fi
}

LOG_FILE="logs/persistent_mev_bot_$(date +%Y%m%d_%H%M%S).log"
mkdir -p logs

echo "🚀 Iniciando bot MEV persistente..."
echo "📝 Log file: $LOG_FILE"
echo "📅 Inicio: $(date)"

# Get initial balance before starting the bot
echo "💰 Obteniendo balance inicial..."
INITIAL_BALANCE=$(get_balance)
echo "💰 Balance inicial: $INITIAL_BALANCE SOL"

echo "Presiona Ctrl+C para detener"

# Set up signal handling for graceful shutdown
trap 'echo "🛑 Señal de interrupción recibida..."; kill $(jobs -p) 2>/dev/null; echo "📊 Generando estadísticas de operación..."; echo "Archivo de log: $LOG_FILE"; echo; echo "📈 ESTADÍSTICAS DEL BOT:"; echo "========================"; bash analyze_stats.sh "$LOG_FILE"; echo; echo "========================"; echo "📊 BALANCE RESUMEN:"; FINAL_BALANCE=$(get_balance); echo "💰 Balance inicial: $INITIAL_BALANCE SOL"; echo "💰 Balance final: $FINAL_BALANCE SOL"; if [ $(echo "$INITIAL_BALANCE > 0" | bc -l 2>/dev/null || echo "0") -eq 1 ]; then PROFIT_LOSS=$(echo "$FINAL_BALANCE - $INITIAL_BALANCE" | bc -l 2>/dev/null || echo "0"); PERCENTAGE_CHANGE=$(echo "scale=4; ($PROFIT_LOSS / $INITIAL_BALANCE) * 100" | bc -l 2>/dev/null || echo "0"); echo "📈 Ganancia/Pérdida: $PROFIT_LOSS SOL ($(printf '%+.2f' $PERCENTAGE_CHANGE)%)"; else echo "📈 Ganancia/Pérdida: N/A (balance inicial no disponible)"; fi; echo; echo "✅ Análisis completado"; exit 0' INT TERM

# Run the bot in the background and restart if it stops
while true; do
    echo "🔄 Iniciando bot MEV... (Intento: $(date))"
    
    # Run the bot with coloring forced on and capture logs
    if cargo run --color=always 2>&1 | tee -a "$LOG_FILE"; then
        echo "⚡ Bot terminado normalmente, reiniciando en 5 segundos..."
    else
        EXIT_CODE=$?
        echo "⚡ Bot terminado con código: $EXIT_CODE (Timestamp: $(date))"
        echo "⚠️  Bot detenido inesperadamente, reiniciando en 5 segundos..."
    fi
    
    sleep 5
done