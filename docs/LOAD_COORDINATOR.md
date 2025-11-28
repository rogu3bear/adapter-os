# LoadCoordinator - Thundering Herd Protection

**Location:** `crates/adapteros-server-api/src/load_coordinator.rs`
**Version:** v0.3-alpha
**Status:** Production-ready

---

## Overview

The `LoadCoordinator` prevents "thundering herd" problems when multiple concurrent requests arrive for adapters that aren't loaded yet. Only the first request triggers the actual load operation; subsequent requests wait for the result.

## Problem Statement

Without coordination:
```rust
// ❌ BAD: Each request loads independently
for request in concurrent_requests {
    let adapter = loader.load_adapter("my-adapter").await?; // Multiple loads!
}
```

With LoadCoordinator:
```rust
// ✓ GOOD: Only first request loads, others wait
for request in concurrent_requests {
    let adapter = coordinator.load_or_wait("my-adapter", || async {
        loader.load_adapter("my-adapter").await
    }).await?; // Single load, multiple waiters
}
```

---

## Architecture

### Core Components

```rust
pub struct LoadCoordinator {
    pending_loads: DashMap<String, Arc<LoadWaiter>>,
}

struct LoadWaiter {
    notify: Notify,              // Wake all waiters
    result: OnceCell<Result>,    // Set exactly once
    waiter_count: AtomicUsize,   // Concurrent requests
    first_request_at: Instant,   // For metrics
}
```

### Concurrency Primitives

| Primitive | Purpose | Properties |
|-----------|---------|------------|
| `DashMap` | Lock-free concurrent map | No global lock, per-shard locking |
| `OnceCell` | One-time result storage | Thread-safe, set exactly once |
| `Notify` | Wake multiple waiters | Efficient broadcast notification |
| `AtomicUsize` | Waiter counting | Lock-free atomic operations |

---

## API Reference

### Core Methods

#### `load_or_wait`

Load model or wait for in-progress load.

```rust
pub async fn load_or_wait<F, Fut>(
    &self,
    model_id: &str,
    load_fn: F,
) -> Result<AdapterHandle, AosError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<AdapterHandle, AosError>>
```

**Behavior:**
- First caller: Executes `load_fn` and stores result
- Subsequent callers: Wait for first caller's result
- All callers: Receive the same result (success or error)

**Example:**
```rust
let coordinator = LoadCoordinator::new();

// 10 concurrent requests, only 1 load
let handle = coordinator.load_or_wait("my-adapter", || async {
    loader.load_adapter(42, "my-adapter").await
}).await?;
```

#### `is_loading`

Check if a model is currently being loaded.

```rust
pub fn is_loading(&self, model_id: &str) -> bool
```

**Example:**
```rust
if coordinator.is_loading("my-adapter") {
    println!("Load in progress, will wait...");
}
```

#### `waiter_count`

Get number of requests waiting for a model.

```rust
pub fn waiter_count(&self, model_id: &str) -> usize
```

**Example:**
```rust
let count = coordinator.waiter_count("my-adapter");
println!("{} requests waiting", count);
```

#### `cancel`

Cancel a pending load (removes from pending map).

```rust
pub fn cancel(&self, model_id: &str)
```

**Note:** Does not stop in-progress load, but prevents new waiters.

#### `metrics`

Get coordinator metrics for monitoring.

```rust
pub fn metrics(&self) -> LoadCoordinatorMetrics

pub struct LoadCoordinatorMetrics {
    pub pending_loads: usize,
    pub total_waiters: usize,
    pub oldest_load_age_ms: u128,
}
```

---

## Usage Patterns

### Basic Integration

```rust
use adapteros_server_api::LoadCoordinator;

// In application state
pub struct AppState {
    load_coordinator: Arc<LoadCoordinator>,
    adapter_loader: Arc<AdapterLoader>,
}

// In request handler
async fn handle_inference(
    state: Arc<AppState>,
    adapter_id: String,
) -> Result<Response> {
    let handle = state.load_coordinator
        .load_or_wait(&adapter_id, || async {
            state.adapter_loader.load_adapter(42, &adapter_id).await
        })
        .await?;

    // Use adapter handle...
}
```

