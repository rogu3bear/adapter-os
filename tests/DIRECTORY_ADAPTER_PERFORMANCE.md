# Directory Adapter Performance Testing

## Overview

This document describes the performance testing infrastructure for the `upsert_directory_adapter` handler, designed to measure and verify improvements from async refactoring.

## Goal

Measure the time spent in **filesystem operations** vs. **database operations** to verify that parallelizing these operations reduces total handler execution time.

## Architecture

### Tracing Instrumentation

**File:** `crates/adapteros-server-api/src/handlers.rs` (upsert_directory_adapter:98-347)

The handler now includes comprehensive tracing spans:

#### Top-Level Span
- `upsert_directory_adapter_handler` - Measures end-to-end handler execution

#### Filesystem Operation Spans (Blocking Task)
- `directory_adapter_blocking_ops` - Overall blocking task wrapper
- `path_validation` - Path validation checks
- `directory_analysis` - Directory traversal, file reading, symbol parsing, hashing
- `artifact_creation` - Placeholder `.safetensors` file creation

#### Database Operation Spans
- `db_get_adapter_check` - Initial adapter existence check
- `db_register_adapter` - Adapter registration (if new)
- `db_get_adapter_for_activation` - Get adapter for activation (if requested)
- `db_update_adapter_state_loading` - Update state to "loading"
- `db_update_adapter_state_success` - Update state to "warm" (success)
- `db_update_adapter_state_failure` - Update state to "cold" (failure)
- `db_update_adapter_state_simulated` - Update state for simulated load

### Tracing Analyzer Utilities

**File:** `tests/helpers/tracing_analyzer.rs`

Provides tools for capturing and analyzing tracing spans:

#### `TracingCapture`
Custom tracing layer that captures span names, durations, and fields.

```rust
let capture = TracingCapture::new();
let subscriber = Registry::default().with(capture.clone());
tracing::subscriber::set_global_default(subscriber);

// ... run code ...

let spans = capture.get_spans();
```

#### `TimingMetrics`
Aggregates span data into performance metrics:

```rust
pub struct TimingMetrics {
    pub filesystem_time_ms: u64,      // Sum of fs operation spans
    pub database_time_ms: u64,         // Sum of db_* spans
    pub total_handler_time_ms: u64,    // Handler span duration
    pub fs_db_ratio: f64,              // filesystem_time / database_time
    pub span_breakdown: HashMap<String, u64>,
}
```

Methods:
- `from_spans(&[SpanRecord])` - Parse spans into metrics
- `save_baseline(path)` - Save metrics to JSON
- `load_baseline(path)` - Load metrics from JSON

#### `ImprovementReport`
Compares baseline vs. current metrics:

```rust
let report = ImprovementReport::compare(baseline, current);
report.print_report();  // Detailed comparison output
report.assert_improvements(min_total_improvement_pct, max_acceptable_ratio);
```

Tracks:
- Total time improvement (%)
- Filesystem time improvement (%)
- Database time improvement (%)
- FS/DB ratio change

### Integration Tests

**File:** `tests/directory_adapter_performance.rs`

#### Test Fixture Setup

`TestFixture::new()` creates:
- Temporary directory with realistic Rust project structure
  - `src/main.rs`, `src/lib.rs`, `src/utils.rs`
  - `tests/integration_test.rs`
  - `README.md`
- In-memory SQLite database
- Test tenant `test-tenant`

#### Baseline Test

`test_directory_adapter_timing_baseline`:
1. Initialize tracing capture layer
2. Create test fixture and AppState
3. Call `upsert_directory_adapter` handler
4. Extract timing metrics from captured spans
5. Save baseline to `test_data/directory_adapter_baseline.json`
6. Print metrics for inspection

Run with:
```bash
cargo test test_directory_adapter_timing_baseline --nocapture
```

#### Verification Test

`test_directory_adapter_timing_after_refactor` (marked `#[ignore]`):
1. Initialize tracing capture layer
2. Create test fixture and AppState
3. Call `upsert_directory_adapter` handler
4. Extract current timing metrics
5. Load baseline metrics from JSON
6. Compare and assert improvements:
   - Total time improvement ≥20%
   - FS/DB ratio ≤5.0

Run after async refactor with:
```bash
cargo test test_directory_adapter_timing_after_refactor --nocapture --include-ignored
```

## Expected Performance Profile

### Current Implementation (Serial)

```
┌─────────────────────────────────────┐
│ Handler Start                       │
├─────────────────────────────────────┤
│ Permission check                    │
└─────────────────────────────────────┘
            ↓
┌─────────────────────────────────────┐
│ spawn_blocking (500ms)              │
│  - Path validation (10ms)           │
│  - Directory analysis (480ms)       │
│  - Artifact creation (10ms)         │
└─────────────────────────────────────┘
            ↓ (wait for blocking)
┌─────────────────────────────────────┐
│ Database operations (50ms)          │
│  - get_adapter (30ms)               │
│  - register_adapter (20ms)          │
└─────────────────────────────────────┘

Total: 550ms
FS/DB Ratio: 10.0
```

### After Async Refactor (Parallel)

```
┌─────────────────────────────────────┐
│ Handler Start                       │
├─────────────────────────────────────┤
│ Permission check                    │
└─────────────────────────────────────┘
            ↓
┌──────────────────────┐  ┌──────────────────────┐
│ Filesystem (500ms)   │  │ DB Check (30ms)      │
│  - Path validation   │  │  - get_adapter       │
│  - Directory         │  └──────────────────────┘
│    analysis          │            ↓
│  - Artifact creation │  ┌──────────────────────┐
└──────────────────────┘  │ DB Register (20ms)   │
            ↓             │  - register_adapter  │
            └─────────────┴──────────────────────┘

Total: 500ms (operations overlap)
FS/DB Ratio: 1.0-2.0
```

