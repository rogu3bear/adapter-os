# MLX FFI Error Handling Guide

## Overview

This document provides comprehensive guidance on error handling, recovery mechanisms, and troubleshooting for the MLX FFI backend.

## Error Categories

### 1. GPU Out of Memory (GPU_OOM)

**Symptoms:**
- `MlxError::GpuOomError` returned
- Error messages containing "out of memory", "allocation failed"
- Operations fail after loading large models/adapters

**Common Causes:**
- Too many adapters loaded simultaneously
- Large batch sizes
- Insufficient GPU memory for model size
- Memory leaks (rare in MLX)

**Recovery Steps:**

1. **Automatic Recovery (via RecoveryManager):**
   ```rust
   use adapteros_lora_mlx_ffi::recovery::RecoveryManager;

   let recovery = RecoveryManager::new(4096.0); // 4GB limit

   // Automatically attempt recovery before allocation
   recovery.check_and_recover(Some(&backend), required_mb)?;
   ```

2. **Manual Recovery:**
   ```rust
   // Step 1: Trigger garbage collection
   use adapteros_lora_mlx_ffi::memory;
   memory::gc_collect();

   // Step 2: Unload unused adapters
   backend.unload_adapter_runtime(adapter_id)?;

   // Step 3: Check memory stats
   let stats = memory::stats();
   println!("Memory: {:.2} MB", memory::bytes_to_mb(stats.total_bytes));
   ```

3. **Preventive Measures:**
   - Set memory thresholds: `validate_memory_available(required_mb, threshold_mb, "operation")?`
   - Monitor usage: `if memory::exceeds_threshold(max_mb) { /* cleanup */ }`
   - Use smaller adapters (reduce rank/alpha)

**Configuration:**
```rust
// Set conservative memory limits
let max_memory_mb = 3072.0; // Leave 1GB headroom on 4GB GPU
let recovery = RecoveryManager::new(max_memory_mb);
```

---

### 2. Shape Mismatch Errors

**Symptoms:**
- `MlxError::ShapeMismatch` returned
- "dimension mismatch", "incompatible shapes"
- Matrix multiplication failures

**Common Causes:**
- Input tensor shape doesn't match model expectations
- LoRA weights incompatible with model dimensions
- Incorrect reshaping operations

**Validation:**
```rust
use adapteros_lora_mlx_ffi::validation;

// Validate shape before operation
validation::validate_shape(&actual, &expected, "matmul input")?;

// Validate matmul compatibility
validation::validate_matmul_shapes(&a_shape, &b_shape, "lora_forward")?;

// Validate broadcasting compatibility
validation::validate_broadcastable(&shape1, &shape2, "element_wise_add")?;
```

**Troubleshooting:**
```rust
// Log detailed shape information
tracing::error!(
    expected_shape = ?expected,
    actual_shape = ?actual,
    operation = "lora_projection",
    "Shape mismatch detected"
);
```

---

### 3. Model Loading Errors

**Symptoms:**
- `MlxError::ModelLoadError` returned
- Missing config.json or weight files
- Malformed JSON configuration

**Pre-flight Checks:**
```rust
use adapteros_lora_mlx_ffi::validation::ModelLoadChecks;

let checks = ModelLoadChecks::run(model_path);

if !checks.is_valid() {
    for error in checks.errors() {
        eprintln!("Validation error: {}", error);
    }
    return Err(/* ... */);
}

if let Some(mb) = checks.estimated_memory_mb {
    println!("Estimated memory: {:.2} MB", mb);
    // Check if sufficient memory available
}
```

**Common Issues:**

| Issue | Check | Solution |
|-------|-------|----------|
| Missing config.json | `checks.has_config` | Verify model directory structure |
| Missing weights | `checks.has_weights` | Download model.safetensors |
| Invalid JSON | `checks.config_valid` | Validate JSON syntax |
| Insufficient memory | `checks.estimated_memory_mb` | Free memory or use smaller model |

**Retry Strategy:**
```rust
use adapteros_lora_mlx_ffi::retry::{RetryConfig, retry_with_backoff_sync};

let config = RetryConfig::model_loading();

let model = retry_with_backoff_sync(&config, "model_load", || {
    MLXFFIModel::load(model_path)
})?;
```

---

### 4. Adapter Loading Errors

**Symptoms:**
- `MlxError::AdapterLoadError` or `AdapterNotFound`
- Missing adapter in registry
- Invalid .aos file format

