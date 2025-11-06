#!/bin/bash
# Script para analizar logs del bot MEV

LOG_FILE=$1

if [ -z "$LOG_FILE" ]; then
    echo "Uso: $0 <archivo_log>"
    echo "O para analizar el último log:"
    echo "  $0 \$(ls -t logs/mev_bot_*.log | head -n1)"
    exit 1
fi

if [ ! -f "$LOG_FILE" ]; then
    echo "Archivo no encontrado: $LOG_FILE"
    exit 1
fi

echo "=== Análisis de $LOG_FILE ==="
echo "Timestamp: $(date)"
echo "Archivo: $LOG_FILE"
echo "Tamaño: $(du -h $LOG_FILE | cut -f1)"
echo

echo "Oportunidades detectadas:"
OPPORTUNITIES=$(grep -c "OPPORTUNITY" $LOG_FILE)
echo "  Total: $OPPORTUNITIES"

echo
echo "Análisis de rentabilidad:"
grep "Final estimated profit potential:" $LOG_FILE | awk '{
    sum+=$5; 
    count++;
    if($5 > max) max=$5;
    if($5 < min || min == 0) min=$5;
} END {
    if(count > 0) {
        print "  Promedio: " sum/count " SOL";
        print "  Máximo: " max " SOL";
        print "  Mínimo: " min " SOL";
        print "  Total teórico: " sum " SOL";
    } else {
        print "  Promedio: 0.000000 SOL";
        print "  Máximo: 0.000000 SOL";
        print "  Mínimo: 0.000000 SOL";
        print "  Total teórico: 0.000000 SOL";
    }
}'

echo
echo "Resultados de ejecución:"
SUCCESSFUL=$(grep -c "Frontrun successful" $LOG_FILE)
echo "  Frontrun exitosos: $SUCCESSFUL"

SKIPPED=$(grep -c "Skipping opportunity with no positive profit" $LOG_FILE)
echo "  Oportunidades skippeadas: $SKIPPED"

JITO_ERRORS=$(grep -c "Failed to send Jito bundle" $LOG_FILE)
echo "  Errores de Jito: $JITO_ERRORS"

FAILED_OPPS=$(grep -c "❌ Opportunity not profitable" $LOG_FILE)
echo "  Oportunidades no rentables: $FAILED_OPPS"

FAILED_TX=$(grep -c "Failed to send frontrun transaction" $LOG_FILE)
echo "  Transacciones fallidas: $FAILED_TX"

echo
echo "Estadísticas:"
if [ $OPPORTUNITIES -gt 0 ]; then
    SUCCESS_RATE=$(echo "scale=2; $SUCCESSFUL * 100 / $OPPORTUNITIES" | bc)
    echo "  Tasa de éxito: $SUCCESS_RATE%"
fi

if [ $OPPORTUNITIES -gt 0 ]; then
    SKIP_RATE=$(echo "scale=2; $SKIPPED * 100 / $OPPORTUNITIES" | bc)
    echo "  Tasa de skippeo: $SKIP_RATE%"
fi

echo
echo "Conexiones y operaciones:"
JITO_CONNECTIONS=$(grep -c "Jito bundle sent successfully" $LOG_FILE)
echo "  Bundles Jito exitosos: $JITO_CONNECTIONS"

JITO_ATTEMPTS=$(grep -c "Sending bundle via Jito" $LOG_FILE)
echo "  Intentos de Jito: $JITO_ATTEMPTS"

if [ $JITO_ATTEMPTS -gt 0 ]; then
    JITO_SUCCESS_RATE=$(echo "scale=2; $JITO_CONNECTIONS * 100 / $JITO_ATTEMPTS" | bc)
    echo "  Tasa de éxito Jito: $JITO_SUCCESS_RATE%"
fi

echo
echo "Análisis de riesgos:"
LOW_BALANCE=$(grep -c "Balance too low" $LOG_FILE)
echo "  Alertas de balance bajo: $LOW_BALANCE"

echo
echo "Transacciones procesadas:"
TX_DETECTED=$(grep -c "Transaction detected:" $LOG_FILE)
echo "  Transacciones detectadas: $TX_DETECTED"

