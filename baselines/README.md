# Baselines

Determinism baseline configurations and manifests for golden run validation.

## Contents

- [`manifest.toml`](manifest.toml) - Baseline test manifest defining expected outputs

## Purpose

This directory stores baseline configurations used in determinism validation. These baselines ensure that the same model input produces identical outputs across runs, which is critical for auditable and reproducible AI.

## Related

- [`golden_runs/`](../golden_runs/) - Determinism verification test results
- [`docs/DETERMINISM.md`](../docs/DETERMINISM.md) - Determinism architecture documentation

## Usage

Baselines are automatically referenced during golden run tests via the `/golden` workflow.
