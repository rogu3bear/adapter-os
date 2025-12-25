#!/usr/bin/env bash
# check_moe_lora_naming.sh - Fast grep-based naming convention checker
#
# Enforces naming conventions:
#   - MoE (not Moe) - Mixture of Experts
#   - LoRA (not Lora) - Low-Rank Adaptation
#   - Flags ambiguous compute_*gating patterns
#
# Uses a baseline to allow existing violations while failing on NEW violations.
# Update BASELINE_VIOLATIONS when fixing existing violations.
#
# Usage:
#   bash scripts/check_moe_lora_naming.sh
#
# Exit codes:
#   0 - No new violations (count <= baseline)
#   1 - New violations found (count > baseline)

set -euo pipefail

# Baseline: known existing violations as of 2025-12-25
# Update this number DOWN as violations are fixed
BASELINE_VIOLATIONS=298

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

violations=0

echo "Checking MoE/LoRA naming conventions..."
echo "========================================"
echo "Baseline: $BASELINE_VIOLATIONS known violations"

# Check 1: Moe should be MoE
# No word boundary - catches mid-word occurrences like QuantizedMoeConfig
echo ""
echo "[1/3] Checking for 'Moe' (should be 'MoE')..."
if moe_violations=$(grep -rn --include="*.rs" 'Moe[^E]\|Moe$' crates/ 2>/dev/null || true); then
    if [ -n "$moe_violations" ]; then
        moe_count=$(echo "$moe_violations" | wc -l | tr -d ' ')
        echo "Found $moe_count Moe violations"
        violations=$((violations + moe_count))
    else
        echo "OK - No Moe violations"
    fi
else
    echo "OK - No Moe violations"
fi

# Check 2: Lora should be LoRA
# No word boundary - catches mid-word occurrences like SomeLoraConfig
echo ""
echo "[2/3] Checking for 'Lora' (should be 'LoRA')..."
if lora_violations=$(grep -rn --include="*.rs" 'Lora[^A]\|Lora$' crates/ 2>/dev/null || true); then
    if [ -n "$lora_violations" ]; then
        lora_count=$(echo "$lora_violations" | wc -l | tr -d ' ')
        echo "Found $lora_count Lora violations"
        violations=$((violations + lora_count))
    else
        echo "OK - No Lora violations"
    fi
else
    echo "OK - No Lora violations"
fi

# Check 3: Ambiguous gating function names (advisory only)
echo ""
echo "[3/3] Checking for ambiguous 'compute_*gating' patterns..."
if gating_violations=$(grep -rn --include="*.rs" 'compute_[a-z_]*gating' crates/ 2>/dev/null || true); then
    if [ -n "$gating_violations" ]; then
        gating_count=$(echo "$gating_violations" | wc -l | tr -d ' ')
        echo "Found $gating_count ambiguous gating names (advisory)"
    else
        echo "OK - No ambiguous gating patterns"
    fi
else
    echo "OK - No ambiguous gating patterns"
fi

# Compare against baseline
echo ""
echo "========================================"
echo "Total violations: $violations (baseline: $BASELINE_VIOLATIONS)"

if [ "$violations" -gt "$BASELINE_VIOLATIONS" ]; then
    new_violations=$((violations - BASELINE_VIOLATIONS))
    echo ""
    echo "FAILED: $new_violations NEW violation(s) introduced!"
    echo ""
    echo "Fix: Use 'MoE' (not 'Moe') and 'LoRA' (not 'Lora')"
    echo ""
    echo "To see all violations, run:"
    echo "  grep -rn --include='*.rs' 'Moe[^E]\\|Moe\$' crates/"
    echo "  grep -rn --include='*.rs' 'Lora[^A]\\|Lora\$' crates/"
    exit 1
elif [ "$violations" -lt "$BASELINE_VIOLATIONS" ]; then
    fixed=$((BASELINE_VIOLATIONS - violations))
    echo ""
    echo "PASSED: $fixed violation(s) fixed! Update BASELINE_VIOLATIONS to $violations"
    exit 0
else
    echo ""
    echo "PASSED: No new violations (at baseline)"
    exit 0
fi
