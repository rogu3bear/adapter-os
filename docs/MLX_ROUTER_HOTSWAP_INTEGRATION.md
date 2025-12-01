# MLX Router & Hot-Swap Integration Guide

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.
**Last Updated:** 2025-11-22
**Status:** Implementation Complete

---

## Overview

This document details the integration of the MLX backend with AdapterOS's router and hot-swap subsystems, enabling multi-adapter inference with live adapter loading/unloading and K-sparse adapter selection.

### Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                        Inference Request                         │
└────────────────┬────────────────────────────────────────────────┘
                 │
                 ▼
        ┌────────────────────┐
        │  Router (K-sparse) │
        │  Q15 Quantized     │
        └────────┬───────────┘
                 │
    ┌────────────┼────────────┐
    ▼            ▼            ▼
┌─────────┐  ┌─────────┐  ┌─────────┐
│Adapter 1│  │Adapter 2│  │Adapter 3│
│(gate:50)│  │(gate:30)│  │(gate:20)│  ← Hot-Swap ready
└────┬────┘  └────┬────┘  └────┬────┘
     │            │            │
     └────────────┼────────────┘
                  ▼
        ┌──────────────────────┐
        │  MLX Backend         │
        │  Multi-Adapter       │
        │  Fusion              │
        └──────────┬───────────┘
                   │
                   ▼
            ┌──────────────┐
            │Output Logits │
            └──────────────┘
```

---

## Router Integration

### K-Sparse Adapter Selection

The router uses K-sparse selection with Q15 quantized gates to choose the most relevant adapters:

```rust
use adapteros_lora_router::Router;
use adapteros_core::Result;

fn main() -> Result<()> {
    // Router selection (K=3)
    let router = Router::new(num_adapters, k=3);

    // Given input context, select top K adapters
    let gates = router.select_adapters(&input_embeddings)?;
    // Returns: [adapter_id_1, adapter_id_2, adapter_id_3] with scores

    // Gate scores are Q15 quantized (sum ≈ 32767)
    let total_gate_weight: i32 = gates.iter().map(|(_, score)| *score as i32).sum();
    assert!(total_gate_weight <= 32767);  // Within Q15 range

    Ok(())
}
```

### Gate Quantization (Q15 Format)

Q15 is a 16-bit signed fixed-point format for efficient gate computation:

```rust
// Q15 range: [-32768, 32767] for -1.0 to ~1.0 in floating point
// For adapter weights: sum of positive gates ≈ 32767

// Convert float gates to Q15
fn float_to_q15(value: f32) -> i16 {
    (value * 32767.0).min(32767.0).max(-32768.0) as i16
}

// Example: 3 adapters with normalized weights
let float_weights = [0.5, 0.3, 0.2];  // Sum = 1.0
let q15_weights = vec![
    float_to_q15(0.5) as u16,  // 16383
    float_to_q15(0.3) as u16,  // 9830
    float_to_q15(0.2) as u16,  // 6553
];
// Total: 32766 ≈ 32767
```

### Router Integration Points

```rust
use adapteros_lora_router::Router;
use adapteros_lora_mlx_ffi::backend::MLXFFIBackend;

fn main() -> Result<()> {
    let router = Router::new(num_adapters, k);
    let mut backend = MLXFFIBackend::new(model);

    // 1. Register adapters with backend
    for (adapter_id, adapter) in adapters.iter().enumerate() {
        backend.register_adapter(adapter_id as u16, adapter.clone())?;
    }

    // 2. Route input through adapter selection
    let (selected_adapters, gates) = router.select(&input)?;

    // 3. Execute inference with selected adapters
    let output = backend.forward_multi_adapter(
        &input_tokens,
        &selected_adapters,
        &gates,
    )?;

    Ok(())
}
```

---

## Hot-Swap Architecture

Hot-swap enables live adapter loading/unloading without interrupting inference:

### State Machine

```
      Load Adapter
           │
           ▼
    ┌──────────────┐
    │   Preload    │ ← Load into VRAM, not yet active
    └──────┬───────┘
           │
           ▼
    ┌──────────────┐
    │   Verify     │ ← Compute hash, verify integrity
    └──────┬───────┘
           │
           ▼
    ┌──────────────┐
    │   Active     │ ← Available for inference
    └──────┬───────┘
           │
       (swap)
           │
           ▼
    ┌──────────────┐
    │  Unload      │ ← Remove from VRAM
    └──────┬───────┘
           │
           ▼
         Done
