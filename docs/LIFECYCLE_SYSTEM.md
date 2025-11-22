# AdapterOS Lifecycle System

**Document Purpose:** Complete reference for launch → shutdown lifecycle across all subsystems.
**Last Updated:** 2025-11-22
**Scope:** Server startup, runtime operation, graceful shutdown, and component cleanup.

---

## 1. System Architecture Overview

The AdapterOS lifecycle spans **9 distinct phases** with **9 major subsystems** operating in coordination:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         COMPLETE LIFECYCLE                              │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  PHASE 1: CLI Entry       PHASE 2: Server Init    PHASE 3: Preflight   │
│  ├─ .env loading          ├─ Config loading       ├─ PF/firewall       │
│  ├─ Tracing init          ├─ PID lock             ├─ Device fingerprint│
│  ├─ Arg parsing           ├─ Shutdown coordinator ├─ Env validation    │
│  └─ Output setup          └─ Executor seeding     └─ Security checks   │
│                                                                          │
│  PHASE 4: Database        PHASE 5: Subsystems    PHASE 6: Runtime     │
│  ├─ Pool creation         ├─ Telemetry           ├─ Worker init       │
│  ├─ Migrations            ├─ Federation daemon   ├─ Backend creation  │
│  ├─ Crash recovery        ├─ UDS metrics         ├─ Adapter lifecycle │
│  └─ Dev seeding           ├─ Git subsystem       ├─ Hot-swap setup    │
│                           ├─ Policy watcher      └─ Background tasks  │
│                           ├─ Alert watcher       │                    │
│                           ├─ Memory watchdog     │ STEADY STATE       │
│                           └─ UMA monitor         │ (accept requests)  │
│                                                  │                    │
│  PHASE 7: Graceful        PHASE 8: Component    PHASE 9: Process     │
│  Shutdown                 Cleanup                Exit                 │
│  ├─ Signal reception       ├─ Telemetry flush    ├─ Audit logging     │
│  ├─ Broadcast shutdown     ├─ DB pool cleanup    ├─ Emit exit events  │
│  ├─ Ordered teardown       ├─ FFI cleanup        └─ Process exit      │
│  └─ Timeout management     └─ Resource release   └─ Exit code         │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Phase 1: CLI Entry (aosctl)

**File:** `crates/adapteros-cli/src/main.rs:1092-1145`
**Duration:** <1s
**Critical:** No

### Sequence

1. **Load environment variables** (Line 1095)
   - Via `adapteros_config::load_dotenv()`
   - Reads `.env` file for configuration overrides

2. **Initialize logging** (Line 1098)
   - Via `init_logging()` in `logging.rs:40-61`
   - Sets up `tracing_subscriber` with EnvFilter

3. **Parse CLI arguments** (Line 1100)
   - Via `Cli::parse()` using `clap` parser
   - Extracts command, global flags (`--json`, `--quiet`, `--verbose`), model config

4. **Create output writer** (Lines 1103-1104)
   - Via `OutputWriter::new(mode, verbose)`
   - Formats output based on flags

5. **Extract metadata** (Lines 1107-1108)
   - Command name and tenant ID for telemetry
   - Used in error reporting and event tracking

6. **Execute command** (Line 1111)
   - Dispatches to appropriate handler (e.g., `serve::run()`)
   - Handlers implement specific command logic

### Subcommand: `serve`

**File:** `crates/adapteros-cli/src/commands/serve.rs:75-407`

The `serve` subcommand launches the AdapterOS server with validation:

```rust
aosctl serve [OPTIONS] <TENANT> [PLAN]
  --plan <PLAN>              [default: "default"]
  --socket <SOCKET>          [default: "var/run/aos.sock"]
  --backend <BACKEND>        Metal | CoreML | Mlx | Auto [default: Auto]
  --dry-run                  Preflight checks only
```

---

## Phase 2: Server Initialization

**File:** `crates/adapteros-server/src/main.rs:129-375`
**Duration:** 1-5s
**Critical:** Yes

### Sequence

#### 2.1: Configuration & PID Lock (Lines 130-155)

```rust
// Load & validate TOML config
let config = Config::load(&cli.config)?;

// Acquire PID lock to prevent concurrent instances
let _pid_guard = acquire_pid_lock("/var/run/aos/cp.pid")?;
```

**Config Precedence:** CLI > Environment > File > Defaults

#### 2.2: Shutdown Coordinator (Lines 161-162)

