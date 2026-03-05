# Phase 50: Runtime State Hygiene - Research

**Researched:** 2026-03-04
**Domain:** Boot sequence runtime state cleanup, UDS socket lifecycle, process supervision
**Confidence:** HIGH

## Summary

This phase addresses three distinct but related problems in the adapterOS boot sequence: stale UDS sockets that cause "address already in use" bind failures, a training worker degraded marker that persists across boots even when the worker spawns successfully, and a backend restart counter that conflates crash restarts with developer-initiated rebuilds (currently showing 311 restarts due to launchd kickstarts being counted as crashes).

The codebase already has most of the infrastructure needed. The boot sequence in `crates/adapteros-server/src/main.rs` runs 12 numbered phases. Socket cleanup should run as the very first action in Phase 1 (config), before any service attempts to bind. The service-manager.sh already has per-service socket cleanup logic (lines 1560-1566 for worker, 1929-1935 for SecD), but it runs only per-service -- a centralized boot-time sweep is missing. The supervision state file at `var/run/backend-supervision.state` uses a simple key=value format (not JSON as the CONTEXT.md suggests), and `record_backend_restart_event()` in `scripts/launchd/aos-launchd-ensure.sh` is the only writer. The `training-worker.degraded` marker is created by Phase 49's spawn logic and needs to be cleared when the worker starts successfully.

