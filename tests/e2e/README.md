# AdapterOS End-to-End Testing Framework

This directory contains comprehensive end-to-end tests for AdapterOS, validating complete inference pipelines, adapter lifecycle management, telemetry validation, failure scenario handling, and determinism workflow guarantees.

## Overview

The e2e testing framework provides:

- **Complete Pipeline Validation**: Tests from model import through inference results
- **Adapter Lifecycle Management**: Creation, loading, activation, hot-swapping, and cleanup
- **Telemetry Validation**: Canonical JSON, BLAKE3 hashing, bundle rotation, and signing
- **Failure Scenario Handling**: Graceful degradation, recovery mechanisms, and resilience
- **Determinism Workflow**: CPID consistency, evidence ordering, and temporal guarantees

## Architecture

```
tests/e2e/
├── mod.rs                 # Module exports and organization
├── orchestration.rs       # Test orchestration utilities
├── inference_pipeline.rs  # Complete inference pipeline tests
├── adapter_lifecycle.rs   # Adapter management lifecycle tests
├── telemetry_validation.rs # Telemetry pipeline validation
├── failure_scenarios.rs   # Failure handling and recovery tests
└── determinism_workflow.rs # Determinism validation tests
```

## Test Orchestration

The framework uses a centralized orchestration system (`TestOrchestrator`) that:

- Manages isolated test environments
- Handles test lifecycle (setup, execution, cleanup)
- Provides telemetry collection and validation
- Supports concurrent test execution
- Tracks test results and generates reports

### Test Environment

Each test runs in an isolated environment with:

- Dedicated telemetry writers and bundle stores
- Policy engine and API handler instances
- Configurable timeouts and resource limits
- Automatic cleanup of test artifacts

## Test Categories

### 1. Inference Pipeline Tests

**File**: `inference_pipeline.rs`

Validates complete workflows from model import to inference results:

- Model import and validation
- Adapter registration and loading
- Evidence database setup
- Inference execution with policy checks
- Results validation and compliance
- Determinism verification across runs

**Key Scenarios**:
- Complete pipeline execution
- Model loading performance
- Adapter hot-swapping during inference
- Evidence retrieval accuracy

### 2. Adapter Lifecycle Tests

**File**: `adapter_lifecycle.rs`

Tests complete adapter management workflows:

- Adapter creation from repository analysis
- Loading and validation
- Activation and routing
- Usage tracking and performance monitoring
- Hot-swapping and memory management
- Cleanup and resource release

**Key Scenarios**:
- Adapter creation and registration
- Concurrent load testing
- Degradation and recovery
- Version migration and compatibility

### 3. Telemetry Validation Tests

**File**: `telemetry_validation.rs`

Validates telemetry collection and integrity:

- Event collection from multiple sources
- Canonical JSON formatting (JCS RFC 8785)
- BLAKE3 hashing determinism
- Bundle rotation and size management
- Merkle tree construction
- Bundle signing with Ed25519
- Audit trail validation and replay

**Key Scenarios**:
- Telemetry sampling strategies
- Performance under load
- Compression and storage efficiency

### 4. Failure Scenario Tests

**File**: `failure_scenarios.rs`

Tests system resilience and recovery:

- Adapter load failures and fallbacks
- Memory exhaustion handling
- Evidence retrieval failures
- Policy enforcement violations
- Network isolation breaches
- Determinism verification failures
- Graceful degradation strategies
- Recovery mechanism validation

**Key Scenarios**:
- Cascading failure handling
- Failure prediction and prevention

### 5. Determinism Workflow Tests

**File**: `determinism_workflow.rs`

Validates determinism guarantees across the entire system:

- CPID consistency across components
- Evidence ordering determinism
- Adapter routing determinism
- Kernel execution determinism
- Response generation determinism
- Telemetry determinism
- Temporal consistency
- Cross-run verification

**Key Scenarios**:
- Concurrent determinism validation
- CPID isolation testing

## Running Tests

### Individual Test Execution

```bash
# Run all e2e tests
cargo test --test e2e

# Run specific test category
cargo test --test e2e inference_pipeline

# Run with verbose output
RUST_LOG=debug cargo test --test e2e
```