```rust
let shutdown_coordinator = ShutdownCoordinator::new();
// See Phase 7 for shutdown details
```

**Key Methods:**
- `subscribe_shutdown()` - Returns `broadcast::Receiver<()>`
- `set_*_handle()` - Register component handles (telemetry, federation, etc.)
- `shutdown()` - Orchestrate graceful component shutdown

#### 2.3: Deterministic Executor Initialization (Lines 164-266)

```rust
// Load manifest (e.g., models/qwen2.5-7b-mlx/manifest.json)
let manifest = serde_json::from_str::<ManifestV3>(...)?;
manifest.validate()?;

// Compute manifest hash
let manifest_hash = manifest.compute_hash()?;

// Derive executor seed via HKDF
let global_seed = derive_seed(&manifest_hash, "executor");

// Initialize executor with deterministic config
init_global_executor(ExecutorConfig {
    global_seed,
    enable_event_logging: true,
    ..Default::default()
})?;
```

**Determinism Guarantee:** All randomness seeded from manifest hash → reproducible execution

#### 2.4: MLX Runtime (Lines 268-279)

```rust
#[cfg(feature = "multi-backend")]
{
    adapteros_lora_mlx_ffi::mlx_runtime_init()?;
    // Falls back to Metal/CoreML if MLX unavailable
}
```

---

## Phase 3: Security Preflight

**File:** `crates/adapteros-server/src/security.rs:1-163`
**Duration:** 0.5-2s
**Critical:** Yes (production-blocking)

### Sequence

#### 3.1: PF/Firewall Security Check (Lines 282-292)

```rust
if config.security.require_pf_deny {
    PfGuard::preflight(&cfg.security)?;
}
```

**macOS (pfctl):** Validates egress blocking rules
**Linux (iptables):** Validates port restrictions

**Production Mode Requirement:** UDS-only (no TCP egress allowed)

#### 3.2: Device Fingerprint Drift Detection (Lines 294-373)

```rust
let current_fingerprint = DeviceFingerprint::capture_current()?;
let baseline_path = Path::new("var/baseline_fingerprint.json");

if baseline_path.exists() {
    let baseline = DeviceFingerprint::from_file(&baseline_path)?;
    if current_fingerprint.drift_score(&baseline) > CRITICAL_DRIFT_THRESHOLD {
        return Err(AosError::Config("Critical fingerprint drift detected".into()));
    }
} else {
    // First run: save baseline
    current_fingerprint.save(&baseline_path)?;
}
```

**Drift Factors:**
- CPU model/count
- GPU device names
- Total memory
- Kernel version
- Unique IDs (serial, UUIDs)

---

## Phase 4: Database Setup

**File:** `crates/adapteros-db/src/lib.rs:40-140`
**Duration:** 1-10s (depends on migration count)
**Critical:** Yes

### Sequence

#### 4.1: Connection Pool Creation (Lines 49-64)

```rust
let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path))?
    .create_if_missing(true)
    .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
    .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
    .busy_timeout(Duration::from_secs(30))
    .statement_cache_capacity(100);

let pool = SqlitePool::connect_with(options).await?;
```

**Configuration:**
- **WAL Mode:** Write-Ahead Logging for concurrent access
- **30s Timeout:** Handles contention gracefully
- **100 Statements:** PreparedStatement cache for performance

#### 4.2: Migration Execution (Lines 86-140)

```rust
// CRITICAL: Verify all migration signatures before applying
let verifier = MigrationVerifier::new(&migrations_path)?;
verifier.verify_all()?; // Ed25519 signature check

// Run migrations via sqlx
let migrator = Migrator::new(migrations_path).await?;
migrator.run(&self.pool).await?;

// Verify post-migration version
self.verify_migration_version(&migrations_path).await?;
```

**Migration Count:** 80 complete migrations (0001-0080)
**Safety:** All migrations signed with Ed25519 keys

#### 4.3: Crash Recovery (Lines 424-426)

```rust
// Find adapters stuck in "loading" state from previous crash
db.recover_from_crash().await?;

// Cleanup:
// - Reset stuck adapters to Unloaded state
// - Fix negative activation counts
// - Remove orphaned state records
```

#### 4.4: Development Data Seeding (Lines 428-431)

```rust
if !cli.production_mode {
    db.seed_dev_data().await?; // Populates test tenants, policies
}
```

---

## Phase 5: Subsystem Initialization

**File:** `crates/adapteros-server/src/main.rs:438-754`
**Duration:** 2-5s
**Critical:** Partially (some subsystems optional)

