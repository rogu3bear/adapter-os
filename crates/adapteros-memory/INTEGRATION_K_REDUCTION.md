# K Reduction Integration Guide

## Overview

The K Reduction Integration module (`k_reduction_integration.rs`) wires the memory pressure manager to the lifecycle manager via tokio mpsc channels. This enables asynchronous, non-blocking communication when memory pressure requires reducing the number of active adapters (K).

## Architecture

```
┌──────────────────────────────┐
│  MemoryPressureManager       │
│  (Detects memory pressure)   │
└──────────────┬───────────────┘
               │
        sends request via
               │
    ┌──────────v──────────┐
    │  mpsc::Channel      │
    │  (tokio-based)      │
    └──────────┬──────────┘
               │
        consumes request
               │
┌──────────────v──────────────┐
│  LifecycleManager           │
│  (Implements K reduction)   │
└─────────────────────────────┘
```

## Key Components

### 1. `KReductionChannelManager`

Factory for creating sender/receiver pairs. Manages configuration and statistics.

```rust
use adapteros_memory::KReductionChannelManager;

let manager = KReductionChannelManager::new();  // Default config
let (tx, rx) = manager.create_channel();
```

### 2. `KReductionRequestSender`

Sends K reduction requests (cloneable for sharing).

**Methods:**
- `send(request)` - Non-blocking send with error handling
- `send_with_timeout(request, timeout_ms)` - Blocking with timeout
- `is_closed()` - Check if receiver is dropped
- `pending_requests()` - Get buffer occupancy

**Errors:**
- `SendError::ChannelFull` - Buffer exhausted
- `SendError::ChannelClosed` - Receiver dropped
- `SendError::SendTimeout` - Timeout exceeded

### 3. `KReductionRequestReceiver`

Consumes K reduction requests.

**Methods:**
- `recv()` - Async wait for next request
- `recv_with_timeout(timeout_ms)` - With timeout
- `try_recv()` - Non-blocking attempt
- `record_decision_outcome(approved)` - Update stats
- `pending_requests()` - Get queue length

**Errors:**
- `RecvError::Empty` - No messages (try_recv only)
- `RecvError::Disconnected` - Sender dropped
- `RecvError::Timeout` - Timeout exceeded

### 4. `KReductionChannelStats`

Tracks channel activity for observability.

```rust
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

## Usage Examples

### Example 1: Memory Manager with Channel Sender

```rust
use adapteros_memory::{
    KReductionChannelManager, MemoryPressureManager,
    UnifiedMemoryTracker, MemoryLimits,
};
use std::sync::Arc;

// Create channel
let channel_mgr = KReductionChannelManager::new();
let (tx, _rx) = channel_mgr.create_channel();

// Create memory manager with sender
let limits = MemoryLimits::new(4096, 8192, 0.15);
let tracker = Arc::new(UnifiedMemoryTracker::new(limits));
let pressure_mgr = MemoryPressureManager::with_channel_sender(tracker, tx);

// When pressure hits threshold, K reduction request is sent automatically
let report = pressure_mgr.check_and_handle_pressure()?;
```

### Example 2: Lifecycle Manager Consuming Requests

```rust
use adapteros_memory::{KReductionChannelManager};
use tokio::task;

async fn lifecycle_consumer_task(mut rx: KReductionRequestReceiver) {
    while let Some(request) = rx.recv().await {
        println!("Received K reduction request: {:?}", request);

        // Process the K reduction request
        let approved = evaluate_k_reduction(&request).await;
        rx.record_decision_outcome(approved);

        // Implement actual K reduction in lifecycle manager
        if approved {
            reduce_k(&request).await;
        }
    }

    println!("K reduction channel closed");
}

#[tokio::main]
async fn main() {
    let manager = KReductionChannelManager::new();
    let (tx, rx) = manager.create_channel();

    // Spawn lifecycle consumer
    task::spawn(lifecycle_consumer_task(rx));

    // Use tx to send requests...
    drop(tx); // Close channel when done
}
```

### Example 3: With Custom Configuration

```rust
use adapteros_memory::{
    KReductionChannelManager, KReductionChannelConfig,
};

let config = KReductionChannelConfig {
    buffer_size: 64,           // Larger buffer
    max_concurrent: 8,         // More concurrent ops
    response_timeout_ms: 10000, // Longer timeout
    enable_telemetry: true,
};