PROFIT_CHECKS=$(grep -c "Analyzing profitability for transaction" $LOG_FILE)
echo "  Análisis de profitabilidad: $PROFIT_CHECKS"

echo
echo "⏰ TIEMPO DE OPERACIÓN:"
START_TIME=$(head -n1 $LOG_FILE | grep -o '[0-9]\{4\}-[0-9]\{2\}-[0-9]\{2\} [0-9]\{2\}:[0-9]\{2\}:[0-9]\{2\}' | head -n1)
END_TIME=$(tail -n1 $LOG_FILE | grep -o '[0-9]\{4\}-[0-9]\{2\}-[0-9]\{2\} [0-9]\{2\}:[0-9]\{2\}:[0-9]\{2\}' | tail -n1)

if [ ! -z "$START_TIME" ] && [ ! -z "$END_TIME" ]; then
    START_SECONDS=$(date -jf "%Y-%m-%d %H:%M:%S" "$START_TIME" +%s 2>/dev/null || date -d "$START_TIME" +%s 2>/dev/null)
    END_SECONDS=$(date -jf "%Y-%m-%d %H:%M:%S" "$END_TIME" +%s 2>/dev/null || date -d "$END_TIME" +%s 2>/dev/null)
    
    if [ ! -z "$START_SECONDS" ] && [ ! -z "$END_SECONDS" ]; then
        DURATION_SECONDS=$(($END_SECONDS - $START_SECONDS))
        DURATION_HOURS=$(echo "scale=2; $DURATION_SECONDS / 3600" | bc)
        DURATION_MINUTES=$(echo "scale=0; $DURATION_SECONDS / 60" | bc)
        
        echo "  Duración total: $DURATION_HOURS horas ($DURATION_MINUTES minutos)"
        echo "  Inicio: $START_TIME"
        echo "  Fin: $END_TIME"
    fi
fi

echo
echo "📊 ESTADÍSTICAS DETALLADAS:"
COMPLETED_TXS=$(grep -c "Frontrun executed for transaction" $LOG_FILE)
echo "  Transacciones completadas: $COMPLETED_TXS"

COMPLETED_SUCCESS_TXS=$(grep -c "Frontrun successful" $LOG_FILE)
echo "  Transacciones exitosas: $COMPLETED_SUCCESS_TXS"

COMPLETED_FAILED_TXS=$(grep -c "Frontrun failed for transaction" $LOG_FILE)
echo "  Transacciones fallidas: $COMPLETED_FAILED_TXS"

if [ $OPPORTUNITIES -gt 0 ]; then
    SUCCESS_RATE=$(echo "scale=2; $COMPLETED_SUCCESS_TXS * 100 / $OPPORTUNITIES" | bc)
    echo "  Tasa de éxito sobre oportunidades: $SUCCESS_RATE%"
fi

if [ $COMPLETED_TXS -gt 0 ]; then
    SUCCESS_RATE_ON_ATTEMPTS=$(echo "scale=2; $COMPLETED_SUCCESS_TXS * 100 / $COMPLETED_TXS" | bc)
    echo "  Tasa de éxito sobre intentos: $SUCCESS_RATE_ON_ATTEMPTS%"
fi

echo
echo "🔍 ANÁLISIS PROFUNDO:"
JITO_SENT=$(grep -c "Jito bundle sent successfully" $LOG_FILE)
echo "  Bundles Jito enviados exitosamente: $JITO_SENT"

JITO_TOTAL=$(grep -c "Sending bundle via Jito" $LOG_FILE)
echo "  Intentos totales de Jito: $JITO_TOTAL"

if [ $JITO_TOTAL -gt 0 ]; then
    JITO_SUCCESS_RATE=$(echo "scale=2; $JITO_SENT * 100 / $JITO_TOTAL" | bc)
    echo "  Tasa de éxito Jito: $JITO_SUCCESS_RATE%"
fi

SLOT_MONITORING=$(grep -c "Monitoring Solana" $LOG_FILE)
echo "  Verificaciones de slot: $SLOT_MONITORING"

CONNECTION_ERRORS=$(grep -c "WebSocket error" $LOG_FILE)
echo "  Errores de conexión WebSocket: $CONNECTION_ERRORS"

echo
echo "=== Fin del análisis ==="