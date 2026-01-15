# Integration Tests Documentation

This document provides an overview of the comprehensive integration test suite for adapterOS.

## Overview

The integration test suite validates end-to-end functionality of key adapterOS subsystems:

1. **Kernel Workflow Integration** - Real Metal kernel-based workflow execution
2. **Streaming Inference** - Server-Sent Events (SSE) streaming with chat-style responses
3. **Policy Evidence Compliance** - Evidence policy enforcement and tracking

---

## Blocking vs Ignored Suites

Blocking suites are the stability gate. For integration tests, "blocking" means any
test that is not marked `#[ignore]` (or `cfg_attr(..., ignore = "...")`) and is run
by default in `bash scripts/test/all.sh all` / `bash scripts/ci/stability.sh`.

| Suite | Status | Command |
| --- | --- | --- |
| Rust integration tests (non-ignored) | Blocking | `cargo test --workspace --exclude adapteros-lora-mlx-ffi --tests` |
| Rust ignored tests | Non-blocking | `cargo test --workspace --features extended-tests --lib --bins --examples -- --ignored` + `cargo test --workspace --features extended-tests --tests -- --ignored` |
| Hardware-dependent tests (Metal/VRAM/residency) | Non-blocking | See `docs/stability/CHECKLIST.md` (Hardware Tests) |

**Tracking requirement:** any test that remains ignored must include a tracking tag
in the ignore reason, e.g. `[tracking: STAB-IGN-0001]`, and the ID must have a
matching entry in `docs/stability/IGNORED_TESTS.md`.

The ignored-test sweep runs with `extended-tests` enabled by default; use
`IGNORED_FEATURES` or `IGNORED_EXCLUDE` to adjust coverage for your environment.

Hardware tests currently cover:
- `tests/lora_buffer_population_integration.rs`
- `tests/kv_residency_quota_integration.rs`
- `crates/adapteros-lora-worker/tests/worker_enforcement_tests.rs`
- `crates/adapteros-lora-worker/tests/residency_probe.rs`
- `crates/adapteros-lora-kernel-coreml/tests/integration_tests.rs`
- `crates/adapteros-memory/tests/metal_heap_tests.rs`
- `crates/adapteros-memory/src/heap_observer.rs` (ignored lib test)

---

## Test Files

### 1. kernel_workflow_integration.rs

Tests real Metal/MLX kernel execution with workflow orchestration.

**Location:** `tests/kernel_workflow_integration.rs`

**Test Coverage:**

| Test Name | Description | Type |
|-----------|-------------|------|
| `test_kernel_backend_sequential_workflow` | Validates sequential adapter execution | Integration |
| `test_kernel_backend_parallel_workflow` | Validates parallel adapter execution | Integration |
| `test_kernel_backend_upstream_downstream_workflow` | Validates two-phase upstream→downstream pipeline | Integration |
| `test_worker_execute_workflow_integration` | Tests Worker integration with workflows | Integration |
| `test_kernel_backend_error_handling_invalid_adapter` | Error handling for nonexistent adapters | Error Case |
| `test_kernel_backend_empty_input_tokens` | Edge case: empty input handling | Edge Case |
| `test_kernel_backend_performance_characteristics` | Performance comparison of workflow types | Performance |
| `test_kernel_backend_concurrent_workflows` | Multiple concurrent workflow execution | Concurrency |
| `test_kernel_backend_large_token_sequence` | Stress test with 1024 token sequence | Stress Test |

**Key Features Tested:**

- ✅ Real Metal kernel integration via `KernelAdapterBackend`
- ✅ Arc<Mutex<K>> kernel sharing pattern
- ✅ All three workflow types (Sequential, Parallel, UpstreamDownstream)
- ✅ Worker::execute_workflow() integration
- ✅ Concurrent workflow execution
- ✅ Error handling and edge cases
- ✅ Performance characteristics

**Example Usage:**

```bash
# Run with Metal runtime available
cargo test --test kernel_workflow_integration -- --ignored

# Run specific test
cargo test --test kernel_workflow_integration test_kernel_backend_sequential_workflow -- --ignored
```

**Note:** Most tests are marked `#[ignore]` because they require Metal runtime and adapter files.

---

### 2. streaming_integration.rs

Tests Server-Sent Events (SSE) streaming inference with chat-style responses.

**Location:** `tests/streaming_integration.rs`

**Test Coverage:**

