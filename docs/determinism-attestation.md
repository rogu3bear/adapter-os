# Determinism Attestation System

## Overview

The determinism attestation system ensures that all kernel backends provide verifiable guarantees of deterministic execution. This is critical for AdapterOS's reproducibility requirements and production deployment.

## Architecture

### Attestation API

Every kernel backend must implement the `attest_determinism()` method from the `FusedKernels` trait:

```rust
pub trait FusedKernels: Send {
    fn attest_determinism(&self) -> Result<DeterminismReport>;
    // ... other methods
}
```

### Determinism Report

The `DeterminismReport` struct contains comprehensive attestation information:

```rust
pub struct DeterminismReport {
    /// Backend type (Metal, MLX, CoreML, Mock)
    pub backend_type: BackendType,
    
    /// Metallib hash (Metal backend only)
    pub metallib_hash: Option<B3Hash>,
    
    /// Kernel manifest with build metadata
    pub manifest: Option<KernelManifest>,
    
    /// RNG seeding method (HKDF, FixedSeed, SystemEntropy)
    pub rng_seed_method: RngSeedingMethod,
    
    /// Floating-point execution mode
    pub floating_point_mode: FloatingPointMode,
    
    /// Compiler flags used to build kernels
    pub compiler_flags: Vec<String>,
    
    /// Overall deterministic attestation
    pub deterministic: bool,
}
```

## Backend Types

### Metal Backend (Deterministic)

The Metal backend provides full determinism guarantees:

- **Metallib Hash**: Precompiled `.metallib` blobs with BLAKE3 hash verification
- **RNG Seeding**: HKDF-derived seeds from global seed
- **Floating-Point**: Deterministic mode (no fast-math)
- **Compiler Flags**: `-O2 -std=metal3.1` (no forbidden flags)

**Example Attestation:**

```rust
use adapteros_lora_kernel_mtl::MetalKernels;

let kernels = MetalKernels::new()?;
let report = kernels.attest_determinism()?;

assert!(report.deterministic);
assert_eq!(report.backend_type, BackendType::Metal);
assert!(report.metallib_hash.is_some());
```

### MLX Backend (Non-Deterministic)

The MLX backend is experimental and non-deterministic:

- **Requires**: `--features experimental-backends`
- **Use Case**: Development and experimentation only
- **NOT FOR PRODUCTION**: Cannot guarantee reproducible outputs

### Mock Backend (Testing)

The Mock backend provides deterministic test doubles:

- **Use Case**: Unit testing without GPU dependencies
- **Deterministic**: Yes (fixed seed RNG)
- **Metallib**: N/A (no actual kernel execution)

## Feature Flags

### Default: `deterministic-only`

```bash
cargo build  # Includes only Metal backend
```

Production builds use this configuration by default. Only the Metal backend is available.

### Optional: `experimental-backends`

```bash
cargo build --features experimental-backends
```

Enables MLX and CoreML backends for development/testing. **NOT FOR PRODUCTION**.

## Validation Rules

The attestation system enforces strict validation rules:

### 1. Overall Deterministic Flag

```rust
if !report.deterministic {
    return Err("Backend attestation indicates non-deterministic execution");
}
```

### 2. Backend Type Check

```rust
if !report.backend_type.is_deterministic_by_design() {
    return Err("Backend type is not deterministic by design");
}
```

Allowed: `Metal`, `Mock`
Disallowed: `MLX`, `CoreML` (unless explicitly validated)

### 3. Metallib Hash (Metal Only)

```rust
if backend_type == Metal && metallib_hash.is_none() {
    return Err("Metal backend must provide metallib hash");
}
```

### 4. RNG Seeding Method

```rust
if !rng_seed_method.is_deterministic() {
    return Err("RNG seeding method is not deterministic");
}
```

Allowed: `HkdfSeeded`, `FixedSeed(n)`
Disallowed: `SystemEntropy`

### 5. Forbidden Compiler Flags

```rust
let forbidden = ["-ffast-math", "-funsafe-math-optimizations"];
for flag in &report.compiler_flags {
    if forbidden.iter().any(|f| flag.contains(f)) {
        return Err("Forbidden compiler flag detected");
    }
}
```

### 6. Floating-Point Mode

```rust
if !floating_point_mode.is_deterministic() {
    return Err("Floating-point mode is not deterministic");
}
```

Allowed: `Deterministic`
Disallowed: `FastMath`, `Unknown`

## Policy Integration

The policy engine validates attestation during initialization:

```rust
impl InferencePipeline {
    pub fn new(..., kernels: Box<dyn FusedKernels>, policy: PolicyEngine, ...) -> Result<Self> {
        // Validate backend determinism before constructing pipeline
        let report = kernels.attest_determinism()?;
        policy.determinism_policy().validate_backend_attestation(&report)?;
        
        // ... rest of initialization
    }
}
```

This ensures that non-deterministic backends **cannot** be used for serving.

## CLI Tools

### Audit Determinism

Validate backend attestation from the command line:

```bash
# Audit Metal backend (default)
aosctl audit-determinism

# Output JSON format
aosctl audit-determinism --format json

# Audit MLX backend (requires experimental-backends feature)
aosctl audit-determinism --backend mlx --model-path ./models/qwen2.5-7b-mlx
```

