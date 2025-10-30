# Memory-Parallel Low-Rank Adaptation (MPLoRA): Deterministic Multi-Adapter Inference

Document type: Draft patent application (US/EPO-ready structure)  
Date: 2025-10-30  
Version: 1.0

---

## Technical Field
Computing; machine learning; parameter-efficient fine-tuning and deterministic serving of large neural networks using multiple low-rank adapters.

## Background
Low-Rank Adaptation (LoRA) augments a base model with rank-decomposed matrices per adapted layer. In multi-adapter settings, conventional approaches maintain separate down-projection `A` and up-projection `B` per adapter, scaling memory as O(N × rank × hidden_dim) for N adapters and complicating deterministic routing and execution. MPLoRA addresses memory, routing bandwidth, and determinism constraints through a shared downsample architecture, deterministic ring buffer propagation, inference-time entropy flooring, orthogonal constraint enforcement, and an attested deterministic runtime .

## Summary
MPLoRA introduces: (i) a shared downsample (A) matrix reused across adapters with adapter-specific up-projections (B); (ii) a CPU↔GPU shared ring buffer carrying top‑K adapter indices and Q15‑quantized gate weights with deterministic ordering; (iii) an inference‑time entropy floor enforcing minimum gate values to prevent single‑adapter collapse; (iv) orthogonal constraint enforcement to discourage redundant adapter co‑activations; and (v) a determinism attestation framework (kernel hash, FP mode, RNG seeding, compiler flags) integrated with a deterministic executor .

---

## Brief Description of the Drawings
- Fig. 1 (System): Router → Ring Buffer (Q15 gates) → Metal Kernels (MPLoRA) → Adapter Registry .
- Fig. 2 (Shared downsample): Shared `A` computes a bottleneck reused by adapter‑specific `B` with gated accumulation .
- Fig. 3 (Ring buffer): Fixed‑size ring with adapter indices and Q15 gates; deterministic update and device dispatch .
- Fig. 4 (Entropy floor): Softmax → min‑clamp ε/K → renormalize; entropy ≥ ε .
- Fig. 5 (Orthogonal constraints): Sliding history + cosine similarity with penalties above threshold .
- Fig. 6 (Determinism attestation): Metallib hash, FP mode, RNG seeding, compiler flags; validation pipeline .

---

## Detailed Description

### 1) Shared Downsample Matrix Architecture (Core Novelty)
Compute a shared bottleneck representation with a single down‑projection matrix `A` applied to activations. Each adapter applies its up‑projection `B` to the shared bottleneck, scaled by a gate, with deterministic accumulation on device. The Metal kernel exposes shared rank, adapter_count, and compression parameters; gates are Q15 to reduce bandwidth . The MTL backend binds ring indices, Q15 gates, shared buffers, and dispatches the kernel in a deterministic sequence . Memory is reduced vs per‑adapter A+B (≈50% at scale) .

### 2) Ring Buffer Decision Propagation (Deterministic, Q15)
A ring buffer structure (up to K=8) stores adapter indices and Q15 gate values; a raw GPU‑parameter struct mirrors this for zero‑copy device access . Updates advance a position counter deterministically and write into a shared Metal buffer for kernels to consume . The kernel API’s `RouterRing` provides a portable equivalent .

### 3) Entropy Floor at Inference Time
After softmax gating of top‑K adapters, a minimum gate ε/K is enforced followed by renormalization, guaranteeing an entropy bound and preventing single‑adapter collapse without retraining . A strong variant is provided in pluggable scoring .

### 4) Orthogonal Constraint Enforcement
A sliding‑window cosine‑similarity penalty discourages redundant co‑activations; implemented on CPU with GPU kernel design . The tracker computes penalties and maintains history to promote diversity. A GPU kernel design mirrors the approach in Metal for acceleration .

### 5) Determinism Attestation and Execution
Before serving, the backend produces and validates a determinism attestation report covering backend type, metallib hash, RNG seeding, FP mode, and compiler flags; validation fails closed if any non‑deterministic element is detected . Metal kernels disable fast‑math and force IEEE‑754 compliance , and end‑to‑end determinism is documented .

### 6) Hot‑Swap Adapter Management (Optional)
Fused kernel trait exposes adapter load/unload for atomic updates without restart, enabling continuous deployment; Metal implementation uses shared memory buffers .

### 7) Optional Compression/Decompression
Compression and decompression kernels reduce bandwidth around the shared bottleneck; parameterized by compression ratio and integrated in the fused path .

---

## Claims

### Independent Claims
1. A computer‑implemented method for multi‑adapter parameter‑efficient fine‑tuning, comprising: computing, by a processor or GPU, a shared bottleneck representation by applying a shared down‑projection matrix to a model activation; for each of a plurality of adapters, applying an adapter‑specific up‑projection to the shared bottleneck and weighting by a gate value; and accumulating weighted adapter outputs to form a layer delta; wherein gate values are produced deterministically and propagated to device via a shared memory structure, and wherein the down‑projection matrix is shared across the plurality of adapters while the up‑projection matrices remain adapter‑specific .

