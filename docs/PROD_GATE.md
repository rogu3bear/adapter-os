# PROD_GATE

Status: Draft.

## Definition

adapterOS inference is "prod ready" when the prod gate suite passes with no failed checks
and emits a report artifact.

## Gate Command

```
cargo test -p adapteros-e2e --features prod-gate
```

## Report Artifact

Default path:

```
target/prod-gate/report.json
```

Override path:

```
AOS_PROD_GATE_REPORT=/path/to/report.json
```

The report includes per-check status, duration, and a pass/fail summary.

## Report Schema

The report artifact is a JSON file with the following structure:

```json
{
  "gate": "prod-gate",
  "status": "pass" | "fail",
  "timestamp_unix": 1234567890,
  "checks": [
    {
      "name": "routing_correctness",
      "status": "pass" | "fail",
      "duration_ms": 123,
      "details": { ... }
    }
  ],
  "summary": {
    "total": 7,
    "passed": 7,
    "failed": 0
  }
}
```

### Top-Level Fields

| Field | Type | Description |
|-------|------|-------------|
| `gate` | string | Always `"prod-gate"` |
| `status` | string | `"pass"` if all checks passed, `"fail"` otherwise |
| `timestamp_unix` | integer | Unix timestamp when the gate run completed |
| `checks` | array | Array of individual check results |
| `summary` | object | Aggregate pass/fail counts |

## Check Details

Each check in the `checks` array includes a `details` object with check-specific information:

### routing_correctness

```json
{
  "indices": [0, 2, 5],
  "gates_q15": [32767, 16384, 8192]
}
```

- `indices`: Selected adapter indices in deterministic order
- `gates_q15`: Q15-quantized gate values (denominator 32767)

### base_model_residency

```json
{
  "resident_bytes": 14336000000,
  "eviction_attempts": 0,
  "pinned": true
}
```

- `resident_bytes`: Memory footprint of the base model
- `eviction_attempts`: Number of eviction attempts observed (should be 0)
- `pinned`: Whether the model remained pinned throughout the test

### adapter_integrity

```json
{
  "stack_hashes": {
    "stack_a": "blake3:abc123...",
    "stack_b": "blake3:def456..."
  },
  "mismatches": []
}
```

- `stack_hashes`: Map of stack names to their canonical BLAKE3 hashes
- `mismatches`: Array of stack names with hash mismatches (empty on pass)

### generation_parity

```json
{
  "runs": 3,
  "outputs_identical": true,
  "reference_hash": "blake3:789abc...",
  "diverged_at_run": null
}
```

- `runs`: Number of repeated generation runs
- `outputs_identical`: Whether all runs produced identical output
- `reference_hash`: BLAKE3 hash of the reference output
- `diverged_at_run`: Run number where divergence occurred (null if identical)

### determinism_envelope

```json
{
  "seed_verified": true,
  "q15_denominator": 32767,
  "hkdf_chain_length": 4
}
```

- `seed_verified`: Whether seed derivation matches expected HKDF-SHA256 output
- `q15_denominator`: Verified Q15 quantization denominator (must be 32767)
- `hkdf_chain_length`: Number of HKDF derivation steps verified

### cancellation

```json
{
  "token_set": true,
  "token_observed": true,
  "cleanup_completed": true
}
```

- `token_set`: Whether the cancellation token was properly set
- `token_observed`: Whether the worker observed the cancellation signal
- `cleanup_completed`: Whether resources were properly released

### latency_sanity

```json
{
  "router_p99_ms": 12,
  "threshold_ms": 50,
  "samples": 100
}
```

- `router_p99_ms`: 99th percentile router decision latency
- `threshold_ms`: Maximum allowed latency threshold
- `samples`: Number of timing samples collected

## Interpreting Results

### Pass Criteria

A check passes when:

| Check | Pass Condition |
|-------|---------------|
| `routing_correctness` | Tie-breaking is deterministic (score DESC, index ASC) |
| `base_model_residency` | Model remains pinned with zero eviction attempts |
| `adapter_integrity` | All stack hashes match canonical values |
| `generation_parity` | All repeated runs produce byte-identical output |
| `determinism_envelope` | Seed derivation and Q15 denominator are correct |
| `cancellation` | Token is set, observed, and cleanup completes |
| `latency_sanity` | P99 latency is below the configured threshold |

### Common Failure Modes and Remediation

#### routing_correctness: FAIL

**Symptom**: `indices` or `gates_q15` differ across runs.

**Causes**:
- Non-deterministic floating-point operations in gate computation
- Incorrect tie-breaking logic

**Remediation**:
1. Verify no `-ffast-math` flags in build
2. Check `AOS_DEBUG_DETERMINISM=1` logs for seed inputs
3. Review router tie-breaking in `crates/adapteros-lora-router/`

#### base_model_residency: FAIL

**Symptom**: `eviction_attempts > 0` or `pinned: false`.

**Causes**:
- Insufficient unified memory
- Cache pressure from concurrent operations

**Remediation**:
1. Increase memory allocation or reduce batch size
2. Check for memory leaks in adapter loading
3. Verify model pinning configuration

#### adapter_integrity: FAIL

**Symptom**: Non-empty `mismatches` array.

**Causes**:
- Adapter weights modified after registration
- Stack composition order changed

**Remediation**:
1. Re-register adapters from canonical sources
2. Verify stack ordering matches expected composition
3. Check for file corruption in adapter storage

#### generation_parity: FAIL

**Symptom**: `outputs_identical: false` with `diverged_at_run` set.

**Causes**:
- Non-deterministic sampling
- Seed not propagated correctly

**Remediation**:
1. Verify seed derivation path with `AOS_DEBUG_DETERMINISM=1`
2. Check for uninitialized memory in generation buffers
3. Review sampler configuration for temperature/top-p settings

#### determinism_envelope: FAIL

**Symptom**: `seed_verified: false` or wrong `q15_denominator`.

**Causes**:
- HKDF implementation mismatch
- Q15 constant modified

**Remediation**:
1. Verify HKDF-SHA256 with BLAKE3 global seed in `crates/adapteros-core/src/seed.rs`
2. Check `Q15_DENOMINATOR` in `crates/adapteros-lora-router/src/constants.rs`

#### cancellation: FAIL

**Symptom**: `token_observed: false` or `cleanup_completed: false`.

**Causes**:
- Worker not checking cancellation token
- Resource cleanup race condition

**Remediation**:
1. Review worker cancellation handling in `crates/adapteros-lora-worker/`
2. Add cancellation checks in long-running operations
3. Verify cleanup order in drop implementations

#### latency_sanity: FAIL

**Symptom**: `router_p99_ms` exceeds `threshold_ms`.

**Causes**:
- Cold cache on first run
- System under resource pressure
- Suboptimal router implementation

**Remediation**:
1. Run warmup iterations before measurement
2. Check for background processes consuming resources
3. Profile router with `AOS_PROFILE=1` for hotspots

## Gate Coverage

The prod gate suite covers the following areas:

1. Routing correctness
   - Verifies deterministic tie-breaking in router selections.
2. Base model residency
   - Verifies base models are pinned and not evicted under cache pressure.
3. Adapter integrity
   - Verifies adapter stack hashes match canonical stack hashing.
4. Generation parity
   - Verifies deterministic generation output stability across repeated runs.
5. Determinism envelope
   - Verifies seed derivation determinism and Q15 denominator invariant.
6. Cancellation
   - Verifies request cancellation tokens are set and observed.
7. Latency sanity checks
   - Verifies router decisions complete under a bounded time threshold.

## CI

CI job: `prod-gate` (runs the gate command and uploads the report artifact).

## Notes

- The prod gate suite is implemented in `crates/adapteros-e2e/tests/prod_gate.rs`.
- The gate is intentionally deterministic and does not require network access.
