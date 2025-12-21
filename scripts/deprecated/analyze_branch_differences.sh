#!/bin/bash

# Branch Difference Analysis Script
# Analyzes commits in branch A that are not in branch B
# Usage: ./analyze_branch_differences.sh <branch_a> <branch_b>

set -euo pipefail

BRANCH_A="${1:-consolidated-integration}"
BRANCH_B="${2:-main}"
OUTPUT_FILE="${3:-feature_inventory.md}"

echo "# Feature Inventory: $BRANCH_A vs $BRANCH_B"
echo ""
echo "**Generated:** $(date)"
echo "**Analysis:** Commits in $BRANCH_A not present in $BRANCH_B"
echo ""

echo "## 📊 Summary Statistics"
echo ""

# Count total commits
TOTAL_COMMITS=$(git rev-list --count $BRANCH_A ^$BRANCH_B)
echo "- **Total commits to reconcile:** $TOTAL_COMMITS"

# Count by author (if available)
echo ""
echo "## 👥 Commit Analysis"
echo ""

# Get commits with details
echo "### Recent Commits (Last 50):"
echo ""
git log --oneline -50 $BRANCH_A ^$BRANCH_B | while read -r line; do
    commit_hash=$(echo "$line" | cut -d' ' -f1)
    commit_msg=$(echo "$line" | cut -d' ' -f2-)

    # Extract PRD references
    prd_refs=$(echo "$commit_msg" | grep -o 'PRD-[0-9]\+' || true)

    # Categorize by type
    if echo "$commit_msg" | grep -qi "^feat:"; then
        category="🚀 FEATURE"
    elif echo "$commit_msg" | grep -qi "^fix:"; then
        category="🔧 FIX"
    elif echo "$commit_msg" | grep -qi "^docs:"; then
        category="📚 DOCS"
    elif echo "$commit_msg" | grep -qi "^test:"; then
        category="🧪 TEST"
    elif echo "$commit_msg" | grep -qi "^refactor:"; then
        category="♻️ REFACTOR"
    elif echo "$commit_msg" | grep -qi "^chore:"; then
        category="⚙️ CHORE"
    else
        category="📝 OTHER"
    fi

    echo "- **$category** $commit_msg"
    if [ -n "$prd_refs" ]; then
        echo "  - *PRD References:* $prd_refs"
    fi
    echo "  - *Hash:* $commit_hash"
    echo ""
done

echo "## 🎯 Feature Categories Identified"
echo ""

# Analyze feature categories
FEATURES=$(git log --oneline $BRANCH_A ^$BRANCH_B | grep -i "^feat:" | wc -l)
FIXES=$(git log --oneline $BRANCH_A ^$BRANCH_B | grep -i "^fix:" | wc -l)
DOCS=$(git log --oneline $BRANCH_A ^$BRANCH_B | grep -i "^docs:" | wc -l)
TESTS=$(git log --oneline $BRANCH_A ^$BRANCH_B | grep -i "^test:" | wc -l)

echo "- **Features:** $FEATURES commits"
echo "- **Fixes:** $FIXES commits"
echo "- **Documentation:** $DOCS commits"
echo "- **Tests:** $TESTS commits"

echo ""
echo "## 🔗 PRD References Found"
echo ""

# Extract all PRD references
git log --oneline $BRANCH_A ^$BRANCH_B | grep -o 'PRD-[0-9]\+' | sort | uniq -c | sort -nr | while read -r count prd; do
    echo "- **$prd:** $count commits"
done

echo ""
echo "## 📁 Files Impacted"
echo ""

# Show most frequently modified files
echo "### Top Modified Files:"
echo ""
git log --name-only --oneline $BRANCH_A ^$BRANCH_B | grep -v "^[a-f0-9]\+" | sort | uniq -c | sort -nr | head -20 | while read -r count file; do
    echo "- **$file:** $count changes"
done

echo ""
echo "## ⚠️ Conflict Risk Assessment"
echo ""

# High-risk files (likely to conflict)
echo "### High-Risk Files (API/Handlers/Schema):"
echo ""
git log --name-only --oneline $BRANCH_A ^$BRANCH_B | grep -E "\.(rs|toml|sql)$" | grep -E "(handler|api|schema|migration)" | sort | uniq -c | sort -nr | head -10 | while read -r count file; do
    echo "- **$file:** $count modifications"
done

echo ""
echo "## 📋 Reconciliation Priority Recommendations"
echo ""

# Based on PRD references and commit analysis
PRD_COMMITS=$(git log --oneline $BRANCH_A ^$BRANCH_B | grep -c "PRD-" || echo "0")
ROUTER_COMMITS=$(git log --oneline $BRANCH_A ^$BRANCH_B | grep -c "router\|Router" || echo "0")
API_COMMITS=$(git log --oneline $BRANCH_A ^$BRANCH_B | grep -c "api\|API" || echo "0")

echo "### Priority 1 (Foundation):"
if [ "$PRD_COMMITS" -gt 0 ]; then
    echo "- PRD implementations ($PRD_COMMITS commits)"
fi
if [ "$ROUTER_COMMITS" -gt 0 ]; then
    echo "- Router/Kernel changes ($ROUTER_COMMITS commits)"
fi

echo ""
echo "### Priority 2 (Core Services):"
if [ "$API_COMMITS" -gt 0 ]; then
    echo "- API and handler modifications ($API_COMMITS commits)"
fi

echo ""
echo "### Priority 3 (Supporting Features):"
echo "- UI enhancements and testing improvements"
echo "- Documentation and tooling updates"

echo ""
echo "## 🎯 Next Steps"
echo ""
echo "1. **Review high-risk files** for merge conflicts"
echo "2. **Prioritize by PRD dependencies** (foundation first)"
echo "3. **Test incrementally** after each priority group"
echo "4. **Document all conflict resolutions** with citations"
echo ""
echo "**Citation:** 【2025-11-20†reconciliation†feature-inventory】"

