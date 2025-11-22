# MLX Backend API Reference

## Overview

The MLX backend provides Apple Silicon-accelerated inference with enterprise-grade resilience, monitoring, and LoRA adaptation support.

## Core Components

### MLXFFIBackend

The main inference engine implementing the `FusedKernels` trait.

#### Constructor
```rust
pub fn new(model: MLXFFIModel, config: MLXResilienceConfig) -> Self
```

Creates a new MLX backend with the specified model and resilience configuration.

**Parameters:**
- `model`: Pre-loaded MLX model
- `config`: Resilience configuration (circuit breaker thresholds, timeouts, etc.)

#### Key Methods

##### `run_step`
```rust
pub fn run_step(&mut self, io: &mut IoBuffers, ring: &RouterRing) -> Result<()>
```

Performs inference on the provided input buffers with LoRA adaptation.

**Parameters:**
- `io`: Input/output buffers containing tokens and logits
- `ring`: Router ring specifying which LoRA adapters to apply

**Behavior:**
- Automatically switches between real MLX inference and stub fallback based on health
- Applies matrix-aware LoRA adaptation when adapters are loaded
- Updates performance metrics and health status

##### `device_info`
```rust
pub fn device_info(&self) -> String
```

Returns detailed device information including health status, performance metrics, and active configuration.

**Returns:** Formatted string with:
- Backend status (Healthy/Degraded/Critical)
- Request statistics (total, success rate)
- Performance metrics (latency, memory usage)
- Active adapters count
- Circuit breaker state

##### `attest_determinism`
```rust
pub fn attest_determinism(&self) -> (RngSeedingMethod, bool)
```

Reports the backend's determinism capabilities.

**Returns:** `(seeding_method, deterministic)`
- Real MLX: `(HkdfSeeded, true)` - Full determinism with HKDF seeding
- Stub mode: `(SystemEntropy, false)` - Non-deterministic fallback

#### Performance Metrics

The backend tracks comprehensive performance metrics:

```rust
pub struct PerformanceMetrics {
    pub total_inference_time_ms: u64,
    pub total_requests: u64,
    pub average_latency_ms: f32,
    pub peak_memory_usage_mb: f32,
    pub cache_hit_rate: f32,
}
```

### MLXFFIModel

Low-level MLX model wrapper providing FFI access to MLX operations.

#### Key Methods

##### `forward`
```rust
pub fn forward(&self, input_ids: &[i32]) -> Result<Vec<f32>>
```

Performs basic forward pass without hidden states.

##### `forward_with_hidden_states`
```rust
pub fn forward_with_hidden_states(&self, input_ids: &[i32]) -> Result<(Vec<f32>, HashMap<String, Vec<f32>>)>
```

Performs forward pass and extracts intermediate hidden states for LoRA application.

**Returns:** `(logits, hidden_states_map)`
- `logits`: Final model predictions
- `hidden_states_map`: Hidden states keyed by layer name (q_proj, k_proj, v_proj, o_proj)

### LoRAAdapter

Represents a loaded LoRA adapter with matrix weights.

#### Key Methods

##### `load`
```rust
pub fn load(path: &Path, config: LoRAConfig) -> Result<Self>
```

Loads LoRA weights from safetensors format.

**Parameters:**
- `path`: Path to safetensors file
- `config`: LoRA configuration (scale, target modules, rank)

##### `apply`
```rust
pub fn apply(&self, hidden_states: &mut [f32], module_name: &str) -> Result<()>
```

Applies LoRA adaptation to hidden states for a specific module.

## Resilience Features

### Circuit Breaker

The backend implements an advanced circuit breaker with three states:

- **Closed**: Normal operation, requests pass through
- **Open**: Failure threshold exceeded, requests fail fast
- **Half-Open**: Testing recovery, limited requests allowed

**Configuration:**
```rust
pub struct MLXResilienceConfig {
    pub failure_threshold: u32,        // Failures before opening
    pub recovery_timeout_secs: u64,    // Time before attempting recovery
    pub success_threshold: u32,        // Successes needed for full recovery
    pub max_stub_fallback_time_secs: u64, // Max time in stub mode
}
```

### Automatic Fallback

When MLX fails, the backend automatically falls back to stub inference:
- Maintains API compatibility
- Provides realistic (but deterministic) outputs
- Logs fallback activation for monitoring

### Health Monitoring

Continuous health assessment with configurable thresholds:

