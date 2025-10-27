# Golden Runs Directory

This directory contains golden-run archives for audit reproducibility verification.

## Purpose

Golden runs serve as cryptographically signed reference baselines for deterministic execution. They enable:

- **Audit reproducibility**: Verify that inference runs can be exactly reproduced
- **Regression detection**: Catch unintended changes in output or numerical stability
- **Compliance verification**: Provide evidence of deterministic execution for audits

## Directory Structure

```
golden_runs/
├── baselines/          # Active golden run baselines
│   ├── baseline-001/   # Individual baseline directory
│   │   ├── manifest.json          # Run metadata (CPID, Plan, toolchain, adapters)
│   │   ├── epsilon_stats.json     # Per-layer ε statistics
│   │   ├── bundle_hash.txt        # BLAKE3 hash of event bundle
│   │   ├── signature.sig          # Ed25519 signature (optional)
│   │   └── event_bundle.ndjson    # Complete event trace (optional, can be large)
│   └── baseline-002/
└── archive/            # Archived golden runs (historical reference)
```

## File Descriptions

### manifest.json

Contains metadata about the golden run:

- `run_id`: Unique identifier for this golden run
- `cpid`: Control Plane ID
- `plan_id`: Plan ID
- `created_at`: Timestamp when golden run was created
- `toolchain`: Toolchain information (rustc version, Metal version, kernel hash)
- `adapters`: List of adapter IDs used in the run
- `device`: Device fingerprint (model, OS version, Metal family)
- `global_seed`: BLAKE3 hash used as global seed for deterministic execution

### epsilon_stats.json

Per-layer floating-point error statistics:

- `layer_stats`: Map of layer ID to epsilon statistics
  - `l2_error`: L2 norm of error vector
  - `max_error`: Maximum absolute error
  - `mean_error`: Mean absolute error
  - `element_count`: Number of elements in the layer

Note: When multiple adapters are active, layer IDs may be prefixed with `adapter:<adapter_id>/` to tag per-adapter epsilon (for example: `adapter:a1/ff.gate`). The file schema remains a string key mapped to stats.

### bundle_hash.txt

BLAKE3 hash of the complete event bundle (NDJSON format). This enables verification without storing the full event trace.

### signature.sig

Ed25519 signature over the golden run archive (hex-encoded). Provides cryptographic proof of authenticity.

### event_bundle.ndjson (optional)

Complete event trace from the inference run. Can be omitted to save space if only hash verification is needed.

## Creating a Golden Run

```bash
# Run inference and capture event bundle
aosctl serve --plan-id <plan> --capture-events var/bundles/capture.ndjson

# Create golden run from bundle
aosctl golden create \
  --bundle var/bundles/capture.ndjson \
  --name baseline-001 \
  --sign

# Golden run saved to golden_runs/baselines/baseline-001/
```

## Verifying Against a Golden Run

```bash
# Run inference and capture events
aosctl serve --plan-id <plan> --capture-events var/bundles/new_run.ndjson

# Verify against golden baseline
aosctl golden verify \
  --golden golden_runs/baselines/baseline-001 \
  --bundle var/bundles/new_run.ndjson

# Output:
# ✓ Verification PASSED
#   Bundle hash: ✓ match
#   Signature: ✓ verified
#   Toolchain: ✓ compatible
#   Adapters: ✓ match
#   Epsilon: ✓ All 32 layers within tolerance (ε < 1.00e-06)
```

## Server API Usage

The server exposes endpoints to list baselines, inspect a baseline summary, and compare a telemetry bundle to a chosen baseline.

Endpoints:

- `GET /v1/golden/runs` → `string[]` of baseline names (from `golden_runs/baselines/*`).
- `GET /v1/golden/runs/:name` → GoldenRunSummary (metadata and ε stats summary).
- `POST /v1/golden/compare` → VerificationReport for a bundle against a baseline.

Example compare request body:

```json
{
  "golden": "baseline-001",
  "bundle_id": "f2b1c3e0",
  "strictness": "epsilon-tolerant",
  "verify_toolchain": true,
  "verify_adapters": true,
  "verify_signature": true,
  "verify_device": false
}
```

Bundle is resolved at `var/bundles/{bundle_id}.ndjson`. Defaults enforce the "golden rules": strictness `epsilon-tolerant`; verify toolchain, adapters, and signature; device verification is optional.

## Verification Strictness Levels

- **Bitwise**: Bit-for-bit identical (ε = 0), no floating-point tolerance
- **EpsilonTolerant**: Default tolerance for floating-point (ε < 1e-6)
- **Statistical**: Relaxed tolerance for sampling variance (ε < 1e-4)

Set strictness with `--strictness` flag:

```bash
aosctl golden verify \
  --golden golden_runs/baselines/baseline-001 \
  --bundle var/bundles/new_run.ndjson \
  --strictness bitwise
```

## Policy Integration

Golden runs integrate with AdapterOS policy packs:

- **Build & Release Ruleset (15)**: Requires zero-diff replay on fixed prompt corpus
- **Compliance Ruleset (16)**: Maps controls to evidence (golden runs as proof)
- **Determinism Ruleset (2)**: Validates kernel hashes and RNG seeding
- **Evidence Ruleset (4)**: Golden runs serve as evidence for audit trail

## Best Practices

1. **Create golden runs at each CP promotion**: Capture baseline for new control plane
2. **Sign all golden runs**: Use Ed25519 signatures for audit trail
3. **Archive old baselines**: Move superseded baselines to `archive/` directory
4. **Verify before deployment**: Run golden verification in CI/CD pipeline
5. **Document divergences**: Track expected epsilon ranges per layer

## Retention Policy

Per **Retention Ruleset (10)**:

- Keep last 12 golden runs per CPID
- Keep all golden runs referenced by open incidents
- Keep at least one "promotion golden run" per CP promotion

## Troubleshooting

### Signature verification failed

```bash
# Re-sign golden run with current keypair
aosctl golden sign golden_runs/baselines/baseline-001
```

### Epsilon divergence detected

```bash
# View detailed epsilon statistics
aosctl golden compare \
  --golden golden_runs/baselines/baseline-001 \
  --bundle var/bundles/new_run.ndjson \
  --show-layers
```

### Toolchain mismatch

Golden run was created with different toolchain. Either:
- Use matching toolchain version
- Create new golden run with current toolchain
- Use `--skip-toolchain-check` flag (not recommended for audit)

## See Also

- [docs/determinism-audit.md](../docs/determinism-audit.md) - Determinism verification guide
- [docs/control-plane.md](../docs/control-plane.md) - Control plane documentation
- [.cursor/rules/global.mdc](../.cursor/rules/global.mdc) - Policy rulesets

## CAB Golden Gate

Control Plane promotion can optionally run a golden-run verification gate between replay tests and approval signing. Enable via `configs/cp.toml`:

```
[cab.golden_gate]
enabled = true
baseline = "baseline-001"
# one of: bitwise | epsilon-tolerant | statistical
strictness = "epsilon-tolerant"
skip_toolchain = false
skip_signature = false
verify_device = false
# bundle_path = "var/bundles/cp-promote.ndjson" # optional explicit bundle
```

Behavior:
- When enabled, the server verifies the current replay bundle against `golden_runs/baselines/<baseline>`.
- If `bundle_path` is not set, the newest `*.ndjson` under `paths.bundles_root` is used.
- Strictness controls epsilon tolerance; bitwise requires identical bundles.
- Any failure blocks promotion.
