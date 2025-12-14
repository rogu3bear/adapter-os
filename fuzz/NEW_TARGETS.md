# New Fuzzing Targets

This document describes the new fuzzing targets added to the AdapterOS project.

## Overview

Four new fuzzing targets have been created to test recently added modules:

1. **evidence_envelope** - Evidence envelope creation and validation
2. **evidence_chain_verification** - Evidence chain verification logic
3. **stop_controller** - Deterministic stop controller decision logic
4. **kv_quota_reservation** - KV cache quota reservation and enforcement

## Running the Fuzz Targets

### Prerequisites

```bash
cargo install cargo-fuzz
```

### Running Individual Targets

```bash
# Evidence envelope fuzzing
cargo fuzz run evidence_envelope

# Evidence chain verification fuzzing
cargo fuzz run evidence_chain_verification

# Stop controller fuzzing
cargo fuzz run stop_controller

# KV quota reservation fuzzing
cargo fuzz run kv_quota_reservation
```

### Running with Custom Options

```bash
# Run for specific time (e.g., 60 seconds)
cargo fuzz run evidence_envelope -- -max_total_time=60

# Run with specific number of runs
cargo fuzz run stop_controller -- -runs=10000

# Run with multiple jobs (parallel fuzzing)
cargo fuzz run kv_quota_reservation -- -jobs=4
```

## Target Details

### 1. evidence_envelope

**Module:** `crates/adapteros-core/src/evidence_envelope.rs`

**Coverage:**
- Envelope creation for all three scopes (telemetry, policy, inference)
- Canonical byte encoding determinism
- Validation logic for all envelope types
- Digest computation correctness
- Chain linking with various `previous_root` combinations
- JSON serialization roundtrips

**Key Properties Tested:**
- Canonical bytes must be deterministic (same input → same output)
- Digest computation must be deterministic
- Scope must match payload type
- Exactly one payload ref must be populated

### 2. evidence_chain_verification

**Module:** `crates/adapteros-core/src/evidence_verifier.rs`

**Coverage:**
- Chain verification with valid chains (1-8 envelopes)
- Chain verification with broken links
- Chain verification with corrupted roots
- Single envelope verification
- Empty chain handling
- Schema version mismatches
- Chain divergence detection

**Key Properties Tested:**
- Valid chains pass verification
- Broken chains are detected
- Divergence is properly flagged
- Schema version validation works
- Root hash verification catches corruption

### 3. stop_controller

**Module:** `crates/adapteros-lora-worker/src/stop_controller.rs`

**Coverage:**
- Budget enforcement (BUDGET_MAX) with various token counts
- EOS probability detection (COMPLETION_CONFIDENT) with different logit distributions
- Repetition detection (REPETITION_GUARD) with various n-gram patterns
- EOS token detection (LENGTH)
- Determinism verification (same inputs → same outputs)
- Edge cases: empty logits, extreme values, boundary conditions
- Policy digest computation

**Key Properties Tested:**
- Stop decisions are deterministic (no RNG)
- Budget cap is always enforced
- EOS probability uses Q15 quantization
- Repetition detection is deterministic
- Two controllers with same policy and inputs produce identical decisions
- Generated token count tracking is accurate

### 4. kv_quota_reservation

**Module:** `crates/adapteros-lora-worker/src/kv_quota.rs`

**Coverage:**
- Reservation creation within quota limits
- Reservation finalization (moving from reserved → used)
- Reservation rollback (releasing reserved bytes)
- Quota overflow detection and rejection
- Concurrent reservation handling
- Eviction tracking
- Edge cases: zero quota, unlimited quota, exact limits, zero-size reservations

**Key Properties Tested:**
- Reservations never exceed quota
- Used + reserved bytes never exceed quota
- Finalization correctly transfers bytes from reserved to used
- Rollback releases reserved bytes without affecting used bytes
- Unlimited quota (None) always succeeds
- Eviction counter tracks correctly
- Usage percentage calculation is valid (0-100% or valid quota states)

## Corpus Directories

Seed inputs are stored in:
- `fuzz/corpus/evidence_envelope/`
- `fuzz/corpus/evidence_chain_verification/`
- `fuzz/corpus/stop_controller/`
- `fuzz/corpus/kv_quota_reservation/`

## Integration with CI

These fuzz targets can be integrated into CI pipelines:

```bash
# Run all targets for 30 seconds each
for target in evidence_envelope evidence_chain_verification stop_controller kv_quota_reservation; do
    cargo fuzz run $target -- -max_total_time=30 || exit 1
done
```

## Coverage Reports

To generate coverage reports:

```bash
# Install coverage tools
cargo install cargo-cov

# Run with coverage
cargo fuzz coverage evidence_envelope
cargo cov report fuzz/target/*/coverage
```

## Findings

Document any crashes or hangs found by fuzzing in this section:

- **Date**: YYYY-MM-DD
- **Target**: target_name
- **Issue**: Description
- **Fix**: PR or commit reference

## References

- [libFuzzer Documentation](https://llvm.org/docs/LibFuzzer.html)
- [cargo-fuzz Guide](https://rust-fuzz.github.io/book/cargo-fuzz.html)
- [AdapterOS CLAUDE.md](../CLAUDE.md) - Project architecture and conventions