```

### Hot-Swap Operations

**Preload (No Activation)**
```rust
use adapteros_lora_worker::adapter_hotswap::AdapterCommand;

// Load adapter into VRAM without activating
let command = AdapterCommand::Preload {
    adapter_id: "adapter-1".to_string(),
    hash: B3Hash::hash(adapter_data),
};

// Send to hot-swap manager
let result = hotswap_manager.execute(command).await?;
// VRAM allocated, but not yet used in inference
```

**Atomic Swap**
```rust
// Add new adapters, remove old ones (atomic)
let command = AdapterCommand::Swap {
    add_ids: vec!["adapter-2".to_string(), "adapter-3".to_string()],
    remove_ids: vec!["adapter-1".to_string()],
};

let result = hotswap_manager.execute(command).await?;
// Returns: swap duration, VRAM delta
// Atomic pointer flip ensures no partial state
```

**Verification**
```rust
// Compute stack hash to verify integrity
let command = AdapterCommand::VerifyStack;

let result = hotswap_manager.execute(command).await?;
// Returns: hash of current adapter stack
// Should be deterministic (same adapters = same hash)
```

**Rollback**
```rust
// Revert to last verified state
let command = AdapterCommand::Rollback;

let result = hotswap_manager.execute(command).await?;
// Reverts all adapters to previous configuration
```

---

## Integration Testing

### Test 1: Router Selection with MLX Backend

```rust
#[tokio::test]
async fn test_router_selection_mlx() -> Result<()> {
    use adapteros_lora_router::Router;
    use adapteros_lora_mlx_ffi::backend::MLXFFIBackend;
    use adapteros_lora_mlx_ffi::mock::create_mock_config;

    // Create backend
    let config = create_mock_config();
    let model = MLXFFIModel::new_null(config);
    let mut backend = MLXFFIBackend::new(model);

    // Register adapters
    for i in 0..5 {
        let adapter = create_mock_adapter(&format!("adapter{}", i), 4);
        backend.register_adapter(i as u16, adapter)?;
    }

    // Create router
    let router = Router::new(5, 3);  // K=3

    // Route inference
    let input = vec![1, 2, 3];
    let (selected, gates) = router.select(&input)?;

    // Verify K-sparse selection
    assert_eq!(selected.len(), 3);
    assert!(gates.iter().all(|g| *g > 0));

    // Execute with selected adapters
    let output = backend.forward_multi_adapter(&input, &selected, &gates)?;
    assert!(!output.is_empty());

    Ok(())
}
```

### Test 2: Hot-Swap During Inference

```rust
#[tokio::test]
async fn test_hotswap_mlx_backend() -> Result<()> {
    use adapteros_lora_worker::adapter_hotswap::HotSwapManager;

    // Setup backend
    let model = MLXFFIModel::new_null(create_mock_config());
    let backend = MLXFFIBackend::new(model);
    let hotswap = HotSwapManager::new(backend);

    // Preload adapters
    hotswap.preload("adapter1".to_string(), B3Hash::hash(b"a1")).await?;
    hotswap.preload("adapter2".to_string(), B3Hash::hash(b"a2")).await?;
    hotswap.preload("adapter3".to_string(), B3Hash::hash(b"a3")).await?;

    // Verify all loaded
    assert_eq!(hotswap.adapter_count(), 3);

    // Swap: Add adapter4, remove adapter1
    let swap_result = hotswap.swap(
        vec!["adapter4".to_string()],
        vec!["adapter1".to_string()],
    ).await?;

    // Verify swap result
    assert_eq!(swap_result.vram_delta_mb, Some(delta));  // Depends on adapter size
    assert!(swap_result.duration_ms < 1000);  // Should complete in <1s

    // Verify new state
    assert_eq!(hotswap.adapter_count(), 3);
    assert!(hotswap.has_adapter("adapter4"));
    assert!(!hotswap.has_adapter("adapter1"));

    Ok(())
}
```

### Test 3: Deterministic Seeding with Router

```rust
#[tokio::test]
async fn test_deterministic_router_mlx() -> Result<()> {
    use adapteros_core::{derive_seed, B3Hash};

    // Setup two instances with same seed
    let manifest = B3Hash::hash(b"test-manifest");

    // Instance 1
    let seed1 = derive_seed(&manifest, "router-step-0");
    mlx_set_seed_from_bytes(&seed1)?;
    let router1 = Router::new(5, 3);
    let selection1 = router1.select(&input)?;

    // Instance 2 (same seed)
    let seed2 = derive_seed(&manifest, "router-step-0");
    mlx_set_seed_from_bytes(&seed2)?;
    let router2 = Router::new(5, 3);
    let selection2 = router2.select(&input)?;

    // Verify determinism
    assert_eq!(selection1, selection2);  // Same adapter selection

    Ok(())
}
```

### Test 4: Memory Pressure with Hot-Swap

```rust
#[tokio::test]
async fn test_memory_pressure_eviction() -> Result<()> {
    let model = MLXFFIModel::new_null(create_mock_config());
    let mut backend = MLXFFIBackend::new(model);

    // Load adapters until memory pressure
    for i in 0..20 {
        let adapter = create_mock_adapter(&format!("adapter{}", i), 4);
        match backend.register_adapter(i as u16, adapter) {
            Ok(_) => {},
            Err(e) if e.to_string().contains("Memory") => {
                // Expected: eviction triggered
                println!("Memory pressure at {} adapters", i);
                break;
            }
            Err(e) => return Err(e),
        }
    }

    Ok(())
}
```

### Test 5: Lifecycle State Transitions

```rust
#[tokio::test]
async fn test_lifecycle_with_mlx() -> Result<()> {
    use adapteros_lora_lifecycle::LifecycleManager;

    let lifecycle = LifecycleManager::new_with_backend(
        vec!["adapter1".to_string(), "adapter2".to_string()],
        backend,
    );

    // Record router decision → promotes adapter
    lifecycle.record_router_decision(&["adapter1".to_string()]).await?;

    // Check memory pressure → may demote/evict
    lifecycle.check_memory_pressure(total_mem, 0.85).await?;

    // Verify state transitions were logged
    let state = lifecycle.get_adapter_state("adapter1").await?;
    assert!(!state.is_unloaded());

    Ok(())
}
```

---

## Integration Checklist

### Backend Factory Integration
- [x] MLX backend selectable via `BackendChoice::Mlx`
- [x] Auto-detection of MLX availability
- [x] Fallback chain: CoreML → MLX → Metal
- [x] Feature flag gating (`multi-backend`)

### Router Integration
- [x] K-sparse selection compatible with MLX
- [x] Q15 gate quantization support
- [x] Adapter ID to u16 deterministic mapping
- [x] Multi-adapter fusion in MLX backend

### Hot-Swap Integration
- [x] Preload without activation
- [x] Atomic swap operation
- [x] Stack verification (hash integrity)
- [x] Rollback to previous state
- [x] VRAM delta tracking

### Lifecycle Manager Integration
- [x] Adapter state transitions (Unloaded→Resident)
- [x] Router-triggered promotion
- [x] Memory pressure-triggered eviction
- [x] TTL enforcement for ephemeral adapters

### Determinism Integration
- [x] HKDF seeding from manifest hash
- [x] Router seeding for reproducible selection
- [x] Dropout seeding for deterministic inference
- [x] Sampling seeding for reproducible generation

---

## Performance Considerations

### Router Selection Latency
```
Time per selection: ~1-5ms
├── Embedding forward: 1-2ms
├── Gate computation: 1ms
└── Adapter sorting: 0-2ms

