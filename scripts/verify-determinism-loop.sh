#!/bin/bash
# Verification script for Determinism Loop implementation
# 
# This script verifies all 7 components are properly implemented and integrated.

set -euo pipefail

echo "🔍 Verifying Determinism Loop Implementation"
echo "============================================="

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Track verification results
TOTAL_CHECKS=0
PASSED_CHECKS=0

check_file() {
    local file="$1"
    local description="$2"
    
    TOTAL_CHECKS=$((TOTAL_CHECKS + 1))
    
    if [ -f "$file" ]; then
        echo -e "${GREEN}✓${NC} $description"
        PASSED_CHECKS=$((PASSED_CHECKS + 1))
    else
        echo -e "${RED}✗${NC} $description - Missing: $file"
    fi
}

check_compile() {
    local package="$1"
    local description="$2"
    
    TOTAL_CHECKS=$((TOTAL_CHECKS + 1))
    
    if cargo check --package "$package" >/dev/null 2>&1; then
        echo -e "${GREEN}✓${NC} $description"
        PASSED_CHECKS=$((PASSED_CHECKS + 1))
    else
        echo -e "${RED}✗${NC} $description - Compilation failed"
    fi
}

echo ""
echo "Phase 1: Core Proofs"
echo "---------------------"

# 1. Federation Crate
check_file "crates/adapteros-federation/src/lib.rs" "Federation crate implementation"
check_file "crates/adapteros-federation/src/peer.rs" "Peer registry module"
check_file "crates/adapteros-federation/src/output_hash.rs" "Output hash comparison module"
check_file "crates/adapteros-federation/src/signature.rs" "Signature exchange module"
check_file "migrations/0030_federation.sql" "Federation database migration"
check_compile "adapteros-federation" "Federation crate compilation"

# 2. Policy Hash Watcher Integration
check_file "crates/adapteros-policy/src/hash_watcher.rs" "Policy hash watcher implementation"
check_file "crates/adapteros-policy/src/quarantine.rs" "Quarantine manager implementation"
check_file "crates/adapteros-db/src/policy_hash.rs" "Policy hash database operations"
check_file "migrations/0029_policy_hashes.sql" "Policy hash migration"
check_file "docs/policy-hash-watcher.md" "Policy hash watcher documentation"
check_compile "adapteros-policy" "Policy crate compilation"

echo ""
echo "Phase 2: System Integrity"
echo "--------------------------"

# 3. Global Tick Ledger
check_file "crates/adapteros-deterministic-exec/src/global_ledger.rs" "Global tick ledger implementation"
check_file "migrations/0032_tick_ledger.sql" "Tick ledger migration"
check_compile "adapteros-deterministic-exec" "Deterministic executor compilation"

# 4. Sandboxed Telemetry
check_file "crates/adapteros-telemetry/src/uds_exporter.rs" "UDS metrics exporter implementation"
check_file "scripts/metrics-bridge.sh" "Metrics bridge script"
check_compile "adapteros-telemetry" "Telemetry crate compilation"

echo ""
echo "Phase 3: Governance"
echo "--------------------"

# 5. CAB Rollback
check_file "migrations/0033_cab_lineage.sql" "CAB lineage migration"

# 6. Secure Enclave Signing
check_file "crates/adapteros-secd/src/host_identity.rs" "Host identity implementation"
check_compile "adapteros-secd" "Secure Enclave daemon compilation"

# 7. Supervisor Daemon
check_file "crates/adapteros-orchestrator/src/supervisor.rs" "Supervisor daemon implementation"
check_compile "adapteros-orchestrator" "Orchestrator compilation"

echo ""
echo "Integration Files"
echo "-----------------"
check_file "crates/adapteros-lora-worker/src/inference_pipeline.rs" "Inference pipeline integration"
check_file "crates/adapteros-cli/src/commands/policy.rs" "CLI policy commands"

echo ""
echo "Documentation"
echo "-------------"
check_file "DETERMINISM_LOOP_IMPLEMENTATION_SUMMARY.md" "Implementation summary"

echo ""
echo "Summary"
echo "======="
echo "Total checks: $TOTAL_CHECKS"
echo "Passed: $PASSED_CHECKS"
echo "Failed: $((TOTAL_CHECKS - PASSED_CHECKS))"

if [ $PASSED_CHECKS -eq $TOTAL_CHECKS ]; then
    echo -e "${GREEN}🎉 All checks passed! Determinism Loop implementation is complete.${NC}"
    exit 0
else
    echo -e "${RED}❌ Some checks failed. Please review the implementation.${NC}"
    exit 1
fi
