# Corrected Codex Prompts

## Prompt 1: Metal 3.x Compute Shader Registry
```
Implement Metal 3.x compute shader registry for AdapterOS.

TARGET: crates/adapteros-lora-kernel-mtl/src/compute_shaders.rs

REQUIREMENTS:
- Create ComputeShaderRegistry struct with BTreeMap storage
- Track ComputeShaderDescriptor (name, source_hash, threadgroup_size, bindings)
- Record ShaderExecutionStats (dispatches, last_used, total_work_items)
- Methods: register(), descriptor(), record_dispatch(), stats(), iter()
- Use B3Hash for source hashing
- Use AosError::Kernel for errors
- Keep under 300 lines
- Add tests

INTEGRATION:
- Export module in crates/adapteros-lora-kernel-mtl/src/lib.rs
- Use adapteros_core::Result and B3Hash types
- Follow existing Metal kernel patterns

Create focused PR with just this functionality.
```

## Prompt 2: MLX Backend Integration
```
Integrate MLX backend with AdapterOS worker runtime.

TARGET: crates/adapteros-lora-worker/src/backend_factory.rs

REQUIREMENTS:
- Fix existing MLX backend implementation (currently disabled)
- Add MLXBackend struct implementing FusedKernels trait
- Enable MLX variant in BackendChoice enum (already exists)
- Add deterministic execution using HKDF seeding
- Implement FusedKernels methods with stub implementations
- Use existing experimental-backends feature flag
- Keep under 400 lines
- Add integration tests

INTEGRATION:
- Fix existing BackendChoice::Mlx variant
- Use adapteros_lora_kernel_api::FusedKernels trait
- Follow existing backend patterns
- Use existing error handling

Create focused PR with just this functionality.
```

## Prompt 3: CoreML Backend Implementation
```
Implement CoreML backend for AdapterOS worker runtime.

TARGET: crates/adapteros-lora-worker/src/backend_factory.rs

REQUIREMENTS:
- Implement CoreML backend (currently marked as "not yet implemented")
- Add CoreMLBackend struct implementing FusedKernels trait
- Enable CoreML variant in BackendChoice enum (already exists)
- Add deterministic execution using HKDF seeding
- Implement FusedKernels methods with stub implementations
- Use existing experimental-backends feature flag
- Keep under 400 lines
- Add integration tests

INTEGRATION:
- Implement existing BackendChoice::CoreML variant
- Use adapteros_lora_kernel_api::FusedKernels trait
- Follow existing backend patterns
- Use existing error handling

Create focused PR with just this functionality.
```

## Parallel Execution Safety
- Different crates: `adapteros-lora-kernel-mtl`, `adapteros-lora-worker`
- No file overlap between prompts
- Independent functionality
- Clear integration points

## Expected Output
- PR 1: Metal 3.x compute shader registry (~300 lines)
- PR 2: MLX backend integration (~400 lines)  
- PR 3: CoreML backend implementation (~400 lines)

These prompts can be run simultaneously without conflicts.

## Additional Parallel Prompts

### Prompt 4: Verification Framework Implementation
```
Implement actual verification logic for AdapterOS verification framework.

TARGET: crates/adapteros-verification/src/unified_validation.rs

REQUIREMENTS:
- Replace TODO stubs with actual verification implementations
- Implement verify_code_quality() with clippy/rustfmt integration
- Implement verify_security() with security scanning tools
- Implement verify_performance() with benchmarking tools
- Implement verify_compliance() with compliance checking
- Implement verify_system_integrity() with integrity checks
- Keep under 500 lines
- Add comprehensive tests

INTEGRATION:
- Use existing verification framework structure
- Follow existing error handling patterns
- Use adapteros_core::Result types
- Integrate with existing telemetry system

Create focused PR with just this functionality.
```

