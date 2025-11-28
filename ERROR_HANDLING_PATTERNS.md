# Error Handling Patterns - Quick Reference

These patterns were implemented in the AdapterOS control plane fixes and can be reused throughout the codebase.

---

## Pattern 1: Lock Poisoning Handling

**Use When:** Reading/writing from `Arc<RwLock<T>>` where poison could cause panic

```rust
// BEFORE (PANIC RISK):
let cfg = server_config.read().unwrap();

// AFTER (SAFE):
let cfg = server_config.read().map_err(|e| {
    error!("Config lock poisoned: {}", e);
    AosError::Config("config lock poisoned".into())
})?;
```

**Alternative (Non-Propagating):**
```rust
match config.read() {
    Ok(cfg) => {
        // Use cfg
    }
    Err(e) => {
        error!("Config lock poisoned: {}", e);
        // Continue with defaults or skip operation
    }
}
```

---

## Pattern 2: Signal Handler Graceful Degradation

**Use When:** Registering Unix signals for config reload, shutdown, etc.

```rust
// BEFORE (PANIC RISK):
let mut sig = signal(SignalKind::hangup()).expect("Failed to setup handler");

// AFTER (SAFE):
let mut sig = match signal(SignalKind::hangup()) {
    Ok(s) => s,
    Err(e) => {
        warn!(
            error = %e,
            "Failed to register signal handler, feature will be unavailable"
        );
        return; // Exit task gracefully
    }
};
```

**For shutdown signals:**
```rust
let ctrl_c = async {
    match signal::ctrl_c().await {
        Ok(()) => {}
        Err(e) => {
            error!(error = %e, "Failed to install Ctrl+C handler");
        }
    }
};

let terminate = async {
    match signal::unix::signal(signal::unix::SignalKind::terminate()) {
        Ok(mut sig) => {
            sig.recv().await;
        }
        Err(e) => {
            warn!(error = %e, "Failed to install SIGTERM handler");
            std::future::pending::<()>().await; // Block forever
        }
    }
};
```

---

## Pattern 3: spawn_deterministic Error Handling

**Use When:** Spawning background tasks with deterministic executor

```rust
// BEFORE (PANIC RISK):
let handle = spawn_deterministic("Task name".to_string(), async move {
    // task logic
})
.expect("Failed to spawn task");
shutdown_coordinator.register_task(handle);

// AFTER (SAFE):
match spawn_deterministic("Task name".to_string(), async move {
    // task logic
}) {
    Ok(handle) => {
        shutdown_coordinator.register_task(handle);
        info!("Task started successfully");
    }
    Err(e) => {
        error!(
            error = %e,
            "Failed to spawn task, feature will be unavailable"
        );
    }
}
```

---

## Pattern 4: Circuit Breaker + Exponential Backoff

**Use When:** Background tasks that could enter infinite error loops

```rust
let mut consecutive_errors = 0u32;
const MAX_CONSECUTIVE_ERRORS: u32 = 5;
const CIRCUIT_BREAKER_PAUSE_SECS: u64 = 1800; // 30 minutes

loop {
    interval.tick().await;

    // Circuit breaker: pause if too many consecutive errors
    if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
        error!(
            consecutive_errors,
            pause_duration_secs = CIRCUIT_BREAKER_PAUSE_SECS,
            "Circuit breaker triggered, pausing task"
        );
        tokio::time::sleep(Duration::from_secs(CIRCUIT_BREAKER_PAUSE_SECS)).await;
        consecutive_errors = 0;
        continue;
    }

    // Perform operation
    match perform_operation().await {
        Ok(result) => {
            // Handle success
            consecutive_errors = 0; // Reset on success
        }
        Err(e) => {
            consecutive_errors += 1;
            let backoff_secs = 2u64.pow(consecutive_errors.min(6)); // Cap at 64 seconds
            warn!(
                error = %e,
                consecutive_errors,
                backoff_secs,
                "Operation failed, applying exponential backoff"
            );
            tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
        }
    }
}
```

**Backoff Progression:** 2s → 4s → 8s → 16s → 32s → 64s (capped)
**Circuit Opens:** After 5 consecutive errors
**Circuit Recovery:** Automatic after 30 minute pause

---

## Pattern 5: Multi-Operation Circuit Breaker

**Use When:** Multiple operations in a loop, any could fail

```rust
let mut consecutive_errors = 0u32;
const MAX_CONSECUTIVE_ERRORS: u32 = 5;
const CIRCUIT_BREAKER_PAUSE_SECS: u64 = 1800;

loop {
    interval.tick().await;

    // Circuit breaker check
    if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
        error!("Circuit breaker triggered");
        tokio::time::sleep(Duration::from_secs(CIRCUIT_BREAKER_PAUSE_SECS)).await;
        consecutive_errors = 0;
        continue;
    }

    let mut had_error = false;

    // Operation 1
    match operation1().await {
        Ok(_) => { /* handle success */ }
        Err(e) => {
            had_error = true;
            warn!(error = %e, "Operation 1 failed");
        }
    }

    // Operation 2
    if let Err(e) = operation2().await {
        had_error = true;
        warn!(error = %e, "Operation 2 failed");
    }

    // Update error counter with exponential backoff
    if had_error {
        consecutive_errors += 1;
        let backoff_secs = 2u64.pow(consecutive_errors.min(6));
        warn!(consecutive_errors, backoff_secs, "Applying exponential backoff");
        tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
    } else {
        consecutive_errors = 0; // Reset on full success
    }
}
```

