#!/bin/bash

# AI Slop Detector for AdapterOS
# Version: 1.2
# Date: 2025-11-25
# Description: Automated detection of AI slop patterns in AdapterOS codebase

set -euo pipefail
VERSION="1.2"

# Dependency check
if ! command -v jq &> /dev/null; then
    echo "Error: jq is required but not installed. Please install jq first."
    exit 1
fi

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUTPUT_DIR="${SCRIPT_DIR}/ai_slop_reports"
SEARCH_ROOT="${SLOP_SEARCH_ROOT:-crates}"
RUN_ADAPTEROS_LINT="${RUN_ADAPTEROS_LINT:-0}"
RUN_MAKE_DUP="${RUN_MAKE_DUP:-1}"
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

    # Use jq to safely add to JSON, with error handling
    if ! JSON_DATA=$(echo "$JSON_DATA" | jq --arg name "$check_name" \
                                          --arg severity "$severity" \
                                          --argjson count "$count" \
                                          --arg desc "$description" \
                                          '.checks[$name] = {"severity": $severity, "count": $count, "description": $desc}' 2>/dev/null); then
        log_warn "Failed to add check result to JSON for $check_name"
        # Continue without JSON for this check
    fi
}

# Function to run searches with ripgrep if available, fallback to grep
safe_grep() {
    local pattern="$1"
    local include="$2"
    local rg_pattern="${pattern//\\|/|}"
    local rg_excludes=( '!.git' '!target' '!node_modules' '!**/tests/**' '!**/benches/**' '!**/examples/**' '!**/fixtures/**' '!**/mocks/**' )

    if command -v rg >/dev/null 2>&1; then
        local args=()
        for glob in "${rg_excludes[@]}"; do
            args+=("--glob" "$glob")
        done
        rg "$rg_pattern" "$SEARCH_ROOT" --type-add "rust:*.rs" -g "$include" --hidden --no-heading "${args[@]}" || true
    else
        grep -r "$pattern" --include="$include" \
             --exclude-dir="target" \
             --exclude-dir=".git" \
             --exclude-dir="tests" \
             --exclude-dir="benches" \
             --exclude-dir="examples" \
             --exclude-dir="fixtures" \
             --exclude-dir="mocks" \
             --exclude-dir="node_modules" \
             "$SEARCH_ROOT"/ 2>/dev/null || true
    fi
}

exclude_allowlist() {
    local line
    while IFS= read -r line; do
        local skip=0
        for allow in "${@:1}"; do
            case "$line" in
                $allow) skip=1 ;;
            esac
        done
        if [ "$skip" -eq 0 ]; then
            echo "$line"
        fi
    done
}

# Function to count lines safely
count_lines() {
    wc -l | tr -d '[:space:]'
}

RUST_FILE_COUNT=$(find "$SEARCH_ROOT" -type d \( -path '*/target' -o -path '*/node_modules' -o -path '*/tests' -o -path '*/benches' -o -path '*/examples' -o -path '*/fixtures' -o -path '*/mocks' \) -prune -o -name '*.rs' -print | count_lines)
CRATE_COUNT=$(find "$SEARCH_ROOT" -mindepth 1 -maxdepth 1 -type d | count_lines)

log_info "🔍 Starting AI Slop Detection for AdapterOS"
log_info "Report will be saved to: $REPORT_FILE"

# Generate markdown report header
cat > "$REPORT_FILE" << EOF
# AI Slop Detection Report - AdapterOS

**Generated:** $(date)
**Timestamp:** $TIMESTAMP
**Target:** AdapterOS monorepo (${CRATE_COUNT} crates, ${RUST_FILE_COUNT} Rust files scanned; excluding tests/benches/examples/fixtures/mocks)

## Executive Summary

This report analyzes the AdapterOS codebase for AI slop indicators using automated pattern matching and quality heuristics.

---

EOF

# ============================================================================
# CHECK 1: Generic Error Handling (HIGH PRIORITY)
# ============================================================================

log_info "Checking for generic error handling patterns..."

GENERIC_ERRORS=$(
    {
        safe_grep "anyhow::Error\|Box<dyn std::error::Error>" "*.rs" \
        | grep -Ev "://!|:///" \
        | grep -Ev "^[^:]+:[0-9]+:[[:space:]]*//" \
        | grep -Ev "///" \
        | grep -v "^crates/adapteros-cli/" \
        | grep -v "^crates/adapteros-core/src/error.rs";
    } || true
)
GENERIC_ERROR_COUNT=$(printf "%s\n" "$GENERIC_ERRORS" | awk 'NF' | wc -l)

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

