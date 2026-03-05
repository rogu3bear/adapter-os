# Phase 49: Training Worker Spawn Fix - Research

**Researched:** 2026-03-04
**Domain:** Process lifecycle management, binary resolution, Tokio child process supervision
**Confidence:** HIGH

## Summary

The training worker (`aos-training-worker`) fails to start because `resolve_training_worker_bin()` in `crates/adapteros-server/src/boot/background_tasks.rs` falls through all resolution strategies and returns bare `"aos-training-worker"` -- a PATH lookup that fails because the binary is never installed to PATH. The sibling-to-current-exe check (line 156-163) works only when both `aos-server` and `aos-training-worker` are in the same directory, but the `start` script doesn't build or verify the training worker binary, and the backend's `std::env::current_exe()` may not resolve to the build directory depending on how it was launched.

The fix is straightforward: (1) add a `training_worker_bin` field to `PathsConfig` / `cp.toml` for explicit override, (2) improve the sibling fallback to be deterministic, (3) validate the binary exists at startup preflight (before port bind), and (4) harden the supervisor loop with a circuit breaker and in-flight job failure marking on crash.

**Primary recommendation:** Add config-driven binary path to `cp.toml`, fix sibling resolution, gate boot on binary existence, and implement crash-restart circuit breaker with in-flight job cleanup.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Config-driven path override in cp.toml, with sibling-to-server-binary fallback
- Binary name is `aos-training-worker` (not configurable)
- Validate binary exists at startup preflight -- fail boot if missing
- Preflight error includes fix hint: "Training worker binary not found at {path}. Build it: cargo build -p adapteros-training-worker"
- No retry on spawn failure -- fail once and block boot
- Spawn is a startup preflight gate -- fails at preflight phase, before binding the port
- Error is actionable with resolved path and build instructions
- Auto-restart on crash via backend monitoring
- Circuit breaker: 3 crashes within 5 minutes -> stop restarting, mark permanently degraded
- Monitoring: tokio::process::Child for exit detection + UDS heartbeat for hang detection
- On restart, mark in-flight training jobs as failed with "training worker crashed" reason (operator re-enqueues)

### Claude's Discretion
- Log level for binary resolution path (info vs debug)
- In-flight job fate on crash (fail vs resume) -- lean toward fail-with-reason given training pipeline doesn't have checkpointing
- Exact heartbeat interval and timeout thresholds
- How circuit breaker state is persisted (in-memory vs file)

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| WRK-01 | Training worker spawns successfully when backend starts (binary resolution fixed) | Binary resolution fix in `resolve_training_worker_bin()`, `PathsConfig.training_worker_bin` field, preflight validation, sibling fallback |
| WRK-02 | Training worker reports healthy in service status after boot | Degraded marker cleanup on success, health probe via UDS `/health`, service-manager.sh status integration |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| tokio | workspace | Async child process (`tokio::process::Child`), timers, signal handling | Already used throughout; `Child::try_wait()` for non-blocking exit detection |
| tokio::net::UnixStream | workspace | UDS health probes to training worker | Already used in `probe_training_worker_health()` |
| serde / toml | workspace | Config deserialization for `cp.toml` | Already used for `PathsConfig` |
| tracing | workspace | Structured logging for resolution path, spawn events, circuit breaker | Project standard -- never `println!` |
| anyhow | workspace | Error propagation in boot/preflight | Already used in boot sequence |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| adapteros-config | workspace | `PathsConfig`, `resolve_training_worker_socket_*` | Config-driven path override source |
| adapteros-db | workspace | `mark_training_job_failed_orphaned` / new crash variant | Mark in-flight jobs on worker crash |
| adapteros-server-api | workspace | `BootStateManager`, `failure_codes` | Boot phase tracking, preflight gate |
| chrono | workspace | Timestamp for crash tracking, circuit breaker window | Already a workspace dep |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| In-memory circuit breaker | File-persisted circuit breaker | In-memory is simpler, resets on backend restart which is correct behavior (fresh start = fresh attempts) |
| `tokio::process::Child` | `nix` crate for process management | Child already provides `try_wait()`, `start_kill()`, and `wait()` -- no need for extra dependency |

## Architecture Patterns