### Subsystems (In Order)

#### 5.1: Telemetry System (Lines 514-554)

```rust
let bundles_path = config.paths.bundles_root.clone();
let telemetry = Arc::new(TelemetryWriter::new(
    &bundles_path,
    10000,            // max_events_per_bundle
    50 * 1024 * 1024, // max_bundle_size (50MB)
)?);
```

**Components:**
- **TelemetryWriter:** Receives events via crossbeam channel
- **Bundle Writer:** Rotates, compresses, signs NDJSON bundles
- **Ed25519 Signing:** All bundles cryptographically signed

#### 5.2: Federation Daemon (Lines 556-586)

```rust
let federation_keypair = adapteros_crypto::Keypair::generate();
let federation_manager = Arc::new(FederationManager::new(db.clone(), federation_keypair)?);
let federation_daemon = Arc::new(FederationDaemon::new(
    federation_manager,
    policy_watcher.clone(),
    telemetry.clone(),
    db.clone(),
    FederationDaemonConfig {
        interval_secs: 300,     // 5 minutes
        max_hosts_per_sweep: 10,
        enable_quarantine: true,
    },
));

let federation_shutdown_rx = shutdown_coordinator.subscribe_shutdown();
let federation_handle = federation_daemon.start(federation_shutdown_rx);
shutdown_coordinator.set_federation_handle(federation_handle);
```

**Interval:** Verifies federation signatures every 5 minutes

#### 5.3: UDS Metrics Exporter (Lines 589-656)

```rust
let socket_path = PathBuf::from("var/run/metrics.sock");
let mut uds_exporter = UdsMetricsExporter::new(socket_path)?;

// Register default metrics
uds_exporter.register_metric("inference_requests_total", MetricType::Counter)?;
uds_exporter.register_metric("memory_usage_bytes", MetricType::Gauge)?;
uds_exporter.register_metric("quarantine_active", MetricType::Gauge)?;

uds_exporter.bind().await?;
let uds_handle = tokio::spawn(async move {
    uds_exporter.serve(shutdown_rx).await
});
shutdown_coordinator.set_uds_metrics_handle(uds_handle);
```

**Protocol:** Unix Domain Socket + Prometheus text format

#### 5.4: Policy Hash Watcher (Lines 514-554)

- Monitors policy changes continuously (60s interval)
- Emits telemetry events on drift detection

#### 5.5: Memory Watchdog (Lines 677-680)

```rust
let uma_monitor = UmaPressureMonitor::new(15, None);
// Default: 15% headroom threshold
```

- Polls memory pressure every 30 seconds
- Triggers K-reduction on pressure events

#### 5.6: AppState Construction (Lines 682-704)

```rust
let app_state = AppState::new(
    db.clone(),
    jwt_secret.clone(),
    config.clone(),
    metrics_exporter.clone(),
    uma_monitor.clone(),
);
app_state.add_dataset_progress_channel(...);
app_state.add_plugin_registry(...);
```

#### 5.7: Background Tasks (Lines 740-843)

**Task 1: Status Writer** (5s interval)
```rust
spawn(async move {
    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        status_writer::write_status(...).await;
    }
});
```

**Task 2: TTL Cleanup** (5 min interval)
```rust
spawn(async move {
    loop {
        tokio::time::sleep(Duration::from_secs(300)).await;
        db.delete_expired_adapters().await;
        db.clean_expired_pins().await;
    }
});
```

**Task 3: Heartbeat Recovery** (5 min interval)
```rust
spawn(async move {
    loop {
        tokio::time::sleep(Duration::from_secs(300)).await;
        lifecycle.recover_stale_adapters().await;
    }
});
```

---

## Phase 6: Runtime - Server Start

**File:** `crates/adapteros-server/src/main.rs:845-956`
**Duration:** <1s (listening)
**Critical:** Yes

### Sequence

#### 6.1: Router Construction (Lines 846-851)

```rust
let router = routes::build(app_state.clone())
    .merge(assets::routes()); // UI routes

// API at /api, UI at /
```

#### 6.2: Server Binding (Production UDS or Dev TCP)

**Production Mode** (Lines 866-906):
```rust
let uds_path = cfg.server.uds_socket
    .ok_or_else(|| AosError::Config("UDS socket required in production".into()))?;

let listener = tokio::net::UnixListener::bind(&uds_path)?;
axum::serve(listener, router)
    .with_graceful_shutdown(shutdown_signal())
    .await?;
```