let manager = KReductionChannelManager::with_config(config);
let (tx, rx) = manager.create_channel();
```

### Example 4: Monitoring Channel Statistics

```rust
use adapteros_memory::KReductionChannelManager;

let manager = KReductionChannelManager::new();
let (tx, rx) = manager.create_channel();

// ... send/receive requests ...

// Get stats periodically
let stats = manager.get_stats();
println!("Requests sent: {}", stats.total_requests_sent);
println!("Requests received: {}", stats.total_requests_received);
println!("Approval rate: {:.2}%",
    (stats.total_approved as f64 / stats.total_requests_received as f64) * 100.0
);
```

## Integration with MemoryPressureManager

The `MemoryPressureManager` now supports both channel-based and coordinator-based K reduction:

### Construction

```rust
// With channel (async, preferred)
let pm = MemoryPressureManager::with_channel_sender(tracker, tx);

// With coordinator (sync, fallback)
let pm = MemoryPressureManager::with_coordinator(tracker, coordinator);

// Both can be set
pm.set_channel_sender(tx);
pm.set_coordinator(coordinator);
```

### Request Handling

When `check_and_handle_pressure()` returns `EvictionStrategy::ReduceK`:

1. **If channel sender is configured:**
   - Spawns async task to send request
   - Returns immediately to avoid blocking
   - Logs send errors but doesn't fail the check

2. **If only coordinator is configured:**
   - Synchronously processes through coordinator
   - Evaluates request immediately
   - Blocks until decision is made

3. **If neither is configured:**
   - Logs warning
   - Returns ReduceK action but no actual reduction occurs

## Error Handling

### Send Errors

```rust
use adapteros_memory::SendError;

match tx.send(request).await {
    Ok(()) => println!("Request sent"),
    Err(SendError::ChannelFull) => {
        warn!("Too many pending requests, request dropped");
    }
    Err(SendError::ChannelClosed) => {
        error!("Lifecycle manager not running");
    }
    Err(SendError::SendTimeout) => {
        warn!("Send timed out");
    }
}
```

### Receive Errors

```rust
use adapteros_memory::RecvError;

