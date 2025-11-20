# AOS Loader Telemetry Event Flow

**Purpose:** Complete reference for telemetry events emitted by the AOS loader subsystem

**Last Updated:** 2025-01-19

**Maintained by:** James KC Auchterlonie

---

## Overview

The AOS loader subsystem emits structured telemetry events for all adapter loading, upload, and deletion operations. This enables:

- **Performance monitoring**: Track load times, file sizes, and throughput
- **Security auditing**: Log permission checks and policy violations
- **Debugging**: Trace errors through the complete load pipeline
- **Compliance**: Maintain audit trail for adapter lifecycle events

---

## Event Catalog

### 1. Adapter Load Events

#### adapter.loaded

**Event Type:** `EventType::AdapterLoaded`

**Level:** Info

**When Emitted:** After successfully loading a .aos file via MmapAdapterLoader

**Source:** `crates/adapteros-aos/src/mmap_loader.rs` (lines 367-380)

**Metadata Fields:**

```json
{
  "path": "/path/to/adapter.aos",
  "adapter_id": "adapter_abc123",
  "base_model": "llama-2-7b",
  "version": "2.0",
  "weights_hash": "blake3_abc123...",
  "size_bytes": 52428800,
  "tensor_count": 42,
  "duration_ms": 156,
  "loader_type": "mmap"
}
```

**Required Fields:**
- `adapter_id` - Unique adapter identifier
- `base_model` - Base model name
- `version` - AOS format version
- `weights_hash` - BLAKE3 hash of weights
- `size_bytes` - File size in bytes
- `tensor_count` - Number of tensors loaded
- `duration_ms` - Load duration in milliseconds
- `loader_type` - Always "mmap" for memory-mapped loader

**Example Query:**

```sql
SELECT * FROM telemetry_events
WHERE event_type = 'AdapterLoaded'
  AND json_extract(metadata, '$.duration_ms') > 1000
  AND timestamp >= datetime('now', '-1 hour')
ORDER BY timestamp DESC;
```

---

#### adapter.load.error

**Event Type:** `EventType::Custom("adapter.load.error")`

**Level:** Error

**When Emitted:** When adapter load fails (file not found, I/O error, parse error)

**Source:** `crates/adapteros-aos/src/mmap_loader.rs` (lines 311-316, 320-322)

**Metadata Fields:**

```json
{
  "path": "/path/to/adapter.aos",
  "error": "Failed to open .aos file: No such file or directory",
  "loader_type": "mmap"
}
```

**Common Error Types:**
- File not found
- Permission denied
- I/O read error
- Memory mapping failed
- Parse error

**Example Query:**

```sql
SELECT * FROM telemetry_events
WHERE event_type = 'adapter.load.error'
  AND timestamp >= datetime('now', '-24 hours')
GROUP BY json_extract(metadata, '$.error')
ORDER BY COUNT(*) DESC;
```

---

### 2. Policy Violation Events

#### adapter.policy.violation

**Event Type:** `EventType::PolicyViolation`

**Level:** Warn

**When Emitted:** When adapter file exceeds maximum size limit

**Source:** `crates/adapteros-aos/src/mmap_loader.rs` (lines 329-330)

**Metadata Fields:**

```json
{
  "path": "/path/to/large_adapter.aos",
  "size_bytes": 1073741824,
  "max_bytes": 524288000,
  "policy": "max_file_size",
  "loader_type": "mmap"
}
```

**Policy Details:**
- Default max size: 500MB (configurable via `MmapAdapterLoader::with_max_file_size()`)
- Prevents OOM attacks and resource exhaustion
- Consistent with Policy Pack #2 (Resource Limits)

**Example Query:**

```sql
SELECT * FROM telemetry_events
WHERE event_type = 'PolicyViolation'
  AND json_extract(metadata, '$.policy') = 'max_file_size'
  AND timestamp >= datetime('now', '-7 days')
ORDER BY timestamp DESC;
```

---

### 3. Validation Events

#### adapter.validation.error

**Event Type:** `EventType::Custom("adapter.validation.error")`

**Level:** Warn

**When Emitted:** When adapter file fails format validation

**Source:** `crates/adapteros-aos/src/mmap_loader.rs` (lines 340-341)

**Metadata Fields:**

```json
{
  "path": "/path/to/invalid.aos",
  "reason": "File too small to be valid .aos file",
  "loader_type": "mmap"
}
```