**Validation:**
```rust
use adapteros_lora_mlx_ffi::validation;

// Validate adapter ID
validation::validate_adapter_id(adapter_id)?;

// Validate LoRA configuration
validation::validate_lora_config(rank, alpha, dropout)?;

// Validate gates for routing
validation::validate_gates_q15(&gates, num_adapters)?;
```

**Error Handling:**
```rust
match backend.load_adapter_runtime(adapter_id, adapter) {
    Ok(_) => tracing::info!("Adapter loaded successfully"),
    Err(MlxError::AdapterLoadError { recoverable: true, .. }) => {
        // Retry with backoff
        retry_with_backoff_sync(&config, "adapter_load", || {
            backend.load_adapter_runtime(adapter_id, adapter.clone())
        })?;
    }
    Err(e) => {
        tracing::error!("Non-recoverable adapter load error: {}", e);
        return Err(e);
    }
}
```

---

## Retry Mechanisms

### Exponential Backoff

```rust
use adapteros_lora_mlx_ffi::retry::{RetryConfig, retry_with_backoff_sync};

// Default config: 3 attempts, 100ms initial, 5s max
let config = RetryConfig::default();

// Transient errors: 5 attempts, shorter backoff
let config = RetryConfig::transient();

// Resource exhaustion: longer backoff
let config = RetryConfig::resource_exhaustion();

// Custom configuration
let config = RetryConfig {
    max_attempts: 5,
    initial_backoff_ms: 200,
    max_backoff_ms: 10000,
    backoff_multiplier: 2.0,
    jitter: true,
};

let result = retry_with_backoff_sync(&config, "operation_name", || {
    // Your operation here
    potentially_failing_operation()
})?;
```

### Retry Decision Tree

```
Operation Failed
    ├─ Is error recoverable?
    │  ├─ Yes → Retry with backoff
    │  └─ No → Fail immediately
    │
    ├─ Retry attempt < max_attempts?
    │  ├─ Yes → Wait backoff duration
    │  └─ No → Return RetryExhausted error
    │
    └─ Success? → Return result
```

**Recoverable Errors:**
- GPU_OOM
- Timeout
- AllocationFailed
- FfiError (some cases)
- IO_ERROR (network, file access)

**Non-Recoverable Errors:**
- ShapeMismatch
- DtypeMismatch
- ValidationError
- ConfigError
- CircuitBreakerOpen

---

## Circuit Breaker Pattern

### Purpose
Prevent cascade failures by "opening" after repeated failures, giving the system time to recover.

### States

```
Closed (Normal) ──[failures ≥ threshold]──> Open (Blocking)
                                               │
                  Half-Open (Testing) <────[timeout]
                       │
            [2 successes]│              [failure]
                       │              │
                       ↓              ↓
                  Closed            Open
```

### Usage

```rust
use adapteros_lora_mlx_ffi::retry::CircuitBreaker;

// Create breaker: 3 failures threshold, 5s timeout
let breaker = CircuitBreaker::new("model_inference", 3, 5000);

// Execute through breaker
match breaker.call(|| {
    backend.run_inference(input)
}) {
    Ok(result) => println!("Success: {:?}", result),
    Err(MlxError::CircuitBreakerOpen { retry_after_ms, .. }) => {
        println!("Circuit open, retry after {}ms", retry_after_ms);
    }
    Err(e) => println!("Operation failed: {}", e),
}

// Monitor state
println!("Circuit state: {}", breaker.state());
println!("Failure count: {}", breaker.failure_count());

// Manual reset (admin operation)
breaker.reset();
```

### Configuration Guidelines

| Operation Type | Failure Threshold | Timeout (ms) | Rationale |
|----------------|-------------------|--------------|-----------|
| Inference | 3-5 | 5000-10000 | Quick recovery, user-facing |
| Model Loading | 2-3 | 30000-60000 | Expensive, allow time to stabilize |
| Adapter Swapping | 3-5 | 2000-5000 | Frequent operation, fast recovery |
| Memory Operations | 5-10 | 1000-3000 | Often transient, aggressive retry |

---

## Error Context and Logging

### Adding Context

```rust
use adapteros_lora_mlx_ffi::mlx_error;

let error = MlxError::TensorOpError {
    operation: "matmul".to_string(),
    reason: "shape mismatch".to_string(),
    tensor_shapes: vec![vec![2, 3], vec![4, 5]],
};

// Add context with macro
let contextual_error = mlx_error!(
    error,
    "lora_forward_pass",
    "adapter_id" => adapter_id,
    "module" => "q_proj",
    "batch_size" => batch_size,
);
```

### Structured Logging

