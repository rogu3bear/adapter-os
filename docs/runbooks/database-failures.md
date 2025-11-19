# Database Failures

Migration errors, connection issues, and WAL mode problems.

## Symptoms

- Server fails to start with database errors
- "Migration failed" errors during startup
- "Database is locked" messages
- WAL checkpoint failures
- Schema version mismatch errors
- Foreign key constraint violations

## Common Failure Modes

### 1. Migration Signature Verification Failed

**Symptoms:**
```
[ERROR] Migration signature verification failed
[ERROR] Missing signature file: migrations/0015_add_routing_decisions.sql.sig
```

**Root Cause:**
- Missing `.sql.sig` file for migration
- Invalid Ed25519 signature
- Corrupted migration file

**Fix Procedure:**
```bash
# Step 1: List migrations and signatures
ls -la migrations/*.sql migrations/*.sig

# Step 2: Verify which signatures are missing
for sql in migrations/*.sql; do
  if [ ! -f "$sql.sig" ]; then
    echo "Missing: $sql.sig"
  fi
done

# Step 3: Re-sign migrations (requires signing key)
cargo run --bin sign-migrations

# Step 4: Verify signatures
cargo run --bin aos-cp -- --migrate-only
```

**Prevention:**
- Never commit `.sql` files without corresponding `.sig` files
- Run pre-commit hooks to verify signatures
- Use `make check` before pushing changes

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-db/src/lib.rs:106-114` - Signature verification
- `/Users/star/Dev/aos/crates/sign-migrations/src/main.rs` - Migration signing tool
- `/Users/star/Dev/aos/crates/adapteros-db/src/migration_verify.rs` - Verification logic

### 2. Schema Version Mismatch

**Symptoms:**
```
[ERROR] ❌ SCHEMA VERSION MISMATCH: Database at version 18, expected 20
[ERROR] Database is BEHIND - 2 migrations missing
[ERROR] Schema version mismatch: DB version 18 != expected 20. Server cannot start with mismatched schema.
```

**Root Cause:**
- Database not migrated after code update
- Migration files removed from filesystem
- Database rolled back without code rollback

**Fix Procedure:**

**Option A: Run Missing Migrations**
```bash
# Step 1: Check current version
sqlite3 var/aos-cp.sqlite3 "SELECT version, description FROM _sqlx_migrations ORDER BY version DESC LIMIT 1;"

