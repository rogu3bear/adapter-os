# Telemetry System Architecture and Implementation Details

## Component Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      Incoming Telemetry Event                   │
└───────────────────────────┬─────────────────────────────────────┘
                            │
                            ▼
        ┌───────────────────────────────────────┐
        │  Rate Limit Check (Token Bucket)      │
        │  Per-tenant: 1000 events/sec max      │
        │  Burst capacity: 10000                │
        └───────────────┬───────────────────────┘
                        │
            ┌───────────┴────────────┐
            │                        │
       Rate Limited?            Continue
            │                       │
            ▼                       ▼
        [DLQ]              ┌──────────────────┐
      Track Drop          │ Backpressure     │
                          │ Check            │
                          │ Max: size/2      │
                          └───────┬──────────┘
                                  │
                        ┌─────────┴───────────┐
                        │                     │
                    Backpressured?        Continue
                        │                    │
                        ▼                    ▼
                      [DLQ]           ┌─────────────────┐
                    Track Drop        │ Add to Buffer   │
                                      │ RwLock<Vec>     │
                                      │ Max: 10000      │
                                      └────────┬────────┘
                                               │
                                    ┌──────────┴──────────┐
                                    │                     │
                               Success             Evict oldest
                                    │              if needed
                                    ▼                     │
                            ┌──────────────┐            ▼
                            │ Update Metrics
                            │ - Buffer util
                            │ - Count accepted
                            └──────────────┘
```

## Core Components

### 1. TelemetryBuffer

Main entry point for telemetry events with integrated error recovery.

**Key State**:
```rust
pub struct TelemetryBuffer {
    events: Arc<RwLock<Vec<TelemetryEvent>>>,        // Main buffer
    max_size: usize,                                  // 10,000 default
    rate_limiters: Arc<RwLock<HashMap<...>>>,       // Per-tenant buckets
    rate_limit_config: Arc<RateLimitConfig>,         // 1000 eps/tenant
    backpressure_detector: Arc<BackpressureDetector>, // Queue monitoring
    health_checker: Arc<TelemetryHealthChecker>,     // Status tracking
}
```

**Critical Methods**:

```rust
/// Add event with full error recovery pipeline
pub async fn push(&self, event: TelemetryEvent) -> Result<(), String> {
    let tenant_id = event.identity.tenant_id.clone();

    // Step 1: Rate limit per tenant
    {
        let rate_limiters = self.rate_limiters.read().await;
        if let Some(bucket) = rate_limiters.get(&tenant_id) {
            if !bucket.try_consume().await {
                self.health_checker.record_rate_limit_drop();
                warn!("Rate limit exceeded");
                return Err("Rate limit exceeded");
            }
        } else {
            // Create new bucket for tenant
            let bucket = TokenBucket::new(&self.rate_limit_config);
            if !bucket.try_consume().await {
                self.health_checker.record_rate_limit_drop();
                return Err("Rate limit exceeded");
            }
            self.rate_limiters.write().await.insert(tenant_id.clone(), bucket);
        }
    }

    // Step 2: Check backpressure
    let mut events = self.events.write().await;
    if self.backpressure_detector.should_apply_backpressure(events.len()) {
        self.health_checker.record_backpressure_drop();
        warn!("Backpressure: queue full");
        return Err("Queue depth exceeded");
    }

    // Step 3: Evict if needed
    if events.len() >= self.max_size {
        events.remove(0);
    }

    // Step 4: Add event
    events.push(event);
    Ok(())
}
```

### 2. DeadLetterQueue

Persistent store for failed events with retry tracking.

**Data Structure**:
```rust
pub struct DeadLetterEvent {
    pub event: TelemetryEvent,          // Original event
    pub retry_attempts: u32,            // Retry counter
    pub last_error: String,             // Most recent error
    pub enqueued_at: u64,               // Entry timestamp
    pub last_retry_at: Option<u64>,     // Last retry attempt
}
```

**Internal Storage**:
```rust
VecDeque<DeadLetterEvent>  // Max 5000 events
                           // FIFO: oldest removed first
