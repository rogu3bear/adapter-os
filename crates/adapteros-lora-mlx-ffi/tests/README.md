# MLX FFI Backend Test Suite

Comprehensive test suite for the MLX FFI backend implementation.

## Test Files

### 1. `array_operations_tests.rs`
Tests for tensor creation, manipulation, and arithmetic operations.

**Test Modules:**
- `array_creation_tests` - Tensor creation from data (float/int, various shapes)
- `array_arithmetic_tests` - Add, multiply, broadcasting operations
- `array_matmul_tests` - Matrix multiplication operations
- `array_data_access_tests` - Data retrieval and shape access
- `array_dtype_tests` - Data type handling
- `array_edge_cases_tests` - Large tensors, zeros, negatives, extreme values

**Coverage:** 25+ tests

### 2. `lora_operations_tests.rs`
Tests for LoRA adapter operations and routing logic.

**Test Modules:**
- `lora_adapter_tests` - Adapter creation, weight management, parameter counting
- `lora_config_tests` - Configuration parsing and serialization
- `lora_routing_tests` - Multi-LoRA routing, top-K selection, gate weights
- `lora_transform_tests` - LoRA transformations, alpha scaling
- `lora_loading_tests` - Adapter loading, hash consistency

**Coverage:** 30+ tests

### 3. `backend_integration_tests.rs`
Tests for FusedKernels trait implementation and backend lifecycle.

**Test Modules:**
- `backend_tests` - Backend creation, adapter registration/hot-swap
- `fused_kernels_trait_tests` - Determinism attestation, load/run operations
- `router_ring_tests` - RouterRing creation and manipulation
- `io_buffers_tests` - IoBuffers operations
- `adapter_lifecycle_tests` - Load/unload/replace workflows
- `mock_model_tests` - Mock model and adapter testing

**Coverage:** 35+ tests

### 4. `model_loading_tests.rs`
Tests for model loading, configuration, and inference.

**Test Modules:**
- `model_config_tests` - Config parsing, defaults, serialization
- `model_loading_tests` - Path validation, error handling
- `forward_pass_tests` - Single/multiple token inference
- `generation_tests` - Text generation (placeholder)
- `model_thread_safety_tests` - Concurrent model access
- `embedding_config_tests` - Embedding model configuration
- `embedding_model_tests` - Embedding model operations (requires real models)

**Coverage:** 20+ tests

### 5. `error_handling_tests.rs`
Tests for error propagation across the FFI boundary.

**Test Modules:**
- `ffi_error_tests` - FFI-level error handling
- `tensor_error_tests` - Tensor operation errors
- `adapter_error_tests` - Adapter lifecycle errors
- `lora_error_tests` - LoRA operation errors
- `model_error_tests` - Model loading errors
- `memory_error_tests` - Memory operation safety
- `boundary_error_tests` - Null pointer safety
- `routing_error_tests` - Routing edge cases (NaN, infinity)
- `concurrency_error_tests` - Concurrent access safety

**Coverage:** 30+ tests

### 6. `deterministic_seeding_tests.rs`
Tests for HKDF-based deterministic seeding.

**Test Modules:**
- `seeding_basic_tests` - Basic seeding operations
- `seeding_domain_separation_tests` - Domain-separated seeds
- `seeding_workflow_tests` - Full initialization workflows
- `seeding_edge_cases_tests` - Empty labels, long labels, special chars
- `seeding_reproducibility_tests` - Seed consistency verification
- `seeding_error_handling_tests` - Invalid seed handling
- `seeding_integration_tests` - Hierarchical and contextual seeding

**Coverage:** 30+ tests

### 7. `memory_tracking_tests.rs` (Existing)
Tests for memory allocation tracking and management.

**Test Modules:**
- `memory_tracking_tests` - Basic memory stats
- `memory_api_interface_tests` - Public API surface
- `memory_lifecycle_scenario_tests` - Checkpoint scenarios
- `memory_boundary_tests` - Large/small value handling

**Coverage:** 25+ tests

### 8. `mlx_seed_test.rs` (Existing)
Tests for MLX backend seeding implementation.

**Coverage:** 10+ tests

## Total Test Coverage

- **Total Test Files:** 8
- **Total Tests:** ~200+
- **Test Modules:** 40+

## Running Tests

### Run all tests:
```bash
cargo test -p adapteros-lora-mlx-ffi
```

### Run specific test file:
```bash
cargo test -p adapteros-lora-mlx-ffi --test array_operations_tests
cargo test -p adapteros-lora-mlx-ffi --test lora_operations_tests
cargo test -p adapteros-lora-mlx-ffi --test backend_integration_tests
cargo test -p adapteros-lora-mlx-ffi --test model_loading_tests
cargo test -p adapteros-lora-mlx-ffi --test error_handling_tests
cargo test -p adapteros-lora-mlx-ffi --test deterministic_seeding_tests
```

### Run with experimental features:
```bash
cargo test -p adapteros-lora-mlx-ffi --features experimental-backends
```

### Run ignored tests (require real models):
```bash
cargo test -p adapteros-lora-mlx-ffi -- --ignored
```

### Run with output:
```bash
cargo test -p adapteros-lora-mlx-ffi -- --nocapture
```

## Test Organization

Tests are organized by functionality:

1. **Low-level FFI operations** - Array/tensor operations
2. **LoRA operations** - Adapter management and routing
3. **Backend integration** - FusedKernels trait, lifecycle
4. **Model operations** - Loading, inference, configuration
5. **Error handling** - All error paths and edge cases
6. **Deterministic execution** - HKDF seeding, reproducibility
7. **Memory management** - Allocation tracking, GC

## Ignored Tests

Some tests are marked with `#[ignore]` because they require:
- Real MLX model files (config.json, weights, tokenizer)
- Actual MLX runtime environment
- GPU/Metal availability

These can be run manually when model files are available:
```bash
cargo test -p adapteros-lora-mlx-ffi -- --ignored --test-threads=1
```

## Test Features

- **Mock implementations** - Test without real MLX models
- **Thread safety tests** - Verify concurrent access
- **Edge case coverage** - NaN, infinity, empty inputs, null pointers
- **Error path testing** - All error conditions tested
- **Integration tests** - Full workflow scenarios

## Dependencies

Tests depend on:
- `adapteros-core` - Error types, HKDF seeding
- `adapteros-lora-kernel-api` - FusedKernels trait, RouterRing
- `tempfile` - Temporary directory creation
- Standard Rust test framework

## Notes

- Most tests use mock implementations to avoid requiring real MLX models
- Error handling tests verify graceful degradation
- Deterministic seeding tests verify HKDF integration
- Concurrency tests verify thread safety of shared state
- All public APIs are tested

## Future Work

- Add performance benchmarks
- Add integration tests with real MLX models (when available)
- Add memory leak detection tests
- Add stress tests for concurrent operations
- Add fuzz testing for FFI boundary
