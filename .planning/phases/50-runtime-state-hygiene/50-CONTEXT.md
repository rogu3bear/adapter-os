# Phase 50: Runtime State Hygiene - Context

**Gathered:** 2026-03-04
**Status:** Ready for planning

<domain>
## Phase Boundary

Clean stale runtime state on boot: orphaned UDS sockets, degraded marker files, and restart counter semantics. No new services, no new monitoring — just make boot start clean.

</domain>

<decisions>
## Implementation Decisions

### Socket cleanup
- On boot, scan `var/run/` for UDS sockets and check if the owning process is alive (PID file or peer credential check)
- Delete stale sockets before binding new ones — prevents "address already in use" failures
- Cover: SecD socket, training worker socket, metrics socket, action-logs socket
- Run as early boot phase (before any service tries to bind)

### Marker lifecycle
- `training-worker.degraded` — cleared when worker spawns successfully (Phase 49 handles creation)
- `backend-supervision.state` — reset on clean boot, preserve across crash-restart
- Stale heartbeat files (e.g., `aos-secd.heartbeat`) — cleaned alongside their stale sockets
- General rule: boot cleans everything in `var/run/` that doesn't have a live process backing it

### Restart counter
- Distinguish crash restarts (process exited unexpectedly) from dev-rebuild restarts (binary replaced, launchd kicked)
- Use binary modification time vs last known boot time — if binary is newer than last boot, it's a rebuild, not a crash
- Reset counter on rebuild-detected restarts
- Persist counter in `var/run/backend-supervision.state` with JSON structure including last boot time, binary mtime, crash count

### Claude's Discretion
- Exact boot phase ordering for cleanup
- Whether to log each cleaned socket/marker (recommend: info level)
- How to handle partial cleanup failures (recommend: warn and continue)
- Whether `system_ready` marker should be removed on boot and re-created after startup completes

</decisions>

<specifics>
## Specific Ideas

- `var/run/` currently has: action-logs.sock, adapteros_status.json, aos-locks, aos-secd.heartbeat, aos-secd.sock, backend-supervision.state, boot_report.json, metrics.sock, model-load-last.json, readyz-last.json, startup_audit.jsonl, system_ready, training-worker.degraded, training-worker.sock, worker.sock
- Only sockets need stale-process checks; JSON state files should be preserved across boots
- `system_ready` marker is the boot-completion signal — should be removed at boot start and re-created at end

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 50-runtime-state-hygiene*
*Context gathered: 2026-03-04*
