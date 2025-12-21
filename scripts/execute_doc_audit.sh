#!/bin/bash
# Execute documentation audit recommendations
# Usage: ./scripts/execute_doc_audit.sh [--dry-run] [--confidence=HIGH|MEDIUM|ALL]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
RESULTS_FILE="$REPO_ROOT/docs/DOCUMENTATION_AUDIT_RESULTS.md"

cd "$REPO_ROOT"

DRY_RUN=false
CONFIDENCE_FILTER="HIGH"

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        --confidence=*)
            CONFIDENCE_FILTER="${1#*=}"
            shift
            ;;
        *)
            echo "Unknown option: $1"
            echo "Usage: $0 [--dry-run] [--confidence=HIGH|MEDIUM|ALL]"
            exit 1
            ;;
    esac
done

# Extract actions from results file
extract_actions() {
    local action="$1"
    local confidence="$2"
    
    awk -F'|' -v action="$action" -v conf="$confidence" '
    NR > 25 && NF >= 8 {
        file = $2; gsub(/^[ \t]+|[ \t]+$/, "", file)
        action_field = $6; gsub(/^[ \t]+|[ \t]+$/, "", action_field)
        target_field = $7; gsub(/^[ \t]+|[ \t]+$/, "", target_field)
        conf_field = $8; gsub(/^[ \t]+|[ \t]+$/, "", conf_field)
        
        if (action_field == action && (conf == "ALL" || conf_field == conf)) {
            print file "|" target_field
        }
    }
    ' "$RESULTS_FILE"
}

execute_action() {
    local file="$1"
    local action="$2"
    local target="$3"
    
    if [ ! -f "$file" ]; then
        echo "⚠️  File not found: $file (skipping)"
        return
    fi
    
    case "$action" in
        KEEP_ROOT)
            echo "✓ Keeping in root: $file"
            ;;
        MOVE_DOCS)
            local dest="${target:-docs/}"
            if [ "$DRY_RUN" = true ]; then
                echo "  [DRY RUN] git mv $file $dest"
            else
                echo "  Moving to docs/: $file -> $dest"
                mkdir -p "$dest"
                git mv "$file" "$dest" || echo "  ⚠️  Failed to move $file"
            fi
            ;;
        MOVE_ARCHIVE)
            local dest="${target:-docs/archive/}"
            if [ "$DRY_RUN" = true ]; then
                echo "  [DRY RUN] git mv $file $dest"
            else
                echo "  Archiving: $file -> $dest"
                mkdir -p "$dest"
                git mv "$file" "$dest" || echo "  ⚠️  Failed to archive $file"
            fi
            ;;
        DELETE)
            if [ "$DRY_RUN" = true ]; then
                echo "  [DRY RUN] git rm $file"
            else
                echo "  Deleting: $file"
                git rm "$file" || echo "  ⚠️  Failed to delete $file"
            fi
            ;;
        *)
            echo "  ⚠️  Unknown action: $action for $file"
            ;;
    esac
}

echo "Documentation Audit Execution"
echo "============================"
echo ""
echo "Mode: $([ "$DRY_RUN" = true ] && echo "DRY RUN" || echo "EXECUTE")"
echo "Confidence Filter: $CONFIDENCE_FILTER"
echo ""

# Process HIGH confidence actions first
if [ "$CONFIDENCE_FILTER" = "HIGH" ] || [ "$CONFIDENCE_FILTER" = "ALL" ]; then
    echo "=== HIGH Confidence Actions ==="
    echo ""
    
    # MOVE_DOCS actions
    echo "Moving files to docs/:"
    extract_actions "MOVE_DOCS" "$CONFIDENCE_FILTER" | while IFS='|' read -r file target; do
        [ -n "$file" ] && execute_action "$file" "MOVE_DOCS" "$target"
    done
    echo ""
    
    # MOVE_ARCHIVE actions
    echo "Archiving files:"
    extract_actions "MOVE_ARCHIVE" "$CONFIDENCE_FILTER" | while IFS='|' read -r file target; do
        [ -n "$file" ] && execute_action "$file" "MOVE_ARCHIVE" "$target"
    done
    echo ""
    
    # DELETE actions (only in non-dry-run mode, and only HIGH confidence)
    if [ "$DRY_RUN" = false ] && [ "$CONFIDENCE_FILTER" = "HIGH" ]; then
        echo "⚠️  WARNING: About to delete files. Review carefully!"
        echo "Press Ctrl+C to cancel, or Enter to continue..."
        read -r
        
        echo "Deleting files:"
        extract_actions "DELETE" "HIGH" | while IFS='|' read -r file target; do
            [ -n "$file" ] && execute_action "$file" "DELETE" "$target"
        done
    else
        echo "Files to delete (dry-run or not HIGH confidence - skipping):"
        extract_actions "DELETE" "$CONFIDENCE_FILTER" | while IFS='|' read -r file target; do
            [ -n "$file" ] && echo "  [SKIP] $file"
        done
    fi
    echo ""
fi

# Process MEDIUM confidence actions
if [ "$CONFIDENCE_FILTER" = "MEDIUM" ] || [ "$CONFIDENCE_FILTER" = "ALL" ]; then
    echo "=== MEDIUM Confidence Actions ==="
    echo ""
    echo "⚠️  Review these actions before executing:"
    extract_actions "MOVE_DOCS" "MEDIUM" | while IFS='|' read -r file target; do
        [ -n "$file" ] && echo "  MOVE_DOCS: $file -> ${target:-docs/}"
    done
    extract_actions "MOVE_ARCHIVE" "MEDIUM" | while IFS='|' read -r file target; do
        [ -n "$file" ] && echo "  MOVE_ARCHIVE: $file -> ${target:-docs/archive/}"
    done
    echo ""
fi

echo "=== Summary ==="
echo "Review the audit results in: $RESULTS_FILE"
echo ""
if [ "$DRY_RUN" = true ]; then
    echo "This was a dry run. Run without --dry-run to execute actions."
else
    echo "Actions executed. Review changes with: git status"
fi

