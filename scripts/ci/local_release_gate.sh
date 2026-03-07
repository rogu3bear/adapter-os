#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

: "${LOCAL_RELEASE_MODE:=standard}"
: "${LOCAL_RELEASE_RUN_INFERENCE:=0}"
: "${LOCAL_RELEASE_GOVERNANCE_MODE:=enforce}"
: "${GOVERNANCE_TARGET_MANIFEST:=$ROOT_DIR/docs/governance/target-manifest.json}"
: "${AOS_SERVER_PORT:=18080}"
: "${AOS_API_URL:=http://localhost:${AOS_SERVER_PORT}}"
: "${LOCAL_RELEASE_BUNDLE_DIR:=$ROOT_DIR/target/release-bundle}"
: "${LOCAL_RELEASE_EVIDENCE_DIR:=$ROOT_DIR/.planning/prod-cut/evidence/release}"

STARTED_BACKEND_FOR_SMOKE=0

run_step() {
  local name="$1"
  shift
  echo ""
  echo "-> ${name}"
  "$@"
}

is_prod_mode() {
  [[ "$LOCAL_RELEASE_MODE" == "prod" ]]
}

backend_healthy() {
  curl -fsS --max-time 2 "http://localhost:${AOS_SERVER_PORT}/healthz" >/dev/null 2>&1
}

cleanup() {
  if [[ "${STARTED_BACKEND_FOR_SMOKE}" == "1" ]]; then
    echo ""
    echo "-> Stop backend (cleanup)"
    bash ./start down >/dev/null 2>&1 || true
  fi
}

trap cleanup EXIT

ensure_backend_for_smoke() {
  if backend_healthy; then
    echo ""
    echo "-> Backend for smoke"
    echo "Backend already healthy on port ${AOS_SERVER_PORT}."
    return 0
  fi

  run_step "Start backend for smoke" env AOS_SERVER_PORT="${AOS_SERVER_PORT}" ./start backend
  STARTED_BACKEND_FOR_SMOKE=1
}

capture_release_evidence() {
  mkdir -p "$LOCAL_RELEASE_EVIDENCE_DIR"
  cp "$LOCAL_RELEASE_BUNDLE_DIR/release_verification.log" \
    "$LOCAL_RELEASE_EVIDENCE_DIR/release_verification.log"
}

governance_blocked_external_exception_reason() {
  local governance_repo="$1"
  local governance_branch="$2"

  if [[ ! -f "$GOVERNANCE_TARGET_MANIFEST" ]]; then
    return 1
  fi

  if ! command -v jq >/dev/null 2>&1; then
    return 1
  fi

  jq -r --arg repo "$governance_repo" --arg branch "$governance_branch" '
    first(.targets[]? | select(.repo == $repo and .branch == $branch)) as $target
    | if $target == null then
        empty
      else
        ($target.approved_exceptions // []
          | map(select(.type == "blocked_external") | (.reason // ""))
          | map(select(length > 0))
          | .[0] // empty)
      end
  ' "$GOVERNANCE_TARGET_MANIFEST"
}

run_governance_preflight() {
  case "$LOCAL_RELEASE_GOVERNANCE_MODE" in
    off)
      echo ""
      echo "-> Governance preflight"
      echo "SKIP: disabled in local mode (LOCAL_RELEASE_GOVERNANCE_MODE=off)."
      return 0
      ;;
    warn|enforce)
      echo ""
      echo "-> Governance preflight (${LOCAL_RELEASE_GOVERNANCE_MODE})"
      local governance_repo governance_branch governance_required_context exception_reason
      governance_repo="${GOVERNANCE_REPO:-rogu3bear/adapter-os}"
      governance_branch="${GOVERNANCE_BRANCH:-main}"
      governance_required_context="${GOVERNANCE_REQUIRED_CONTEXT:-FFI AddressSanitizer (push)}"
      set +e
      preflight_output="$(bash scripts/ci/check_governance_preflight.sh \
        --repo "${governance_repo}" \
        --branch "${governance_branch}" \
        --required-context "${governance_required_context}" 2>&1)"
      preflight_rc=$?
      set -e
      printf '%s\n' "$preflight_output"

      case "$preflight_rc" in
        0)
          return 0
          ;;
        20)
          if [[ "$LOCAL_RELEASE_GOVERNANCE_MODE" == "enforce" ]]; then
            exception_reason="$(governance_blocked_external_exception_reason "$governance_repo" "$governance_branch" || true)"
            if [[ -n "$exception_reason" ]]; then
              echo "WARN: governance preflight blocked_external is manifest-approved for ${governance_repo}@${governance_branch}; continuing in enforce mode."
              echo "INFO: governance approved_exception reason: ${exception_reason}"
              return 0
            fi
            echo "ERROR: governance preflight blocked_external (enforced)." >&2
            exit 1
          fi
          echo "WARN: governance preflight blocked_external; continuing in warn mode."
          return 0
          ;;
        30|40)
          if [[ "$LOCAL_RELEASE_GOVERNANCE_MODE" == "enforce" ]]; then
            echo "ERROR: governance preflight failed (rc=$preflight_rc)." >&2
            exit 1
          fi
          echo "WARN: governance preflight failed (rc=$preflight_rc); continuing in warn mode."
          return 0
          ;;
        *)
          if [[ "$LOCAL_RELEASE_GOVERNANCE_MODE" == "enforce" ]]; then
            echo "ERROR: unexpected governance preflight exit code: $preflight_rc" >&2
            exit 1
          fi
          echo "WARN: unexpected governance preflight exit code: $preflight_rc; continuing in warn mode."
          return 0
          ;;
      esac
      ;;
    *)
      echo "ERROR: invalid LOCAL_RELEASE_GOVERNANCE_MODE='$LOCAL_RELEASE_GOVERNANCE_MODE' (expected off|warn|enforce)." >&2
      exit 1
      ;;
  esac
}

