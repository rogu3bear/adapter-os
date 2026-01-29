# AdapterOS Server Boot Process

## Overview

The AdapterOS control plane server (`adapteros-server`) uses a 12-phase boot sequence with strict ordering, phase tracking, and comprehensive invariant validation. The boot process is implemented in `/Users/star/Dev/adapter-os/crates/adapteros-server/src/main.rs` with modular phases in `/Users/star/Dev/adapter-os/crates/adapteros-server/src/boot/`.

## Boot Phases (12 Total)

### Phase 1: Configuration (`boot/config.rs`)
- Parse CLI arguments
- Load configuration from `configs/cp.toml`
- Resolve port with precedence: env > CLI > config
- Initialize logging and tracing
- Set boot timeout (default from config)

### Phase 2: Security Initialization (`boot/security.rs`)
- Acquire PID file lock (if single-writer mode)
- Load or generate Ed25519 worker signing keypair for CP->Worker auth
- Log effective configuration (with redacted secrets)
- Validate JWT secret (reject placeholder values in production)
- Block dev bypass flags in release builds (`AOS_DEV_NO_AUTH`, `AOS_DEV_SIGNATURE_BYPASS`)

### Phase 3: Deterministic Executor (`boot/executor.rs`)
- Resolve manifest path (env > CLI > config > dev fallback)
- Load and validate manifest (ManifestV3)
- Derive deterministic seed from manifest hash using HKDF-SHA256
- Initialize global executor with derived seed
- Initialize MLX runtime (feature-gated with `multi-backend`)
- Create ShutdownCoordinator and BackgroundTaskTracker

### Phase 4: Security Preflight Checks (`boot/security.rs`)
- Validate PF (packet filter) rules on macOS/Linux
- Verify environment fingerprint and detect drift
- Block startup on critical drift in production mode
- Create baseline fingerprint on first run

### Phase 4b: Boot Invariants Validation (`boot/invariants.rs`)
- 28+ invariant checks across categories:
  - Security (SEC-001 to SEC-015): dev bypass, dual-write, cookie security, JWT
  - Authentication (AUTH-001 to AUTH-003): JWT key, HMAC secret, session store
  - Authorization (AUTHZ-001, AUTHZ-002): RBAC tables, admin role
  - Cryptographic (CRYPTO-001 to CRYPTO-003): worker keypair, entropy, algorithm
  - Configuration (CFG-001, CFG-002): var paths, session TTL hierarchy
  - Database (DAT-001 to DAT-007): triggers, foreign keys, migrations, audit
  - Memory (MEM-003): headroom configuration
  - Lifecycle (LIF-001 to LIF-004): boot ordering, executor init, pool drain
  - Federation (FED-001, FED-002): quorum keys, peer certificates
  - Adapters (ADAPT-001, ADAPT-002): bundle signatures, manifest hashes
  - Policy (POL-001, POL-002): default pack, enforcement mode
- Production mode: fatal violations block boot (fail closed)
- Development mode: violations logged as warnings (fail open)

### Phase 5: Database Connection (`boot/database.rs`)
- Determine storage backend (SQL, Dual, KV-Primary, KV-Only)
- Create database connection with DbFactory
- Validate atomic dual-write configuration
- Attach database to boot state manager
- Initialize effective config and detect configuration drift
- Freeze configuration guards (prohibit env var access after boot)
- Initialize global tick ledger for inference tracking
- Resolve runtime mode (env > db > config > default)

### Phase 6: Database Migrations (`boot/migrations.rs`)
- Run schema migrations with Ed25519 signature verification
- Crash recovery from orphaned adapters and stale state
- Seed development data
- Seed base models from cache
- Ensure system tenant and core policies exist
- Handle `--migrate-only` flag for early exit

### Phase 6b: Post-DB Invariants Validation
- Foreign key constraints enabled (DAT-002)
- Migration table exists with entries (DAT-006)
- Archive state machine triggers exist (DAT-001)
- Audit chain table initialized (DAT-007)

### Phase 7: Startup Recovery (`boot/startup_recovery.rs`)
- Recover orphaned training jobs (running >5min without progress)
- Transition orphaned jobs to "interrupted" state for retry
- Best-effort: errors logged but don't block boot

### Phase 9a: API Configuration (`boot/api_config.rs`)
- Build API configuration
- Enable dev bypass from config (debug builds only)
- Set up SIGHUP handler for config hot-reload

### Phase 9b: Federation (`boot/federation.rs`)
- Initialize federation daemon
- Start policy watcher

### Phase 9c: Metrics (`boot/metrics.rs`)
- Initialize metrics exporter
- Start UMA pressure monitor

### Phase 10a: Application State (`boot/app_state.rs`)
- Initialize WorkerHealthMonitor (30s polling)
- Create MetricsCollector and MetricsRegistry
- Wire TrainingService to DB and dataset storage
- Build AppState with all dependencies
- Resolve manifest hash (env fallback hierarchy)
- Initialize plugin registry
- Start self-hosting agent (if enabled)
- Initialize adapter registry (SQLite registry.db)
- Initialize Git subsystem (if enabled)
- Initialize diagnostics service (if enabled)

