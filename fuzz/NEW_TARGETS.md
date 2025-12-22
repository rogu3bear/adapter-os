This document describes the new fuzzing targets added to the AdapterOS project.

Four new fuzzing targets have been created to test recently added modules:

1. `evidence_envelope`: Fuzzes the serialization and construction of `EvidenceEnvelope`
2. `evidence_chain_verification`: Fuzzes the verification of evidence chains (Merkle links)
3. `stop_controller`: Fuzzes the token generation stop logic
4. `kv_quota_reservation`: Fuzzes the KV cache quota reservation system
5. `model_cache_eviction`: Fuzzes the Model Handle Cache eviction policies (pinned/active/LRU)

## Prerequisites

To run these fuzz targets, you need `cargo-fuzz` installed:

```bash
cargo install cargo-fuzz
```

## Running the Fuzz Targets

You can run each target individually:

```bash
# Evidence envelope fuzzing
cargo fuzz run evidence_envelope

# Evidence chain verification fuzzing
cargo fuzz run evidence_chain_verification

# Stop controller fuzzing
cargo fuzz run stop_controller

# KV quota reservation fuzzing
cargo fuzz run kv_quota_reservation

# Model cache eviction fuzzing
cargo fuzz run model_cache_eviction
```

## Configuration

You can configure the fuzzing run with standard libfuzzer arguments:

```bash
# Run for a specific time (e.g., 60 seconds)
cargo fuzz run evidence_envelope -- -max_total_time=60

# Run for a specific number of iterations
cargo fuzz run stop_controller -- -runs=10000

# Run with multiple jobs (parallel fuzzing)
cargo fuzz run kv_quota_reservation -- -jobs=4
```

## Corpus Management

The fuzzing corpus (input data) is stored in `fuzz/corpus/$TARGET_NAME/`. You can check this directory into git to preserve interesting test cases.

- `fuzz/corpus/evidence_envelope/`
- `fuzz/corpus/evidence_chain_verification/`
- `fuzz/corpus/stop_controller/`
- `fuzz/corpus/kv_quota_reservation/`
- `fuzz/corpus/model_cache_eviction/`

## CI Integration

These fuzz targets can be integrated into CI pipelines:

```bash
# Example CI step
for target in evidence_envelope evidence_chain_verification stop_controller kv_quota_reservation model_cache_eviction; do
    cargo fuzz run $target -- -max_total_time=30 || exit 1
done
```

## Coverage Reporting

To generate coverage reports:

```bash
cargo fuzz coverage evidence_envelope
cargo cov report fuzz/target/*/coverage
```

## Findings

Document any crashes or hangs found by fuzzing in this section:

*(No crashes found yet)*

## References

- [cargo-fuzz Guide](https://rust-fuzz.github.io/book/cargo-fuzz.html)