### Prompt 5: Testing Framework Implementation
```
Implement actual test execution logic for AdapterOS testing framework.

TARGET: crates/adapteros-testing/src/unified_framework.rs

REQUIREMENTS:
- Replace TODO stubs with actual test execution
- Implement run_test_step() with different action types
- Implement run_assertion() with assertion logic
- Add support for different test frameworks
- Add test result aggregation and reporting
- Keep under 400 lines
- Add comprehensive tests

INTEGRATION:
- Use existing testing framework structure
- Follow existing error handling patterns
- Use adapteros_core::Result types
- Integrate with existing telemetry system

Create focused PR with just this functionality.
```

### Prompt 6: CLI Output Writer Enhancement
```
Enhance CLI output writer with missing methods and formatting.

TARGET: crates/adapteros-cli/src/output.rs

REQUIREMENTS:
- Add missing table() method for formatted output
- Add additional output formatting helpers
- Fix struct field mismatches with database schema
- Add progress indicators and status updates
- Keep under 300 lines
- Add comprehensive tests

INTEGRATION:
- Use existing OutputWriter structure
- Follow existing CLI patterns
- Use existing error handling
- Integrate with existing telemetry system

Create focused PR with just this functionality.
```

## Parallel Execution Safety
- Different crates: `adapteros-verification`, `adapteros-testing`, `adapteros-cli`
- No file overlap between prompts
- Independent functionality
- Clear integration points

## Expected Output
- PR 4: Verification framework implementation (~500 lines)
- PR 5: Testing framework implementation (~400 lines)
- PR 6: CLI output writer enhancement (~300 lines)

These additional prompts can be run simultaneously with the original three without conflicts.

## More Parallel Prompts

### Prompt 7: MLX FFI Implementation
```
Implement actual MLX FFI functionality for AdapterOS.

TARGET: crates/adapteros-lora-mlx-ffi/src/lib.rs

REQUIREMENTS:
- Replace placeholder generate() method with actual tokenization and generation
- Implement forward_with_hidden_states() with real hidden state extraction
- Add proper error handling and validation
- Integrate with existing MLX C++ wrapper
- Keep under 400 lines
- Add comprehensive tests

INTEGRATION:
- Use existing MLXFFIModel structure
- Follow existing error handling patterns
- Use adapteros_core::Result types
- Integrate with existing telemetry system

Create focused PR with just this functionality.
```

### Prompt 8: Domain Adapter API Implementation
```
Implement actual domain adapter execution logic for AdapterOS API.

TARGET: crates/adapteros-server-api/src/handlers/domain_adapters.rs

REQUIREMENTS:
- Replace placeholder execute_domain_adapter() with real execution
- Implement actual adapter preparation, forward pass, and postprocessing
- Add proper input/output validation and error handling
- Integrate with deterministic executor and trace collection
- Keep under 500 lines
- Add comprehensive tests

INTEGRATION:
- Use existing domain adapter handlers structure
- Follow existing API patterns
- Use adapteros_core::Result types
- Integrate with existing telemetry system

Create focused PR with just this functionality.
```

### Prompt 9: Noise Tracker Implementation
```
Implement actual noise tracking functionality for AdapterOS Metal kernels.

TARGET: crates/adapteros-lora-kernel-mtl/src/noise_tracker.rs

REQUIREMENTS:
- Replace placeholder extract_buffer_data() with real Metal buffer reading
- Implement create_reference_data() with high-precision computation
- Add proper tensor data extraction and conversion
- Integrate with existing noise tracking infrastructure
- Keep under 300 lines
- Add comprehensive tests

INTEGRATION:
- Use existing NoiseTracker structure
- Follow existing Metal kernel patterns
- Use adapteros_core::Result types
- Integrate with existing telemetry system

Create focused PR with just this functionality.
```

## Parallel Execution Safety
- Different crates: `adapteros-lora-mlx-ffi`, `adapteros-server-api`, `adapteros-lora-kernel-mtl`
- No file overlap between prompts
- Independent functionality
- Clear integration points

## Expected Output
- PR 7: MLX FFI implementation (~400 lines)
- PR 8: Domain adapter API implementation (~500 lines)
- PR 9: Noise tracker implementation (~300 lines)

These additional prompts can be run simultaneously with the original six without conflicts.
