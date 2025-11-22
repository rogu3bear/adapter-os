# K Reduction Event Bus Integration

## Overview

The lifecycle coordinator has been integrated with the K reduction event bus to enable memory-aware adapter management. This allows the memory pressure manager to request K reduction (reducing the number of active adapters) when memory pressure exceeds critical thresholds, and the lifecycle manager to evaluate these requests and execute them with rollback support.

## Architecture

### Components

1. **Memory Pressure Manager** (`adapteros-memory/src/pressure_manager.rs`)
   - Monitors memory usage across backends
   - Creates `KReductionRequest` when memory pressure requires K reduction
   - Sends requests through `KReductionCoordinator` channel

2. **Lifecycle Manager** (`adapteros-lora-lifecycle/src/lib.rs`)
   - Receives K reduction requests via channel
   - Evaluates requests using `LifecycleKReductionCoordinator`
   - Executes approved reductions with rollback capability

3. **K Reduction Coordinator** (`adapteros-lora-lifecycle/src/k_reduction_coordinator.rs`)
   - Evaluates requests based on adapter states
   - Selects adapters for unload (lowest activation first)
   - Respects pinned adapters and critical states

### Data Flow

```
Memory Pressure Manager
        ↓
  Creates KReductionRequest
        ↓
  Sends to Lifecycle Manager via Channel
        ↓
  LifecycleManager.poll_k_reduction_events()
        ↓
  Coordinator.evaluate_request()
        ↓
  Execute or Reject
        ↓
  Record Decision in History
        ↓
  Emit Telemetry
```

## Integration Steps

### Step 1: Wire Channel Connection

```rust
// In server initialization
let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
lifecycle_manager.wire_k_reduction_channel(rx);
memory_manager.set_k_reduction_channel(tx);
```

### Step 2: Poll for Events in Main Loop

```rust
// In background task or main loop
loop {
    match lifecycle_manager.poll_k_reduction_events().await {
        Ok(count) if count > 0 => {
            debug!("Processed {} K reduction events", count);
        }
        Err(e) => {
            error!("Error polling K reduction events: {}", e);
        }
        _ => {}
    }

    tokio::time::sleep(Duration::from_millis(100)).await;
}
```

## API Reference

### LifecycleManager Methods

#### `wire_k_reduction_channel()`

Establishes the connection with the memory manager's event bus.

```rust
pub fn wire_k_reduction_channel(
    &self,
    rx: tokio::sync::mpsc::UnboundedReceiver<adapteros_memory::KReductionRequest>,
)
```

**Arguments:**
- `rx`: Unbounded channel receiver for K reduction requests

**Example:**
```rust
lifecycle.wire_k_reduction_channel(memory_mgr.create_k_reduction_channel());
```

#### `poll_k_reduction_events()`

Non-blocking poll for K reduction requests from the event bus.

```rust
pub async fn poll_k_reduction_events(&self) -> Result<usize>
```

**Returns:** Number of requests processed

**Behavior:**
- Non-blocking: processes all pending requests immediately
- Evaluates each request through the coordinator
- Executes approved reductions with rollback on failure
- Records all decisions in history

**Example:**
```rust
match lifecycle.poll_k_reduction_events().await {
    Ok(count) => println!("Processed {} events", count),
    Err(e) => eprintln!("Error: {}", e),
}
```

#### `get_k_reduction_history()`

Retrieves the audit trail of K reduction decisions.

```rust
pub fn get_k_reduction_history(&self) -> Vec<KReductionExecutionRecord>
```

**Returns:** Vector of execution records

**Record Structure:**
```rust
pub struct KReductionExecutionRecord {
    pub request_id: String,
    pub old_k: usize,
    pub new_k: usize,
    pub approved: bool,
    pub executed: bool,
    pub adapters_unloaded: Vec<u16>,
    pub failure_reason: Option<String>,
    pub timestamp: std::time::SystemTime,
}
```

