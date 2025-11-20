# Upload Queue Integration Guide

**Target:** Integrate concurrent upload queue into existing `upload_aos_adapter` handler

## Current State (Before Integration)

The current implementation (`src/handlers/aos_upload.rs`) processes uploads synchronously:

```rust
pub async fn upload_aos_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // 1. Parse multipart and stream file to disk
    // 2. Validate and hash
    // 3. Database registration
    // 4. Return response
    // ⚠️ Client blocks for entire duration (large files = long wait)
}
```

**Problem:** Each upload blocks on disk I/O and database operations

## Proposed Solution: Queueing Layer

### Option A: Lightweight Queue (Recommended)

Queue the upload and return immediately, process asynchronously:

```
Client Upload Request
    ↓
[Rate Limit Check]
    ↓
[Multipart Streaming]
    ↓
[Enqueue to Queue]  ← Return 202 Accepted immediately
    ↓
[Worker Pool]       ← Process asynchronously
    ├─ Validate
    ├─ Hash
    ├─ Database register
    └─ Error handling
```

**Benefits:**
- Client gets response in milliseconds
- Can process many uploads concurrently
- Worker pool handles resource management
- Graceful degradation under load (queue fills, return 429)

### Option B: Hybrid Queue (For Large Files)

Queue only files > threshold, process inline for small files:

```
Upload < 100MB → Process inline (backward compatible)
Upload > 100MB → Queue for async processing
```

**Benefits:**
- Simple uploads still fast
- Large uploads don't block
- Compatibility with existing clients

## Implementation Steps

### Step 1: Extract Upload Work into Worker Function

Create `src/handlers/upload_work.rs`:

```rust
pub struct UploadWork {
    pub tenant_id: String,
    pub request_data: Vec<u8>,
    pub adapter_name: String,
    pub tier: String,
    pub category: String,
    pub scope: String,
    pub rank: i32,
    pub alpha: f64,
}

pub async fn execute_upload_work(
    state: &AppState,
    work: UploadWork,
) -> Result<AosUploadResponse, String> {
    // Extract from upload_aos_adapter:
    // 1. File streaming and hashing
    // 2. Database registration
    // 3. Response building

    Ok(AosUploadResponse {
        adapter_id: "...".into(),
        tenant_id: work.tenant_id,
        hash_b3: "...".into(),
        file_path: "...".into(),
        file_size: 0,
        lifecycle_state: "draft".into(),
        created_at: chrono::Utc::now().to_rfc3339(),
    })
}
```

### Step 2: Modify Upload Handler to Queue Work

Update `src/handlers/aos_upload.rs`:

```rust
pub async fn upload_aos_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Permission check (keep as-is)
    if let Err(e) = require_permission(&claims, Permission::AdapterRegister) {
        return Err((StatusCode::FORBIDDEN, e.to_string()));
    }

    let tenant_id = claims.tenant_id.clone()
        .ok_or((StatusCode::BAD_REQUEST, "Missing tenant_id".into()))?;

    // Rate limiting (keep as-is)
    let (allowed, remaining, reset_at) = state.upload_rate_limiter
        .check_rate_limit(&tenant_id).await;
    if !allowed {
        return Err((StatusCode::TOO_MANY_REQUESTS, "Rate limit exceeded".into()));
    }

    // Parse multipart (keep as-is)
    let mut file_data = Vec::new();
    let mut adapter_name = String::new();
    let mut tier = "ephemeral".to_string();
    let mut category = "general".to_string();
    let mut scope = "general".to_string();
    let mut rank = 1;
    let mut alpha = 1.0;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        (StatusCode::BAD_REQUEST, format!("Invalid multipart: {}", e))
    })? {
        // Parse fields (same as before)
        // ...
    }

    // Queue the work instead of executing inline
    let work = UploadWork {
        tenant_id: tenant_id.clone(),
        request_data: file_data,
        adapter_name,
        tier,
        category,
        scope,
        rank,
        alpha,
    };

    let result = state.upload_queue
        .enqueue(tenant_id.clone(), serde_json::to_vec(&work).unwrap())
        .await
        .map_err(|e| (StatusCode::TOO_MANY_REQUESTS, e))?;

    // Return 202 Accepted with queue info
    let response = json!({
        "item_id": result.item_id,
        "status": "queued",
        "queue_depth": result.queue_depth,
        "queue_position": result.queue_position,
        "message": "Upload queued for processing"
    });

    Ok((StatusCode::ACCEPTED, Json(response)))
}
```

