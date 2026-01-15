#!/bin/bash
# Stop Colima and cleanup for adapterOS development

echo "Stopping Colima development environment..."

# Stop PostgreSQL container
if docker ps | grep -q adapteros-postgres; then
    echo "Stopping PostgreSQL container..."
    docker stop adapteros-postgres
    docker rm adapteros-postgres
else
    echo "PostgreSQL container not running"
fi

# Stop Colima
echo "Stopping Colima..."
colima stop

echo "Colima development environment stopped"
