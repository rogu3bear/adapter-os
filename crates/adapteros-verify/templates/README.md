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
│   │   ├── routing_decisions.json # Per-step router decisions for deterministic verification
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

### bundle_hash.txt

BLAKE3 hash of the complete event bundle (NDJSON format). This enables verification without storing the full event trace.

### signature.sig

Ed25519 signature over the golden run archive (hex-encoded). Provides cryptographic proof of authenticity.

### routing_decisions.json

Per-step routing decisions captured during inference:

- Records which adapters were selected at each token generation step
- Includes Q15 quantized gate values for deterministic verification
- Enables replay verification to confirm exactly which adapters fired
- Used by verification to detect routing divergences

This file enables **full deterministic replay** including adapter selection, not just final output verification.

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

Golden runs integrate with adapterOS policy packs:

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

- [docs/determinism-audit.md](../../../docs/determinism-audit.md) - Determinism verification guide
- [docs/control-plane.md](../../../docs/control-plane.md) - Control plane documentation
- [.cursor/rules/global.mdc](../../.cursor/rules/global.mdc) - Policy rulesets