# Check for std::thread::spawn calls (should use deterministic spawn)
THREAD_SPAWN=$(safe_grep "std::thread::spawn[[:space:]]*\\(" "*.rs")
THREAD_COUNT=$(printf "%s\n" "$THREAD_SPAWN" | awk 'NF' | wc -l)
if [ "$THREAD_COUNT" -gt 0 ]; then
    PLATFORM_ISSUES="${PLATFORM_ISSUES}Thread spawn issues: $THREAD_COUNT\n"
    PLATFORM_COUNT=$((PLATFORM_COUNT + THREAD_COUNT))
fi

# Check for tokio::spawn calls in deterministic paths
TOKIO_ALLOW=(
    "crates/adapteros-telemetry/src/*"
    "crates/adapteros-system-metrics/src/*"
    "crates/adapteros-storage/src/cleanup.rs:*"
    "crates/adapteros-storage/src/monitor.rs:*"
    "crates/adapteros-service-supervisor/src/*"
    "crates/adapteros-metrics-collector/src/*"
    "crates/adapteros-policy/src/hash_watcher.rs:*"
    "crates/adapteros-crypto/src/rotation_daemon.rs:*"
    "crates/adapteros-crypto/src/providers/keychain.rs:*"
    "crates/adapteros-git/src/subsystem.rs:*"
    "crates/adapteros-server/src/shutdown.rs:*"
    "crates/adapteros-server/src/router_telemetry_consumer.rs:*"
    "crates/adapteros-telemetry/src/metrics/system.rs:*"
    "crates/adapteros-telemetry/src/uds_exporter.rs:*"
)

TOKIO_SPAWN=$(safe_grep "tokio::spawn[[:space:]]*\\(" "*.rs" | exclude_allowlist "${TOKIO_ALLOW[@]}")
TOKIO_COUNT=$(printf "%s\n" "$TOKIO_SPAWN" | awk 'NF' | wc -l)
if [ "$TOKIO_COUNT" -gt 0 ]; then
    PLATFORM_ISSUES="${PLATFORM_ISSUES}tokio::spawn (potentially non-deterministic): $TOKIO_COUNT\n"
    PLATFORM_COUNT=$((PLATFORM_COUNT + TOKIO_COUNT))
fi

# Check for rand::thread_rng calls (should use HKDF)
RAND_THREAD=$(safe_grep "rand::thread_rng[[:space:]]*\\(" "*.rs")
RAND_COUNT=$(printf "%s\n" "$RAND_THREAD" | awk 'NF' | wc -l)
if [ "$RAND_COUNT" -gt 0 ]; then
    PLATFORM_ISSUES="${PLATFORM_ISSUES}Random number issues: $RAND_COUNT\n"
    PLATFORM_COUNT=$((PLATFORM_COUNT + RAND_COUNT))
fi

# Check for StdRng::from_entropy (should use HKDF-derived seeds)
STD_FROM_ENTROPY=$(safe_grep "StdRng::from_entropy[[:space:]]*\\(" "*.rs")
STD_FROM_ENTROPY_COUNT=$(printf "%s\n" "$STD_FROM_ENTROPY" | awk 'NF' | wc -l)
if [ "$STD_FROM_ENTROPY_COUNT" -gt 0 ]; then
    PLATFORM_ISSUES="${PLATFORM_ISSUES}StdRng::from_entropy (missing HKDF seed): $STD_FROM_ENTROPY_COUNT\n"
    PLATFORM_COUNT=$((PLATFORM_COUNT + STD_FROM_ENTROPY_COUNT))
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
# CHECK 3: Code Duplication (INFO)
# ============================================================================

log_info "Checking for code duplication patterns..."

# Look for suspicious repetition patterns in function bodies
# This is a simplified check - real duplication detection would need tools like jscpd
DUPLICATE_PATTERNS=$(safe_grep "pub fn.*\{" "*.rs" | grep -E "(create|build|parse|validate|process|handle)" | sort | uniq -c | sort -nr | awk '$1 > 2 {print $2}' | wc -l)