```rust
pub struct AlertThresholds {
    pub max_failure_rate: f32,         // e.g., 0.1 (10%)
    pub max_response_time_ms: f32,     // e.g., 5000.0
    pub min_health_score: f32,         // e.g., 70.0
}
```

## LoRA Implementation

### Matrix-Aware Adaptation

Unlike simple scaling, the MLX backend implements sophisticated LoRA:

1. **Matrix Properties**: Uses actual matrix dimensions and rank
2. **Position Awareness**: Different adaptation patterns for different token positions
3. **Multi-Module Support**: Adapts query, key, value, and output projections
4. **Scale Normalization**: Proper scaling based on matrix rank

### Configuration

```rust
pub struct LoRAConfig {
    pub scale: f32,                    // Adaptation strength
    pub target_modules: Vec<String>,   // Which modules to adapt
    pub rank: usize,                   // LoRA rank
    pub alpha: f32,                    // LoRA alpha parameter
}
```

## Monitoring & Alerting

### Health Checks

Regular health assessments generate structured alerts:

```rust
pub enum AlertType {
    HighFailureRate,
    SlowResponseTime,
    MemoryPressure,
    CircuitBreakerOpen,
    RecoveryTimeExceeded,
}
```

### Metrics Export

Prometheus-compatible metrics for operational monitoring:

- `mlx_requests_total`: Total inference requests
- `mlx_requests_success`: Successful requests
- `mlx_inference_duration_ms`: Inference latency
- `mlx_memory_usage_mb`: Current memory usage
- `mlx_active_adapters`: Number of loaded adapters

## Error Handling

### Error Types

```rust
pub enum MlxError {
    ModelLoadFailed(String),
    InferenceFailed(String),
    InvalidInput(String),
    MemoryAllocationFailed(String),
    CircuitBreakerOpen,
    HealthCheckFailed(String),
}
```

### Recovery Strategies

1. **Graceful Degradation**: Fallback to stub when MLX unavailable
2. **Exponential Backoff**: Progressive delays on repeated failures
3. **Resource Cleanup**: Automatic cleanup on failures
4. **Alert Escalation**: Progressive alert severity

## Performance Optimization

### Memory Pooling

Pre-allocated MLX array pool reduces allocation overhead:

- Objects are reused across requests
- Reduces garbage collection pressure
- Improves latency consistency

### Batch Processing (Future)

Planned optimization for concurrent requests:
- Request queuing and batching
- Parallel inference execution
- Improved GPU utilization

## Configuration

### Environment Variables

- `MLX_REAL_MODE`: Enable real MLX (requires MLX C++ API)
- `MLX_FAILURE_THRESHOLD`: Circuit breaker threshold
- `MLX_HEALTH_CHECK_INTERVAL`: Health check frequency

### Feature Flags

- `--features real-mlx`: Enable real MLX compilation
- `--features enhanced-monitoring`: Enable detailed metrics

## Troubleshooting

### Common Issues

1. **MLX Not Available**: Falls back to stub mode automatically
2. **Memory Pressure**: Monitor `peak_memory_usage_mb` metric
3. **Slow Inference**: Check `average_latency_ms` and optimize batch size
4. **Circuit Breaker Trips**: Review failure patterns and thresholds

### Debug Information

Enable detailed logging:
```bash
RUST_LOG=adapteros_lora_mlx_ffi=debug cargo run
```

### Health Check Commands

```bash
# Check backend health
curl http://localhost:8080/api/health/mlx

# Get performance metrics
curl http://localhost:8080/api/metrics/mlx

# View active alerts
curl http://localhost:8080/api/alerts/mlx
```

## Migration Guide

### From Stub-Only Implementation

1. Enable `real-mlx` feature flag
2. Install MLX C++ dependencies
3. Update resilience configuration
4. Monitor performance metrics
5. Adjust circuit breaker thresholds

### Configuration Changes

```rust
// Before (stub-only)
let config = MLXResilienceConfig::default();

// After (with real MLX)
let config = MLXResilienceConfig {
    failure_threshold: 10,
    recovery_timeout_secs: 300,
    success_threshold: 3,
    max_stub_fallback_time_secs: 3600,
};
```

## Future Enhancements

- **Batch Processing**: Concurrent request handling
- **Model Sharding**: Large model support across multiple GPUs
- **Dynamic LoRA**: Runtime adapter loading/unloading
- **ANE Optimization**: Apple Neural Engine acceleration
- **Quantization**: 8-bit/4-bit weight quantization

