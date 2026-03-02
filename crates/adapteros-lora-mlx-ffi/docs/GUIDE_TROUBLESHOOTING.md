# MLX Backend Troubleshooting Guide

## Quick Health Check

### Check Backend Status
```bash
# Get overall health
curl http://localhost:18080/api/health/mlx

# Expected response (healthy):
{
  "status": "healthy",
  "success_rate": 0.98,
  "average_latency_ms": 45.2,
  "active_adapters": 3,
  "circuit_breaker": "closed"
}
```

### View Performance Metrics
```bash
# Prometheus metrics
curl http://localhost:18080/metrics | grep mlx_

# Key metrics to monitor:
mlx_requests_total{backend="mlx"} 12543
mlx_requests_success{backend="mlx"} 12250
mlx_inference_duration_ms{backend="mlx"} 45230
mlx_memory_usage_mb{backend="mlx"} 256
mlx_active_adapters{backend="mlx"} 3
```

## Common Issues & Solutions

### 1. Backend in Stub Mode

**Symptoms:**
- Logs show "MLX stub inference" instead of "Real MLX inference"
- Performance metrics show 0ms inference time
- Determinism attestation returns `SystemEntropy`

**Causes:**
- MLX C++ API not available
- `mlx` feature not enabled
- Missing MLX library dependencies

**Solutions:**

#### Enable Real MLX Mode
```bash
# Enable feature flag
cargo build --features mlx

# Verify compilation
cargo check -p adapteros-lora-mlx-ffi --features mlx
```

#### Install MLX Dependencies
```bash
# macOS with Homebrew
brew install mlx

# Or build from source
git clone https://github.com/ml-explore/mlx.git
cd mlx
pip install -e .
```

#### Check Library Linking
```bash
# Verify MLX libraries
otool -L target/debug/deps/libadapteros_lora_mlx_ffi.dylib | grep mlx
```

#### Fix Undefined MLX Symbols in Workspace Builds
If the linker reports undefined `mlx::core::*` symbols, you are likely mixing
Homebrew MLX headers with `mlx-sys` static libs. Point `MLX_PATH` at the
`mlx-sys` build output so headers and libs match:

```bash
MLX_PATH="$(pwd)/$(ls -d target/debug/build/mlx-sys-*/out/build | tail -n 1)" \
  cargo test --workspace --all-targets --exclude adapteros-lora-mlx-ffi
```

### 2. High Memory Usage

**Symptoms:**
- `peak_memory_usage_mb` > 1GB
- System memory pressure warnings
- Slow inference times

**Causes:**
- Memory pool not recycling arrays
- Large batch sizes
- Memory leaks in MLX operations

**Solutions:**

#### Check Memory Pool
```rust
// Monitor memory pool usage
let pool_size = backend.memory_pool.read().len();
println!("Memory pool size: {}", pool_size);
```

#### Force Garbage Collection
```rust
// In debug builds, force cleanup
unsafe { crate::mlx_gc() };
```

#### Reduce Batch Size
```rust
// Adjust batch configuration
config.max_batch_size = 1; // Conservative setting
```

### 3. Circuit Breaker Tripping

**Symptoms:**
- Requests failing with "CircuitBreakerOpen"
- Backend status shows "Degraded" or "Critical"
- Recovery attempts failing

**Causes:**
- MLX inference failures
- Network timeouts
- Resource exhaustion

**Solutions:**

#### Check Failure Patterns
```bash
# View recent failures
grep "inference failed" /var/log/adapteros.log | tail -10
```

#### Adjust Circuit Breaker Thresholds
```rust
let config = MLXResilienceConfig {
    failure_threshold: 20,      // More tolerant
    recovery_timeout_secs: 60,  // Faster recovery
    success_threshold: 5,       // Easier recovery
    max_stub_fallback_time_secs: 7200, // Longer fallback
};
```

#### Manual Recovery
```bash
# Force circuit breaker reset
curl -X POST http://localhost:18080/api/health/mlx/reset
```

### 4. Slow Inference Performance

**Symptoms:**
- `average_latency_ms` > 100ms
- GPU utilization low
- Memory pool frequently empty

**Causes:**
- Single-threaded execution
- No batching optimization
- Memory allocation overhead

**Solutions:**

#### Enable Performance Logging
```bash
RUST_LOG=adapteros_lora_mlx_ffi=debug cargo run
```

#### Check GPU Utilization
```bash
# macOS GPU monitoring
powermetrics --samplers gpu_power | grep -i mlx
```

#### Optimize Memory Pool
```rust
// Pre-allocate memory pool
for _ in 0..10 {
    let array = unsafe { crate::mlx_array_alloc(4096) };
    backend.memory_pool.write().push(array);
}
```

### 5. LoRA Adaptation Issues

**Symptoms:**
- LoRA matrices not loading
- Adaptation effects too weak/strong
- Inconsistent behavior across requests

**Causes:**
- Incorrect safetensors format
- Wrong matrix dimensions
- Scale parameter misconfiguration

**Solutions:**

#### Validate LoRA Files
```bash
# Check safetensors contents
python3 -c "
import safetensors
tensors = safetensors.safe_open('adapter.safetensors', 'numpy')
for k, v in tensors.items():
    print(f'{k}: {v.shape} {v.dtype}')
"
```