### Step 3: Spawn Worker Pool at Startup

In `src/main.rs` or server initialization:

```rust
// After AppState creation
let queue = state.upload_queue.clone();
let rx = queue.take_receiver().await
    .expect("Could not take queue receiver");

let worker_pool = UploadWorkerPool::new(queue, rx);

tokio::spawn(async move {
    worker_pool.run().await;
});
```

### Step 4: Add Worker Task Processing

Create `src/workers/upload_worker.rs`:

```rust
use tokio::task;

pub async fn process_upload_queue(
    state: AppState,
    mut queue_rx: mpsc::UnboundedReceiver<UploadQueueItem>,
) {
    while let Some(item) = queue_rx.recv().await {
        let state = state.clone();

        // Spawn worker task
        task::spawn(async move {
            match serde_json::from_slice::<UploadWork>(&item.request_data) {
                Ok(work) => {
                    match execute_upload_work(&state, work).await {
                        Ok(response) => {
                            info!(
                                item_id = %item.id,
                                adapter_id = %response.adapter_id,
                                "Upload completed successfully"
                            );
                            // Could store response in cache for status checks
                        }
                        Err(e) => {
                            warn!(
                                item_id = %item.id,
                                error = %e,
                                "Upload failed"
                            );
                            // Retry logic here
                        }
                    }
                }
                Err(e) => {
                    error!(
                        item_id = %item.id,
                        error = %e,
                        "Failed to deserialize upload work"
                    );
                }
            }
        });
    }
}
```

## API Changes

### Request

**POST /v1/adapters/upload-aos** (unchanged)

Accepts multipart form with .aos file and metadata.

### Response

**Before (Synchronous):**
```
HTTP 200 OK
{
    "adapter_id": "adapter_550e8400...",
    "hash_b3": "abc123...",
    "file_path": "./adapters/adapter_550e8400....aos",
    "file_size": 4194304,
    "lifecycle_state": "draft",
    "created_at": "2025-01-19T12:00:00Z"
}
```

**After (Asynchronous):**
```
HTTP 202 Accepted
{
    "item_id": "upload_550e8400...",
    "status": "queued",
    "queue_depth": 5,
    "queue_position": 2,
    "message": "Upload queued for processing"
}
```

### New Endpoints

**Check upload status:**
```
GET /v1/uploads/{item_id}/status
HTTP 200 OK
{
    "item_id": "upload_550e8400...",
    "status": "processing",
    "queue_depth": 4,
    "queue_position": null,
    "time_in_queue": 30,
    "processing_time": 5
}
```

**Get queue metrics:**
```
GET /v1/uploads/queue/metrics
HTTP 200 OK
{
    "queue_depth": 4,
    "max_queue_depth": 127,
    "total_processed": 1523,
    "total_failed": 12,
    "avg_processing_time_ms": 250.5,
    "per_tenant_depths": {
        "tenant-a": 2,
        "tenant-b": 2
    }
}
```

## Client Migration

### Old Workflow

```javascript
// Upload and wait for completion
POST /v1/adapters/upload-aos [multipart file]
// Blocks until upload complete
Response: 200 OK { adapter_id, hash_b3, ... }
```

### New Workflow

```javascript
// Step 1: Queue upload (returns immediately)
POST /v1/adapters/upload-aos [multipart file]
// Returns in milliseconds
Response: 202 Accepted { item_id, queue_position, ... }

// Step 2: Poll for completion
GET /v1/uploads/{item_id}/status
Response: { status: "queued", queue_position: 2 }
// Wait...
GET /v1/uploads/{item_id}/status
Response: { status: "processing", time_in_queue: 30 }
// Wait...
GET /v1/uploads/{item_id}/status
Response: { status: "completed", adapter_id, hash_b3, ... }
```

### WebSocket Alternative (Optional)

For real-time progress, add WebSocket endpoint:

```rust
pub async fn upload_progress_stream(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse> {
    ws.on_upgrade(|mut socket| async move {
        // Send updates as upload progresses
        while let Some(update) = get_next_update(&item_id).await {
            socket.send(Message::Text(
                serde_json::to_string(&update).unwrap()
            )).await.ok();
        }
    })
}
```

## Backward Compatibility

### Option: Maintain Synchronous Fallback

