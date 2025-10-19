# MPLoRA End-to-End

This document summarizes the Memory‑Parallel LoRA (MPLoRA) path end‑to‑end on macOS/Metal and how it ties into parity checks and golden‑run verification.

## Pipeline Overview

- Router selects top‑K adapters and quantizes gates to Q15; decisions are written into a ring buffer.
- Fused kernels (QKV, MLP) read the ring buffer and apply LoRA deltas with deterministic math.
- Vocabulary projection mixes adapter contributions and produces logits.
- Noise tracking emits per‑layer `kernel.noise` events; ε statistics are rolled up for golden runs.

Key components:
- `crates/adapteros-lora-kernel-api`: traits (`FusedKernels`, `MploraKernels`), ring `RouterRing` model.
- `crates/adapteros-lora-kernel-mtl`: Metal backend (`MetalKernels`), fused kernels, ring buffer, noise tracker.
- `metal/src/kernels`: MSL kernels; determinism flags are enabled (IEEE‑754 compliant operations).
- `crates/adapteros-verify`: epsilon extraction and golden‑run comparison with strictness levels.

## Determinism

- Metal kernels compile with fast‑math disabled and IEEE‑754 compliance.
- LoRA dropout is disabled in inference paths (`dropout_rate = 0.0`).
- Backend attestation validates metallib hash, toolchain, and deterministic settings.

## Parity Tests (macOS)

- CPU↔Metal LoRA math parity: `crates/adapteros-lora-kernel-mtl/tests/metal_lora_parity.rs` compares a minimal LoRA transform on CPU vs a tiny Metal kernel. ε ≤ 1e‑6 by default.
- Repeatability: the same test suite includes a repeatability check that runs the Metal kernel twice and verifies exact equality.
- Fused MLP smoke: `crates/adapteros-lora-kernel-mtl/tests/fused_mlp_smoke.rs` executes the embedded Metal MLP pipeline with zero weights to exercise buffer setup and dispatch.

Run on macOS with Metal:

```
cargo test -p adapteros-lora-kernel-mtl -- --nocapture
```

## Golden Runs and ε Tagging

- Epsilon stats are derived from `kernel.noise` telemetry. The default strictness is epsilon‑tolerant (1e‑6).
- Per‑adapter tagging: layer IDs may be prefixed with `adapter:<id>/` so ε aggregates per adapter without schema changes. See `golden_runs/README.md` and `docs/golden-runs-spec.md`.

## Backend Selection and Attestation

- On macOS, the worker prefers the Metal backend; if Metal initialization fails for determinism/policy reasons, fallback is refused for explicitly requested backends.
- Attestation must succeed before serving: metallib hash, RNG seeding mode, and IEEE‑754 compliance are validated.

## References

- API: `crates/adapteros-lora-kernel-api/src/lib.rs`
- Metal fused MLP: `crates/adapteros-lora-kernel-mtl/src/fused_mlp.rs`, `metal/src/kernels/mlp.metal`
- Ring buffer: `crates/adapteros-lora-kernel-mtl/src/ring_buffer.rs`, `metal/src/kernels/common.metal`
- Noise tracking: `crates/adapteros-lora-kernel-mtl/src/noise_tracker.rs`
- Strictness and ε: `crates/adapteros-verify/src/lib.rs`, `crates/adapteros-verify/src/epsilon.rs`