#### Check Matrix Dimensions
```rust
// Verify matrix compatibility
assert_eq!(lora_a[0].len(), lora_b.len(), "LoRA rank mismatch");
assert!(lora_a.len() == hidden_size, "LoRA input dimension mismatch");
```

#### Adjust Scale Parameters
```rust
let config = LoRAConfig {
    scale: 0.5,  // Reduce for subtler adaptation
    rank: 8,     // Lower rank for smaller models
    alpha: 16.0, // Match typical LoRA configurations
    target_modules: vec!["q_proj".into(), "v_proj".into()],
};
```

### 6. Determinism Violations

**Symptoms:**
- Policy compliance failures
- `attest_determinism()` returns non-deterministic
- Reproducible execution check fails

**Causes:**
- System entropy used instead of HKDF
- Random operations in MLX code
- Non-deterministic fallbacks active

**Solutions:**

#### Verify Feature Flags
```bash
# Check if mlx is enabled
cargo tree -p adapteros-lora-mlx-ffi | grep mlx
```

#### Check Determinism Attestation
```rust
let (method, deterministic) = backend.attest_determinism();
assert_eq!(method, RngSeedingMethod::HkdfSeeded);
assert!(deterministic);
```

#### Force Deterministic Mode
```rust
// Ensure HKDF seeding is used
let seed = adapteros_core::derive_seed(&manifest_hash, "mlx_backend");
unsafe { crate::mlx_set_seed_from_bytes(seed.as_bytes()) };
```

## Advanced Debugging

### Enable Core Dumps
```bash
# macOS core dump settings
ulimit -c unlimited
sudo sysctl -w kern.coredump=1
```

### Memory Leak Detection
```rust
// Track allocations
#[cfg(debug_assertions)]
{
    let before = unsafe { crate::mlx_memory_usage() };
    // ... operation ...
    let after = unsafe { crate::mlx_memory_usage() };
    if after > before + 1024 * 1024 { // 1MB threshold
        warn!("Potential memory leak detected: {} bytes", after - before);
    }
}
```

### MLX Operation Tracing
```rust
// Enable MLX internal tracing
std::env::set_var("MLX_TRACE", "1");
std::env::set_var("MLX_METAL_DEBUG", "1");
```

## Performance Tuning

### Optimal Configuration
```rust
pub struct OptimalConfig {
    pub max_batch_size: usize = 4,
    pub memory_pool_size: usize = 16,
    pub failure_threshold: u32 = 15,
    pub prefetch_adapters: bool = true,
    pub enable_metal_optimization: bool = true,
}
```

### Benchmarking Script
```bash
#!/bin/bash
# benchmark_mlx.sh

echo "Benchmarking MLX Backend..."

# Warmup
for i in {1..10}; do
    curl -s http://localhost:18080/api/inference > /dev/null
done

# Benchmark
time for i in {1..100}; do
    curl -s http://localhost:18080/api/inference > /dev/null
done

echo "Benchmark complete. Check logs for performance metrics."
```

## Emergency Procedures

### Force Stub Mode
```rust
// Emergency fallback
config.force_stub_mode = true;
backend.set_stub_fallback(true);
```

### Backend Restart
```bash
# Graceful restart
curl -X POST http://localhost:18080/api/admin/restart/mlx

# Force restart
pkill -f adapteros-lora-mlx-ffi
systemctl restart adapteros
```

### Data Recovery
```bash
# Backup performance metrics
cp /var/lib/adapteros/mlx_metrics.db /var/lib/adapteros/mlx_metrics.db.backup

# Clear corrupted state
rm /var/lib/adapteros/mlx_health.state
```

## Prevention Best Practices

### Monitoring Alerts
Set up alerts for:
- Success rate < 95%
- Average latency > 100ms
- Memory usage > 80%
- Circuit breaker open > 5 minutes

### Regular Maintenance
```bash
# Weekly health checks
0 2 * * 1 /usr/local/bin/mlx_health_check.sh

# Monthly performance benchmarks
0 3 1 * * /usr/local/bin/mlx_benchmark.sh
```

### Capacity Planning
- Monitor peak usage patterns
- Scale memory pool based on load
- Adjust circuit breaker thresholds for traffic patterns

## Support Resources

### Logs to Collect
```bash
# System logs
journalctl -u adapteros -f

# MLX-specific logs
tail -f /var/log/adapteros/mlx_backend.log

# Performance traces
perf record -g -p $(pgrep adapteros)
```

### Diagnostic Commands
```bash
# Full system health
/usr/local/bin/adapteros-diagnostics.sh

# MLX-specific diagnostics
python3 /usr/local/bin/mlx_diagnostics.py
```

### Community Support
- GitHub Issues: Report bugs with full logs
- Documentation: Check API reference for parameter details
- Performance Tuning: Join #mlx-performance channel

## Recovery Checklist

- [ ] Check backend health endpoint
- [ ] Review recent error logs
- [ ] Verify MLX library installation
- [ ] Check memory and CPU usage
- [ ] Test with minimal configuration
- [ ] Enable debug logging
- [ ] Collect diagnostic information
- [ ] Escalate to engineering if needed
