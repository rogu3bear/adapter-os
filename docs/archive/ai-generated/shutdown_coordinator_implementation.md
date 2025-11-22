# Graceful Shutdown Implementation for AdapterOS Server

## Overview
This document outlines the implementation of graceful shutdown coordination for background tasks in the AdapterOS control plane server.

## Problem
The server spawns 8+ background tasks that run in infinite loops without shutdown coordination, causing unclean shutdowns when SIGTERM/SIGINT signals are received.

## Background Tasks Identified
1. **SIGHUP handler** (line 535) - Config reload listener
2. **Alert watcher** (line 585) - Job failure monitoring
3. **Policy hash watcher** (line 639) - Continuous policy validation
4. **UDS metrics exporter** (line 677) - Zero-network metrics export
5. **Status writer** (line 813) - 5-second status file updates
6. **TTL cleanup** (line 829) - 5-minute expired adapter cleanup
7. **Heartbeat recovery** (line 883) - 5-minute stale adapter recovery
8. **Index rebuilder** (line 911) - 5-minute index verification

## Solution Architecture

### 1. ShutdownCoordinator Struct
```rust
/// Shutdown coordinator for graceful termination of background tasks
struct ShutdownCoordinator {
    shutdown_tx: broadcast::Sender<()>,
    task_handles: Vec<TaskHandle>,
}

enum TaskHandle {
    Deterministic(String, DeterministicJoinHandle),
    Tokio(String, JoinHandle<()>),
}
```

The coordinator:
- Maintains a broadcast channel for shutdown signals
- Stores join handles for all spawned tasks
- Provides subscription mechanism for tasks to listen for shutdown
- Implements graceful shutdown with configurable timeout (30s default)

### 2. Task Modifications

Each background task loop must be updated to:
- Accept a `broadcast::Receiver<()>` for shutdown signals
- Use `tokio::select!` to listen for both normal operations and shutdown
- Clean up resources before exiting
- Log shutdown completion

Example pattern:
```rust
let mut shutdown_rx = shutdown_coordinator.subscribe();
let handle = spawn_deterministic("Task name".to_string(), async move {
    let mut interval = tokio::time::interval(Duration::from_secs(N));
    loop {
        tokio::select! {
            _ = interval.tick() => {
                // Normal task work
            }
            _ = shutdown_rx.recv() => {
                info!("Task name shutting down");
                break;
            }
        }
    }
    // Cleanup code here
});
shutdown_coordinator.add_deterministic_task("Task name".to_string(), handle);
```

### 3. Shutdown Signal Handler Update

The `shutdown_signal()` function must:
- Wait for SIGTERM/SIGINT
- Call `shutdown_coordinator.shutdown(Duration::from_secs(30)).await`
- Log tasks that complete vs timeout

## Implementation Steps

### Step 1: Add imports to main.rs
```rust
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use adapteros_deterministic_exec::DeterministicJoinHandle;
```

### Step 2: Add ShutdownCoordinator after PidFileLock (line ~100)
[See full struct definition above]

### Step 3: Initialize coordinator before SIGHUP handler (line ~530)
```rust
// Initialize shutdown coordinator
let mut shutdown_coordinator = ShutdownCoordinator::new();
```

### Step 4: Update each background task

#### 4a. SIGHUP handler (line 535)
```rust
let mut shutdown_rx = shutdown_coordinator.subscribe();
let handle = spawn_deterministic("SIGHUP handler".to_string(), async move {
    use tokio::signal::unix::{signal, SignalKind};
    let mut sig = signal(SignalKind::hangup()).expect("Failed to setup SIGHUP handler");
    loop {
        tokio::select! {
            _ = sig.recv() => {
                // ... existing config reload logic ...
            }
            _ = shutdown_rx.recv() => {
                info!("SIGHUP handler shutting down");
                break;
            }
        }
    }
});
if let Ok(h) = handle {
    shutdown_coordinator.add_deterministic_task("SIGHUP handler".to_string(), h);
}
```

#### 4b. Alert watcher (line 585)
Update `alerting.rs`:
```rust
pub async fn start(
    mut self,
    mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
) -> Result<()> {
    // ... existing setup ...
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                // ... existing work ...
            }
            _ = shutdown_rx.recv() => {
                info!("Alert watcher received shutdown signal");
                break;
            }
        }
    }
    info!("Alert watcher shutting down");
    Ok(())
}
```

Update spawn call in main.rs:
```rust
let shutdown_rx = shutdown_coordinator.subscribe();
let handle = alerting::spawn_alert_watcher(db.clone(), cfg.alerting.clone(), shutdown_rx)?;
shutdown_coordinator.add_deterministic_task("Alert watcher".to_string(), handle);
```

#### 4c. Policy hash watcher (line 639)
Update `adapteros-policy/src/hash_watcher.rs`:
```rust
pub fn start_background_watcher(
    self: Arc<Self>,
    interval: Duration,
    policy_hashes: Arc<RwLock<HashMap<String, B3Hash>>>,
    mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    // ... existing validation logic ...
                }
                _ = shutdown_rx.recv() => {
                    info!("Policy hash watcher shutting down");
                    break;
                }
            }
        }
    })
}
```

