# Phase 13: Determinism Guard Hardening - Research

**Researched:** 2026-02-24
**Domain:** Determinism diagnostics freshness, replay guardrails, and telemetry/alert surfacing
**Confidence:** HIGH (repo-local contracts), MEDIUM (runtime deployment variance)

## Summary

Repository foundations for Phase 13 already exist, but they are currently split across disconnected surfaces:

1. Diagnostics freshness foundations exist but freshness semantics are weak.
- `aosctl diag` persists rows into `determinism_checks` (`crates/adapteros-cli/src/commands/diag.rs`).
- `/v1/diagnostics/determinism-status` reads the latest row (`crates/adapteros-server-api/src/handlers/diagnostics.rs`).
- Endpoint output reports latest values but does not yet enforce explicit stale/fresh contract behavior.

2. Replay guardrails exist in CI but are not yet unified with runtime readiness.
- `.github/workflows/ci.yml` already includes `replay-harness`, `determinism-gate`, and `determinism` lanes.
- `/readyz`/health pathways exist and are tested (`handlers/health.rs`, `health_readyz_timeout_tests.rs`) but do not currently encode replay freshness/recency as first-class readiness criteria.

3. Telemetry/alert infrastructure exists, but determinism alert surfacing is incomplete.
- SSE architecture supports `SseStreamType::Alerts` and replay buffering (`crates/adapteros-server-api/src/sse`).
- `alerts_stream` exists in `handlers/streams/mod.rs`, but protected route exposure is not currently present in `routes/mod.rs`.
- Monitoring/error-alert handlers already provide alert data APIs; stream-layer and route-layer closure is the remaining integration work.

**Primary recommendation:** execute a three-plan closure sequence:
- `13-01` (`DET-07`) freshness contract hardening,
- `13-02` (`DET-08`) replay guardrails in CI and health,
- `13-03` (`DET-09`) telemetry/alert stream surfacing and operator visibility.

<user_constraints>
## User Constraints (from phase context and request)

### Locked Decisions
- Keep changes native to existing determinism/diagnostics/health/SSE ownership files.
- Use minimal, concrete, executable plan tasks.
- Requirement mapping for this phase is fixed to `DET-07`, `DET-08`, `DET-09`.
- Avoid parallel abstractions for determinism diagnostics or stream transport.

### Claude's Discretion
- Exact freshness window and stale semantics.
- Exact health payload shape for replay guard signal.
- Exact test additions needed for regression coverage.

### Deferred Ideas (OUT OF SCOPE)
- New determinism features unrelated to guard hardening.
- Broad runtime observability redesign beyond Phase 13 targets.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Requirement | Evidence Anchor | Plan Coverage |
|----|-------------|-----------------|---------------|
| DET-07 | Determinism diagnostics are automatically refreshed and explicitly marked fresh/stale | `determinism_checks` write path (`diag.rs`) + `/v1/diagnostics/determinism-status` handler | `13-01` |
| DET-08 | Deterministic replay guardrails are merge-gate enforced and reflected in runtime health/readiness | `.github/workflows/ci.yml` determinism lanes + `handlers/health.rs` readiness model | `13-02` |
| DET-09 | Determinism-relevant telemetry and alerts are surfaced through protected APIs/SSE with replay-safe behavior | `handlers/streams/mod.rs`, `sse/*`, monitoring/error-alert handlers, and route registration in `routes/mod.rs` | `13-03` |
</phase_requirements>

<plan_dependency_model>
## Plan Dependency Model

| Plan | Wave | Depends On | Requirement Focus | Why This Shape |
|------|------|------------|-------------------|----------------|
| 13-01 | 1 | none | DET-07 | Freshness contract is an independent data/API hardening lane. |
| 13-02 | 1 | none | DET-08 | Replay CI+health guardrails can be hardened in parallel with diagnostics freshness. |
| 13-03 | 2 | 13-01, 13-02 | DET-09 | Stream surfacing should reflect finalized freshness and guard semantics. |
</plan_dependency_model>

## Standard Stack

| Surface | Canonical Path | Role in Phase 13 |
|---------|----------------|------------------|
| Determinism diagnostics writer | `crates/adapteros-cli/src/commands/diag.rs` | Emits latest determinism check outcomes into `determinism_checks` |
| Determinism diagnostics API | `crates/adapteros-server-api/src/handlers/diagnostics.rs` | Serves freshness/status for operators and automation |
| Determinism status schema | `migrations/20260217090000_determinism_checks.sql` | Storage contract for determinism check recency and outcomes |
| Replay CI guardrails | `.github/workflows/ci.yml` | Merge-time replay determinism signal |
| Runtime health/readiness | `crates/adapteros-server-api/src/handlers/health.rs` | Runtime guard surface for replay status |
| SSE stream ownership | `crates/adapteros-server-api/src/handlers/streams/mod.rs` | Telemetry + alerts stream implementation and replay |
| SSE infrastructure | `crates/adapteros-server-api/src/sse/mod.rs`, `crates/adapteros-server-api/src/sse/types.rs` | Monotonic IDs, ring buffers, replay behavior |
| Alert APIs | `crates/adapteros-server-api/src/handlers/monitoring/mod.rs`, `crates/adapteros-server-api/src/handlers/error_alerts.rs` | Alert CRUD/history and operator actions |

