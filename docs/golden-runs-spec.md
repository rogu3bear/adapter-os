# Golden Runs Directory Specification

## Overview

The `golden_runs/` directory provides a formalized archive system for storing and verifying deterministic inference runs. It enables audit reproducibility by capturing reference baselines with signed metadata, epsilon statistics, and cryptographic hashes.

## Purpose

Golden runs serve three primary functions:

1. **Audit Reproducibility**: Verify that inference runs can be exactly reproduced with identical outputs
2. **Regression Detection**: Catch unintended changes in numerical outputs or execution behavior
3. **Compliance Evidence**: Provide cryptographically signed proof of deterministic execution for audits

## Directory Structure

```
golden_runs/
├── README.md              # Documentation (auto-generated)
├── .gitignore             # Ignore large event bundles
├── baselines/             # Active golden run baselines
│   ├── baseline-001/
│   │   ├── manifest.json          # Run metadata
│   │   ├── epsilon_stats.json     # Per-layer ε statistics
│   │   ├── bundle_hash.txt        # BLAKE3 hash of event bundle
│   │   ├── signature.sig          # Ed25519 signature (hex-encoded)
│   │   └── event_bundle.ndjson    # Complete event trace (optional)
│   ├── baseline-002/
│   └── ...
└── archive/               # Archived golden runs (historical reference)
    ├── archived-001/
    └── ...
```

## File Formats

### manifest.json

Complete metadata about the golden run:

```json
{
  "run_id": "golden-test-cpid-20241013-143022",
  "cpid": "test-cpid",
  "plan_id": "test-plan-001",
  "created_at": "2024-10-13T14:30:22.123456Z",
  "toolchain": {
    "rustc_version": "1.75.0",
    "metal_version": "3.1",
    "kernel_hash": "b3:abc123..."
  },
  "adapters": [
    "adapter-001",
    "adapter-002"
  ],
  "device": {
    "device_model": "MacBookPro18,3",
    "os_version": "14.0",
    "metal_family": "Apple9"
  },
  "global_seed": "b3:def456..."
}
```

**Fields:**
- `run_id`: Unique identifier (auto-generated: `golden-{cpid}-{timestamp}`)
- `cpid`: Control Plane ID
- `plan_id`: Plan ID used for this run
- `created_at`: ISO 8601 timestamp (UTC)
- `toolchain`: Compiler and kernel versions
  - `rustc_version`: Rust compiler version
  - `metal_version`: Metal shader compiler version
  - `kernel_hash`: BLAKE3 hash of compiled Metal kernels
- `adapters`: List of adapter IDs used in the run
- `device`: Device fingerprint
  - `device_model`: Hardware model
  - `os_version`: Operating system version
  - `metal_family`: GPU family (e.g., "Apple9")
- `global_seed`: BLAKE3 hash used as global seed for RNG

### epsilon_stats.json

Per-layer floating-point error statistics:

```json
{
  "layer_stats": {
    "layer_0": {
      "l2_error": 1.23e-7,
      "max_error": 5.67e-7,
      "mean_error": 2.34e-7,
      "element_count": 1000
    },
    "layer_1": {
      "l2_error": 8.90e-8,
      "max_error": 3.21e-7,
      "mean_error": 1.56e-7,
      "element_count": 2000
    }
  }
}
```

**Fields:**
- `layer_stats`: Map of layer ID to epsilon statistics
  - `l2_error`: L2 norm of the error vector
  - `max_error`: Maximum absolute error
  - `mean_error`: Mean absolute error
  - `element_count`: Number of elements in the layer

These statistics are extracted from `kernel.noise` telemetry events during inference.

Note: For runs with multiple adapters, layer IDs can be adapter‑qualified using the prefix `adapter:<adapter_id>/`. Example: `adapter:a1/ff.gate`. This tags ε per adapter without changing the file schema (string key → stats).

### bundle_hash.txt

BLAKE3 hash of the complete event bundle (plain text):

```
b3:1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef
```

This enables verification without storing the full event trace.

### signature.sig

Ed25519 signature over the golden run archive (hex-encoded, plain text):

```
0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
```

Signature is computed over the serialized `manifest.json` and `epsilon_stats.json` (excluding the signature field itself).

### event_bundle.ndjson (optional)

Complete event trace in newline-delimited JSON format. Can be omitted to save space if only hash verification is needed.

```
{"event_type":"inference.start","timestamp":1000,...}
{"event_type":"kernel.noise","timestamp":1001,...}
{"event_type":"inference.token","timestamp":1002,...}
```

## CLI Commands