if is_prod_mode; then
  export LOCAL_REQUIRED_CLIPPY_SCOPE="all-targets"
  export LOCAL_REQUIRED_PROFILE="prod"
  export AOS_REQUIRE_RELEASE_BINARIES=1
  export ROUTE_COVERAGE_STRICT_OPENAPI_ONLY=1
  export ROUTE_COVERAGE_STRICT_PARAM_MISMATCH=1
  export SMOKE_INFERENCE_STRICT=1
  if [[ "$LOCAL_RELEASE_RUN_INFERENCE" != "1" ]]; then
    echo "ERROR: prod mode requires LOCAL_RELEASE_RUN_INFERENCE=1" >&2
    exit 1
  fi
else
  export LOCAL_REQUIRED_PROFILE="${LOCAL_REQUIRED_PROFILE:-standard}"
fi

if backend_healthy; then
  run_step "Config check (backend already running)" bash scripts/check-config.sh --allow-in-use
else
  run_step "Config check" bash scripts/check-config.sh
  run_step "Startup preflight" ./start preflight
fi

run_step "Local required checks" bash scripts/ci/local_required_checks.sh
run_governance_preflight

ensure_backend_for_smoke

if is_prod_mode; then
  run_step "MVP smoke (full lane)" env \
    MVP_SMOKE_SKIP_UI_BUILD=0 \
    MVP_SMOKE_SKIP_SERVER_API_TESTS=0 \
    MVP_SMOKE_SKIP_FMT=0 \
    AOS_API_URL="$AOS_API_URL" \
    bash scripts/mvp_smoke.sh

  run_step "Inference smoke (strict)" env \
    AOS_SERVER_PORT="$AOS_SERVER_PORT" \
    SMOKE_INFERENCE_STRICT=1 \
    bash scripts/smoke-inference.sh

  run_step "Runbook drill evidence" env RUNBOOK_DRILL_STRICT=1 bash scripts/ci/check_runbook_drill_evidence.sh
  run_step "Release artifact integrity (SBOM/provenance/signing)" env \
    OUT_DIR="$LOCAL_RELEASE_BUNDLE_DIR" \
    SBOM_REQUIRE_SIGNING=1 \
    bash scripts/release/sbom.sh
  run_step "Capture release verification evidence" capture_release_evidence
else
  run_step "MVP smoke (local no-browser lane)" env \
    MVP_SMOKE_SKIP_UI_BUILD=1 \
    MVP_SMOKE_SKIP_SERVER_API_TESTS=1 \
    MVP_SMOKE_SKIP_FMT=1 \
    AOS_API_URL="$AOS_API_URL" \
    bash scripts/mvp_smoke.sh

  if [[ "$LOCAL_RELEASE_RUN_INFERENCE" == "1" ]]; then
    run_step "Inference smoke" env AOS_SERVER_PORT="$AOS_SERVER_PORT" bash scripts/smoke-inference.sh
  else
    echo ""
    echo "-> Inference smoke"
    echo "SKIP: set LOCAL_RELEASE_RUN_INFERENCE=1 to enable model-dependent inference lane."
  fi
fi

echo ""
echo "=== Local Release Gate: PASSED ==="
