#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

: "${LOCAL_REQUIRED_CLIPPY_SCOPE:=lib-bin}"
: "${LOCAL_REQUIRED_PROFILE:=standard}"

run_step() {
  local name="$1"
  shift
  echo ""
  echo "-> ${name}"
  "$@"
}

run_clippy() {
  if [[ "$LOCAL_REQUIRED_CLIPPY_SCOPE" == "all-targets" ]]; then
    run_step "Clippy (all-targets)" cargo clippy --workspace --all-targets --exclude adapteros-lora-mlx-ffi -- -D warnings
  else
    run_step "Clippy (lib/bin)" cargo clippy --workspace --lib --bins --exclude adapteros-lora-mlx-ffi -- -D warnings
  fi
}

run_step "Port contract" bash scripts/contracts/check_port_contract.sh
run_step "Rectification contract suite" bash scripts/contracts/check_all.sh
run_step "Rust fmt" cargo fmt --all -- --check
run_clippy
run_step "Fast-math flags" bash scripts/check_fast_math_flags.sh
run_step "Network defaults guard" cargo test -p adapteros-config --test network_defaults_guard
run_step "CLI parsing" cargo test -p adapteros-cli --test command_parsing_tests
run_step "Citations contract" cargo test -p adapteros-server-api --test citations_contract

if [[ "$LOCAL_REQUIRED_PROFILE" == "prod" ]]; then
  run_step "Tenant isolation contract" cargo test -p adapteros-server-api --test tenant_isolation_fix_test
  run_step "Training guardrails (fail-fast semantics)" cargo test -p adapteros-server-api --test training_guardrails start_training_rejects_base_model_mismatch -- --exact
  run_step "Determinism replay-seed contract" cargo test --test determinism_core_suite test_router_ordering_and_q15_gates_are_stable -- --exact
fi

echo ""
echo "=== Local Required Checks: PASSED ==="
