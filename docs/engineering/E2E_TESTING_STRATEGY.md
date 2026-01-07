# AdapterOS E2E Testing Strategy

**Author:** Test Infrastructure Engineering
**Date:** 2026-01-06
**Status:** Implemented
**PRD References:** PRD-DET-001, PRD-DET-002

---

## Implementation Summary

The following artifacts were created to implement this testing strategy:

| Artifact | Path | Description |
|----------|------|-------------|
| **Gold Standard E2E** | `crates/adapteros-server-api/tests/gold_standard_e2e.rs` | Canonical E2E test with deterministic receipts |
| **Failure Bundle Capture** | `crates/adapteros-server-api/tests/common/test_failure_bundle.rs` | Shared failure artifact capture module |
| **Replay Harness** | `tests/determinism_replay_harness.rs` | Determinism replay verification |
| **Streaming Reliability** | `crates/adapteros-server-api/tests/streaming_reliability.rs` | SSE validation without timing flakiness |
| **Suite Runner** | `scripts/test/run_test_pyramid.sh` | Test pyramid runner with env vars and thread caps |
| **Golden Fixtures** | `tests/fixtures/golden/replay_*.json` | Placeholder golden vectors for replay |
| **CI Jobs** | `.github/workflows/ci.yml` | Added gold-standard-e2e, streaming-reliability, replay-harness jobs |

### Running the Tests

```bash
# Run PR suite (unit + integration + gold-standard)
./scripts/test/run_test_pyramid.sh

# Run full suite
./scripts/test/run_test_pyramid.sh --full

# Run nightly suite
./scripts/test/run_test_pyramid.sh --nightly

# Run specific suites
./scripts/test/run_test_pyramid.sh gold-standard
./scripts/test/run_test_pyramid.sh replay
./scripts/test/run_test_pyramid.sh streaming
```

---

## Suites & Pyramid

### Test Tier Definitions

| Tier | Scope | Runtime Budget | Parallelism | CI Trigger |
|------|-------|----------------|-------------|------------|
| **Unit** | Single function/module | <100ms per test | Unlimited | Every PR |
| **Integration** | Cross-crate boundaries | <5s per test | 4 threads | Every PR |
| **E2E (Fast)** | Server→Worker→DB | <30s total suite | 2 threads | Every PR |
| **E2E (Full)** | Complete pipeline + audit | <120s total suite | 2 threads | Nightly / Main merge |
| **Forensic Replay** | Determinism verification | <60s total suite | 1 thread (serial) | Nightly |

### Suite Details

#### 1. Unit Tests (`cargo test --lib`)

**Coverage:**
- Seed derivation: `adapteros-core/src/seed.rs`
- Q15 quantization: `adapteros-lora-router/src/quantization.rs`
- Evidence envelope construction: `adapteros-core/src/evidence_envelope.rs`
- Decision hashing: `adapteros-lora-router/src/types.rs`
- Attestation types: `adapteros-lora-kernel-api/src/attestation.rs`

**Flake Risks:** None (pure functions, no I/O)

**Mitigations:** N/A

---

#### 2. Integration Tests (`cargo test --test`)

**Coverage:**
- `crates/adapteros-db/tests/atomic_dual_write_tests.rs` - SQL↔KV consistency
- `crates/adapteros-db/tests/evidence_envelope_integration.rs` - Envelope persistence
- `crates/adapteros-lora-router/tests/determinism.rs` - Router determinism
- `crates/adapteros-server-api/tests/determinism_mode_tests.rs` - Mode resolution
- `tests/evidence_envelope_integration.rs` - Chain verification

**Flake Risks:**
- Database contention under parallel execution
- Temp directory cleanup races

**Mitigations:**
- Use `:memory:` SQLite for isolation
- `TempDir` in current directory (not `/tmp`) per test
- `--test-threads=4` ceiling

---

#### 3. E2E (Fast) Suite

**Coverage:**
- `crates/adapteros-server-api/tests/e2e_inference_test.rs::test_e2e_inference_with_audit_trail`
- Server boot → Worker mock → Single inference → Receipt validation

**Flake Risks:**
- UDS socket binding race conditions
- Mock worker response timing

**Mitigations:**
- Bind retry with exponential backoff (max 3 attempts)
- Synchronous request/response (no streaming)
- Isolated `TempDir` for socket paths

