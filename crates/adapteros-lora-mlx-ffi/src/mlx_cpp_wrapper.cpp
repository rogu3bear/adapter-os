// MLX C++ wrapper implementation (stub)
// Provides C-compatible interface for MLX functionality
// Note: MLX is primarily a Python framework, so this is a stub implementation

#include "wrapper.h"
#include <memory>
#include <string>
#include <vector>
#include <iostream>
#include <cstring>

// Global error state
static thread_local std::string g_last_error;

// Simple array structure for stub implementation
struct StubArray {
    std::vector<float> data;
    std::vector<int> shape;
    
    StubArray(const std::vector<float>& d) : data(d), shape({static_cast<int>(d.size())}) {}
    StubArray(const std::vector<int>& d) : shape({static_cast<int>(d.size())}) {
        data.resize(d.size());
        for (size_t i = 0; i < d.size(); ++i) {
            data[i] = static_cast<float>(d[i]);
        }
    }
    StubArray(const std::vector<uint32_t>& d) : shape({static_cast<int>(d.size())}) {
        data.resize(d.size());
        for (size_t i = 0; i < d.size(); ++i) {
            data[i] = static_cast<float>(d[i]);
        }
    }
    StubArray(int size, float value) : data(size, value), shape({size}) {}
};

// Simple model structure for stub implementation
struct StubModel {
    std::string path;
    std::vector<float> weights; // Dummy weights
    
    StubModel(const std::string& p) : path(p) {
        // Initialize with dummy weights
        weights.resize(1000, 0.1f);
    }
};

