#!/bin/bash
# Setup SQLX offline mode
export SQLX_OFFLINE=true
export DATABASE_URL="sqlite://./target/sqlx-cache.db"
echo "SQLX offline mode enabled. Build with: cargo build"
echo ""
echo "To disable offline mode (for development with live database):"
echo "  unset SQLX_OFFLINE"