**Development Mode** (Lines 914-956):
```rust
let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
axum::serve(listener, router)
    .with_graceful_shutdown(shutdown_signal())
    .await?;
```

### Runtime Steady State

Once server is listening:

1. **Accept incoming requests** (batch/streaming inference, adapter management, etc.)
2. **Monitor memory pressure** (UMA monitor every 30s)
3. **Run background tasks** (status writer every 5s, TTL cleanup every 5min, heartbeat every 5min)
4. **Track activations** (adapter lifecycle updates via `record_router_decision()`)
5. **Emit telemetry** (events queue and rotate bundles)
6. **Verify federation** (every 5 minutes)

---

## Phase 7: Graceful Shutdown

**File:** `crates/adapteros-server/src/shutdown.rs:171-360`
**Duration:** 0-30s (depending on timeouts)
**Critical:** Yes

### Shutdown Initiation

#### 7.1: Signal Reception (main.rs:961-984)

```rust
async fn shutdown_signal() {
    let ctrl_c = async { tokio::signal::ctrl_c().await; };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(SignalKind::terminate())?
            .recv()
            .await;
    };
    select_2(ctrl_c, terminate).await;
    info!("Shutdown signal received");
}
```

**Signals Handled:** `SIGINT` (Ctrl+C), `SIGTERM` (systemd/container stop)

#### 7.2: Shutdown Coordinator Broadcast (shutdown.rs:175)

```rust
// Broadcast shutdown signal to all subscribers
self.shutdown_tx.broadcast(());
```

All subscribed components receive `()` through `broadcast::Receiver`

### Ordered Component Shutdown

#### 7.3: Shutdown Sequence (shutdown.rs:171-360)

The coordinator shuts down components in dependency order with per-component timeouts:

| Order | Component | Timeout | Reason |
|-------|-----------|---------|--------|
| **1** | Telemetry | 10s | **Critical**: Must flush data integrity |
| **2** | Federation | 15s | High: Cross-host signature verification |
| **3** | UDS Metrics | 5s | Normal: Metrics collection |
| **4** | Git Daemon | 10s | Medium: Git operations |
| **5** | Policy Watcher | 5s | Low: Policy monitoring |
| **6** | Alert Watcher | (abort) | Low: Alert system |
| **7** | Background Tasks | (abort) | Low: Status/TTL/heartbeat |

#### 7.4: Telemetry Shutdown (Lines 183-208)

```rust
pub fn shutdown(self) -> Result<()> {
    info!("Initiating telemetry writer shutdown");
    drop(self.sender); // Signal shutdown to writer thread

    let handle = Arc::try_unwrap(self._handle)?;
    match handle.join() {
        Ok(_) => {
            info!("Telemetry writer thread shutdown complete");
            Ok(())
        }
        Err(e) => {
            error!("Telemetry writer panicked: {:?}", e);
            Err(AosError::Internal("Thread panicked".into()))
        }
    }
}
```

**Flush Operation:**
1. Drop sender to signal shutdown
2. Writer thread processes remaining events
3. Flushes final bundle with signature
4. Waits for thread completion (up to 10s)

#### 7.5: Database Cleanup (Lines 329-330)

```rust
info!("Database connection pool cleanup handled automatically");
// SQLx pool Drop trait handles cleanup
```

**Automatic Pool Cleanup:**
- SQLite: WAL checkpoint, PRAGMA optimize
- PostgreSQL: Explicit `pool.close().await`

#### 7.6: MLX Runtime Cleanup (main.rs:909-913)

```rust
#[cfg(feature = "multi-backend")]
{
    adapteros_lora_mlx_ffi::mlx_runtime_shutdown();
}
```

### Error Handling During Shutdown

```rust
pub enum ShutdownError {
    Timeout,            // Component exceeded timeout
    ComponentError(String), // Component failed
    PartialFailure,     // Some components failed
    CriticalFailure,    // System integrity compromised
}
```

**Exit Codes:**
- `0`: Graceful shutdown, all components closed
- `1`: CriticalFailure or unrecovered error
- `2`: PartialFailure in critical components

---

## Phase 8: Component Cleanup

**File:** Various crates
**Duration:** Concurrent with Phase 7
**Critical:** Varies by component

### Resource Cleanup Patterns

#### 8.1: FFI Cleanup