---

#### 4. E2E (Full) Suite

**Coverage:**
- All Fast E2E tests +
- `test_e2e_inference_fails_when_model_not_ready` - Fail-fast validation
- `test_e2e_inference_tenant_isolation` - Cross-tenant blocking
- Streaming integration (1 SSE run)
- Policy audit chain verification

**Flake Risks:**
- Streaming buffer timing
- Policy decision recording latency

**Mitigations:**
- Buffered channel consumption (not real-time)
- `tokio::sync::watch` for completion signal
- 5s timeout ceiling per stream test

---

#### 5. Forensic Replay Tests

**Coverage:**
- `tests/determinism_hardening_tests.rs` (T1-T10)
- `tests/determinism_smoke.rs` - Golden vector verification
- Replay harness (new, see below)

**Flake Risks:**
- Floating-point accumulation variance
- Timestamp embedding in receipts

**Mitigations:**
- Fixed seeds via `DeterminismConfig::fixed_seed`
- Mock `SystemTime` via feature flag
- Serial execution (`--test-threads=1`)

---

## Gold Standard E2E

### Minimal E2E Test Specification

**Test Name:** `test_gold_standard_e2e_inference`

**Location:** `crates/adapteros-server-api/tests/gold_standard_e2e.rs`

**Purpose:** The smallest test that validates the complete inference path with deterministic receipts.

#### Preconditions
1. In-memory SQLite database (`:memory:`)
2. `AOS_DEV_NO_AUTH=1` (bypass auth for test isolation)
3. Fixed seed: `[42u8; 32]`
4. Mock UDS worker server (single-threaded)

#### Test Stages

```
┌─────────────────────────────────────────────────────────────────┐
│ 1. Server Boot                                                   │
│    - AppState::new() with test config                           │
│    - Migrations run                                              │
│    - Policy bindings initialized                                 │
├─────────────────────────────────────────────────────────────────┤
│ 2. Worker Registration                                           │
│    - Create tenant, user, model, adapter                        │
│    - Register worker manifest                                    │
│    - Create node + plan records                                  │
│    - Bind UDS path to worker manifest                           │
├─────────────────────────────────────────────────────────────────┤
│ 3. Model Load                                                    │
│    - Mark model ready                                            │
│    - Verify /readyz returns 200                                  │
├─────────────────────────────────────────────────────────────────┤
│ 4. Inference Request                                             │
│    - POST /v1/infer with fixed seed                             │
│    - Mock worker responds with deterministic tokens              │
│    - Response collected synchronously                            │
├─────────────────────────────────────────────────────────────────┤
│ 5. Receipt Validation                                            │
│    - Extract InferenceReceiptRef from response                   │
│    - Verify all required fields present                          │
│    - Validate digests are 64-char hex                            │
├─────────────────────────────────────────────────────────────────┤
│ 6. (Optional) Streaming Run                                      │
│    - POST /v1/infer/stream with same seed                       │
│    - Consume all SSE chunks                                      │
│    - Verify [DONE] marker received                               │
│    - Verify chunk count matches non-streaming token count        │
└─────────────────────────────────────────────────────────────────┘
```

#### Exact Assertions

```rust
// Response content
assert_eq!(response.text, "mock_response_text_from_worker");
assert_eq!(response.finish_reason, "stop");
assert_eq!(response.adapters_used, vec!["test-adapter"]);

// Receipt structure (InferenceReceiptRef)
let receipt = response.receipt.expect("receipt must be present");
assert!(!receipt.trace_id.is_empty(), "trace_id required");
assert_eq!(receipt.backend_used, "mock", "backend_used must match worker response");
assert!(!receipt.router_seed.is_empty(), "router_seed required");

// Digest format validation
assert_eq!(receipt.run_head_hash.len(), 64, "run_head_hash must be 64 hex chars");
assert_eq!(receipt.output_digest.len(), 64, "output_digest must be 64 hex chars");
assert_eq!(receipt.receipt_digest.len(), 64, "receipt_digest must be 64 hex chars");

// Token accounting
assert!(receipt.logical_prompt_tokens > 0);
assert!(receipt.logical_output_tokens > 0);
assert!(receipt.billed_input_tokens >= receipt.logical_prompt_tokens);

// Stop metadata
assert_eq!(receipt.stop_reason_code, Some("end_turn".to_string()));
assert!(receipt.stop_reason_token_index.is_some());

// PRD-DET-001 fields (if strict mode)
if determinism_mode == "strict" {
    assert!(receipt.seed_lineage_hash.is_some(), "strict mode requires seed_lineage_hash");
    assert!(receipt.backend_attestation_b3.is_some(), "strict mode requires attestation");
}
```