```

**Lifecycle**:
1. Event fails to be accepted/processed
2. `enqueue()` adds to DLQ with error context
3. `list_events()` retrieves for manual inspection
4. `retry_event()` updates retry metadata
5. `remove()` deletes after successful processing

### 3. TelemetryHealthChecker

Monitors system health with atomic counters and circuit breaker integration.

**Health Determination Algorithm**:
```rust
pub async fn check(&self) -> TelemetryHealthMetrics {
    let cb_metrics = self.circuit_breaker.metrics();

    match cb_metrics.state {
        CircuitState::Open { .. } => {
            // Database unavailable - UNHEALTHY
            TelemetryHealth::Unhealthy
        },
        CircuitState::HalfOpen => {
            // In recovery - DEGRADED
            TelemetryHealth::Degraded
        },
        CircuitState::Closed => {
            // Check secondary conditions
            if buffer_util > 0.9 ||           // >90% full
               persistence_failures > 100 ||  // Many DB failures
               rate_limit_drops > 100 {       // Heavy throttling
                TelemetryHealth::Degraded
            } else {
                TelemetryHealth::Healthy
            }
        }
    }
}
```

**Metrics Tracked**:
- Buffer utilization (% full)
- Circuit breaker state
- Total dropped events
- Persistence failures
- Rate limit drops
- Backpressure drops
- Last check timestamp

### 4. TokenBucket (Rate Limiter)

Implements token bucket algorithm per tenant.

**Algorithm**:
```rust
async fn try_consume(&self) -> bool {
    // 1. Calculate elapsed time since last refill
    let elapsed_ms = now - last_refill;

    // 2. Add tokens if enough time passed
    if elapsed_ms >= refill_interval_ms {
        let tokens_to_add = (rate * elapsed_ms) / 1000;
        current_tokens = (current_tokens + tokens_to_add).min(capacity);
        last_refill = now;
    }

    // 3. Try to consume one token atomically
    if current_tokens > 0 {
        current_tokens -= 1;
        return true;  // Allowed
    }
    false  // Rate limited
}
```

**Configuration**:
```rust
RateLimitConfig {
    events_per_second: 1000,      // Base rate
    refill_interval_ms: 100,      // Check every 100ms
    burst_capacity: 10000,        // Max burst
}
```

### 5. BackpressureDetector

Detects and handles queue congestion.

**Trigger Logic**:
```rust
pub fn should_apply_backpressure(&self, current_queue_depth: usize) -> bool {
    current_queue_depth >= self.max_queue_depth
}

// where max_queue_depth = buffer_size / 2
```

**Purpose**: Prevent memory exhaustion from slow SSE clients that can't consume fast enough.

## State Transitions

### Circuit Breaker State Machine

```
                ┌─────────┐
                │ Closed  │ (normal operation)
                └────┬────┘
                     │
         Failures ≥ threshold
                     │
                     ▼
            ┌─────────────────┐
            │ Open            │ (reject requests)
            │ until=now+30s   │
            └────────┬────────┘
                     │
          Timeout elapsed?
                     │
                     ▼
            ┌─────────────────┐
            │ HalfOpen        │ (test recovery)
            │ max 5 requests  │
            └────┬─────────┬──┘
                 │         │
        Successes≥3    Any failure
                 │         │
                 ▼         ▼
            Closed       Open
```

### Health Status Transitions

```
        ┌──────────┐
        │ Healthy  │ (CB closed, <90% buf, <100 fails)
        └─────┬────┘
              │
      (CB half-open OR >90% buf OR >100 fails)
              │
              ▼
        ┌─────────────┐
        │ Degraded    │ (limited operation)
        └─────┬───────┘
              │
      (CB open for 30 seconds)
              │
              ▼
        ┌──────────────┐
        │ Unhealthy    │ (service down)
        └──────────────┘
```

## Error Handling Strategies

### Strategy 1: Rate Limit Exceeded
```
Incoming Event with rate limit exceeded
         ↓
   record_rate_limit_drop()
   health_checker.rate_limit_drops += 1
         ↓
   Return Err("Rate limit exceeded")
         ↓
   Caller decides: retry, queue to DLQ, or discard
```

### Strategy 2: Backpressure Applied
```
Incoming Event with queue > max_depth
         ↓
   record_backpressure_drop()
   backpressure_detector.events_dropped += 1
         ↓
   Return Err("Backpressure: buffer queue depth exceeded")
         ↓
   Caller receives immediate rejection
   Prevents slow clients from blocking
```

### Strategy 3: Buffer Full
```
Incoming Event when len() == max_size
         ↓
   Evict oldest event
   (FIFO, loses least recent data)
         ↓
   Insert new event
         ↓
   Return Ok(())
   ↓
   Event accepted but oldest was dropped
```

### Strategy 4: Circuit Breaker Open
```
Database write fails 5 times in a row
         ↓
   circuit_breaker.transition_to(Open)
         ↓
   Next push() call:
   - Detects CB is open
   - Rejects adding to buffer
   - Routes to DLQ for later retry
         ↓
   Health status: Unhealthy
         ↓
   Wait 30 seconds, transition to HalfOpen
   Test recovery with limited writes