**MLX Runtime** (`adapteros-lora-mlx-ffi/`):
```rust
pub fn mlx_runtime_shutdown() {
    // C++ -> Rust FFI cleanup
    // Deallocates MLX device objects
    // Releases GPU memory
}
```

**Memory Watchdog** (`adapteros-memory/src/page_migration_iokit.rs:879-887`):
```rust
impl Drop for PageMigrationTracker {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        unsafe {
            iokit_memory_pressure_disable();
            iokit_vm_cleanup(); // IOKit FFI
        }
    }
}
```

**HeapObserver** (`adapteros-memory/src/ffi_wrapper.rs:565-574`):
```rust
impl Drop for HeapObserverHandle {
    fn drop(&mut self) {
        // Clean up cached fragmentation metrics
        let mut cached = self.cached_fragmentation.lock();
        *cached = None;
    }
}
```

#### 8.2: Buffer Pool Cleanup

```rust
impl Drop for BufferPool {
    fn drop(&mut self) {
        // Clear all buffers
        let mut buffers = self.buffers.lock();
        buffers.clear();

        // Clear conversion cache
        let mut cache = self.conversion_cache.lock();
        cache.clear();
    }
}
```

#### 8.3: Socket Cleanup

**UDS Exporter** (Lines 81-104):
```rust
pub async fn shutdown(&mut self) -> Result<()> {
    if let Some(listener) = self.listener.take() {
        drop(listener); // Stop accepting
    }

    // Clean up socket file
    if self.socket_path.exists() {
        std::fs::remove_file(&self.socket_path)?;
    }
    Ok(())
}
```

---

## Phase 9: Process Exit

**File:** `crates/adapteros-cli/src/main.rs:1113-1143`
**Duration:** <100ms
**Critical:** No (informational)

### Final Steps

#### 9.1: Emit Exit Telemetry

```rust
match result {
    Ok(_) => {
        cli_telemetry::emit_cli_command(&command_name, tenant_id, true).await;
        Ok(())
    }
    Err(e) => {
        let error_code = cli_telemetry::extract_error_code(&e);
        let event_id = cli_telemetry::emit_cli_error(...).await;

        if let Some(code) = error_code {
            eprintln!("\n* {} -- see: aosctl explain {} (event: {})", code, code, event_id);
        }
        Err(e)
    }
}
```

#### 9.2: Exit with Code

```rust
std::process::exit(match result {
    Ok(()) => 0,
    Err(_) => 1,
});
```

---

## Lifecycle State Machines

### Adapter State Machine

```
Unloaded → Cold → Warm → Hot → Resident
    ↑                              ↓
    |______ (eviction) ___________|
```

| State | Memory | Active | Pinned | Priority Boost |
|-------|--------|--------|--------|----------------|
| Unloaded | No | No | No | 0.0 |
| Cold | Yes | No | No | 0.1 |
| Warm | Yes | Yes | No | 0.3 |
| Hot | Yes | Yes | No | 0.3 |
| Resident | Yes | Yes | Yes | 0.5 |

**Transitions Triggered By:**
- `promote_adapter()`: Manual promotion
- `record_router_decision()`: Router selection updates activation %
- `check_memory_pressure()`: Eviction on pressure

### Memory Pressure State Machine

```
Low (>25%)
    ↓
Medium (15-25%)  ← EvictLowPriority
    ↓
High (10-15%)    ← EvictCrossBackend
    ↓
Critical (<10%)  ← EmergencyEvict or ReduceK
```

**Headroom Policy:** Maintain ≥15% free memory at all times

**Eviction Order:**
1. Metal adapters (preserve ANE)
2. MLX adapters (research)
3. CoreML adapters (last resort)

---

## Configuration & Policies

### Startup Configuration (`Config::load()`)

```rust
pub struct Config {
    pub server: ServerConfig {
        port: u16,
        bind: String,
        production_mode: bool,
        uds_socket: Option<PathBuf>,
    },
    pub db: DbConfig {
        path: String,
    },
    pub security: SecurityConfig {
        require_pf_deny: bool,
        mtls_required: bool,
        jwt_secret: String,
    },
    pub paths: PathsConfig {
        artifacts_root: PathBuf,
        bundles_root: PathBuf,
        adapters_root: PathBuf,
    },
    pub metrics: MetricsConfig {
        enabled: bool,
        bearer_token: String,
    },
    // ... more fields ...
}
```

### Shutdown Configuration (`ShutdownConfig`)