#### Mock Worker Protocol

```rust
struct MockWorkerServer {
    socket_path: PathBuf,
    response: WorkerInferResponse,
}

impl MockWorkerServer {
    async fn handle_request(&self, request: &[u8]) -> Vec<u8> {
        // Parse request (minimal validation)
        let _req: WorkerInferRequest = serde_json::from_slice(request)?;

        // Return deterministic response
        let response = WorkerInferResponse {
            text: Some("mock_response_text_from_worker".into()),
            status: "stop".into(),
            trace: WorkerTrace {
                router_summary: RouterSummary {
                    adapters_used: vec!["test-adapter".into()],
                },
                token_count: 12,
                router_decision_chain: Some(vec![RouterDecisionChainEntry {
                    token_index: 0,
                    indices: vec![0],
                    gates_q15: vec![32767], // Max Q15 = full weight
                    entropy: 0.0,
                    decision_hash: "deadbeef".repeat(8),
                }]),
            },
            token_usage: Some(TokenUsage {
                prompt_tokens: 100,
                completion_tokens: 12,
                billed_tokens: 112,
            }),
            backend_version: Some(adapteros_core::version::VERSION.into()),
            determinism_mode_applied: Some("strict".into()),
        };

        serde_json::to_vec(&response).unwrap()
    }
}
```

#### Failure Artifacts

On test failure, capture and save:

```rust
struct FailureBundle {
    /// Full receipt JSON (if available)
    receipt_json: Option<String>,
    /// Telemetry events captured during test
    telemetry_events: Vec<TelemetryEvent>,
    /// Request/response raw bytes
    request_bytes: Vec<u8>,
    response_bytes: Vec<u8>,
    /// trace_id for correlation
    trace_id: String,
    /// Timing breakdown
    stage_timings: HashMap<&'static str, Duration>,
}

impl FailureBundle {
    fn save(&self, path: &Path) -> io::Result<()> {
        // Save as JSON for easy debugging
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path.join(format!("{}_failure.json", self.trace_id)), json)
    }
}
```

---

## Replay Harness

### Determinism Replay Test Design

**Location:** `tests/determinism_replay_harness.rs`

**Purpose:** Run the same request twice with identical inputs and verify bit-exact determinism.

#### Test Flow

```
┌──────────────────────────────────────────────────────────────────┐
│                      REPLAY TEST HARNESS                          │
├──────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ┌─────────────┐     ┌─────────────┐                             │
│  │   Run A     │     │   Run B     │                             │
│  │ (seed: S)   │     │ (seed: S)   │  ← Same seed                │
│  └──────┬──────┘     └──────┬──────┘                             │
│         │                   │                                     │
│         ▼                   ▼                                     │
│  ┌─────────────┐     ┌─────────────┐                             │
│  │ Receipt A   │     │ Receipt B   │                             │
│  └──────┬──────┘     └──────┬──────┘                             │
│         │                   │                                     │
│         └────────┬──────────┘                                     │
│                  ▼                                                │
│         ┌───────────────┐                                         │
│         │   COMPARE     │                                         │
│         │ decision_hash │  ← MUST BE EQUAL                       │
│         │ gates_q15     │  ← MUST BE EQUAL                       │
│         │ output_digest │  ← MUST BE EQUAL                       │
│         │ receipt_digest│  ← MUST BE EQUAL                       │
│         └───────────────┘                                         │
│                                                                   │
└──────────────────────────────────────────────────────────────────┘
```

#### Core Test Implementation

