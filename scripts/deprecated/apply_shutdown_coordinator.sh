#!/bin/bash

# Script to apply graceful shutdown coordinator to adapterOS server
# This script patches the necessary files to implement graceful shutdown

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "Applying shutdown coordinator patches..."
echo "Project root: $PROJECT_ROOT"

cd "$PROJECT_ROOT"

# Create backup
BACKUP_DIR="backups/shutdown_coordinator_$(date +%Y%m%d_%H%M%S)"
mkdir -p "$BACKUP_DIR"

echo "Creating backups in $BACKUP_DIR..."
cp crates/adapteros-server/src/main.rs "$BACKUP_DIR/main.rs.bak"
cp crates/adapteros-server/src/alerting.rs "$BACKUP_DIR/alerting.rs.bak"

echo "Backups created successfully"

# Note: The ShutdownCoordinator struct has already been added to main.rs
# Now we need to add the remaining patches

echo "Step 1: Initialize shutdown coordinator in main()"
# This will be done via direct edit in the editor

echo "Step 2: Update background tasks to accept shutdown signals"
# This requires modifying 8 different spawn sites

echo "Step 3: Update shutdown_signal() function"
# This requires passing the coordinator to the function

echo ""
echo "Manual steps required:"
echo "1. Initialize 'let mut shutdown_coordinator = ShutdownCoordinator::new();' after API config creation"
echo "2. Update each of 8 background task spawns to:"
echo "   a. Subscribe to shutdown signal: let mut shutdown_rx = shutdown_coordinator.subscribe();"
echo "   b. Pass shutdown_rx to task async block"
echo "   c. Add tokio::select! in task loop to listen for shutdown"
echo "   d. Store handle: shutdown_coordinator.add_*_task(name, handle);"
echo "3. Update shutdown_signal() to accept and use shutdown_coordinator"
echo "4. Pass shutdown_coordinator to axum serve's with_graceful_shutdown()"
echo ""
echo "See docs/shutdown_coordinator_implementation.md for detailed instructions"

exit 0
