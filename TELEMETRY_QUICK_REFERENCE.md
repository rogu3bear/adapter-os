# Telemetry System - Quick Reference Guide

## File Location
`/Users/star/Dev/aos/crates/adapteros-server-api/src/telemetry/mod.rs` (1010 lines)

## 7 Core Features Added

### 1. Circuit Breaker for Database Writes
**Prevents cascading failures when database goes down**

```rust
// Automatically integrated in TelemetryBuffer
pub fn circuit_breaker_state(&self) -> String
pub fn circuit_breaker_metrics(&self) -> CircuitBreakerMetrics

// States: closed (normal), open (failing), half-open (recovering)
// Threshold: 5 failures → open, 3 successes → close
// Recovery timeout: 30 seconds
```

### 2. Dead Letter Queue (DLQ)
**Stores failed events for later retry**

```rust
pub async fn list_dlq_events(&self) -> Vec<DeadLetterEvent>
pub async fn retry_dlq_events(&self) -> usize
pub async fn dlq_size(&self) -> usize

// Max capacity: 5000 events
// Tracks: retry attempts, last error, timestamps
```

### 3. Exponential Backoff
**Smart retry strategy with jitter**

```rust
// Configured automatically
RetryPolicy::database("telemetry")
// Max attempts: 3, base delay: 200ms, max delay: 10s
// Backoff factor: 1.5, jitter: enabled
```

### 4. Health Checks
**Monitor system health with status levels**

```rust
pub async fn health_metrics(&self) -> TelemetryHealthMetrics

// Returns:
// - status: Healthy / Degraded / Unhealthy
// - buffer_utilization_percent: 0-100
// - circuit_breaker_state: "closed" / "open" / "half_open"
// - events_dropped_total: count
// - persistence_failures_total: count
// - rate_limit_drops: count
// - backpressure_drops: count
// - dlq_size: count
```

### 5. Rate Limiting (Token Bucket)
**Fair resource allocation per tenant**

```rust
// Configured: 1000 events/sec per tenant
// Burst: 10000 events allowed
// Refill: every 100ms

// Rejected events tracked in health metrics
health_metrics().rate_limit_drops
```

### 6. Backpressure Detection
**Protects against slow SSE consumers**

```rust
// Trigger: queue depth > buffer_size / 2
// Events dropped if threshold exceeded
// Protects memory from unbounded growth

pub fn backpressure_metrics(&self) -> (usize, usize)
```

### 7. Graceful Degradation
**Continue operating under stress**

```
Request flow:
1. Check rate limit (1000 eps/tenant)
2. Check backpressure (queue < max/2)
3. Add to buffer (evict oldest if full)
4. Update metrics

Failure modes:
- Rate limited → Drop, record metric
- Backpressured → Drop, record metric
- CB open → Route to DLQ, record metric
- CB half-open → Test write with limited requests
```

## Health Status Thresholds

| Condition | Status | Action |
|-----------|--------|--------|
| Circuit Open | Unhealthy | Reject writes, use DLQ |
| Circuit Half-Open | Degraded | Limited test writes |
| Buffer > 90% | Degraded | Monitor closely |
| Failures > 100 | Degraded | Check database |
| Rate limit drops > 100 | Degraded | Tenant overload |
| All good | Healthy | Normal operation |

## Integration Checklist

- [ ] TelemetryBuffer integrated in AppState
- [ ] Health check endpoint added to API
- [ ] Monitoring dashboard updated with health metrics
- [ ] Alerts configured for Unhealthy status
- [ ] DLQ monitoring and manual retry process documented
- [ ] Rate limit per tenant validated
- [ ] Database circuit breaker working
- [ ] Tests passing

## Resources

- **Full Documentation**: `TELEMETRY_RESILIENCE.md`
- **Architecture Deep Dive**: `TELEMETRY_ARCHITECTURE.md`
- **Source Code**: `src/telemetry/mod.rs`
