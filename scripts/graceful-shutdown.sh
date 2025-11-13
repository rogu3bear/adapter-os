#!/bin/bash
sudo launchctl stop adapteros-supervisor
sudo launchctl stop adapteros-server
brew services stop postgresql@15 || true

# Port freeze: gracefully terminate any lingering listeners on key ports
PORTS=(3300 9090 5432)
for port in "${PORTS[@]}"; do
    pids=$(sudo lsof -ti:"$port" 2>/dev/null)
    if [ -n "$pids" ]; then
        echo "Terminating processes listening on port $port (PIDs: $pids)"
        echo "$pids" | xargs sudo kill -TERM 2>/dev/null || true
    fi
done

sleep 5

# Force kill if still bound
for port in "${PORTS[@]}"; do
    pids=$(sudo lsof -ti:"$port" 2>/dev/null)
    if [ -n "$pids" ]; then
        echo "Force killing processes still listening on port $port (PIDs: $pids)"
        echo "$pids" | xargs sudo kill -KILL 2>/dev/null || true
    fi
done

echo "Port freeze complete. Shutdown finalized."