2. A system comprising: memory storing a shared down‑projection matrix and adapter‑specific up‑projection matrices for a plurality of adapters; a ring buffer comprising adapter indices and quantized gate values in Q15 format shared between a router and a GPU kernel; and a GPU configured to deterministically execute a kernel that consumes the ring buffer, applies the shared down‑projection, applies respective up‑projections and gate values per adapter, and accumulates outputs; wherein the system enforces an inference‑time entropy floor of gate values and validates determinism via kernel binary attestation and floating‑point compliance .

3. A non‑transitory computer‑readable medium storing instructions that, when executed, cause one or more processors to perform the method of claim 1, including receiving deterministic routing decisions in a shared memory ring buffer with Q15 gates, enforcing an entropy floor at inference time, and verifying determinism attestation before serving .

### Dependent Claims
4. The method of claim 1, wherein gate values are quantized to 16‑bit signed integers in Q15 format prior to device propagation .

5. The method of claim 1, wherein the shared memory structure is a fixed‑size ring buffer storing up to K adapter indices and gate values with a monotonically advancing position counter .

6. The system of claim 2, wherein an entropy floor sets a minimum gate of ε/K followed by renormalization to maintain probability mass 1.0, guaranteeing Shannon entropy ≥ ε .

7. The system of claim 2 further comprising an orthogonal constraint module computing cosine similarity over a sliding history window and applying penalties above a threshold to reduce redundant adapter selections .

8. The system of claim 2 wherein determinism is attested by validating a kernel binary hash, enforcing deterministic floating‑point mode, deterministic RNG seeding, and absence of forbidden compiler flags .

9. The method of claim 1 wherein sharing the down‑projection reduces memory by approximately 50% compared to per‑adapter down‑ and up‑projections at fixed rank and hidden dimension for large N .

10. The system of claim 2 further comprising adapter hot‑swap interfaces to atomically load and unload adapter weights during serving without restart .

11. The system of claim 2 wherein compression and decompression kernels are applied to reduce bandwidth with a configurable compression ratio around the shared bottleneck .

---

## Advantages Over Prior Art
- Shared downsample `A` with adapter‑specific `B` yields substantial memory reduction enabling large N multi‑adapter deployments .
- Deterministic ring buffer with Q15 gates and entropy‑floor routing are not taught by LoRA/MoRA/AdaLoRA and provide production‑grade reproducibility and diversity guarantees .
- Orthogonal constraints reduce redundant compute with a GPU‑amenable design .

## Enablement and Best Mode
The shared downsample kernel, ring buffer, entropy floor, orthogonal constraints (CPU, GPU design), and determinism attestation are implemented or designed with explicit APIs and tests across the codebase . Best mode uses Metal kernels with fast‑math disabled, attested metallib binaries, and deterministic executor policies ; end‑to‑end guidance is documented for parity and golden runs .

## Prior Art Comparison (selected)
| Feature | LoRA | MoRA | AdaLoRA | MPLoRA |
|---|---|---|---|---|
| Shared downsample A across adapters | ❌ | ❌ | ❌ | ✅ |
| Deterministic ring buffer with Q15 | ❌ | ❌ | ❌ | ✅ |
| Entropy floor at inference time | ❌ | ❌ | ❌ | ✅ |
| Orthogonal constraints (GPU design) | ❌ | ❌ | ❌ | ✅ |
| Determinism attestation | ❌ | ❌ | ❌ | ✅ |

## Risks and Validation Tests
- Closest prior art might implicitly suggest shared `A` under different nomenclature.  
  Test: Build limitation charts versus LoRA/MoRA/AdaLoRA; independently map terms; reconcile gaps.
- Determinism claims depend on production flags and binaries.  
  Test: Capture attestation artifacts (hash, FP flags) at runtime; re‑run golden runs twice for bit‑exact outputs .
- Entropy floor construed as routine post‑processing.  
  Test: Publish ablations showing stability gains and entropy bounds across datasets/hardware without retraining .

---

## References
1) metal/src/kernels/mplora.metal  
2) docs/PATENT_MPLORA_NOVELTY.md  
3) docs/PATENT_MPLORA_ARCHITECTURE.md  
4) crates/adapteros-lora-kernel-api/src/lib.rs  
5) crates/adapteros-lora-router/src/lib.rs  
6) crates/adapteros-lora-router/src/orthogonal.rs  
7) crates/adapteros-lora-kernel-api/src/attestation.rs  
8) metal/aos_kernels.metal  
9) crates/adapteros-lora-kernel-mtl/src/ring_buffer.rs  
10) crates/adapteros-lora-kernel-mtl/src/mplora.rs  
11) crates/adapteros-lora-router/src/scoring.rs  
12) docs/mplora-e2e.md