```rust
pub struct ShutdownConfig {
    pub telemetry_timeout: Duration,        // 10 seconds
    pub federation_timeout: Duration,       // 15 seconds
    pub uds_metrics_timeout: Duration,      // 5 seconds
    pub git_daemon_timeout: Duration,       // 10 seconds
    pub policy_watcher_timeout: Duration,   // 5 seconds
    pub overall_timeout: Duration,          // 30 seconds
}
```

---

## Determinism Guarantees

### HKDF Seed Hierarchy

All randomness seeded from manifest hash via HKDF (key derivation):

```
Manifest Hash (BLAKE3)
    ↓
derive_seed(&manifest_hash, "executor") → Global Executor Seed
    ↓
    ├─ derive_seed(..., "router") → Router Gate Randomness
    ├─ derive_seed(..., "dropout") → Dropout Randomness
    ├─ derive_seed(..., "sampling") → Temperature Sampling
    └─ derive_seed(..., "text-generation") → Text Generation RNG
```

**Determinism Guarantee:** Same manifest hash → same execution trajectory

---

## Monitoring & Observability

### Key Metrics Emitted

| Metric | Type | Source |
|--------|------|--------|
| inference_requests_total | Counter | Router |
| memory_usage_bytes | Gauge | UMA Monitor |
| adapter_activations | Counter | LifecycleManager |
| gpu_verification_failures | Counter | Worker |
| federation_signatures_verified | Counter | FederationDaemon |
| telemetry_events_bundled | Counter | TelemetryWriter |

### Key Events Logged

| Event | Level | Trigger |
|-------|-------|---------|
| adapter.transition | info | State change (Unloaded→Cold, etc.) |
| memory.pressure | warn | Headroom < 15% |
| federation.verification_failed | error | Signature verification failure |
| shutdown.component_timeout | error | Component exceeded timeout |
| gpu.integrity_violation | critical | GPU fingerprint mismatch |

---

## Testing & Verification

### Integration Test Scenarios

1. **Graceful Shutdown (SIGTERM)**
   - Send SIGTERM to running server
   - Verify all components shut down within 30s
   - Verify telemetry final bundle is created
   - Verify exit code is 0

2. **Signal Handling (SIGINT)**
   - Send SIGINT (Ctrl+C) to running server
   - Verify same shutdown sequence as SIGTERM
   - Verify cleanup completes

3. **Timeout Handling**
   - Simulate slow component shutdown
   - Verify timeout mechanism aborts component
   - Verify system continues shutting down other components

4. **Crash Recovery**
   - Kill server during adapter loading
   - Restart server
   - Verify orphaned adapters are reset
   - Verify database consistency

5. **Memory Pressure**
   - Fill memory to trigger pressure
   - Verify eviction order (Metal → MLX → CoreML)
   - Verify pinned adapters are not evicted

---

## Best Practices

### For Developers

1. **Always emit telemetry** on state transitions
2. **Use Arc<Mutex/RwLock>>** for shared state
3. **Subscribe to shutdown** signal for graceful cleanup
4. **Implement Drop** for resource cleanup
5. **Avoid blocking** in async contexts
6. **Use timeouts** on all I/O operations

### For Operators

1. **Monitor graceful shutdown** duration (should be <30s)
2. **Check telemetry bundles** for completeness
3. **Verify adapter state** after crash recovery
4. **Monitor memory headroom** (should stay ≥15%)
5. **Test SIGTERM handling** regularly

---

## Appendix: File Reference

| Phase | Primary Files | Line Ranges |
|-------|---|---|
| Phase 1 (CLI) | `adapteros-cli/src/main.rs`, `serve.rs` | 1092-1145, 75-407 |
| Phase 2 (Init) | `adapteros-server/src/main.rs` | 129-375 |
| Phase 3 (Preflight) | `adapteros-server/src/security.rs`, `main.rs` | 1-163, 282-373 |
| Phase 4 (Database) | `adapteros-db/src/lib.rs` | 40-140 |
| Phase 5 (Subsystems) | `adapteros-server/src/main.rs` | 438-843 |
| Phase 6 (Runtime) | `adapteros-server/src/main.rs` | 845-956 |
| Phase 7 (Shutdown) | `adapteros-server/src/shutdown.rs`, `main.rs` | 171-360, 961-984 |
| Phase 8 (Cleanup) | Various crates | Drop implementations |
| Phase 9 (Exit) | `adapteros-cli/src/main.rs` | 1113-1143 |

---

**Document Status:** Complete
**Last Review:** 2025-11-22
**Maintainer:** @rogu3bear
