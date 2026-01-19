# PRD-002: Unwrap Elimination Phase 1 - Critical Request Handlers

**Status**: Draft
**Priority**: P0 (Production Blocker)
**Estimated Effort**: 4-6 hours
**Owner**: TBD

---

## 1. Problem Statement

The `adapteros-server-api` crate contains approximately 789 instances of `.unwrap()` and `.expect()` calls. Of these, ~15-20 are in critical request handler paths where a panic will:

1. Crash the current request with a 500 error
2. Leave no structured error response for clients
3. Potentially leak stack traces in logs
4. Create gaps in audit trails (no receipt for failed request)

In production, these represent denial-of-service vectors where malformed input can crash request handling.

---

## 2. Scope

### In Scope (Phase 1 - Critical Path Only)

| File | Risk Level | Instance Count |
|------|------------|----------------|
| `handlers/chunked_upload.rs` | HIGH | 3 |
| `handlers/training.rs` | HIGH | 2 |
| `handlers/streaming_infer.rs` | HIGH | 12 |
| `handlers/event_applier.rs` | HIGH | 8 |
| `handlers/datasets/helpers.rs` | HIGH | 2 |
| `handlers/datasets/synthesize.rs` | MEDIUM | 2 |
| `middleware/policy_enforcement.rs` | HIGH | 2 |

**Total Phase 1 instances**: ~31

### Out of Scope (Phase 2+)

- Test code `.unwrap()` calls (acceptable in tests)
- Infallible patterns (e.g., `Mutex::lock()` on non-poisoned lock)
- Low-risk utility functions
- Non-request-path code

---

## 3. Technical Analysis

### 3.1 Error Handling Patterns

Replace `.unwrap()` with appropriate error handling:

| Pattern | Use Case | Example |
|---------|----------|---------|
| `?` operator | Propagate to caller | `let x = fallible_op()?;` |
| `.ok_or(ApiError::X)?` | Convert Option to Result | `opt.ok_or(ApiError::MissingField)?` |
| `.map_err(ApiError::from)?` | Convert error types | `io_op().map_err(ApiError::Io)?` |
| `.unwrap_or(default)` | Safe default exists | `opt.unwrap_or("")` |
| `.unwrap_or_else(|| ...)` | Computed default | `opt.unwrap_or_else(|| compute())` |

### 3.2 Critical Instances Detail

#### 3.2.1 Chunked Upload - Workspace ID

**File**: `handlers/chunked_upload.rs:280`

```rust
// BEFORE (panics if None)
workspace_id.as_deref().unwrap()

// AFTER (returns 400 Bad Request)
workspace_id.as_deref().ok_or_else(|| {
    ApiError::bad_request("workspace_id is required")
})?
```

**Trigger**: Client omits `workspace_id` query parameter
**Impact**: Upload endpoint crashes

#### 3.2.2 Training Handler - Dataset ID

**File**: `handlers/training.rs:1412`

```rust
// BEFORE (panics if None)
let dataset_id = req.dataset_id.clone().unwrap();

// AFTER (returns 400 Bad Request)
let dataset_id = req.dataset_id.clone().ok_or_else(|| {
    ApiError::bad_request("dataset_id is required for training")
})?;
```

**Trigger**: Training request without `dataset_id` field
**Impact**: Training endpoint crashes

#### 3.2.3 Streaming Inference - JSON Serialization

**File**: `handlers/streaming_infer.rs:2103, 2129, 2231, 2243, 2255, 2274, 2293`

```rust
// BEFORE (panics on serialization failure)
let json = serde_json::to_string(&chunk).unwrap();

// AFTER (logs error and returns 500)
let json = serde_json::to_string(&chunk).map_err(|e| {
    tracing::error!(error = %e, "Failed to serialize streaming chunk");
    ApiError::internal("Response serialization failed")
})?;
```

**Trigger**: Non-serializable value (NaN, circular ref)
**Impact**: Streaming response crashes mid-stream

#### 3.2.4 Event Applier - Database Transactions

**File**: `handlers/event_applier.rs:699, 714, 751, 789`

```rust
// BEFORE (panics on DB failure)
let mut tx = pool.begin().await.unwrap();
tx.commit().await.unwrap();

// AFTER (returns 503 Service Unavailable)
let mut tx = pool.begin().await.map_err(|e| {
    tracing::error!(error = %e, "Database transaction failed to start");
    ApiError::service_unavailable("Database temporarily unavailable")
})?;

tx.commit().await.map_err(|e| {
    tracing::error!(error = %e, "Database commit failed");
    ApiError::internal("Failed to persist changes")
})?;
```

**Trigger**: Database connection pool exhausted, connection lost
**Impact**: Event processing crashes

#### 3.2.5 Policy Enforcement Middleware

**File**: `middleware/policy_enforcement.rs:874-875`

```rust
// BEFORE (panics in middleware - affects ALL requests)
let metadata_json = stable_metadata_json_for_audit(&ctx).expect("expected metadata");
let parsed: serde_json::Value = serde_json::from_str(&metadata_json).unwrap();

// AFTER (returns 500 with context)
let metadata_json = stable_metadata_json_for_audit(&ctx).map_err(|e| {
    tracing::error!(error = %e, "Failed to generate audit metadata");
    ApiError::internal("Audit metadata generation failed")
})?;

let parsed: serde_json::Value = serde_json::from_str(&metadata_json).map_err(|e| {
    tracing::error!(error = %e, json = %metadata_json, "Invalid audit metadata JSON");
    ApiError::internal("Audit metadata parsing failed")
})?;
```