**Primary recommendation:** Add a `clean_stale_runtime_state()` function in `adapteros-boot` (or in the server's boot module), called at the top of `initialize_config()`. This function scans `var/run/` for socket files, checks PID files for liveness, and removes stale sockets. Separately, modify the supervision state format to include `binary_mtime` for crash-vs-rebuild discrimination, and clear the degraded marker in the training worker spawn success path.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- On boot, scan `var/run/` for UDS sockets and check if the owning process is alive (PID file or peer credential check)
- Delete stale sockets before binding new ones -- prevents "address already in use" failures
- Cover: SecD socket, training worker socket, metrics socket, action-logs socket
- Run as early boot phase (before any service tries to bind)
- `training-worker.degraded` -- cleared when worker spawns successfully (Phase 49 handles creation)
- `backend-supervision.state` -- reset on clean boot, preserve across crash-restart
- Stale heartbeat files (e.g., `aos-secd.heartbeat`) -- cleaned alongside their stale sockets
- General rule: boot cleans everything in `var/run/` that doesn't have a live process backing it
- Distinguish crash restarts from dev-rebuild restarts using binary modification time vs last known boot time
- Reset counter on rebuild-detected restarts
- Persist counter in `var/run/backend-supervision.state` with JSON structure including last boot time, binary mtime, crash count

### Claude's Discretion
- Exact boot phase ordering for cleanup
- Whether to log each cleaned socket/marker (recommend: info level)
- How to handle partial cleanup failures (recommend: warn and continue)
- Whether `system_ready` marker should be removed on boot and re-created after startup completes

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| RTH-01 | Stale SecD socket is cleaned up on boot when no backing process exists | Socket cleanup function scanning `var/run/*.sock`, PID file liveness check (`var/secd.pid`), called before Phase 2 |
| RTH-02 | Training worker degraded marker is cleared when worker successfully starts | Clear `var/run/training-worker.degraded` in training worker spawn success path (Phase 49's `spawn_training_worker_managed`) |
| RTH-03 | Backend restart counter reflects actual crash count, not dev-rebuild kickstarts | Migrate `backend-supervision.state` to JSON format with `binary_mtime`, compare against current binary mtime on boot to distinguish crash from rebuild |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| std::fs | stdlib | File I/O for socket/marker cleanup | No external deps needed |
| std::os::unix::net::UnixStream | stdlib | `connect()` to test if socket has a live listener | More reliable than PID check for sockets |
| serde_json | workspace | JSON format for supervision state file | Already a workspace dep, replaces key=value format |
| chrono | workspace | Timestamps for boot time, restart events | Already a workspace dep |
| tracing | workspace | Structured logging of cleanup actions | Project standard |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| adapteros-core | workspace | `resolve_var_dir()` for var directory path | Path resolution for runtime state directory |
| adapteros-boot | workspace | `ensure_runtime_dir()` already called in Phase 1 | Hook cleanup into existing boot infrastructure |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `UnixStream::connect()` for liveness | PID file check + `kill -0` | PID check is unreliable (PID reuse, stale PID files); `connect()` directly tests if anything is listening |
| JSON supervision state | Keep key=value format | JSON is more extensible and easier to evolve; current format has 3 fields, migration is trivial |

## Architecture Patterns

### Current Runtime State Layout
```
var/
├── backend.pid                     # Backend process PID
├── worker.pid                      # Worker process PID
├── secd.pid                        # SecD process PID
├── worker.id                       # Worker UUID
├── worker.start_ts                 # Worker start timestamp
├── worker.restart_count            # Worker restart count (int)
└── run/
    ├── action-logs.sock            # UDS socket - needs stale check
    ├── aos-secd.sock               # UDS socket - needs stale check
    ├── aos-secd.heartbeat          # Heartbeat file - clean with socket
    ├── metrics.sock                # UDS socket - needs stale check
    ├── training-worker.sock        # UDS socket - needs stale check
    ├── worker.sock                 # UDS socket - needs stale check
    ├── training-worker.degraded    # Marker file - clear on successful spawn
    ├── backend-supervision.state   # Supervision state - migrate to JSON
    ├── system_ready                # Boot completion marker - remove on boot start
    ├── boot_report.json            # Boot report - preserve
    ├── adapteros_status.json       # Status JSON - preserve (overwritten each cycle)
    ├── startup_audit.jsonl         # Audit log - preserve
    ├── model-load-last.json        # Model status - preserve
    ├── readyz-last.json            # Readiness status - preserve
    ├── aos-locks/                  # Lock directory - preserve
    └── service-control.lock/       # Lock directory - preserve
```

### Pattern 1: Socket Liveness Check
**What:** For each `*.sock` file in `var/run/`, attempt a non-blocking `UnixStream::connect()`. If it fails with `ConnectionRefused` or `NotFound`, the socket is stale.
**When to use:** At boot, before any service binds.
**Example:**
```rust
use std::os::unix::net::UnixStream;
use std::path::Path;

fn is_socket_stale(path: &Path) -> bool {
    match UnixStream::connect(path) {
        Ok(_stream) => false, // Something is listening -- not stale
        Err(e) => matches!(
            e.kind(),
            std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
        ),
    }
}
```

### Pattern 2: PID-Backed Liveness (fallback)
**What:** For services with PID files (backend, worker, secd), check if the PID is alive via `kill(pid, 0)`.
**When to use:** As additional signal alongside socket connect check; primary for non-socket resources (heartbeat files).
**Example:**
```rust
fn is_pid_alive(pid_path: &Path) -> bool {
    let Ok(content) = std::fs::read_to_string(pid_path) else { return false };
    let Ok(pid) = content.trim().parse::<i32>() else { return false };
    // kill(pid, 0) checks if process exists without sending a signal
    unsafe { libc::kill(pid, 0) == 0 }
}
```

### Pattern 3: Supervision State Migration
**What:** Convert `backend-supervision.state` from key=value to JSON. Include `binary_mtime` field.
**When to use:** On first boot after this change, and every subsequent boot.
**Example:**
```rust
#[derive(Serialize, Deserialize)]
struct SupervisionState {
    restart_count: u32,
    crash_restart_count: u32,
    last_restart_cause: String,
    last_restart_ts: String,
    last_boot_ts: String,
    binary_mtime: String, // ISO-8601 of the server binary's modification time
}
```

### Pattern 4: Boot-Start Cleanup Sequence
**What:** Remove `system_ready` marker at boot start, re-create after finalization.
**When to use:** At the very beginning of Phase 1, before config loading.
**Why:** `system_ready` is a boot-completion signal. If the server crashed mid-boot, a stale `system_ready` would falsely indicate readiness.

### Anti-Patterns to Avoid
- **Deleting sockets that have live listeners:** Always check liveness before removing. A concurrent service (e.g., SecD started independently) might be using the socket.
- **Blocking boot on cleanup failures:** Socket cleanup is best-effort. If a file can't be removed (permissions, in-use), warn and continue.
- **Using `HashMap` in the boot path for listing sockets:** The number of sockets is tiny (5-6). A simple `Vec` or hardcoded list is fine.
- **Changing the supervision state format without backward compatibility:** The guardian script `aos-launchd-ensure.sh` reads the state file with `awk`. After migrating to JSON, the guardian script must also be updated to use `jq` or a shell JSON parser.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Socket liveness | Custom protocol handshake | `UnixStream::connect()` | Kernel tells you if anyone is listening; no protocol needed |
| Process liveness | Parse `/proc` or `ps` output | `libc::kill(pid, 0)` or `nix::sys::signal::kill(pid, None)` | Direct syscall, no parsing |
| JSON file atomic write | Direct `fs::write()` | Write to `.tmp` then `rename()` | Prevents corrupted state on crash mid-write |
| Directory scanning | `walkdir` crate | `std::fs::read_dir()` | `var/run/` is flat, no recursion needed |

## Common Pitfalls

### Pitfall 1: Socket Connect Blocks or Hangs
**What goes wrong:** `UnixStream::connect()` is blocking. If a service is in a bad state (accepting connections but not reading), it could hang.
**Why it happens:** Default connect has no timeout.
**How to avoid:** Use `std::os::unix::net::UnixStream::connect_addr()` is not timeout-aware, but for boot cleanup, the socket connect is to localhost and will fail fast if no listener exists. The concern is low since we only care about `ConnectionRefused` vs success.
**Warning signs:** Boot takes unexpectedly long.

### Pitfall 2: Supervision State File Race with Guardian
**What goes wrong:** The launchd guardian (`aos-launchd-ensure.sh`) writes the supervision state file every 30 seconds. If the boot cleanup modifies the file concurrently, data could be lost.
**Why it happens:** No file locking between guardian and boot.
**How to avoid:** Boot cleanup reads the supervision state, updates in-memory, writes atomically (tmp+rename). The guardian uses the same pattern. File rename is atomic on the same filesystem.
**Warning signs:** Restart count jumps or resets unexpectedly.

### Pitfall 3: PID File Points to Wrong Process
**What goes wrong:** After a crash, the PID in `var/backend.pid` might point to a completely different process (PID reuse).
**Why it happens:** macOS reuses PIDs relatively quickly (sequential allocation wraps).
**How to avoid:** Combine PID check with socket connect check. If PID is alive but socket connect fails, treat socket as stale (the PID is a different process).
**Warning signs:** Stale sockets not cleaned because PID check returns "alive".

### Pitfall 4: Guardian Script After JSON Migration
**What goes wrong:** `aos-launchd-ensure.sh` uses `awk -F= '/^restart_count=/{print $2}'` to read the state file. After migrating to JSON, this breaks.
**Why it happens:** Format change without updating all readers/writers.
**How to avoid:** Update both: (1) the Rust boot code that reads/writes the state, and (2) the guardian script that reads/writes it. The `service-manager.sh status` command also reads it (line 2232-2235).
**Warning signs:** Guardian shows 0 restarts after migration, or crashes on parse.

### Pitfall 5: system_ready Removed but Never Re-created
**What goes wrong:** If boot crashes after removing `system_ready` but before finalization, the marker stays missing. External monitoring sees the system as down.
**Why it happens:** `system_ready` is written by the health handler's `update_ready_state_on_disk()` function, which runs as part of the periodic system_ready health check. It is NOT written as part of the boot sequence directly -- it's written by the background health check task after boot completes and the first successful health check runs.
**How to avoid:** This is actually safe -- the `system_ready` marker is always produced by the health check loop, not by boot. Removing it at boot start is correct behavior. The health check will re-create it once all components report healthy.
**Warning signs:** None -- this is the correct behavior.

## Code Examples

### Stale Socket Cleanup Function
```rust
// Location: crates/adapteros-server/src/boot/config.rs (or new module)
use std::path::Path;
use tracing::{info, warn};

/// Known UDS socket files in var/run/
const KNOWN_SOCKETS: &[&str] = &[
    "aos-secd.sock",
    "training-worker.sock",
    "worker.sock",
    "metrics.sock",
    "action-logs.sock",
];

/// Associated PID files (in var/) for each socket
const SOCKET_PID_MAP: &[(&str, &str)] = &[
    ("aos-secd.sock", "secd.pid"),
    ("training-worker.sock", "worker.pid"),
    ("worker.sock", "worker.pid"),
    // metrics.sock and action-logs.sock are in-process -- no separate PID
];

/// Associated heartbeat files to clean with their sockets
const SOCKET_HEARTBEAT_MAP: &[(&str, &str)] = &[
    ("aos-secd.sock", "aos-secd.heartbeat"),
];

pub fn clean_stale_runtime_state(var_dir: &Path) {
    let run_dir = var_dir.join("run");
    if !run_dir.exists() {
        return;
    }

    // Remove system_ready marker (will be re-created by health check)
    let system_ready = run_dir.join("system_ready");
    if system_ready.exists() {
        match std::fs::remove_file(&system_ready) {
            Ok(()) => info!(path = %system_ready.display(), "Removed system_ready marker (will re-create after boot)"),
            Err(e) => warn!(error = %e, path = %system_ready.display(), "Failed to remove system_ready marker"),
        }
    }

    // Clean stale sockets
    for socket_name in KNOWN_SOCKETS {
        let socket_path = run_dir.join(socket_name);
        if !socket_path.exists() {
            continue;
        }

        if is_socket_live(&socket_path) {
            info!(socket = %socket_name, "Socket has live listener, keeping");
            continue;
        }

        // Socket is stale -- remove it
        match std::fs::remove_file(&socket_path) {
            Ok(()) => info!(socket = %socket_name, "Removed stale socket"),
            Err(e) => warn!(error = %e, socket = %socket_name, "Failed to remove stale socket"),
        }

        // Clean associated heartbeat file
        for (sock, heartbeat) in SOCKET_HEARTBEAT_MAP {
            if *sock == *socket_name {
                let hb_path = run_dir.join(heartbeat);
                if hb_path.exists() {
                    match std::fs::remove_file(&hb_path) {
                        Ok(()) => info!(file = %heartbeat, "Removed stale heartbeat file"),
                        Err(e) => warn!(error = %e, file = %heartbeat, "Failed to remove stale heartbeat"),
                    }
                }
            }
        }
    }
}

fn is_socket_live(path: &Path) -> bool {
    use std::os::unix::net::UnixStream;
    UnixStream::connect(path).is_ok()
}
```

### Supervision State JSON Format
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct SupervisionState {
    /// Total crash-restart count (excludes rebuild restarts)
    pub crash_restart_count: u32,
    /// Total restart count (all causes)
    pub total_restart_count: u32,
    /// Cause of last restart
    pub last_restart_cause: String,
    /// ISO-8601 timestamp of last restart
    pub last_restart_ts: String,
    /// ISO-8601 timestamp of last clean boot
    pub last_boot_ts: String,
    /// ISO-8601 of the server binary's mtime at last boot
    pub binary_mtime: String,
}

impl SupervisionState {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn write_atomic(&self, path: &Path) -> std::io::Result<()> {
        let tmp = path.with_extension("tmp");
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, path)
    }

    /// Returns true if the current binary is a rebuild (binary mtime changed)
    pub fn is_rebuild(&self, current_binary_mtime: &str) -> bool {
        !self.binary_mtime.is_empty() && self.binary_mtime != current_binary_mtime
    }
}
```

### Guardian Script Update (shell)
```bash
# In aos-launchd-ensure.sh -- replace awk-based reader with jq
record_backend_restart_event() {
    local cause="$1"
    local ts
    ts="$(date -Iseconds)"

    local state="{}"
    if [ -f "$BACKEND_METRICS_FILE" ]; then
        state="$(cat "$BACKEND_METRICS_FILE" 2>/dev/null || echo "{}")"
    fi

    # Parse existing counts (default to 0)
    local total_count crash_count
    total_count=$(echo "$state" | jq -r '.total_restart_count // 0' 2>/dev/null || echo 0)
    crash_count=$(echo "$state" | jq -r '.crash_restart_count // 0' 2>/dev/null || echo 0)
    total_count=$((total_count + 1))

    # Only increment crash count for actual crashes (not rebuilds)
    if [[ "$cause" != *"rebuild"* ]]; then
        crash_count=$((crash_count + 1))
    fi

    local tmp_file="${BACKEND_METRICS_FILE}.tmp"
    jq -n \
        --argjson total "$total_count" \
        --argjson crash "$crash_count" \
        --arg cause "$cause" \
        --arg ts "$ts" \
        --arg boot_ts "$(echo "$state" | jq -r '.last_boot_ts // ""' 2>/dev/null || echo "")" \
        --arg mtime "$(echo "$state" | jq -r '.binary_mtime // ""' 2>/dev/null || echo "")" \
        '{total_restart_count: $total, crash_restart_count: $crash, last_restart_cause: $cause, last_restart_ts: $ts, last_boot_ts: $boot_ts, binary_mtime: $mtime}' \
        > "$tmp_file"
    mv "$tmp_file" "$BACKEND_METRICS_FILE"
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Per-service socket cleanup in service-manager.sh | Centralized boot-time sweep | This phase | Catches sockets not covered by service-manager start paths |
| Key=value supervision state | JSON supervision state with binary_mtime | This phase | Enables crash vs rebuild discrimination |
| Restart counter counts all restarts | Separate crash_restart_count and total_restart_count | This phase | Health endpoint shows meaningful crash count |
| No system_ready cleanup on boot | Remove at boot start, re-create via health check | This phase | Prevents stale readiness signal after crash |

## Open Questions

1. **Guardian script jq dependency**
   - What we know: `jq` is standard on macOS (ships with Xcode command line tools / Homebrew). The guardian script currently uses `awk`.
   - What's unclear: Whether `jq` is guaranteed available in the launchd environment.
   - Recommendation: Check for `jq` availability in the guardian script; fall back to `python3 -c 'import json...'` if not found. Alternatively, keep a dual-format reader that can parse both key=value (legacy) and JSON (new).

2. **Backward compatibility of state file format**
   - What we know: `service-manager.sh status` (line 2232-2235) reads with awk. `aos-launchd-ensure.sh` writes with echo/awk. Both need updating.
   - What's unclear: Whether there are other consumers.
   - Recommendation: Update both scripts in the same commit. Use `jq` with a fallback parser. The Rust code should be the canonical reader/writer going forward.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (built-in) |
| Config file | Cargo.toml per crate |
| Quick run command | `cargo test -p adapteros-server --lib` |
| Full suite command | `cargo test -p adapteros-server && cargo test -p adapteros-boot` |
| Estimated runtime | ~15 seconds |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| RTH-01 | Stale socket cleaned on boot | unit | `cargo test -p adapteros-server --lib -- boot::test_stale_socket_cleanup -x` | No - Wave 0 gap |
| RTH-01 | Live socket preserved on boot | unit | `cargo test -p adapteros-server --lib -- boot::test_live_socket_preserved -x` | No - Wave 0 gap |
| RTH-02 | Degraded marker cleared on worker success | unit | `cargo test -p adapteros-server --lib -- boot::test_degraded_marker_cleared -x` | No - Wave 0 gap |
| RTH-03 | Crash restart increments crash counter | unit | `cargo test -p adapteros-server --lib -- boot::test_crash_restart_counted -x` | No - Wave 0 gap |
| RTH-03 | Rebuild restart resets crash counter | unit | `cargo test -p adapteros-server --lib -- boot::test_rebuild_restart_resets -x` | No - Wave 0 gap |
| RTH-03 | Supervision state JSON round-trip | unit | `cargo test -p adapteros-server --lib -- boot::test_supervision_state_serde -x` | No - Wave 0 gap |

### Nyquist Sampling Rate
- **Minimum sample interval:** After every committed task -> run: `cargo test -p adapteros-server --lib`
- **Full suite trigger:** Before merging final task of any plan wave
- **Phase-complete gate:** Full suite green before `/gsd:verify-work`
- **Estimated feedback latency per task:** ~15 seconds

### Wave 0 Gaps (must be created before implementation)
- [ ] Tests for socket cleanup logic (create temp UDS, verify cleanup, verify live sockets preserved)
- [ ] Tests for supervision state JSON serde round-trip
- [ ] Tests for crash-vs-rebuild discrimination logic
- [ ] Tests for degraded marker lifecycle

## Sources

### Primary (HIGH confidence)
- Codebase inspection: `crates/adapteros-server/src/main.rs` - Boot phase ordering (Phases 1-12)
- Codebase inspection: `crates/adapteros-server/src/boot/startup_recovery.rs` - Existing recovery patterns
- Codebase inspection: `scripts/launchd/aos-launchd-ensure.sh` - Guardian supervision state writer
- Codebase inspection: `scripts/service-manager.sh` - Socket cleanup per-service, supervision state reader
- Codebase inspection: `crates/adapteros-secd/src/main.rs` - SecD socket/heartbeat/PID lifecycle
- Codebase inspection: `crates/adapteros-server-api/src/health.rs` - system_ready marker writer
- Live inspection: `var/run/backend-supervision.state` - Current format: `restart_count=311` / key=value
- Live inspection: `var/run/training-worker.degraded` - Current content: spawn failure message

### Secondary (MEDIUM confidence)
- Unix domain socket behavior: `UnixStream::connect()` returns `ConnectionRefused` when no listener exists (Rust std library documentation, verified against POSIX behavior)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - All stdlib + existing workspace deps, no new dependencies
- Architecture: HIGH - Boot sequence is well-understood, all insertion points identified
- Pitfalls: HIGH - Guardian script compat and PID reuse are real risks, documented with mitigations

**Research date:** 2026-03-04
**Valid until:** 2026-04-04 (stable domain, no moving targets)
