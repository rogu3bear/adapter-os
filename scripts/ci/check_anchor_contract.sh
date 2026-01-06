#!/usr/bin/env bash
# CI Guard: Verify AnchorComparison contract remains stable.
# This script ensures the diagnostics AnchorComparison struct retains
# required fields and the diff response continues to expose it.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
API_TYPES_FILE="$ROOT_DIR/crates/adapteros-api-types/src/diagnostics.rs"

if [[ ! -f "$API_TYPES_FILE" ]]; then
  echo "❌ FAIL: diagnostics API types file not found: $API_TYPES_FILE"
  exit 1
fi

required_fields=(
  request_hash_match
  manifest_hash_match
  decision_chain_hash_match
  backend_identity_hash_match
  model_identity_hash_match
  all_anchors_match
  request_hash_a
  request_hash_b
  decision_chain_hash_a
  decision_chain_hash_b
)

missing=()
for field in "${required_fields[@]}"; do
  if ! rg -q "pub ${field}:" "$API_TYPES_FILE"; then
    missing+=("$field")
  fi
done

if ! rg -q "pub anchor_comparison: AnchorComparison" "$API_TYPES_FILE"; then
  echo "❌ FAIL: DiagDiffResponse missing anchor_comparison field"
  exit 1
fi

if [[ ${#missing[@]} -gt 0 ]]; then
  echo "❌ FAIL: AnchorComparison missing fields: ${missing[*]}"
  exit 1
fi

echo "=== Anchor Comparison Contract Guard: PASSED ==="
