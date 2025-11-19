# Startup Failures

Server won't start, missing configuration, and initialization errors.

## Symptoms

- Server exits immediately after startup
- "Configuration not found" errors
- "Failed to initialize" messages
- Panic/crash during startup
- Server starts but immediately stops

## Common Failure Modes

### 1. Missing Configuration File

**Symptoms:**
```
[ERROR] Failed to load configuration: No such file or directory (os error 2)
[ERROR] Configuration not found: configs/cp.toml
```

**Root Cause:**
- Configuration file not present
- Wrong path specified
- Working directory incorrect

**Fix:**
```bash
# Check if config exists
ls -la configs/cp.toml

# If missing, create from template
cp configs/cp-example.toml configs/cp.toml

# Edit configuration
vim configs/cp.toml

# Verify syntax
cargo run --bin aos-cp -- --config configs/cp.toml --help
```

**Prevention:**
- Include config in version control
- Document required configuration
- Provide example configs

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-server/src/main.rs:252` - Config loading
- `/Users/star/Dev/aos/configs/` - Configuration files

### 2. Production Mode Without Valid Manifest

**Symptoms:**
```
[ERROR] Production mode (require_pf_deny=true) requires valid manifest for executor seeding
[ERROR] Manifest path: models/qwen2.5-7b-mlx/manifest.json
```

**Root Cause:**
- `require_pf_deny = true` in config (production mode)
- Manifest file missing or invalid
- Manifest validation failed

**Fix:**
```bash
# Check if manifest exists
ls -la models/qwen2.5-7b-mlx/manifest.json

# Validate manifest
python -c "import json; json.load(open('models/qwen2.5-7b-mlx/manifest.json'))"

# Option A: Provide valid manifest
# Download or create proper manifest

# Option B: Disable production mode (development only)
# Edit configs/cp.toml
[security]
require_pf_deny = false

# Restart
cargo run --bin aos-cp -- --config configs/cp.toml
```

**Prevention:**
- Always use valid manifests in production
- Include manifest with model distribution
- Validate manifest during build

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-server/src/main.rs:320-336` - Production mode check
- `/Users/star/Dev/aos/crates/adapteros-manifest/src/lib.rs` - Manifest validation

### 3. Environment Drift Blocking Startup

**Symptoms:**
```
[ERROR] Critical environment drift detected!
[ERROR] cpu_model: Apple M1 Pro -> Apple M2
[ERROR] Refusing to start due to critical environment drift
```

**Root Cause:**
- Hardware changed (CPU, GPU)
- OS version changed significantly
- Drift policy too strict
- Baseline fingerprint from different machine

**Fix:**
```bash
# View drift details
aosctl drift-check

# Option A: Update baseline (if change is intentional)
aosctl drift-check --save-baseline

# Option B: Relax drift policy
# Edit configs/cp.toml
[policies.drift]
block_on_cpu_change = false

# Option C: Start with --skip-pf-check (development only)
cargo run --bin aos-cp -- --skip-pf-check
```

**Prevention:**
- Regenerate baseline after hardware changes
- Document baseline requirements
- Set appropriate drift policies

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-server/src/main.rs:372-451` - Drift detection
- `/Users/star/Dev/aos/crates/adapteros-verify/src/lib.rs` - Fingerprinting

### 4. Database Migration Failure on Startup

**Symptoms:**
```
[ERROR] Migration failed: error applying migration 0015
[ERROR] Schema version mismatch: DB version 14 != expected 20
```

**Root Cause:**
- See [Database Failures](./database-failures.md)

**Quick Fix:**
```bash
# Development: Reset database
aosctl db reset
aosctl db migrate

# Production: Fix migrations
# See database-failures.md for detailed procedures
```

### 5. PID Lock Conflict

**Symptoms:**
```
[ERROR] Another aos-cp process is running (PID: 12345)
[ERROR] Stop it first or use --no-single-writer
```

**Root Cause:**
- See [Port Binding Conflicts](./port-binding-conflicts.md)

**Quick Fix:**
```bash
# Remove stale lock
rm -f var/aos-cp.pid /var/run/aos/cp.pid

# Restart
./scripts/start_server.sh
```

### 6. Crash Recovery Failure

**Symptoms:**
```
[ERROR] Failed to recover from crash
[ERROR] Orphaned adapter cleanup failed
```

**Root Cause:**
- Database corruption during crash
- Orphaned resources
- Recovery logic bug

**Fix:**
```bash
# Step 1: Check database integrity
sqlite3 var/aos-cp.sqlite3 "PRAGMA integrity_check;"

