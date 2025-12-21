# AdapterOS Unit Testing Framework

A comprehensive unit testing framework designed specifically for the unique requirements of a deterministic inference runtime with async components, Metal kernels, and evidence-grounded responses.

## Overview

The AdapterOS Unit Testing Framework provides specialized testing utilities that address the challenges of testing:

- **Deterministic systems** that must produce identical outputs for identical inputs
- **Async components** with complex concurrency patterns
- **Metal/GPU kernels** requiring specialized validation
- **Evidence-grounded responses** needing quality and citation validation
- **Component isolation** for testing without external dependencies

## Architecture

The framework is organized into specialized modules:

### Core Modules

- **`mocks`** - Deterministic mocking utilities for controlled test doubles
- **`isolation`** - Component isolation helpers for testing in minimal environments
- **`property`** - Property-based testing infrastructure for mathematical properties
- **`async_utils`** - Async component testing utilities with timeout and determinism
- **`metal`** - Metal kernel testing helpers for GPU operations
- **`evidence`** - Evidence-grounded response testing utilities

### Key Design Principles

1. **Determinism** - All test utilities produce reproducible results
2. **Isolation** - Components can be tested with minimal external dependencies
3. **Composability** - Utilities can be combined for complex test scenarios
4. **Performance** - Minimal overhead compared to real implementations
5. **Cross-Crate** - Framework can be reused across all AdapterOS crates

## Quick Start

Add the framework to your crate's `Cargo.toml`:

```toml
[dev-dependencies]
adapteros-unit-testing = { path = "../../tests/unit" }
```

Then use in your tests:

```rust
use adapteros_unit_testing::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_deterministic_mocking() {
        let mock = DeterministicRng::from_seed(42);
        let value = mock.gen_range(0..100);
        assert_eq!(value, 42); // Always the same for seed 42
    }

    #[tokio::test]
    async fn test_async_component() {
        let timeout = Timeout::new(Duration::from_secs(5));
        let result = timeout.run(my_async_function()).await;
        assert!(result.is_ok());
    }
}
```

## Module Documentation

### Mocks Module

Provides deterministic test doubles for AdapterOS components.

```rust
use adapteros_unit_testing::mocks::*;

// Deterministic random number generation
let rng = DeterministicRng::from_seed(123);
let value = rng.gen_range(0..100); // Always returns same value

// Mock telemetry collector
let telemetry = MockTelemetryCollector::new(456);
telemetry.record_event("test_event", json!({"data": "value"}));

// Mock policy engine
let policy = MockPolicyEngine::new(789);
let allowed = policy.check_action("read", "file.txt");
```

### Isolation Module

Enables testing components with controlled dependencies.

```rust
use adapteros_unit_testing::isolation::*;

// Test sandbox for file system operations
let sandbox = TestSandbox::new();
sandbox.create_file("test.txt", 1024);

// Isolated component with mocked dependencies
let mut isolated = IsolatedComponent::new(my_component);
isolated.register_mock("database", mock_db);
```

### Property Module

Property-based testing for mathematical and logical properties.

```rust
use adapteros_unit_testing::property::*;

// Test hash determinism
let property = hash_deterministic_property();
let result = check_property(property, 1000);
assert!(result.is_passed());

// Custom property
struct MyProperty;
impl Property for MyProperty {
    fn test(&self, input: &[u8]) -> bool {
        // Test some property of the input
        true
    }
    fn name(&self) -> &str { "my_property" }
}
```

### Async Utils Module

Testing utilities for async components and futures.

```rust
use adapteros_unit_testing::async_utils::*;

// Timeout wrapper
let timeout = Timeout::new(Duration::from_secs(1));
let result = timeout.run(my_async_fn()).await;

// Deterministic async execution
let executor = DeterministicExecutor::new();
executor.spawn(async_task1);
executor.spawn(async_task2);
executor.run_all().await;

// Future inspection
let inspector = FutureInspector::new(my_future);
let poll_count = inspector.poll_count();
```

### Metal Module

GPU kernel testing and validation.

