# LoadCoordinator Quick Reference

## Basic Usage

```rust
use adapteros_server_api::LoadCoordinator;
use std::sync::Arc;

// Create coordinator (typically in AppState)
let coordinator = Arc::new(LoadCoordinator::new());

// Use in request handler
let handle = coordinator.load_or_wait("my-adapter", || async {
    // This closure is only called by the first request
    adapter_loader.load_adapter(42, "my-adapter").await
}).await?;
```

## Key Points

1. **First Request:** Executes the load function
2. **Subsequent Requests:** Wait for the first to complete
3. **All Requests:** Receive the same result (success or error)
4. **Cleanup:** Automatic when load completes

## Common Patterns

### With AppState

```rust
pub struct AppState {
    pub load_coordinator: Arc<LoadCoordinator>,
    pub adapter_loader: Arc<AdapterLoader>,
}

async fn handle_request(
    State(state): State<Arc<AppState>>,
    adapter_id: String,
) -> Result<Response> {
    let handle = state.load_coordinator
        .load_or_wait(&adapter_id, || {
            state.adapter_loader.load_adapter(42, &adapter_id)
        })
        .await?;
    // Use handle...
}
```

### Monitoring

```rust
// Check if loading
if coordinator.is_loading("my-adapter") {
    println!("Load in progress");
}

// Get waiter count
let waiters = coordinator.waiter_count("my-adapter");
println!("{} requests waiting", waiters);

// Get metrics
let metrics = coordinator.metrics();
println!("Pending: {}, Waiters: {}",
    metrics.pending_loads,
    metrics.total_waiters
);
```

## Error Handling

```rust
match coordinator.load_or_wait("adapter", load_fn).await {
    Ok(handle) => {
        // All waiters get the handle
    }
    Err(e) => {
        // All waiters get the same error
        // Next request will retry (fresh load)
    }
}
```

## See Also

- Full documentation: [docs/LOAD_COORDINATOR.md](../../../../docs/LOAD_COORDINATOR.md)
- Tests: [tests/load_coordinator_standalone.rs](../../tests/load_coordinator_standalone.rs)
- Example: [examples/load_coordinator_demo.rs](../../examples/load_coordinator_demo.rs)
