# Upload Queue and Worker Pool Implementation

**Date:** 2025-01-19
**Agent:** Agent 11 of 15 (PRD-02 Fixes)
**Purpose:** Non-blocking concurrent upload processing with fair tenant scheduling

## Overview

The upload queue and worker pool system decouples upload processing from HTTP request handling, enabling:

- **Concurrent Processing:** Multiple uploads processed simultaneously
- **Fair Scheduling:** Equal service to all tenants across batches
- **Priority Support:** High-priority uploads processed first within tenant queues
- **Queue Size Limits:** Prevents unbounded memory growth
- **Comprehensive Metrics:** Queue depth, processing times, per-tenant statistics
- **Graceful Degradation:** Handles worker failures with automatic retry logic

## Architecture

### Core Components

#### 1. UploadQueue (src/upload_queue.rs)

Central queue management with per-tenant scheduling:

```rust
pub struct UploadQueue {
    // Per-tenant queues for fair scheduling
    tenant_queues: Arc<RwLock<HashMap<String, TenantQueueState>>>,

    // Global item lookup
    all_items: Arc<RwLock<HashMap<String, UploadQueueItem>>>,

    // Worker communication
    worker_tx: mpsc::UnboundedSender<UploadQueueItem>,

    // Metrics tracking
    max_queue_depth: Arc<AtomicU64>,
    total_processed: Arc<AtomicU64>,
    total_failed: Arc<AtomicU64>,
}
```

**Key Responsibilities:**
- Manage upload queues per tenant
- Maintain priority ordering within tenant queues
- Implement fair tenant scheduling (round-robin with recency bias)
- Track queue metrics (depth, processing time, failures)
- Enforce queue size limits

#### 2. UploadQueueItem

Upload request metadata:

```rust
pub struct UploadQueueItem {
    pub id: String,                      // Unique item ID
    pub tenant_id: String,               // For fair scheduling
    pub request_data: Vec<u8>,           // Upload payload
    pub enqueued_at: u64,                // Enqueue timestamp
    pub priority: u8,                    // 0-255 (higher = more important)
    pub retries_remaining: u8,           // Retry counter
    pub last_error: Option<String>,      // Error on last attempt
}
```

#### 3. UploadWorkerPool

Worker task coordination:

```rust
pub struct UploadWorkerPool {
    queue: Arc<UploadQueue>,
    rx: mpsc::UnboundedReceiver<UploadQueueItem>,
    worker_count: usize,
    timeout_secs: u64,
}
```

**Responsibilities:**
- Spawn N worker tasks
- Consume items from queue
- Coordinate with actual upload handlers
- Handle timeouts and failures

#### 4. UploadQueueConfig

Runtime configuration:

```rust
pub struct UploadQueueConfig {
    pub max_queue_size: usize,           // Default: 10,000
    pub worker_count: usize,             // Default: 4
    pub max_retries: u8,                 // Default: 3
    pub retry_backoff_ms: u64,           // Default: 100ms
    pub upload_timeout_secs: u64,        // Default: 300s
    pub cleanup_interval_secs: u64,      // Default: 300s
}
```

### Integration Points

#### 1. AppState Integration (src/state.rs)

```rust
pub struct AppState {
    // ... existing fields ...

    // Upload queue with worker pool
    pub upload_queue: Arc<UploadQueue>,
}
```

Initialized in `AppState::new()`:
```rust
upload_queue: Arc::new(UploadQueue::new(UploadQueueConfig::default())),
```

#### 2. Handler Integration (src/handlers/upload_queue_integration.rs)

Three REST endpoints for queue operations:

**POST /v1/uploads/queue** - Queue an upload
```rust
pub async fn queue_upload(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<QueueUploadRequest>,
) -> Result<(StatusCode, Json<UploadQueueResult>), (StatusCode, String)>
```

Returns `HTTP 202 Accepted` with queue metadata:
```json
{
    "item_id": "upload_550e8400-e29b-41d4-a716-446655440000",
    "status": "queued",
    "queue_depth": 5,
    "queue_position": 2,
    "time_in_queue": 0,
    "processing_time": null
}
```

**GET /v1/uploads/{item_id}/status** - Check upload status

Returns current position and processing status:
```json
{
    "item_id": "upload_...",
    "status": "queued|processing|completed|failed",
    "queue_depth": 5,
    "queue_position": 2,
    "time_in_queue": 45,
    "processing_time": null
}
```

**GET /v1/uploads/queue/metrics** - Retrieve queue metrics

Returns aggregate statistics:
```json
{
    "queue_depth": 5,
    "max_queue_depth": 127,
    "total_processed": 1523,
    "total_failed": 12,
    "avg_processing_time_ms": 250.5,
    "per_tenant_depths": {
        "tenant-a": 2,
        "tenant-b": 3,
        "tenant-c": 0
    }
}
```