```rust
tracing::error!(
    error = %error,
    severity = %error.severity(),
    recoverable = error.is_recoverable(),
    hint = error.recovery_hint(),
    operation = "adapter_load",
    adapter_id = %adapter_id,
    "Operation failed with detailed context"
);
```

### Log Levels by Severity

| Severity | tracing Level | When to Use |
|----------|---------------|-------------|
| Critical | `error!` | System integrity at risk, immediate action needed |
| High | `error!` | Operation failed, user impact |
| Medium | `warn!` | Degraded performance, retry in progress |
| Low | `info!` or `debug!` | Expected errors, validation failures |

---

## Recovery Strategies

### Automatic Recovery Pipeline

```rust
use adapteros_lora_mlx_ffi::recovery::{RecoveryManager, RecoveryStrategy};

let recovery = RecoveryManager::new(4096.0); // 4GB

// Before large allocation
recovery.check_and_recover(Some(&backend), required_mb)?;

// Track adapter usage for LRU
recovery.record_adapter_access(adapter_id);

// Manual recovery trigger
match recovery.recover_from_oom(Some(&backend), requested_mb) {
    Ok(result) => {
        println!("Recovery: {:?}", result.strategy);
        println!("Freed: {:.2} MB", result.freed_mb);
        println!("Message: {}", result.message);
    }
    Err(e) => eprintln!("Recovery failed: {}", e),
}
```

### Recovery Strategies Comparison

| Strategy | Speed | Disruptiveness | Success Rate | When to Use |
|----------|-------|----------------|--------------|-------------|
| GarbageCollect | Fast (100ms) | Low | Low-Medium | First attempt, always try |
| UnloadLRU | Medium (500ms) | Medium | High | If GC fails and adapters loaded |
| ReduceBatchSize | Instant | Low | High | For future requests |
| FallbackCpu | N/A | High | N/A | Not supported in MLX |

### Cleanup Guard

```rust
use adapteros_lora_mlx_ffi::recovery::CleanupGuard;

// Automatic cleanup on scope exit
let _guard = CleanupGuard::new(&recovery, Some(&backend));

// If operation fails or panics, cleanup runs automatically
risky_operation()?;
// Guard dropped here, cleanup runs if needed
```

---

## Validation Best Practices

### Pre-flight Validation Checklist

```rust
// 1. Validate inputs
validation::validate_non_empty(&input_ids, "input_ids")?;
validation::validate_token_ids(&input_ids, vocab_size)?;

// 2. Validate shapes
validation::validate_shape(&tensor_shape, &expected_shape, "input_tensor")?;

// 3. Validate finite values
validation::validate_all_finite(&logits, "model_output")?;

// 4. Validate memory
validation::validate_memory_available(required_mb, threshold_mb, "adapter_load")?;

// 5. Validate configuration
validation::validate_lora_config(rank, alpha, dropout)?;
validation::validate_model_config(hidden_size, num_layers, num_heads, vocab_size)?;
```

### When to Validate

| Validation | Frequency | Performance Impact | Recommendation |
|------------|-----------|-------------------|----------------|
| Input shapes | Every call | Low | Always validate |
| Token IDs | Every call | Low-Medium | Validate in debug mode |
| Memory availability | Before allocation | Low | Always validate |
| Config params | Once at init | Negligible | Always validate |
| Finite values | Every output | Medium | Validate in development |

---

## Common Error Scenarios

### Scenario 1: GPU OOM During Inference

**Problem:** Model runs out of memory mid-inference

**Solution:**
```rust
let recovery = RecoveryManager::new(gpu_memory_mb);

loop {
    match backend.run_inference(&input) {
        Ok(output) => break output,
        Err(MlxError::GpuOomError { requested_mb, .. }) => {
            tracing::warn!("GPU OOM, attempting recovery");

            // Try recovery
            match recovery.recover_from_oom(Some(&backend), requested_mb) {
                Ok(result) if result.success => {
                    tracing::info!("Recovery succeeded, retrying");
                    continue; // Retry inference
                }
                _ => return Err(/* OOM unrecoverable */),
            }
        }
        Err(e) => return Err(e),
    }
}
```

### Scenario 2: Transient Model Loading Failures

**Problem:** Model loading fails intermittently (network issues, disk I/O)

**Solution:**
```rust
let config = RetryConfig::transient();

let model = retry_with_backoff_sync(&config, "model_load", || {
    MLXFFIModel::load(model_path)
        .map_err(|e| match e {
            AosError::Io(_) => MlxError::ModelLoadError {
                path: model_path.display().to_string(),
                source: Box::new(e),
                recoverable: true, // Retry I/O errors
            },
            _ => MlxError::ModelLoadError {
                path: model_path.display().to_string(),
                source: Box::new(e),
                recoverable: false,
            },
        })
})?;
```

