#!/bin/bash
# List root-level markdown files grouped by pattern for batch review
# Usage: ./scripts/list_root_docs.sh [pattern]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$REPO_ROOT"

# Function to list files matching a pattern
list_pattern() {
    local pattern="$1"
    local description="$2"
    local files
    
    files=$(find . -maxdepth 1 -name "$pattern" -type f | sed 's|^\./||' | sort)
    
    if [ -n "$files" ]; then
        echo ""
        echo "=== $description ==="
        echo "$files" | while read -r file; do
            echo "  - $file"
        done
        echo ""
    fi
}

# If pattern provided, just list that pattern
if [ $# -gt 0 ]; then
    pattern="$1"
    find . -maxdepth 1 -name "$pattern" -type f | sed 's|^\./||' | sort
    exit 0
fi

# List all root markdown files grouped by pattern
echo "Root-Level Documentation Files"
echo "================================"
echo ""
echo "Total files: $(find . -maxdepth 1 -name "*.md" -type f | wc -l | tr -d ' ')"
echo ""

# Core files (likely keep in root)
echo "=== CORE FILES (Likely Keep in Root) ==="
for file in README.md CONTRIBUTING.md SECURITY.md LICENSE LICENSE-APACHE LICENSE-MIT CHANGELOG.md CLAUDE.md AGENTS.md CITATIONS.md QUICKSTART.md QUICKSTART_GPU_TRAINING.md PRD.md CODE_OF_CONDUCT.md; do
    if [ -f "$file" ]; then
        echo "  - $file"
    fi
done
echo ""

# Fix-related files (likely ephemeral)
list_pattern "*_FIXES_*.md" "FIXES FILES (Likely Ephemeral)"
list_pattern "*_FIX_*.md" "FIX FILES (Likely Ephemeral)"
list_pattern "*_FIX*.md" "FIX VARIATIONS (Likely Ephemeral)"

# Summary files (likely ephemeral)
list_pattern "*_SUMMARY.md" "SUMMARY FILES (Likely Ephemeral)"
list_pattern "*_REPORT.md" "REPORT FILES (Likely Ephemeral)"

# Implementation files
list_pattern "*_IMPLEMENTATION*.md" "IMPLEMENTATION FILES"

# Checklist files (likely ephemeral)
list_pattern "*_CHECKLIST.md" "CHECKLIST FILES (Likely Ephemeral)"

# Analysis/Audit files (likely ephemeral)
list_pattern "*_ANALYSIS.md" "ANALYSIS FILES (Likely Ephemeral)"
list_pattern "*_AUDIT.md" "AUDIT FILES (Likely Ephemeral)"

# MLX integration files (likely move to docs/)
list_pattern "MLX_*.md" "MLX INTEGRATION FILES (Likely Move to docs/)"

# Benchmark files (likely move to docs/)
list_pattern "BENCHMARK*.md" "BENCHMARK FILES (Likely Move to docs/)"

# Error handling files (likely move to docs/)
list_pattern "ERROR_*.md" "ERROR HANDLING FILES (Likely Move to docs/)"

# SQLX migration files
list_pattern "SQLX_*.md" "SQLX MIGRATION FILES"

# Training files
list_pattern "TRAINING_*.md" "TRAINING FILES"

# UI files
list_pattern "UI_*.md" "UI FILES"

# Auth files
list_pattern "AUTH_*.md" "AUTH FILES"

# Policy files
list_pattern "POLICY_*.md" "POLICY FILES"

# Security files
list_pattern "SECURITY_*.md" "SECURITY FILES (Excluding SECURITY.md)"

# Database files
list_pattern "DATABASE_*.md" "DATABASE FILES"

# Remaining files (catch-all)
echo "=== REMAINING FILES ==="
find . -maxdepth 1 -name "*.md" -type f | sed 's|^\./||' | sort | while read -r file; do
    # Skip already categorized files
    if [[ ! "$file" =~ ^(README|CONTRIBUTING|SECURITY|LICENSE|CHANGELOG|CLAUDE|AGENTS|CITATIONS|QUICKSTART|PRD|CODE_OF_CONDUCT)\.md$ ]] && \
       [[ ! "$file" =~ _(FIXES?|SUMMARY|REPORT|IMPLEMENTATION|CHECKLIST|ANALYSIS|AUDIT)\.md$ ]] && \
       [[ ! "$file" =~ ^(MLX_|BENCHMARK|ERROR_|SQLX_|TRAINING_|UI_|AUTH_|POLICY_|SECURITY_|DATABASE_) ]]; then
        echo "  - $file"
    fi
done
echo ""

echo "=== USAGE ==="
echo "Review files in batches using the prompt in docs/DOCUMENTATION_AUDIT_PROMPT.md"
echo "Track results in docs/DOCUMENTATION_AUDIT_RESULTS.md"
echo ""
echo "Example batch review:"
echo "  ./scripts/list_root_docs.sh '*_FIXES_*.md' | xargs -I {} echo 'Reviewing: {}'"

