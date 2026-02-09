#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SEED_FILE="$ROOT_DIR/crates/adapteros-core/src/seed.rs"
Q15_FILE="$ROOT_DIR/crates/adapteros-lora-router/src/quantization.rs"
PATH_SEC_FILE="$ROOT_DIR/crates/adapteros-core/src/path_security.rs"

fail() {
  echo "FAIL: $1"
  exit 1
}

rg -q "pub const HKDF_ALGORITHM_VERSION: u32 = 2;" "$SEED_FILE" \
  || fail "HKDF algorithm version constant changed unexpectedly"

rg -q "pub const HKDF_OUTPUT_LENGTH: usize = 32;" "$SEED_FILE" \
  || fail "HKDF output length constant changed unexpectedly"

rg -q "pub const ROUTER_GATE_Q15_DENOM: f32 = 32767.0;" "$Q15_FILE" \
  || fail "Q15 denominator must remain 32767.0"

rg -q "pub const ROUTER_GATE_Q15_MAX: i16 = 32767;" "$Q15_FILE" \
  || fail "Q15 max must remain 32767"

for p in '"/tmp"' '"/private/tmp"' '"/var/tmp"'; do
  rg -q "$p" "$PATH_SEC_FILE" || fail "Path security must forbid $p"
done

CRITICAL_FILES=(
  "crates/adapteros-core/src/seed.rs"
  "crates/adapteros-lora-router/src/quantization.rs"
  "crates/adapteros-core/src/path_security.rs"
)

if [[ "${GITHUB_EVENT_NAME:-}" == "pull_request" ]] && [[ -n "${GITHUB_BASE_REF:-}" ]]; then
  if ! git rev-parse --verify "origin/${GITHUB_BASE_REF}" >/dev/null 2>&1; then
    echo "WARN: origin/${GITHUB_BASE_REF} not available, skipping label gate"
  else
    changed="$(git diff --name-only "origin/${GITHUB_BASE_REF}"...HEAD || true)"
    requires_label=0
    for f in "${CRITICAL_FILES[@]}"; do
      if grep -qx "$f" <<<"$changed"; then
        requires_label=1
      fi
    done

    if [[ "$requires_label" -eq 1 ]]; then
      labels=""
      if [[ -f "${GITHUB_EVENT_PATH:-}" ]]; then
        labels="$(jq -r '.pull_request.labels[].name // empty' "$GITHUB_EVENT_PATH" 2>/dev/null || true)"
      fi
      if ! grep -qx "determinism-contract-change" <<<"$labels"; then
        fail "Determinism contract files changed without PR label 'determinism-contract-change'"
      fi
    fi
  fi
fi

echo "=== Determinism Contract Check: PASSED ==="
