#!/bin/bash
# Backup PostgreSQL schema only
# Usage: ./scripts/backup_schema.sh
# Requires DATABASE_URL environment variable

set -e

if [ -z "$DATABASE_URL" ]; then
    echo "Error: DATABASE_URL environment variable is not set" >&2
    exit 1
fi

# Create backups directory if it doesn't exist
mkdir -p var/backups

# Generate backup filename with date
BACKUP_FILE="var/backups/schema_$(date +%F).sql"

echo "Backing up schema to $BACKUP_FILE..."

# Run pg_dump with schema-only flag
pg_dump --schema-only --file "$BACKUP_FILE" "$DATABASE_URL"

if [ $? -eq 0 ]; then
    echo "Schema backup completed successfully: $BACKUP_FILE"
else
    echo "Error: Schema backup failed" >&2
    exit 1
fi

