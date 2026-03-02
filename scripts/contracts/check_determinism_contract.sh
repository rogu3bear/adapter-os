#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SEED_FILE="$ROOT_DIR/crates/adapteros-core/src/seed.rs"
Q15_FILE="$ROOT_DIR/crates/adapteros-lora-router/src/quantization.rs"
PATH_SEC_FILE="$ROOT_DIR/crates/adapteros-core/src/path_security.rs"
ALLOWLIST_FILE="$ROOT_DIR/docs/contracts/determinism_unseeded_allowlist.csv"
TMP_DIR="$ROOT_DIR/var/tmp/determinism-contract"
mkdir -p "$TMP_DIR"

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

# Static scan: no unseeded randomness in production inference paths.
# Allowed in: crypto, CLI, tests, benchmarks, stress tests.
UNSEEDED_HITS=$(rg -l "thread_rng|OsRng|from_entropy|rand::random" \
  --type rust \
  --glob '!**/tests/**' \
  --glob '!**/test/**' \
  --glob '!**/benches/**' \
  --glob '!**/*_test.rs' \
  --glob '!**/stress_test*' \
  "$ROOT_DIR/crates/adapteros-lora-router/src" \
  "$ROOT_DIR/crates/adapteros-lora-worker/src" \
  "$ROOT_DIR/crates/adapteros-lora-kernel-mtl/src" \
  "$ROOT_DIR/crates/adapteros-lora-kernel-coreml/src" \
  "$ROOT_DIR/crates/adapteros-deterministic-exec/src" \
  2>/dev/null || true)

if [[ -n "$UNSEEDED_HITS" ]]; then
  if [[ ! -f "$ALLOWLIST_FILE" ]]; then
    fail "Unseeded randomness detected and allowlist is missing: $ALLOWLIST_FILE"
  fi

  python3 - "$ALLOWLIST_FILE" <<'PY'
import csv
import datetime as dt
import pathlib
import sys

allowlist = pathlib.Path(sys.argv[1])
today = dt.date.today()
expired = []
missing_fields = []

with allowlist.open("r", encoding="utf-8", newline="") as handle:
    reader = csv.DictReader(handle)
    for row in reader:
        path = (row.get("path") or "").strip()
        owner = (row.get("owner") or "").strip()
        expires_on = (row.get("expires_on") or "").strip()
        rationale = (row.get("rationale") or "").strip()
        if not path:
            continue
        if not owner or not expires_on or not rationale:
            missing_fields.append(path)
            continue
        try:
            expiry = dt.date.fromisoformat(expires_on)
        except ValueError:
            missing_fields.append(path)
            continue
        if expiry < today:
            expired.append(f"{path} (expired {expires_on})")

if missing_fields:
    print("Determinism allowlist entries missing required owner/expires_on/rationale fields:", file=sys.stderr)
    for item in missing_fields:
        print(item, file=sys.stderr)
    sys.exit(1)

if expired:
    print("Determinism allowlist entries are expired:", file=sys.stderr)
    for item in expired:
        print(item, file=sys.stderr)
    sys.exit(1)
PY

  HITS_FILE="$TMP_DIR/unseeded_hits.txt"
  ALLOWLIST_PATHS="$TMP_DIR/unseeded_allowlist_paths.txt"
  UNRESOLVED_FILE="$TMP_DIR/unseeded_unresolved.txt"

  printf "%s\n" "$UNSEEDED_HITS" \
    | sed '/^[[:space:]]*$/d' \
    | sed "s#^$ROOT_DIR/##" \
    | sort -u > "$HITS_FILE"
  awk -F',' 'NR>1 {gsub(/^[[:space:]]+|[[:space:]]+$/, "", $1); if ($1 != "") print $1}' "$ALLOWLIST_FILE" \
    | sort -u > "$ALLOWLIST_PATHS"

  grep -Fvx -f "$ALLOWLIST_PATHS" "$HITS_FILE" > "$UNRESOLVED_FILE" || true
  unresolved_count="$(wc -l < "$UNRESOLVED_FILE" | tr -d ' ')"

  if [[ "$unresolved_count" -gt 0 ]]; then
    echo "FAIL: Unseeded randomness found outside approved allowlist:"
    cat "$UNRESOLVED_FILE"
    echo "Add a deterministic fix or a temporary allowlist decision with owner/expires_on/rationale."
    exit 1
  fi

  echo "INFO: Unseeded randomness hits are fully accounted for by allowlist decisions."
fi

echo "=== Determinism Contract Check: PASSED ==="