### Phase 10b: Background Tasks (`boot/background_tasks.rs`)
- **Dev mode optimization**: Only essential tasks (status writer, WAL checkpoint, TTL cleanup)
- **Production mode tasks (10+ tasks)**:
  1. Status writer (5s interval)
  2. KV metrics alert monitor (5s interval)
  3. Log cleanup (24h interval)
  4. TTL/expiration cleanup (5m interval with circuit breaker)
  5. WAL checkpoint (5m interval)
  6. Upload session cleanup (1h prod, 12h dev)
  7. Security cleanup (1h interval)
  8. Telemetry bundle GC (6h interval)
  9. Orphaned training job cleanup (1h interval, 24h threshold)
  10. Rate limiter eviction (60s interval)
  11. Audit chain verification (1h interval)
  12. Diagnostics writer
  13. Stale task monitor (60s interval, 5min threshold)

### Phase 11: Adapter Loading (`boot/finalization.rs`)
- Transition through boot states: StartingBackend → LoadingBaseModels → LoadingAdapters → WorkerDiscovery

### Phase 12: Finalization (`boot/finalization.rs`)
- Build Axum router with API routes
- Merge spoke routes (audit, health handlers)
- Merge UI routes
- Add compression layer
- Resolve server binding configuration
- Write boot report to `AOS_VAR_DIR/run/boot_report.json`

## Graceful Shutdown (`shutdown.rs`)

### Shutdown Coordinator
- Broadcast shutdown signal to all components
- Component-specific timeouts:
  - Telemetry: 10s (critical for data integrity)
  - Federation: 15s
  - UDS metrics: 5s
  - Git daemon: 10s
  - Policy watcher: 5s
  - Background tasks: 5s
  - Overall: 30s

### Shutdown Order
1. Telemetry system (flush buffers, close connections)
2. Federation daemon (clean verification completion)
3. UDS metrics exporter (close socket connections)
4. Git daemon (stop polling and file watching)
5. Policy watcher (stop hash validation sweeps)
6. Alert watcher (stop job monitoring)
7. Background tasks (wait for graceful exit, then abort)
8. Database connections (automatic pool cleanup)

### Drain Process
- Transition to draining state on SIGINT/SIGTERM
- Wait for in-flight requests to complete (configurable timeout)
- Track peak and average in-flight requests for analysis
- Log detailed recovery instructions on timeout

## Worker Registration During Boot

Workers register with the control plane via:
1. **WorkerHealthMonitor**: Polls workers every 30s with health checks
2. **Worker Discovery Phase (Phase 11)**: Boot state transitions to worker discovery
3. **CP->Worker Authentication**: Uses Ed25519 signed tokens (keypair from Phase 2)
4. **Manifest-bound routing**: Workers must match manifest hash for routing

## Debugging Boot Issues

### Key Environment Variables
- `AOS_DEV_NO_AUTH=1`: Disable authentication (debug builds only)
- `AOS_DEBUG_DETERMINISM=1`: Log seed inputs and router details
- `AOS_ALLOW_INSECURE_DEV_FLAGS=1`: Override security gates in release builds (dangerous)
- `AOS_BOOT_TIMEOUT_SECS`: Override boot timeout
- `AOS_VAR_DIR`: Override var directory location

### Boot State Tracking
- `BootStateManager` tracks current phase and failures
- Phases: Initializing → DbConnecting → Migrating → Seeding → LoadingPolicies → StartingBackend → LoadingBaseModels → LoadingAdapters → WorkerDiscovery → Ready → Draining → Stopping
- Degraded state for partial failures
- Boot report written to `var/run/boot_report.json`

### Common Boot Failures
1. **Config lock poisoned**: Concurrent access error, restart server
2. **Migration signature invalid**: Check Ed25519 signatures in migrations
3. **Invariant violation**: See logs for remediation steps
4. **Manifest not found**: Set `--manifest-path` or `AOS_MANIFEST_PATH`
5. **PF rules missing**: Run PF preflight or use `--skip-pf-check` (dev only)
6. **Environment drift**: Run `aosctl drift-check` for details

### Useful Commands
```bash
# Run with auth disabled (dev only)
AOS_DEV_NO_AUTH=1 cargo run -p adapteros-server -- --config configs/cp.toml

# Run migrations only
./aosctl db migrate

# Check boot report
cat var/run/boot_report.json

# System health diagnostics
./aosctl doctor

# Pre-flight readiness check
./aosctl preflight
```

## Key Files

| File | Purpose |
|------|---------|
| `main.rs` | Entry point, orchestrates all phases |
| `boot/mod.rs` | Module exports and documentation |
| `boot/config.rs` | Phase 1: Configuration loading |
| `boot/security.rs` | Phase 2 & 4: Security init and preflight |
| `boot/executor.rs` | Phase 3: Deterministic executor |
| `boot/invariants.rs` | Phase 4b & 6b: Invariant validation (28+ checks) |
| `boot/database.rs` | Phase 5: Database connection |
| `boot/migrations.rs` | Phase 6: Schema migrations |
| `boot/startup_recovery.rs` | Phase 7: Orphaned resource recovery |
| `boot/app_state.rs` | Phase 10a: AppState construction |
| `boot/background_tasks.rs` | Phase 10b: Background task spawning |
| `boot/finalization.rs` | Phase 11-12: Router and boot report |
| `shutdown.rs` | Graceful shutdown coordination |