For clients needing synchronous behavior, add header flag:

```rust
pub async fn upload_aos_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let sync_mode = headers
        .get("X-Upload-Mode")
        .and_then(|h| h.to_str().ok())
        .map(|s| s == "sync")
        .unwrap_or(false);

    if sync_mode {
        // Execute synchronously (keep old behavior)
        execute_upload_work(...).await
    } else {
        // Queue for async processing (new behavior)
        state.upload_queue.enqueue(...).await
    }
}
```

## Testing Strategy

### Unit Tests

- Queue enqueue/dequeue
- Priority ordering
- Fair scheduling
- Metrics tracking

### Integration Tests

- End-to-end upload via HTTP
- Status polling
- Concurrent uploads
- Queue limit enforcement

### Load Tests

```bash
# Simulate 100 concurrent uploads
ab -n 100 -c 10 \
  -p multipart_data.txt \
  -T multipart/form-data \
  http://localhost:8080/v1/adapters/upload-aos
```

## Rollout Plan

### Phase 1: Staging (Week 1)
- Deploy with feature flag disabled
- Test with internal uploads
- Verify queue metrics

### Phase 2: Gradual Rollout (Week 2-3)
- Enable for 10% of traffic
- Monitor metrics
- Gather feedback

### Phase 3: Full Rollout (Week 4)
- Enable for all tenants
- Keep synchronous fallback available
- Remove fallback after 1 month

## Monitoring & Alerts

### Key Metrics

```
# Queue depth (should stay < max_queue_size)
aos_upload_queue_depth

# Processing time (should stay < p95 SLA)
aos_upload_queue_processing_time_p95

# Success rate (should stay > 99.5%)
aos_upload_queue_success_rate

# Worker utilization (should stay < 80%)
aos_upload_workers_utilization
```

### Alert Rules

```yaml
- alert: UploadQueueFull
  expr: aos_upload_queue_depth > 0.9 * max_queue_size

- alert: HighProcessingTime
  expr: aos_upload_queue_processing_time_p95 > 60s

- alert: HighFailureRate
  expr: aos_upload_queue_success_rate < 0.95
```

## Troubleshooting During Integration

### Issue: Rate Limiter Too Strict

**Symptom:** Uploads immediately queued, queue fills quickly

**Solution:** Increase `UploadRateLimiter` burst capacity:
```rust
// In AppState::new()
upload_rate_limiter: Arc::new(UploadRateLimiter::new(
    20,  // Increased from 10 uploads/min
    20,  // Increased from 5 burst
)),
```

### Issue: Workers Slow

**Symptom:** Queue depth grows, processing time high

**Solution:** Increase worker count:
```rust
let config = UploadQueueConfig {
    worker_count: 8,  // Increased from 4
    ..Default::default()
};
```

### Issue: Memory Growth

**Symptom:** Server memory increases over time

**Solution:** Reduce queue size or process faster:
```rust
let config = UploadQueueConfig {
    max_queue_size: 5000,  // Reduced from 10000
    ..Default::default()
};
```

## Performance Expectations

### Before Integration (Synchronous)

```
Concurrent Clients: 4
Upload Size: 100MB
Total Time: 4 × 30s = 120s
Throughput: 1 upload per 30s
```

### After Integration (Asynchronous)

```
Concurrent Clients: 100 (queue can hold 10,000)
Upload Size: 100MB
Queue Return Time: 10ms
Processing Time: 30s (4 workers in parallel)
Throughput: 4 uploads per 30s (8x improvement)
```

## Files to Create/Modify

```
Created:
  src/upload_queue.rs
  src/handlers/upload_queue_integration.rs
  src/handlers/upload_work.rs
  src/workers/upload_worker.rs
  tests/upload_queue_test.rs

Modified:
  src/lib.rs                    (add modules)
  src/state.rs                  (add upload_queue field)
  src/handlers/aos_upload.rs    (queue instead of sync)
  src/main.rs                   (spawn worker pool)
  src/routes.rs                 (add new endpoints)
```

## References

- [UPLOAD_QUEUE_AND_WORKER_POOL.md](UPLOAD_QUEUE_AND_WORKER_POOL.md) - Full system docs
- [ARCHITECTURE_PATTERNS.md](ARCHITECTURE_PATTERNS.md) - Architecture guidance
- [RBAC.md](RBAC.md) - Permission requirements
