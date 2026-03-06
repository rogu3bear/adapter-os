#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/../.." && pwd)"
# shellcheck disable=SC1091
source "${ROOT_DIR}/scripts/lib/build-targets.sh"

fail() {
  echo "[FAIL] $1" >&2
  exit 1
}

TMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/adapter-os-build-targets-XXXXXX")"
cleanup() {
  rm -rf "${TMP_ROOT}"
}
trap cleanup EXIT

touch "${TMP_ROOT}/Cargo.toml"
mkdir -p "${TMP_ROOT}/target-codex/debug"
touch "${TMP_ROOT}/target-codex/debug/placeholder"

ROOT_DIR="${TMP_ROOT}"

EXPECTED_TARGET="${TMP_ROOT}/target"
ACTUAL_TARGET="$(aos_build_target_root)"
[[ "${ACTUAL_TARGET}" == "${EXPECTED_TARGET}" ]] \
  || fail "expected canonical target root ${EXPECTED_TARGET}, got ${ACTUAL_TARGET}"

if \
  AOS_INCREMENTAL_WARN_GB=999 \
  AOS_INCREMENTAL_PRUNE_GB=999 \
  AOS_INCREMENTAL_MAX_AGE_HOURS=0 \
  AOS_AUTO_PRUNE_INCREMENTAL=0 \
  aos_prune_scratch_targets_if_needed; then
  fail "expected scratch-target warning path to return non-zero"
fi

[[ -d "${TMP_ROOT}/target-codex" ]] || fail "warning path should not prune scratch target"

if \
  AOS_INCREMENTAL_WARN_GB=999 \
  AOS_INCREMENTAL_PRUNE_GB=999 \
  AOS_INCREMENTAL_MAX_AGE_HOURS=0 \
  AOS_AUTO_PRUNE_INCREMENTAL=1 \
  aos_prune_scratch_targets_if_needed; then
  fail "expected scratch-target prune path to return non-zero"
fi

[[ ! -d "${TMP_ROOT}/target-codex" ]] || fail "expected scratch target root to be pruned"

echo "[PASS] adapter-os build target guardrails"