**Common Validation Failures:**
- File too small (< 8 bytes)
- Invalid header format
- Manifest offset out of bounds
- Manifest JSON parse error
- Unsupported format version
- Weights section corruption

**Example Query:**

```sql
SELECT * FROM telemetry_events
WHERE event_type = 'adapter.validation.error'
  AND timestamp >= datetime('now', '-1 day')
GROUP BY json_extract(metadata, '$.reason')
ORDER BY COUNT(*) DESC;
```

---

### 4. Upload Events

#### adapter.upload.success

**Event Type:** `EventType::Custom("adapter.upload.success")`

**Level:** Info

**When Emitted:** After successfully uploading .aos file via REST API

**Source:** `crates/adapteros-server-api/src/handlers/aos_upload.rs` (lines 456-457)

**Metadata Fields:**

```json
{
  "tenant_id": "tenant_abc",
  "adapter_id": "adapter_xyz789",
  "hash_b3": "blake3_def456...",
  "size_bytes": 104857600,
  "duration_ms": 2340,
  "operation": "upload"
}
```

**Required Fields:**
- `tenant_id` - Tenant identifier
- `adapter_id` - Generated adapter ID
- `hash_b3` - BLAKE3 hash of uploaded file
- `size_bytes` - Upload size in bytes
- `duration_ms` - Total upload duration (includes validation, write, hash verification)
- `operation` - Always "upload"

**Example Query:**

```sql
SELECT
  json_extract(metadata, '$.tenant_id') AS tenant,
  COUNT(*) AS upload_count,
  AVG(json_extract(metadata, '$.duration_ms')) AS avg_duration_ms,
  SUM(json_extract(metadata, '$.size_bytes')) AS total_bytes
FROM telemetry_events
WHERE event_type = 'adapter.upload.success'
  AND timestamp >= datetime('now', '-30 days')
GROUP BY tenant
ORDER BY upload_count DESC;
```

---

#### adapter.upload.permission_denied

**Event Type:** `EventType::Custom("adapter.upload.permission_denied")`

**Level:** Warn

**When Emitted:** When user lacks AdapterRegister permission

**Source:** `crates/adapteros-server-api/src/handlers/aos_upload.rs` (line 136)

**Metadata Fields:**

```json
{
  "tenant_id": "tenant_abc",
  "reason": "User lacks required permission: AdapterRegister",
  "operation": "upload"
}
```

**Security Implications:**
- Indicates potential unauthorized access attempt
- Should trigger security review if frequency is high
- Correlated with RBAC audit logs

**Example Query:**

```sql
SELECT
  user_id,
  json_extract(metadata, '$.tenant_id') AS tenant,
  COUNT(*) AS denied_count
FROM telemetry_events
WHERE event_type = 'adapter.upload.permission_denied'
  AND timestamp >= datetime('now', '-7 days')
GROUP BY user_id, tenant
HAVING denied_count > 5
ORDER BY denied_count DESC;
```

---

### 5. Deletion Events

#### adapter.unloaded

**Event Type:** `EventType::AdapterUnloaded`

**Level:** Info

**When Emitted:** After successfully deleting .aos adapter via REST API

**Source:** `crates/adapteros-server-api/src/handlers/aos_upload.rs` (line 515)

**Metadata Fields:**

```json
{
  "tenant_id": "tenant_abc",
  "adapter_id": "adapter_xyz789",
  "operation": "delete"
}
```

**Required Fields:**
- `tenant_id` - Tenant identifier
- `adapter_id` - Deleted adapter ID
- `operation` - Always "delete"

**Example Query:**

```sql
SELECT * FROM telemetry_events
WHERE event_type = 'AdapterUnloaded'
  AND json_extract(metadata, '$.operation') = 'delete'
  AND timestamp >= datetime('now', '-24 hours')
ORDER BY timestamp DESC;
```

---

## Event Flow Diagrams

### Successful Load Flow

```
User/System
    |
    v
[MmapAdapterLoader::load()]
    |
    |--> Open file
    |--> Read metadata
    |--> Check file size
    |    |--> [emit_policy_violation()] if too large
    |
    |--> Validate header
    |    |--> [emit_validation_error()] if invalid
    |
    |--> Create memory map
    |--> Parse manifest
    |--> Parse tensors
    |
    |--> [emit_load_success()] ✓
    |
    v
Return MmapAdapter
```

### Error Flow

```
User/System
    |
    v
[MmapAdapterLoader::load()]
    |
    |--> Open file
    |    |--> FAIL: File not found
    |    |--> [emit_load_error()] ✗
    |    |--> Return Err(AosError::Io)
    |
    v
Error propagated to caller
```