#### `clear_k_reduction_history()`

Clears the history (useful for testing).

```rust
pub fn clear_k_reduction_history(&self)
```

## Evaluation Logic

The `LifecycleKReductionCoordinator` evaluates requests based on:

1. **Request Validity**
   - Target K must be less than current K
   - Target K must be >= minimum K (default: 2)

2. **Memory Pressure Threshold**
   - Pressure level must exceed critical threshold (default: 0.70)
   - Calculated as: `memory_used / total_memory`

3. **Adapter Selection**
   - Sorts adapters by activation count (lowest first)
   - Skips pinned adapters
   - Skips Resident state adapters
   - Selects N adapters where N = current_K - target_K

4. **Estimated Memory Freed**
   - Conservative estimate: 1MB per adapter
   - More accurate estimate available if adapter metadata provided

## Execution with Rollback

When a K reduction is approved:

### Normal Execution Path

1. Unload adapters one by one
2. Record each successful unload
3. Update K value after all adapters unloaded
4. Emit telemetry with memory freed

### Rollback Triggered If

- Any adapter fails to unload
- Unload operation times out
- Unexpected error during eviction

### Rollback Process

1. Stop further unloads
2. Attempt to reload previously unloaded adapters in reverse order
3. Accept partial state if reload also fails
4. Emit rollback telemetry
5. Return error to caller

**Rollback Example:**
```
Request K: 8 -> 6
Unload: [adapter_8] ✓
Unload: [adapter_9] ✗ Error

Rollback triggered:
Reload: [adapter_8] ✓
K remains: 8
Decision recorded as: approved=true, executed=false, failure_reason="Failed to unload adapter 9"
```

## Telemetry Events

### k_reduction_executed

Emitted when K reduction completes successfully:

```json
{
  "request_id": "uuid",
  "old_k": 10,
  "new_k": 8,
  "adapters_unloaded": 2,
  "pressure_level": 0.85,
  "memory_freed": 2097152
}
```

### k_reduction_rollback

Emitted when K reduction fails and rollback occurs:

```json
{
  "request_id": "uuid",
  "attempted_rollback": 2,
  "successfully_reloaded": 2,
  "timestamp": "2025-11-22T10:30:45Z"
}
```

### adapter_evicted

Standard eviction event emitted for each unloaded adapter:

```json
{
  "adapter_id": "tenant/domain/purpose/r001",
  "from_state": "warm",
  "category": "high-priority",
  "memory_freed": 1048576
}
```

## Configuration

Default values in `LifecycleKReductionCoordinator::new()`:

| Parameter | Default | Purpose |
|-----------|---------|---------|
| `initial_k` | From manifest | Initial K value for router |
| `min_k` | 2 | Never reduce K below this |
| `critical_pressure_threshold` | 0.70 | Only approve if pressure ≥ 70% |

Customize in server initialization:

```rust
let coordinator = LifecycleKReductionCoordinator::new(
    10,    // initial_k
    1,     // min_k (allow reduction to single adapter)
    0.85   // critical_pressure_threshold (stricter)
);
```

## Error Handling

### Request Rejection Reasons

1. **Invalid Request**: Target K >= current K
2. **Below Min K**: Target K < minimum K threshold
3. **Low Pressure**: Pressure level < critical threshold
4. **Insufficient Adapters**: Not enough unloadable adapters available

### Execution Failures

Failures during adapter unload trigger rollback. Common failure sources:

- Adapter hold count > 0 (still in use)
- GPU buffer cleanup failure
- Database update failure
- Filesystem errors

All failures are logged and telemetered.

## Usage Example

### Complete Integration Flow