## Refactoring Approach

To achieve the parallel execution model:

### Option 1: Parallelize Initial DB Check

Move the initial `db.get_adapter()` check to run concurrently with the blocking task:

```rust
// Before
let (adapter_id, hash_hex, ...) = blocking_task().await?;
let existing = state.db.get_adapter(&adapter_id).await?;

// After
let (blocking_result, db_check) = tokio::join!(
    blocking_task(),
    state.db.get_adapter(&tentative_adapter_id)
);
```

**Challenge:** Adapter ID is computed inside the blocking task from the directory fingerprint. Would need to either:
- Compute tentative adapter ID before blocking task (requires exposing fingerprint calculation)
- Accept that this specific DB call remains sequential

### Option 2: Use Async Filesystem Operations

Replace `tokio::task::spawn_blocking` with async filesystem operations:

```rust
// Before
tokio::task::spawn_blocking(move || {
    let root = std::path::PathBuf::from(&root_str);
    if !root.exists() { ... }
    // ... more std::fs operations
})

// After
use tokio::fs;
let root = PathBuf::from(&root_str);
if !fs::try_exists(&root).await? { ... }
// ... use tokio::fs throughout
```

**Benefits:**
- Truly async filesystem operations
- Can interleave DB calls during directory traversal
- Better tokio runtime integration

**Challenges:**
- `adapteros_codegraph::analyze_directory` is synchronous and CPU-intensive
- Would need to refactor codegraph module for async

### Option 3: Hybrid Approach

Keep directory analysis blocking but parallelize database checks around it:

```rust
// Compute adapter ID early (lightweight)
let tentative_id = compute_tentative_adapter_id(&root, &path);

// Run DB check in parallel with blocking analysis
let (analysis_result, existing_adapter) = tokio::join!(
    spawn_blocking(|| analyze_directory(&root, &path)),
    state.db.get_adapter(&tentative_id)
);
```

## Usage

### Step 1: Establish Baseline (Before Refactor)

```bash
# Run baseline test
cargo test test_directory_adapter_timing_baseline --nocapture

# Or use helper script
./scripts/verify_directory_adapter_perf.sh baseline
```

This creates `test_data/directory_adapter_baseline.json`:

```json
{
  "filesystem_time_ms": 500,
  "database_time_ms": 50,
  "total_handler_time_ms": 550,
  "fs_db_ratio": 10.0,
  "span_breakdown": {
    "directory_adapter_blocking_ops": 500,
    "path_validation": 10,
    "directory_analysis": 480,
    "artifact_creation": 10,
    "db_get_adapter_check": 30,
    "db_register_adapter": 20
  }
}
```

### Step 2: Implement Async Refactor

Choose and implement one of the refactoring approaches above.

### Step 3: Verify Improvements

```bash
# Run verification test
cargo test test_directory_adapter_timing_after_refactor --nocapture --include-ignored

# Or use helper script
./scripts/verify_directory_adapter_perf.sh verify
```

The test will:
1. Run the handler with the same test fixture
2. Capture timing metrics
3. Load baseline from JSON
4. Compare and print detailed report
5. Assert improvements meet thresholds:
   - Total time improvement ≥20%
   - FS/DB ratio ≤5.0

Example output:

```
=== Performance Comparison Report ===

Baseline:
  Total time:      550 ms
  Filesystem time: 500 ms
  Database time:   50 ms
  FS/DB ratio:     10.00

Current:
  Total time:      500 ms
  Filesystem time: 500 ms
  Database time:   50 ms
  FS/DB ratio:     1.00

Improvements:
  Total time:      9.1%
  Filesystem time: 0.0%
  Database time:   0.0%
  Ratio change:    9.00

✓ Performance improvements verified!
```

## Troubleshooting

### Test Fails: Baseline Not Found

```
Error: Failed to load baseline - run test_directory_adapter_timing_baseline first
```

**Solution:** Run the baseline test first to create the JSON file.

### Improvements Don't Meet Thresholds

```
Assertion failed: Total time improvement 5.0% is below threshold 20.0%
```

**Possible causes:**
- Async refactor not implemented yet (this is expected before refactoring)
- Operations not actually running in parallel
- Database operations too fast to measure meaningful improvement
- System load affecting timing measurements

**Solutions:**
- Verify async refactoring implementation
- Check that database calls are not awaited sequentially
- Run test multiple times to average out noise
- Consider adjusting thresholds if operations are inherently very fast

### Tracing Spans Not Captured

```
Error: No spans captured
```

**Possible causes:**
- Tracing subscriber not initialized
- Spans dropped before capture
- Subscriber configured incorrectly

**Solutions:**
- Ensure `init_tracing()` is called before handler execution
- Check that subscriber is set as global default
- Verify span guards (`_span`) are not dropped prematurely

## Future Enhancements

1. **Automated Benchmarking**: Integrate with `criterion` for statistical benchmarking
2. **CI Integration**: Run baseline test in CI, store results as artifacts
3. **Regression Detection**: Alert if performance regresses below baseline
4. **Multi-Platform Testing**: Test on different systems to verify portability
5. **Larger Test Fixtures**: Test with various directory sizes (small, medium, large)
6. **Concurrency Testing**: Measure performance with concurrent handler invocations

## References

- Handler implementation: `crates/adapteros-server-api/src/handlers.rs:98-347`
- Tracing utilities: `tests/helpers/tracing_analyzer.rs`
- Integration tests: `tests/directory_adapter_performance.rs`
- Verification script: `scripts/verify_directory_adapter_perf.sh`
- Baseline storage: `test_data/directory_adapter_baseline.json`
