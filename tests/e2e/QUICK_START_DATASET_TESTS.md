# Dataset-to-Inference Tests - Quick Start Guide

## Files Location
```
/Users/star/Dev/aos/tests/e2e/dataset_to_inference.rs       [Main test file - 574 lines]
/Users/star/Dev/aos/tests/e2e/DATASET_TO_INFERENCE_TESTS.md  [Detailed documentation]
/Users/star/Dev/aos/DATASET_TO_INFERENCE_TEST_SUMMARY.md     [Implementation summary]
```

## Run Tests

### Quick Run (All Tests)
```bash
cd /Users/star/Dev/aos
cargo test --test dataset_to_inference -- --ignored --nocapture
```

### Run Specific Test Suite

**Complete Workflow (7 phases)**
```bash
cargo test test_dataset_to_inference_complete_workflow -- --ignored --nocapture
```

**Error Scenarios (5 error cases)**
```bash
cargo test test_dataset_error_scenarios -- --ignored --nocapture
```

### With Logging
```bash
RUST_LOG=debug cargo test --test dataset_to_inference -- --ignored --nocapture
```

## Test Structure Overview

### Test Class: DatasetToInferenceTest

**Two main test suites:**

1. **Complete Workflow** - 7 sequential phases (happy path)
2. **Error Scenarios** - 5 error conditions

## Phase Breakdown

| # | Phase | Function | Duration | Purpose |
|---|-------|----------|----------|---------|
| 1 | Upload | `test_upload_dataset()` | <1s | Create 5 test files |
| 2 | Validate | `test_validate_dataset()` | <1s | 5 validation checks |
| 3 | Training | `test_start_training()` | <1s | Create training job |
| 4 | Monitor | `test_wait_for_training()` | 1-2s | Simulate training |
| 5 | Verify | `test_verify_adapter_creation()` | <1s | Confirm adapter |
| 6 | Infer | `test_run_inference()` | <1s | 3 inference calls |
| 7 | Cleanup | `test_cleanup()` | <1s | Remove artifacts |

## Error Scenarios

| # | Test | Error Type | Expected |
|---|------|-----------|----------|
| 1 | Invalid Files | `invalid_file_format` | Rejected |
| 2 | Size Exceeded | `size_limit_exceeded` | Rejected |
| 3 | Dataset Missing | `dataset_not_found` | Rejected |
| 4 | Bad Format | `format_invalid` | Rejected |
| 5 | Corruption | `hash_mismatch` | Detected |

## Key Features

✓ 16 async functions
✓ 2 test suites
✓ 18 telemetry events
✓ 5 error scenarios
✓ 5-minute timeout
✓ Thread-safe ID generation
✓ Production-compatible logging
✓ Resource cleanup
✓ Comprehensive documentation

## Expected Output

```
📤 Phase 1: Upload Dataset Files
  Created dataset: dataset_0
  Files uploaded: 5

✓ Phase 2: Validate Dataset
  Dataset validation successful

🏋️  Phase 3: Start Training Job
  Training job created: job_0
  Dataset: dataset_0

⏳ Phase 4: Wait for Training Completion
  Training completed in X seconds
  Final loss: 0.XXXX

🔍 Phase 5: Verify Adapter Creation
  Adapter created: dataset-trained-adapter_1
  Status: registered

🚀 Phase 6: Run Inference
  Inference 1: 150 ms latency
  Inference 2: 175 ms latency
  Inference 3: 200 ms latency

🧹 Phase 7: Cleanup
  Cleanup completed

🎉 Complete dataset-to-inference workflow test passed!
```

## Telemetry Events (18 total)

### Dataset Events (5)
- `dataset_upload` - Upload initiated
- `dataset_created` - Created
- `dataset_validation_start` - Validation started
- `dataset_validation_check` - Check result
- `dataset_validation_complete` - Validation done

### Training Events (3)
- `training_job_created` - Job created
- `training_progress` - Progress per epoch
- `training_job_completed` - Job done

### Adapter Events (2)
- `adapter_created` - Adapter ready
- `adapter_registered` - Registered

### Inference Events (1)
- `inference_executed` - Inference done

### Error Events (5)
- `dataset_upload_failed` - Upload failed
- `dataset_size_exceeded` - Size limit hit
- `training_failed` - Training failed
- `validation_failed` - Validation failed
- `file_corruption_detected` - Corruption found

### Cleanup Events (2)
- `cleanup_started` - Cleanup began
- `cleanup_completed` - Cleanup done

## Debug Tips

### View Telemetry
```bash
ls -la var/tmp/adapteros_e2e/telemetry/
cat var/tmp/adapteros_e2e/telemetry/*.ndjson | jq .
```

### Enable Backtraces
```bash
RUST_BACKTRACE=1 cargo test --test dataset_to_inference -- --ignored --nocapture
```

### Check Artifacts
```bash
find var/tmp/adapteros_e2e -type f
```

### Verbose Logging
```bash
RUST_LOG=adapteros=debug cargo test --test dataset_to_inference -- --ignored --nocapture
```

## Test Duration

- **Complete Workflow**: 5-10 seconds
- **Error Scenarios**: 3-5 seconds
- **Total Suite**: 15-20 seconds

## Integration

- Module: `tests/e2e::DatasetToInferenceTest`
- Framework: Uses shared `TestEnvironment`
- Telemetry: Canonical JSON format
- Features: Requires `extended-tests` feature flag

## Files Summary

```
tests/e2e/dataset_to_inference.rs
├── DatasetToInferenceTest struct (1)
├── Impl block with methods (11)
│   ├── Constructor
│   ├── test_complete_workflow (orchestrator)
│   ├── test_error_scenarios (orchestrator)
│   ├── 7 workflow phase methods
│   └── 5 error scenario methods
└── Test module with 2 test cases
    ├── test_dataset_to_inference_complete_workflow
    └── test_dataset_error_scenarios
```

## Usage Pattern

```rust
// Create test environment
let config = TestConfig::default();
let env = TestEnvironment::new(config).await?;
let env = Arc::new(Mutex::new(env));

// Create test instance
let test = DatasetToInferenceTest::new(env);

// Run workflow
test.test_complete_workflow().await?;

// Or run errors
test.test_error_scenarios().await?;
```

## Key Design Patterns

1. **Atomic IDs**: Thread-safe unique ID generation
2. **Telemetry-First**: All operations logged as JSON
3. **Timeout Protected**: 5-minute maximum duration
4. **Phase Logging**: Clear progress indicators
5. **Error Simulation**: Errors logged, not thrown

## Validation Checklist

- [x] 574 lines of production-quality code
- [x] 16 async functions
- [x] 2 test suites (7 phases + 5 errors)
- [x] 18 telemetry events
- [x] 5 error types
- [x] Module registered in `mod.rs`
- [x] Comprehensive documentation
- [x] Thread-safe implementation
- [x] Timeout protection
- [x] Resource cleanup

## Next Steps

1. Run the test suite: `cargo test --test dataset_to_inference -- --ignored --nocapture`
2. Review test output for all phases
3. Check telemetry logs in `var/tmp/adapteros_e2e/telemetry/`
4. Read detailed docs: `DATASET_TO_INFERENCE_TESTS.md`
5. Explore implementation: `tests/e2e/dataset_to_inference.rs`

---

**For detailed documentation, see**: `/Users/star/Dev/aos/tests/e2e/DATASET_TO_INFERENCE_TESTS.md`
