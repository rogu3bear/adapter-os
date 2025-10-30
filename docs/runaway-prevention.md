# Runaway Process Prevention Implementation

## Overview

This document describes the comprehensive runaway process prevention mechanisms implemented in AdapterOS. The implementation follows best practices and integrates with existing policy enforcement, error handling, and telemetry systems.

## Implementation Summary

### Phase 1: Timeout Mechanisms & Circuit Breakers ✅

**Files Created:**
- `crates/aos-worker/src/timeout.rs` - Timeout and circuit breaker implementation
- `crates/aos-worker/src/health.rs` - Health monitoring and process lifecycle management
- `crates/aos-worker/src/limiter.rs` - Resource limiting and exhaustion protection
- `crates/aos-worker/src/deadlock.rs` - Deadlock detection and recovery mechanisms

**Key Features:**
- Request timeout protection with configurable timeouts per operation type
- Circuit breaker pattern to prevent cascading failures
- Health monitoring with automatic shutdown on failure
- Resource limiting with concurrent request caps and rate limiting
- Deadlock detection with automatic recovery

### Phase 2: Process Lifecycle Management ✅

**Files Modified:**
- `crates/aos-worker/src/lib.rs` - Integrated safety mechanisms into Worker
- `crates/aos-node/src/agent.rs` - Proper process termination with SIGTERM/SIGKILL

**Key Features:**
- Graceful process termination with signal handling
- Health monitoring with configurable thresholds
- Memory and CPU time tracking
- Automatic shutdown on health check failures

### Phase 3: Resource Limits & Exhaustion Protection ✅

**Key Features:**
- Concurrent request limiting with semaphore-based guards
- Token rate limiting using sliding window algorithm
- Memory usage tracking and limits
- CPU time monitoring and limits
- Request rate limiting per minute

### Phase 4: Deadlock Detection & Recovery ✅

**Key Features:**
- Lock tracking with thread and lock ID mapping
- Deadlock detection using cycle detection algorithms
- Automatic recovery with configurable timeouts
- Telemetry logging for deadlock events

### Phase 5: Monitoring Integration ✅

**Files Modified:**
- `crates/aos-policy/src/lib.rs` - Added resource limit policy checks
- `configs/cp.toml` - Added safety configuration options

**Key Features:**
- Telemetry integration with existing event system
- Policy enforcement for resource limits
- Configuration management for all safety mechanisms
- Comprehensive logging and monitoring

### Related Database Documentation

