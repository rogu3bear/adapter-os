# Deterministic Execution Validation

This document captures validation findings for deterministic routing and kernel execution as required for production hardening.

## Router Determinism

### Seed-Based Initialization
- Router accepts a 32-byte seed parameter via `Router::new()` constructor
- Seed is used for deterministic telemetry sampling (BLAKE3-based sampling after first N tokens)
- Worker initialization derives router seed from manifest's global seed: `derive_seed(&manifest.seeds.global, "router")`

### Q15 Quantization
- Router gates are quantized to Q15 format (i16 values in range [-32768, 32767])
- Gate values are normalized to [0, 1] by dividing by 32767.0
- Q15 quantization ensures deterministic fixed-point representation

### Test Coverage
- Determinism tests verify identical inputs produce identical outputs across router instances
- Test suite: `crates/adapteros-lora-router/tests/determinism.rs`
- Fixed test failure in orthogonal constraints activation vector conversion

## Kernel Determinism

### Precompiled Metal Libraries
- Metal kernels are embedded as precompiled `.metallib` files via `include_bytes!()`
- Build-time hash verification ensures metallib matches expected binary
- Runtime kernel compilation is blocked by Determinism policy pack validator

### Hash Verification
- `METALLIB_HASH` constant verified at runtime against embedded binary
- Mismatch triggers `AosError::Kernel` preventing non-deterministic execution
- Hash stored in `metallib_manifest.json` with signature verification

### Evidence Capture
- Kernel tolerance checks recorded in evidence tracker
- Includes input/output checksums for deterministic replay validation
- Kernel hash stored in replay session metadata

## Global Seed Propagation

### Control Plane Initialization
- `global_seed` from config (`security.global_seed`) must be 32-byte hex string
- Seed initializes `DeterministicExecutor` via `init_global_executor()`
- Production mode validates seed format at startup

### Worker Seed Derivation
- Workers derive component seeds from global seed using HKDF
- Router seed: `derive_seed(&manifest.seeds.global, "router")`
- Generator seed: `derive_seed(&manifest.seeds.global, "generation")`
- Telemetry seed: `derive_seed(&manifest.seeds.global, "telemetry")`

## Policy Enforcement

### Determinism Policy Pack
- Validates against runtime kernel compilation attempts
- Enforces HKDF-seeded RNG usage (blocks non-deterministic RNG)
- Requires precompiled metallib embeds

### Router Policy Pack
- Validates K-sparse configuration (default K=3)
- Enforces Q15 gate quantization
- Checks entropy floor (default 0.02)

## Validation Status

✅ Router determinism: Seed-based initialization verified  
✅ Q15 quantization: Gates normalized deterministically  
✅ Kernel precompilation: Metallib embedding with hash verification  
✅ Global seed propagation: Control plane → worker → components  
✅ Policy enforcement: Determinism and router packs validate at runtime  
✅ Test coverage: Determinism test suite passes

## Recommendations

1. Add integration test verifying identical global seeds produce identical router decisions across runs
2. Document seed derivation labels in policy pack configuration
3. Add telemetry event for seed mismatch warnings
4. Validate metallib hash at worker startup, not just control plane

