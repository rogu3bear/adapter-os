# Metal Kernel Implementation (Phase 4)

Status: Maintained. This document summarizes the Metal kernel components and where they integrate. For full model integration context, see MLX Integration and Precision Diagrams.

Related docs:
- MLX Integration: ../MLX_INTEGRATION.md
- Precision Architecture Diagrams: ../architecture/precision-diagrams.md
- System Architecture: ../architecture.md

## Overview

AdapterOS uses precompiled Metal kernels (metallib) for deterministic, high-performance operations on Apple Silicon.

Goals:
- Deterministic kernel behavior and fixed rounding
- Precompiled metallib artifacts for reproducibility
- Support for LoRA application and fused attention/MLP ops

## Kernels

- Fused attention with LoRA parameters
- SwiGLU MLP with LoRA application
- Quantization helpers (Q15 gates, int4/int8 weight paths)

Example (illustrative only):

```metal
kernel void fused_attention_lora(
    constant AttentionParams& params,
    device float* Q,
    device float* K,
    device float* V,
    device float* lora_A,
    device float* lora_B
) {
    // Deterministic execution with fixed rounding
    // Precompiled to .metallib for reproducibility
}
```

## Determinism

- Precompiled metallib shipped with release
- Canonical parameter order and fixed math paths
- Kernel hashes included in plans for verification

## Integration Points

- Worker runtime: adapteros-lora-kernel-mtl
- Router: Q15 gate quantization inputs to kernels
- Telemetry: trace kernel noise and timing for audits

See also: ../TESTING_MODEL_LOADING.md and ../kernel-weight-loading-determinism.md

