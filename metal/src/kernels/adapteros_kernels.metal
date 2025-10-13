// AdapterOS Modular Metal Kernels
// Production-optimized Metal kernels for Qwen2.5-7B-Instruct
//
// Features:
// - Fused MLP with SwiGLU activation and LoRA support
// - Fused QKV with Grouped Query Attention (GQA)
// - Flash Attention for memory efficiency
// - Deterministic math operations
// - Optimized memory access patterns
//
// References:
// - SwiGLU: https://arxiv.org/abs/2002.05202
// - GQA: https://arxiv.org/abs/2305.13245
// - Flash Attention: https://arxiv.org/abs/2205.14135
// - LoRA: https://arxiv.org/abs/2106.09685
// - Metal Performance Shaders: https://developer.apple.com/documentation/metalperformanceshaders

// Disable fast-math and force IEEE 754 compliance for determinism
#pragma clang fp contract(off)

#include <metal_stdlib>
using namespace metal;

// Include all modular components
#include "common.metal"
#include "utils.metal"
#include "mlp.metal"
#include "attention.metal"
#include "flash_attention.metal"
#include "mplora.metal"
