# LoRA Buffer Population Integration Tests

## Overview

This test suite provides thin integration coverage for the shared LoRA buffer population mechanism in the Metal kernel implementation. It exercises the code path where multiple adapters are toggled to verify correct buffer management.

## Test Coverage

### 1. `test_single_adapter_population`
- **Purpose**: Verifies basic single adapter population and idempotency
- **Coverage**: Tests that the same adapter can be activated multiple times without errors
- **Path**: `MetalKernels::populate_lora_for_adapter` via `run_step`

### 2. `test_multiple_adapter_toggle_sequence`
- **Purpose**: Tests toggling between different adapter sets (A→B→A)
- **Coverage**: Exercises the `populated_lora_adapters` HashSet tracking mechanism
- **Validation**: Ensures different adapters can be activated in sequence without conflicts

### 3. `test_population_idempotency`
- **Purpose**: Stress-tests idempotency by calling the same adapter 100 times
- **Coverage**: Verifies that `populated_lora_adapters.insert()` properly prevents duplicate work
- **Expected**: No errors or memory issues after repeated calls

### 4. `test_edge_case_adapter_ids`
- **Purpose**: Tests boundary conditions for adapter IDs
- **Coverage**: 
  - Adapter ID 0 (reserved for base model, should be skipped)
  - Mixed valid and invalid IDs
- **Validation**: System handles edge cases gracefully

### 5. `test_rapid_adapter_switching`
- **Purpose**: Simulates realistic workload with frequent adapter changes
- **Coverage**: 50 iterations across 10 different adapter patterns
- **Validation**: Stress-tests the tracking mechanism under load

### 6. `test_concurrent_adapter_activation`
- **Purpose**: Tests activating multiple adapters simultaneously
- **Coverage**: All combinations of adapter pairs and triplets
- **Validation**: Buffer population handles concurrent activation correctly

## Code Path Exercised

```
run_step()
  → run_transformer_layers()
    → ensure_lora_buffers()  // Allocates shared buffers if needed
    → populate_lora_for_adapter()  // For each active adapter
      → populated_lora_adapters.insert()  // Idempotency check
      → copy_lora_from_weights() OR fill_buffer_with_rng()
```

## Key Components Tested

1. **Buffer Allocation** (`ensure_lora_buffers`)
   - Allocates shared Metal buffers for all LoRA matrices
   - Called once per kernel instance

2. **Population Tracking** (`populated_lora_adapters: HashSet<u32>`)
   - Prevents duplicate population of the same adapter
   - Ensures idempotency of the population operation

3. **Adapter Weight Loading** (`populate_lora_for_adapter`)
   - Copies adapter weights from safetensors into shared buffers
   - Or generates deterministic random weights if not available

## Running the Tests

### Default (Ignored)
```bash
cargo test --test lora_buffer_population_integration
# Output: 0 passed; 0 failed; 6 ignored
```

### With Metal Kernel
```bash
# First, ensure Metal kernel is properly built and signed
make build-metal  # or equivalent build command

# Then run with --ignored flag
cargo test --test lora_buffer_population_integration -- --ignored --nocapture
```

### Prerequisites
- macOS with Metal support
- Properly signed Metal kernel library
- Valid cryptographic signing keys in place

## Why Tests Are Ignored by Default

These tests require:
1. A full Metal kernel build with proper cryptographic signatures
2. The `SIGNING_PUBLIC_KEY_PEM` to be valid and accessible
3. The embedded `metallib_manifest.json` and signature to match

Without these prerequisites, `MetalKernels::new()` will fail with a crypto error during manifest verification. This is intentional security behavior.

## CI/CD Integration

These tests should be run:
- On macOS CI runners with proper build infrastructure
- After successful Metal kernel compilation
- As part of full integration test suite (not unit tests)

## Related Files

- `crates/adapteros-lora-kernel-mtl/src/lib.rs` - MetalKernels implementation
- `crates/adapteros-lora-kernel-mtl/src/manifest.rs` - Manifest verification
- `tests/adapter_hotswap.rs` - Related adapter lifecycle tests
- `tests/integration_tests.rs` - Other Metal kernel integration tests