### Current Code Structure (files to modify)
```
crates/
├── adapteros-config/src/types.rs          # Add training_worker_bin to PathsConfig
├── adapteros-server/src/boot/
│   └── background_tasks.rs                # Fix resolve_training_worker_bin(), add preflight
├── adapteros-server/src/boot/security.rs  # Add preflight check (or new module)
├── adapteros-server/src/main.rs           # Wire preflight gate before port bind
├── adapteros-server-api/src/config.rs     # PathsConfig re-export (auto from config crate)
├── adapteros-db/src/training_jobs.rs      # Add mark_training_jobs_failed_worker_crash()
configs/
└── cp.toml                                # Add [paths] training_worker_bin field
scripts/
└── service-manager.sh                     # Already reads degraded marker -- no changes needed
```

### Pattern 1: Config-Driven Binary Resolution
**What:** Add `training_worker_bin` to `PathsConfig` with sibling-to-server-binary fallback.
**When to use:** Always -- this is the core fix.
**Example:**
```rust
// In resolve_training_worker_bin():
// 1. Check AOS_TRAINING_WORKER_BIN env var (existing, highest priority)
// 2. Check config paths.training_worker_bin from cp.toml (NEW)
// 3. Check sibling to current_exe (existing, now more robust)
// 4. Check target/debug and target/release relative to workspace root (existing)
// 5. NEVER fall back to bare name -- return Err instead of bare "aos-training-worker"
fn resolve_training_worker_bin(config: &PathsConfig) -> Result<String> {
    // Priority 1: explicit env override
    if let Ok(path) = std::env::var("AOS_TRAINING_WORKER_BIN") {
        if !path.trim().is_empty() {
            return Ok(path);
        }
    }

    // Priority 2: config file override
    if let Some(ref config_path) = config.training_worker_bin {
        if !config_path.is_empty() {
            let p = std::path::Path::new(config_path);
            if p.exists() {
                return Ok(config_path.clone());
            }
            // Config path specified but not found -- warn and continue to fallbacks
            tracing::warn!(
                config_path = %config_path,
                "Training worker binary path from config does not exist, trying fallbacks"
            );
        }
    }

    // Priority 3: sibling to current exe
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(dir) = current_exe.parent() {
            let candidate = dir.join("aos-training-worker");
            if candidate.exists() {
                return Ok(candidate.to_string_lossy().to_string());
            }
        }
    }

    // Priority 4: workspace target directories
    // ... existing root detection logic ...

    // Priority 5: FAIL -- never return bare name
    Err(anyhow::anyhow!(
        "Training worker binary not found at any search path. \
         Build it: cargo build -p adapteros-training-worker"
    ))
}
```

### Pattern 2: Startup Preflight Gate
**What:** Validate binary exists during boot preflight, before port binding.
**When to use:** Every boot -- blocks startup with actionable error.
**Example:**
```rust
// In boot sequence, before Phase 10b (worker_attach):
// This runs BEFORE bind_and_serve, so the port is never opened
// if the training worker binary is missing.
fn preflight_check_training_worker_binary(config: &PathsConfig) -> Result<String> {
    let bin_path = resolve_training_worker_bin(config)?;
    let path = std::path::Path::new(&bin_path);
    if !path.exists() {
        anyhow::bail!(
            "Training worker binary not found at {}. Build it: cargo build -p adapteros-training-worker",
            bin_path
        );
    }
    if !path.is_file() {
        anyhow::bail!(
            "Training worker path {} exists but is not a file",
            bin_path
        );
    }
    info!(path = %bin_path, "Training worker binary validated at preflight");
    Ok(bin_path)
}
```

### Pattern 3: Circuit Breaker with Crash Window
**What:** Track crash timestamps in a sliding window; after N crashes in M minutes, stop restarting.
**When to use:** In the supervisor loop (existing background task in `background_tasks.rs`).
**Example:**
```rust
struct WorkerCircuitBreaker {
    crash_timestamps: VecDeque<Instant>,
    max_crashes: u32,        // 3
    window: Duration,        // 5 minutes
    tripped: bool,
}

impl WorkerCircuitBreaker {
    fn record_crash(&mut self) -> bool {
        let now = Instant::now();
        self.crash_timestamps.push_back(now);
        // Evict crashes outside the window
        while let Some(&front) = self.crash_timestamps.front() {
            if now.duration_since(front) > self.window {
                self.crash_timestamps.pop_front();
            } else {
                break;
            }
        }
        if self.crash_timestamps.len() >= self.max_crashes as usize {
            self.tripped = true;
        }
        self.tripped
    }
}
```