Update call in main.rs:
```rust
let shutdown_rx = shutdown_coordinator.subscribe();
let watcher_handle = policy_watcher
    .clone()
    .start_background_watcher(Duration::from_secs(60), policy_hashes.clone(), shutdown_rx);
shutdown_coordinator.add_tokio_task("Policy hash watcher".to_string(), watcher_handle);
```

#### 4d. UDS metrics exporter (line 677)
```rust
let shutdown_rx = shutdown_coordinator.subscribe();
let exporter_socket_path = socket_path.clone();
let handle = tokio::spawn(async move {
    tokio::select! {
        result = uds_exporter.serve() => {
            if let Err(e) = result {
                error!("UDS metrics exporter error: {}", e);
            }
        }
        _ = shutdown_rx.recv() => {
            info!("UDS metrics exporter shutting down");
        }
    }
});
shutdown_coordinator.add_tokio_task("UDS metrics exporter".to_string(), handle);
```

#### 4e. Status writer (line 813)
```rust
let state_clone = state.clone();
let mut shutdown_rx = shutdown_coordinator.subscribe();
let handle = spawn_deterministic("Status writer".to_string(), async move {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    loop {
        tokio::select! {
            _ = interval.tick() => {
                if let Err(e) = status_writer::write_status(&state_clone).await {
                    warn!("Failed to write status: {}", e);
                }
            }
            _ = shutdown_rx.recv() => {
                info!("Status writer shutting down");
                break;
            }
        }
    }
});
if let Ok(h) = handle {
    shutdown_coordinator.add_deterministic_task("Status writer".to_string(), h);
}
```

#### 4f. TTL cleanup (line 829)
```rust
let db_clone = db.clone();
let mut shutdown_rx = shutdown_coordinator.subscribe();
let handle = spawn_deterministic("TTL cleanup".to_string(), async move {
    let mut interval = tokio::time::interval(Duration::from_secs(300));
    loop {
        tokio::select! {
            _ = interval.tick() => {
                // ... existing cleanup logic ...
            }
            _ = shutdown_rx.recv() => {
                info!("TTL cleanup shutting down");
                break;
            }
        }
    }
});
if let Ok(h) = handle {
    shutdown_coordinator.add_deterministic_task("TTL cleanup".to_string(), h);
}
```

#### 4g. Heartbeat recovery (line 883)
```rust
let db_clone = db.clone();
let mut shutdown_rx = shutdown_coordinator.subscribe();
let handle = spawn_deterministic("Heartbeat recovery".to_string(), async move {
    let mut interval = tokio::time::interval(Duration::from_secs(300));
    loop {
        tokio::select! {
            _ = interval.tick() => {
                // ... existing recovery logic ...
            }
            _ = shutdown_rx.recv() => {
                info!("Heartbeat recovery shutting down");
                break;
            }
        }
    }
});
if let Ok(h) = handle {
    shutdown_coordinator.add_deterministic_task("Heartbeat recovery".to_string(), h);
}
```

#### 4h. Index rebuilder (line 911)
```rust
let mut shutdown_rx = shutdown_coordinator.subscribe();
let index_rebuilder = tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(300));
    loop {
        tokio::select! {
            _ = interval.tick() => {
                if let Err(e) = rebuild_all_indexes(&db).await {
                    warn!("Index rebuild failed: {}", e);
                }
            }
            _ = shutdown_rx.recv() => {
                info!("Index rebuilder shutting down");
                break;
            }
        }
    }
});
shutdown_coordinator.add_tokio_task("Index rebuilder".to_string(), index_rebuilder);
```

### Step 5: Update shutdown_signal() function (line 969)

Replace current implementation with:
```rust
async fn shutdown_signal(shutdown_coordinator: ShutdownCoordinator) {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    // Use deterministic select instead of tokio::select!
    let _ = select_2(ctrl_c, terminate).await;

    info!("Shutdown signal received, coordinating graceful shutdown");

    // Coordinate graceful shutdown with 30-second timeout
    shutdown_coordinator.shutdown(Duration::from_secs(30)).await;
}
```

### Step 6: Update axum serve call (line 941)

```rust
axum::serve(listener, app)
    .with_graceful_shutdown(shutdown_signal(shutdown_coordinator))
    .await?;
```

## Testing

### Manual Testing
```bash
# Start the server
cargo run --bin aos-cp

# In another terminal, send SIGTERM
pkill -TERM aos-cp

# Verify log output shows:
# - "Broadcasting shutdown signal to 8 tasks"
# - Each task logging "Task name shutting down"
# - "Shutdown complete: N tasks completed, 0 timed out"
```

### Test Cases
1. **Clean shutdown**: All tasks complete within 30s timeout
2. **Timeout scenario**: Simulate stuck task to verify timeout handling
3. **SIGINT (Ctrl+C)**: Verify same shutdown behavior
4. **Multiple signals**: Verify second signal doesn't cause issues

## Expected Results

After implementation:
- **8 tasks coordinated** for shutdown
- Clean termination with no zombie processes
- All tasks log completion messages
- Database connections properly closed
- Socket files cleaned up
- No resource leaks

## Success Metrics

1. All 8 background tasks successfully coordinate shutdown
2. Shutdown completes in < 5 seconds under normal conditions
3. No tasks timeout during clean shutdown
4. Log messages show orderly shutdown progression
5. Process exits cleanly with exit code 0