# Look for copy-paste error patterns (same function name with different casing)
SUSPICIOUS_NAMES=$(safe_grep "pub fn [a-zA-Z_]*" "*.rs" | sed 's/.*pub fn \([a-zA-Z_]*\).*/\1/' | tr '[:upper:]' '[:lower:]' | sort | uniq -c | sort -nr | awk '$1 > 1 {print $2}' | wc -l)

DUPLICATION_COUNT=$((DUPLICATE_PATTERNS + SUSPICIOUS_NAMES))

add_check_result "code_duplication" "INFO" "$DUPLICATION_COUNT" "Potential code duplication or copy-paste patterns"

cat >> "$REPORT_FILE" << EOF

## 🟡 Check 3: Code Duplication (INFO)

**Status:** $([ "$DUPLICATION_COUNT" -gt 0 ] && echo "⚠️ ISSUES FOUND" || echo "✅ CLEAN")

**Count:** $DUPLICATION_COUNT suspicious patterns

**Description:** Look for repeated function names or patterns that suggest copy-paste development instead of proper abstraction.

EOF

if [ "$DUPLICATION_COUNT" -gt 0 ]; then
    echo "**Note:** This check uses simple heuristics. For comprehensive duplication analysis, consider using \`jscpd\` or similar tools." >> "$REPORT_FILE"
fi

echo "" >> "$REPORT_FILE"

# ============================================================================
# CHECK 4: Boilerplate Code (INFO)
# ============================================================================

log_info "Checking for boilerplate code patterns..."

# Look for excessive error handling boilerplate
BOILERPLATE_ERRORS=$(safe_grep "map_err.*format!" "*.rs" | wc -l)

# Look for repetitive validation patterns
VALIDATION_PATTERNS=$(safe_grep "if.*is_empty\|if.*is_none\|if.*len.*==.*0" "*.rs" | wc -l)

# Boilerplate logging check removed (noise); focus on error/validation patterns

BOILERPLATE_COUNT=$((BOILERPLATE_ERRORS + VALIDATION_PATTERNS))

add_check_result "boilerplate_code" "INFO" "$BOILERPLATE_COUNT" "Excessive boilerplate suggesting lack of helper functions or abstractions"

cat >> "$REPORT_FILE" << EOF

## 🟡 Check 4: Boilerplate Code (INFO)

**Status:** $([ "$BOILERPLATE_COUNT" -gt 50 ] && echo "⚠️ ISSUES FOUND" || echo "⚠️ NEEDS REVIEW")

**Count:** $BOILERPLATE_COUNT instances

**Description:** Excessive boilerplate code suggests missing abstractions or helper functions. Look for opportunities to extract common patterns.

**Breakdown:**
- Error mapping patterns: $BOILERPLATE_ERRORS
- Validation patterns: $VALIDATION_PATTERNS

EOF

echo "" >> "$REPORT_FILE"

# ============================================================================
# CHECK 5: Incomplete Code Markers (INFO)
# ============================================================================

log_info "Checking for incomplete code markers..."

TODO_COMMENTS=$(safe_grep "TODO\|FIXME\|XXX\|HACK\|NOTE:" "*.rs")
TODO_COUNT=$(echo "$TODO_COMMENTS" | grep -c "^")

add_check_result "incomplete_code" "INFO" "$TODO_COUNT" "Incomplete code markers indicating unfinished work"

cat >> "$REPORT_FILE" << EOF

## 🟢 Check 5: Incomplete Code Markers (INFO)

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

# Calculate overall score (only high-priority affects status/exit)
HIGH_PRIORITY=$((GENERIC_ERROR_COUNT + PLATFORM_COUNT))
INFO_PRIORITY=$((DUPLICATION_COUNT + BOILERPLATE_COUNT + TODO_COUNT))
TOTAL_ISSUES=$((HIGH_PRIORITY + INFO_PRIORITY))

# Update JSON summary (with error handling)
if ! JSON_DATA=$(echo "$JSON_DATA" | jq --argjson total "$TOTAL_ISSUES" '.summary.total_issues = $total' 2>/dev/null); then
    log_warn "Failed to update JSON summary"
fi

if ! JSON_DATA=$(echo "$JSON_DATA" | jq --argjson high "$HIGH_PRIORITY" --argjson info "$INFO_PRIORITY" \
                                      '.summary += {"high_priority": $high, "info_priority": $info}' 2>/dev/null); then
    log_warn "Failed to update JSON priority counts"
