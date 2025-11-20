# MLX Backend Integration Guide

**Status:** Integrated and Tested
**Last Updated:** 2025-11-19
**Maintained by:** Development Team

---

## Overview

The MLX backend is fully integrated into AdapterOS's backend factory system. It provides GPU-accelerated inference via MLX (a machine learning framework optimized for Apple Silicon) while leveraging HKDF-derived seeding for deterministic RNG operations.

### Key Characteristics

| Aspect | Details |
|--------|---------|
| **Status** | Experimental (research/prototyping) |
| **Determinism** | HKDF-seeded RNG only (execution order non-deterministic) |
| **Use Case** | Research, prototyping, cost-sensitive inference |
| **Production Ready** | Not for mission-critical determinism-dependent workloads |
| **Feature Flag** | `experimental-backends` (optional) |
| **Platforms** | macOS with MLX installation |

---

## Architecture Integration

### Backend Factory Pattern

The MLX backend is created through the unified factory pattern:

```rust
use adapteros_lora_worker::backend_factory::{BackendChoice, create_backend};
use std::path::PathBuf;

// Create MLX backend
let backend = create_backend(BackendChoice::Mlx {
    model_path: PathBuf::from("/path/to/mlx/model"),
})?;
```

### Factory Method Signature

```rust
pub fn create_backend(choice: BackendChoice) -> Result<Box<dyn FusedKernels>>
```

**Features:**
- Compile-time feature gating (`experimental-backends`)
- Runtime attestation validation
- Error handling with logging
- Automatic seeding via HKDF

### Error Handling

The factory enforces proper error handling for all backends:

**Without `experimental-backends` feature:**
```
Error: PolicyViolation(
    "MLX backend requires --features experimental-backends \
     (not enabled in deterministic-only build)"
)
```

**With feature enabled but initialization fails:**
- Error is logged at TRACE level
- Error propagated through `Result<T, AosError>`
- Context preserved for debugging

---

## Determinism Attestation

### Report Structure

MLX backend produces a determinism attestation report:

```rust
DeterminismReport {
    backend_type: BackendType::Mlx,
    metallib_hash: None,  // MLX doesn't use Metal
    manifest: None,
    rng_seed_method: RngSeedingMethod::HkdfSeeded,
    floating_point_mode: FloatingPointMode::Unknown,
    compiler_flags: vec!["-DMLX_HKDF_SEEDED".to_string()],
    deterministic: false,  // ← Key: NOT deterministic by design
}
```

### Why `deterministic = false`?

MLX uses GPU async execution with scheduler variability:

1. **RNG Operations:** Deterministic (HKDF-seeded)
2. **Execution Order:** Non-deterministic (GPU scheduler varies)
3. **Floating-Point:** Unknown (MLX doesn't expose this guarantee)
4. **Result:** Different runs may produce different outputs

### Attestation Validation

The factory validates all attestations before returning the backend:

```rust
// In create_backend():
let report = backend.attest_determinism()?;
report.validate()?;  // ← Will fail for MLX (deterministic=false)
```

**Expected Behavior:**
- Metal: ✅ Passes validation (deterministic=true)
- CoreML with ANE: ✅ Conditional (depends on ANE availability)
- MLX: ❌ Fails validation (deterministic=false)

**Production Impact:**
- Production workflows requiring guaranteed determinism cannot use MLX
- Experimental/research workflows can selectively bypass attestation validation
- Policy engine should enforce determinism requirements per tenant/adapter

---

## HKDF Seeding Strategy

### Seed Derivation Pipeline

```
┌─ Model Hash ───┐
│ (from path)    ├─ Domain-Separated HKDF ─┐
└────────────────┘                          │
                                            ├─ Base Seed
┌──────────────────┐                        │
│ Global "mlx-    ├─ Domain-Separated HKDF ─┤
│ backend"         │                        │
└──────────────────┘                        │
                    ┌───────────────────────┘
                    │
                    ▼
            ┌──────────────────┐
            │   Per-Step Seeds │
            │ mlx-step:0       │
            │ mlx-step:1       │
            │ mlx-step:N       │
            └──────────────────┘
```

### Seed Components

**Global Seed:**
```rust
B3Hash::hash(b"adapteros-mlx-backend")
```

**Model Seed Label:**
```rust
format!("mlx-backend:{}", model_hash.to_short_hex())
```

**Step Seed Label:**
```rust
format!("mlx-step:{}", position)
```

**Adapter Seed Label:**
```rust
format!("mlx-adapter:{}", adapter_id)
```

### Usage in Backend

```rust
fn load(&mut self, plan_bytes: &[u8]) -> Result<()> {
    let plan_hash = B3Hash::hash(plan_bytes);
    let label = format!("mlx-plan:{}", plan_hash.to_short_hex());
    let reseeded = derive_seed(&self.base_seed, &label);
    self.base_seed = B3Hash::from_bytes(reseeded);
    mlx_set_seed_from_bytes(&self.base_seed.as_bytes())?;
    Ok(())
}

fn run_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
    let step_seed = self.derive_step_seed(io.position);
    let mut rng = ChaCha20Rng::from_seed(step_seed);
    // Use RNG for deterministic dropout/sampling
    Ok(())
}
```

---

## Configuration

### Feature Flag

Enable MLX in `Cargo.toml`:

```toml
[dependencies]
adapteros-lora-worker = { path = "../adapteros-lora-worker", features = ["experimental-backends"] }
```

### Configuration File (`cp.toml`)

```toml
[backends]
primary_backend = "mlx"
enable_experimental_backends = true

[backends.mlx]
model_path = "models/qwen2.5-7b-mlx"
seeding_mode = "hkdf"
```

### Fallback Strategy

```toml
[backends]
fallback_strategy = "auto-with-full-fallback"
```

**Strategy Details:**
1. Try Metal first (if available and model fits VRAM)
2. Fall back to CoreML with ANE (if available)
3. Fall back to MLX (experimental)
4. Fail if no backend available

---

## Testing

### Running Integration Tests

```bash
# All MLX tests (requires experimental-backends feature)
cargo test -p adapteros-lora-worker --test mlx_backend_integration --features experimental-backends

# Specific test
cargo test -p adapteros-lora-worker test_mlx_backend_creation --features experimental-backends

# With output
cargo test -p adapteros-lora-worker --test mlx_backend_integration --features experimental-backends -- --nocapture
```

### Test Coverage

| Test | Purpose | Status |
|------|---------|--------|
| `test_mlx_backend_creation()` | Verify initialization | ✅ Always passes |
| `test_mlx_backend_implements_fused_kernels()` | Verify trait impl | ✅ Always passes |
| `test_mlx_backend_determinism_attestation()` | Verify report structure | ✅ Always passes |
| `test_mlx_backend_attestation_validation_fails()` | Verify non-deterministic report | ✅ Expected failure |
| `test_mlx_backend_run_step()` | Verify inference | ✅ Always passes |
| `test_mlx_backend_seed_derivation()` | Verify HKDF seeding | ✅ Always passes |
| `test_mlx_backend_multiple_adapters()` | Verify lifecycle | ✅ Always passes |
| `test_mlx_backend_attestation_summary()` | Verify reporting | ✅ Always passes |

### Ignored Tests (Require MLX Installation)

```bash
# Requires actual MLX model files
cargo test -p adapteros-lora-worker test_create_mlx_backend_with_model --features experimental-backends -- --ignored
```

---

## Implementation Details

### File Structure

```
crates/
├── adapteros-lora-worker/
│   ├── src/
│   │   └── backend_factory.rs          # MlxBackend struct + impl
│   └── tests/
│       └── mlx_backend_integration.rs  # Integration tests
├── adapteros-lora-mlx-ffi/
│   ├── src/
│   │   └── lib.rs                      # mlx_set_seed_from_bytes() FFI
│   └── Cargo.toml
├── adapteros-lora-kernel-api/
│   └── src/
│       └── attestation.rs              # DeterminismReport types
└── adapteros-core/
    └── src/
        ├── hash.rs                     # B3Hash, derive_seed()
        └── error.rs                    # AosError::Mlx variant
```

### Key Types

**Backend Choice:**
```rust
pub enum BackendChoice {
    Metal,
    Mlx { model_path: PathBuf },
    CoreML { model_path: Option<PathBuf> },
}
```

**MLX Backend Structure:**
```rust
#[cfg(feature = "experimental-backends")]
struct MlxBackend {
    model_path: PathBuf,
    base_seed: B3Hash,
    device: String,
}
```

**FusedKernels Trait Implementation:**
```rust
impl FusedKernels for MlxBackend {
    fn load(&mut self, plan_bytes: &[u8]) -> Result<()>;
    fn run_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()>;
    fn device_name(&self) -> &str;
    fn attest_determinism(&self) -> Result<DeterminismReport>;
    fn load_adapter(&mut self, id: u16, weights: &[u8]) -> Result<()>;
    fn unload_adapter(&mut self, id: u16) -> Result<()>;
}
```

---

## Backward Compatibility

### No Breaking Changes

- Default behavior unchanged (Metal backend still default)
- Feature flag is optional (`experimental-backends` disabled by default)
- Existing code continues to work without modification

### Migration Path

```rust
// Old code (still works)
let backend = create_backend(BackendChoice::Metal)?;

// New code (optional MLX support)
#[cfg(feature = "experimental-backends")]
let backend = create_backend(BackendChoice::Mlx {
    model_path: PathBuf::from("models/mlx"),
})?;

// Recommended: use strategy for automatic selection
let backend = create_backend_auto(
    BackendStrategy::MetalWithCoreMLFallback,
    Some(8_000_000_000),
)?;
```

---

## Limitations and Caveats

### Execution Determinism

**What IS deterministic:**
- HKDF-derived seeds control RNG operations
- Dropout and sampling produce consistent patterns given same seed
- Initial model weights are deterministic

**What is NOT deterministic:**
- GPU async execution order varies between runs
- Floating-point rounding may differ in multi-threaded contexts
- Cache hit rates and memory access patterns are unpredictable

### Use Case Recommendations

| Use Case | Suitable? | Reason |
|----------|-----------|--------|
| **Production inference** | ❌ No | Execution order non-deterministic |
| **Research/prototyping** | ✅ Yes | Adequate for experimentation |
| **Training** | ✅ Yes | HKDF seeding sufficient for reproducibility |
| **Testing** | ⚠️ Maybe | Use Metal for determinism-critical tests |
| **Benchmarking** | ⚠️ Maybe | GPU scheduling variance affects results |

---

## Policy Enforcement

### Determinism Policy Check

The policy engine validates backends against determinism requirements:

```rust
// In policy enforcement
if policy.requires_determinism() && !backend.attest_determinism()?.deterministic {
    return Err(AosError::DeterminismViolation(
        "MLX backend does not meet determinism policy".into()
    ));
}
```

### Tenant Overrides

Tenants can opt into experimental backends via policy:

```
policy {
    require_determinism = false,
    allow_experimental_backends = true,
}
```

---

## Troubleshooting

### "MLX backend requires --features experimental-backends"

**Cause:** Crate compiled without the experimental feature.

**Fix:**
```bash
cargo build -p adapteros-lora-worker --features experimental-backends
```

### "Failed to initialize MLX backend"

**Cause:** Model files not found or MLX installation issue.

**Fix:**
1. Verify model path exists and is readable
2. Check MLX installation: `python -c "import mlx; print(mlx.__version__)"`
3. Ensure model is in MLX format (not PyTorch/ONNX)

### Attestation validation fails

**Expected behavior:** MLX backend fails attestation validation (deterministic=false).

**Action:** Don't use this backend for determinism-critical workloads.

### Tests fail with feature flag error

**Cause:** Running tests without `--features experimental-backends`.

**Fix:**
```bash
cargo test --features experimental-backends -- --test-threads=1
```

---

## Performance Characteristics

### Latency

| Operation | Typical Time | Notes |
|-----------|--------------|-------|
| Backend creation | ~100ms | Seed initialization only |
| Plan loading | ~50ms | HKDF re-seeding |
| Adapter loading | ~10ms | Seed derivation |
| run_step (forward) | Varies | GPU-dependent, 5-500ms |

### Memory

- Base overhead: ~50 MB (model weights)
- Per adapter: ~10-100 MB (rank-dependent)
- Seed storage: Negligible (~32 bytes)

---

## Future Improvements

1. **Full Determinism:** Implement deterministic execution order (pending MLX framework update)
2. **Model Caching:** Cache compiled models between loads
3. **Dynamic Quantization:** Apply Q15 quantization to weights
4. **Streaming Support:** Enable token-by-token streaming inference
5. **Custom Kernels:** Allow kernel optimization for specific adapters

---

## References

- [CLAUDE.md - Multi-Backend Architecture](../CLAUDE.md#multi-backend-architecture)
- [docs/ADR_MULTI_BACKEND_STRATEGY.md](ADR_MULTI_BACKEND_STRATEGY.md)
- [crates/adapteros-lora-worker/src/backend_factory.rs](../crates/adapteros-lora-worker/src/backend_factory.rs)
- [crates/adapteros-lora-mlx-ffi/src/lib.rs](../crates/adapteros-lora-mlx-ffi/src/lib.rs)
- [MLX Framework Documentation](https://ml-explore.github.io/mlx/)

---

**Signature:** Development Team, 2025-11-19