### Pattern 4: In-Flight Job Failure on Crash
**What:** When worker crashes, mark all "running" training jobs as failed with crash reason.
**When to use:** In supervisor loop, after detecting worker exit.
**Example:**
```rust
// In training_jobs.rs:
pub async fn mark_running_jobs_failed_worker_crash(&self) -> Result<u64> {
    let now = chrono::Utc::now().to_rfc3339();
    let metadata = serde_json::json!({
        "failure_reason": "training_worker_crashed",
        "failure_type": "worker_crash",
        "marked_failed_at": &now,
    });
    let metadata_str = serde_json::to_string(&metadata)
        .map_err(|e| AosError::Database(format!("Failed to serialize metadata: {}", e)))?;

    // Optimistic: only update jobs that are still running
    let result = sqlx::query(
        "UPDATE repository_training_jobs SET status = 'failed', completed_at = ?, metadata_json = ?
         WHERE status = 'running'"
    )
    .bind(&now)
    .bind(&metadata_str)
    .execute(self.pool_result()?)
    .await
    .map_err(|e| AosError::Database(e.to_string()))?;

    Ok(result.rows_affected())
}
```

### Anti-Patterns to Avoid
- **Bare binary name fallback:** Never return `"aos-training-worker"` as a bare name -- it will fail with `ENOENT` and produce an unhelpful error. Always return `Result`.
- **Retrying spawn on ENOENT:** If the binary doesn't exist, retrying won't help. Fail immediately with a build instruction.
- **Silent degradation on binary-not-found:** The current code writes a degraded marker file and continues. The decision is to block boot instead.
- **Swallowing spawn errors:** Current code in some paths does `let _ = tx.send(Ok(()))` even on fallback errors. Don't signal readiness when the worker isn't ready.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Process exit detection | Custom signal handling | `tokio::process::Child::try_wait()` | Already works, handles all exit signals |
| UDS health check | Custom TCP health | `probe_training_worker_health()` (existing) | Already implemented with 2s timeout |
| Crash rate tracking | Complex persistent state | In-memory `VecDeque<Instant>` sliding window | Resets on backend restart (correct behavior), no persistence needed |
| Job failure marking | Manual SQL updates | Extend `Db` with `mark_running_jobs_failed_worker_crash()` | Follows existing `mark_training_job_failed_orphaned()` pattern |

**Key insight:** Most of the infrastructure exists. The supervisor loop, health probes, degraded markers, and job failure marking are all present. The fix is mostly about (a) making binary resolution deterministic, (b) moving from "degrade gracefully" to "fail boot", and (c) adding the circuit breaker to the existing restart logic.

## Common Pitfalls

### Pitfall 1: `current_exe()` Returns Symlink Target
**What goes wrong:** `std::env::current_exe()` follows symlinks. If `aos-server` is launched via a symlink (e.g., from `./start`), the parent directory may not be where `aos-training-worker` lives.
**Why it happens:** macOS returns the symlink-resolved path.
**How to avoid:** Use `current_exe()` as a hint, not a guarantee. Prefer config-driven path or workspace root detection.
**Warning signs:** Works in development, fails in production or when launched from different directory.

### Pitfall 2: Race Between Spawn and Health Check
**What goes wrong:** The supervisor spawns the worker and immediately probes health. The worker hasn't had time to bind the UDS socket yet.
**Why it happens:** Worker startup takes a few hundred milliseconds (DB connection, socket bind).
**How to avoid:** The existing 2-second poll interval handles this correctly. Don't add an immediate health check after spawn -- let the next tick handle it.
**Warning signs:** Flaky "worker not healthy" logs on first tick after spawn.

### Pitfall 3: Stale UDS Socket from Previous Worker
**What goes wrong:** If a previous worker crashed without cleaning up `training-worker.sock`, the new worker can't bind to it.
**Why it happens:** `UnixListener::bind()` fails with `EADDRINUSE` on existing socket files.
**How to avoid:** The worker already calls `prepare_socket_path()` which removes stale sockets. Ensure preflight doesn't interfere with this. The `probe_training_worker_health()` function correctly checks if the socket is live before adoption.
**Warning signs:** "Address already in use" errors in worker logs.

### Pitfall 4: `kill_on_drop` Races with Graceful Shutdown
**What goes wrong:** `Command::kill_on_drop(true)` sends SIGKILL when the `Child` is dropped. If the supervisor loop exits before calling `terminate_managed_training_worker()`, the worker gets SIGKILL instead of graceful shutdown.
**Why it happens:** The existing code has `kill_on_drop(true)` as a safety net.
**How to avoid:** This is actually correct behavior -- `kill_on_drop` is the fallback. The existing `shutdown_rx` handling calls `terminate_managed_training_worker()` first (SIGKILL + 5s wait), which is fine.
**Warning signs:** None -- this is working correctly.