### Scenario 3: Circuit Breaker for Failing Adapter

**Problem:** One adapter consistently fails, impacting system

**Solution:**
```rust
let breaker = CircuitBreaker::new("adapter_42_inference", 3, 10000);

match breaker.call(|| {
    backend.run_with_adapter(adapter_id, input)
}) {
    Ok(result) => Ok(result),
    Err(MlxError::CircuitBreakerOpen { .. }) => {
        // Circuit open, fallback to base model
        tracing::warn!("Adapter circuit open, using base model");
        backend.run_with_base_model(input)
    }
    Err(e) => Err(e),
}
```

---

## Performance Impact

### Error Handling Overhead

| Mechanism | Overhead | Recommended Frequency |
|-----------|----------|----------------------|
| Validation (shape check) | ~1-5μs | Every call |
| Validation (memory check) | ~10-50μs | Before allocations |
| Retry (no failure) | ~0μs | Always use |
| Retry (with failure) | Backoff duration | Transient errors only |
| Circuit Breaker (closed) | ~1μs | Always use for critical ops |
| Recovery (GC) | ~100ms | On OOM |
| Recovery (Unload LRU) | ~100-500ms | On OOM after GC |

### Optimization Tips

1. **Cache validation results** for static configs
2. **Use debug assertions** for expensive checks in hot paths
3. **Lazy validation** - only validate on first error
4. **Batch validations** - validate once per batch, not per token

---

## Monitoring and Observability

### Key Metrics

```rust
use adapteros_lora_mlx_ffi::{memory, recovery::RecoveryManager};

// Memory metrics
let stats = memory::stats();
println!("Memory: {:.2} MB", memory::bytes_to_mb(stats.total_bytes));
println!("Allocations: {}", stats.allocation_count);

// Recovery metrics
let recovery_stats = recovery.memory_stats();
println!("Usage: {:.1}%", recovery_stats.usage_pct);
println!("Health: {}", recovery_stats.is_healthy());

// Circuit breaker metrics
println!("Breaker state: {}", breaker.state());
println!("Failures: {}", breaker.failure_count());
```

### Telemetry Integration

```rust
tracing::info_span!("mlx_operation",
    operation = "inference",
    adapter_count = backend.adapter_count(),
    memory_mb = memory::bytes_to_mb(memory::memory_usage()),
).in_scope(|| {
    // Operation here - automatically logged with context
    backend.run_inference(input)
});
```

---

## Troubleshooting Decision Tree

```
Error Occurred
├─ Check error type
│  ├─ GPU_OOM → Run recovery pipeline
│  ├─ ShapeMismatch → Validate input shapes
│  ├─ ModelLoadError → Check pre-flight validations
│  └─ Other → Check logs
│
├─ Is error recoverable?
│  ├─ Yes → Apply retry with backoff
│  └─ No → Validate inputs and config
│
├─ Check system state
│  ├─ Memory usage > 90%? → Trigger recovery
│  ├─ Circuit breaker open? → Wait for reset
│  └─ Too many adapters? → Unload LRU
│
└─ Escalate
   └─ Log with full context and severity
```

---

## Error Code Reference

| Error Code | Category | Recoverable | Severity |
|------------|----------|-------------|----------|
| MLX-001 | GPU_OOM | Yes | High |
| MLX-002 | CPU_OOM | Yes | High |
| MLX-003 | SHAPE_MISMATCH | No | Low |
| MLX-004 | DTYPE_MISMATCH | No | Low |
| MLX-005 | MODEL_LOAD | Maybe | High |
| MLX-006 | ADAPTER_LOAD | Maybe | Medium |
| MLX-007 | VALIDATION | No | Low |
| MLX-008 | FFI_ERROR | Maybe | Medium |
| MLX-009 | TIMEOUT | Yes | Medium |
| MLX-010 | CIRCUIT_OPEN | No | High |

---

## Additional Resources

- [MLX Documentation](https://ml-explore.github.io/mlx/build/html/index.html)
- [Rust Error Handling Best Practices](https://doc.rust-lang.org/book/ch09-00-error-handling.html)
- [Circuit Breaker Pattern](https://learn.microsoft.com/en-us/azure/architecture/patterns/circuit-breaker)
- [Exponential Backoff Algorithm](https://cloud.google.com/iot/docs/how-tos/exponential-backoff)

---

**Document Version:** 1.0.0
**Last Updated:** 2025-01-19
**Maintained by:** AdapterOS Team
