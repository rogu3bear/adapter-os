// adapterOS MLP Kernel
// Fused MLP with SwiGLU activation, LoRA support, and bias
//
// Features:
// - SwiGLU activation (SiLU gate + linear up)
// - LoRA (Low-Rank Adaptation) support
// - Deterministic math operations
// - Optimized memory access patterns
//
// References:
// - SwiGLU: https://arxiv.org/abs/2002.05202
// - LoRA: https://arxiv.org/abs/2106.09685

// Disable fast-math and force IEEE 754 compliance for determinism
#pragma clang fp contract(off)

#include <metal_stdlib>
using namespace metal;

#include "common.metal"

// Function constants for optimization
constant bool has_gate_lora = false;
constant bool has_up_lora = false;
constant bool has_down_lora = false;

// Fused MLP kernel with SwiGLU activation, LoRA support, and bias
kernel void fused_mlp(
    device const float* input [[buffer(0)]],
    device float* output [[buffer(1)]],
    device const float* gate_weight [[buffer(2)]],
    device const float* up_weight [[buffer(3)]],
    device const float* down_weight [[buffer(4)]],
    device const float* gate_bias [[buffer(5)]],
    device const float* up_bias [[buffer(6)]],
    device const float* down_bias [[buffer(7)]],
    device const float* gate_lora_a [[buffer(8)]],
    device const float* gate_lora_b [[buffer(9)]],
    device const float* up_lora_a [[buffer(10)]],
    device const float* up_lora_b [[buffer(11)]],
    device const float* down_lora_a [[buffer(12)]],
    device const float* down_lora_b [[buffer(13)]],
    constant LoraConfig& lora_config [[buffer(14)]],
    constant RingBuffer& ring_buffer [[buffer(15)]],
    constant uint& dropout_seed [[buffer(16)]],
    constant uint& hidden_size [[buffer(17)]],
    constant uint& intermediate_size [[buffer(18)]],
    constant uint& batch_size [[buffer(19)]],
    constant uint& max_adapters [[buffer(20)]],
    uint3 gid [[thread_position_in_grid]]
) {
    uint batch_idx = gid.x;
    uint hidden_idx = gid.y;

    if (batch_idx >= batch_size || hidden_idx >= hidden_size) {
        return;
    }

    // Preload input vector pointer for convenience
    device const float* input_vec = input + batch_idx * hidden_size;
    float output_val = 0.0f;

    // Precompute x^T A for gate and up once per token for all adapters
    thread float gate_ax[MAX_ADAPTER_SLOTS * MAX_LORA_RANK];
    thread float up_ax[MAX_ADAPTER_SLOTS * MAX_LORA_RANK];
    if (has_gate_lora) {
        compute_lora_ax_thread(
            gate_lora_a,
            input_vec,
            hidden_size,
            lora_config.rank,
            ring_buffer,
            max_adapters,
            gate_ax
        );
    }
    if (has_up_lora) {
        compute_lora_ax_thread(
            up_lora_a,
            input_vec,
            hidden_size,
            lora_config.rank,
            ring_buffer,
            max_adapters,
            up_ax
        );
    }

    for (uint j = 0; j < intermediate_size; ++j) {
        // Base projections: gate and up
        float gate_val = 0.0f;
        float up_val = 0.0f;
        for (uint i = 0; i < hidden_size; ++i) {
            float x = input_vec[i];
            gate_val = fma(x, gate_weight[i * intermediate_size + j], gate_val);
            up_val   = fma(x, up_weight[i * intermediate_size + j],   up_val);
        }

        // LoRA deltas via precomputed A*x and adapter-specific B
        if (has_gate_lora) {
            const uint K = min(ring_buffer.top_k, (uint)MAX_ADAPTER_SLOTS);
            for (uint kslot = 0; kslot < K; ++kslot) {
                uint adapter_id = ring_buffer.adapter_indices[kslot];
                if (adapter_id >= max_adapters) { continue; }
                uint b_base = adapter_id * lora_config.rank * intermediate_size;
                for (uint r = 0; r < lora_config.rank && r < (uint)MAX_LORA_RANK; ++r) {
                    gate_val = fma(
                        gate_ax[kslot * MAX_LORA_RANK + r],
                        gate_lora_b[b_base + r * intermediate_size + j],
                        gate_val
                    );
                }
            }
        }
        if (has_up_lora) {
            const uint K = min(ring_buffer.top_k, (uint)MAX_ADAPTER_SLOTS);
            for (uint kslot = 0; kslot < K; ++kslot) {
                uint adapter_id = ring_buffer.adapter_indices[kslot];
                if (adapter_id >= max_adapters) { continue; }
                uint b_base = adapter_id * lora_config.rank * intermediate_size;
                for (uint r = 0; r < lora_config.rank && r < (uint)MAX_LORA_RANK; ++r) {
                    up_val = fma(
                        up_ax[kslot * MAX_LORA_RANK + r],
                        up_lora_b[b_base + r * intermediate_size + j],
                        up_val
                    );
                }
            }
        }

        if (gate_bias) { gate_val += gate_bias[j]; }
        if (up_bias)   { up_val   += up_bias[j]; }

        float activated = deterministic_silu(gate_val);
        if (lora_config.dropout_rate > 0.0f) {
            uint dropout_position = batch_idx * hidden_size * intermediate_size + hidden_idx * intermediate_size + j;
            float mask = deterministic_dropout(dropout_seed, dropout_position, lora_config.dropout_rate);
            activated *= mask;
        }

        float intermediate_val = activated * up_val;

        // Down projection (base + LoRA for current j, hidden_idx)
        float down_val = down_weight[j * hidden_size + hidden_idx];
        if (has_down_lora) {
            const uint K = min(ring_buffer.top_k, (uint)MAX_ADAPTER_SLOTS);
            for (uint kslot = 0; kslot < K; ++kslot) {
                uint adapter_id = ring_buffer.adapter_indices[kslot];
                if (adapter_id >= max_adapters) { continue; }
                float gate = q15_to_float(ring_buffer.gates[kslot]);
                uint a_base = adapter_id * intermediate_size * lora_config.rank;
                uint b_base = adapter_id * lora_config.rank * hidden_size;
                for (uint r = 0; r < lora_config.rank && r < (uint)MAX_LORA_RANK; ++r) {
                    float a = down_lora_a[a_base + j * lora_config.rank + r];
                    float b = down_lora_b[b_base + r * hidden_size + hidden_idx];
                    down_val = fma(gate * a, b, down_val);
                }
            }
        }

        output_val += intermediate_val * down_val;
    }

    if (down_bias) {
        output_val += down_bias[hidden_idx];
    }

    output[batch_idx * hidden_size + hidden_idx] = output_val;
}