```rust
#[tokio::test]
async fn test_determinism_replay_identical_outputs() {
    // Setup: isolated environment with fixed seed
    let seed = [42u8; 32];
    let config = DeterminismConfig::builder()
        .fixed_seed(Some(u64::from_le_bytes(seed[..8].try_into().unwrap())))
        .fixed_timestamp(Some(1704067200)) // 2024-01-01T00:00:00Z
        .stable_ordering(true)
        .build();

    let env = TestEnvironment::new_with_determinism(config).await;

    // Run A
    let request = build_test_request(&seed);
    let result_a = env.run_inference(&request).await.unwrap();

    // Run B (identical request)
    let result_b = env.run_inference(&request).await.unwrap();

    // Assert: Router decisions equal
    assert_eq!(
        result_a.receipt.decision_hash,
        result_b.receipt.decision_hash,
        "decision_hash must be identical across replays"
    );

    // Assert: Q15 gates equal
    assert_eq!(
        result_a.trace.router_decision_chain,
        result_b.trace.router_decision_chain,
        "gates_q15 must be identical across replays"
    );

    // Assert: Output digests equal
    assert_eq!(
        result_a.receipt.output_digest,
        result_b.receipt.output_digest,
        "output_digest must be identical for deterministic replay"
    );

    // Assert: Receipt digests equal
    assert_eq!(
        result_a.receipt.receipt_digest,
        result_b.receipt.receipt_digest,
        "receipt_digest must be identical for deterministic replay"
    );

    // Assert: Seed lineage bindings equal
    assert_eq!(
        result_a.receipt.seed_lineage_hash,
        result_b.receipt.seed_lineage_hash,
        "seed_lineage_hash must be identical"
    );
}
```

#### Nondeterminism Source Isolation

| Source | Isolation Strategy |
|--------|-------------------|
| **Time** | `DeterminismConfig::fixed_timestamp` + mock `SystemTime` |
| **Random** | `HKDF(fixed_seed, label)` for all RNG |
| **Scheduling** | `--test-threads=1` + sequential execution |
| **Floating-point** | No `-ffast-math`, verified in CI |
| **Backend variance** | `MockKernels` for pure determinism |

#### Data Fixtures Strategy

**Avoid large fixtures.** Use generated fixtures with deterministic seeds:

```rust
fn generate_fixture(seed: u64, token_count: usize) -> TestFixture {
    let mut rng = StdRng::seed_from_u64(seed);
    TestFixture {
        input_tokens: (0..token_count).map(|_| rng.gen_range(0..32000)).collect(),
        adapters: vec!["adapter-a".into()],
        gates: vec![1.0], // Single adapter, full weight
    }
}
```

**Golden vectors** (small, checked into repo):

```
tests/fixtures/golden/
├── replay_001.json      # 10 tokens, single adapter
├── replay_002.json      # 50 tokens, K=3 sparse routing
└── replay_003.json      # 100 tokens, policy mask applied
```

Each golden file contains:
- Input parameters
- Expected `decision_hash`
- Expected `output_digest`
- Expected `receipt_digest`

---

## Streaming Reliability

### SSE Test Requirements

#### What to Validate

| Aspect | Validation |
|--------|------------|
| **Event format** | `data: {json}\n\n` with valid JSON |
| **Chunk ordering** | Monotonically increasing IDs |
| **Done marker** | `data: [DONE]\n\n` terminates stream |
| **Token deltas** | Each chunk's `delta.content` is non-empty (except first/last) |
| **First chunk** | Contains `role: "assistant"` |
| **Final chunk** | Contains `finish_reason: "stop"` |

#### Timing Sensitivity Mitigations

**Problem:** SSE tests are timing-sensitive due to real I/O and async scheduling.

**Solution 1: Buffered Channel Collection**

```rust
async fn collect_sse_events(stream: impl Stream<Item = SseEvent>) -> Vec<SseEvent> {
    // Collect all events into buffer, no timing assumptions
    let events: Vec<_> = stream.collect().await;

    // Validate ordering after collection
    for window in events.windows(2) {
        assert!(window[0].id < window[1].id, "Event IDs must be monotonic");
    }

    events
}
```

**Solution 2: Deterministic Event IDs**

```rust
// In ring_buffer.rs - ensure deterministic ID generation
pub fn next_id(&self) -> u64 {
    // SeqCst ensures total ordering across threads
    self.sequence.fetch_add(1, Ordering::SeqCst)
}
```

**Solution 3: Completion Signal Instead of Timeout**

