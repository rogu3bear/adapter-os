# Phase 14: Runtime and Observability Hardening - Research

**Researched:** 2026-02-24
**Domain:** Runtime fail-safe behavior, SSE breaker telemetry, and model-server transport policy alignment
**Confidence:** HIGH
**Status:** Ready for planning execution

## Summary

Phase 14 is a repo-grounded hardening pass, not a discovery-heavy phase. The key surfaces already exist but are partially misaligned:

- Deadlock recovery logic exists and already exits fail-closed with artifact output, but worker initialization still uses `DeadlockConfig::default()` instead of `[worker.safety]` deadlock knobs.
- SSE breaker + telemetry implementation exists for stream handlers and is configurable from `state.streaming.*`, but coverage/consistency must be tightened across intended SSE paths.
- Model-server transport contracts are split: control-plane config emphasizes `model_server.server_addr` with TCP defaults, while model-server crate usage/docs are UDS-first; current worker client `from_socket_path` does not actually use UDS transport.

**Primary recommendation:** execute a 3-plan closure model:
- `14-01` hardens deadlock fail-safe config + evidence (`OBS-08`)
- `14-02` hardens configurable SSE breaker + telemetry coverage (`OBS-09`)
- `14-03` aligns model-server transport with UDS/zero-egress expectations (`SEC-06`)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Scope is hardening only for runtime/observability behavior.
- Must cover:
  - deadlock recovery fail-safe behavior
  - configurable SSE breaker + telemetry
  - model-server UDS/zero-egress alignment
- Plans must be minimal, concrete, executable.
- Do not introduce parallel transport/streaming architectures.

### Claude's Discretion
- Smallest viable compatibility strategy for model-server config alignment.
- Minimal refactoring needed to make deadlock fail-safe branches testable.
- Telemetry metric wiring details so long as current contracts remain coherent.

### Deferred Ideas (OUT OF SCOPE)
- New observability product features.
- Broad worker orchestration redesign.
- Model-server capability expansion outside transport/policy hardening.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Requirement | Research Support |
|----|-------------|------------------|
| OBS-08 | Deadlock recovery behavior is fail-safe, config-driven, and evidenced | `deadlock.rs` has artifact+exit behavior; `lib.rs` still builds detector from defaults; `[worker.safety]` deadlock knobs exist in `etc/adapteros/cp.toml`. |
| OBS-09 | SSE breaker settings are configurable and transitions are observable via telemetry/metrics | `state.rs` has `sse_circuit_*` config; `handlers/streaming.rs` has breaker state machine + telemetry/metrics transition emission. |
| SEC-06 | Model-server connectivity aligns to UDS-first and production zero-egress expectations | `effective.rs` models `model_server.server_addr` TCP default; model-server crate/docs are UDS-socket oriented; worker `from_socket_path` currently falls back to TCP. |
</phase_requirements>

<plan_dependency_model>
## Plan Dependency Model

| Plan | Wave | Depends On | Requirement Focus | Why This Shape |
|------|------|------------|-------------------|----------------|
| 14-01 | 1 | none | OBS-08 | Deadlock fail-safe closure is isolated to worker safety/runtime behavior. |
| 14-02 | 1 | none | OBS-09 | SSE breaker/telemetry hardening is isolated to server-api streaming surfaces. |
| 14-03 | 2 | 14-01, 14-02 | SEC-06 | Transport/policy alignment is safest after runtime observability lanes are hardened and evidenced. |
</plan_dependency_model>

## Standard Stack

| Surface | Existing Files | Reuse Rationale |
|---------|----------------|-----------------|
| Deadlock fail-safe | `crates/adapteros-lora-worker/src/deadlock.rs`, `crates/adapteros-lora-worker/src/lib.rs`, `etc/adapteros/cp.toml` | Recovery + exit semantics already exist; hardening is wiring + targeted tests. |
| SSE breaker + telemetry | `crates/adapteros-server-api/src/state.rs`, `crates/adapteros-server-api/src/handlers/streaming.rs`, `crates/adapteros-server-api/src/handlers/admin.rs` | Breaker config + transition metrics/telemetry already implemented; closure is consistency and coverage. |
| Model-server transport contract | `crates/adapteros-config/src/effective.rs`, `crates/adapteros-lora-worker/src/model_server_client.rs`, `crates/adapteros-lora-worker/src/backend_factory.rs`, `crates/adapteros-server-api/src/runtime_mode.rs` | Existing contract surfaces are close but split between TCP-style and UDS-style assumptions. |

## Architecture Patterns

### Pattern: Fail-Safe Artifact + Deterministic Exit
- Persist deadlock recovery artifacts first.
- Exit with explicit cause code.
- Keep supervisor restart as best-effort side effect.

### Pattern: Config-Driven Breaker Behavior
- Read breaker thresholds/timeouts from config once per stream context.
- Emit transition events + metrics on state changes only.
- Keep transition names stable for operator dashboards.

