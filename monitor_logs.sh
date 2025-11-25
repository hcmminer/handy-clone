#!/bin/bash
# Monitor Handy logs continuously for BlackHole debugging

LOG_FILE="$HOME/Library/Logs/com.pais.handy/handy.log"

echo "=== Monitoring Handy logs for BlackHole debugging ==="
echo "Log file: $LOG_FILE"
echo "Press Ctrl+C to stop"
echo ""

# Wait for log file to exist
while [ ! -f "$LOG_FILE" ]; do
    echo "Waiting for log file to be created..."
    sleep 2
done

echo "Log file found! Monitoring..."
echo ""

# Monitor logs with filtering
tail -f "$LOG_FILE" 2>/dev/null | grep --line-buffered -E "(BlackHole|Device config|Callback|First 10|samples|RMS|Max|Range|ZERO|AUDIO DETECTED|System audio capture)" | while read line; do
    echo "[$(date +%H:%M:%S)] $line"
done

