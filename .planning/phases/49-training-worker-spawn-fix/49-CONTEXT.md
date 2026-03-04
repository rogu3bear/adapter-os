# Phase 49: Training Worker Spawn Fix - Context

**Gathered:** 2026-03-04
**Status:** Ready for planning

<domain>
## Phase Boundary

Fix the training worker binary resolution and spawn lifecycle so it starts reliably on backend boot, restarts on crash, and reports actionable errors when it can't. No new training features — just make the existing worker actually run.

</domain>

<decisions>
## Implementation Decisions

### Binary resolution
- Config-driven path override in cp.toml, with sibling-to-server-binary fallback
- Binary name is `aos-training-worker` (not configurable)
- Validate binary exists at startup preflight — fail boot if missing
- Preflight error includes fix hint: "Training worker binary not found at {path}. Build it: cargo build -p adapteros-training-worker"

### Spawn failure behavior
- No retry on spawn failure — fail once and block boot
- Spawn is a startup preflight gate — fails at preflight phase, before binding the port
- Error is actionable with resolved path and build instructions

### Worker lifecycle
- Auto-restart on crash via backend monitoring
- Circuit breaker: 3 crashes within 5 minutes → stop restarting, mark permanently degraded
- Monitoring: tokio::process::Child for exit detection + UDS heartbeat for hang detection
- On restart, mark in-flight training jobs as failed with "training worker crashed" reason (operator re-enqueues)

### Claude's Discretion
- Log level for binary resolution path (info vs debug)
- In-flight job fate on crash (fail vs resume) — lean toward fail-with-reason given training pipeline doesn't have checkpointing
- Exact heartbeat interval and timeout thresholds
- How circuit breaker state is persisted (in-memory vs file)

</decisions>

<specifics>
## Specific Ideas

- The binary already exists at `target/debug/aos-training-worker` — the current bug is bare-name PATH lookup instead of sibling resolution
- Worker already communicates over UDS at `var/run/training-worker.sock`
- `service-manager.sh` already has worker status checks that can be extended
- Existing `training-worker.degraded` marker file in `var/run/` should be cleaned up on successful spawn

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 49-training-worker-spawn-fix*
*Context gathered: 2026-03-04*