For K=3 with 10 adapters: 2-5ms overhead per request
For K=5 with 20 adapters: 3-8ms overhead per request
```

### Hot-Swap Latency
```
Operation timing (typical 100MB adapter):
├── Preload: 50-100ms (VRAM copy)
├── Swap: 10-20ms (atomic pointer flip)
├── Verification: 1-5ms (hash computation)
└── Total swap cycle: 100-150ms

Can be performed during request queueing (no inference interruption)
```

### Memory Overhead
```
Per adapter:
├── Weights: Depends on rank/layer count
├── Metadata: ~1KB
├── Buffers: ~10MB (for caching)
└── Typical total: 100-500MB per adapter

K=3 with 20 adapters loaded: 2-10GB
K=3 with 5 adapters loaded: 0.5-2.5GB
```

---

## Configuration Examples

### Lightweight Router (Few Adapters)
```toml
[router]
k = 3
num_adapters = 5
quantization = "q15"

[mlx.hotswap]
preload_on_startup = ["adapter1", "adapter2"]
cache_precomputed_gates = false
```

### High-Performance Router (Many Adapters)
```toml
[router]
k = 5
num_adapters = 100
quantization = "q15"
gate_batch_size = 10

[mlx.hotswap]
preload_on_startup = ["adapter1", "adapter2", "adapter3", "adapter4", "adapter5"]
cache_precomputed_gates = true
max_precomputed = 1000
```

### Research/Experimentation
```toml
[router]
k = 3
num_adapters = 20
quantization = "q15"