```rust
async fn wait_for_stream_completion(
    stream: impl Stream<Item = SseEvent>,
    done_signal: watch::Receiver<bool>,
) -> Result<Vec<SseEvent>, StreamError> {
    let mut events = Vec::new();

    tokio::select! {
        _ = async {
            pin_mut!(stream);
            while let Some(event) = stream.next().await {
                events.push(event);
                if event.data == "[DONE]" {
                    return;
                }
            }
        } => {}
        _ = done_signal.changed() => {
            // External completion signal (for testing)
        }
    }

    Ok(events)
}
```

#### Connecting Streaming to Receipts

**Assertion:** Streaming response must produce same receipt as non-streaming.

```rust
#[tokio::test]
async fn test_streaming_receipt_matches_sync() {
    let seed = [42u8; 32];
    let request = build_test_request(&seed);

    // Sync inference
    let sync_result = env.run_inference(&request).await.unwrap();

    // Streaming inference (same request)
    let stream_events = env.run_streaming_inference(&request).await.unwrap();

    // Extract receipt from final streaming event
    let done_event = stream_events.last().unwrap();
    let stream_receipt = extract_receipt_from_done(done_event);

    // Trace IDs may differ (new request), but digests must match
    assert_eq!(
        sync_result.receipt.output_digest,
        stream_receipt.output_digest,
        "Output digest must match between sync and streaming"
    );

    // Token count must match
    let stream_token_count: usize = stream_events
        .iter()
        .filter(|e| e.event_type == "token")
        .count();
    assert_eq!(
        sync_result.receipt.logical_output_tokens as usize,
        stream_token_count,
        "Token count must match"
    );
}
```

#### SSE Test Checklist

- [ ] First event has `choices[0].delta.role = "assistant"`
- [ ] Intermediate events have `choices[0].delta.content` (non-empty string)
- [ ] Final event has `choices[0].finish_reason = "stop"`
- [ ] `[DONE]` marker is last event
- [ ] Event IDs are monotonically increasing
- [ ] All events parse as valid JSON
- [ ] `trace_id` header matches `id` field prefix
- [ ] Token count matches receipt's `logical_output_tokens`

---

## CI Plan

### Test Distribution by Trigger

| Test Suite | PR | Main Merge | Nightly |
|------------|:--:|:----------:|:-------:|
| Format, Clippy | ✓ | ✓ | ✓ |
| Unit Tests | ✓ | ✓ | ✓ |
| Integration Tests | ✓ | ✓ | ✓ |
| Gold Standard E2E | ✓ | ✓ | ✓ |
| Full E2E Suite | | ✓ | ✓ |
| Determinism Suite | | ✓ | ✓ |
| Forensic Replay | | | ✓ |
| Streaming Full | | | ✓ |
| Stress Tests | | | ✓ |

### Timeout Configuration

```yaml
# Per-job timeouts (conservative)
jobs:
  unit-tests:
    timeout-minutes: 10
  integration-tests:
    timeout-minutes: 20
  gold-standard-e2e:
    timeout-minutes: 5    # Must be fast
  full-e2e:
    timeout-minutes: 30
  determinism-suite:
    timeout-minutes: 15
  forensic-replay:
    timeout-minutes: 20
  streaming-tests:
    timeout-minutes: 10
```

### Retry Policy

```yaml
# Only retry on infrastructure failures, not test failures
steps:
  - name: Run tests
    uses: nick-fields/retry@v2
    with:
      timeout_minutes: 10
      max_attempts: 2
      retry_on: error  # Not on failure (test assertion failures)
      command: cargo test ...
```

### Required Environment Variables

```yaml
env:
  # Auth bypass for test isolation
  AOS_DEV_NO_AUTH: "1"

  # Determinism enforcement
  AOS_DETERMINISM_SEED: "2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a"
  AOS_DEBUG_DETERMINISM: "1"  # Enable for debug logs on failure

  # Backend selection (no GPU in CI)
  AOS_BACKEND: "mock"

  # Logging for actionable diagnostics
  RUST_BACKTRACE: "1"
  RUST_LOG: "adapteros=debug,tower_http=warn"

  # Legacy guard
  AOS_ALLOW_LEGACY_AOS: "0"

  # Storage backend for dual-write tests
  AOS_STORAGE_BACKEND: "dual_write"
  AOS_ATOMIC_DUAL_WRITE_STRICT: "true"
```