### Pitfall 5: Circuit Breaker Window vs Dev Rebuild Restarts
**What goes wrong:** During development, `cargo build` replaces the binary, the worker detects its binary changed and exits, triggering the circuit breaker.
**Why it happens:** Worker exit during cargo build looks like a crash to the supervisor.
**How to avoid:** Count only non-zero exit codes as crashes. Normal `cargo build` causes the running binary to be replaced, and the next `try_wait()` sees the process exited -- but this is an edge case. The 5-minute window and 3-crash threshold should be generous enough.
**Warning signs:** Circuit breaker tripping during development.

## Code Examples

### Existing `resolve_training_worker_bin()` (current, broken)
```rust
// Source: crates/adapteros-server/src/boot/background_tasks.rs:149-192
fn resolve_training_worker_bin() -> String {
    if let Ok(path) = std::env::var("AOS_TRAINING_WORKER_BIN") { ... }
    if let Ok(current_exe) = std::env::current_exe() { ... }
    // ... workspace target checks ...
    "aos-training-worker".to_string()  // <-- THE BUG: bare name, not in PATH
}
```

### Existing `PathsConfig` (needs `training_worker_bin` field)
```rust
// Source: crates/adapteros-config/src/types.rs:152-166
pub struct PathsConfig {
    pub artifacts_root: String,
    pub bundles_root: String,
    pub adapters_root: String,
    pub plan_dir: String,
    pub datasets_root: String,
    pub documents_root: String,
    pub synthesis_model_path: Option<String>,
    // MISSING: training_worker_bin: Option<String>,
}
```

### Existing Supervisor Loop Structure
```rust
// Source: crates/adapteros-server/src/boot/background_tasks.rs:340-552
// The supervisor loop already handles:
// - Probing for existing healthy worker (adoption)
// - Spawning new worker
// - Detecting exit via try_wait()
// - Restart with backoff
// - Fallback mode with degraded marker
// MISSING:
// - Circuit breaker (3 crashes in 5 min)
// - In-flight job failure marking on crash
// - Preflight binary validation
```

### Existing `mark_training_job_failed_orphaned()` Pattern
```rust
// Source: crates/adapteros-db/src/training_jobs.rs:3472-3531
// Pattern to follow for crash-related job failure:
// 1. Build metadata JSON with failure_reason
// 2. UPDATE ... SET status='failed' WHERE status='running' (optimistic locking)
// 3. Log with structured tracing
```

