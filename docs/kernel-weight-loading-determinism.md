# Kernel Weight-Loading Determinism

AdapterOS enforces deterministic weight loading for the Metal backend so that identical deployment plans always produce the same GPU state. This note documents the code paths and invariants that guarantee reproducibility.

## Weight Loading Path

- **Entry Point**: `FusedKernels::load(plan_bytes)` in `crates/adapteros-lora-kernel-mtl/src/lib.rs:1020` seeds the session with `plan_bytes`, initializes the Metal library, and orchestrates weight transfers.
- **Embedding Weights**: `parse_embedding_weights`, `create_embedding_buffer`, and `validate_embedding_dimensions` (lib.rs) extract the embedding matrix from the plan and upload it to deterministic Metal buffers.
- **Transformer Weights**: `parse_transformer_weights` and `load_transformer_weights` deserialize layer weights in fixed order and stream them into pre-sized buffers.
- **LM Head Weights**: `parse_lm_head_weights` finalizes the vocabulary projection tensors, keeping ordering stable across runs.
- **Plan Hashing**: `plan_hash = B3Hash::hash(plan_bytes)` followed by `StdRng::from_seed(plan_seed)` derives the deterministic RNG used for adapter mixing and auxiliary buffers (lib.rs:1026-1034).

## Determinism Guarantees

1. **Kernel Hash Verification (`lib.rs:317-360`)**  
   The embedded `adapteros_kernels.metallib` bytes are hashed at runtime and compared against the compile-time `METALLIB_HASH` constant. Any mismatch triggers `AosError::DeterminismViolation`, preventing execution with stale or tampered kernels.

2. **Plan-Derived Seeding (`lib.rs:1026-1034`)**  
   `plan_hash = B3Hash::hash(plan_bytes)` followed by `plan_seed = plan_hash.to_bytes()` seeds `StdRng::from_seed(plan_seed)`. Because the seed is a pure function of `plan_bytes`, identical plans always reconstruct the same weights without relying on system entropy or clocks.

3. **Adapter Fusion (`lib.rs:52-139`)**  
   Adapter deltas call `derive_adapter_seed(plan_seed, adapter_id)` which uses deterministic XOR, wrapping addition, and bit rotations before seeding `StdRng`. Cached deltas ensure that subsequent invocations reuse the same buffers, guaranteeing stable adapter fusion outputs.

4. **Floating-Point Mode (`metal/src/kernels/adapteros_kernels.metal:20`)**  
   The unified kernel file enforces `#pragma clang fp contract(off)`, disabling fast-math contractions so the compiler emits IEEE 754-compliant instructions. This removes platform-dependent floating-point variability.

## Cross-Reference

For a broader determinism overview see `docs/determinism-attestation.md`, specifically the "Metal Backend (Deterministic)" section, which now references this document.
