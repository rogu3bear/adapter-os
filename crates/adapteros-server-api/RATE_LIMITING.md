# Telemetry Rate Limiting and Backpressure Implementation

## Overview

This document describes the production-grade rate limiting and backpressure mechanisms implemented in the adapterOS telemetry system.

## Components

### 1. Token Bucket Rate Limiter

**Location**: `TokenBucket` struct in `telemetry/mod.rs`

**Algorithm**: Token Bucket with per-tenant rate limiting

**Features**:
- Token-based rate limiting per tenant
- Configurable events per second (default: 1000)
- Burst capacity support (default: 10000)
- Lock-free atomic operations for token consumption
- Automatic refill based on elapsed time

**Configuration**:
```rust
pub struct RateLimitConfig {
    /// Maximum events per second per tenant (default: 1000)
    pub events_per_second: u64,
    /// Refill interval for token bucket (milliseconds)
    pub refill_interval_ms: u64,
    /// Maximum burst capacity (default: 10000)
    pub burst_capacity: u64,
}
```

**How It Works**:
- Each tenant gets their own token bucket
- Tokens refill at: `(events_per_second / 1000ms) * elapsed_ms`
- Each event consumes 1 token
- If no tokens available, event is rejected with rate limit error
- Metric tracking: `rate_limit_drops` counter

### 2. Backpressure Detection

**Location**: `BackpressureDetector` struct in `telemetry/mod.rs`

**Algorithm**: Queue depth threshold monitoring

**Features**:
- Monitors buffer queue depth
- Triggers when queue exceeds threshold (default: buffer_size / 2)
- Prevents slow clients from causing head-of-line blocking
- Metric tracking: `backpressure_drops` counter

**Usage Pattern**:
```rust
let detector = BackpressureDetector::new(max_queue_depth);

// Check if we should apply backpressure
if detector.should_apply_backpressure(current_queue_depth) {
    // Reject event to prevent queue growth
}

// Track dropped events
detector.record_dropped_event();
```

### 3. TelemetryBuffer with Integrated Limits

**Location**: `TelemetryBuffer` struct in `telemetry/mod.rs`

**Enhancement**: Original buffer enhanced with rate limiting and backpressure

**Push Method Features**:
1. Per-tenant token bucket consumption check
2. Backpressure queue depth check
3. Automatic bucket creation for new tenants
4. Event dropping with detailed logging
5. Health metric updates

**Code Flow**:
```
push(event)
  ├─> Extract tenant_id
  ├─> Check rate limit (token bucket)
  │   └─> If rate limited: record drop, return error
  ├─> Lock buffer
  ├─> Check backpressure (queue depth)
  │   └─> If backpressure: record drop, return error
  └─> Accept event (with eviction if needed)
```

### 4. Health Checker Metrics

**Location**: `TelemetryHealthChecker` struct in `telemetry/mod.rs`

**New Metrics**:
- `rate_limit_drops`: Events dropped due to rate limiting
- `backpressure_drops`: Events dropped due to backpressure
- Health status reflects these metrics

**Health Status**:
- `Healthy`: All subsystems normal
- `Degraded`: Buffer > 90% or rate limit drops > 100
- `Unhealthy`: Circuit breaker open

## Configuration

### Default Configuration

```rust
impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            events_per_second: 1000,      // 1000 events/sec per tenant
            refill_interval_ms: 100,      // Refill every 100ms
            burst_capacity: 10000,        // Allow bursts up to 10k
        }
    }
}
```

### Custom Configuration

```rust
let custom_config = RateLimitConfig {
    events_per_second: 500,   // Lower rate limit
    refill_interval_ms: 100,  // Same refill interval
    burst_capacity: 5000,     // Lower burst
};

let buffer = TelemetryBuffer::with_config(10000, custom_config);
```

## API Usage

### Creating a Buffer with Rate Limiting

```rust
// Default configuration (1000 events/sec per tenant)
let buffer = TelemetryBuffer::new(10000);

// Custom configuration
let config = RateLimitConfig {
    events_per_second: 500,
    refill_interval_ms: 100,
    burst_capacity: 5000,
};
let buffer = TelemetryBuffer::with_config(10000, config);
```

### Pushing Events

```rust
match buffer.push(event).await {
    Ok(()) => {
        // Event accepted
    }
    Err(e) => {
        match e.as_str() {
            msg if msg.contains("Rate limit exceeded") => {
                // Event rate limited for this tenant
            }
            msg if msg.contains("Backpressure") => {
                // Buffer queue depth exceeded
            }
            _ => {
                // Other error
            }
        }
    }
}
```

### Accessing Metrics

```rust
// Get backpressure metrics
let (drops, max_depth) = buffer.backpressure_metrics();
println!("Events dropped: {}, Max queue: {}", drops, max_depth);

// Get rate limit config
let config = buffer.rate_limit_config();
println!("Rate limit: {}/sec", config.events_per_second);

// Get health metrics
let health_checker = buffer.health_checker();
let metrics = health_checker.check().await;
println!(
    "Health: {:?}, Rate drops: {}, Backpressure drops: {}",
    metrics.status,
    metrics.rate_limit_drops,
    metrics.backpressure_drops
);
```

## Behavior

### Rate Limit Enforcement

**Per-Tenant Isolation**:
- Each tenant has independent rate limit
- Prevents one tenant from affecting others
- Automatic bucket creation on first event

**Token Refill**:
- Tokens refill at configured rate
- Refill only happens when events are pushed (lazy)
- No background thread needed

**Burst Support**:
- Allows temporary bursts up to burst_capacity
- Useful for event spikes
- Still respects per-second average

