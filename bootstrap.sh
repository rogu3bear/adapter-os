#!/bin/bash
# Local paths for macOS dev
CONFIG_DIR=&quot;$HOME/adapteros-var/etc/adapteros&quot;
LIB_DIR=&quot;$HOME/adapteros-var/lib/adapteros&quot;
LOG_DIR=&quot;$HOME/adapteros-var/log/adapteros&quot;

# Start Postgres
brew services start postgresql@15 || true
sleep 5

# Run migrations
./scripts/migrate.sh

# Drift check
if [ -f &quot;target/release/adapteros-server&quot; ]; then
  target/release/adapteros-server --config $CONFIG_DIR/cp.toml -- aosctl drift-check
else
  echo &quot;Server binary not found; skip drift check&quot;
fi

# Start services manually (launchd stub)
if [ -f &quot;target/release/adapteros-server&quot; ]; then
  target/release/adapteros-server --config $CONFIG_DIR/cp.toml &
  echo $! > $LIB_DIR/server.pid
fi

if [ -f &quot;target/release/adapteros-service-supervisor&quot; ]; then
  target/release/adapteros-service-supervisor --config $CONFIG_DIR/supervisor.yaml &
  echo $! > $LIB_DIR/supervisor.pid
fi

echo &quot;Services started manually; use launchctl for production&quot;
