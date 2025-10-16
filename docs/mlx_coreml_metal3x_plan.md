# MLX/CoreML/Metal 3.x Integration Plan

This document captures the high-level design and outstanding engineering work
required to deliver the "Advanced Metal Features" milestone. The implementation
is intentionally deferred because the requested scope crosses several crates
that do not yet exist in this repository (for example `adapteros-lora-kernel-mlx`).
Documenting the gaps prevents accidental partial implementations that would
violate the deterministic execution guarantees described in
`DETERMINISM_LOOP_PROGRESS.md`.

## Current blockers

1. **Missing crates** – The repository does not contain the MLX and CoreML
   kernel crates that the prompt references. Creating them ad-hoc would break
   workspace layout conventions and make future merges hazardous.
2. **PyO3 toolchain drift** – The minimum supported Rust version in
   `rust-toolchain.toml` is 1.74, while the PyO3 version that supports MLX
   requires 1.76+. Updating the toolchain would ripple through CI. A coordinated
   upgrade plan is required before any binding code can land.
3. **Metal 3.x SDK availability** – The current CI containers run on Linux
   without access to Apple's proprietary SDKs. Implementing and testing advanced
   Metal features needs macOS builders.

## Proposed staged approach

### Stage 1: API scaffolding
- Add feature flags to `adapteros-lora-kernel-api` describing MLX/CoreML.
- Introduce trait shims that abstract tensor/memory management semantics.
- Define telemetry payloads for backend health reporting.

### Stage 2: Toolchain preparation
- Bump the workspace toolchain to Rust 1.76 (minimum for PyO3 0.20).
- Add a `pyo3-build-config` crate to centralise Python discovery logic.
- Provide build scripts that short-circuit gracefully on non-macOS targets.

### Stage 3: Backend implementations
- Implement MLX backend using the out-of-tree C FFI in
  `adapteros-lora-mlx-ffi` (already present) and expose deterministic batch
  inference.
- Integrate CoreML by wrapping `coremltools`-generated `.mlmodel` artifacts
  through Objective-C++ shims compiled with `cxx`.
- Extend `adapteros-lora-worker` with conversion pipelines that reuse the
  existing ONNX tooling in `adapteros-lora-quant`.

### Stage 4: Advanced Metal 3.x features
- Introduce a `metal3x` module gated behind the `metal_advanced` feature.
- Provide runtime capability detection to ensure dynamic memory allocation only
  executes on supported GPUs.
- Layer telemetry counters into `adapteros-system-metrics` to monitor memory
  pressure and barrier stalls.

### Stage 5: Testing and validation
- Stand up macOS CI runners for deterministic smoke tests.
- Add criterion benchmarks that compare CPU vs MLX/CoreML throughput.
- Capture reproducibility artefacts per `DETERMINISM_LOOP_PROGRESS.md`.

## Next steps
- Socialise this plan with the determinism working group.
- Secure macOS build capacity.
- Create tracking issues for each stage.

This document should be referenced before any code is merged to ensure the
implementation adheres to AdapterOS determinism guarantees.
