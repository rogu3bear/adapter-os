# Phase 14: Runtime and Observability Hardening - Context

**Gathered:** 2026-02-24
**Status:** Planned (ready for execution)

<domain>
## Phase Boundary

Phase 14 hardens runtime safety and operator visibility in existing code paths only. Scope is limited to:

1. deadlock recovery fail-safe behavior in the worker (`OBS-08`)
2. configurable SSE breaker behavior with transition telemetry (`OBS-09`)
3. model-server transport alignment with UDS + zero-egress expectations (`SEC-06`)

This is closure/hardening work. It is not a new feature phase.

</domain>

<decisions>
## Implementation Decisions

### Deadlock Fail-Safe Must Be Fail-Closed and Config-Driven (`OBS-08`)
- Worker deadlock thresholds must come from effective `[worker.safety]` values, not only `DeadlockConfig::default()`.
- Deadlock detection must always produce a durable artifact and terminate with explicit exit reason/codes.
- Supervisor restart remains best-effort; restart failure cannot keep a deadlocked worker alive.

### SSE Breaker Must Be Configurable and Observable (`OBS-09`)
- `streaming.sse_circuit_failure_threshold` and `streaming.sse_circuit_recovery_timeout_secs` remain the source of truth.
- Breaker transitions (`open`, `half_open`, `recover`) must remain visible in metrics + telemetry.
- Keep existing SSE route surfaces; avoid parallel stream implementations.

### Model-Server UDS + Zero-Egress Contract Must Be Aligned (`SEC-06`)
- Production hardening must not rely on implicit TCP localhost model-server transport.
- Config schema, worker model-server client, and boot/runtime validation must agree on UDS-first semantics.
- Production mode egress constraints must reject incompatible model-server endpoint configuration.

### Plan Topology
- Wave 1: `14-01` (`OBS-08`) and `14-02` (`OBS-09`) in parallel.
- Wave 2: `14-03` (`SEC-06`) after wave-1 so transport/policy alignment is validated against hardened runtime behavior.

### Claude's Discretion
- Minimal extraction needed to test deadlock fail-safe branches without broad worker lifecycle changes.
- Exact compatibility strategy for model-server config key migration (`server_addr` vs UDS path expression).
- Existing telemetry metric name reuse as long as dashboard contracts are preserved.

</decisions>

<specifics>
## Specific Ideas

- Requirements for this phase: `OBS-08`, `OBS-09`, `SEC-06`.
- Current implementation anchors:
  - `crates/adapteros-lora-worker/src/deadlock.rs` already contains artifact writing and explicit exit codes for recovery outcomes.
  - `crates/adapteros-lora-worker/src/lib.rs` currently instantiates `DeadlockDetector::new(DeadlockConfig::default())`.
  - `etc/adapteros/cp.toml` already defines deadlock knobs in `[worker.safety]`.
  - `crates/adapteros-server-api/src/state.rs` already contains streaming breaker config fields.
  - `crates/adapteros-server-api/src/handlers/streaming.rs` already implements SSE breaker transitions and emits telemetry/metrics.
  - `crates/adapteros-config/src/effective.rs` and `docs/CONFIGURATION.md` define model-server as `server_addr` (TCP-style), while model-server crate docs/config emphasize socket-path UDS usage.
  - `crates/adapteros-lora-worker/src/model_server_client.rs` has `from_socket_path` but currently falls back to TCP localhost.
- Planning artifacts required in this phase:
  - `14-CONTEXT.md`
  - `14-RESEARCH.md`
  - `14-01-PLAN.md`
  - `14-02-PLAN.md`
  - `14-03-PLAN.md`

</specifics>

<deferred>
## Deferred Ideas

- New SSE products/endpoints or broader event taxonomy redesign.
- Worker supervision redesign beyond deadlock fail-safe hardening.
- Model-server capability expansion beyond transport/policy alignment.

</deferred>

---
*Phase: 14-runtime-and-observability-hardening*
*Context gathered: 2026-02-24*