### With Lifecycle Manager

```rust
use adapteros_lora_lifecycle::LifecycleManager;

async fn ensure_adapter_loaded(
    coordinator: &LoadCoordinator,
    lifecycle: &LifecycleManager,
    adapter_id: &str,
) -> Result<AdapterHandle> {
    coordinator.load_or_wait(adapter_id, || async {
        lifecycle.transition_to_warm(adapter_id).await?;
        lifecycle.get_handle(adapter_id).await
    }).await
}
```

### Error Handling

```rust
match coordinator.load_or_wait("my-adapter", load_fn).await {
    Ok(handle) => {
        // All waiters receive the handle
        info!("Adapter loaded: {:?}", handle.path);
    }
    Err(AosError::Lifecycle(msg)) => {
        // All waiters receive the same error
        warn!("Load failed: {}", msg);
    }
    Err(e) => {
        error!("Unexpected error: {}", e);
    }
}
```

### Monitoring

```rust
// Periodic metrics collection
tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(10));
    loop {
        interval.tick().await;
        let metrics = coordinator.metrics();

        info!(
            pending_loads = metrics.pending_loads,
            total_waiters = metrics.total_waiters,
            oldest_load_ms = metrics.oldest_load_age_ms,
            "LoadCoordinator metrics"
        );

        if metrics.oldest_load_age_ms > 30_000 {
            warn!("Load taking longer than 30s!");
        }
    }
});
```

---

## Logging

### Log Levels

**DEBUG:**
- First request starting load
- Load completed with no waiters

**INFO:**
- Load completed with waiters (includes count, time)
- Waiter completed waiting (includes wait time)

**WARN:**
- Result set by another task (race condition)
- Load cancelled with active waiters

### Example Output

```
DEBUG load_coordinator: First request for adapter, triggering load model_id="code-assistant"
INFO  load_coordinator: Load completed with 9 waiting requests model_id="code-assistant" waiters=9 load_time_ms=347 success=true
INFO  load_coordinator: Load wait completed model_id="code-assistant" wait_time_ms=123 success=true
```

---

## Performance Characteristics

### Time Complexity

| Operation | Complexity | Notes |
|-----------|-----------|-------|
| `load_or_wait` (first) | O(1) + O(load) | DashMap insert + load operation |
| `load_or_wait` (waiter) | O(1) + O(wait) | DashMap lookup + async wait |
| `is_loading` | O(1) | DashMap contains check |
| `waiter_count` | O(1) | Atomic load |
| `metrics` | O(n) | Iterate all pending loads |

### Space Complexity

- Per adapter: ~96 bytes (LoadWaiter struct)
- Per coordinator: O(concurrent loads)
- Cleanup: Automatic on load completion

### Benchmarks

```
Single request:          ~10µs overhead
10 concurrent requests:  ~15µs overhead (9 saved loads)
100 concurrent requests: ~50µs overhead (99 saved loads)
```

---

## Testing

### Unit Tests

Run tests:
```bash
cargo test -p adapteros-server-api load_coordinator
```

Test coverage:
- ✓ Single request (no coordination needed)
- ✓ Concurrent requests coalesce
- ✓ Error propagation to all waiters
- ✓ Sequential loads (no interference)
- ✓ Waiter count accuracy
- ✓ Metrics reporting
- ✓ Cancel functionality

### Integration Example

```bash
cargo run --example load_coordinator_demo -p adapteros-server-api
```

Expected output:
```
=== LoadCoordinator Demo ===

Spawning 10 concurrent requests for 'test-adapter'...

Request 0: Starting
Request 1: Starting
...
  >>> Performing actual load (expensive operation)...
Request 0: Success (adapter_id=42, memory=1MB)
...

=== Results ===
Total load operations performed: 1

✓ SUCCESS: Only 1 load performed despite 10 concurrent requests!
```

