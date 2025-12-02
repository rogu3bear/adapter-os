#!/bin/bash
# Check SQLX offline cache
if [[ "$SQLX_OFFLINE" != "true" ]]; then
    echo "SQLX_OFFLINE not set. Skipping cache check."
    exit 0
fi

if ! cargo check --workspace --quiet; then
    echo "SQLX cache validation failed"
    exit 1
fi

echo "SQLX cache OK"