## Architecture Patterns

### Pattern: Single-source freshness from existing determinism check table
- Keep freshness derivation from `determinism_checks` (and existing diagnostics tables as needed).
- Avoid introducing a second freshness store.

### Pattern: Guardrail parity between CI and health
- CI lanes remain merge-time gate.
- Health/readiness surfaces expose runtime-time guard status using equivalent semantics.

### Pattern: Replay-safe tenant-scoped stream surfacing
- Use existing SSE manager and replay buffers.
- Preserve tenant filtering on telemetry/alert streams.

## Do Not Hand-Roll

| Problem | Do Not Build | Use Instead | Why |
|---------|--------------|-------------|-----|
| Freshness state | New ad-hoc freshness file/cache | `determinism_checks` + diagnostics handlers | Existing truth source and migration already present |
| Replay gate | New one-off determinism workflow | Existing CI determinism lanes in `.github/workflows/ci.yml` | Prevents duplicate merge policy paths |
| Alert transport | New websocket or side channel | Existing SSE streams and `SseEventManager` | Preserves replay/gap semantics and route consistency |

## Common Pitfalls

### Pitfall 1: Treating "no recent check" as healthy
- Risk: stale diagnostics silently pass.
- Guard: explicit stale classification with threshold and operator-visible reason.

### Pitfall 2: CI replay signal and runtime readiness drift apart
- Risk: merge passes but runtime reports no guard context (or vice versa).
- Guard: define one replay guard contract and project it into both surfaces.

### Pitfall 3: Exposing alerts stream without tenant/permission guarantees
- Risk: cross-tenant data leakage.
- Guard: keep route-level auth and tenant filtering consistent with existing stream patterns.

### Pitfall 4: Surfacing alert data without replay gap behavior
- Risk: reconnecting clients miss critical alerts with no recovery cue.
- Guard: route through existing `SseEventManager` replay and gap warning behavior.

## Code Examples

### Diagnostics freshness baseline
```bash
rg -n "INSERT INTO determinism_checks|determinism-status|last_run" \
  crates/adapteros-cli/src/commands/diag.rs \
  crates/adapteros-server-api/src/handlers/diagnostics.rs \
  migrations/20260217090000_determinism_checks.sql
```

### Replay guardrail command set (existing CI-aligned)
```bash
cargo test -p adapter-os --no-default-features --test determinism_replay_harness -- --test-threads=1 --nocapture
cargo test -p adapteros-core --test determinism_regression_harness -- --test-threads=1
cargo test -p adapteros-server-api --test replay_determinism_tests -- --test-threads=1
bash scripts/check_fast_math_flags.sh
```

### Health/readiness verification anchor
```bash
cargo test -p adapteros-server-api --test health_readyz_timeout_tests -- --test-threads=1
```

### Stream surfacing verification anchor
```bash
rg -n "/v1/stream/alerts|/v1/stream/anomalies|/v1/stream/dashboard" \
  crates/adapteros-server-api/src/routes/mod.rs
```

## Current State (Verified)

- `determinism_checks` persistence path exists and is active in CLI diagnostics flow.
- `/v1/diagnostics/determinism-status` endpoint exists and reads latest check row.
- Determinism replay CI lanes already run in `ci.yml` with serial replay settings where required.
- `alerts_stream` implementation exists in `handlers/streams/mod.rs` with replay support hooks.
- Protected route registration currently includes telemetry/workers/reviews streams but not alert/anomaly/dashboard streams.

## Sources

### Primary (repository-grounded)
- `.planning/ROADMAP.md` (Phase 13 definition)
- `crates/adapteros-cli/src/commands/diag.rs`
- `crates/adapteros-server-api/src/handlers/diagnostics.rs`
- `crates/adapteros-server-api/src/handlers/health.rs`
- `crates/adapteros-server-api/src/handlers/streams/mod.rs`
- `crates/adapteros-server-api/src/routes/mod.rs`
- `crates/adapteros-server-api/src/sse/mod.rs`
- `crates/adapteros-server-api/src/sse/types.rs`
- `.github/workflows/ci.yml`
- `migrations/20260217090000_determinism_checks.sql`

---
*Phase: 13-determinism-guard-hardening*
*Research completed: 2026-02-24*
*Ready for planning: yes*