## Fair Scheduling Algorithm

### Per-Tenant Queue Model

Each tenant has a separate queue:
```
Tenant A:  [High(200), Low(50), Med(100)]  <- sorted by priority DESC
Tenant B:  [High(190), VeryHigh(250)]
Tenant C:  [Med(128)]
```

### Scheduling Strategy

1. **Find next tenant:** Select tenant with:
   - Non-empty queue
   - Oldest `last_processed_at` timestamp
   - Tiebreak: Highest priority item in queue

2. **Process item:** Worker gets first item from selected tenant

3. **Update state:**
   - Set `tenant.last_processed_at = now()`
   - Move to next tenant on next iteration

4. **Example sequence (with 1 worker):**
   ```
   Time 0: Process Tenant A (prio 200) → last_processed_at[A] = 0
   Time 1: Process Tenant B (prio 250) → last_processed_at[B] = 1
   Time 2: Process Tenant C (prio 128) → last_processed_at[C] = 2
   Time 3: Process Tenant A (prio 50)  → last_processed_at[A] = 3  [A oldest]
   Time 4: Process Tenant B (prio 190) → last_processed_at[B] = 4  [B oldest]
   ```

**Benefits:**
- No tenant starves (guaranteed processing within N tenants)
- Within-tenant priority respected
- Fair under concurrent enqueue/dequeue
- O(log N) complexity per operation

## Queue Size Limits

### Enforcement

Maximum queue size (default 10,000 items) prevents unbounded growth:

```rust
// In enqueue_with_priority()
let total_depth: usize = tenant_queues
    .values()
    .map(|q| q.items.len())
    .sum();

if total_depth >= self.config.max_queue_size {
    return Err("Upload queue full");
}
```

### Behavior Under Load

- Clients get `429 Too Many Requests` when queue full
- Telemetry tracks max depth reached (`max_queue_depth` metric)
- Admin can increase `max_queue_size` in config

## Metrics Tracking

### Metrics Structure

```rust
pub struct UploadQueueMetrics {
    pub queue_depth: usize,                    // Current items
    pub max_queue_depth: u64,                  // High water mark
    pub total_processed: u64,                  // Successful items
    pub total_failed: u64,                     // Failed items
    pub avg_processing_time_ms: f64,           // Mean time
    pub total_processing_time_ms: u64,         // Cumulative time
    pub per_tenant_depths: HashMap<String, usize>,  // Per-tenant breakdown
}
```

### Usage

```rust
let metrics = state.upload_queue.get_metrics().await;
println!("Queue depth: {}", metrics.queue_depth);
println!("Max depth: {}", metrics.max_queue_depth);
println!("Total processed: {}", metrics.total_processed);
println!("Tenant A depth: {}", metrics.per_tenant_depths.get("tenant-a"));
```

## Error Handling and Retries

### Retry Logic

Configured in `UploadQueueConfig`:

```rust
pub max_retries: u8,            // Max retry attempts (default: 3)
pub retry_backoff_ms: u64,      // Backoff between retries (default: 100ms)
```

**Example backoff schedule:**
- Attempt 1: Immediate
- Attempt 2: 100ms delay
- Attempt 3: 200ms delay
- Attempt 4: 300ms delay
- If all fail: Item marked as failed, stored in audit log

### Error Recording

Failed uploads are:
1. Logged with detailed error context
2. Tracked in metrics (`total_failed`)
3. Recorded in audit logs
4. Optionally retried with exponential backoff

## Performance Characteristics

### Complexity Analysis

| Operation | Complexity | Notes |
|-----------|-----------|-------|
| Enqueue | O(log N) | Priority search within tenant queue |
| Get Next | O(T) | Linear scan of tenant count |
| Status Lookup | O(1) | Atomic hash lookup |
| Get Metrics | O(T) | Iterate tenant counts |

Where:
- N = items in queue
- T = number of active tenants (typically << N)

### Memory Overhead

Per item: ~256 bytes (ID, tenant_id, priority, timestamps)
Per 10,000 items: ~2.5 MB base + request payload

## Testing

Comprehensive test suite in `tests/upload_queue_test.rs`:

### Tests Covered

1. **Basic Operations**
   - `test_upload_queue_enqueue_single_item` - Single item queueing
   - `test_upload_queue_enqueue_with_priority` - Priority support
   - `test_upload_queue_get_status` - Status retrieval

2. **Queue Size Limits**
   - `test_upload_queue_respects_max_size` - Capacity enforcement
   - `test_upload_queue_size_limit_concurrent` - Concurrent limit checking

