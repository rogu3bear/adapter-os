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

// Fused MLP kernel with SwiGLU activation, LoRA support, and bias
kernel void fused_mlp(
    constant MlpParams& params,
    uint3 gid [[thread_position_in_grid]]
) {
    uint batch_idx = gid.x;
    uint hidden_idx = gid.y;

    if (batch_idx >= params.batch_size || hidden_idx >= params.hidden_size) {
        return;
    }

    const uint hidden_size = params.hidden_size;
    const uint intermediate_size = params.intermediate_size;
    const uint rank = params.lora_config.rank;

    float output_val = 0.0f;
    const bool has_gate_lora = (params.gate_lora_a && params.gate_lora_b && rank > 0);
    const bool has_up_lora   = (params.up_lora_a && params.up_lora_b && rank > 0);
    const bool has_down_lora = (params.down_lora_a && params.down_lora_b && rank > 0);

    // Preload input vector pointer for convenience
    device const float* input_vec = params.input + batch_idx * hidden_size;

    // Precompute x^T A for gate and up once per token for all adapters
    thread float gate_ax[MAX_ADAPTER_SLOTS * MAX_LORA_RANK];
    thread float up_ax[MAX_ADAPTER_SLOTS * MAX_LORA_RANK];
    if (has_gate_lora) {
        compute_lora_ax_thread(
            params.gate_lora_a,
            input_vec,
            hidden_size,
            rank,
            params.ring_buffer,
            params.max_adapters,
            gate_ax
        );
    }
    if (has_up_lora) {
        compute_lora_ax_thread(
            params.up_lora_a,
            input_vec,
            hidden_size,
            rank,
            params.ring_buffer,
            params.max_adapters,
            up_ax
        );
    }

    for (uint j = 0; j < intermediate_size; ++j) {
        // Base projections: gate and up
        float gate_val = 0.0f;
        float up_val = 0.0f;
        for (uint i = 0; i < hidden_size; ++i) {
            float x = input_vec[i];
            gate_val = fma(x, params.gate_weight[i * intermediate_size + j], gate_val);
            up_val   = fma(x, params.up_weight[i * intermediate_size + j],   up_val);
        }

        // LoRA deltas via precomputed A*x and adapter-specific B
        if (has_gate_lora) {
            const uint K = min(params.ring_buffer.top_k, (uint)MAX_ADAPTER_SLOTS);
            for (uint kslot = 0; kslot < K; ++kslot) {
                uint adapter_id = params.ring_buffer.adapter_indices[kslot];
                if (adapter_id >= params.max_adapters) { continue; }
                uint b_base = adapter_id * rank * intermediate_size;
                for (uint r = 0; r < rank && r < (uint)MAX_LORA_RANK; ++r) {
                    gate_val = fma(
                        gate_ax[kslot * MAX_LORA_RANK + r],
                        params.gate_lora_b[b_base + r * intermediate_size + j],
                        gate_val
                    );
                }
            }
        }
        if (has_up_lora) {
            const uint K = min(params.ring_buffer.top_k, (uint)MAX_ADAPTER_SLOTS);
            for (uint kslot = 0; kslot < K; ++kslot) {
                uint adapter_id = params.ring_buffer.adapter_indices[kslot];
                if (adapter_id >= params.max_adapters) { continue; }
                uint b_base = adapter_id * rank * intermediate_size;
                for (uint r = 0; r < rank && r < (uint)MAX_LORA_RANK; ++r) {
                    up_val = fma(
                        up_ax[kslot * MAX_LORA_RANK + r],
                        params.up_lora_b[b_base + r * intermediate_size + j],
                        up_val
                    );
                }
            }
        }

        if (params.gate_bias) { gate_val += params.gate_bias[j]; }
        if (params.up_bias)   { up_val   += params.up_bias[j]; }

        float activated = deterministic_silu(gate_val);
        if (params.lora_config.dropout_rate > 0.0f) {
            uint dropout_position = batch_idx * hidden_size * intermediate_size + hidden_idx * intermediate_size + j;
            float mask = deterministic_dropout(params.dropout_seed, dropout_position, params.lora_config.dropout_rate);
            activated *= mask;
        }

        float intermediate_val = activated * up_val;

        // Down projection (base + LoRA for current j, hidden_idx)
        float down_val = params.down_weight[j * hidden_size + hidden_idx];
        if (has_down_lora) {
            const uint K = min(params.ring_buffer.top_k, (uint)MAX_ADAPTER_SLOTS);
            for (uint kslot = 0; kslot < K; ++kslot) {
                uint adapter_id = params.ring_buffer.adapter_indices[kslot];
                if (adapter_id >= params.max_adapters) { continue; }
                float gate = q15_to_float(params.ring_buffer.gates[kslot]);
                uint a_base = adapter_id * intermediate_size * rank;
                uint b_base = adapter_id * rank * hidden_size;
                for (uint r = 0; r < rank && r < (uint)MAX_LORA_RANK; ++r) {
                    float a = params.down_lora_a[a_base + j * rank + r];
                    float b = params.down_lora_b[b_base + r * hidden_size + hidden_idx];
                    down_val = fma(gate * a, b, down_val);
                }
            }
        }

        output_val += intermediate_val * down_val;
    }

    if (params.down_bias) {
        output_val += params.down_bias[hidden_idx];
    }

    params.output[batch_idx * hidden_size + hidden_idx] = output_val;
}