```rust
// 1. Initialize managers
let lifecycle = LifecycleManager::new_with_db(...);
let memory_mgr = MemoryPressureManager::new(tracker);

// 2. Wire the channel
let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
lifecycle.wire_k_reduction_channel(rx);
memory_mgr.set_coordinator(Arc::new(
    KReductionCoordinator::new(Arc::new(DefaultKReductionDecisionMaker::new(2, 0.70)), 100)
));

// 3. Spawn polling task
tokio::spawn({
    let lifecycle = lifecycle.clone();
    async move {
        loop {
            if let Ok(count) = lifecycle.poll_k_reduction_events().await {
                if count > 0 {
                    info!("Processed {} K reduction events", count);
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
});

// 4. Check results
let history = lifecycle.get_k_reduction_history();
for record in history {
    println!(
        "K {} -> {}: {}",
        record.old_k,
        record.new_k,
        if record.executed { "✓" } else { "✗" }
    );
}
```

### Monitoring K Reduction Decisions

```rust
// Get approval rate
let history = lifecycle.get_k_reduction_history();
let approved = history.iter().filter(|r| r.approved).count();
let approval_rate = approved as f32 / history.len() as f32;
println!("K reduction approval rate: {:.1}%", approval_rate * 100.0);

// Get execution success rate
let executed = history.iter().filter(|r| r.executed).count();
let success_rate = executed as f32 / approved as f32;
println!("Execution success rate: {:.1}%", success_rate * 100.0);

// Find failures
let failures: Vec<_> = history
    .iter()
    .filter(|r| !r.executed && r.approved)
    .collect();
for failure in failures {
    eprintln!(
        "K reduction failed: {} ({})",
        failure.request_id,
        failure.failure_reason.as_deref().unwrap_or("unknown")
    );
}
```

## Testing

### Unit Tests

The coordinator includes tests for:
- Valid request evaluation
- Pinned adapter protection
- Resident state protection
- Minimum K enforcement
- Pressure threshold validation

```bash
cargo test -p adapteros-lora-lifecycle k_reduction
```

### Integration Testing

Test the full flow:

```rust
#[tokio::test]
async fn test_k_reduction_integration() {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    lifecycle.wire_k_reduction_channel(rx);

    // Simulate pressure event
    let request = KReductionRequest::new(
        8,     // target_k
        10,    // current_k
        0.85,  // pressure_level
        1024*1024,  // bytes_to_free
        10.0,  // headroom_pct
        "Test".to_string(),
    );

    tx.send(request).unwrap();

    // Poll and verify
    let count = lifecycle.poll_k_reduction_events().await.unwrap();
    assert_eq!(count, 1);

    let history = lifecycle.get_k_reduction_history();
    assert!(history[0].approved);
}
```

## Troubleshooting

### "No requests received"

Verify:
1. Channel is wired: `lifecycle.wire_k_reduction_channel(rx)`
2. Memory manager is sending: Check pressure manager logs
3. Poll is running: Ensure background task is spawned

### "All requests rejected"

Check:
1. Pressure level: Must exceed 0.70 (configurable)
2. Available adapters: Must have unloadable adapters
3. Current K: Must be > min K
4. Adapter states: Pinned/Resident adapters skipped

### "Rollback failed"

May occur if:
1. Adapter held by another component
2. Database unavailable
3. GPU state corrupted

Monitor with: `lifecycle.get_k_reduction_history()`

## Performance Considerations

- **Poll Overhead**: O(n) where n = pending requests, minimal blocking
- **Coordinator Evaluation**: O(a log a) where a = total adapters (sorts by activation)
- **Rollback Cost**: Can be expensive if many adapters unloaded; design to avoid frequent failures
- **History Growth**: Bounded; consider cleanup strategy for long-running systems

## Future Enhancements

1. **Predictive K Reduction**: Reduce K before pressure becomes critical
2. **Tiered Strategies**: Different reduction targets based on workload
3. **Adaptive Thresholds**: Learn from historical pressure patterns
4. **Cross-Tenant K Sharing**: Dynamically balance K across isolated workloads
5. **K Expansion**: Reverse K reduction when pressure decreases
