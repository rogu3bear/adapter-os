# Startup Contract

Canonical startup behavior for local and CI environments.

Last verified: 2026-02-09

## Primary Entry Point

Use `./start` for local orchestration.

Supported command surface (from `start`):

- `up` / `start` (default): backend + worker
- `down` / `stop`: graceful shutdown
- `status`: service status
- `backend`: backend only
- `worker`: worker only
- `secd`: Secure Enclave daemon only
- `node`: node agent only
- `preflight`: checks only

## UI Behavior

- Default mode: backend serves Leptos static UI from `crates/adapteros-server/static`.
- Dev hot reload mode: run `trunk serve` in `crates/adapteros-ui`.
- `scripts/service-manager.sh start ui` is intentionally a compatibility no-op because UI is backend-served in default mode.

## Readiness and Liveness

- `/healthz`: liveness probe, cheap and always available when process is up.
- `/readyz`: readiness gate requiring boot dependencies to be complete.

`./start` readiness behavior:

- With worker enabled and not in quick mode, `./start` polls `/readyz` and fails with actionable guidance when timeout is exceeded.
- `--quick` skips final readiness wait.
- `--verify-chat` performs optional post-ready inference chat verification.

## Worker Startup Contract

Worker startup fails fast on:

- Missing manifest path
- Missing model directory
- Requested backend not present in worker binary features

On success, worker must create its configured UDS socket and self-register with the control plane.

## Validation

Contract checks are enforced by:

- `scripts/contracts/check_startup_contract.sh`
- `scripts/contracts/check_docs_claims.sh`