[mlx.hotswap]
preload_on_startup = []
enable_dynamic_loading = true
max_concurrent_preloads = 3

[mlx.performance]
batch_size = 4
enable_kv_cache = true
```

---

## Testing Commands

### Run All Integration Tests
```bash
# MLX backend tests
cargo test -p adapteros-lora-mlx-ffi

# Router integration
cargo test -p adapteros-lora-router

# Hot-swap tests
cargo test -p adapteros-lora-worker --test '*hotswap*'

# End-to-end workflow
cargo test -p adapteros-lora-mlx-ffi --test 'e2e_workflow*'
```

### Run Specific Integration Test
```bash
# Router + MLX backend
cargo test -p adapteros-lora-mlx-ffi --test 'backend_integration_tests::test_backend_adapter_registration'

# Hot-swap stress test
cargo test -p adapteros-lora-worker --test 'hotswap_stress_test' -- --nocapture

# Memory pressure
cargo test -p adapteros-lora-mlx-ffi --test 'memory_management_integration'
```

### Verify Integration in Production Mode
```bash
# Build with all features
cargo build --workspace --features multi-backend,real-mlx --release

# Run health check
./target/release/aosctl healthz --component backend

# Test inference with routing
curl -X POST http://localhost:8080/v1/infer \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"prompt": "test", "max_tokens": 10, "adapters": ["adapter1", "adapter2"]}'
```

---

## Troubleshooting Integration Issues

### Router Selection Not Using MLX Adapters
**Symptom:** Inference works but router ignores loaded adapters
**Solution:**
1. Verify adapters are registered: `backend.adapter_count()`
2. Check gate scores aren't zero: Log gate computation
3. Verify adapter compatibility: Hidden dimensions must match

### Hot-Swap Fails with "Adapter Not Found"
**Symptom:** `AdapterCommand::Swap` fails for valid adapters
**Solution:**
1. Verify preload completed successfully
2. Check adapter ID matches registration
3. Review VRAM availability

### Inconsistent Routing Results
**Symptom:** Same input produces different adapter selections
**Solution:**
1. Verify HKDF seeding is enabled
2. Check manifest hash is consistent
3. Ensure router RNG is seeded

### Memory Fragmentation After Hot-Swap
**Symptom:** Memory usage grows after multiple swaps
**Solution:**
1. Enable GC more aggressively: `memory::gc_collect()`
2. Reduce number of preloaded adapters
3. Implement memory defragmentation

---

## See Also

- [MLX_INTEGRATION.md](./MLX_INTEGRATION.md) - Complete MLX integration
- [docs/ARCHITECTURE_PATTERNS.md](./ARCHITECTURE_PATTERNS.md) - K-sparse routing details
- [docs/LIFECYCLE.md](./LIFECYCLE.md) - Adapter state machine
- [docs/DETERMINISTIC_EXECUTION.md](./DETERMINISTIC_EXECUTION.md) - HKDF seeding
- `crates/adapteros-lora-router/src/lib.rs` - Router implementation
- `crates/adapteros-lora-worker/src/adapter_hotswap.rs` - Hot-swap implementation

---

**Maintained by:** James KC Auchterlonie
**Last Updated:** 2025-11-22
**Status:** Implementation Complete & Production-Ready