# Step 2: Manual cleanup
aosctl maintenance sweep-orphaned

# Step 3: Force recovery
sqlite3 var/aos-cp.sqlite3 <<EOF
UPDATE adapters
SET current_state = 'cold'
WHERE current_state IN ('loading', 'transitioning');
EOF

# Step 4: Restart
./scripts/start_server.sh
```

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-server/src/main.rs:502-504` - Crash recovery
- `/Users/star/Dev/aos/crates/adapteros-db/src/lib.rs` - Recovery logic

### 7. Insufficient Permissions

**Symptoms:**
```
[ERROR] Permission denied: var/aos-cp.sqlite3
[ERROR] Failed to create directory: /var/run/aos
```

**Root Cause:**
- Database file permissions
- Directory permissions
- Running as wrong user

**Fix:**
```bash
# Check permissions
ls -la var/aos-cp.sqlite3
ls -la /var/run/aos

# Fix database permissions
chmod 644 var/aos-cp.sqlite3

# Fix directory permissions
mkdir -p var/run
chmod 755 var/run

# Fix system directory
sudo mkdir -p /var/run/aos
sudo chown $(whoami):staff /var/run/aos
```

### 8. Rust/Cargo Build Issues

**Symptoms:**
```
error: failed to compile aos-cp
error: could not compile `adapteros-server`
```

**Root Cause:**
- Dependency issues
- Rust version mismatch
- Missing system libraries

**Fix:**
```bash
# Update Rust
rustup update stable

# Clean build
cargo clean
cargo build --release

# Check for missing dependencies
cargo tree --duplicates
```

## Startup Checklist

Before investigating startup failure:

- [ ] Configuration file exists: `configs/cp.toml`
- [ ] Database exists: `var/aos-cp.sqlite3`
- [ ] Migrations up to date: `aosctl db migrate`
- [ ] No PID lock file: `var/aos-cp.pid`
- [ ] Port 8080 available: `lsof -i :8080`
- [ ] Manifest valid (if production): `models/*/manifest.json`
- [ ] Permissions correct: `ls -la var/`
- [ ] Disk space available: `df -h .`

## Diagnostic Script

```bash
#!/bin/bash
# startup-diagnostic.sh

echo "=== AdapterOS Startup Diagnostic ==="
echo ""

echo "1. Configuration:"
test -f configs/cp.toml && echo "✓ Config exists" || echo "✗ Config missing"
echo ""

echo "2. Database:"
test -f var/aos-cp.sqlite3 && echo "✓ Database exists" || echo "✗ Database missing"
if [ -f var/aos-cp.sqlite3 ]; then
  sqlite3 var/aos-cp.sqlite3 "SELECT COUNT(*) FROM _sqlx_migrations;" 2>/dev/null && echo "✓ Migrations applied" || echo "✗ Migrations not applied"
fi
echo ""

echo "3. PID Lock:"
test -f var/aos-cp.pid && echo "⚠ PID lock exists" || echo "✓ No PID lock"
if [ -f var/aos-cp.pid ]; then
  PID=$(cat var/aos-cp.pid)
  ps -p $PID >/dev/null 2>&1 && echo "  Process $PID running" || echo "  Process $PID not found (stale)"
fi
echo ""

echo "4. Port 8080:"
lsof -i :8080 >/dev/null 2>&1 && echo "⚠ Port in use" || echo "✓ Port available"
echo ""

echo "5. Disk Space:"
df -h . | tail -1
echo ""

echo "6. Permissions:"
ls -la var/aos-cp.sqlite3 2>/dev/null
echo ""

echo "=== Recommendation ==="
if [ ! -f configs/cp.toml ]; then
  echo "Create configuration: cp configs/cp-example.toml configs/cp.toml"
elif [ ! -f var/aos-cp.sqlite3 ]; then
  echo "Initialize database: aosctl db migrate"
elif [ -f var/aos-cp.pid ]; then
  echo "Remove stale lock: rm var/aos-cp.pid"
elif lsof -i :8080 >/dev/null 2>&1; then
  echo "Stop process on port 8080 or change port in config"
else
  echo "Ready to start: ./scripts/start_server.sh"
fi
```

## Related Runbooks

- [Startup Procedures](./startup-procedures.md)
- [Database Failures](./database-failures.md)
- [Port Binding Conflicts](./port-binding-conflicts.md)
- [Log Analysis](./log-analysis.md)

## Escalation Criteria

Escalate if:
- Server crashes repeatedly
- Unknown startup errors
- Configuration issues persist
- See [Escalation Guide](./escalation.md)
