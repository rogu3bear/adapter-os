#!/bin/bash

# Migration Conflict Detection Script
# Checks for potential conflicts between migration files

set -e

echo "🔍 Checking for migration conflicts..."

MIGRATIONS_DIR="migrations"
CONFLICTS_FOUND=0

# Check for duplicate migration numbers
echo "Checking for duplicate migration numbers..."
MIGRATION_NUMBERS=$(ls "$MIGRATIONS_DIR"/*.sql 2>/dev/null | grep -E '[0-9]+\.sql' | sed -E 's/.*\/([0-9]+)_.*/\1/' | sort | uniq -d || true)

if [ -n "$MIGRATION_NUMBERS" ]; then
    echo "❌ ERROR: Duplicate migration numbers found:"
    echo "$MIGRATION_NUMBERS"
    CONFLICTS_FOUND=$((CONFLICTS_FOUND + 1))
fi

# Check for migrations that modify the same tables
echo "Checking for table modification conflicts..."

# Common table patterns to check for conflicts
TABLE_PATTERNS=(
    "CREATE TABLE.*adapters"
    "CREATE TABLE.*tenants"
    "CREATE TABLE.*training_jobs"
    "ALTER TABLE.*adapters"
    "ALTER TABLE.*tenants"
    "ALTER TABLE.*training_jobs"
)

for pattern in "${TABLE_PATTERNS[@]}"; do
    CONFLICTING_FILES=$(grep -l "$pattern" "$MIGRATIONS_DIR"/*.sql 2>/dev/null || true)

    if [ $(echo "$CONFLICTING_FILES" | wc -l) -gt 1 ]; then
        echo "⚠️  WARNING: Multiple migrations modify similar tables with pattern '$pattern':"
        echo "$CONFLICTING_FILES"
        echo ""
    fi
done

# Check for migrations with missing rollback procedures
echo "Checking for migrations without rollback procedures..."
ROLLBACK_DIR="$MIGRATIONS_DIR/rollbacks"

for migration_file in "$MIGRATIONS_DIR"/*.sql; do
    if [[ -f "$migration_file" ]]; then
        filename=$(basename "$migration_file" .sql)
        expected_rollback="$ROLLBACK_DIR/${filename}_rollback.sql"

        if [[ ! -f "$expected_rollback" ]]; then
            # Only warn for recent migrations (last 20)
            migration_num=$(echo "$filename" | grep -oE '^[0-9]+' | head -1)
            if [[ $migration_num -gt 55 ]] 2>/dev/null; then  # Recent migrations
                echo "⚠️  WARNING: Missing rollback procedure for: $filename"
            fi
        fi
    fi
done

# Validate migration signatures
echo "Validating migration signatures..."
if [[ -f "$MIGRATIONS_DIR/signatures.json" ]]; then
    SIGNATURE_COUNT=$(jq '.signatures | length' "$MIGRATIONS_DIR/signatures.json" 2>/dev/null || echo "0")
    MIGRATION_COUNT=$(ls "$MIGRATIONS_DIR"/*.sql 2>/dev/null | wc -l)

    if [[ $SIGNATURE_COUNT -ne $MIGRATION_COUNT ]]; then
        echo "❌ ERROR: Signature count ($SIGNATURE_COUNT) doesn't match migration count ($MIGRATION_COUNT)"
        CONFLICTS_FOUND=$((CONFLICTS_FOUND + 1))
    else
        echo "✅ All migrations have valid signatures"
    fi
else
    echo "❌ ERROR: Migration signatures file not found"
    CONFLICTS_FOUND=$((CONFLICTS_FOUND + 1))
fi

# Check for migration file naming consistency
echo "Checking migration file naming consistency..."
INVALID_NAMES=$(ls "$MIGRATIONS_DIR"/*.sql 2>/dev/null | grep -vE '^[0-9]{4}_.*\.sql$' || true)

if [ -n "$INVALID_NAMES" ]; then
    echo "⚠️  WARNING: Migration files with non-standard naming:"
    echo "$INVALID_NAMES"
fi

echo ""
if [ $CONFLICTS_FOUND -eq 0 ]; then
    echo "✅ No migration conflicts detected"
    exit 0
else
    echo "❌ $CONFLICTS_FOUND migration conflict(s) found"
    exit 1
fi