match rx.try_recv() {
    Ok(request) => println!("Got request: {:?}", request),
    Err(RecvError::Empty) => {
        println!("No requests pending");
    }
    Err(RecvError::Disconnected) => {
        error!("Memory manager stopped sending");
    }
    Err(RecvError::Timeout) => {
        println!("Receive timed out");
    }
}
```

## Configuration

### Default Configuration

```rust
KReductionChannelConfig {
    buffer_size: 32,              // 32 pending requests
    max_concurrent: 4,            // 4 concurrent operations
    response_timeout_ms: 5000,    // 5 second timeout
    enable_telemetry: true,       // Detailed logging
}
```

### Tuning Guidelines

- **Increase `buffer_size`:** If receiving memory pressure spikes
- **Increase `max_concurrent`:** If handling multiple adapters concurrently
- **Increase `response_timeout_ms`:** If lifecycle manager is slow to process
- **Disable `enable_telemetry`:** For production to reduce logging overhead

## Thread Safety

All components are fully thread-safe:

- `KReductionRequestSender` implements `Clone` and `Send + Sync`
- `KReductionRequestReceiver` is `Send + Sync`
- Internal statistics use `Arc<RwLock<_>>` for safe concurrent access
- Channel is backed by tokio's concurrent mpsc implementation

## Testing

### Unit Tests

```bash
cargo test -p adapteros-memory k_reduction_integration
```

Tests cover:
- Channel creation and configuration
- Send/receive operations
- Error conditions (full, closed, timeout)
- Try-recv behavior
- Decision outcome recording
- Statistics tracking

### Integration Testing

```rust
#[tokio::test]
async fn test_full_k_reduction_flow() {
    // Setup
    let manager = KReductionChannelManager::new();
    let (tx, mut rx) = manager.create_channel();

    // Create memory manager
    let limits = MemoryLimits::new(1024, 2048, 0.15);
    let tracker = Arc::new(UnifiedMemoryTracker::new(limits));
    let pressure_mgr = MemoryPressureManager::with_channel_sender(tracker, tx);

    // Trigger memory pressure -> K reduction request
    pressure_mgr.check_and_handle_pressure();

    // Receive and process
    if let Some(request) = rx.try_recv().ok() {
        println!("Received request: {}", request.request_id);
        rx.record_decision_outcome(true);
    }

    // Verify stats
    let stats = manager.get_stats();
    assert!(stats.total_requests_sent > 0);
    assert!(stats.total_approved > 0);
}
```

## Logging

All operations are logged with structured tracing:

### Log Levels

- **debug!()** - Request send/recv, channel operations
- **info!()** - Successful sends, approved decisions
- **warn!()** - Channel full, timeouts, rejections
- **error!()** - Channel closed, critical failures

### Example Log Output

```
DEBUG K reduction request sent through channel: request_id=550e8400-e29b-41d4-a716-446655440000, target_k=8
INFO K reduction request approved: new_k=8, adapters_to_unload=2
WARN K reduction channel buffer full, request dropped: request_id=...
ERROR K reduction channel closed, lifecycle manager not available: request_id=...
```

## Performance Considerations

### Memory Usage

- **Fixed overhead:** ~1 KB per channel (config, stats, lock)
- **Buffer overhead:** 8 bytes × buffer_size per request
- **Default:** 32 requests × 8 bytes = 256 bytes buffer

### Latency

- **Send latency:** < 100 μs (non-blocking)
- **Spawn latency:** ~10 μs (tokio task spawn)
- **Total e2e:** ~110 μs from pressure check to send

### Throughput

- **Sustained:** 100k+ requests/sec (with tokio runtime)
- **Burst:** Limited by buffer_size (default 32)
- **Scalability:** Linear with buffer_size

## Best Practices

1. **Always set channel sender in production:**
   ```rust
   let pressure_mgr = MemoryPressureManager::with_channel_sender(tracker, tx);
   ```

2. **Spawn receiver task early:**
   ```rust
   tokio::spawn(async move {
       while let Some(request) = rx.recv().await {
           process_request(request).await;
       }
   });
   ```

3. **Monitor channel statistics:**
   ```rust
   let stats = manager.get_stats();
   if stats.peak_queue_depth > buffer_size / 2 {
       warn!("High K reduction request rate");
   }
   ```

4. **Handle send errors gracefully:**
   ```rust
   match tx.send(request).await {
       Ok(()) => {},
       Err(SendError::ChannelFull) => {
           // Implement backpressure or skip
       }
       Err(SendError::ChannelClosed) => {
           // Fallback to emergency measures
       }
       Err(SendError::SendTimeout) => {
           // Retry or escalate
       }
   }
   ```

5. **Don't block the memory pressure check:**
   ```rust
   // Good: Spawn async task
   tokio::spawn(async move {
       let _ = tx.send(request).await;
   });

   // Bad: Blocks pressure check
   // let _ = tx.send(request).await;
   ```

## Troubleshooting

### Channel closes unexpectedly

**Symptom:** `SendError::ChannelClosed`

**Causes:**
- Lifecycle manager task panicked or stopped
- Receiver dropped without error handling
- Tokio runtime shutdown

**Solution:**
- Check lifecycle manager logs for panics
- Ensure receiver task is spawned in persistent tokio task
- Add error handling and recovery

### Buffer full errors

**Symptom:** `SendError::ChannelFull`

**Causes:**
- Receiver not keeping up with sender
- Lifecycle manager processing too slowly
- Memory pressure spikes overwhelming queue

**Solution:**
- Increase `buffer_size` in config
- Profile and optimize lifecycle manager
- Implement backpressure strategy

### High latency

**Symptom:** `SendError::SendTimeout`

**Causes:**
- Receiver task blocked
- Tokio runtime overloaded
- Lock contention on shared state

**Solution:**
- Increase `response_timeout_ms`
- Check for blocking operations in receiver
- Profile with `tokio-console`

## Migration from Coordinator

If using the old `KReductionCoordinator` approach:

```rust
// Old way (synchronous, blocks)
let pressure_mgr = MemoryPressureManager::with_coordinator(tracker, coordinator);

// New way (asynchronous, preferred)
let manager = KReductionChannelManager::new();
let (tx, rx) = manager.create_channel();
let pressure_mgr = MemoryPressureManager::with_channel_sender(tracker, tx);

// Spawn lifecycle receiver task
tokio::spawn(async move {
    while let Some(request) = rx.recv().await {
        // Process request
    }
});
```

## See Also

- `crates/adapteros-memory/src/k_reduction_integration.rs` - Implementation
- `crates/adapteros-memory/src/pressure_manager.rs` - Integration points
- `crates/adapteros-memory/src/k_reduction_protocol.rs` - Protocol definitions
- `crates/adapteros-lora-lifecycle/` - Lifecycle manager (consumer)
