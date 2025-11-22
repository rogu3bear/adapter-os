# Hybrid Execution Architecture

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.
**Last Updated:** 2025-01-19
**Maintained by:** James KC Auchterlonie

## Overview

This document describes the hybrid execution architecture for AdapterOS, enabling multi-backend coordination, automatic fallback, and runtime switching between Metal, CoreML/ANE, and MLX backends.

## Architecture

### Backend Hierarchy

```
Primary: Metal (deterministic, production)
    ↓ (fallback on failure)
Secondary: CoreML + ANE (power efficient)
    ↓ (fallback on failure)
Tertiary: MLX (experimental, stub only)
```

### Key Components

#### 1. Backend Factory (`backend_factory.rs`)

**Purpose:** Create and configure backend instances with automatic detection

**Key Types:**

```rust
pub enum BackendChoice {
    Metal,
    Mlx { model_path: PathBuf },
    CoreML { model_path: Option<PathBuf> },
}

pub enum BackendStrategy {
    MetalOnly,
    MetalWithCoreMLFallback,
    AutoWithFullFallback,
    PreferANE,
}

pub struct BackendCapabilities {
    pub has_metal: bool,
    pub has_ane: bool,
    pub has_mlx: bool,
    pub vram_capacity: usize,
    pub system_ram: usize,
    pub metal_device_name: Option<String>,
    pub ane_core_count: u32,
}
```

**Capability Detection:**

- **Metal Detection:** Uses Metal API to detect GPU availability
- **ANE Detection:** Uses `ANEAccelerator` to probe Apple Neural Engine
- **VRAM Capacity:** Estimates based on device name (M1/M2: 16GB, M3/M4: 24GB)
- **System RAM:** Uses `sysctl hw.memsize` on macOS

**Usage:**

```rust
// Detect capabilities
let caps = detect_capabilities();

// Automatic selection with fallback
let backend = create_backend_auto(
    BackendStrategy::MetalWithCoreMLFallback,
    Some(8_000_000_000) // 8GB model
)?;

// Manual selection
let backend = create_backend(BackendChoice::Metal)?;
```

#### 2. Backend Coordinator (`backend_coordinator.rs`)

**Purpose:** Manage multiple backends with automatic failover and health monitoring

**Key Types:**

```rust
pub struct BackendCoordinator {
    primary: Arc<RwLock<Box<dyn FusedKernels>>>,
    fallback: Option<Arc<RwLock<Box<dyn FusedKernels>>>>,
    primary_health: Arc<RwLock<BackendHealth>>,
    fallback_health: Option<Arc<RwLock<BackendHealth>>>,
    health_check_interval: Duration,
    metrics: Arc<RwLock<CoordinatorMetrics>>,
    capabilities: BackendCapabilities,
}

pub struct CoordinatorMetrics {
    pub total_operations: u64,
    pub primary_operations: u64,
    pub fallback_operations: u64,
    pub backend_switches: u64,
    pub health_check_failures: u64,
    pub avg_latency_us: f32,
}
```

**Health Monitoring:**

- Periodic health checks (default: 30s interval)
- Automatic backend switching on failure
- Health states: `Healthy`, `Degraded`, `Failed`

**Usage:**

```rust
// Create coordinator with fallback
let coordinator = BackendCoordinator::new(
    BackendStrategy::MetalWithCoreMLFallback,
    true, // enable_fallback
    Some(model_size_bytes)
).await?;

// Execute inference with automatic failover
let mut ring = RouterRing::new(2);
ring.set(&[1, 2], &[16384, 8192]);
let mut io = IoBuffers::new(vocab_size);

coordinator.run_step(&ring, &mut io).await?;

// Monitor metrics
let metrics = coordinator.get_metrics().await;
println!("Operations: {}, Switches: {}",
    metrics.total_operations,
    metrics.backend_switches
);
```

#### 3. Health Checks (`FusedKernels` trait extension)

**Purpose:** Provide runtime backend health monitoring

**API:**