See [Database Schema Documentation](database-schema/README.md) for database structure, including:
- [Incident Response](database-schema/workflows/incident-response.md) - Incident detection and resolution workflows
- [Monitoring Flow](database-schema/workflows/monitoring-flow.md) - Real-time metrics collection and alerting
- [System Metrics Tables](database-schema/schema-diagram.md#system-metrics) - Complete schema for metrics storage

### Phase 6: Testing & Validation ✅

**Files Created:**
- `tests/runaway_prevention.rs` - Comprehensive test suite

**Test Coverage:**
- Timeout mechanism testing
- Circuit breaker behavior validation
- Resource limiter functionality
- Health monitor operation
- Deadlock detector behavior
- Integration testing with Worker

## Configuration

### Safety Configuration (`configs/cp.toml`)

```toml
[worker.safety]
# Timeout configuration
inference_timeout_secs = 30
evidence_timeout_secs = 5
router_timeout_ms = 100
policy_timeout_ms = 50

# Circuit breaker configuration
circuit_breaker_threshold = 5
circuit_breaker_timeout_secs = 60

# Resource limits
max_concurrent_requests = 10
max_tokens_per_second = 40
max_memory_per_request_mb = 50
max_cpu_time_per_request_secs = 30
max_requests_per_minute = 100

# Health monitoring
health_check_interval_secs = 30
max_response_time_secs = 60
max_memory_growth_mb = 100
max_cpu_time_secs = 300
max_consecutive_failures = 3

# Deadlock detection
deadlock_check_interval_secs = 5
max_wait_time_secs = 30
max_lock_depth = 10
recovery_timeout_secs = 10
```

## Usage Examples

### Basic Worker Usage

```rust
use aos_worker::{Worker, InferenceRequest};
use aos_telemetry::TelemetryWriter;

// Create telemetry writer
let telemetry = TelemetryWriter::new("/tmp/telemetry", 1000, 1024 * 1024)?;

// Create worker with safety mechanisms
let mut worker = Worker::new(manifest, kernels, rag, "tokenizer.json", "model.bin", telemetry)?;

// Run inference with automatic safety checks
let request = InferenceRequest {
    cpid: "test".to_string(),
    prompt: "Test prompt".to_string(),
    max_tokens: 100,
    require_evidence: false,
    request_type: Default::default(),
};

let response = worker.infer(request).await?;
```

### Manual Safety Mechanism Usage

```rust
use aos_worker::{TimeoutWrapper, CircuitBreaker, ResourceLimiter};

// Timeout wrapper
let timeout_wrapper = TimeoutWrapper::new(TimeoutConfig::default());
let result = timeout_wrapper.infer_with_timeout(async {
    // Your inference logic here
    Ok("success")
}).await?;

// Circuit breaker
let mut circuit_breaker = CircuitBreaker::new(5, Duration::from_secs(60));
let result = circuit_breaker.call(async {
    // Your operation here
    Ok("success")
}).await?;

// Resource limiter
let limiter = ResourceLimiter::new(ResourceLimits::default());
let guard = limiter.acquire_request().await?;
// Use guard - automatically released when dropped
```

## Policy Integration

The implementation integrates with existing policy enforcement:

### Memory Ruleset #12
- Memory pressure detection and eviction
- Headroom monitoring with configurable thresholds
- Automatic process termination on memory exhaustion

### Performance Ruleset #11
- Latency monitoring with p95 thresholds
- Throughput limiting with token rate controls
- Router overhead monitoring

### Isolation Ruleset #8
- Process isolation with UID/GID separation
- Capability-scoped filesystem access
- No shared memory across tenants

### Telemetry Ruleset #9
- Canonical JSON event serialization
- BLAKE3 event hashing
- Configurable sampling rates

## Error Handling

The implementation follows existing error handling patterns:

```rust
use aos_core::{AosError, Result};

// Timeout errors
Err(AosError::Worker("Operation timeout".to_string()))

// Memory pressure errors
Err(AosError::MemoryPressure("Memory limit exceeded".to_string()))

// Policy violation errors
Err(AosError::PolicyViolation("Resource limit exceeded".to_string()))
```

## Monitoring and Observability

### Telemetry Events

The implementation logs comprehensive telemetry events:

```rust
// Inference event
InferenceEvent {
    duration_ms: 1500,
    success: true,
    timeout_occurred: false,
    circuit_breaker_open: false,
    memory_usage: 1024 * 1024,
}

// Health event
HealthEvent {
    status: "healthy",
    memory_usage_bytes: 1024 * 1024,
    memory_growth_bytes: 512 * 1024,
    cpu_time_secs: 30,
    uptime_secs: 3600,
    consecutive_failures: 0,
    timestamp: 1640995200,
}

// Deadlock event
DeadlockEvent {
    lock_id: "worker_mutex",
    thread_id: 12345,
    wait_time_secs: 35,
    recovery_triggered: true,
    total_deadlocks: 1,
    timestamp: 1640995200,
}
```

### Health Checks

Health monitoring provides:
- Memory usage tracking
- CPU time monitoring
- Response time monitoring
- Automatic failure detection
- Graceful shutdown on critical failures

## Testing

### Running Tests

```bash
# Run all runaway prevention tests
cargo test --test runaway_prevention

# Run specific test
cargo test --test runaway_prevention test_circuit_breaker

# Run with output
cargo test --test runaway_prevention -- --nocapture
```

### Test Coverage

- ✅ Timeout mechanism testing
- ✅ Circuit breaker behavior validation
- ✅ Resource limiter functionality
- ✅ Health monitor operation
- ✅ Deadlock detector behavior
- ✅ Integration testing with Worker
- ✅ Concurrent request handling
- ✅ Memory pressure handling
- ✅ Telemetry integration

## Dependencies

### New Dependencies Added

```toml
# aos-worker/Cargo.toml
tokio-util = { version = "0.7", features = ["time"] }

# aos-node/Cargo.toml
nix = { workspace = true }  # For signal handling
```

## Best Practices Followed

1. **Error Handling**: Consistent use of `AosError` and `Result<T>` types
2. **Telemetry**: Integration with existing telemetry system
3. **Policy Enforcement**: Alignment with 22 policy packs
4. **Configuration**: TOML-based configuration management
5. **Testing**: Comprehensive test coverage
6. **Documentation**: Inline documentation and examples
7. **Performance**: Efficient algorithms and data structures
8. **Safety**: Memory-safe Rust implementation
9. **Observability**: Rich telemetry and logging
10. **Maintainability**: Clear separation of concerns

## Future Enhancements

1. **Dynamic Configuration**: Runtime configuration updates
2. **Advanced Deadlock Detection**: More sophisticated cycle detection
3. **Resource Prediction**: ML-based resource usage prediction
4. **Distributed Monitoring**: Cross-node health monitoring
5. **Automated Recovery**: Self-healing mechanisms
6. **Performance Optimization**: Further performance improvements
7. **Security Hardening**: Additional security measures
8. **Compliance**: Enhanced compliance reporting

## Conclusion

The runaway process prevention implementation provides comprehensive protection against:

- **Memory Exhaustion**: Through monitoring and limits
- **Infinite Loops**: Through timeouts and circuit breakers
- **Resource Exhaustion**: Through rate limiting and quotas
- **Deadlocks**: Through detection and recovery
- **Process Hangs**: Through health monitoring and termination
- **Cascading Failures**: Through circuit breaker patterns

The implementation follows AdapterOS best practices and integrates seamlessly with existing systems while providing robust protection against runaway processes.