### Initialize Directory

```bash
aosctl golden init
```

Creates the `golden_runs/` directory structure with README and .gitignore.

### Create Golden Run

```bash
aosctl golden create \
  --bundle var/bundles/baseline.ndjson \
  --name baseline-001 \
  --adapters adapter-001,adapter-002 \
  --sign
```

**Options:**
- `--bundle`: Path to replay bundle (NDJSON format)
- `--name`: Name for the golden run (becomes directory name)
- `--toolchain`: Toolchain version (defaults to current)
- `--adapters`: Comma-separated list of adapter IDs
- `--sign`: Sign the golden run with Ed25519

**Output:**
```
Creating golden run from bundle: var/bundles/baseline.ndjson
✓ Extracted epsilon stats: 32 layers, max_ε=1.234e-06
✓ Signed golden run
✓ Golden run created: baseline-001
  Location: golden_runs/baselines/baseline-001
```

### List Golden Runs

```bash
aosctl golden list
```

**Output:**
```
Available golden runs:
baseline-001
  CPID: test-cpid
  Plan: test-plan-001
  Created: 2024-10-13 14:30 UTC
  Toolchain: rustc=1.75.0, metal=3.1, kernels=b3:abc123...
  Signed: yes

baseline-002
  CPID: test-cpid
  Plan: test-plan-002
  Created: 2024-10-13 15:45 UTC
  Toolchain: rustc=1.75.0, metal=3.1, kernels=b3:abc123...
  Signed: yes

Total: 2 golden runs
```

### Verify Against Golden Run

```bash
aosctl golden verify \
  --golden baseline-001 \
  --bundle var/bundles/new_run.ndjson
```

**Output (success):**
```
Verifying bundle: var/bundles/new_run.ndjson
Against golden run: baseline-001

✓ Verification PASSED

Golden Run:
  ID: golden-test-cpid-20241013-143022
  CPID: test-cpid
  Plan: test-plan-001
  Toolchain: rustc=1.75.0, metal=3.1, kernels=b3:abc123...

Current Run:
  ID: golden-test-cpid-20241013-154501
  CPID: test-cpid
  Plan: test-plan-001
  Toolchain: rustc=1.75.0, metal=3.1, kernels=b3:abc123...

Verification Results:
  Bundle hash: ✓ match
  Signature: ✓ verified
  Toolchain: ✓ compatible
  Adapters: ✓ match
  Device: ⚠ different

  Epsilon: ✓ All 32 layers within tolerance (ε < 1.00e-06)
```

**Output (failure):**
```
✗ Verification FAILED

Verification Results:
  Bundle hash: ✗ mismatch
  Signature: ✓ verified
  Toolchain: ✓ compatible
  Adapters: ✓ match
  Device: ⚠ different

  Epsilon: ✗ Epsilon verification failed: 3 divergent layers

  Divergent layers:
    layer_5: rel_error=2.34e-05 (golden: l2=1.23e-06, current: l2=1.52e-06)
    layer_12: rel_error=1.87e-05 (golden: l2=8.90e-07, current: l2=1.07e-06)
    layer_23: rel_error=3.12e-05 (golden: l2=5.67e-07, current: l2=7.45e-07)

Messages:
  Verification failed: epsilon verification failed
```

### Show Golden Run Details

```bash
aosctl golden show baseline-001
```

**Output:**
```
Golden Run: golden-test-cpid-20241013-143022
  CPID: test-cpid
  Plan: test-plan-001
  Toolchain: rustc=1.75.0, metal=3.1, kernels=b3:abc123...
  Adapters: adapter-001, adapter-002
  Device: MacBookPro18,3 (OS 14.0, Metal Apple9)
  Created: 2024-10-13 14:30:22 UTC

Epsilon Statistics:
  Layers: 32
  Max epsilon: 1.234e-06
  Mean epsilon: 3.456e-07

Bundle Hash: b3:1234567890abcdef...
Signed: yes
```

## Verification Strictness Levels

Golden run verification supports three strictness levels:

### 1. Bitwise (ε = 0)

Bit-for-bit identical outputs. No floating-point tolerance.

```bash
aosctl golden verify --golden baseline-001 --bundle new_run.ndjson --strictness bitwise
```

**Use case:** Final compliance verification, regression testing

### 2. Epsilon-Tolerant (ε < 1e-6, default)

Allows small floating-point differences within epsilon tolerance.

```bash
aosctl golden verify --golden baseline-001 --bundle new_run.ndjson --strictness epsilon-tolerant
```

**Use case:** Standard verification, accounts for non-deterministic floating-point operations

