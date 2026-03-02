#!/bin/bash
# Colima lifecycle management script for adapterOS development

set -e

echo "Starting Colima for adapterOS development..."

# Start Colima if not running
if ! colima status >/dev/null 2>&1; then
    echo "Colima not running, starting..."
    colima start
else
    echo "Colima already running"
fi

# Wait for Docker to be ready
echo "Waiting for Docker daemon..."
timeout=30
while [ $timeout -gt 0 ]; do
    if docker ps >/dev/null 2>&1; then
        echo "Docker daemon ready"
        break
    fi
    echo "Waiting for Docker daemon... ($timeout seconds remaining)"
    sleep 2
    timeout=$((timeout - 2))
done

if [ $timeout -le 0 ]; then
    echo "Error: Docker daemon not ready after 30 seconds"
    exit 1
fi

# Start PostgreSQL if not running
if ! docker ps | grep -q adapteros-postgres; then
    echo "Starting PostgreSQL container..."
    docker run -d \
        --name adapteros-postgres \
        -e POSTGRES_PASSWORD=aos_password \
        -e POSTGRES_DB=adapteros \
        -p 5432:5432 \
        postgres:15

    echo "Waiting for PostgreSQL to be ready..."
    sleep 10
else
    echo "PostgreSQL container already running"
fi

# Set DATABASE_URL
export DATABASE_URL="postgresql://postgres:aos_password@localhost:5432/adapteros"
echo "DATABASE_URL set to: $DATABASE_URL"

echo "Colima development environment ready!"
echo "Run 'source scripts/colima-start.sh' in new shells to set DATABASE_URL"