```

## Integration Points

### With AppState

```rust
pub struct AppState {
    pub telemetry_buffer: TelemetryBuffer,
    pub telemetry_tx: TelemetrySender,  // For broadcasting
    pub db: Arc<dyn DatabaseBackend>,
    // ... other fields
}
```

### With Handlers

```rust
pub async fn stream_logs(
    State(state): State<AppState>,
) -> Sse<impl Stream> {
    // Health check before streaming
    let health = state.telemetry_buffer.health_metrics().await;
    if health.status == TelemetryHealth::Unhealthy {
        warn!("Telemetry system unhealthy, degraded service");
        // Implement reduced-feature streaming
    }

    // Subscribe to telemetry channel
    let rx = state.telemetry_tx.subscribe();
    // Stream with filtering and rate limiting
}
```

### With Database

```rust
async fn flush_to_database(buffer: &TelemetryBuffer, db: &Db) -> Result<()> {
    let events = buffer.flush().await;

    match db.write_events(&events).await {
        Ok(_) => {
            buffer.record_write_success().await;
            Ok(())
        },
        Err(e) => {
            buffer.record_write_failure();
            // Events moved to DLQ for retry
            Err(e)
        }
    }
}
```

## Performance Tuning

### Buffer Size
```rust
// Current: 10,000 events
let buffer = TelemetryBuffer::new(10000);

// Tune based on:
// - Memory available
// - Event size (~500 bytes average)
// - Flush frequency
// - Peak traffic

// Monitor health_metrics.buffer_utilization_percent
if utilization > 80% {
    // Consider increasing buffer size or decreasing flush interval
}
```

### Rate Limit
```rust
let config = RateLimitConfig {
    events_per_second: 1000,  // Adjust per tenant capacity
    refill_interval_ms: 100,  // Lower = more precise
    burst_capacity: 10000,    // Allow burst traffic
};

let buffer = TelemetryBuffer::with_config(10000, config);
```

### Circuit Breaker Tuning
```rust
CircuitBreakerConfig {
    failure_threshold: 5,            // Open after 5 failures
    success_threshold: 3,            // Close after 3 successes
    timeout_ms: 30000,               // 30 seconds recovery time
    half_open_max_requests: 5,       // Test with 5 requests
}

// Monitor via circuit_breaker_metrics()
let metrics = buffer.circuit_breaker_metrics();
if metrics.opens_total > 10 {
    // Database issues detected, increase timeout or failure threshold
}
```

## Observability

### Key Metrics to Monitor

1. **Buffer Health**:
   - `health_metrics().buffer_utilization_percent`
   - Should stay <80%
   - Spike indicates write congestion

2. **Drop Rates**:
   - `health_metrics().rate_limit_drops`
   - `health_metrics().backpressure_drops`
   - `health_metrics().events_dropped_total`
   - Indicates traffic exceeds capacity

3. **Persistence**:
   - `health_metrics().persistence_failures_total`
   - Indicates database issues
   - Triggers health downgrade

4. **Circuit Breaker**:
   - `circuit_breaker_metrics().state`
   - `circuit_breaker_metrics().opens_total`
   - Shows database availability

5. **DLQ**:
   - `health_metrics().dlq_size`
   - Number of pending retries
   - Size > 100 indicates persistent issues

### Logging Output

```
WARN: Telemetry event dropped due to rate limit
     tenant_id=tenant-123
     max_rate=1000 events/sec

WARN: Telemetry event dropped due to backpressure
     tenant_id=tenant-456
     queue_depth=5000
     max_queue_depth=5000

WARN: Telemetry buffer full and circuit breaker open
     routing to DLQ

INFO: Telemetry event accepted
     tenant_id=tenant-789
     queue_depth=1234
```

## Testing Strategy

### Unit Tests Included
1. Basic buffer operations
2. DLQ enqueue/dequeue
3. Rate limiting per tenant
4. Backpressure detection
5. Health status transitions
6. Circuit breaker state management
7. Token bucket refill accuracy

### Integration Test Areas
1. Full request flow with all validations
2. Multiple tenants simultaneously
3. Stress with high-frequency events
4. Circuit breaker recovery scenarios
5. DLQ retry mechanisms

### Load Testing Recommendations
```
Scenario 1: Normal Load
- 500 events/sec total
- 100 tenants (5 events/sec each)
- Expected: Health=Healthy

Scenario 2: One Tenant Overload
- Tenant-A: 2000 events/sec (exceeds limit)
- Other: 100 events/sec
- Expected: Rate limit drops only for Tenant-A

Scenario 3: Database Failure
- Write errors to database
- Expected: CB opens, events to DLQ, Health=Unhealthy
- After recovery: CB half-open, gradual recovery

Scenario 4: Slow SSE Consumer
- One SSE client very slow
- Expected: Backpressure drops, health degrades
- Unaffected: Other clients and buffer operations
```