fi

# Determine overall status
if [ "$HIGH_PRIORITY" -gt 0 ]; then
    OVERALL_STATUS="🔴 CRITICAL - High-priority issues require immediate attention"
else
    OVERALL_STATUS="🟢 GOOD - No high-priority issues detected"
fi

cat >> "$REPORT_FILE" << EOF

## 📊 Summary & Recommendations

### **Overall Assessment:** $OVERALL_STATUS

### **Issue Breakdown:**
- **🔴 High Priority:** $HIGH_PRIORITY issues (Generic errors, platform patterns)
- **ℹ️ Informational:** $INFO_PRIORITY signals (Duplication heuristics, boilerplate, TODOs)

### **Total Issues Found:** $TOTAL_ISSUES

### **Recommended Actions:**

#### **Immediate (High Priority):**
$(if [ "$GENERIC_ERROR_COUNT" -gt 0 ]; then echo "- Replace generic error types with \`AosError\` variants"; fi)
$(if [ "$PLATFORM_COUNT" -gt 0 ]; then echo "- Update platform-agnostic code to use AdapterOS patterns"; fi)

#### **Informational (noise-prone):**
$(if [ "$DUPLICATION_COUNT" -gt 0 ]; then echo "- For duplication, prefer \`make dup\` or \`adapteros-lint\` for authoritative signal"; fi)
$(if [ "$BOILERPLATE_COUNT" -gt 50 ]; then echo "- Consider extracting repeated patterns; counts are heuristic"; fi)
$(if [ "$TODO_COUNT" -gt 20 ]; then echo "- Resolve TODO/FIXME comments or create implementation plans"; fi)

### **Quality Metrics (informational only):**
- **Error Handling:** $([ "$GENERIC_ERROR_COUNT" -eq 0 ] && echo "✅ Excellent" || echo "⚠️ Needs refactoring")
- **Platform Awareness:** $([ "$PLATFORM_COUNT" -eq 0 ] && echo "✅ Excellent" || echo "⚠️ Critical fixes needed")
- **Duplication/Boilerplate:** Heuristic; prefer \`make dup\`/\`adapteros-lint\` for authoritative checks

---

**Report Generated:** $(date)
**Detection Script:** ai_slop_detector.sh v${VERSION:-1.2}
**Coverage:** ${RUST_FILE_COUNT} Rust files under crates/ across ${CRATE_COUNT} top-level crates

EOF

# Save JSON report
echo "$JSON_DATA" | jq '.' > "$JSON_REPORT"

# Optional authoritative checks (disabled by default; enable via env)
if [ "$RUN_ADAPTEROS_LINT" -eq 1 ] && command -v adapteros-lint >/dev/null 2>&1; then
    log_info "Running adapteros-lint (authoritative lint)..."
    if ! adapteros-lint; then
        log_warn "adapteros-lint reported issues"
    else
        log_success "adapteros-lint completed without reported issues"
    fi
fi

if [ "$RUN_MAKE_DUP" -eq 1 ]; then
    log_info "Running make dup (duplication check)..."
    if ! make dup; then
        log_warn "make dup reported duplication issues"
    else
        log_success "make dup completed without reported issues"
    fi
fi

log_success "AI Slop detection complete!"
log_info "Markdown report: $REPORT_FILE"
log_info "JSON data: $JSON_REPORT"

# Display summary on console
echo ""
echo "🎯 AI Slop Detection Summary:"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "High Priority Issues: $HIGH_PRIORITY"
echo "Informational Signals: $INFO_PRIORITY"
echo "Total Signals: $TOTAL_ISSUES"
echo ""
echo "Checks Performed:"
echo "• Generic Error Handling: $GENERIC_ERROR_COUNT"
echo "• Platform Patterns: $PLATFORM_COUNT"
echo "• Code Duplication: $DUPLICATION_COUNT"
echo "• Boilerplate Code: $BOILERPLATE_COUNT"
echo "• TODO Comments: $TODO_COUNT"
echo ""
echo "Status: $OVERALL_STATUS"
echo ""
echo "Full report: $REPORT_FILE"

# Exit with appropriate code
if [ "$HIGH_PRIORITY" -gt 0 ]; then
    exit 1
else
    exit 0
fi
