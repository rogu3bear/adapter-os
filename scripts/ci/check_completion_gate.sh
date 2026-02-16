#!/usr/bin/env bash
# CI Guard: completion-first regression checks for primary UX contracts.
# Detects known regressions that previously caused 404/501 or misleading states.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

echo "=== Completion Gate Check ==="

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

require_present() {
  local pattern="$1"
  local file="$2"
  local message="$3"
  if ! rg --fixed-strings --quiet -- "$pattern" "$file"; then
    fail "$message ($file)"
  fi
}

require_absent() {
  local pattern="$1"
  local file="$2"
  local message="$3"
  if rg --fixed-strings --quiet -- "$pattern" "$file"; then
    fail "$message ($file)"
  fi
}

run_check() {
  local description="$1"
  shift
  echo "CHECK: $description"
  if ! "$@"; then
    fail "$description"
  fi
}

# PRD-01 guard: UI must not call nonexistent worker metrics route.
require_absent "/v1/workers/{}/metrics" \
  "crates/adapteros-ui/src/api/client.rs" \
  "legacy worker metrics route call reintroduced"
require_present "/v1/workers/{}/detail" \
  "crates/adapteros-ui/src/api/client.rs" \
  "worker detail contract missing for metrics mapping"

# PRD-05 guard: documents flow must not use hidden testkit-first path.
require_absent "/testkit/create_training_job_stub" \
  "crates/adapteros-ui/src/pages/documents.rs" \
  "document training path still coupled to testkit stub"

# PRD-08 guard: reviews live stream path must exist end-to-end.
require_present "/v1/stream/reviews" \
  "crates/adapteros-server-api/src/routes/mod.rs" \
  "reviews stream route missing"
require_present "/v1/stream/reviews" \
  "crates/adapteros-ui/src/pages/reviews.rs" \
  "reviews UI is not wired to stream route"

# OP-02 guard: reviews must expose stream-vs-polling mode truthfully.
require_present "if is_polling_fallback_active(sse_status.get_untracked())" \
  "crates/adapteros-ui/src/pages/reviews.rs" \
  "reviews polling fallback is not explicitly gated by SSE status"
require_present "Polling fallback" \
  "crates/adapteros-ui/src/pages/reviews.rs" \
  "reviews UI missing fallback mode indicator"
require_present "Live stream" \
  "crates/adapteros-ui/src/pages/reviews.rs" \
  "reviews UI missing live stream mode indicator"

# PRD-03 guard: reject fabricated 8GiB GPU fallback.
require_absent "8 * 1024 * 1024 * 1024" \
  "crates/adapteros-server-api/src/handlers/capacity.rs" \
  "fabricated 8GiB GPU fallback detected"
require_present "GpuMemoryAvailability" \
  "crates/adapteros-server-api/src/handlers/capacity.rs" \
  "GPU availability semantics missing"

# PRD-02 guard: core auth lifecycle should not route to generic 501 stubs.
require_absent "stub_not_implemented_error(\"bootstrap_admin\")" \
  "crates/adapteros-server-api/src/handlers/auth_enhanced/stubs.rs" \
  "bootstrap lifecycle still wired to 501 stub"
require_absent "stub_not_implemented_error(\"mfa_" \
  "crates/adapteros-server-api/src/handlers/auth_enhanced/stubs.rs" \
  "MFA lifecycle still wired to 501 stub"

# PRD-10 guard: repository path deprecation semantics must be behaviorally verified.
run_check "deprecated /v1/repos emits migration headers" \
  cargo test -p adapteros-server-api --lib \
    middleware::versioning::tests::deprecated_repo_family_emits_migration_headers \
    -- --exact
run_check "repository deprecation mapping keeps /v1/code/repositories canonical" \
  cargo test -p adapteros-server-api --lib \
    middleware::versioning::tests::test_check_deprecation \
    -- --exact

# PRD-11 guard: UI routing contract must compile with current route coverage.
run_check "adapteros-ui routing tests compile" \
  cargo test -p adapteros-ui --test routing --no-run

echo "PASS: completion gate checks satisfied"