### Upload Flow

```
User (via REST API)
    |
    v
[upload_aos_adapter()]
    |
    |--> require_permission(AdapterRegister)
    |    |--> FAIL: Permission denied
    |    |--> [emit_upload_permission_denied()] ✗
    |    |--> [log_failure()] (audit)
    |    |--> Return 403 Forbidden
    |
    |--> Validate multipart form
    |--> Check file extension (.aos)
    |--> Check file size
    |--> Generate adapter_id
    |--> Compute BLAKE3 hash
    |--> Write to temp file
    |--> Sync to disk
    |--> Rename to final path
    |--> Read back and verify hash
    |--> Register in database
    |
    |--> [emit_upload_success()] ✓
    |--> [log_success()] (audit)
    |
    v
Return AosUploadResponse
```

---

## Telemetry Integration Points

### 1. MmapAdapterLoader

**Crate:** `adapteros-aos`

**File:** `crates/adapteros-aos/src/mmap_loader.rs`

**Events Emitted:**
- `AdapterLoaded` (line 370)
- `adapter.load.error` (lines 313, 321)
- `PolicyViolation` (line 330)
- `adapter.validation.error` (line 341)

**Feature Flag:** `telemetry`

**Usage:**

```rust
use adapteros_aos::MmapAdapterLoader;

let loader = MmapAdapterLoader::new();
let adapter = loader.load("./adapters/my_adapter.aos").await?;
// Telemetry automatically emitted
```

---

### 2. AOS Upload Handler

**Crate:** `adapteros-server-api`

**File:** `crates/adapteros-server-api/src/handlers/aos_upload.rs`

**Events Emitted:**
- `adapter.upload.success` (line 457)
- `adapter.upload.permission_denied` (line 136)
- `AdapterUnloaded` (line 515)

**Always Enabled:** Yes (no feature flag required)

**Usage:**

```bash
curl -X POST http://localhost:8080/v1/adapters/upload-aos \
  -H "Authorization: Bearer $TOKEN" \
  -F "file=@adapter.aos" \
  -F "name=my-adapter" \
  -F "tier=persistent"
# Telemetry automatically emitted
```

---

### 3. Audit Logging Integration

Upload and deletion operations emit **both** telemetry events **and** audit logs:

**Audit Actions:**
- `adapter.upload` (via `log_success()` / `log_failure()`)
- `adapter.delete` (via `log_success()`)

**Audit Table:** `audit_logs`

**Correlation:** Use `adapter_id` to correlate telemetry events with audit entries

**Example Correlation Query:**

```sql
SELECT
  t.timestamp AS telemetry_time,
  t.event_type AS telemetry_event,
  a.timestamp AS audit_time,
  a.action AS audit_action,
  a.status AS audit_status
FROM telemetry_events t
JOIN audit_logs a ON json_extract(t.metadata, '$.adapter_id') = a.resource_id
WHERE t.event_type = 'adapter.upload.success'
  AND a.action = 'adapter.upload'
  AND t.timestamp >= datetime('now', '-1 hour')
ORDER BY t.timestamp DESC;
```

---

## Performance Monitoring

### Key Metrics

| Metric | Query | Threshold |
|--------|-------|-----------|
| **Load Duration** | `AVG(json_extract(metadata, '$.duration_ms'))` | < 500ms (for < 100MB files) |
| **Upload Duration** | `AVG(json_extract(metadata, '$.duration_ms'))` | < 5000ms (for < 500MB files) |
| **Failure Rate** | `COUNT(*) WHERE event_type LIKE '%error%' / COUNT(*)` | < 5% |
| **Policy Violations** | `COUNT(*) WHERE event_type = 'PolicyViolation'` | < 10/day |

### Performance Query Examples

#### 1. Slowest Loads (Last 24 Hours)

```sql
SELECT
  json_extract(metadata, '$.adapter_id') AS adapter,
  json_extract(metadata, '$.size_bytes') AS size,
  json_extract(metadata, '$.duration_ms') AS duration,
  ROUND(CAST(json_extract(metadata, '$.size_bytes') AS REAL) /
        (json_extract(metadata, '$.duration_ms') / 1000.0) / 1024 / 1024, 2) AS mb_per_sec
FROM telemetry_events
WHERE event_type = 'AdapterLoaded'
  AND timestamp >= datetime('now', '-24 hours')
ORDER BY duration DESC
LIMIT 10;
```

