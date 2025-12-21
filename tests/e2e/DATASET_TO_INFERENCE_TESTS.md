# Dataset-to-Inference End-to-End Test Suite

## Overview

The `dataset_to_inference.rs` test suite provides comprehensive end-to-end testing for the complete dataset-to-inference workflow in AdapterOS. It validates the full pipeline from dataset creation through inference execution, including robust error handling for edge cases and failure scenarios.

## Test Structure

### Main Test Module: `DatasetToInferenceTest`

Located in: `/Users/star/Dev/aos/tests/e2e/dataset_to_inference.rs`

This test class orchestrates the complete workflow and error scenario testing.

## Complete Workflow Test: `test_complete_workflow`

Tests the happy path of the entire dataset-to-inference pipeline in 7 sequential phases:

### Phase 1: Upload Dataset Files
- **Function**: `test_upload_dataset()`
- **Tests**: Creating multiple dataset files with different formats
- **File Types Created**:
  - Python source files (`.py`) - 3 files with code samples
  - JSON patch files (`.jsonl`) - 2 files with input/output pairs
- **Verification**:
  - Files are created successfully
  - Unique dataset ID is generated
  - Upload event is logged to telemetry
  - File count and total size are captured

**Expected Output**:
```
📤 Phase 1: Upload Dataset Files
  Created dataset: dataset_0
  Files uploaded: 5
```

### Phase 2: Dataset Validation
- **Function**: `test_validate_dataset()`
- **Tests**: Validating dataset integrity and format compliance
- **Validation Checks**:
  1. File integrity (checksum verification)
  2. Format compliance (file format validation)
  3. Encoding check (UTF-8 verification)
  4. Size sanity (within limits)
  5. Duplicate detection (no duplicate files)
- **Verification**: All checks pass and completion is logged

**Expected Output**:
```
✓ Phase 2: Validate Dataset
  Dataset validation successful
```

### Phase 3: Training Job Startup
- **Function**: `test_start_training()`
- **Tests**: Initiating a training job with the dataset
- **Training Configuration**:
  - Rank: 16
  - Alpha: 32
  - Epochs: 2
  - Batch Size: 8
  - Learning Rate: 0.001
- **Verification**:
  - Training job ID is generated
  - Job is linked to dataset
  - Job configuration is logged

**Expected Output**:
```
🏋️  Phase 3: Start Training Job
  Training job created: job_0
  Dataset: dataset_0
```

### Phase 4: Training Completion Monitoring
- **Function**: `test_wait_for_training()`
- **Tests**: Monitoring training progress across multiple epochs
- **Features**:
  - Simulates training progress over 2 epochs
  - Tracks loss decay (95% per step)
  - Monitors tokens per second (250.5 tps)
  - Enforces 5-minute timeout
  - Logs progress events at each step
- **Verification**:
  - Training completes within timeout
  - Loss decreases over time
  - Final loss is logged

**Expected Output**:
```
⏳ Phase 4: Wait for Training Completion
  Training completed in X seconds
  Final loss: 0.XXXX
```

### Phase 5: Adapter Creation Verification
- **Function**: `test_verify_adapter_creation()`
- **Tests**: Confirming the trained adapter was successfully created
- **Adapter Metadata**:
  - Size: 256 MB
  - Rank: 16
  - Alpha: 32
  - Format: BLAKE3 hash verification
- **Verification**:
  - Adapter creation event is logged
  - Adapter is registered in the system
  - Status transitions are recorded

**Expected Output**:
```
🔍 Phase 5: Verify Adapter Creation
  Adapter created: dataset-trained-adapter_1
  Status: registered
```

### Phase 6: Inference Execution
- **Function**: `test_run_inference()`
- **Tests**: Running inference with the trained adapter
- **Test Prompts**:
  1. "def quicksort" (code completion)
  2. "class DatabaseConnection" (code generation)
  3. "async def fetch_data" (async function generation)
- **Metrics Collected**:
  - Request/response tokens
  - Latency (ms)
  - Success status
- **Verification**: 3 inference requests complete successfully

**Expected Output**:
```
🚀 Phase 6: Run Inference
  Inference 1: 150 ms latency
  Inference 2: 175 ms latency
  Inference 3: 200 ms latency
```

### Phase 7: Cleanup
- **Function**: `test_cleanup()`
- **Tests**: Proper resource cleanup after testing
- **Cleanup Operations**:
  - Remove dataset files
  - Clean temporary artifacts
  - Remove cache entries
- **Verification**: Cleanup events are logged

**Expected Output**:
```
🧹 Phase 7: Cleanup
  Cleanup completed
```

## Error Scenario Tests: `test_error_scenarios`

Tests 5 critical error conditions with proper error handling:

### Error Test 1: Invalid Files Upload
- **Function**: `test_upload_invalid_files()`
- **Tests**: Attempting to upload binary/corrupted files
- **Invalid Files**:
  - Binary file with null bytes
  - File with invalid UTF-8 encoding
- **Expected Behavior**: Upload rejected with proper error event
- **Verification**: Error is logged as `invalid_file_format`

### Error Test 2: Size Limit Exceeded
- **Function**: `test_exceed_size_limits()`
- **Tests**: Attempting to upload oversized dataset
- **Configuration**:
  - File Size: 5 MB (test size)
  - Limit: 1 GB
- **Expected Behavior**: Upload rejected with size limit error
- **Verification**: Error is logged as `size_limit_exceeded`

### Error Test 3: Non-Existent Dataset Training
- **Function**: `test_train_nonexistent_dataset()`
- **Tests**: Starting training with non-existent dataset ID
- **Expected Behavior**: Training request rejected
- **Verification**: Error is logged as `dataset_not_found`

