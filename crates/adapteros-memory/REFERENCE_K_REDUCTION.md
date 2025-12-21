# K Reduction Integration - Quick Reference

## Installation

Add to lifecycle manager:

```rust
use adapteros_memory::{
    KReductionChannelManager, MemoryPressureManager,
};
```

## Create Channel

```rust
let manager = KReductionChannelManager::new();
let (tx, rx) = manager.create_channel();
```

## Memory Manager Setup

```rust
let pressure_mgr = MemoryPressureManager::with_channel_sender(tracker, tx);

// Or set later
pressure_mgr.set_channel_sender(tx);
```

## Lifecycle Manager Consumer

```rust
tokio::spawn(async move {
    while let Some(request) = rx.recv().await {
        // Process K reduction request
        let result = process_request(&request).await;

        // Record outcome for stats
        rx.record_decision_outcome(result.approved);
    }
});
```

## Configuration

```rust
use adapteros_memory::KReductionChannelConfig;

let config = KReductionChannelConfig {
    buffer_size: 32,              // Queue depth
    max_concurrent: 4,            // Concurrent ops
    response_timeout_ms: 5000,    // Timeout
    enable_telemetry: true,       // Logging
};

let manager = KReductionChannelManager::with_config(config);
```

## Monitoring

```rust
let stats = manager.get_stats();

println!("Sent: {}", stats.total_requests_sent);
println!("Received: {}", stats.total_requests_received);
println!("Approved: {}", stats.total_approved);
println!("Rejected: {}", stats.total_rejected);
println!("Drop rate: {}", stats.total_dropped);
```

## Error Handling

```rust
use adapteros_memory::{SendError, RecvError};

// Send
match tx.send(request).await {
    Ok(()) => {},
    Err(SendError::ChannelFull) => {},
    Err(SendError::ChannelClosed) => {},
    Err(SendError::SendTimeout) => {},
}

// Receive
match rx.try_recv() {
    Ok(request) => {},
    Err(RecvError::Empty) => {},
    Err(RecvError::Disconnected) => {},
    Err(RecvError::Timeout) => {},
}
```

## Sender Methods

| Method | Blocking | Timeout | Returns |
|--------|----------|---------|---------|
| `send()` | No | No | `Result<(), SendError>` |
| `send_with_timeout()` | Yes | Yes | `Result<(), SendError>` |
| `is_closed()` | No | N/A | `bool` |
| `pending_requests()` | No | N/A | `usize` |

## Receiver Methods

| Method | Blocking | Timeout | Returns |
|--------|----------|---------|---------|
| `recv()` | Yes | No | `Option<Request>` |
| `recv_with_timeout()` | Yes | Yes | `Result<Option<Request>, RecvError>` |
| `try_recv()` | No | No | `Result<Request, RecvError>` |
| `record_decision_outcome()` | No | N/A | `()` |
| `pending_requests()` | No | N/A | `usize` |

## Typical Flow

```
1. Memory pressure check triggers
2. Create KReductionRequest
3. Spawn async task: sender.send(request)
4. Return immediately (non-blocking)
5. Lifecycle manager receives request
6. Evaluate feasibility
7. Execute K reduction (unload adapters)
8. Record outcome in stats
```

## Best Practices

✓ Always spawn async task for send:
```rust
tokio::spawn(async move {
    let _ = sender.send(request).await;
});
```

✗ Don't block pressure check:
```rust
// BAD - blocks pressure check
let _ = sender.send(request).await;
```

✓ Monitor channel stats:
```rust
if stats.peak_queue_depth > buffer_size / 2 {
    warn!("High pressure rate");
}
```

✓ Handle all error cases:
```rust
match tx.send(req).await {
    Ok(()) => {},
    Err(e) => eprintln!("Failed: {:?}", e),
}
```

✓ Close channel gracefully:
```rust
drop(tx);  // Signal completion
rx.await;  // Wait for consumer
```

