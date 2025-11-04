# QA Testing Issues Found

## Issue #1: Duplicate Route Definition - Hot-Swap Endpoint

**Status:** BLOCKING - Server fails to start (RESOLVED - Route commented out)

**Description:**
The server fails to start with the error:
```
Overlapping method route. Handler for `POST /v1/adapters/:adapter_id/hot-swap` already exists
```

**Location:**
- `crates/adapteros-server-api/src/routes.rs:496`

**Impact:**
- Server cannot start
- Hot-swap functionality unavailable

**Reproduction:**
1. Run: `./target/release/adapteros-server --config configs/cp.toml`
2. Server panics immediately with duplicate route error

**Temporary Workaround:**
Route is currently commented out in routes.rs (line 495-499)

**Investigation Needed:**
- Find where the duplicate route is being registered
- May be in a nested router or merge operation
- Check if domain_adapters module has conflicting route pattern

**Next Steps:**
- Search for all route registrations that could match this pattern
- Check router merge operations for conflicts
- Fix duplicate registration and re-enable route

---

## Issue #2: Database Migration Failure

**Status:** BLOCKING - Server fails to start

**Description:**
Server fails during database migration with error:
```
Error: Other("Migration failed")
```

**Location:**
- Database migration execution during server startup
- Log shows: `Running database migrations` followed by failure

**Impact:**
- Server cannot start
- Database operations unavailable

**Reproduction:**
1. Run: `./target/release/adapteros-server --config configs/cp.toml`
2. Server starts initialization but fails at migration step

**Log Output:**
```
2025-11-02T03:07:23.759953Z  INFO adapteros_server: Running database migrations
...
Error: Other("Migration failed")
```

**Investigation Needed:**
- Check migration files for syntax errors
- Verify database schema compatibility
- Check migration execution logs for specific failure

**Next Steps:**
- Review migration files in `migrations/` directory
- Check database connection and permissions
- Run migrations manually to identify specific failure point

