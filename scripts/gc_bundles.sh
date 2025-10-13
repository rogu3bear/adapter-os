#!/bin/bash
# AdapterOS Bundle Garbage Collection Script
#
# Implements retention policy from Ruleset #10:
# - Keep last K bundles per CPID
# - Keep all bundles referenced in open incidents
# - Keep at least one promotion bundle per CP promotion
#
# Usage: ./scripts/gc_bundles.sh [--dry-run] [--keep-count N]

set -e

# Configuration
BUNDLES_PATH="${BUNDLES_PATH:-/srv/aos/bundles}"
DB_PATH="${DB_PATH:-var/aos-cp.sqlite3}"
KEEP_COUNT="${KEEP_COUNT:-12}"  # Default: keep last 12 bundles per CPID
DRY_RUN=false

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        --keep-count)
            KEEP_COUNT="$2"
            shift 2
            ;;
        *)
            echo "Unknown option: $1"
            echo "Usage: $0 [--dry-run] [--keep-count N]"
            exit 1
            ;;
    esac
done

echo "AdapterOS Bundle Garbage Collection"
echo "===================================="
echo "Bundles path: ${BUNDLES_PATH}"
echo "Keep count: ${KEEP_COUNT} per CPID"
echo "Dry run: ${DRY_RUN}"
echo ""

if [ ! -d "${BUNDLES_PATH}" ]; then
    echo "Error: Bundles directory not found: ${BUNDLES_PATH}"
    exit 1
fi

if [ ! -f "${DB_PATH}" ]; then
    echo "Error: Database not found: ${DB_PATH}"
    exit 1
fi

# Get list of incident-referenced bundles
echo "Querying incident-referenced bundles..."
INCIDENT_BUNDLES=$(sqlite3 "${DB_PATH}" \
    "SELECT DISTINCT bundle_id FROM incidents WHERE status != 'closed';" \
    2>/dev/null || echo "")

if [ -n "${INCIDENT_BUNDLES}" ]; then
    INCIDENT_COUNT=$(echo "${INCIDENT_BUNDLES}" | wc -l)
    echo "  Found ${INCIDENT_COUNT} bundles referenced by open incidents"
else
    echo "  No incident-referenced bundles"
fi

# Get list of promotion bundles
echo "Querying promotion bundles..."
PROMOTION_BUNDLES=$(sqlite3 "${DB_PATH}" \
    "SELECT DISTINCT tb.bundle_id 
     FROM telemetry_bundles tb 
     JOIN cp_pointers cp ON tb.cpid = cp.name 
     WHERE cp.promoted = 1;" \
    2>/dev/null || echo "")

if [ -n "${PROMOTION_BUNDLES}" ]; then
    PROMOTION_COUNT=$(echo "${PROMOTION_BUNDLES}" | wc -l)
    echo "  Found ${PROMOTION_COUNT} promotion bundles"
else
    echo "  No promotion bundles"
fi

# Process each CPID
echo ""
echo "Processing bundles by CPID..."

# Get list of CPIDs from database
CPIDS=$(sqlite3 "${DB_PATH}" "SELECT DISTINCT cpid FROM telemetry_bundles ORDER BY cpid;" || echo "")

if [ -z "${CPIDS}" ]; then
    echo "No CPIDs found in database"
    exit 0
fi

TOTAL_DELETED=0
TOTAL_KEPT=0

for CPID in ${CPIDS}; do
    echo ""
    echo "CPID: ${CPID}"
    
    # Get all bundles for this CPID, sorted by timestamp (newest first)
    BUNDLES=$(sqlite3 "${DB_PATH}" \
        "SELECT bundle_id, created_at 
         FROM telemetry_bundles 
         WHERE cpid = '${CPID}' 
         ORDER BY created_at DESC;")
    
    BUNDLE_COUNT=$(echo "${BUNDLES}" | wc -l)
    echo "  Total bundles: ${BUNDLE_COUNT}"
    
    # Keep first K bundles
    KEPT_COUNT=0
    DELETED_COUNT=0
    
    echo "${BUNDLES}" | while IFS='|' read -r BUNDLE_ID CREATED_AT; do
        if [ -z "${BUNDLE_ID}" ]; then
            continue
        fi
        
        SHOULD_DELETE=true
        REASON=""
        
        # Check if in keep window (first K)
        KEPT_COUNT=$((KEPT_COUNT + 1))
        if [ ${KEPT_COUNT} -le ${KEEP_COUNT} ]; then
            SHOULD_DELETE=false
            REASON="within keep window (${KEPT_COUNT}/${KEEP_COUNT})"
        fi
        
        # Check if referenced by incident
        if echo "${INCIDENT_BUNDLES}" | grep -q "^${BUNDLE_ID}$"; then
            SHOULD_DELETE=false
            REASON="referenced by open incident"
        fi
        
        # Check if promotion bundle
        if echo "${PROMOTION_BUNDLES}" | grep -q "^${BUNDLE_ID}$"; then
            SHOULD_DELETE=false
            REASON="promotion bundle"
        fi
        
        # Execute deletion or report
        if [ "${SHOULD_DELETE}" = true ]; then
            BUNDLE_FILE="${BUNDLES_PATH}/${BUNDLE_ID}.ndjson"
            
            if [ "$DRY_RUN" = true ]; then
                echo "    [DRY RUN] Would delete: ${BUNDLE_ID} (${CREATED_AT})"
            else
                if [ -f "${BUNDLE_FILE}" ]; then
                    rm -f "${BUNDLE_FILE}"
                    echo "    Deleted: ${BUNDLE_ID} (${CREATED_AT})"
                    DELETED_COUNT=$((DELETED_COUNT + 1))
                fi
            fi
        else
            if [ "$DRY_RUN" = true ]; then
                echo "    [DRY RUN] Would keep: ${BUNDLE_ID} (${REASON})"
            else
                TOTAL_KEPT=$((TOTAL_KEPT + 1))
            fi
        fi
    done
    
    TOTAL_DELETED=$((TOTAL_DELETED + DELETED_COUNT))
done

# Summary
echo ""
echo "Garbage Collection Summary"
echo "=========================="

if [ "$DRY_RUN" = true ]; then
    echo "Dry run complete (no files deleted)"
else
    echo "Bundles deleted: ${TOTAL_DELETED}"
    echo "Bundles kept: ${TOTAL_KEPT}"
    echo ""
    echo "✓ Garbage collection complete"
fi

exit 0
