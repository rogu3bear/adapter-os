# AdapterOS Boot Troubleshooting Guide

This document provides a decision-tree approach to diagnosing and resolving common boot failures in AdapterOS.

---

## Quick Diagnosis Flowchart

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        Boot Failure Diagnosis                                │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Does the server start at all?                                              │
│     │                                                                       │
│     ├─ No → Check Section 1: Startup Failures                              │
│     │                                                                       │
│     └─ Yes → Does /healthz return 200?                                     │
│                │                                                            │
│                ├─ No → Check Section 2: Early Boot Failures                │
│                │                                                            │
│                └─ Yes → Does /readyz return 200?                           │
│                          │                                                  │
│                          ├─ No → Check Section 3: Boot Phase Failures      │
│                          │                                                  │
│                          └─ Yes → Check Section 4: Runtime Issues          │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Section 1: Startup Failures (Server Doesn't Start)

### 1.1 Port Already in Use

**Symptom:**
```
ERROR Port 8080 already in use. Kill existing process: lsof -ti:8080 | xargs kill
```

**Exit code:** 10

**Cause:** Another process is using the configured port.

**Solutions:**
```bash
# Option 1: Kill existing process
lsof -ti:8080 | xargs kill

# Option 2: Use a different port
AOS_SERVER_PORT=8081 ./start

# Option 3: Check what's using the port
lsof -i:8080
```

---

### 1.2 Config File Not Found

**Symptom:**
```
ERROR Failed to load configuration from configs/aos.toml: No such file or directory
```

**Cause:** Config file missing or wrong path.

**Solutions:**
```bash
# Create default config
cp configs/aos.example.toml configs/aos.toml

# Or specify custom path
./target/release/adapteros-server --config /path/to/your/config.toml
```

---

### 1.3 Base Model Path Invalid

**Symptom:**
```
FATAL: Base model path missing or invalid: Path does not exist
```

**Exit code:** 1

**Cause:** Model directory not found or not configured.

**Solutions:**
```bash
# Option 1: Set model cache directory
export AOS_MODEL_CACHE_DIR="$HOME/.cache/aos-models"

# Option 2: Set specific model ID
export AOS_BASE_MODEL_ID="Qwen2.5-7B-Instruct-4bit"

# Option 3: Download model
./aosctl models download qwen2.5-7b-4bit
```

---

### 1.4 Permission Denied

**Symptom:**
```
ERROR Failed to bind UDS socket: Permission denied
ERROR Failed to open database: unable to open database file
```

**Cause:** Insufficient permissions for directories or files.

**Solutions:**
```bash
# Fix var directory permissions
chmod -R 755 var/
mkdir -p var/run var/logs var/keys

# Check database file permissions
ls -la var/aos-cp.sqlite3

# If running as different user
chown -R $(whoami) var/
```

---

### 1.5 Missing Dependencies

**Symptom:**
```
dyld: Library not loaded: @rpath/libmlx.dylib
```

**Cause:** MLX or other native libraries not installed.

**Solutions:**
```bash
# macOS: Install via Homebrew
brew install mlx

# Or disable MLX backend
export AOS_MODEL_BACKEND=coreml
```

---

## Section 2: Early Boot Failures (/healthz fails)

### 2.1 Config Lock Poisoned

**Symptom:**
```
FATAL: Config lock poisoned: ...
```

**Cause:** Internal panic during config access. Rare.

**Solutions:**
1. Check for config file corruption
2. Reset to known-good config
3. Check system memory (OOM can cause this)

---

### 2.2 Logging Initialization Failed

**Symptom:**
```
Failed to initialize logging: ...
```

**Cause:** Invalid logging configuration or permissions issue.

**Solutions:**
```bash
# Check log directory exists and is writable
mkdir -p var/logs
chmod 755 var/logs

# Try simpler log config
export AOS_LOG_LEVEL=info
export AOS_LOG_FORMAT=text
```

---

### 2.3 CORS Validation Failed

**Symptom:**
```
FATAL: CORS config validation failed: Invalid origin pattern
```

**Cause:** Malformed CORS configuration.

**Solutions:**
1. Check `security.cors_origins` in config
2. Ensure valid URL patterns
3. In dev mode: `AOS_DEV_NO_AUTH=1` bypasses some checks

---

## Section 3: Boot Phase Failures (/healthz OK, /readyz fails)

### 3.1 Worker Signing Keypair Failure (Strict Mode)

**Symptom:**
```
STRICT MODE: Failed to load worker signing keypair
```

**Boot state:** `Booting`

**Cause:** Missing or corrupted keypair in strict mode.

**Solutions:**
```bash
# Regenerate keypair
rm var/keys/worker_signing.key
./target/release/adapteros-server --config configs/aos.toml

# Or disable strict mode (dev only)
./target/release/adapteros-server --config configs/aos.toml  # without --strict
```

---

### 3.2 Manifest Required in Production

**Symptom:**
```
Production mode (require_pf_deny=true) requires valid manifest for executor seeding
```

**Boot state:** `Booting`

**Cause:** No manifest file in production mode.

**Solutions:**
```bash
# Option 1: Set manifest path
export AOS_MANIFEST_PATH=manifests/qwen7b-4bit-mlx.yaml

# Option 2: Disable production mode (dev only)
# In configs/aos.toml: production_mode = false
```

---

### 3.3 PF Security Check Failed

**Symptom:**
```
PF firewall check failed: Egress not blocked
```

**Boot state:** `SecurityPreflight`

**Cause:** Packet filter not configured to block egress.

**Solutions:**
```bash
# Option 1: Configure PF (production)
sudo pfctl -e
# Add rules per docs/SECURITY.md

# Option 2: Skip check (dev only)
./target/release/adapteros-server --skip-pf-check

# Option 3: Disable requirement (dev config)
# In configs/aos.toml: require_pf_deny = false
```

---

### 3.4 Critical Environment Drift

**Symptom:**
```
Refusing to start due to critical environment drift
```

**Boot state:** `SecurityPreflight`

**Cause:** System configuration changed significantly since baseline.

**Solutions:**
```bash
# View drift details
./aosctl drift-check

# Option 1: Update baseline (if changes are expected)
rm var/baseline_fingerprint.json
./target/release/adapteros-server  # Creates new baseline

# Option 2: Skip drift check (dev only)
./target/release/adapteros-server --skip-drift-check
```

---

### 3.5 Database Connection Failed

**Symptom:**
```
ERROR Database connection failed: unable to open database file
```

**Boot state:** `InitDb` → `ConnectingDb`

**Cause:** Database file corrupted, missing, or locked.

**Solutions:**
```bash
# Check database exists
ls -la var/aos-cp.sqlite3

# Check for lock files
ls -la var/aos-cp.sqlite3*

# Verify integrity
sqlite3 var/aos-cp.sqlite3 "PRAGMA integrity_check;"

# If corrupted, restore from backup or recreate
rm var/aos-cp.sqlite3*  # WARNING: Data loss
# Server will recreate on next start
```

---

### 3.6 Migration Signature Invalid

**Symptom:**
```
ERROR [boot] code=E_MIG_INVALID: Database migrations failed
ERROR Migration signature verification failed for 0142_...
```

**Boot state:** `Migrating`

**Cause:** Migration file modified after signing, or signatures.json outdated.

**Solutions:**
```bash
# Option 1: Re-sign migrations (if you modified them)
./scripts/sign_migrations.sh

# Option 2: Pull latest migrations
git checkout -- migrations/
git checkout -- migrations/signatures.json

# Option 3: Check for local modifications
git status migrations/
```

---

### 3.7 Boot Timeout

**Symptom:**
```
FATAL: Boot timeout after 300 seconds. Boot was stuck in state: DiscoveringWorkers
```

**Exit code:** 10

**Cause:** A boot phase took too long (common: worker discovery, model loading).

**Solutions:**
```bash
# Increase timeout
# In configs/aos.toml: boot_timeout_secs = 600

# Check which phase is slow
curl -s http://localhost:8080/api/readyz | jq

# If stuck on DiscoveringWorkers:
# - Check worker process: ps aux | grep aos_worker
# - Check worker logs: cat var/logs/worker.log
# - Verify socket path: ls -la var/run/

# If stuck on LoadingBaseModels:
# - Model download may be slow
# - Pre-download: ./aosctl models download ...
```

---

### 3.8 JWT Secret Too Short (Production)

**Symptom:**
```
FATAL: JWT secret is too short for production mode (current: 16)
```

**Boot state:** `LoadingPolicies`

**Cause:** JWT secret less than 32 characters in production mode.

**Solutions:**
```bash
# Generate secure secret
openssl rand -hex 32

# Set in environment
export AOS_JWT_SECRET="your-64-char-hex-secret-here"

# Or in config file
# jwt_secret = "your-64-char-hex-secret-here"
```

---

## Section 4: Runtime Issues (Both endpoints OK, but problems)

### 4.1 Worker Not Responding

**Symptom:**
- `/readyz` returns 200 but inference fails
- Logs show "Worker socket not found"

**Solutions:**
```bash
# Check worker status
./scripts/service-manager.sh status

# Restart worker
./scripts/service-manager.sh stop worker
./scripts/service-manager.sh start worker

# Check worker logs
tail -f var/logs/worker.log
```

---

### 4.2 High Memory Usage

**Symptom:**
- Server becomes slow
- macOS memory pressure warnings

**Solutions:**
```bash
# Check memory usage
./aosctl status

# Force adapter eviction
./aosctl adapters evict --tier cold

# Reduce model memory
export AOS_MODEL_MAX_MEMORY_GB=4
```

---

### 4.3 Slow Inference

**Symptom:**
- Inference latency > 10s
- GPU not being utilized

**Solutions:**
```bash
# Check backend
./aosctl status | grep backend

# Force GPU backend
export AOS_MODEL_BACKEND=mlx

# Check GPU availability
system_profiler SPDisplaysDataType | grep "Metal"
```

---

## Log File Locations

| Log | Location | Contents |
|-----|----------|----------|
| Backend | `var/logs/backend.log` | Control plane logs |
| Worker | `var/logs/worker.log` | Inference worker logs |
| UI | `var/logs/ui.log` | UI dev server logs (dev only) |
| Status | `var/run/status.json` | Current system status |
| Boot Report | `var/run/boot_report.json` | Boot configuration snapshot |

---

## Diagnostic Commands

```bash
# Full system status
./aosctl status

# Health check
curl -s http://localhost:8080/api/healthz | jq

# Ready check (includes boot state)
curl -s http://localhost:8080/api/readyz | jq

# Boot report
cat var/run/boot_report.json | jq

# Database integrity
sqlite3 var/aos-cp.sqlite3 "PRAGMA integrity_check;"

# Recent logs
tail -100 var/logs/backend.log

# Process status
ps aux | grep adapteros
ps aux | grep aos_worker

# Port usage
lsof -i:8080
lsof -i:3200

# Environment drift
./aosctl drift-check
```

---

## Environment Variables Reference

### Boot-Related Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `AOS_SERVER_PORT` | 8080 | HTTP port |
| `AOS_DATABASE_URL` | `sqlite://var/aos-cp.sqlite3` | Database path |
| `AOS_MANIFEST_PATH` | (none) | Manifest file for determinism |
| `AOS_MANIFEST_HASH` | (computed) | Pre-computed manifest hash |
| `AOS_JWT_SECRET` | (generated) | JWT signing secret |
| `AOS_LOG_LEVEL` | info | Log verbosity |
| `AOS_LOG_FORMAT` | text | `text` or `json` |

### Dev-Mode Bypass Variables

| Variable | Effect |
|----------|--------|
| `AOS_DEV_NO_AUTH=1` | Disable JWT authentication |
| `AOS_DEV_JWT_SECRET` | Use custom JWT secret |
| `AOS_SKIP_PREFLIGHT=1` | Skip disk/memory checks in shell |

---

## Common Error Codes

| Exit Code | Meaning | Common Cause |
|-----------|---------|--------------|
| 0 | Success | Normal exit |
| 1 | General error | Various runtime errors |
| 10 | Config/Boot error | Port in use, boot timeout, bind failure |

---

## Getting Help

1. **Check logs first:** `tail -100 var/logs/backend.log`
2. **Run diagnostics:** `./aosctl diag`
3. **Check documentation:** `docs/BOOT_WALKTHROUGH.md`
4. **Report issues:** https://github.com/mlnavigator/adapteros/issues

---

## Appendix: Annotated Successful Boot Log

```
# Phase 1: Config loaded
INFO  config_path="configs/aos.toml" Configuration loaded

# Phase 2: Logging initialized
INFO  Panic capture hook installed

# Phase 3: Boot state manager created
INFO  timeout_secs=300 Starting boot sequence with timeout

# Phase 4: Worker keypair loaded
INFO  kid="abc123" elapsed_ms=45 Worker signing keypair loaded

# Phase 5: Executor seeded
INFO  seed_hash="def456..." manifest_based=true Derived deterministic executor seed
INFO  Deterministic executor initialized with manifest-derived seed

# Phase 6: Security preflight
INFO  Running security preflight checks
INFO  Verifying environment fingerprint
INFO  No environment drift detected

# Phase 7: Database connected
INFO  db_path="var/aos-cp.sqlite3" storage_mode="dual" Connecting to database
INFO  Atomic dual-write strict mode enabled

# Phase 8: Migrations
INFO  Running database migrations...
INFO  Running crash recovery checks...

# Phase 9: Policies and backend
INFO  Runtime mode resolved: Development

# Phase 10: Background tasks
INFO  SIGHUP handler registered
INFO  Worker health monitor started
INFO  Status writer started (5s interval)

# Phase 11: Router and report
INFO  path="var/run/boot_report.json" Boot report written

# Phase 12: Server binding
INFO  addr="127.0.0.1:8080" Starting control plane
INFO  url="http://127.0.0.1:8080/" UI available
INFO  url="http://127.0.0.1:8080/api/" API available
WARN  Development mode: TCP binding enabled
INFO  Boot sequence completed successfully
```