### Backpressure Behavior

**Queue Monitoring**:
- Tracks current queue depth in memory
- Threshold: `max_size / 2` (default: 5000 for 10k buffer)
- Prevents unbounded queue growth

**Event Rejection**:
- When threshold exceeded, new events rejected
- Clients won't accept slow streams
- Prevents cascading failures

**Logging**:
- Rate limit drops: WARN level
- Backpressure drops: WARN level
- Normal acceptance: INFO level

## Examples

### Example 1: Basic Usage with Defaults

```rust
let buffer = TelemetryBuffer::new(10000);

// First tenant, first event - accepted
match buffer.push(event1.clone()).await {
    Ok(()) => println!("Event accepted"),
    Err(e) => println!("Error: {}", e),
}

// Same tenant, rapid fire events - some may be rate limited
for i in 0..2000 {
    let _ = buffer.push(event.clone()).await;
}

// Check health
let health = buffer.health_checker().check().await;
if health.rate_limit_drops > 0 {
    println!("Rate limited: {} events", health.rate_limit_drops);
}
```

### Example 2: Custom Rate Limiting

```rust
// Stricter rate limit for specific use case
let config = RateLimitConfig {
    events_per_second: 100,  // Only 100/sec
    refill_interval_ms: 100,
    burst_capacity: 1000,
};

let buffer = TelemetryBuffer::with_config(5000, config);

// Same event rejected faster
match buffer.push(event).await {
    Ok(()) => {},
    Err(e) if e.contains("Rate limit") => {
        println!("Hit rate limit");
    }
    _ => {}
}
```

### Example 3: Monitoring Health

```rust
let buffer = TelemetryBuffer::new(10000);
let health_checker = buffer.health_checker();

loop {
    let metrics = health_checker.check().await;

    match metrics.status {
        TelemetryHealth::Healthy => {
            println!("All systems nominal");
        }
        TelemetryHealth::Degraded => {
            println!(
                "Degraded: {} rate limit drops, {} backpressure drops",
                metrics.rate_limit_drops,
                metrics.backpressure_drops
            );
        }
        TelemetryHealth::Unhealthy => {
            println!("Critical: telemetry system unhealthy");
            // Take remedial action
        }
    }

    tokio::time::sleep(Duration::from_secs(10)).await;
}
```

## Performance Characteristics

### Time Complexity
- `push()`: O(1) amortized (lock-free token check + buffer write)
- `query()`: O(n) where n is buffer size (read all, filter, sort)
- Rate limit check: O(1) atomic operations

### Space Complexity
- Per-tenant bucket: ~120 bytes (Arc, Mutex, AtomicU64)
- Per-event: stored in buffer
- Total: O(num_tenants + num_events)

### Lock Contention
- Minimal lock contention on rate limiter read path
- Write path acquires bucket write lock briefly
- Backpressure check inside event buffer write lock

## Testing

Unit tests included in `telemetry/mod.rs`:

```bash
cargo test --lib telemetry --features=
```

Tests cover:
- `test_rate_limit_config()`: Config creation
- `test_backpressure_detector()`: Queue threshold detection
- `test_token_bucket()`: Token consumption and refill
- `test_telemetry_buffer()`: Buffer basic operations

## Production Deployment

### Configuration Recommendations

**High-throughput systems**:
```rust
RateLimitConfig {
    events_per_second: 5000,
    refill_interval_ms: 100,
    burst_capacity: 50000,
}
```

**Standard deployments**:
```rust
RateLimitConfig::default()  // 1000 events/sec
```

**Low-resource systems**:
```rust
RateLimitConfig {
    events_per_second: 100,
    refill_interval_ms: 100,
    burst_capacity: 1000,
}
```

### Monitoring

Track these metrics in production:
1. `telemetry_rate_limit_drops_total`: Indicates noisy tenants
2. `telemetry_backpressure_drops_total`: Indicates slow clients
3. `telemetry_buffer_queue_depth`: Current queue state
4. `telemetry_health_status`: Overall system health

### Alerting

Set up alerts for:
1. Rate limit drops > 100/minute: Review tenant quotas
2. Backpressure drops > 50/minute: Check client capacity
3. Health status = Unhealthy: Immediate investigation
4. Buffer utilization > 90%: Risk of data loss

## Troubleshooting

### Events Mysteriously Disappearing

**Symptom**: Events pushed but not in buffer

**Causes**:
1. Rate limiting - check `rate_limit_drops` metric
2. Backpressure - check `backpressure_drops` metric
3. Buffer full eviction - check buffer size configuration

**Solution**:
1. Increase `events_per_second` in rate limit config
2. Increase buffer size (`max_size` parameter)
3. Implement event sampling on client side

### High Memory Usage

**Symptom**: Telemetry buffer consuming excessive RAM

**Causes**:
1. Buffer size too large
2. Slow consumers not draining events
3. Backpressure not rejecting enough events

**Solution**:
1. Reduce buffer size
2. Ensure flush() is called regularly
3. Lower backpressure threshold

### Circuit Breaker Open

**Symptom**: Health status = Unhealthy

**Causes**:
1. Persistence layer failures
2. Database timeouts
3. Multiple retry failures

**Solution**:
1. Check database health
2. Review persistence failures metric
3. Wait for half-open recovery or restart

## Future Enhancements

1. Dynamic rate limit adjustment based on load
2. Adaptive backpressure with exponential decay
3. Per-event-type rate limiting
4. Client-specific quotas (not just tenant)
5. Graduated response (warn → throttle → drop)
6. Metrics export to Prometheus