### Pattern: Security Contract Alignment over Feature Addition
- Align schema/runtime/docs to one transport contract.
- Reject configuration that violates production egress guarantees.
- Avoid adding fallback paths that silently weaken policy.

## Do Not Hand-Roll

| Problem | Do Not Build | Use Instead | Why |
|---------|--------------|-------------|-----|
| Deadlock fail-safe hardening | New supervisor/restart subsystem | Existing `deadlock.rs` recovery path | Existing path already encodes artifact + exit behavior. |
| SSE resiliency | New stream stack | Existing `StreamCircuitBreaker` + telemetry transition flow | Keeps observability contract continuity. |
| Model-server transport policy | Ad-hoc transport flags in random call sites | Effective config + runtime mode validation + model-server client config | Keeps policy enforcement centralized and auditable. |

## Common Pitfalls

### Pitfall 1: Deadlock knobs defined but unused
**What goes wrong:** Operators tune `[worker.safety]` deadlock settings but runtime behavior remains default.
**How to avoid:** Bind `DeadlockConfig` from effective config and cover with targeted tests.

### Pitfall 2: Breaker config present but incomplete path coverage
**What goes wrong:** Some SSE flows honor breaker thresholds while others silently bypass them.
**How to avoid:** Reuse one breaker settings source and add narrow tests for transition telemetry.

### Pitfall 3: UDS intent with TCP fallback in production
**What goes wrong:** Model-server traffic remains TCP while policy assumes zero network egress.
**How to avoid:** Enforce transport contract in config/runtime validation and remove silent TCP fallback for socket-path config.

## Code Examples

### Deadlock fail-safe wiring baseline
```bash
rg -n "deadlock_check_interval_secs|max_wait_time_secs|max_lock_depth|recovery_timeout_secs|DeadlockConfig::default|DeadlockDetector::new" \
  etc/adapteros/cp.toml \
  crates/adapteros-config/src/effective.rs \
  crates/adapteros-lora-worker/src/lib.rs \
  crates/adapteros-lora-worker/src/deadlock.rs
```

### SSE breaker config + transition telemetry baseline
```bash
rg -n "sse_circuit_failure_threshold|sse_circuit_recovery_timeout_secs|stream_breaker_settings|streaming.circuit_breaker.transition" \
  crates/adapteros-server-api/src/state.rs \
  crates/adapteros-server-api/src/handlers/streaming.rs \
  crates/adapteros-server-api/src/handlers/admin.rs

cargo test -p adapteros-server-api stream_circuit_breaker_ -- --test-threads=1
```

### Model-server transport contract drift baseline
```bash
rg -n "model_server.server_addr|socket_path|from_socket_path|http://127.0.0.1:50051" \
  crates/adapteros-config/src/effective.rs \
  crates/adapteros-config/src/schema.rs \
  crates/adapteros-lora-worker/src/model_server_client.rs \
  crates/adapteros-lora-worker/src/backend_factory.rs \
  docs/CONFIGURATION.md \
  crates/adapteros-model-server/src/lib.rs
```

## Current State (Verified)

- `deadlock.rs` recovery path writes an artifact and exits with explicit reason codes (`111`, `112`, `113`) after recovery decision.
- Worker boot currently initializes deadlock detection via `DeadlockConfig::default()` in `crates/adapteros-lora-worker/src/lib.rs`.
- `[worker.safety]` deadlock settings are already present in `etc/adapteros/cp.toml`.
- `state.rs` exposes streaming breaker settings (`sse_circuit_failure_threshold`, `sse_circuit_recovery_timeout_secs`).
- `handlers/streaming.rs` consumes those settings and emits telemetry/metrics transition signals.
- Model-server config surface currently centers on `model_server.server_addr` with TCP-style default in effective config/schema.
- Model-server crate docs/config are UDS-socket oriented, and `ModelServerClientConfig::from_socket_path` currently falls back to localhost TCP.

## Sources

- `.planning/ROADMAP.md` (Phase 14 slot and dependency)
- `crates/adapteros-lora-worker/src/deadlock.rs`
- `crates/adapteros-lora-worker/src/lib.rs`
- `etc/adapteros/cp.toml`
- `crates/adapteros-server-api/src/state.rs`
- `crates/adapteros-server-api/src/handlers/streaming.rs`
- `crates/adapteros-server-api/src/handlers/admin.rs`
- `crates/adapteros-config/src/effective.rs`
- `crates/adapteros-config/src/schema.rs`
- `crates/adapteros-lora-worker/src/model_server_client.rs`
- `crates/adapteros-lora-worker/src/backend_factory.rs`
- `crates/adapteros-server-api/src/runtime_mode.rs`
- `docs/CONFIGURATION.md`
- `crates/adapteros-model-server/src/lib.rs`

---
*Phase: 14-runtime-and-observability-hardening*
*Research completed: 2026-02-24*
*Ready for planning: yes*