| Test Name | Description | Type |
|-----------|-------------|------|
| `test_streaming_chat_completion_basic` | Basic streaming chat completion | Integration |
| `test_streaming_chunk_format` | StreamChunk serialization format | Format |
| `test_streaming_done_message` | [DONE] message format | Format |
| `test_streaming_error_handling` | Error handling for invalid requests | Error Case |
| `test_streaming_multiple_messages` | Multi-turn conversation streaming | Integration |
| `test_streaming_temperature_variation` | Temperature parameter effects | Parameter |
| `test_streaming_max_tokens_limit` | Max tokens limit enforcement | Limit |
| `test_streaming_concurrent_requests` | Multiple concurrent streams | Concurrency |
| `test_stream_chunk_serialization_roundtrip` | Serialization/deserialization | Serialization |
| `test_stream_delta_partial_updates` | Partial StreamDelta updates | Format |
| `test_streaming_chunk_shape_compatibility` | Streaming chunk shape compliance | Compliance |

**Key Features Tested:**

- ✅ SSE format: `data: {json}\n\n`
- ✅ StreamChunk, StreamChoice, StreamDelta structures
- ✅ [DONE] message termination
- ✅ Temperature and max_tokens parameters
- ✅ Multi-message conversations
- ✅ Concurrent streaming requests
- ✅ Error handling for invalid inputs
- ✅ Serialization roundtrip correctness

**Example Usage:**

```bash
# Run with Metal runtime
cargo test --test streaming_integration -- --ignored

# Run format tests (no runtime required)
cargo test --test streaming_integration test_streaming_chunk_format
cargo test --test streaming_integration test_stream_delta_partial_updates
```

---

### 3. policy_evidence_integration.rs

Tests Evidence Policy Pack enforcement for regulatory compliance.

**Location:** `tests/policy_evidence_integration.rs`

**Test Coverage:**

| Test Name | Description | Type |
|-----------|-------------|------|
| `test_evidence_policy_default_configuration` | Default policy configuration | Config |
| `test_evidence_policy_enforcement_basic` | Basic policy enforcement | Integration |
| `test_evidence_spans_minimum_requirement` | Minimum evidence span count | Validation |
| `test_evidence_quality_thresholds` | Quality score thresholds | Validation |
| `test_evidence_type_restrictions` | Evidence type allowlisting | Validation |
| `test_source_signature_requirements` | Source signature enforcement | Security |
| `test_source_domain_restrictions` | Domain allow/block lists | Security |
| `test_superseded_evidence_warnings` | Superseded revision detection | Validation |
| `test_open_book_grounding_requirement` | Open-book retrieval enforcement | Compliance |
| `test_evidence_quality_score_calculation` | Quality score computation | Metrics |
| `test_comprehensive_policy_validation` | End-to-end policy validation | Integration |
| `test_multiple_evidence_types` | Mixed evidence type handling | Integration |
| `test_edge_case_empty_spans` | Edge case: empty evidence | Edge Case |
| `test_edge_case_boundary_quality_scores` | Boundary threshold testing | Edge Case |

**Key Features Tested:**

- ✅ EvidencePolicy enforcement
- ✅ Minimum evidence span requirements
- ✅ Quality thresholds (relevance, confidence)
- ✅ Evidence type restrictions
- ✅ Source signature requirements
- ✅ Domain allow/block lists
- ✅ Superseded revision warnings
- ✅ Open-book grounding requirements
- ✅ Quality score calculation
- ✅ Comprehensive policy validation

**Evidence Types Supported:**

```rust
pub enum EvidenceType {
    CodeDoc,
    ApiDoc,
    TestCase,
    Config,
    ErrorLog,
    Performance,
    SecurityAudit,
    Compliance,
}
```

**Quality Thresholds:**

```rust
pub struct QualityThresholds {
    pub min_relevance: f32,     // Default: 0.7
    pub min_confidence: f32,    // Default: 0.8
    pub min_recency_days: u32,  // Default: 0
    pub max_age_days: u32,      // Default: 365
}
```

**Example Usage:**

```bash
# Run all policy evidence tests
cargo test --test policy_evidence_integration

# Run specific validation tests
cargo test --test policy_evidence_integration test_evidence_quality_thresholds
cargo test --test policy_evidence_integration test_source_signature_requirements
```

---

## Running Integration Tests

### Run All Integration Tests