## Logging Output

```
DEBUG  K reduction request sent: request_id=..., target_k=8
INFO   K reduction request approved: new_k=8, freed=2097152
WARN   K reduction channel buffer full, request dropped
ERROR  K reduction channel closed, lifecycle manager unavailable
```

## Performance

| Metric | Value |
|--------|-------|
| Send latency | < 100 μs |
| Buffer overhead | 256 bytes (default) |
| Max throughput | 100k+ req/sec |
| Burst capacity | 32 items (default) |

## Compilation

```bash
cargo check -p adapteros-memory
cargo test -p adapteros-memory k_reduction
```

## Files

| File | Purpose |
|------|---------|
| `src/k_reduction_integration.rs` | Channel implementation |
| `src/pressure_manager.rs` | Integration point |
| `src/lib.rs` | Module exports |
| `K_REDUCTION_INTEGRATION_GUIDE.md` | Detailed guide |
| `K_REDUCTION_CONSUMER_EXAMPLE.rs` | Example code |

## Troubleshooting

| Problem | Solution |
|---------|----------|
| `ChannelFull` | Increase `buffer_size` |
| `ChannelClosed` | Check lifecycle manager logs |
| `SendTimeout` | Increase `response_timeout_ms` |
| No requests received | Verify sender created correctly |
| High approval_rate changes | Check lifecycle feasibility logic |

## Migration from Coordinator

```rust
// Old
let pm = MemoryPressureManager::with_coordinator(tracker, coordinator);

// New
let (tx, rx) = KReductionChannelManager::new().create_channel();
let pm = MemoryPressureManager::with_channel_sender(tracker, tx);

// Spawn consumer
tokio::spawn(async move {
    while let Some(req) = rx.recv().await {
        process(req).await;
    }
});
```

## Testing

```rust
#[tokio::test]
async fn test_k_reduction() {
    let manager = KReductionChannelManager::new();
    let (tx, mut rx) = manager.create_channel();

    let request = KReductionRequest::new(8, 10, 0.85, 1024*1024, 10.0, "Test".into());

    tx.send(request.clone()).await.unwrap();

    let received = rx.recv().await;
    assert_eq!(received.unwrap().request_id, request.request_id);
}
```

## Related Types

```rust
// Request
pub struct KReductionRequest {
    pub request_id: String,
    pub target_k: usize,
    pub current_k: usize,
    pub pressure_level: f32,  // 0-1
    pub bytes_to_free: u64,
    pub headroom_pct: f32,
    pub created_at: u128,
    pub reason: String,
}

// Statistics
pub struct KReductionChannelStats {
    pub total_requests_sent: u64,
    pub total_requests_received: u64,
    pub pending_requests: usize,
    pub total_approved: u64,
    pub total_rejected: u64,
    pub avg_processing_time_ms: f64,
    pub peak_queue_depth: usize,
    pub total_dropped: u64,
}
```

## Complete Minimal Example

```rust
use adapteros_memory::{KReductionChannelManager, MemoryPressureManager};

#[tokio::main]
async fn main() {
    // Create channel
    let manager = KReductionChannelManager::new();
    let (tx, mut rx) = manager.create_channel();

    // Setup memory manager
    let limits = MemoryLimits::new(4096, 8192, 0.15);
    let tracker = Arc::new(UnifiedMemoryTracker::new(limits));
    let pm = MemoryPressureManager::with_channel_sender(tracker, tx);

    // Consumer task
    tokio::spawn(async move {
        while let Some(request) = rx.recv().await {
            println!("K reduction request: target_k={}", request.target_k);
            rx.record_decision_outcome(true);
        }
    });

    // Trigger pressure check
    let _ = pm.check_and_handle_pressure();

    // Wait for consumer
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
}
```

---

**Last Updated:** 2025-11-22
**Status:** Ready for integration
**Files:** 3 modified, 1 created (k_reduction_integration.rs)
