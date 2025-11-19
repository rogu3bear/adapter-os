# Startup Procedures

Initial setup and first-run procedures for AdapterOS.

## Prerequisites

### System Requirements
- macOS 13+ (Metal GPU support)
- 16GB+ RAM (32GB+ recommended)
- Rust toolchain (cargo, rustc)
- pnpm (for UI)
- SQLite 3.35+

### Environment Variables
```bash
# Optional configuration
export DATABASE_URL="./var/aos-cp.sqlite3"
export AOS_MANIFEST_PATH="models/qwen2.5-7b-mlx/manifest.json"
export RUST_LOG="info,aos_cp=debug"
```

## First-Time Setup

### Step 1: Build System

```bash
# Build all crates
make build

# Or with MLX support
make build-mlx

# Build UI
cd ui && pnpm install && pnpm build && cd ..
```

**Expected output:**
```
Compiling adapteros-core v0.1.0
Compiling adapteros-server v0.1.0
...
Finished release [optimized] target(s)
```

**Related files:**
- `/Users/star/Dev/aos/Makefile:6-16` - Build targets
- `/Users/star/Dev/aos/Cargo.toml` - Workspace definition

### Step 2: Initialize Database

```bash
# Create database and run migrations
aosctl db migrate

# Verify migrations
sqlite3 var/aos-cp.sqlite3 "SELECT version, description FROM _sqlx_migrations ORDER BY version;"
```

**Expected output:**
```
1|init
2|add_adapters_table
...
20|add_audit_log
```

**Troubleshooting:**
- If migrations fail, check `/Users/star/Dev/aos/migrations/` for signature files
- All migrations must have `.sql.sig` Ed25519 signatures
- See [Database Failures](./database-failures.md) for migration errors

**Related files:**
- `/Users/star/Dev/aos/crates/adapteros-db/src/lib.rs:78-132` - Migration logic
- `/Users/star/Dev/aos/migrations/` - Migration SQL files

### Step 3: Initialize Baseline Fingerprint

```bash
# Capture environment baseline
cargo run --bin aos-cp -- --config configs/cp.toml
```

**First run creates:**
- `var/baseline_fingerprint.json` - Environment fingerprint
- `var/aos-cp.sqlite3` - Database with schema
- `var/aos-cp.pid` - PID lock file

**Related files:**
- `/Users/star/Dev/aos/crates/adapteros-server/src/main.rs:432-450` - Fingerprint creation
- `/Users/star/Dev/aos/crates/adapteros-verify/src/lib.rs` - Fingerprint logic

### Step 4: Start Server

```bash
# Start control plane
cargo run --bin aos-cp -- --config configs/cp.toml

# Or use convenience script
./scripts/start_server.sh
```

**Expected output:**
```
[INFO] Loading configuration from configs/cp.toml
[INFO] Connecting to database: var/aos-cp.sqlite3
[INFO] Running database migrations...
[INFO] ✓ All 20 migration signatures verified
[INFO] ✓ No environment drift detected
[INFO] Starting control plane on 127.0.0.1:8080
[INFO] UI available at http://127.0.0.1:8080/
[INFO] API available at http://127.0.0.1:8080/api/
```

**Related files:**
- `/Users/star/Dev/aos/crates/adapteros-server/src/main.rs:220-964` - Server startup
- `/Users/star/Dev/aos/scripts/start_server.sh` - Startup script

### Step 5: Verify Health

```bash
# Check all components
curl http://localhost:8080/healthz/all | jq

# Or use CLI
aosctl doctor
```

**Expected response:**
```json
{
  "overall_status": "healthy",
  "components": [
    {"component": "db", "status": "healthy"},
    {"component": "router", "status": "healthy"},
    {"component": "loader", "status": "healthy"},
    {"component": "kernel", "status": "healthy"},
    {"component": "telemetry", "status": "healthy"},
    {"component": "system-metrics", "status": "healthy"}
  ]
}
```

**Related files:**
- `/Users/star/Dev/aos/crates/adapteros-server-api/src/health.rs:459-556` - Health check aggregation
- `/Users/star/Dev/aos/crates/adapteros-cli/src/commands/doctor.rs` - Doctor command

## Production Checklist

Before deploying to production:

- [ ] All migrations verified and signed
- [ ] Environment fingerprint baseline captured
- [ ] Configuration reviewed (see `configs/cp.toml`)
- [ ] PF/firewall egress blocking enabled (`require_pf_deny = true`)
- [ ] Valid manifest for deterministic executor seeding
- [ ] JWT secret configured (not default)
- [ ] Metrics bearer token set
- [ ] Database backup strategy in place
- [ ] Log rotation configured
- [ ] Health check monitoring enabled

**Configuration file:**
```toml
# configs/cp.toml
[server]
port = 8080
host = "127.0.0.1"

[security]
require_pf_deny = true  # Production: must be true
jwt_secret = "CHANGE_IN_PRODUCTION"

[db]
path = "./var/aos-cp.sqlite3"
max_connections = 20

[metrics]
enabled = true
bearer_token = "CHANGE_IN_PRODUCTION"
```

## Common First-Run Issues

### Issue: "Another aos-cp process is running"

**Cause:** PID lock file exists from previous run

**Fix:**
```bash
# Check if process is actually running
ps aux | grep aos-cp

# If no process found, remove stale lock
rm var/aos-cp.pid

# Or start with --no-single-writer (not recommended)
cargo run --bin aos-cp -- --no-single-writer
```

**Related files:**
- `/Users/star/Dev/aos/crates/adapteros-server/src/main.rs:38-83` - PID lock logic

### Issue: "Migration signature verification failed"

**Cause:** Missing or invalid `.sql.sig` files

**Fix:**
```bash
# Re-sign migrations (requires signing key)
cargo run --bin sign-migrations

# Verify signatures
ls -la migrations/*.sig
```

**Related files:**
- `/Users/star/Dev/aos/crates/sign-migrations/src/main.rs` - Migration signing
- `/Users/star/Dev/aos/crates/adapteros-db/src/migration_verify.rs` - Signature verification

### Issue: "Port 8080 already in use"

**Cause:** Another service using port 8080

**See:** [Port Binding Conflicts](./port-binding-conflicts.md)

## Next Steps

After successful startup:

1. Initialize tenant: `aosctl init-tenant --id default --uid 1000 --gid 1000`
2. Import base model: See [Model Import Guide](../MODEL_IMPORT.md)
3. Register adapters: `aosctl register-adapter <name> <hash> --tier persistent`
4. Start training: `aosctl train --data training.json --output adapter/`

## Related Runbooks

- [Startup Failures](./startup-failures.md)
- [Database Failures](./database-failures.md)
- [Port Binding Conflicts](./port-binding-conflicts.md)
- [Health Check Failures](./health-check-failures.md)