---

## Pattern 6: Drain Timeout with Diagnostics

**Use When:** Gracefully shutting down with in-flight operations

```rust
let start = tokio::time::Instant::now();
let mut logged_waiting = false;
let mut sample_count = 0u64;
let mut total_in_flight = 0u64;
let mut peak_in_flight = 0usize;

loop {
    let count = in_flight_requests.load(std::sync::atomic::Ordering::SeqCst);

    // Track statistics for drain analysis
    sample_count += 1;
    total_in_flight += count as u64;
    peak_in_flight = peak_in_flight.max(count);

    if count == 0 {
        info!("All operations completed");
        break;
    }

    if !logged_waiting {
        info!(
            in_flight = count,
            timeout_secs = drain_timeout.as_secs(),
            "Waiting for operations to complete"
        );
        logged_waiting = true;
    }

    let elapsed = start.elapsed();
    if elapsed >= drain_timeout {
        let avg_in_flight = if sample_count > 0 {
            total_in_flight as f64 / sample_count as f64
        } else {
            0.0
        };

        error!(
            in_flight_current = count,
            in_flight_peak = peak_in_flight,
            in_flight_avg = format!("{:.2}", avg_in_flight),
            elapsed_secs = elapsed.as_secs(),
            timeout_secs = drain_timeout.as_secs(),
            sample_count,
            "Drain timeout exceeded - incomplete operations detected"
        );

        error!(
            "MANUAL RECOVERY REQUIRED: {} operations incomplete. \
             Peak: {}, Average: {:.2}. \
             Investigate: database locks, slow I/O, stuck tasks.",
            count,
            peak_in_flight,
            avg_in_flight
        );

        break;
    }

    // Sample every 100ms
    tokio::time::sleep(Duration::from_millis(100)).await;
}
```

**Metrics Collected:**
- `sample_count`: Number of 100ms samples (total drain time / 100ms)
- `total_in_flight`: Sum of all in-flight counts
- `peak_in_flight`: Maximum simultaneous operations
- `avg_in_flight`: Average operations during drain

---

## Anti-Patterns to Avoid

### 1. Unwrap on Locks
```rust
// BAD:
let cfg = config.read().unwrap();

// GOOD:
let cfg = config.read().map_err(|e| AosError::Config(...))?;
```

### 2. Expect on Spawns
```rust
// BAD:
let handle = spawn_deterministic(...).expect("spawn failed");

// GOOD:
match spawn_deterministic(...) {
    Ok(handle) => { /* use handle */ }
    Err(e) => { error!("spawn failed: {}", e); }
}
```

### 3. Infinite Error Loops
```rust
// BAD:
loop {
    if let Err(e) = operation().await {
        warn!("error: {}", e);
        // Retry immediately, flooding logs
    }
}

// GOOD:
let mut errors = 0;
loop {
    match operation().await {
        Ok(_) => { errors = 0; }
        Err(e) => {
            errors += 1;
            if errors >= 5 {
                // Circuit breaker
                tokio::time::sleep(Duration::from_secs(1800)).await;
                errors = 0;
            } else {
                // Exponential backoff
                let backoff = 2u64.pow(errors.min(6));
                tokio::time::sleep(Duration::from_secs(backoff)).await;
            }
        }
    }
}
```

### 4. Panicking Signal Handlers
```rust
// BAD:
let sig = signal(SignalKind::hangup()).expect("signal failed");

// GOOD:
let sig = match signal(SignalKind::hangup()) {
    Ok(s) => s,
    Err(e) => {
        warn!("signal handler unavailable: {}", e);
        return; // Exit gracefully
    }
};
```

---

## Configuration Values

### Circuit Breaker
- `MAX_CONSECUTIVE_ERRORS`: 5 (triggers circuit breaker)
- `CIRCUIT_BREAKER_PAUSE_SECS`: 1800 (30 minutes)

### Exponential Backoff
- Base: 2 seconds
- Max exponent: 6 (cap at 64 seconds)
- Formula: `2^min(consecutive_errors, 6)` seconds

### Drain Timeout
- Sample interval: 100ms
- Default timeout: 30 seconds (configurable via `drain_timeout_secs`)

---

## Quick Decision Tree

**Should I use error handling pattern?**

1. **Is it a lock operation (`read()`/`write()`)? → Use Pattern 1**
2. **Is it signal registration? → Use Pattern 2**
3. **Is it spawning a task? → Use Pattern 3**
4. **Is it a background loop that could fail? → Use Pattern 4 or 5**
5. **Is it draining operations on shutdown? → Use Pattern 6**

---

## References

- CLAUDE.md: Error handling must use `Result<T, AosError>`
- CLAUDE.md: Logging uses `tracing` macros (info!, warn!, error!)
- CLAUDE.md: Deterministic tasks use `spawn_deterministic`
- Implementation: `/Users/mln-dev/Dev/adapter-os/crates/adapteros-server/src/main.rs`