### Error Test 4: Invalid Dataset Format
- **Function**: `test_invalid_dataset_format()`
- **Tests**: Dataset with malformed JSON
- **Invalid Data**:
  - Incomplete JSON objects
  - Non-JSON content
- **Expected Behavior**: Validation fails
- **Verification**: Error is logged as `format_invalid`

### Error Test 5: File Corruption Detection
- **Function**: `test_corrupted_dataset_files()`
- **Tests**: Detecting corrupted files via hash mismatch
- **Simulation**: File with mismatched BLAKE3 hash
- **Expected Behavior**: Corruption detected and logged
- **Verification**: Error is logged as `hash_mismatch`

## Running the Tests

### Run Complete Workflow Test
```bash
cargo test --test dataset_to_inference -- --ignored --nocapture
```

Or with feature flag:
```bash
cargo test --test dataset_to_inference --features extended-tests -- --ignored --nocapture
```

### Run Error Scenario Tests
```bash
cargo test --test dataset_to_inference test_dataset_error_scenarios -- --ignored --nocapture
```

### Run Both Test Suites
```bash
cargo test --test dataset_to_inference -- --ignored --nocapture
```

## Telemetry Events Logged

The test suite logs comprehensive telemetry events for audit and debugging:

### Dataset Lifecycle Events
- `dataset_upload` - Dataset upload initiated
- `dataset_created` - Dataset successfully created
- `dataset_validation_start` - Validation process started
- `dataset_validation_check` - Individual validation check result
- `dataset_validation_complete` - Validation completed with overall status

### Training Lifecycle Events
- `training_job_created` - Training job created with configuration
- `training_progress` - Training progress at each epoch
- `training_job_completed` - Training job completed with metrics

### Adapter Events
- `adapter_created` - Adapter created from training
- `adapter_registered` - Adapter registered in system

### Inference Events
- `inference_executed` - Inference request completed with metrics

### Error Events
- `dataset_upload_failed` - Upload failed (invalid files)
- `dataset_size_exceeded` - Size limit exceeded
- `training_failed` - Training failed (missing dataset)
- `validation_failed` - Validation failed (format issues)
- `file_corruption_detected` - File corruption detected

### Cleanup Events
- `cleanup_started` - Cleanup process initiated
- `cleanup_completed` - Cleanup completed

## Test Configuration

### Default Configuration (TestConfig::default())
- **Test Directory**: `var/tmp/adapteros_e2e`
- **Telemetry Directory**: `var/tmp/adapteros_e2e/telemetry`
- **Model Registry**: `var/tmp/adapteros_e2e/models`
- **Timeout**: 300 seconds (5 minutes)
- **Verbose Logging**: Enabled
- **CPID**: `e2e_test_cpid_001`
- **Tenant**: `e2e_test_tenant`

### Custom Configuration
Create a `TestConfig` with custom values:
```rust
let mut config = TestConfig::default();
config.test_dir = PathBuf::from("/custom/path");
config.timeout = Duration::from_secs(600);
config.verbose = false;
```

## Integration with Other E2E Tests

This test is part of the broader E2E testing framework and uses:
- **TestEnvironment**: Manages test lifecycle, telemetry, and policy enforcement
- **TestConfig**: Configuration for test execution
- **TelemetryWriter**: Records all test events for analysis

The test is registered in `/Users/star/Dev/aos/tests/e2e/mod.rs` and exported as `DatasetToInferenceTest`.

## Key Testing Patterns Used

1. **Telemetry Verification**: All operations logged to verify execution paths
2. **Error Simulation**: Explicit error scenarios to test failure handling
3. **Atomic ID Generation**: Safe concurrent ID generation using `AtomicUsize`
4. **Timeout Protection**: 5-minute timeout prevents hanging tests
5. **Phase Logging**: Clear phase names with visual indicators for debugging
6. **Cleanup Assurance**: Explicit cleanup of test artifacts

## Expected Test Duration

- **Complete Workflow**: ~5-10 seconds (simulated training)
- **Error Scenarios**: ~3-5 seconds (rapid failure simulation)
- **Total Suite**: ~15-20 seconds

## Debugging Tips

### View Telemetry Events
Check the telemetry directory for detailed event logs:
```bash
ls -la var/tmp/adapteros_e2e/telemetry/
```

### Enable Verbose Output
```bash
RUST_LOG=debug cargo test --test dataset_to_inference -- --ignored --nocapture
```

### Inspect Test Artifacts
```bash
find var/tmp/adapteros_e2e -type f -name "*.txt" -o -name "*.jsonl"
```

### Check for Panics
If a test panics, the backtrace can be viewed with:
```bash
RUST_BACKTRACE=1 cargo test --test dataset_to_inference -- --ignored --nocapture
```

## Implementation Notes

### Why Atomic Counters for IDs?
- Thread-safe unique ID generation without UUID dependency
- Simpler than uuid crate for test purposes
- Deterministic ordering for debugging

### Why JSON-based Telemetry?
- Matches AdapterOS canonical JSON format
- Human-readable event logs
- Easy to parse and analyze
- Matches production telemetry format

### Error Event Strategy
- All errors are logged to telemetry (not thrown)
- Tests verify error events were created
- Simulates production error handling without crashing

## Future Enhancements

Potential additions to the test suite:
1. Parallel dataset uploads (concurrent file handling)
2. Dataset streaming/chunking tests
3. Mixed format validation (multiple file types)
4. Training job cancellation mid-epoch
5. Resume training from checkpoint
6. Dataset caching and reuse
7. Cross-tenant dataset isolation
8. Dataset versioning tests
9. Adapter versioning with dataset lineage
10. Performance benchmarks (throughput, latency)
