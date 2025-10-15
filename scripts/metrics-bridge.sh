#!/bin/bash
# Metrics Bridge - UDS to External Prometheus
# 
# This script reads metrics from the Unix domain socket and exports them
# to an external Prometheus push gateway, ensuring zero network egress
# from the AdapterOS worker processes (Egress Ruleset #1).

set -euo pipefail

# Configuration
UDS_SOCKET="${AOS_METRICS_SOCKET:-/var/run/aos/default/metrics.sock}"
PUSH_GATEWAY="${PROMETHEUS_PUSH_GATEWAY:-http://pushgateway:9091}"
JOB_NAME="${PROMETHEUS_JOB:-aos}"
INTERVAL="${METRICS_PUSH_INTERVAL:-15}"

# Check dependencies
if ! command -v socat &> /dev/null; then
    echo "Error: socat not installed"
    exit 1
fi

if ! command -v curl &> /dev/null; then
    echo "Error: curl not installed"
    exit 1
fi

echo "Starting metrics bridge..."
echo "  UDS Socket: $UDS_SOCKET"
echo "  Push Gateway: $PUSH_GATEWAY"
echo "  Job Name: $JOB_NAME"
echo "  Interval: ${INTERVAL}s"

# Wait for socket to be available
while [ ! -S "$UDS_SOCKET" ]; then
    echo "Waiting for metrics socket..."
    sleep 1
done

echo "Metrics socket available, starting bridge loop..."

# Main bridge loop
while true; do
    # Read metrics from UDS
    if ! metrics=$(socat - UNIX-CONNECT:"$UDS_SOCKET" < /dev/null 2>/dev/null); then
        echo "Warning: Failed to read metrics from socket"
        sleep "$INTERVAL"
        continue
    fi

    # Check if we got any metrics
    if [ -z "$metrics" ]; then
        echo "Warning: No metrics received"
        sleep "$INTERVAL"
        continue
    fi

    # Push to Prometheus push gateway
    if echo "$metrics" | curl -s -X POST \
        --data-binary @- \
        "${PUSH_GATEWAY}/metrics/job/${JOB_NAME}" > /dev/null; then
        echo "$(date '+%Y-%m-%d %H:%M:%S') - Metrics pushed successfully"
    else
        echo "$(date '+%Y-%m-%d %H:%M:%S') - Warning: Failed to push metrics"
    fi

    # Wait before next push
    sleep "$INTERVAL"
done

