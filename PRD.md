# PRD 1: Inference Request Circuit Breaker

### Title
Implement circuit breaker pattern for inference request handling

### Problem Statement
Inference requests that trigger adapter failures or infinite loops can cause cascading failures across the entire system, violating live inference stability requirements.

### Non-goals
- Changing existing adapter selection logic
- Implementing request queuing beyond circuit state
- Modifying inference execution protocols

### Canonical Constraints
- Must preserve existing TelemetryEvent schema and emission patterns
- Cannot modify plugin lifecycle state transitions
- Must use existing IdentityEnvelope for request correlation
- Cannot alter stack hashing or adapter loading mechanisms

### Acceptance Criteria
- [ ] Circuit breaker opens after configurable failure threshold (5 failures/60s default)
- [ ] Open circuits return fast failure responses without adapter invocation
- [ ] Half-open state allows single test request for recovery verification
- [ ] Circuit state tracked per adapter and exposed via health endpoints
- [ ] Circuit events logged via canonical telemetry with full context
- [ ] Recovery mechanisms prevent thundering herd on circuit closure

### Migration/Upgrade Notes
Existing inference requests gain circuit breaker protection automatically.

### File-level Impact List
```
crates/adapteros-core/src/circuit_breaker.rs
crates/adapteros-lora-worker/src/inference_pipeline.rs
crates/adapteros-server-api/src/handlers/inference.rs
tests/circuit_breaker_tests.rs
crates/adapteros-telemetry/src/events/circuit_breaker.rs
```