### Config TOML Addition
```toml
# In configs/cp.toml, under [paths]:
[paths]
# ... existing fields ...
# Path to training worker binary. If unset, resolves from sibling directory or target/.
# training_worker_bin = "target/debug/aos-training-worker"
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Bare PATH lookup fallback | Config + sibling + workspace + fail | This phase | Eliminates ENOENT spawn failure |
| Silent degradation on missing binary | Boot preflight gate | This phase | Actionable errors before port bind |
| Unlimited restart attempts | Circuit breaker (3 in 5 min) | This phase | Prevents crash-loop resource waste |
| Orphaned job cleanup only | Crash-triggered job failure | This phase | Faster training job status resolution |

## Open Questions

1. **Log level for resolution path**
   - What we know: The resolution path is useful for debugging but noisy in normal operation.
   - Recommendation: `info!` for the final resolved path, `debug!` for each candidate checked. This matches the existing pattern in `resolve_training_worker_socket_*`.

2. **Heartbeat interval and timeout**
   - What we know: The existing supervisor polls every 2 seconds (`tokio::time::interval(Duration::from_secs(2))`).
   - What's unclear: Whether 2s is optimal for hang detection.
   - Recommendation: Keep 2s poll interval (already fast enough). Hang detection via UDS `/health` probe is already built-in. A worker that can't respond to health in 2s is likely hung.

3. **Circuit breaker state: in-memory vs file**
   - What we know: In-memory resets on backend restart, file persists across restarts.
   - Recommendation: **In-memory.** Backend restart = fresh start = should retry worker spawn. The degraded marker file (`training-worker.degraded`) already provides persistence for the "permanently degraded" case. The circuit breaker just prevents crash-loop thrashing within a single backend session.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (built-in) |
| Config file | Cargo.toml per crate |
| Quick run command | `cargo test -p adapteros-server --lib -- training_worker` |
| Full suite command | `cargo test -p adapteros-server && cargo test -p adapteros-config -p adapteros-db` |
| Estimated runtime | ~30 seconds |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| WRK-01 | Binary resolution finds sibling binary | unit | `cargo test -p adapteros-server --lib -- resolve_training_worker_bin` | No -- Wave 0 gap |
| WRK-01 | Binary resolution uses config override | unit | `cargo test -p adapteros-server --lib -- resolve_training_worker_bin` | No -- Wave 0 gap |
| WRK-01 | Binary resolution fails with actionable error when missing | unit | `cargo test -p adapteros-server --lib -- resolve_training_worker_bin` | No -- Wave 0 gap |
| WRK-01 | Preflight blocks boot when binary missing | unit | `cargo test -p adapteros-server --lib -- preflight_training_worker` | No -- Wave 0 gap |
| WRK-01 | PathsConfig deserializes training_worker_bin from TOML | unit | `cargo test -p adapteros-config -- paths_config` | No -- Wave 0 gap |
| WRK-02 | Degraded marker cleared on successful spawn | unit | `cargo test -p adapteros-server --lib -- degraded_marker` | No -- Wave 0 gap |
| WRK-02 | Circuit breaker trips after 3 crashes in 5 minutes | unit | `cargo test -p adapteros-server --lib -- circuit_breaker` | No -- Wave 0 gap |
| WRK-02 | In-flight jobs marked failed on worker crash | unit | `cargo test -p adapteros-db -- mark_running_jobs_failed_worker_crash` | No -- Wave 0 gap |

### Nyquist Sampling Rate
- **Minimum sample interval:** After every committed task -> run: `cargo check -p adapteros-server -p adapteros-config -p adapteros-db`
- **Full suite trigger:** Before merging final task of any plan wave
- **Phase-complete gate:** `cargo test -p adapteros-server --lib && cargo test -p adapteros-config && cargo test -p adapteros-db`
- **Estimated feedback latency per task:** ~15 seconds (check), ~30 seconds (test)

### Wave 0 Gaps (must be created before implementation)
- [ ] `crates/adapteros-server/src/boot/background_tasks.rs` -- unit tests for `resolve_training_worker_bin()` (extract to testable function accepting config)
- [ ] `crates/adapteros-server/src/boot/background_tasks.rs` -- unit tests for circuit breaker logic (extract to struct)
- [ ] `crates/adapteros-config/src/types.rs` -- test `PathsConfig` TOML roundtrip with `training_worker_bin`
- [ ] `crates/adapteros-db/src/training_jobs.rs` -- test `mark_running_jobs_failed_worker_crash()`

## Sources

### Primary (HIGH confidence)
- `crates/adapteros-server/src/boot/background_tasks.rs` -- current spawn/supervisor implementation (lines 149-217, 340-666)
- `crates/adapteros-config/src/types.rs` -- `PathsConfig` struct (lines 152-166)
- `crates/adapteros-config/src/path_resolver.rs` -- socket resolution patterns (lines 518-570)
- `crates/adapteros-server/src/main.rs` -- boot phase sequence (lines 430-454)
- `crates/adapteros-server/src/boot/startup_orchestrator.rs` -- retry/circuit breaker pattern for boot phases
- `crates/adapteros-db/src/training_jobs.rs` -- `mark_training_job_failed_orphaned()` pattern (lines 3472-3531)
- `crates/adapteros-training-worker/src/main.rs` -- worker binary entry point
- `configs/cp.toml` -- current config structure (no training worker binary path)
- `scripts/service-manager.sh` -- degraded marker display (line 2363-2369)
- `scripts/golden_path_adapter_chat.sh` -- binary resolution reference (lines 44-93)

### Secondary (MEDIUM confidence)
- `scripts/start` -- binary resolution for other services (`require_binary_or_cargo`, lines 1323-1422)
- `crates/adapteros-server/src/boot/security.rs` -- preflight check patterns

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all libraries already in use, no new dependencies needed
- Architecture: HIGH -- extending existing patterns (PathsConfig, supervisor loop, job failure marking)
- Pitfalls: HIGH -- verified against actual codebase, all failure modes observed in existing code

**Research date:** 2026-03-04
**Valid until:** 2026-04-04 (stable domain, internal codebase)