### Actionable Test Logs

Every test must log:

```rust
// At test start
tracing::info!(
    trace_id = %request.trace_id,
    seed = ?seed_hex,
    test_name = %test_name,
    "Starting E2E test"
);

// At test end (success)
tracing::info!(
    trace_id = %request.trace_id,
    receipt_digest = %receipt.receipt_digest,
    duration_ms = %elapsed.as_millis(),
    "E2E test passed"
);

// At test end (failure)
tracing::error!(
    trace_id = %request.trace_id,
    expected_digest = %expected,
    actual_digest = %actual,
    "Determinism violation detected"
);
```

### CI Job Definition (New)

Add to `.github/workflows/ci.yml`:

```yaml
gold-standard-e2e:
  name: Gold Standard E2E
  runs-on: ubuntu-latest
  timeout-minutes: 5
  needs: [test]  # Run after unit tests pass
  steps:
    - uses: actions/checkout@v4
    - uses: dtolnay/rust-toolchain@stable
    - name: Run Gold Standard E2E
      env:
        AOS_DEV_NO_AUTH: "1"
        AOS_BACKEND: "mock"
        AOS_DETERMINISM_SEED: "2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a"
      run: |
        cargo test --test gold_standard_e2e -- --nocapture
    - name: Upload failure artifacts
      if: failure()
      uses: actions/upload-artifact@v4
      with:
        name: e2e-failure-bundle
        path: target/test-failures/

forensic-replay:
  name: Forensic Replay Tests
  runs-on: ubuntu-latest
  timeout-minutes: 20
  # Nightly only
  if: github.event_name == 'schedule' || github.event_name == 'workflow_dispatch'
  steps:
    - uses: actions/checkout@v4
    - uses: dtolnay/rust-toolchain@stable
    - name: Run Replay Harness
      env:
        AOS_DETERMINISM_SEED: "2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a"
      run: |
        cargo test --test determinism_replay_harness -- --test-threads=1 --nocapture
    - name: Run Golden Vector Tests
      run: cargo test --test determinism_smoke -- --nocapture
```

---

## New Tests Backlog

### 1. Receipt Mismatch Detection

**Name:** `test_receipt_mismatch_raises_alarm`

**Location:** `crates/adapteros-server-api/tests/receipt_validation.rs`

**Prerequisites:**
- Mock worker that returns inconsistent receipt data
- Telemetry capture enabled

**Assertions:**
- `DeterminismViolation` event emitted when receipt fields don't match
- Response includes `determinism_warning` header
- Metrics counter `aos_receipt_mismatch_total` incremented

**Expected Runtime:** <2s

**Bug Class:** Silent determinism violations, audit trail corruption

---

### 2. Determinism Violation Event Emission

**Name:** `test_determinism_violation_event_emitted`

**Location:** `crates/adapteros-telemetry/tests/determinism_events.rs`

**Prerequisites:**
- Event bus subscription
- Forced determinism violation (different output for same seed)

**Assertions:**
- `DeterminismViolationEvent` published within 100ms
- Event contains: `trace_id`, `expected_digest`, `actual_digest`, `violation_type`
- Event persisted to telemetry store

**Expected Runtime:** <1s

**Bug Class:** Missing observability for determinism failures

---

### 3. Dual-Write Drift Detection Metrics

**Name:** `test_dual_write_drift_increments_metric`

**Location:** `crates/adapteros-db/tests/drift_metrics.rs`

**Prerequisites:**
- `AOS_STORAGE_BACKEND=dual_write`
- Simulated SQL↔KV divergence

**Assertions:**
- `aos_dual_write_drift_total` counter incremented
- Drift report contains field name and values
- Alert threshold triggers at >0 drift

**Expected Runtime:** <2s

**Bug Class:** PRD-DET-002 compliance, silent data corruption

---

### 4. Backend Used Propagation

**Name:** `test_backend_used_propagates_to_receipt`

**Location:** `crates/adapteros-server-api/tests/backend_propagation.rs`

**Prerequisites:**
- Mock worker configured with specific backend identifier
- Receipt extraction from response

**Assertions:**
- `response.receipt.backend_used == "mock"`
- `response.headers["X-AOS-Backend"] == "mock"`
- Backend matches worker's `DeterminismReport.backend_type`