**Exit Codes:**
- `0`: Backend passes determinism validation
- `1`: Backend fails determinism validation

**Example Output:**

```
🔍 Auditing Backend Determinism

=== Determinism Attestation Report ===

Backend Type:       Metal
Deterministic:      true
RNG Seeding:        HkdfSeeded
FP Mode:            Deterministic
Metallib Hash:      b3:f53b0b...

Compiler Flags:
  - -O2
  - -std=metal3.1

Kernel Manifest:
  Build Time:       2024-01-15T10:30:00Z
  Rust Version:     1.75.0
  SDK Version:      14.0
  xcrun Version:    xcrun 1.0.0

=== Validation ===

✓ Backend passes determinism validation

This backend is suitable for production use with deterministic execution.
```

## Serving Guards

The serve command includes compile-time and runtime guards:

### Compile-Time Guard

```rust
#[cfg(feature = "experimental-backends")]
{
    if !matches!(backend, BackendType::Metal) {
        warn!("⚠️  EXPERIMENTAL BACKENDS ENABLED - NOT FOR PRODUCTION ⚠️");
    }
}
```

### Runtime Guard

```rust
let backend_choice = match backend {
    BackendType::Mlx => {
        #[cfg(not(feature = "experimental-backends"))]
        {
            return Err("MLX backend requires --features experimental-backends");
        }
        // ...
    }
}
```

## Testing

Comprehensive test suites validate the attestation system:

### Attestation Tests

```rust
// tests/determinism_attestation.rs
#[test]
fn test_mock_kernels_attestation() {
    let kernels = MockKernels::new();
    let report = kernels.attest_determinism().unwrap();
    assert!(report.deterministic);
}

#[test]
fn test_attestation_validation_failure_forbidden_flags() {
    let report = DeterminismReport {
        compiler_flags: vec!["-ffast-math".to_string()],
        // ...
    };
    assert!(report.validate().is_err());
}
```

### Backend Selection Tests

```rust
// tests/backend_selection.rs
#[test]
#[cfg(not(feature = "experimental-backends"))]
fn test_mlx_backend_requires_feature_flag() {
    let result = create_backend(BackendChoice::Mlx { ... });
    assert!(result.is_err());
}
```

### Policy Integration Tests

```rust
// tests/policy_attestation.rs
#[test]
fn test_policy_validates_deterministic_attestation() {
    let policy = DeterminismPolicy::new(config);
    let report = /* Metal backend report */;
    assert!(policy.validate_backend_attestation(&report).is_ok());
}
```

## Promotion Gates

Promotion to production requires passing all attestation checks:

1. **Metallib Hash Verified**: Embedded hash matches build output
2. **RNG Seeding Validated**: HKDF-seeded or fixed seed
3. **Compiler Flags Checked**: No forbidden flags present
4. **Floating-Point Mode Confirmed**: Deterministic mode enabled
5. **Policy Enforcement**: All policy packs validate successfully
6. **Test Suite**: All attestation tests pass

## Best Practices

### 1. Always Use Default Features in Production

```bash
# Production build
cargo build --release

# Deployment
./target/release/aosctl serve --backend metal
```

### 2. Validate Attestation Before Serving

The system automatically validates attestation during initialization, but you can also verify manually:

```bash
aosctl audit-determinism --format json > attestation-report.json
```

### 3. Monitor Telemetry for Violations

Attestation failures are logged to telemetry:

```json
{
  "event_type": "policy.violation",
  "severity": "critical",
  "message": "Backend attestation failed",
  "details": {
    "backend_type": "MLX",
    "reason": "Non-deterministic RNG seeding"
  }
}
```

### 4. Use Experimental Backends Only for Development

```bash
# Development/testing only
cargo build --features experimental-backends
./target/debug/aosctl serve --backend mlx --model-path ./models/test
```

**Never use experimental backends in production.**

## Troubleshooting

### Attestation Validation Failed

**Error**: `Backend attestation indicates non-deterministic execution`

**Solution**: Check that you're using the Metal backend and that it was built with the correct toolchain.

### Metallib Hash Mismatch

**Error**: `Metallib hash mismatch! Expected: b3:abc... Got: b3:def...`

**Solution**: Rebuild kernels with `make metal` or `cargo clean && cargo build`.

### Forbidden Compiler Flag

**Error**: `Forbidden compiler flag detected: -ffast-math`

**Solution**: Remove fast-math flags from Metal shader build scripts.

### Backend Requires Feature Flag

**Error**: `MLX backend requires --features experimental-backends`

**Solution**: Either:
1. Rebuild with `cargo build --features experimental-backends` (dev only)
2. Switch to Metal backend for production use

## References

- **Trait Definition**: `crates/adapteros-lora-kernel-api/src/lib.rs`
- **Attestation Types**: `crates/adapteros-lora-kernel-api/src/attestation.rs`
- **Metal Implementation**: `crates/adapteros-lora-kernel-mtl/src/lib.rs`
- **Policy Validation**: `crates/adapteros-policy/src/packs/determinism.rs`
- **Backend Factory**: `crates/adapteros-lora-worker/src/backend_factory.rs`
- **CLI Audit Command**: `crates/adapteros-cli/src/commands/audit_determinism.rs`
- **Tests**: `tests/determinism_attestation.rs`, `tests/backend_selection.rs`, `tests/policy_attestation.rs`

