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
grep "Estimated profit potential:" $LOG_FILE | awk '{
    sum+=$4; 
    count++;
    if($4 > max) max=$4;
    if($4 < min || min == 0) min=$4;
} END {
    print "  Promedio: " sum/count " SOL";
    print "  Máximo: " max " SOL";
    print "  Mínimo: " min " SOL";
    print "  Total teórico: " sum " SOL";
}'

echo
echo "Resultados de ejecución:"
SUCCESSFUL=$(grep -c "Frontrun successful" $LOG_FILE)
echo "  Frontrun exitosos: $SUCCESSFUL"

SKIPPED=$(grep -c "Skipping unprofitable" $LOG_FILE)
echo "  Oportunidades skippeadas: $SKIPPED"

JITO_ERRORS=$(grep -c "Failed to send Jito bundle" $LOG_FILE)
echo "  Errores de Jito: $JITO_ERRORS"

FAILED_OPPS=$(grep -c "❌ Opportunity not profitable" $LOG_FILE)
echo "  Oportunidades no rentables: $FAILED_OPPS"

echo
echo "Estadísticas:"
if [ $OPPORTUNITIES -gt 0 ]; then
    SUCCESS_RATE=$(echo "scale=2; $SUCCESSFUL * 100 / $OPPORTUNITIES" | bc)
    echo "  Tasa de éxito: $SUCCESS_RATE%"
fi

echo
echo "Transacciones Jito:"
JITO_ATTEMPTS=$(grep -c "Sending bundle via Jito" $LOG_FILE)
echo "  Intentos de Jito: $JITO_ATTEMPTS"
if [ $JITO_ATTEMPTS -gt 0 ]; then
    JITO_SUCCESS_RATE=$(echo "scale=2; ($JITO_ATTEMPTS - $JITO_ERRORS) * 100 / $JITO_ATTEMPTS" | bc)
    echo "  Tasa de éxito Jito: $JITO_SUCCESS_RATE%"
fi

echo
echo "Tip transactions creadas:"
TIP_TXS=$(grep -c "Tip transaction created" $LOG_FILE)
echo "  Total: $TIP_TXS"

echo
echo "Horas de operación estimadas:"
HOURS=$(($(grep -c "Frontrun executed" $LOG_FILE) / 2))
echo "  Aproximadamente: $HOURS horas (basado en frecuencia de ejecución)"

echo
echo "=== Fin del análisis ==="