**Expected Runtime:** <3s

**Bug Class:** Missing backend attribution, incorrect audit trails

---

### 5. FIFO Determinism Under Load

**Name:** `test_fifo_determinism_stress`

**Location:** `crates/adapteros-lora-router/tests/determinism.rs` (existing, enhance)

**Prerequisites:**
- 1000 concurrent routing decisions
- Fixed seed for reproducibility

**Assertions:**
- All 1000 iterations produce identical routing decisions
- No race conditions in decision ordering
- Completed within 10s (100 decisions/sec minimum)

**Expected Runtime:** <10s

**Bug Class:** Concurrent routing nondeterminism, tie-break instability

**CI Integration:** Add to nightly with `--test-threads=8` for stress

---

### 6. Seed Lineage Hash Stability

**Name:** `test_seed_lineage_hash_cross_version_stable`

**Location:** `tests/determinism_hardening_tests.rs`

**Prerequisites:**
- Known seed + mode combinations
- Golden hash values from previous version

**Assertions:**
- `SeedLineage::to_binding_hash()` matches golden value
- No regression from `HKDF_ALGORITHM_VERSION` changes

**Expected Runtime:** <100ms

**Bug Class:** Seed algorithm drift, replay incompatibility

---

### 7. Evidence Chain Tamper Detection

**Name:** `test_evidence_chain_tamper_detection_comprehensive`

**Location:** `tests/evidence_envelope_integration.rs`

**Prerequisites:**
- Chain of 10 linked envelopes
- Various tampering scenarios

**Assertions:**
- Tampered `root` detected immediately
- Tampered `previous_root` detected
- Tampered `inference_receipt_ref` field detected
- `verify_policy_audit_chain()` returns `is_valid: false`

**Expected Runtime:** <2s

**Bug Class:** Audit trail manipulation, evidence forgery

---

### 8. Strict Mode Seed Requirement

**Name:** `test_strict_mode_requires_seed`

**Location:** `crates/adapteros-server-api/tests/determinism_mode_tests.rs`

**Prerequisites:**
- Request with `determinism_mode: "strict"`
- No seed provided

**Assertions:**
- Returns 400 Bad Request
- Error code: `STRICT_MODE_SEED_REQUIRED`
- Error message includes required field

**Expected Runtime:** <1s

**Bug Class:** Silent fallback to nondeterministic mode

---

### 9. Q15 Quantization Boundary

**Name:** `test_q15_boundary_values`

**Location:** `crates/adapteros-lora-router/tests/determinism.rs`

**Prerequisites:**
- Gate values at boundaries: 0.0, 0.5, 1.0, 1.0 - ε

**Assertions:**
- `0.0 → 0` (Q15)
- `1.0 → 32767` (Q15)
- Round-trip error < 1/32767
- No overflow for edge cases

**Expected Runtime:** <100ms

**Bug Class:** Quantization errors at boundaries, overflow bugs

---

### 10. Router Decision Hash Determinism

**Name:** `test_router_decision_hash_identical_inputs`

**Location:** `crates/adapteros-lora-router/tests/decision_hash.rs`

**Prerequisites:**
- Identical feature vectors and priors
- Multiple invocations

**Assertions:**
- `DecisionHash.combined_hash` identical across 100 runs
- Hash includes: input, output, tau, eps, k
- Backend identity hash included when present

**Expected Runtime:** <500ms

**Bug Class:** Decision hash computation drift

---

### 11. Streaming Event ID Monotonicity

**Name:** `test_sse_event_ids_monotonic_under_load`

**Location:** `crates/adapteros-server-api/tests/streaming_reliability.rs`

**Prerequisites:**
- 5 concurrent streaming sessions
- 100 tokens each

**Assertions:**
- Within each stream: `event[n].id < event[n+1].id`
- No ID gaps within stream
- Cross-stream IDs may interleave but each stream is monotonic

**Expected Runtime:** <5s

**Bug Class:** SSE event ordering corruption, client reconnect issues

---

### 12. Backend Attestation Hash Binding

**Name:** `test_backend_attestation_bound_to_receipt`

**Location:** `tests/determinism_hardening_tests.rs`

**Prerequisites:**
- `DeterminismReport::for_metal_verified()` with known hash
- Receipt generation