# Step 2: Count migration files
ls -1 migrations/*.sql | wc -l

# Step 3: Run migrations
aosctl db migrate

# Step 4: Verify version
sqlite3 var/aos-cp.sqlite3 "SELECT version FROM _sqlx_migrations ORDER BY version DESC LIMIT 1;"
```

**Option B: Reset Database (Development Only)**
```bash
# WARNING: This destroys all data
aosctl db reset

# Recreates database with all migrations
aosctl db migrate
```

**Option C: Rollback Code**
```bash
# If database is ahead of code
git checkout <previous-version>
cargo build --release
```

**Prevention:**
- Always run migrations after pulling code updates
- Never delete migration files in production
- Use version tags for coordinated deployments

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-db/src/lib.rs:141-200` - Version verification logic
- `/Users/star/Dev/aos/migrations/` - Migration files directory

### 3. Database Locked Errors

**Symptoms:**
```
[ERROR] Failed to connect to database: database is locked
[ERROR] SQLITE_BUSY: database is locked
```

**Root Cause:**
- Multiple processes accessing database without WAL mode
- Long-running transaction blocking writers
- Busy timeout too short
- Filesystem permission issues

**Diagnostic Commands:**
```bash
# Check if database is in WAL mode
sqlite3 var/aos-cp.sqlite3 "PRAGMA journal_mode;"
# Should output: wal

# Check for other processes
lsof var/aos-cp.sqlite3

# Check file permissions
ls -la var/aos-cp.sqlite3*

# Check for stale PID locks
ps aux | grep aos-cp
```

**Fix Procedure:**

**Step 1: Enable WAL Mode (if not enabled)**
```bash
sqlite3 var/aos-cp.sqlite3 "PRAGMA journal_mode=WAL;"
sqlite3 var/aos-cp.sqlite3 "PRAGMA journal_mode;"
# Verify: should show "wal"
```

**Step 2: Kill Blocking Processes**
```bash
# Find processes
lsof var/aos-cp.sqlite3

# Kill if necessary
kill -9 <PID>

# Remove stale lock
rm var/aos-cp.pid
```

**Step 3: Fix Permissions**
```bash
chmod 644 var/aos-cp.sqlite3
chmod 644 var/aos-cp.sqlite3-wal
chmod 644 var/aos-cp.sqlite3-shm
```

**Step 4: Increase Busy Timeout**
```bash
# Edit connection options in code
# busy_timeout is set to 30s by default
```

**Prevention:**
- Always use WAL mode in production
- Set appropriate busy_timeout (30s default)
- Use connection pooling (max 20 connections)
- Avoid long-running transactions

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-db/src/lib.rs:33-56` - Connection configuration
- `/Users/star/Dev/aos/crates/adapteros-db/src/lib.rs:46` - WAL mode setup

### 4. WAL Checkpoint Failures

**Symptoms:**
```
[WARN] WAL checkpoint failed
[WARN] WAL file growing unbounded
```

**Root Cause:**
- Readers holding open transactions
- Checkpoint not running frequently enough
- High write volume

**Diagnostic Commands:**
```bash
# Check WAL file size
ls -lh var/aos-cp.sqlite3-wal

# Check database integrity
sqlite3 var/aos-cp.sqlite3 "PRAGMA integrity_check;"

# Force checkpoint
sqlite3 var/aos-cp.sqlite3 "PRAGMA wal_checkpoint(TRUNCATE);"
```

**Fix Procedure:**

**Step 1: Manual Checkpoint**
```bash
# Stop server
pkill -SIGTERM aos-cp

# Force checkpoint
sqlite3 var/aos-cp.sqlite3 <<EOF
PRAGMA wal_checkpoint(TRUNCATE);
PRAGMA optimize;
EOF

# Verify WAL size reduced
ls -lh var/aos-cp.sqlite3-wal

# Restart server
./scripts/start_server.sh
```

**Step 2: Configure Automatic Checkpoints**
```bash
sqlite3 var/aos-cp.sqlite3 "PRAGMA wal_autocheckpoint=1000;"
```

**Step 3: Optimize Database**
```bash
sqlite3 var/aos-cp.sqlite3 "PRAGMA optimize;"
```

**Prevention:**
- Run periodic checkpoints (see [Database Optimization](./database-optimization.md))
- Monitor WAL file size
- Avoid long-running read transactions
- Use `PRAGMA optimize` during maintenance windows

**Related Files:**
- `/Users/star/Dev/aos/docs/database-schema.md:1544` - WAL mode configuration

### 5. Foreign Key Constraint Violations

**Symptoms:**
```
[ERROR] FOREIGN KEY constraint failed
[ERROR] Failed to delete adapter: constraint violation
```

**Root Cause:**
- Attempting to delete parent record with dependent children
- Foreign keys not enabled
- Incorrect deletion order

**Diagnostic Commands:**
```bash
# Check foreign key enforcement
sqlite3 var/aos-cp.sqlite3 "PRAGMA foreign_keys;"
# Should output: 1 (enabled)

# List foreign key constraints for a table
sqlite3 var/aos-cp.sqlite3 "PRAGMA foreign_key_list(adapters);"

# Check for orphaned records
sqlite3 var/aos-cp.sqlite3 "PRAGMA foreign_key_check;"
```

**Fix Procedure:**

**Step 1: Enable Foreign Keys**
```bash
sqlite3 var/aos-cp.sqlite3 "PRAGMA foreign_keys = ON;"
```

**Step 2: Find Dependent Records**
```bash
# Example: Find adapters dependent on a stack
sqlite3 var/aos-cp.sqlite3 <<EOF
SELECT adapter_id, name FROM adapters
WHERE stack_id = '<stack-id>';
EOF
```

**Step 3: Delete in Correct Order**
```bash
# Delete children first
aosctl adapter delete <adapter-id>

# Then delete parent
aosctl stack delete <stack-id>
```

**Prevention:**
- Always enable foreign keys: `PRAGMA foreign_keys = ON`
- Use cascading deletes where appropriate
- Delete dependent records before parents

**Related Files:**
- `/Users/star/Dev/aos/migrations/rollbacks/QUICK_REFERENCE.md:68` - Foreign key reference
- `/Users/star/Dev/aos/crates/adapteros-db/src/lib.rs` - Database operations

## Emergency Recovery

### Database Corrupted Beyond Repair

**Last Resort Procedure:**

```bash
# Step 1: Stop server
pkill -SIGTERM aos-cp

# Step 2: Backup current database
cp var/aos-cp.sqlite3 var/aos-cp.sqlite3.corrupt.$(date +%s)

# Step 3: Attempt integrity check
sqlite3 var/aos-cp.sqlite3 "PRAGMA integrity_check;"

# Step 4: Export data if possible
sqlite3 var/aos-cp.sqlite3 .dump > var/dump.sql

# Step 5: Recreate database
rm var/aos-cp.sqlite3*
aosctl db reset
aosctl db migrate

# Step 6: Import critical data
sqlite3 var/aos-cp.sqlite3 < var/critical_data.sql

# Step 7: Verify
aosctl doctor
```

## Related Runbooks

- [Database Optimization](./database-optimization.md)
- [Startup Failures](./startup-failures.md)
- [Cleanup Procedures](./cleanup-procedures.md)

## Escalation Criteria

Escalate to engineering if:
- Database corruption detected
- Multiple migration failures
- Data loss suspected
- Schema inconsistency across nodes
- See [Escalation Guide](./escalation.md)
