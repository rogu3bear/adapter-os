#include <metal_stdlib>
using namespace metal;

// RMSNorm configuration (must match Rust struct)
struct RmsNormConfig {
    uint hidden_size;
    float eps;
};

// RMSNorm kernel implementation
// Formula: y = (x / sqrt(mean(x^2) + eps)) * weight
kernel void rms_norm(
    device const float* input [[buffer(0)]],
    device const float* weight [[buffer(1)]],
    device float* output [[buffer(2)]],
    constant RmsNormConfig& config [[buffer(3)]],
    uint lid [[thread_position_in_threadgroup]],
    uint tgid [[threadgroup_position_in_grid]],
    uint gsz [[threads_per_threadgroup]]
) {
    uint batch_idx = tgid;
    uint hidden_size = config.hidden_size;
    float eps = config.eps;
    
    // Pointer to current row
    device const float* row_input = input + batch_idx * hidden_size;
    device float* row_output = output + batch_idx * hidden_size;
    
    // Step 1: Compute sum of squares (parallel reduction)
    threadgroup float shared_sq_sum[256];
    float local_sq_sum = 0.0f;
    
    for (uint i = lid; i < hidden_size; i += gsz) {
        float val = row_input[i];
        local_sq_sum += val * val;
    }
    
    shared_sq_sum[lid] = local_sq_sum;
    threadgroup_barrier(mem_flags::mem_threadgroup);
    
    // Reduction in shared memory
    for (uint s = gsz / 2; s > 0; s >>= 1) {
        if (lid < s) {
            shared_sq_sum[lid] += shared_sq_sum[lid + s];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
    
    // Step 2: Compute normalization factor
    float mean_sq = shared_sq_sum[0] / (float)hidden_size;
    float inv_rms = rsqrt(mean_sq + eps);
    
    // Step 3: Apply normalization and weight
    for (uint i = lid; i < hidden_size; i += gsz) {
        row_output[i] = (row_input[i] * inv_rms) * weight[i];
    }
}
