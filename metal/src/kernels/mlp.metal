// AdapterOS MLP Kernel
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
    const bool has_gate_lora =
        (params.gate_lora_a != nullptr) && (params.gate_lora_b != nullptr) && rank > 0;
    const bool has_up_lora =
        (params.up_lora_a != nullptr) && (params.up_lora_b != nullptr) && rank > 0;
    const bool has_down_lora =
        (params.down_lora_a != nullptr) && (params.down_lora_b != nullptr) && rank > 0;

    for (uint j = 0; j < intermediate_size; j++) {
        float gate_val = 0.0f;
        float up_val = 0.0f;

        for (uint i = 0; i < hidden_size; i++) {
            float input_val = params.input[batch_idx * hidden_size + i];
            float gate_weight = params.gate_weight[i * intermediate_size + j];
            float up_weight = params.up_weight[i * intermediate_size + j];

            float gate_delta = 0.0f;
            float up_delta = 0.0f;

            if (has_gate_lora || has_up_lora) {
                for (uint k = 0;
                     k < params.ring_buffer.top_k && k < params.max_adapters;
                     k++) {
                    uint adapter_idx = params.ring_buffer.adapter_indices[k];
                    if (adapter_idx >= rank) {
                        continue;
                    }
                    float gate_scale = q15_to_float(params.ring_buffer.gates[k]);

                    if (has_gate_lora) {
                        float gate_a = params.gate_lora_a[i * rank + adapter_idx];
                        float gate_b = params.gate_lora_b[adapter_idx * intermediate_size + j];
                        gate_delta += gate_scale * gate_a * gate_b;
                    }

                    if (has_up_lora) {
                        float up_a = params.up_lora_a[i * rank + adapter_idx];
                        float up_b = params.up_lora_b[adapter_idx * intermediate_size + j];
                        up_delta += gate_scale * up_a * up_b;
                    }
                }
            }

            gate_val += input_val * (gate_weight + gate_delta);
            up_val += input_val * (up_weight + up_delta);
        }

        if (params.gate_bias != nullptr) {
            gate_val += params.gate_bias[j];
        }
        if (params.up_bias != nullptr) {
            up_val += params.up_bias[j];
        }

        float activated = deterministic_silu(gate_val);

        if (params.lora_config.dropout_rate > 0.0f) {
            uint dropout_position =
                batch_idx * hidden_size * intermediate_size + hidden_idx * intermediate_size + j;
            float dropout_mask = deterministic_dropout(
                params.dropout_seed,
                dropout_position,
                params.lora_config.dropout_rate
            );
            activated *= dropout_mask;
        }

        float intermediate_val = activated * up_val;

        float down_weight = params.down_weight[j * hidden_size + hidden_idx];
        float down_delta = 0.0f;

        if (has_down_lora) {
            for (uint k = 0;
                 k < params.ring_buffer.top_k && k < params.max_adapters;
                 k++) {
                uint adapter_idx = params.ring_buffer.adapter_indices[k];
                if (adapter_idx >= rank) {
                    continue;
                }
                float gate_scale = q15_to_float(params.ring_buffer.gates[k]);
                float down_a = params.down_lora_a[j * rank + adapter_idx];
                float down_b = params.down_lora_b[adapter_idx * hidden_size + hidden_idx];
                down_delta += gate_scale * down_a * down_b;
            }
        }

        output_val += intermediate_val * (down_weight + down_delta);
    }

    if (params.down_bias != nullptr) {
        output_val += params.down_bias[hidden_idx];
    }

    params.output[batch_idx * hidden_size + hidden_idx] = output_val;
}
