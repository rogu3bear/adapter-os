---
description: Create and verify golden runs for determinism audits
---

# Golden Runs Workflow

## Create Golden Run

### 1. Capture Events
```bash
./aosctl serve --plan <plan> --capture-events var/bundles/capture_dir
```

### 2. Create Signed Baseline
```bash
./aosctl golden create \
  --bundle var/bundles/capture_dir \
  --name baseline-001 \
  --sign
```

## Verify Against Baseline

### 1. Run New Inference
```bash
./aosctl serve --plan <plan> --capture-events var/bundles/new_run
```

### 2. Verify
```bash
./aosctl golden verify \
  --golden golden_runs/baselines/baseline-001 \
  --bundle var/bundles/new_run/bundle_000000.ndjson
```

## Strictness Levels
- `--strictness bitwise` — bit-for-bit (ε = 0)
- `--strictness epsilon` — default (ε < 1e-6)
- `--strictness statistical` — relaxed (ε < 1e-4)

## Compare Layers
```bash
./aosctl golden compare \
  --golden golden_runs/baselines/baseline-001 \
  --bundle var/bundles/new_run.ndjson \
  --show-layers
```

## Re-sign
```bash
./aosctl golden sign golden_runs/baselines/baseline-001
```