```bash
# Run all integration tests (excluding ignored tests)
cargo test --tests

# Run all integration tests including ignored tests
cargo test --tests -- --ignored

# Run ignored Rust tests (unit + integration) across the workspace
cargo test --workspace --features extended-tests --lib --bins --examples -- --ignored
cargo test --workspace --features extended-tests --tests -- --ignored

# Run hardware-dependent test suites (Metal/VRAM/residency)
cargo test --test lora_buffer_population_integration --features extended-tests --profile release -- --ignored --nocapture
cargo test --test kv_residency_quota_integration --features hardware-residency
cargo test -p adapteros-lora-worker --features hardware-residency,ci-residency --test worker_enforcement_tests
cargo test -p adapteros-lora-worker --features hardware-residency,ci-residency --test residency_probe
cargo test -p adapteros-lora-kernel-coreml --test integration_tests -- --ignored
cargo test -p adapteros-memory --test metal_heap_tests --profile release -- --ignored
cargo test -p adapteros-memory --lib --profile release -- --ignored

# Run all tests (unit + integration)
cargo test --all
```

### Run Specific Test File

```bash
# Kernel workflow tests
cargo test --test kernel_workflow_integration

# Streaming tests
cargo test --test streaming_integration

# Policy evidence tests
cargo test --test policy_evidence_integration
```

### Run Specific Test

```bash
cargo test --test kernel_workflow_integration test_kernel_backend_sequential_workflow -- --ignored
```

### Run Tests with Output

```bash
cargo test --test streaming_integration -- --nocapture --show-output
```

---

## Test Requirements

### Metal Kernel Tests

**Requirements:**
- macOS with Metal support
- Metal kernels initialized
- Adapter files present in test locations

**Marked with:** `#[ignore]` and comment `// Requires Metal runtime`

### Streaming Tests

**Requirements:**
- Metal kernels initialized (for integration tests)
- Worker with loaded adapters

**Format tests:** No special requirements

### Policy Evidence Tests

**Requirements:**
- None - pure unit/integration tests

**Run anytime:** No special dependencies

---

## Test Statistics

| Category | Test Count | Runtime Required | Coverage |
|----------|------------|------------------|----------|
| Kernel Workflow | 9 | Metal | High |
| Streaming Inference | 11 | Metal (some) | High |
| Policy Evidence | 14 | None | High |
| **Total** | **34** | - | **Comprehensive** |

---

## Coverage Areas

### ✅ Completed

- [x] Real Metal kernel workflow execution
- [x] Sequential, Parallel, UpstreamDownstream workflows
- [x] Worker::execute_workflow() integration
- [x] Arc<Mutex<K>> kernel sharing
- [x] SSE streaming format
- [x] StreamChunk serialization
- [x] Evidence policy enforcement
- [x] Quality threshold validation
- [x] Source signature verification
- [x] Domain restrictions
- [x] Concurrent execution
- [x] Error handling
- [x] Edge cases

### ⏸️ Future Enhancements

- [ ] Autoregressive generation workflows
- [ ] Batch workflow execution
- [ ] Workflow result streaming
- [ ] RAG integration tests
- [ ] Training pipeline integration tests
- [ ] Performance benchmarking
- [ ] Load testing
- [ ] Chaos engineering tests

---

## Maintenance

### Adding New Tests

1. Create test file in `tests/` directory
2. Use appropriate `#[test]` or `#[tokio::test]` attributes
3. Mark hardware-dependent tests with `#[ignore]`
4. Document in this file
5. Update test statistics

### Test Naming Convention

```rust
// Format: test_<component>_<scenario>_<expected_result>
#[tokio::test]
async fn test_kernel_backend_sequential_workflow() { ... }

// Format: test_<component>_<error_scenario>
#[tokio::test]
#[ignore]
async fn test_kernel_backend_error_handling_invalid_adapter() { ... }
```

### Helper Functions

Place test helper functions at the bottom of each test file:

```rust
// Helper functions
fn create_test_manifest(adapter_count: usize) -> ManifestV3 { ... }
fn create_valid_evidence_span(doc_id: &str, rev: u32) -> EvidenceSpan { ... }
```

---

## References

- **Kernel Backend Usage:** `crates/adapteros-lora-lifecycle/KERNEL_BACKEND_USAGE.md`
- **Workflow Executor:** `crates/adapteros-lora-lifecycle/src/workflow_executor.rs`
- **Streaming API:** `crates/adapteros-api/src/streaming.rs`
- **Evidence Policy:** `crates/adapteros-policy/src/packs/evidence.rs`
- **Worker Integration:** `crates/adapteros-lora-worker/src/lib.rs`

---

**Last Updated:** 2025-01-15
**Test Suite Version:** 1.0
**Total Tests:** 34 comprehensive integration tests