---

## Edge Cases

### Race Conditions

**Scenario:** Multiple tasks call `load_or_wait` simultaneously

**Resolution:** DashMap's `entry().or_insert_with()` ensures atomic insertion. First task wins, others wait.

### Load Failure

**Scenario:** Load operation fails

**Behavior:** All waiters receive the error. Next request will retry (fresh load).

### Cancellation

**Scenario:** `cancel()` called during load

**Behavior:** Entry removed from map, but in-flight load continues. Existing waiters still receive result.

### Memory Leak Prevention

**Scenario:** Coordinator holds entries forever

**Prevention:** Entries automatically removed when load completes (success or failure).

---

## Integration Points

### AppState

```rust
pub struct AppState {
    pub load_coordinator: Arc<LoadCoordinator>,
    // ... other fields
}

impl AppState {
    pub fn new() -> Self {
        Self {
            load_coordinator: Arc::new(LoadCoordinator::new()),
            // ...
        }
    }
}
```

### Request Handlers

```rust
async fn load_adapter_handler(
    State(state): State<Arc<AppState>>,
    Path(adapter_id): Path<String>,
) -> Result<Json<AdapterInfo>> {
    let handle = state.load_coordinator
        .load_or_wait(&adapter_id, || {
            load_adapter_impl(&state, &adapter_id)
        })
        .await?;

    Ok(Json(AdapterInfo::from(handle)))
}
```

---

## Comparison to Alternatives

### vs. Mutex<HashMap>

| LoadCoordinator | Mutex<HashMap> |
|-----------------|----------------|
| Lock-free reads | Global lock |
| Per-shard locking | Contention on writes |
| Async-native | Blocking lock |

### vs. Manual async::Mutex

| LoadCoordinator | async::Mutex |
|-----------------|--------------|
| Efficient broadcast | Manual notification |
| Automatic cleanup | Manual cleanup |
| Built-in metrics | Manual instrumentation |

---

## Future Enhancements

### Planned Features

1. **TTL Support:** Expire cached loads after timeout
2. **Load Prioritization:** Priority queue for pending loads
3. **Backpressure:** Limit concurrent loads
4. **Tracing:** OpenTelemetry spans

### API Extensions

```rust
// Future API (not yet implemented)
impl LoadCoordinator {
    pub fn load_or_wait_with_timeout(
        &self,
        model_id: &str,
        timeout: Duration,
        load_fn: F,
    ) -> Result<AdapterHandle>;

    pub fn load_or_wait_with_priority(
        &self,
        model_id: &str,
        priority: u8,
        load_fn: F,
    ) -> Result<AdapterHandle>;
}
```

---

## Related Documentation

- [ARCHITECTURE_PATTERNS.md](ARCHITECTURE_PATTERNS.md) - Concurrency patterns
- [LIFECYCLE.md](LIFECYCLE.md) - Adapter state machine
- [COREML_INTEGRATION.md](COREML_INTEGRATION.md) - Backend loading
- [MLX_INTEGRATION.md](MLX_INTEGRATION.md) - MLX backend

---

## References

**Source Code:**
- Implementation: `crates/adapteros-server-api/src/load_coordinator.rs`
- Tests: `crates/adapteros-server-api/tests/load_coordinator_standalone.rs`
- Example: `crates/adapteros-server-api/examples/load_coordinator_demo.rs`

**Dependencies:**
- [DashMap](https://docs.rs/dashmap) - Concurrent hash map
- [tokio::sync::Notify](https://docs.rs/tokio/latest/tokio/sync/struct.Notify.html) - Async notification
- [tokio::sync::OnceCell](https://docs.rs/tokio/latest/tokio/sync/struct.OnceCell.html) - One-time initialization

---

**Author:** AdapterOS Team
**Last Updated:** 2025-11-27
**Status:** Production-ready