**Trigger**: Malformed audit context
**Impact**: ALL requests through this middleware crash

#### 3.2.6 File System Operations

**File**: `handlers/chunked_upload.rs:1044-1045`

```rust
// BEFORE (panics on disk issues)
std::fs::create_dir_all(&temp_root).unwrap();
let temp_dir = tempfile::TempDir::new_in(&temp_root).unwrap();

// AFTER (returns 507 Insufficient Storage or 500)
std::fs::create_dir_all(&temp_root).map_err(|e| {
    tracing::error!(error = %e, path = %temp_root.display(), "Failed to create temp directory");
    match e.kind() {
        std::io::ErrorKind::PermissionDenied => ApiError::forbidden("Upload directory not writable"),
        _ => ApiError::insufficient_storage("Cannot create upload directory"),
    }
})?;

let temp_dir = tempfile::TempDir::new_in(&temp_root).map_err(|e| {
    tracing::error!(error = %e, "Failed to create temp file");
    ApiError::insufficient_storage("Disk space unavailable for upload")
})?;
```

**Trigger**: Disk full, permission denied
**Impact**: Upload endpoint crashes

#### 3.2.7 UTF-8 Conversion

**File**: `handlers/streaming_infer.rs:2193`

```rust
// BEFORE (panics on invalid UTF-8)
let body_str = String::from_utf8(body.to_vec()).unwrap();

// AFTER (returns 400 or uses lossy conversion)
let body_str = String::from_utf8(body.to_vec()).map_err(|e| {
    tracing::warn!(error = %e, "Response contained invalid UTF-8");
    ApiError::bad_request("Response encoding error")
})?;

// OR if lossy is acceptable:
let body_str = String::from_utf8_lossy(&body).into_owned();
```

**Trigger**: Binary data in response, encoding mismatch
**Impact**: Streaming response crashes

---

## 4. Implementation Plan

### Phase 1.1: Middleware (Highest Priority)

Middleware affects ALL requests. Fix first.

1. `middleware/policy_enforcement.rs:874-875`

### Phase 1.2: Core Request Handlers

2. `handlers/chunked_upload.rs:280, 1044-1045`
3. `handlers/training.rs:1412`
4. `handlers/event_applier.rs:699, 714, 751, 789`

### Phase 1.3: Streaming Inference

5. `handlers/streaming_infer.rs` (all 12 instances)

### Phase 1.4: Dataset Operations

6. `handlers/datasets/helpers.rs:755`
7. `handlers/datasets/synthesize.rs:406, 461`

---

## 5. Acceptance Criteria

- [ ] All 31 identified `.unwrap()` calls replaced with proper error handling
- [ ] Each replacement includes structured logging with context
- [ ] HTTP status codes are semantically correct (400/403/500/503/507)
- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace -- -D warnings` passes
- [ ] No new `.unwrap()` calls introduced in touched files

---

## 6. Testing Strategy

### 6.1 Unit Tests

Add tests for each error path:

```rust
#[tokio::test]
async fn test_chunked_upload_missing_workspace_id() {
    let response = upload_handler(Request::without_workspace_id()).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body: ErrorResponse = response.json().await;
    assert!(body.message.contains("workspace_id"));
}
```

### 6.2 Integration Tests

```bash
# Test malformed requests don't crash server
curl -X POST http://localhost:8080/api/upload \
  -H "Content-Type: application/json" \
  -d '{"invalid": "request"}'
# Expected: 400 Bad Request, not 500 Internal Server Error
```

### 6.3 Regression Tests

```bash
cargo test --workspace
cargo test -p adapteros-server-api
```

---

## 7. Rollback Plan

Each file change is isolated. If a specific change causes issues:

1. Revert the single file change
2. Re-run tests
3. Investigate root cause before re-attempting

---

## 8. Error Response Format

All error responses should follow the existing `ApiError` format:

```json
{
  "error": {
    "code": "BAD_REQUEST",
    "message": "workspace_id is required",
    "details": null,
    "trace_id": "abc123"
  }
}
```

---

## 9. Logging Requirements

Each error path must log:

1. **Error level**: `error!` for 5xx, `warn!` for 4xx
2. **Context**: What operation failed
3. **Cause**: The underlying error (if any)
4. **Request context**: trace_id, relevant parameters (not sensitive data)

Example:
```rust
tracing::error!(
    error = %e,
    trace_id = %ctx.trace_id,
    workspace_id = ?workspace_id,
    "Database transaction failed during upload"
);
```

---

## 10. Success Metrics

| Metric | Before | After | Target |
|--------|--------|-------|--------|
| Critical path `.unwrap()` | 31 | 0 | 0 |
| 500 errors from panics | Unknown | Measurable | < 0.01% |
| Structured error responses | Partial | 100% | 100% |

---

## 11. Future Work (Phase 2+)

After Phase 1:
- Phase 2: Medium-risk paths (~50 instances)
- Phase 3: Low-risk utilities (~100 instances)
- Phase 4: Audit remaining ~600 instances (mostly test code)

Consider adopting `parking_lot::RwLock` which doesn't poison on panic, eliminating an entire class of `.unwrap()` calls on lock acquisition.