```rust
use adapteros_unit_testing::metal::*;

// Kernel compilation testing
let tester = MetalKernelTester::new();
let result = tester.test_kernel_compilation("fused_mlp")?;

// Performance benchmarking
let benchmarker = MetalPerformanceBenchmarker::new();
let results = benchmarker.benchmark_kernel("test_kernel", &[100, 1000, 10000]);

// Memory testing
let memory_tester = MetalMemoryTester::new(42);
let alloc_result = memory_tester.test_allocation("test_buffer", 1024, 16);
```

### Evidence Module

Validation of evidence-grounded responses.

```rust
use adapteros_unit_testing::evidence::*;

// Response validation
let validator = EvidenceValidator::new();
let result = validator.validate_response(&my_response);
assert!(result.is_valid);

// Evidence generation for testing
let mut generator = EvidenceGenerator::new(123);
let response = generator.generate_response(source_text, prompt);

// Quality assessment
let assessor = ResponseQualityAssessor::new();
let quality = assessor.assess_quality(&response);
```

## Advanced Usage

### Combining Utilities

```rust
use adapteros_unit_testing::*;

#[tokio::test]
async fn complex_test_scenario() {
    // Setup isolated environment
    let sandbox = TestSandbox::with_seed(42);
    let mut isolated = IsolatedComponent::with_seed(my_component, 42);

    // Register mocks
    isolated.register_mock("telemetry", MockTelemetryCollector::new(42));
    isolated.register_mock("policy", MockPolicyEngine::new(42));

    // Create async test harness
    let mut harness = AsyncTestHarness::new();
    harness.add_setup(setup_fn);
    harness.add_teardown(teardown_fn);

    // Run test with timeout
    let timeout = Timeout::new(Duration::from_secs(10));
    let result = timeout.run(harness.run_test(|h| {
        // Test implementation using isolated component
        let component = h.get_service::<MyComponent>("component").unwrap();
        component.do_something()
    })).await;

    assert!(result.is_ok());
}
```

### Property-Based Testing with Custom Generators

```rust
use adapteros_unit_testing::property::*;

struct CustomGenerator;
impl Generator for CustomGenerator {
    fn generate(&mut self, size: usize, seed: &B3Hash) -> Vec<u8> {
        // Custom generation logic
        vec![0; size]
    }
}

#[test]
fn test_custom_property() {
    let property = MyCustomProperty;
    let mut generator = CustomGenerator;
    let config = PropertyConfig {
        max_tests: 500,
        max_input_size: 1024,
        seed: B3Hash::hash(b"custom_test"),
    };

    let result = check_property_with_config(&property, &mut generator, &config);
    assert!(result.is_passed());
}
```

### Metal Kernel Integration Testing

```rust
use adapteros_unit_testing::metal::*;

#[test]
fn test_kernel_integration() {
    let tester = MetalKernelTester::new();
    let executor = MetalExecutionTester::new(42);

    // Test compilation
    let kernels = vec!["fused_mlp", "flash_attention", "vocabulary_projection"];
    for kernel in kernels {
        let result = tester.test_kernel_compilation(kernel);
        assert!(result.is_ok(), "Kernel {} failed to compile", kernel);
    }

    // Test execution simulation
    let input_data = vec![1.0f32; 768];
    let exec_result = executor.simulate_execution("fused_mlp", &input_data);
    assert!(exec_result.success);
    assert!(exec_result.execution_time > Duration::ZERO);
}
```

## Contributing

When adding new testing utilities:

1. Follow the established patterns for determinism and isolation
2. Include comprehensive documentation and examples
3. Add unit tests for the utilities themselves
4. Ensure cross-platform compatibility where possible
5. Update this README with new features

## Performance Considerations

- Mock objects are designed to be lightweight and fast
- Property-based testing can be resource-intensive; use appropriate test sizes
- Async utilities include timeouts to prevent hanging tests
- Metal testing simulates GPU operations for CI/CD compatibility

## Compatibility

- **Rust**: 1.70+
- **macOS**: 12.0+ (for Metal kernel testing)
- **Linux/Windows**: Full compatibility (Metal features disabled)

## License

This framework is part of AdapterOS and follows the same licensing terms.</code>
</edit_file>