// Context management (stub implementation)
mlx_context_t* mlx_context_new(void) {
    try {
        auto ctx = new int(1); // Dummy context
        return reinterpret_cast<mlx_context_t*>(ctx);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

void mlx_context_free(mlx_context_t* ctx) {
    if (ctx) {
        delete reinterpret_cast<int*>(ctx);
    }
}

void mlx_set_default_context(mlx_context_t* ctx) {
    // Stub implementation - no-op
    (void)ctx;
}

// Array operations
mlx_array_t* mlx_array_from_data(const float* data, int size) {
    try {
        std::vector<float> vec(data, data + size);
        auto array = new StubArray(vec);
        return reinterpret_cast<mlx_array_t*>(array);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

mlx_array_t* mlx_array_from_ints(const int* data, int size) {
    try {
        std::vector<int> vec(data, data + size);
        auto array = new StubArray(vec);
        return reinterpret_cast<mlx_array_t*>(array);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

mlx_array_t* mlx_array_from_uints(const uint32_t* data, int size) {
    try {
        std::vector<uint32_t> vec(data, data + size);
        auto array = new StubArray(vec);
        return reinterpret_cast<mlx_array_t*>(array);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

mlx_array_t* mlx_array_zeros(int size) {
    try {
        auto array = new StubArray(size, 0.0f);
        return reinterpret_cast<mlx_array_t*>(array);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

mlx_array_t* mlx_array_ones(int size) {
    try {
        auto array = new StubArray(size, 1.0f);
        return reinterpret_cast<mlx_array_t*>(array);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

mlx_array_t* mlx_array_full(int size, float value) {
    try {
        auto array = new StubArray(size, value);
        return reinterpret_cast<mlx_array_t*>(array);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

// Array properties
float* mlx_array_data(mlx_array_t* array) {
    if (!array) return nullptr;
    try {
        auto arr = reinterpret_cast<StubArray*>(array);
        return arr->data.data();
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

int mlx_array_size(mlx_array_t* array) {
    if (!array) return 0;
    try {
        auto arr = reinterpret_cast<StubArray*>(array);
        return static_cast<int>(arr->data.size());
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return 0;
    }
}

int mlx_array_shape(mlx_array_t* array, int* shape, int max_dims) {
    if (!array || !shape) return 0;
    try {
        auto arr = reinterpret_cast<StubArray*>(array);
        int count = std::min(static_cast<int>(arr->shape.size()), max_dims);
        for (int i = 0; i < count; ++i) {
            shape[i] = arr->shape[i];
        }
        return count;
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return 0;
    }
}

int mlx_array_ndim(mlx_array_t* array) {
    if (!array) return 0;
    try {
        auto arr = reinterpret_cast<StubArray*>(array);
        return static_cast<int>(arr->shape.size());
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return 0;
    }
}

int mlx_array_dtype(mlx_array_t* array) {
    if (!array) return 0;
    try {
        // Stub implementation - always return float32
        return 1; // Float32 dtype
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return 0;
    }
}

// Array operations
mlx_array_t* mlx_array_copy(mlx_array_t* array) {
    if (!array) return nullptr;
    try {
        auto arr = reinterpret_cast<StubArray*>(array);
        // Optimize: use move semantics instead of copy for large arrays
        auto copy = new StubArray(arr->data);
        // Reserve capacity to avoid reallocation in future operations
        copy->data.shrink_to_fit();
        return reinterpret_cast<mlx_array_t*>(copy);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

mlx_array_t* mlx_array_reshape(mlx_array_t* array, const int* shape, int ndim) {
    if (!array || !shape) return nullptr;
    try {
        auto arr = reinterpret_cast<StubArray*>(array);
        // Simple reshape - just update shape
        auto reshaped = new StubArray(arr->data);
        reshaped->shape.assign(shape, shape + ndim);
        return reinterpret_cast<mlx_array_t*>(reshaped);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

mlx_array_t* mlx_array_transpose(mlx_array_t* array) {
    if (!array) return nullptr;
    try {
        auto arr = reinterpret_cast<StubArray*>(array);
        // Simple transpose - just reverse shape
        auto transposed = new StubArray(arr->data);
        std::reverse(transposed->shape.begin(), transposed->shape.end());
        return reinterpret_cast<mlx_array_t*>(transposed);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

void mlx_array_free(mlx_array_t* array) {
    if (array) {
        delete reinterpret_cast<StubArray*>(array);
    }
}

// Model operations
mlx_model_t* mlx_model_load(const char* path) {
    if (!path) return nullptr;
    try {
        auto model = new StubModel(std::string(path));
        return reinterpret_cast<mlx_model_t*>(model);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

mlx_model_t* mlx_model_load_from_buffer(const uint8_t* buffer, size_t buffer_len, const char* config_json) {
    if (!buffer || buffer_len < 4 || !config_json) {
        g_last_error = "Invalid buffer or config";
        return nullptr;
    }
    try {
        // For stub, create a model with empty path
        auto model = new StubModel("");
        return reinterpret_cast<mlx_model_t*>(model);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

mlx_array_t* mlx_model_forward(mlx_model_t* model, mlx_array_t* input) {
    if (!model || !input) return nullptr;
    try {
        auto mdl = reinterpret_cast<StubModel*>(model);
        auto inp = reinterpret_cast<StubArray*>(input);
        
        // Stub forward pass - return dummy output
        std::vector<float> output(inp->data.size(), 0.5f);
        auto result = new StubArray(output);
        return reinterpret_cast<mlx_array_t*>(result);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

mlx_array_t* mlx_model_forward_with_hidden_states(mlx_model_t* model, mlx_array_t* input, mlx_array_t** hidden_states, int* num_hidden) {
    if (!model || !input || !hidden_states || !num_hidden) return nullptr;
    try {
        auto mdl = reinterpret_cast<StubModel*>(model);
        auto inp = reinterpret_cast<StubArray*>(input);
        
        // Stub forward pass with hidden states
        // Create dummy hidden states for the 4 target modules
        const int num_modules = 4;
        mlx_array_t** hidden_array = new mlx_array_t*[num_modules];

        // Create stub hidden states for q_proj, k_proj, v_proj, o_proj
        for (int i = 0; i < num_modules; ++i) {
            std::vector<float> hidden_data(inp->data.size(), 0.5f + i * 0.1f);
            auto hidden = new StubArray(hidden_data);
            hidden_array[i] = reinterpret_cast<mlx_array_t*>(hidden);
        }

        *hidden_states = reinterpret_cast<mlx_array_t*>(hidden_array);
        *num_hidden = num_modules;

        // Return output logits
        std::vector<float> output(inp->data.size(), 0.5f);
        auto result = new StubArray(output);

        return reinterpret_cast<mlx_array_t*>(result);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

void mlx_model_free(mlx_model_t* model) {
    if (model) {
        delete reinterpret_cast<StubModel*>(model);
    }
}

void mlx_hidden_states_free(mlx_array_t* hidden_states, int num_hidden) {
    if (hidden_states && num_hidden > 0) {
        // Cast back to array of pointers
        mlx_array_t** hidden_array = reinterpret_cast<mlx_array_t**>(hidden_states);

        // Free each individual hidden state array
        for (int i = 0; i < num_hidden; ++i) {
            if (hidden_array[i]) {
                mlx_array_free(hidden_array[i]);
            }
        }

        // Free the array of pointers itself
        delete[] hidden_array;
    }
}

// Stub hidden state names for the 4 target modules
static const char* g_stub_hidden_state_names[] = {
    "layer.0.self_attn.q_proj",
    "layer.0.self_attn.k_proj",
    "layer.0.self_attn.v_proj",
    "layer.0.self_attn.o_proj"
};
static const int g_stub_hidden_state_count = 4;

// Get the name of a hidden state at the given index (stub implementation)
int mlx_model_get_hidden_state_name(
    mlx_model_t* model,
    int index,
    char* out_name,
    int out_name_len
) {
    if (!model || index < 0 || index >= g_stub_hidden_state_count) return 0;

    const char* name = g_stub_hidden_state_names[index];
    int name_len = static_cast<int>(std::strlen(name));

    // If buffer provided and large enough, copy the name
    if (out_name && out_name_len > name_len) {
        std::memcpy(out_name, name, name_len + 1); // Include null terminator
    }

    return name_len;
}

// Get the number of hidden states stored in the model (stub implementation)
int mlx_model_get_hidden_state_count(mlx_model_t* model) {
    if (!model) return 0;
    return g_stub_hidden_state_count;
}

// Core operations
mlx_array_t* mlx_add(mlx_array_t* a, mlx_array_t* b) {
    if (!a || !b) return nullptr;
    try {
        auto arr_a = reinterpret_cast<StubArray*>(a);
        auto arr_b = reinterpret_cast<StubArray*>(b);
        
        std::vector<float> result;
        size_t min_size = std::min(arr_a->data.size(), arr_b->data.size());
        for (size_t i = 0; i < min_size; ++i) {
            result.push_back(arr_a->data[i] + arr_b->data[i]);
        }
        
        auto result_array = new StubArray(result);
        return reinterpret_cast<mlx_array_t*>(result_array);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

mlx_array_t* mlx_subtract(mlx_array_t* a, mlx_array_t* b) {
    if (!a || !b) return nullptr;
    try {
        auto arr_a = reinterpret_cast<StubArray*>(a);
        auto arr_b = reinterpret_cast<StubArray*>(b);
        
        std::vector<float> result;
        size_t min_size = std::min(arr_a->data.size(), arr_b->data.size());
        for (size_t i = 0; i < min_size; ++i) {
            result.push_back(arr_a->data[i] - arr_b->data[i]);
        }
        
        auto result_array = new StubArray(result);
        return reinterpret_cast<mlx_array_t*>(result_array);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

mlx_array_t* mlx_multiply(mlx_array_t* a, mlx_array_t* b) {
    if (!a || !b) return nullptr;
    try {
        auto arr_a = reinterpret_cast<StubArray*>(a);
        auto arr_b = reinterpret_cast<StubArray*>(b);
        
        std::vector<float> result;
        size_t min_size = std::min(arr_a->data.size(), arr_b->data.size());
        for (size_t i = 0; i < min_size; ++i) {
            result.push_back(arr_a->data[i] * arr_b->data[i]);
        }
        
        auto result_array = new StubArray(result);
        return reinterpret_cast<mlx_array_t*>(result_array);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

mlx_array_t* mlx_divide(mlx_array_t* a, mlx_array_t* b) {
    if (!a || !b) return nullptr;
    try {
        auto arr_a = reinterpret_cast<StubArray*>(a);
        auto arr_b = reinterpret_cast<StubArray*>(b);
        
        std::vector<float> result;
        size_t min_size = std::min(arr_a->data.size(), arr_b->data.size());
        for (size_t i = 0; i < min_size; ++i) {
            result.push_back(arr_a->data[i] / arr_b->data[i]);
        }
        
        auto result_array = new StubArray(result);
        return reinterpret_cast<mlx_array_t*>(result_array);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

mlx_array_t* mlx_matmul(mlx_array_t* a, mlx_array_t* b) {
    if (!a || !b) return nullptr;
    try {
        auto arr_a = reinterpret_cast<StubArray*>(a);
        auto arr_b = reinterpret_cast<StubArray*>(b);
        
        // Simple matrix multiplication stub
        std::vector<float> result(arr_a->data.size(), 0.0f);
        for (size_t i = 0; i < arr_a->data.size(); ++i) {
            result[i] = arr_a->data[i] * 0.5f; // Dummy matmul
        }
        
        auto result_array = new StubArray(result);
        return reinterpret_cast<mlx_array_t*>(result_array);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

// Activation functions
mlx_array_t* mlx_relu(mlx_array_t* array) {
    if (!array) return nullptr;
    try {
        auto arr = reinterpret_cast<StubArray*>(array);
        std::vector<float> result;
        for (float val : arr->data) {
            result.push_back(std::max(0.0f, val));
        }
        auto result_array = new StubArray(result);
        return reinterpret_cast<mlx_array_t*>(result_array);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

mlx_array_t* mlx_gelu(mlx_array_t* array) {
    if (!array) return nullptr;
    try {
        auto arr = reinterpret_cast<StubArray*>(array);
        // Simple GELU approximation
        std::vector<float> result;
        for (float val : arr->data) {
            result.push_back(val * 0.5f * (1.0f + std::tanh(0.79788456f * (val + 0.044715f * val * val * val))));
        }
        auto result_array = new StubArray(result);
        return reinterpret_cast<mlx_array_t*>(result_array);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

mlx_array_t* mlx_sigmoid(mlx_array_t* array) {
    if (!array) return nullptr;
    try {
        auto arr = reinterpret_cast<StubArray*>(array);
        std::vector<float> result;
        for (float val : arr->data) {
            result.push_back(1.0f / (1.0f + std::exp(-val)));
        }
        auto result_array = new StubArray(result);
        return reinterpret_cast<mlx_array_t*>(result_array);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

mlx_array_t* mlx_tanh(mlx_array_t* array) {
    if (!array) return nullptr;
    try {
        auto arr = reinterpret_cast<StubArray*>(array);
        std::vector<float> result;
        for (float val : arr->data) {
            result.push_back(std::tanh(val));
        }
        auto result_array = new StubArray(result);
        return reinterpret_cast<mlx_array_t*>(result_array);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

mlx_array_t* mlx_softmax(mlx_array_t* array) {
    if (!array) return nullptr;
    try {
        auto arr = reinterpret_cast<StubArray*>(array);
        std::vector<float> result;
        
        // Simple softmax
        float max_val = *std::max_element(arr->data.begin(), arr->data.end());
        float sum = 0.0f;
        for (float val : arr->data) {
            sum += std::exp(val - max_val);
        }
        for (float val : arr->data) {
            result.push_back(std::exp(val - max_val) / sum);
        }
        
        auto result_array = new StubArray(result);
        return reinterpret_cast<mlx_array_t*>(result_array);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

// LoRA operations
mlx_array_t* mlx_lora_forward(mlx_array_t* input, mlx_array_t* lora_a, mlx_array_t* lora_b, float alpha, float rank) {
    if (!input || !lora_a || !lora_b) return nullptr;
    try {
        auto inp = reinterpret_cast<StubArray*>(input);
        auto a = reinterpret_cast<StubArray*>(lora_a);
        auto b = reinterpret_cast<StubArray*>(lora_b);
        
        // Simple LoRA forward pass stub
        std::vector<float> result(inp->data.size(), 0.0f);
        for (size_t i = 0; i < inp->data.size(); ++i) {
            result[i] = inp->data[i] * (alpha / rank) * 0.1f; // Dummy LoRA
        }
        
        auto result_array = new StubArray(result);
        return reinterpret_cast<mlx_array_t*>(result_array);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

mlx_array_t* mlx_lora_combine(mlx_array_t* base_output, mlx_array_t* lora_output, float gate) {
    if (!base_output || !lora_output) return nullptr;
    try {
        auto base = reinterpret_cast<StubArray*>(base_output);
        auto lora = reinterpret_cast<StubArray*>(lora_output);

        std::vector<float> result;
        size_t min_size = std::min(base->data.size(), lora->data.size());
        for (size_t i = 0; i < min_size; ++i) {
            result.push_back(base->data[i] + gate * lora->data[i]);
        }

        auto result_array = new StubArray(result);
        return reinterpret_cast<mlx_array_t*>(result_array);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

// Multi-adapter K-sparse LoRA routing with Q15 quantized gates - Stub implementation
//
// Formula: output = input + sum_i(gate_i * B_i(A_i(input)) * (alpha/rank))
//
// This stub provides the same interface as the real MLX implementation for testing.
mlx_array_t* mlx_multi_lora_forward(
    mlx_array_t* input,
    mlx_array_t** lora_a_list,
    mlx_array_t** lora_b_list,
    int num_adapters,
    const int16_t* gates_q15,
    float alpha,
    float rank
) {
    // Validate input parameters
    if (!input) {
        g_last_error = "mlx_multi_lora_forward: input tensor is null";
        return nullptr;
    }
    if (!lora_a_list || !lora_b_list) {
        g_last_error = "mlx_multi_lora_forward: adapter weight lists are null";
        return nullptr;
    }
    if (!gates_q15) {
        g_last_error = "mlx_multi_lora_forward: gates_q15 array is null";
        return nullptr;
    }
    if (num_adapters <= 0) {
        g_last_error = "mlx_multi_lora_forward: num_adapters must be positive";
        return nullptr;
    }

    // Enforce maximum K=8 adapters for K-sparse routing
    if (num_adapters > 8) {
        g_last_error = "mlx_multi_lora_forward: num_adapters exceeds K-sparse limit (max 8)";
        return nullptr;
    }

    // Validate rank to prevent division by zero
    if (rank <= 0.0f) {
        g_last_error = "mlx_multi_lora_forward: rank must be positive";
        return nullptr;
    }

    try {
        auto inp = reinterpret_cast<StubArray*>(input);

        // Initialize result with input (identity path will be preserved)
        std::vector<float> result = inp->data;

        // Precompute LoRA scaling factor: alpha / rank
        const float scaling = alpha / rank;

        // Q15 dequantization constant
        constexpr float Q15_SCALE = 32767.0f;

        // Process each adapter with its K-sparse gate weight
        for (int i = 0; i < num_adapters; ++i) {
            // Skip null adapters (sparse routing may leave some slots empty)
            if (!lora_a_list[i] || !lora_b_list[i]) {
                continue;
            }

            // Dequantize Q15 gate weight: gate_f32 = gate_q15 / 32767.0
            // Clamp negative values to 0 (gates should be non-negative)
            int16_t gate_q15 = gates_q15[i];
            if (gate_q15 < 0) {
                gate_q15 = 0;
            }
            float gate_weight = static_cast<float>(gate_q15) / Q15_SCALE;

            // Skip adapters with zero or negligible gate (K-sparse efficiency)
            if (gate_weight <= 1e-6f) {
                continue;
            }

            // Note: In stub mode, we skip actual use of a and b matrices
            // Real implementation does: input @ A @ B
            (void)lora_a_list[i];
            (void)lora_b_list[i];

            // Simplified stub LoRA forward pass
            // Stub: apply a simple transformation simulating LoRA contribution
            float combined_scale = gate_weight * scaling;
            for (size_t j = 0; j < result.size(); ++j) {
                // Dummy computation: scale input by combined factor
                // Real impl would be: result[j] += gate_weight * (input @ A @ B) * scaling
                result[j] += inp->data[j] * combined_scale * 0.1f;
            }
        }

        auto result_array = new StubArray(result);
        return reinterpret_cast<mlx_array_t*>(result_array);

    } catch (const std::exception& e) {
        g_last_error = std::string("mlx_multi_lora_forward failed: ") + e.what();
        return nullptr;
    }
}

// Error handling
const char* mlx_get_last_error(void) {
    return g_last_error.c_str();
}

void mlx_clear_error(void) {
    g_last_error.clear();
}

// RNG seeding (stub implementation)
// Sets MLX's global random seed from a seed buffer (HKDF-derived)
// In stub mode, this is a no-op but must be present for linking
void mlx_set_seed(const uint8_t* seed, size_t seed_len) {
    // Stub implementation - no actual RNG state to set
    // In real MLX, this would set the global random state
    (void)seed;
    (void)seed_len;
}

// Memory management
void mlx_gc_collect(void) {
    // Stub implementation - no-op
}

size_t mlx_memory_usage(void) {
    // Stub implementation - return dummy value
    return 1024 * 1024; // 1MB
}

size_t mlx_allocation_count(void) {
    // Stub implementation - return dummy value
    return 10; // 10 allocations
}

void mlx_memory_reset(void) {
    // Stub implementation - no-op
}

void mlx_memory_stats(size_t* out_total_bytes, size_t* out_allocation_count) {
    // Stub implementation - return dummy values
    if (out_total_bytes) {
        *out_total_bytes = 1024 * 1024; // 1MB
    }
    if (out_allocation_count) {
        *out_allocation_count = 10; // 10 allocations
    }
}

// ============================================================================
// Runtime initialization (stub implementations)
// ============================================================================

int mlx_init(int device_type) {
    (void)device_type;
    return 0; // Success
}

int mlx_init_default(void) {
    return 0; // Success
}

void mlx_shutdown(void) {
    // Stub - no-op
}

bool mlx_is_initialized(void) {
    return true; // Always "initialized" in stub mode
}

mlx_device_type_t mlx_get_device_type(void) {
    return MLX_DEVICE_AUTO;
}

int mlx_set_device(mlx_device_type_t device_type) {
    (void)device_type;
    return 0; // Success
}

int mlx_backend_info(mlx_backend_capabilities_t* capabilities) {
    if (!capabilities) return -1;
    // Fill with stub values
    std::memset(capabilities, 0, sizeof(mlx_backend_capabilities_t));
    return 0;
}

const char* mlx_get_version(void) {
    static const char* version = "stub-0.1.0";
    return version;
}

// ============================================================================
// Quantization (stub implementations)
// ============================================================================

mlx_array_t* mlx_quantize(mlx_array_t* array, int group_size, int bits) {
    (void)group_size;
    (void)bits;
    if (!array) return nullptr;
    // Return copy of input (stub doesn't actually quantize)
    return mlx_array_copy(array);
}

mlx_array_t* mlx_dequantize(mlx_array_t* array, mlx_array_t* scales, mlx_array_t* biases, int group_size, int bits) {
    (void)scales;
    (void)biases;
    (void)group_size;
    (void)bits;
    if (!array) return nullptr;
    // Return copy of input (stub doesn't actually dequantize)
    return mlx_array_copy(array);
}

// ============================================================================
// RoPE (stub implementation)
// ============================================================================

mlx_array_t* mlx_rope(mlx_array_t* array, int dims, bool traditional, float base, float scale, int offset) {
    (void)dims;
    (void)traditional;
    (void)base;
    (void)scale;
    (void)offset;
    if (!array) return nullptr;
    // Return copy of input (stub doesn't apply RoPE)
    return mlx_array_copy(array);
}

// ============================================================================
// Attention (stub implementations)
// ============================================================================

mlx_array_t* mlx_scaled_dot_product_attention(mlx_array_t* queries, mlx_array_t* keys, mlx_array_t* values, float scale, mlx_array_t* mask) {
    (void)keys;
    (void)values;
    (void)scale;
    (void)mask;
    if (!queries) return nullptr;
    // Return copy of queries (stub doesn't compute attention)
    return mlx_array_copy(queries);
}

mlx_array_t* mlx_create_causal_mask(int seq_len) {
    // Create a simple mask array
    std::vector<float> mask_data(seq_len * seq_len, 0.0f);
    for (int i = 0; i < seq_len; ++i) {
        for (int j = i + 1; j < seq_len; ++j) {
            mask_data[i * seq_len + j] = -1e9f;
        }
    }
    auto arr = new StubArray(mask_data);
    return reinterpret_cast<mlx_array_t*>(arr);
}

// ============================================================================
// KV Cache (stub implementations)
// ============================================================================

struct StubKVCache {
    int num_layers;
    int num_heads;
    int head_dim;
    int max_seq_len;
    int current_seq_len;
};

mlx_kv_cache_t* mlx_kv_cache_new(int num_layers, int num_heads, int head_dim, int max_seq_len) {
    auto cache = new StubKVCache{num_layers, num_heads, head_dim, max_seq_len, 0};
    return reinterpret_cast<mlx_kv_cache_t*>(cache);
}

int mlx_kv_cache_update(mlx_kv_cache_t* cache, int layer_idx, mlx_array_t* keys, mlx_array_t* values) {
    (void)layer_idx;
    (void)keys;
    (void)values;
    if (!cache) return -1;
    auto kv = reinterpret_cast<StubKVCache*>(cache);
    kv->current_seq_len++;
    return 0;
}

mlx_array_t* mlx_kv_cache_get_keys(mlx_kv_cache_t* cache, int layer_idx) {
    (void)layer_idx;
    if (!cache) return nullptr;
    // Return empty array stub
    return mlx_array_zeros(64);
}

mlx_array_t* mlx_kv_cache_get_values(mlx_kv_cache_t* cache, int layer_idx) {
    (void)layer_idx;
    if (!cache) return nullptr;
    // Return empty array stub
    return mlx_array_zeros(64);
}

int mlx_kv_cache_seq_len(mlx_kv_cache_t* cache) {
    if (!cache) return 0;
    auto kv = reinterpret_cast<StubKVCache*>(cache);
    return kv->current_seq_len;
}

void mlx_kv_cache_reset(mlx_kv_cache_t* cache) {
    if (!cache) return;
    auto kv = reinterpret_cast<StubKVCache*>(cache);
    kv->current_seq_len = 0;
}

void mlx_kv_cache_free(mlx_kv_cache_t* cache) {
    if (cache) {
        delete reinterpret_cast<StubKVCache*>(cache);
    }
}

// ============================================================================
// SafeTensors (stub implementations)
// ============================================================================

struct StubWeights {
    std::vector<std::string> names;
};

mlx_weights_t* mlx_load_safetensors(const char* path) {
    (void)path;
    auto weights = new StubWeights();
    weights->names.push_back("model.embed_tokens.weight");
    weights->names.push_back("lm_head.weight");
    return reinterpret_cast<mlx_weights_t*>(weights);
}

mlx_array_t* mlx_weights_get(mlx_weights_t* weights, const char* name) {
    (void)name;
    if (!weights) return nullptr;
    // Return dummy weight array
    return mlx_array_ones(1000);
}

int mlx_weights_list(mlx_weights_t* weights, const char** names, int max_names) {
    if (!weights) return 0;
    auto w = reinterpret_cast<StubWeights*>(weights);
    int count = static_cast<int>(w->names.size());
    if (names && max_names > 0) {
        int to_copy = std::min(count, max_names);
        for (int i = 0; i < to_copy; ++i) {
            names[i] = w->names[i].c_str();
        }
    }
    return count;
}

void mlx_weights_free(mlx_weights_t* weights) {
    if (weights) {
        delete reinterpret_cast<StubWeights*>(weights);
    }
}

// ============================================================================
// Evaluation (stub implementations)
// ============================================================================

void mlx_eval(mlx_array_t* array) {
    (void)array;
    // Stub - no-op (lazy evaluation not implemented)
}

void mlx_eval_all(mlx_array_t** arrays, int num_arrays) {
    (void)arrays;
    (void)num_arrays;
    // Stub - no-op
}

void mlx_synchronize(void) {
    // Stub - no-op (no GPU to synchronize)
}

// ============================================================================
// LoRA Adapter Caching (stub implementations)
// ============================================================================

static std::unordered_map<std::string, int> g_lora_cache_stub;

const char* mlx_lora_cache_adapter(const char* adapter_id, mlx_array_t* lora_a, mlx_array_t* lora_b) {
    (void)lora_a;
    (void)lora_b;
    if (!adapter_id) return nullptr;
    g_lora_cache_stub[std::string(adapter_id)] = 1;
    return adapter_id;
}

bool mlx_lora_get_cached(const char* adapter_id, mlx_array_t** out_lora_a, mlx_array_t** out_lora_b) {
    if (!adapter_id || !out_lora_a || !out_lora_b) return false;
    auto it = g_lora_cache_stub.find(std::string(adapter_id));
    if (it == g_lora_cache_stub.end()) return false;
    // Return dummy arrays
    *out_lora_a = mlx_array_ones(64);
    *out_lora_b = mlx_array_ones(64);
    return true;
}

void mlx_lora_evict_cached(const char* adapter_id) {
    if (!adapter_id) return;
    g_lora_cache_stub.erase(std::string(adapter_id));
}

void mlx_lora_clear_cache(void) {
    g_lora_cache_stub.clear();
}

size_t mlx_lora_cache_size(void) {
    return g_lora_cache_stub.size();
}

void mlx_lora_set_cache_limit(size_t max_entries) {
    (void)max_entries;
    // Stub - no actual limit enforcement
}