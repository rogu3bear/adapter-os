#!/bin/bash

# AI Slop Detector for AdapterOS
# Version: 1.0
# Date: 2025-11-20
# Description: Automated detection of AI slop patterns in AdapterOS codebase

set -euo pipefail

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUTPUT_DIR="${SCRIPT_DIR}/ai_slop_reports"
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
REPORT_FILE="${OUTPUT_DIR}/ai_slop_report_${TIMESTAMP}.md"
JSON_REPORT="${OUTPUT_DIR}/ai_slop_data_${TIMESTAMP}.json"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Logging functions
log_info() { echo -e "${BLUE}[INFO]${NC} $1" >&2; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1" >&2; }
log_error() { echo -e "${RED}[ERROR]${NC} $1" >&2; }
log_success() { echo -e "${GREEN}[SUCCESS]${NC} $1" >&2; }

# Initialize JSON data structure
JSON_DATA='{"timestamp":"'"$TIMESTAMP"'","summary":{},"checks":{}}'

# Function to add check result to JSON
add_check_result() {
    local check_name="$1"
    local severity="$2"
    local count="$3"
    local description="$4"

    # Add to JSON
    JSON_DATA=$(echo "$JSON_DATA" | jq --arg name "$check_name" \
                                      --arg severity "$severity" \
                                      --argjson count "$count" \
                                      --arg desc "$description" \
                                      '.checks += {($name): {"severity": $severity, "count": $count, "description": $desc}}')
}

# Function to run grep checks with proper error handling
safe_grep() {
    local pattern="$1"
    local include="$2"
    local exclude="${3:-}"

    if [ -n "$exclude" ]; then
        grep -r "$pattern" --include="$include" "$exclude" crates/ 2>/dev/null || true
    else
        grep -r "$pattern" --include="$include" crates/ 2>/dev/null || true
    fi
}

log_info "🔍 Starting AI Slop Detection for AdapterOS"
log_info "Report will be saved to: $REPORT_FILE"

# Generate markdown report header
cat > "$REPORT_FILE" << EOF
# AI Slop Detection Report - AdapterOS

**Generated:** $(date)
**Timestamp:** $TIMESTAMP
**Target:** AdapterOS monorepo (69 crates, 864+ files)

## Executive Summary

This report analyzes the AdapterOS codebase for AI slop indicators using automated pattern matching and quality heuristics.

---

EOF

# ============================================================================
# CHECK 1: Generic Error Handling (HIGH PRIORITY)
# ============================================================================

log_info "Checking for generic error handling patterns..."

GENERIC_ERRORS=$(safe_grep "anyhow::Error\|Box<dyn std::error::Error>" "*.rs")
GENERIC_ERROR_COUNT=$(echo "$GENERIC_ERRORS" | grep -v "^$" | wc -l)

add_check_result "generic_errors" "HIGH" "$GENERIC_ERROR_COUNT" "Generic error types instead of domain-specific AosError"

cat >> "$REPORT_FILE" << EOF

## 🔴 Check 1: Generic Error Handling (HIGH PRIORITY)

**Status:** $([ "$GENERIC_ERROR_COUNT" -gt 0 ] && echo "⚠️ ISSUES FOUND" || echo "✅ CLEAN")

**Count:** $GENERIC_ERROR_COUNT instances

**Description:** Code should use domain-specific \`AosError\` variants instead of generic error types.

EOF

if [ "$GENERIC_ERROR_COUNT" -gt 0 ]; then
    echo "**Found Issues:**" >> "$REPORT_FILE"
    echo "$GENERIC_ERRORS" | head -20 | sed 's/^/- /' >> "$REPORT_FILE"
    [ "$GENERIC_ERROR_COUNT" -gt 20 ] && echo "- ... and $((GENERIC_ERROR_COUNT - 20)) more instances" >> "$REPORT_FILE"
fi

echo "" >> "$REPORT_FILE"

# ============================================================================
# CHECK 2: Platform-Agnostic Patterns (HIGH PRIORITY)
# ============================================================================

log_info "Checking for platform-agnostic patterns..."

PLATFORM_ISSUES=""
PLATFORM_COUNT=0

# Check for std::thread::spawn (should use deterministic spawn)
THREAD_SPAWN=$(safe_grep "std::thread::spawn" "*.rs")
THREAD_COUNT=$(echo "$THREAD_SPAWN" | grep -v "^$" | wc -l)
if [ "$THREAD_COUNT" -gt 0 ]; then
    PLATFORM_ISSUES="${PLATFORM_ISSUES}Thread spawn issues: $THREAD_COUNT\n"
    PLATFORM_COUNT=$((PLATFORM_COUNT + THREAD_COUNT))
fi

# Check for rand::thread_rng (should use HKDF)
RAND_THREAD=$(safe_grep "rand::thread_rng" "*.rs")
RAND_COUNT=$(echo "$RAND_THREAD" | grep -v "^$" | wc -l)
if [ "$RAND_COUNT" -gt 0 ]; then
    PLATFORM_ISSUES="${PLATFORM_ISSUES}Random number issues: $RAND_COUNT\n"
    PLATFORM_COUNT=$((PLATFORM_COUNT + RAND_COUNT))
fi

add_check_result "platform_agnostic" "HIGH" "$PLATFORM_COUNT" "Platform-agnostic patterns that ignore AdapterOS deterministic requirements"

cat >> "$REPORT_FILE" << EOF

## 🔴 Check 2: Platform-Agnostic Patterns (HIGH PRIORITY)

**Status:** $([ "$PLATFORM_COUNT" -gt 0 ] && echo "⚠️ ISSUES FOUND" || echo "✅ CLEAN")

**Count:** $PLATFORM_COUNT instances

**Description:** Code should use AdapterOS-specific patterns (deterministic spawn, HKDF seeding) instead of generic platform APIs.

EOF

if [ "$PLATFORM_COUNT" -gt 0 ]; then
    echo "**Issues by Category:**" >> "$REPORT_FILE"
    echo -e "$PLATFORM_ISSUES" | sed 's/^/- /' >> "$REPORT_FILE"
fi

echo "" >> "$REPORT_FILE"

# ============================================================================
# CHECK 3: Generic Variable Names (MEDIUM PRIORITY)
# ============================================================================

log_info "Checking for generic variable names..."

GENERIC_VARS=$(safe_grep "\bdata\b\|\bresult\b\|\bvalue\b\|\bitem\b\|\binput\b\|\boutput\b" "*.rs" | grep -v "match\|enum\|struct\|fn")
GENERIC_VAR_COUNT=$(echo "$GENERIC_VARS" | grep -v "^$" | wc -l)

add_check_result "generic_variables" "MEDIUM" "$GENERIC_VAR_COUNT" "Generic variable names lacking domain specificity"

cat >> "$REPORT_FILE" << EOF

## 🟡 Check 3: Generic Variable Names (MEDIUM PRIORITY)

**Status:** $([ "$GENERIC_VAR_COUNT" -gt 10 ] && echo "⚠️ ISSUES FOUND" || echo "✅ CLEAN")

**Count:** $GENERIC_VAR_COUNT instances

**Description:** Variables should have domain-specific names (e.g., \`adapter_weights\` instead of \`data\`).

EOF

if [ "$GENERIC_VAR_COUNT" -gt 10 ]; then
    echo "**Sample Issues:**" >> "$REPORT_FILE"
    echo "$GENERIC_VARS" | head -10 | sed 's/^/- /' >> "$REPORT_FILE"
    echo "- ... and $((GENERIC_VAR_COUNT - 10)) more instances" >> "$REPORT_FILE"
fi

echo "" >> "$REPORT_FILE"

# ============================================================================
# CHECK 4: Repetitive Patterns (MEDIUM PRIORITY)
# ============================================================================

log_info "Checking for repetitive async function patterns..."

ASYNC_FUNCTIONS=$(safe_grep "pub async fn.*-> Result<.*>" "*.rs")
REPETITIVE_COUNT=$(echo "$ASYNC_FUNCTIONS" | grep -v "^$" | wc -l)

# Find patterns that appear more than 3 times
REPETITIVE_PATTERNS=$(echo "$ASYNC_FUNCTIONS" | sed 's/.*pub async fn \([a-zA-Z_][a-zA-Z0-9_]*\).*/\1/' | sort | uniq -c | sort -nr | awk '$1 > 3 {print $2 ": " $1 " times"}' | wc -l)

add_check_result "repetitive_patterns" "MEDIUM" "$REPETITIVE_PATTERNS" "Repetitive function patterns suggesting template reuse"

cat >> "$REPORT_FILE" << EOF

## 🟡 Check 4: Repetitive Patterns (MEDIUM PRIORITY)

**Status:** $([ "$REPETITIVE_PATTERNS" -gt 0 ] && echo "⚠️ ISSUES FOUND" || echo "✅ CLEAN")

**Count:** $REPETITIVE_PATTERNS repetitive patterns

**Description:** Avoid repetitive function signatures and implementations that suggest copy-paste or template reuse.

EOF

if [ "$REPETITIVE_PATTERNS" -gt 0 ]; then
    echo "**Most Common Patterns:**" >> "$REPORT_FILE"
    echo "$ASYNC_FUNCTIONS" | sed 's/.*pub async fn \([a-zA-Z_][a-zA-Z0-9_]*\).*/\1/' | sort | uniq -c | sort -nr | head -5 | sed 's/^/- /' >> "$REPORT_FILE"
fi

echo "" >> "$REPORT_FILE"

# ============================================================================
# CHECK 5: Missing Domain Context (MEDIUM PRIORITY)
# ============================================================================

log_info "Checking for missing domain context..."

MISSING_POLICY=$(safe_grep "policy\|Policy" "*.rs" | grep -v "AosError::PolicyViolation\|adapteros-policy\|PolicyId" | wc -l)
MISSING_ADAPTER=$(safe_grep "adapter\|Adapter" "*.rs" | grep -v "AosError\|AdapterId\|adapteros-" | wc -l)
MISSING_TENANT=$(safe_grep "tenant\|Tenant" "*.rs" | grep -v "AosError\|TenantId\|tenant_id" | wc -l)

CONTEXT_COUNT=$((MISSING_POLICY + MISSING_ADAPTER + MISSING_TENANT))

add_check_result "missing_context" "MEDIUM" "$CONTEXT_COUNT" "Generic references to domain concepts without specific AdapterOS context"

cat >> "$REPORT_FILE" << EOF

## 🟡 Check 5: Missing Domain Context (MEDIUM PRIORITY)

**Status:** $([ "$CONTEXT_COUNT" -gt 50 ] && echo "⚠️ ISSUES FOUND" || echo "✅ CLEAN")

**Count:** $CONTEXT_COUNT instances

**Description:** References to core concepts (policies, adapters, tenants) should include specific AdapterOS context and error types.

**Breakdown:**
- Generic policy references: $MISSING_POLICY
- Generic adapter references: $MISSING_ADAPTER
- Generic tenant references: $MISSING_TENANT

EOF

echo "" >> "$REPORT_FILE"

# ============================================================================
# CHECK 6: TODO/FIXME Comments (LOW PRIORITY)
# ============================================================================

log_info "Checking for incomplete code markers..."

TODO_COMMENTS=$(safe_grep "TODO\|FIXME\|XXX\|HACK\|NOTE:" "*.rs")
TODO_COUNT=$(echo "$TODO_COMMENTS" | grep -v "^$" | wc -l)

add_check_result "incomplete_code" "LOW" "$TODO_COUNT" "Incomplete code markers indicating unfinished work"

cat >> "$REPORT_FILE" << EOF

## 🟢 Check 6: Incomplete Code Markers (LOW PRIORITY)

**Status:** $([ "$TODO_COUNT" -gt 20 ] && echo "⚠️ ISSUES FOUND" || echo "✅ CLEAN")

**Count:** $TODO_COUNT instances

**Description:** TODO/FIXME comments should be resolved or converted to proper implementation plans.

EOF

if [ "$TODO_COUNT" -gt 20 ]; then
    echo "**Sample Issues:**" >> "$REPORT_FILE"
    echo "$TODO_COMMENTS" | head -10 | sed 's/^/- /' >> "$REPORT_FILE"
    echo "- ... and $((TODO_COUNT - 10)) more instances" >> "$REPORT_FILE"
fi

echo "" >> "$REPORT_FILE"

# ============================================================================
# SUMMARY AND RECOMMENDATIONS
# ============================================================================

# Calculate overall score
TOTAL_ISSUES=$((GENERIC_ERROR_COUNT + PLATFORM_COUNT + GENERIC_VAR_COUNT + REPETITIVE_PATTERNS + CONTEXT_COUNT + TODO_COUNT))

# Update JSON summary
JSON_DATA=$(echo "$JSON_DATA" | jq --argjson total "$TOTAL_ISSUES" '.summary.total_issues = $total')

HIGH_PRIORITY=$((GENERIC_ERROR_COUNT + PLATFORM_COUNT))
MEDIUM_PRIORITY=$((GENERIC_VAR_COUNT + REPETITIVE_PATTERNS + CONTEXT_COUNT))
LOW_PRIORITY=$TODO_COUNT

JSON_DATA=$(echo "$JSON_DATA" | jq --argjson high "$HIGH_PRIORITY" --argjson med "$MEDIUM_PRIORITY" --argjson low "$LOW_PRIORITY" \
                                  '.summary += {"high_priority": $high, "medium_priority": $med, "low_priority": $low}')

# Determine overall status
if [ "$HIGH_PRIORITY" -gt 0 ]; then
    OVERALL_STATUS="🔴 CRITICAL - High-priority issues require immediate attention"
elif [ "$MEDIUM_PRIORITY" -gt 20 ]; then
    OVERALL_STATUS="🟡 WARNING - Medium-priority issues should be addressed"
else
    OVERALL_STATUS="🟢 GOOD - Codebase appears clean of major AI slop indicators"
fi

cat >> "$REPORT_FILE" << EOF

## 📊 Summary & Recommendations

### **Overall Assessment:** $OVERALL_STATUS

### **Issue Breakdown:**
- **🔴 High Priority:** $HIGH_PRIORITY issues (Generic errors, platform patterns)
- **🟡 Medium Priority:** $MEDIUM_PRIORITY issues (Naming, patterns, context)
- **🟢 Low Priority:** $LOW_PRIORITY issues (Incomplete markers)

### **Total Issues Found:** $TOTAL_ISSUES

### **Recommended Actions:**

#### **Immediate (High Priority):**
$(if [ "$GENERIC_ERROR_COUNT" -gt 0 ]; then echo "- Replace generic error types with \`AosError\` variants"; fi)
$(if [ "$PLATFORM_COUNT" -gt 0 ]; then echo "- Update platform-agnostic code to use AdapterOS patterns"; fi)

#### **Short-term (Medium Priority):**
$(if [ "$GENERIC_VAR_COUNT" -gt 10 ]; then echo "- Rename generic variables with domain-specific names"; fi)
$(if [ "$REPETITIVE_PATTERNS" -gt 0 ]; then echo "- Extract common patterns into shared utilities"; fi)
$(if [ "$CONTEXT_COUNT" -gt 50 ]; then echo "- Add specific AdapterOS context to domain references"; fi)

#### **Ongoing (Low Priority):**
$(if [ "$TODO_COUNT" -gt 20 ]; then echo "- Resolve TODO/FIXME comments or create implementation plans"; fi)

### **Quality Metrics:**
- **Domain Specificity:** $([ "$CONTEXT_COUNT" -lt 50 ] && echo "✅ Good" || echo "⚠️ Needs improvement")
- **Error Handling:** $([ "$GENERIC_ERROR_COUNT" -eq 0 ] && echo "✅ Excellent" || echo "⚠️ Needs refactoring")
- **Platform Awareness:** $([ "$PLATFORM_COUNT" -eq 0 ] && echo "✅ Excellent" || echo "⚠️ Critical fixes needed")

---

**Report Generated:** $(date)
**Detection Script:** ai_slop_detector.sh v1.0
**Coverage:** All 864 Rust files in crates/ directory

EOF

# Save JSON report
echo "$JSON_DATA" | jq '.' > "$JSON_REPORT"

log_success "AI Slop detection complete!"
log_info "Markdown report: $REPORT_FILE"
log_info "JSON data: $JSON_REPORT"

# Display summary on console
echo ""
echo "🎯 AI Slop Detection Summary:"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "High Priority Issues: $HIGH_PRIORITY"
echo "Medium Priority Issues: $MEDIUM_PRIORITY"
echo "Low Priority Issues: $LOW_PRIORITY"
echo "Total Issues: $TOTAL_ISSUES"
echo ""
echo "Status: $OVERALL_STATUS"
echo ""
echo "Full report: $REPORT_FILE"

# Exit with appropriate code
if [ "$HIGH_PRIORITY" -gt 0 ]; then
    exit 1
elif [ "$MEDIUM_PRIORITY" -gt 20 ]; then
    exit 1
else
    exit 0
fi
