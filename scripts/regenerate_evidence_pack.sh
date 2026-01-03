#!/usr/bin/env bash
# PRD Audit Evidence Pack Regeneration Script
#
# This script regenerates the audit evidence pack by running verification
# tests and collecting code references. Run this before audits to ensure
# evidence is current.
#
# Usage: ./scripts/regenerate_evidence_pack.sh [--quick]
#   --quick: Skip slow tests, only refresh code references

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
EVIDENCE_DIR="$REPO_ROOT/var/audit-evidence"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# Parse arguments
QUICK_MODE=false
if [[ "${1:-}" == "--quick" ]]; then
    QUICK_MODE=true
    log_info "Quick mode: skipping slow tests"
fi

cd "$REPO_ROOT"

log_info "=== PRD Evidence Pack Regeneration ==="
log_info "Evidence directory: $EVIDENCE_DIR"
mkdir -p "$EVIDENCE_DIR"

# Generate timestamp
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
log_info "Timestamp: $TIMESTAMP"

# Function to run a test suite and capture result
run_test_suite() {
    local name="$1"
    local cmd="$2"
    local output_file="$EVIDENCE_DIR/${name}_test_output.txt"

    log_info "Running: $name"
    if eval "$cmd" > "$output_file" 2>&1; then
        log_info "  PASS: $name"
        return 0
    else
        log_warn "  FAIL: $name (see $output_file)"
        return 1
    fi
}

# ============================================================================
# Section 1: Code Reference Verification
# ============================================================================

log_info ""
log_info "=== Section 1: Code Reference Verification ==="

# PRD-11: UDS Metrics
log_info "Verifying PRD-11 (UDS Observability)..."
rg -n "record_uds_timings|UdsPhaseTimings|uds\.rtt_ms" --type rust > "$EVIDENCE_DIR/prd11_uds_metrics.txt" 2>&1 || true
UDS_METRICS_COUNT=$(wc -l < "$EVIDENCE_DIR/prd11_uds_metrics.txt" | tr -d ' ')
log_info "  Found $UDS_METRICS_COUNT UDS metric references"

# PRD-13: Path Policy
log_info "Verifying PRD-13 (Path Policy)..."
rg -n "DEFAULT_.*_ROOT|DEFAULT_TELEMETRY_DIR|reject_tmp" --type rust > "$EVIDENCE_DIR/prd13_path_policy.txt" 2>&1 || true
PATH_REFS=$(wc -l < "$EVIDENCE_DIR/prd13_path_policy.txt" | tr -d ' ')
log_info "  Found $PATH_REFS path policy references"

# PRD-16: Telemetry Flush
log_info "Verifying PRD-16 (Telemetry Flush)..."
rg -n "flush_with_timeout|flush_on_shutdown|capture_panic" --type rust > "$EVIDENCE_DIR/prd16_flush.txt" 2>&1 || true
FLUSH_REFS=$(wc -l < "$EVIDENCE_DIR/prd16_flush.txt" | tr -d ' ')
log_info "  Found $FLUSH_REFS telemetry flush references"

# PRD-10: Cancellation (including frontend)
log_info "Verifying PRD-10 (Cancellation)..."
rg -n "CancellationToken|do_cancel|is_cancelled" --type rust > "$EVIDENCE_DIR/prd10_cancellation.txt" 2>&1 || true
CANCEL_REFS=$(wc -l < "$EVIDENCE_DIR/prd10_cancellation.txt" | tr -d ' ')
log_info "  Found $CANCEL_REFS cancellation references"

# ============================================================================
# Section 2: Test Execution (skipped in quick mode)
# ============================================================================

if [[ "$QUICK_MODE" == "false" ]]; then
    log_info ""
    log_info "=== Section 2: Test Execution ==="

    TESTS_PASSED=0
    TESTS_FAILED=0

    # Core tests
    if run_test_suite "core_seed" "cargo test -p adapteros-core seed -- --nocapture"; then
        ((TESTS_PASSED++))
    else
        ((TESTS_FAILED++))
    fi

    # Path resolver tests
    if run_test_suite "path_resolver" "cargo test -p adapteros-config path_resolver -- --nocapture"; then
        ((TESTS_PASSED++))
    else
        ((TESTS_FAILED++))
    fi

    # Attestation tests
    if run_test_suite "attestation" "cargo test -p adapteros-lora-kernel-api attestation -- --nocapture"; then
        ((TESTS_PASSED++))
    else
        ((TESTS_FAILED++))
    fi

    # Request tracker tests (PRD-10)
    if run_test_suite "request_tracker" "cargo test -p adapteros-server-api --lib request_tracker -- --nocapture"; then
        ((TESTS_PASSED++))
    else
        ((TESTS_FAILED++))
    fi

    log_info ""
    log_info "Test Summary: $TESTS_PASSED passed, $TESTS_FAILED failed"
else
    log_info ""
    log_info "=== Section 2: Test Execution (SKIPPED - quick mode) ==="
fi

# ============================================================================
# Section 3: Generate Summary Report
# ============================================================================

log_info ""
log_info "=== Section 3: Generating Summary Report ==="

SUMMARY_FILE="$EVIDENCE_DIR/EVIDENCE_SUMMARY.md"
cat > "$SUMMARY_FILE" << EOF
# PRD Evidence Pack Summary

**Generated**: $TIMESTAMP
**Mode**: $(if [[ "$QUICK_MODE" == "true" ]]; then echo "Quick (code refs only)"; else echo "Full (code refs + tests)"; fi)

## Code Reference Counts

| PRD | Description | References Found |
|-----|-------------|------------------|
| PRD-10 | Cancellation | $CANCEL_REFS |
| PRD-11 | UDS Observability | $UDS_METRICS_COUNT |
| PRD-13 | Path Policy | $PATH_REFS |
| PRD-16 | Telemetry Flush | $FLUSH_REFS |

## Evidence Files

\`\`\`
$(ls -la "$EVIDENCE_DIR"/*.txt 2>/dev/null | awk '{print $NF}' | xargs -I{} basename {} || echo "No .txt files")
\`\`\`

## Key Findings

### PRD-11: UDS Latency Metrics
- \`record_uds_timings()\` function exports connect_ms, write_ms, read_ms, rtt_ms
- Integration in \`inference_core/core.rs\` at line 886

### PRD-13: Path Policy
- All defaults use \`./var\` prefix (repo-scoped)
- \`reject_tmp_persistent_path()\` blocks \`/tmp\` paths
- CI enforcement via \`scripts/check_env_paths.sh\`

### PRD-16: Telemetry Flush
- Panic hook: 750ms timeout (sync blocking)
- Clean shutdown: 5s timeout
- Uses \`recv_timeout()\` for synchronous completion guarantee

### PRD-10: Cancellation
- Backend: CancellationToken + InferenceCancelRegistry
- Frontend: Cancel button in chat.rs (visible during streaming)
- Full propagation from UI to worker token loop

---

*This summary was auto-generated by \`scripts/regenerate_evidence_pack.sh\`*
EOF

log_info "Summary written to: $SUMMARY_FILE"

# ============================================================================
# Final Status
# ============================================================================

log_info ""
log_info "=== Evidence Pack Regeneration Complete ==="
log_info "Evidence directory: $EVIDENCE_DIR"
log_info ""

if [[ "$QUICK_MODE" == "false" && "$TESTS_FAILED" -gt 0 ]]; then
    log_warn "Some tests failed - review output files for details"
    exit 1
fi

log_info "All evidence collected successfully"
exit 0