### 3. Statistical (ε < 1e-4)

Relaxed tolerance for statistical sampling variance.

```bash
aosctl golden verify --golden baseline-001 --bundle new_run.ndjson --strictness statistical
```

**Use case:** Sampling-based inference, Monte Carlo methods

## Verification Options

### Skip Toolchain Check

Allow verification across different toolchain versions:

```bash
aosctl golden verify --golden baseline-001 --bundle new_run.ndjson --skip-toolchain
```

**Warning:** May produce false positives if kernels differ.

### Skip Signature Check

Skip signature verification (useful for unsigned golden runs):

```bash
aosctl golden verify --golden baseline-001 --bundle new_run.ndjson --skip-signature
```

**Warning:** Not recommended for audit compliance.

## Policy Integration

Golden runs integrate with AdapterOS policy packs:

### Determinism Ruleset (2)

- **Requirement:** Precompiled Metal kernels, HKDF seeding, deterministic retrieval ordering
- **Verification:** Golden run captures kernel hash and seed, verifies identical execution

### Build & Release Ruleset (15)

- **Requirement:** Zero-diff replay on fixed prompt corpus
- **Verification:** Golden run serves as reference for CI/CD gates

### Compliance Ruleset (16)

- **Requirement:** Control matrix mapping to evidence files
- **Verification:** Golden runs provide cryptographic evidence for audits

### Retention Ruleset (10)

- **Requirement:** Keep last 12 golden runs per CPID, keep promotion golden runs
- **Implementation:** Archive old baselines to `archive/` directory

## Workflow Examples

### Promote Control Plane with Golden Run

```bash
# 1. Run inference with new CP
aosctl serve --plan-id new-cp-001 --capture-events var/bundles/cp-001.ndjson

# 2. Create golden run
aosctl golden create --bundle var/bundles/cp-001.ndjson --name cp-001-baseline --sign

# 3. Verify subsequent runs
aosctl serve --plan-id new-cp-001 --capture-events var/bundles/verify.ndjson
aosctl golden verify --golden cp-001-baseline --bundle var/bundles/verify.ndjson

# 4. If verification passes, promote CP
aosctl rollback --tenant prod CP-001
```

### CI/CD Integration

```yaml
# .github/workflows/verify-determinism.yml
name: Verify Determinism

on: [pull_request]

jobs:
  verify:
    runs-on: macos-14
    steps:
      - uses: actions/checkout@v4
      - name: Build project
        run: cargo build --release
      - name: Run inference
        run: ./target/release/aosctl serve --plan-id test --capture-events test.ndjson
      - name: Verify against golden run
        run: ./target/release/aosctl golden verify --golden baseline-main --bundle test.ndjson --strictness bitwise
```

### Regression Testing

```bash
# Create golden run for feature branch
aosctl golden create --bundle feature.ndjson --name feature-baseline

# After changes, verify no regression
aosctl golden verify --golden feature-baseline --bundle updated.ndjson

# If verification fails, investigate divergences
aosctl golden show feature-baseline
```

## Best Practices

1. **Sign all golden runs**: Use `--sign` flag for audit trail
2. **One golden run per CP**: Create baseline at each control plane promotion
3. **Archive old baselines**: Move superseded baselines to `archive/` directory
4. **Document epsilon ranges**: Track expected error bounds per layer
5. **Verify in CI**: Run golden verification in CI/CD pipeline
6. **Keep event bundles**: Store full event traces for critical baselines
7. **Regular cleanup**: Remove unused golden runs per retention policy

## Troubleshooting

### Signature verification failed

Re-sign the golden run:

```bash
aosctl golden sign golden_runs/baselines/baseline-001
```

### Epsilon divergence detected

View detailed layer statistics:

```bash
aosctl golden show baseline-001 --layers
```

Compare epsilon stats between runs:

```bash
aosctl replay diff baseline.ndjson current.ndjson --epsilon-stats
```

### Toolchain mismatch

Either use matching toolchain or skip verification:

```bash
aosctl golden verify --golden baseline-001 --bundle new_run.ndjson --skip-toolchain
```

Or create new golden run with current toolchain:

```bash
aosctl golden create --bundle new_run.ndjson --name baseline-002
```

## See Also

- [docs/determinism-audit.md](./determinism-audit.md) - Determinism verification
- [docs/control-plane.md](./control-plane.md) - Control plane operations
- [.cursor/rules/global.mdc](../.cursor/rules/global.mdc) - Policy rulesets
- [crates/adapteros-verify/README.md](../crates/adapteros-verify/README.md) - Library documentation
