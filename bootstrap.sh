#!/usr/bin/env bash
set -euo pipefail

# Local paths for macOS dev
CONFIG_DIR="$HOME/adapteros-var/etc/adapteros"
LIB_DIR="$HOME/adapteros-var/lib/adapteros"
LOG_DIR="$HOME/adapteros-var/log/adapteros"

# Start Postgres
brew services start postgresql@15 || true
sleep 5

# Run migrations via aosctl
aosctl db migrate

# Drift check
if [ -f "target/release/adapteros-server" ]; then
  target/release/adapteros-server --config "$CONFIG_DIR/cp.toml" -- aosctl drift-check
else
  echo "Server binary not found; skip drift check"
fi

# Start services manually (launchd stub)
if [ -f "target/release/adapteros-server" ]; then
  target/release/adapteros-server --config "$CONFIG_DIR/cp.toml" &
  echo $! > "$LIB_DIR/server.pid"
fi

if [ -f "target/release/adapteros-service-supervisor" ]; then
  target/release/adapteros-service-supervisor --config "$CONFIG_DIR/supervisor.yaml" &
  echo $! > "$LIB_DIR/supervisor.pid"
fi

echo "Services started manually; use launchctl for production"
