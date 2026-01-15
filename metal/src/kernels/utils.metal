// adapterOS Kernel Utilities
// Shared utility functions for all Metal kernels
//
// Features:
// - Deterministic math operations
// - LoRA helper functions
// - Memory access optimizations
// - Q15 format conversions
//
// References:
// - Metal Shading Language: https://developer.apple.com/metal/Metal-Shading-Language-Specification.pdf

// Disable fast-math and force IEEE 754 compliance for determinism
#pragma clang fp contract(off)

#include <metal_stdlib>
using namespace metal;

#include "common.metal"

// Utility functions for kernel computations
// Note: LoRA and dropout functions are defined in common.metal

// Utility function for RoPE (Rotary Position Embedding)
// Note: apply_rope_2d is defined in common.metal

// Utility function for memory-efficient matrix multiplication
float matrix_multiply_element(
    device const float* a,
    device const float* b,
    uint a_row,
    uint a_col,
    uint b_row,
    uint b_col,
    uint a_stride,
    uint b_stride
) {
    float result = 0.0f;
    for (uint k = 0; k < a_col; k++) {
        result += a[a_row * a_stride + k] * b[k * b_stride + b_col];
    }
    return result;
}

// Utility function for attention scaling
// Note: compute_attention_scale is defined in common.metal