#### 2. Upload Throughput by Hour

```sql
SELECT
  strftime('%Y-%m-%d %H:00', timestamp) AS hour,
  COUNT(*) AS uploads,
  SUM(json_extract(metadata, '$.size_bytes')) / 1024 / 1024 AS total_mb,
  AVG(json_extract(metadata, '$.duration_ms')) AS avg_duration_ms
FROM telemetry_events
WHERE event_type = 'adapter.upload.success'
  AND timestamp >= datetime('now', '-7 days')
GROUP BY hour
ORDER BY hour DESC;
```

#### 3. Error Distribution

```sql
SELECT
  event_type,
  json_extract(metadata, '$.error') AS error_type,
  COUNT(*) AS count
FROM telemetry_events
WHERE event_type LIKE '%error%'
  AND timestamp >= datetime('now', '-24 hours')
GROUP BY event_type, error_type
ORDER BY count DESC;
```

---

## Testing

### Telemetry Test Suite

**File:** `crates/adapteros-aos/tests/telemetry_tests.rs`

**Coverage:**
- ✅ Load success emits telemetry
- ✅ File not found emits error
- ✅ File too large emits policy violation
- ✅ File too small emits validation error
- ✅ Event ordering is correct
- ✅ No events are dropped
- ✅ Performance metrics are included

**Run Tests:**

```bash
cargo test --package adapteros-aos --test telemetry_tests --features "mmap,telemetry"
```

### Manual Validation

```bash
# 1. Start server with telemetry enabled
cargo run --bin adapteros-server --features telemetry

# 2. Upload an adapter
curl -X POST http://localhost:8080/v1/adapters/upload-aos \
  -H "Authorization: Bearer $TOKEN" \
  -F "file=@test_adapter.aos"

# 3. Query telemetry events
sqlite3 var/aos-cp.sqlite3 \
  "SELECT * FROM telemetry_events ORDER BY timestamp DESC LIMIT 10;"
```

---

## Compliance

### Policy Pack #9 (Telemetry)

**Requirement:** "MUST log events with canonical JSON"

**Compliance:**
- ✅ All events use `TelemetryEventBuilder` with canonical JSON metadata
- ✅ Events stored in `telemetry_events` table with indexed `event_type` and `timestamp`
- ✅ Metadata follows consistent schema (see TELEMETRY_EVENTS.md)

### RBAC Integration

**Audit Actions:**
- `adapter.upload` - Logged on upload (success/failure)
- `adapter.delete` - Logged on deletion (success/failure)

**Permissions Required:**
- Upload: `Permission::AdapterRegister`
- Delete: `Permission::AdapterDelete`

**Permission Denied Events:**
- Emit telemetry event: `adapter.upload.permission_denied`
- Log audit failure: `log_failure(actions::ADAPTER_UPLOAD, ...)`

---

## Troubleshooting

### Issue: No telemetry events emitted

**Diagnosis:**
1. Check feature flag: `cargo build --features telemetry`
2. Verify `TelemetryWriter` is initialized
3. Check database schema: `SELECT * FROM telemetry_events LIMIT 1;`

**Fix:**
```rust
use adapteros_telemetry::TelemetryWriter;

// Initialize telemetry writer
let writer = TelemetryWriter::new_with_db(&db).await?;
```

---

### Issue: Events emitted but not queryable

**Diagnosis:**
1. Check database connection
2. Verify table exists: `.schema telemetry_events`
3. Check write permissions

**Fix:**
```bash
sqlite3 var/aos-cp.sqlite3 ".schema telemetry_events"
# Should show CREATE TABLE statement
```

---

### Issue: High event drop rate

**Diagnosis:**
1. Check channel capacity: `TelemetryWriter::with_capacity()`
2. Monitor memory usage
3. Check disk I/O performance

**Fix:**
```rust
// Increase channel capacity
let writer = TelemetryWriter::with_capacity(10000);

// Or use batch writes
let writer = TelemetryWriter::with_batch_size(100);
```

---

## See Also

- [TELEMETRY_EVENTS.md](TELEMETRY_EVENTS.md) - Canonical event catalog
- [TELEMETRY_ARCHITECTURE.md](TELEMETRY_ARCHITECTURE.md) - Overall design
- [RBAC.md](RBAC.md) - Permission system
- [CLAUDE.md](../CLAUDE.md) - Developer guide
- [docs/AUDIT_LOGGING.md](AUDIT_LOGGING.md) - Audit log reference

---

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.