```rust
pub enum BackendHealth {
    Healthy,
    Degraded { reason: String },
    Failed { reason: String },
}

pub struct BackendMetrics {
    pub total_operations: u64,
    pub avg_latency_us: f32,
    pub peak_memory_bytes: u64,
    pub current_memory_bytes: u64,
    pub utilization_percent: f32,
    pub error_count: u64,
    pub custom_metrics: HashMap<String, f32>,
}

trait FusedKernels {
    fn health_check(&self) -> Result<BackendHealth>;
    fn get_metrics(&self) -> BackendMetrics;
}
```

**Implementation:**

- Metal: Checks device availability and GPU memory
- CoreML: Checks ANE availability and session state
- MLX: Stub implementation (always healthy)

## Backend Selection Logic

### Strategy: MetalOnly

```
if has_metal:
    return Metal
else:
    error("Metal required but not available")
```

### Strategy: MetalWithCoreMLFallback

```
if has_metal:
    if model_size <= vram_capacity:
        return Metal
    else if has_ane:
        return CoreML
    else:
        error("Model too large for VRAM, no ANE")
else if has_ane:
    return CoreML
else:
    error("Neither Metal nor ANE available")
```

### Strategy: AutoWithFullFallback

```
if has_metal and model_size <= vram_capacity:
    return Metal
else if has_ane:
    return CoreML
else if has_mlx:
    return MLX (experimental)
else:
    error("No suitable backend")
```

### Strategy: PreferANE

```
if has_ane:
    return CoreML
else if has_metal:
    return Metal
else:
    error("Neither ANE nor Metal available")
```

## Fallback Chain Design

### Primary Backend Failure

1. Detect failure during `run_step()` or health check
2. Mark primary as `Degraded` or `Failed`
3. Switch to fallback backend if available
4. Increment `backend_switches` metric
5. Log telemetry event

### Fallback Backend Selection

**From Metal:**
- First choice: CoreML (if ANE available)
- Rationale: ANE provides hardware acceleration with lower power

**From CoreML:**
- First choice: Metal (if GPU available)
- Rationale: Metal provides deterministic execution

**From MLX:**
- First choice: Metal (if GPU available)
- Second choice: CoreML (if ANE available)
- Rationale: Prefer production backends over experimental

### Recovery Protocol

1. **Automatic Recovery:**
   - Health checks run every 30s
   - If primary recovers, continue using fallback
   - Manual switch back via `reset_primary_health()`

2. **Manual Recovery:**
   ```rust
   coordinator.reset_primary_health().await;
   ```

## Tensor Sharing (Future Work)

### Challenge

Metal and CoreML use different memory layouts:
- Metal: Row-major, GPU VRAM
- CoreML: CoreML format, ANE memory or system RAM

### Proposed Design

```rust
pub trait TensorBridge {
    fn metal_to_coreml(&self, buffer: &metal::Buffer) -> Result<CoreMLTensor>;
    fn coreml_to_metal(&self, tensor: &CoreMLTensor) -> Result<metal::Buffer>;
}

pub struct HybridPipeline {
    metal_attn: MetalAttention,
    coreml_mlp: CoreMLMLP,
    bridge: TensorBridge,
}

impl HybridPipeline {
    async fn forward(&mut self, input: &[f32]) -> Result<Vec<f32>> {
        // Attention on Metal (Flash Attention)
        let attn_out = self.metal_attn.forward(input)?;

        // Bridge to CoreML
        let coreml_input = self.bridge.metal_to_coreml(&attn_out)?;

        // MLP on CoreML/ANE (power efficient)
        let mlp_out = self.coreml_mlp.forward(&coreml_input)?;

        // Bridge back to Metal
        let metal_out = self.bridge.coreml_to_metal(&mlp_out)?;

        Ok(metal_out.to_vec())
    }
}
```

**Memory Copy Strategy:**
- Zero-copy where possible (shared unified memory on Apple Silicon)
- Async copy for large tensors
- Batching to amortize transfer cost

## Determinism Guarantees