3. **Priority & Scheduling**
   - `test_upload_queue_priority_ordering` - Priority ordering
   - `test_upload_queue_fair_scheduling` - Fair tenant scheduling
   - `test_upload_queue_multi_tenant_priority` - Multi-tenant priorities

4. **Metrics**
   - `test_upload_queue_metrics` - Metrics tracking
   - `test_upload_queue_per_tenant_metrics` - Per-tenant breakdown
   - `test_upload_queue_max_depth_tracking` - Max depth high water mark

5. **Concurrency**
   - `test_upload_queue_concurrent_enqueue` - 100 concurrent enqueues
   - `test_upload_queue_status_consistency` - Status after concurrent ops

### Running Tests

```bash
# Run all upload queue tests
cargo test -p adapteros-server-api --test upload_queue_test

# Run specific test
cargo test -p adapteros-server-api --test upload_queue_test test_upload_queue_concurrent_enqueue

# Run with output
cargo test -p adapteros-server-api --test upload_queue_test -- --nocapture
```

## Configuration Guide

### Default Configuration

```rust
UploadQueueConfig {
    max_queue_size: 10000,          // 10K items max
    worker_count: 4,                // 4 concurrent workers
    max_retries: 3,                 // Retry up to 3 times
    retry_backoff_ms: 100,          // 100ms base backoff
    upload_timeout_secs: 300,       // 5 minute timeout
    cleanup_interval_secs: 300,     // Cleanup every 5 min
}
```

### Customization

Create custom config:

```rust
let config = UploadQueueConfig {
    max_queue_size: 50000,
    worker_count: 8,
    max_retries: 5,
    retry_backoff_ms: 200,
    upload_timeout_secs: 600,
    cleanup_interval_secs: 600,
};

let queue = UploadQueue::new(config);
```

## Future Enhancements

1. **Dead Letter Queue (DLQ)** - Failed items moved to separate queue
2. **Priority Escalation** - Increase priority over time
3. **Queue Persistence** - Survive server restart
4. **Worker Auto-scaling** - Dynamic worker count based on queue depth
5. **Rate Limiting Per Tenant** - Prevent single tenant overload
6. **Circuit Breaker** - Temporarily reject uploads if backend fails
7. **Latency SLA Tracking** - Alert if p99 latency exceeds threshold

## Integration Checklist

When integrating with existing upload handlers:

- [ ] Add upload queue initialization to AppState
- [ ] Create queue integration handler module
- [ ] Update routes.rs to include new endpoints
- [ ] Add handler modules to handlers directory
- [ ] Integrate with actual upload processing in handlers
- [ ] Add worker pool startup to server initialization
- [ ] Configure queue via environment variables
- [ ] Add prometheus metrics export
- [ ] Test with concurrent upload scenarios
- [ ] Add rate limiting per tenant
- [ ] Document in API specification (OpenAPI/Swagger)

## Troubleshooting

### Queue Always Full

**Symptom:** Clients always get 429 Too Many Requests

**Causes:**
- Worker pool not processing items (workers crashed?)
- Upload backend slow or failing
- Queue size too small for workload

**Solution:**
1. Check worker logs for errors
2. Monitor backend service health
3. Increase `max_queue_size` in config
4. Increase `worker_count` for more parallelism

### High Latency in Queue

**Symptom:** Items stay in queue > expected time

**Causes:**
- Few workers vs many items
- Some uploads very slow
- Network issues to backend

**Solution:**
1. Increase `worker_count`
2. Increase `upload_timeout_secs`
3. Check network/backend metrics
4. Review `avg_processing_time_ms` in metrics

### Memory Growth

**Symptom:** Server memory grows over time

**Causes:**
- Failed items not being removed
- Too many concurrent uploads
- Request data not cleared

**Solution:**
1. Monitor `per_tenant_depths` - should stabilize
2. Reduce `max_queue_size`
3. Reduce `worker_count` to process slower
4. Check for memory leaks in handlers

## Files Modified/Created

### New Files

1. **src/upload_queue.rs** - Queue and worker pool implementation
2. **src/handlers/upload_queue_integration.rs** - REST endpoints
3. **tests/upload_queue_test.rs** - Comprehensive test suite

### Modified Files

1. **src/lib.rs** - Added `pub mod upload_queue`
2. **src/state.rs** - Added `upload_queue: Arc<UploadQueue>` field
3. **src/handlers/mod.rs** - Added upload queue integration module (if using explicit mod.rs)

## References

- [ARCHITECTURE_INDEX.md](ARCHITECTURE_INDEX.md) - System architecture
- [DATABASE_REFERENCE.md](DATABASE_REFERENCE.md) - Database schema
- [RBAC.md](RBAC.md) - Permission requirements
