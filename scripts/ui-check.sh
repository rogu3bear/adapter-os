#!/bin/bash
# Single-instance UI check - prevents multiple agents from running cargo check simultaneously

LOCKFILE="/tmp/adapteros-ui-check.lock"
LOCKFD=200

# Try to acquire lock (non-blocking)
exec 200>"$LOCKFILE"
if ! flock -n $LOCKFD; then
    echo "Another UI check is running. Waiting..."
    flock $LOCKFD  # Block until lock is available
fi

# We have the lock - run the check
echo "Running UI check..."
cargo check -p adapteros-ui --target wasm32-unknown-unknown 2>&1
EXIT_CODE=$?

# Lock is automatically released when script exits
exit $EXIT_CODE