### Boot Entrypoint for E2E
- Canonical boot path: `./start` (delegates to `scripts/service-manager.sh`, includes drift checks and health waits)
- Legacy scripts (`scripts/run_complete_system.sh`, bootstrap variants) are deprecated and guarded by opt-in prompts (default No); avoid in new tests.
- Dev boot smoke coverage lives at `tests/dev_boot.rs` and exercises `./start` help/status.

### Test Configuration

Tests can be configured via environment variables:

```bash
# Test timeout (default: 300s)
ADAPTEROS_E2E_TIMEOUT=600

# Telemetry directory
ADAPTEROS_E2E_TELEMETRY_DIR=var/tmp/adapteros_e2e_telemetry

# Verbose logging
ADAPTEROS_E2E_VERBOSE=1

# CPID for deterministic tests
ADAPTEROS_E2E_CPID=e2e_test_cpid_001
```

### Orchestrated Test Runs

```rust
use adapteros_e2e::{TestOrchestrator, TestConfig};

#[tokio::test]
async fn test_complete_pipeline() {
    let config = TestConfig::default();
    let mut orchestrator = TestOrchestrator::new(config);

    let result = orchestrator.run_test("complete_pipeline", |env| async move {
        let test = InferencePipelineTest::new(env);
        test.test_complete_pipeline().await
    }).await?;

    assert!(result.status == TestStatus::Passed);
}
```

## Test Data and Fixtures

Tests use synthetic data and fixtures to avoid external dependencies:

- **Model Fixtures**: Mock model configurations and artifacts
- **Adapter Fixtures**: Simulated adapter metadata and weights
- **Evidence Fixtures**: Synthetic document collections with metadata
- **Policy Fixtures**: Test policy configurations and rules

## Performance Benchmarks

The framework includes performance validation:

- **Latency Targets**: <500ms p95 for inference pipelines
- **Throughput**: >40 tokens/sec generation rate
- **Memory Usage**: <4GB for typical workloads
- **Telemetry Overhead**: <8% of total inference time

## Determinism Validation

Tests validate multiple determinism aspects:

- **CPID Consistency**: Same CPID produces identical results
- **Temporal Ordering**: Events maintain chronological order
- **Hash Stability**: BLAKE3 hashes are deterministic
- **Cross-Run Verification**: Multiple executions produce identical outputs

## Failure Injection

Tests include controlled failure injection:

- **Adapter Corruption**: Simulate corrupted adapter files
- **Memory Pressure**: Artificial memory exhaustion
- **Network Failures**: Simulate connectivity issues
- **Evidence Unavailability**: Database connection failures

## Monitoring and Observability

All tests generate comprehensive telemetry:

- **Test Execution Traces**: Complete execution paths
- **Performance Metrics**: Latency, throughput, resource usage
- **Failure Analysis**: Root cause identification
- **Determinism Reports**: Validation of reproducibility

## Integration with CI/CD

The e2e tests integrate with CI/CD pipelines:

```yaml
# .github/workflows/e2e.yml
name: E2E Tests
on: [push, pull_request]

jobs:
  e2e:
    runs-on: macos-latest  # Metal GPU required
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - name: Run E2E Tests
        run: cargo test --test e2e --verbose
      - name: Upload Test Artifacts
        uses: actions/upload-artifact@v3
        with:
          name: e2e-results
          path: var/tmp/adapteros_e2e/
```

## Troubleshooting

### Common Issues

1. **Timeout Errors**: Increase `ADAPTEROS_E2E_TIMEOUT`
2. **Memory Issues**: Reduce concurrent test execution
3. **GPU Unavailable**: Tests fall back to CPU simulation
4. **Telemetry Conflicts**: Use unique test directories

### Debug Mode

Enable detailed logging:

```bash
RUST_LOG=adapteros_e2e=debug cargo test --test e2e -- --nocapture
```

### Test Isolation

Each test runs in isolation with unique identifiers to prevent conflicts.

## Contributing

When adding new e2e tests:

1. Follow the established patterns in existing test files
2. Include comprehensive telemetry logging
3. Add performance assertions where appropriate
4. Document test scenarios and expected behaviors
5. Ensure tests clean up after themselves

## Security Considerations

- Tests run with restricted permissions
- No external network access during execution
- Sensitive data uses synthetic fixtures
- Telemetry bundles are encrypted and signed
- Test environments are isolated from production systems

MLNavigator Inc 2025-12-06.
