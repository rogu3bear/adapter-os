# Phase 8 Execution: Security + Replay Evidence Closure

## Objective
- Re-validate high-risk replay and determinism claims against current source, and capture remaining evidence gaps.

## Replay Endpoint Proof (Current Source)
- Verified registered replay routes in `crates/adapteros-server-api/src/routes/mod.rs`:
  - `/v1/replay/sessions`
  - `/v1/replay/sessions/{id}`
  - `/v1/replay/sessions/{id}/verify`
  - `/v1/replay/sessions/{id}/execute`
  - `/v1/replay/check/{inference_id}`
  - `/v1/replay`
  - `/v1/replay/history/{inference_id}`
  - `/v1/adapteros/replay`

## Evidence Gap Findings
1. `docs/VERIFIED_REPO_FACTS.md` replay line references are stale against current `routes/mod.rs` line positions (doc was audited on 2026-02-04).
2. `docs/replay.md` documents:
  - `POST /v1/replay/verify/trace`
  - `POST /v1/replay/verify/bundle`
  but these routes are not registered in `crates/adapteros-server-api/src/routes/mod.rs`.
3. Replay consolidation intent remains split in practice (session-based canonical plus inference-based endpoints still active).

## Determinism/Tenant Controls Evidence
- Determinism and Q15 invariants remain documented in `docs/DETERMINISM.md` and `docs/POLICIES.md`.
- Tenant scoping evidence remains captured in `docs/VERIFIED_REPO_FACTS.md` tables (`tenant_id` FKs on replay/session tables).

## Verification Run
- Route proof:
`rg -n "replay|Replay" crates/adapteros-server-api/src/routes/mod.rs`
- Replay docs consistency scan:
`rg -n "replay|Replay|session|verify|execute" docs/replay.md docs/VERIFIED_REPO_FACTS.md`
- Replay verification route mismatch scan:
`rg -n "replay/verify|/v1/replay/verify" crates/adapteros-server-api/src/routes/mod.rs docs/replay.md`

- Determinism replay test:
`cargo test -p adapteros-server-api --test replay_determinism_tests`
- Result: passed (`32 passed; 0 failed`).

## Phase 8 Completion
- [x] Replay endpoint proof refreshed.
- [x] Determinism replay test executed and passed.
- [x] Evidence gaps documented with concrete source references.
- [x] Canonical/deprecated split status captured for integration phase.