**Assertions:**
- `receipt.backend_attestation_b3 == report.to_attestation_hash()`
- Different metallib → different attestation hash
- Missing attestation in strict mode → validation error

**Expected Runtime:** <500ms

**Bug Class:** Backend binding bypass, attestation forgery

---

### 13. Policy Mask Digest in Decision

**Name:** `test_policy_mask_digest_propagates`

**Location:** `crates/adapteros-server-api/tests/policy_routing.rs`

**Prerequisites:**
- Active policy pack with routing constraints
- Inference request

**Assertions:**
- `decision.policy_mask_digest_b3` is Some
- Digest changes when policy changes
- Digest stable for same policy configuration

**Expected Runtime:** <2s

**Bug Class:** Policy bypass, missing audit context

---

### 14. Model Not Ready Fast Fail

**Name:** `test_model_not_ready_fast_fail_timing`

**Location:** `crates/adapteros-server-api/tests/e2e_inference_test.rs` (existing, enhance)

**Prerequisites:**
- Model registered but not marked ready
- Timing instrumentation

**Assertions:**
- Returns 503 within 100ms
- No worker communication attempted
- Error code: `MODEL_NOT_READY`

**Expected Runtime:** <200ms

**Bug Class:** Slow failure paths, unnecessary resource consumption

---

### 15. Tenant Isolation Enforcement

**Name:** `test_tenant_isolation_adapter_access`

**Location:** `crates/adapteros-server-api/tests/e2e_inference_test.rs` (existing)

**Prerequisites:**
- Two tenants with separate adapters
- Cross-tenant access attempt

**Assertions:**
- Returns 403 or 404 (adapter not found for tenant)
- No data leakage in error response
- Audit log records denied access

**Expected Runtime:** <2s

**Bug Class:** Cross-tenant data access, privilege escalation

---

## Risks / Unknowns

### Known Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| **Streaming tests flaky in CI** | High | Medium | Buffered collection, completion signals, no timeouts |
| **Mock backend diverges from real** | Medium | High | Quarterly real-backend validation runs |
| **Golden vectors become stale** | Low | Medium | Version-tagged fixtures, migration tests |
| **Seed algorithm changes break replay** | Low | High | `HKDF_ALGORITHM_VERSION` checks, golden hash tests |

### Unknowns Requiring Investigation

1. **Floating-point determinism on ARM vs x86**
   - CI runs on both (macos-14 ARM, ubuntu x86)
   - Need to verify Metal/CoreML backends produce identical results
   - Mitigation: Add cross-platform golden vector tests

2. **SQLite WAL mode interaction with dual-write**
   - In-memory tests use different mode than production
   - May mask race conditions
   - Mitigation: Add explicit WAL mode tests

3. **Ring buffer eviction under extreme load**
   - `has_gap()` uses `Ordering::Relaxed` for `lowest_id`
   - May produce false positives on weak memory models
   - Mitigation: Upgrade to `Ordering::Acquire` or add fence

4. **Circuit breaker recovery timing in tests**
   - 30-second hardcoded recovery timeout
   - Tests can't fast-forward time
   - Mitigation: Make recovery timeout configurable for tests

### Dependencies on External Work

- **Mock clock infrastructure**: Needed for deterministic streaming tests
- **Feature flag for test time control**: `#[cfg(test)]` time mocking
- **Failure bundle capture**: New test infrastructure code

---

## Appendix: Test File Locations Summary

| Suite | Primary Location |
|-------|------------------|
| Unit | `crates/*/src/*.rs` (inline `#[cfg(test)]`) |
| Integration (router) | `crates/adapteros-lora-router/tests/determinism.rs` |
| Integration (db) | `crates/adapteros-db/tests/*.rs` |
| E2E (canonical) | `crates/adapteros-server-api/tests/e2e_inference_test.rs` |
| E2E (gold standard) | `crates/adapteros-server-api/tests/gold_standard_e2e.rs` (NEW) |
| Streaming | `crates/adapteros-server-api/tests/streaming_*.rs` |
| Determinism | `tests/determinism_hardening_tests.rs` |
| Replay harness | `tests/determinism_replay_harness.rs` (NEW) |
| Golden vectors | `tests/determinism_smoke.rs` |
| Kernel harness | `tests/e2e_inference_harness.rs` |