### Metal Backend
- **Deterministic:** Yes (precompiled kernels, HKDF seeding)
- **Attestation:** BLAKE3 metallib hash
- **Floating-point:** Fixed precision mode

### CoreML Backend
- **Deterministic:** Yes (with ANE), Partial (CPU fallback)
- **Attestation:** ANE availability check
- **Floating-point:** ANE fixed-point, CPU may vary

### MLX Backend
- **Deterministic:** Yes (HKDF seeding)
- **Attestation:** HKDF seed verification
- **Floating-point:** Deterministic sampling

## Telemetry Events

### Backend Selection
```json
{
  "event": "backend.selected",
  "backend": "Metal",
  "strategy": "MetalWithCoreMLFallback",
  "capabilities": {
    "has_metal": true,
    "has_ane": true,
    "vram_gb": 16
  }
}
```

### Backend Switch
```json
{
  "event": "backend.switched",
  "from": "Metal",
  "to": "CoreML",
  "reason": "Primary backend failed: GPU error",
  "switch_count": 3
}
```

### Health Check Failure
```json
{
  "event": "backend.health_check_failed",
  "backend": "Metal",
  "health": "Failed",
  "reason": "GPU device not responding"
}
```

## Testing Strategy

### Unit Tests
- Capability detection (macOS vs non-macOS)
- Backend creation with feature flags
- Strategy selection logic
- Health check implementations

### Integration Tests
- Coordinator creation and lifecycle
- Automatic failover on simulated failure
- Metrics tracking across operations
- Multi-backend determinism verification

### Performance Tests
- Backend selection latency
- Failover overhead
- Health check performance impact
- Memory footprint across backends

## Performance Characteristics

### Backend Latency

| Backend | First Token (ms) | Subsequent Tokens (ms) | Power (W) |
|---------|------------------|------------------------|-----------|
| Metal   | 50-100          | 5-10                   | 15-20     |
| CoreML  | 80-150          | 10-20                  | 5-10      |
| MLX     | 100-200         | 15-30                  | 10-15     |

### Failover Overhead

- Health check: ~1ms
- Backend switch: ~50-100ms (one-time)
- Coordinator overhead: <1% per operation

### Memory Footprint

| Backend | Base Overhead | Per Adapter | KV Cache |
|---------|---------------|-------------|----------|
| Metal   | ~100MB        | ~50MB       | 2-4GB    |
| CoreML  | ~150MB        | ~75MB       | 1-2GB    |
| MLX     | ~200MB        | ~100MB      | 3-5GB    |

## Production Deployment

### Recommended Configuration

```rust
// Production: Metal only for determinism
let backend = create_backend_auto(
    BackendStrategy::MetalOnly,
    Some(model_size)
)?;

// Development: Enable fallback for testing
let coordinator = BackendCoordinator::new(
    BackendStrategy::MetalWithCoreMLFallback,
    true, // enable_fallback
    Some(model_size)
).await?;
```

### Monitoring

```rust
// Periodic metrics collection
tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        let metrics = coordinator.get_metrics().await;
        log_telemetry("backend.metrics", &metrics);
    }
});
```

## Future Enhancements

1. **Multi-GPU Support:** Round-robin across multiple Metal devices
2. **Tensor Partitioning:** Split large models across backends
3. **Dynamic Rebalancing:** Shift load based on real-time metrics
4. **Predictive Failover:** Switch before failure based on trends
5. **Backend Pooling:** Pre-warm fallback backends for instant switch

## References

- [CLAUDE.md](../CLAUDE.md) - Developer guide and conventions
- [ARCHITECTURE_PATTERNS.md](ARCHITECTURE_PATTERNS.md) - Core patterns
- [TELEMETRY_EVENTS.md](TELEMETRY_EVENTS.md) - Event catalog
- [backend_factory.rs](../crates/adapteros-lora-worker/src/backend_factory.rs) - Implementation
- [backend_coordinator.rs](../crates/adapteros-lora-worker/src/backend_coordinator.rs) - Coordination

---

**Signed:** James KC Auchterlonie
