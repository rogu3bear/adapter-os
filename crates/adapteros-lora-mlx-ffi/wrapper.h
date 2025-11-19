// MLX FFI wrapper header
// This file provides C-compatible bindings for MLX C++ API

#ifdef __cplusplus
extern "C" {
#endif

#include <stddef.h>

#include <stdbool.h>
#include <stdint.h>

// Forward declarations for MLX types
typedef struct mlx_array mlx_array_t;
typedef struct mlx_model mlx_model_t;
typedef struct mlx_context mlx_context_t;

// Context management
mlx_context_t* mlx_context_new(void);
void mlx_context_free(mlx_context_t* ctx);
void mlx_set_default_context(mlx_context_t* ctx);

// Array operations
mlx_array_t* mlx_array_from_data(const float* data, int size);
mlx_array_t* mlx_array_from_ints(const int* data, int size);
mlx_array_t* mlx_array_from_uints(const uint32_t* data, int size);
mlx_array_t* mlx_array_zeros(int size);
mlx_array_t* mlx_array_ones(int size);
mlx_array_t* mlx_array_full(int size, float value);

// Array properties
float* mlx_array_data(mlx_array_t* array);
int mlx_array_size(mlx_array_t* array);
int mlx_array_shape(mlx_array_t* array, int* shape, int max_dims);
int mlx_array_ndim(mlx_array_t* array);
int mlx_array_dtype(mlx_array_t* array);

// Array operations
mlx_array_t* mlx_array_copy(mlx_array_t* array);
mlx_array_t* mlx_array_reshape(mlx_array_t* array, const int* shape, int ndim);
mlx_array_t* mlx_array_transpose(mlx_array_t* array);
void mlx_array_free(mlx_array_t* array);

// Model operations
mlx_model_t* mlx_model_load(const char* path);
mlx_array_t* mlx_model_forward(mlx_model_t* model, mlx_array_t* input);
mlx_array_t* mlx_model_forward_with_hidden_states(mlx_model_t* model, mlx_array_t* input, mlx_array_t** hidden_states, int* num_hidden);
void mlx_model_free(mlx_model_t* model);
void mlx_hidden_states_free(mlx_array_t* hidden_states, int num_hidden);

// Core operations
mlx_array_t* mlx_add(mlx_array_t* a, mlx_array_t* b);
mlx_array_t* mlx_subtract(mlx_array_t* a, mlx_array_t* b);
mlx_array_t* mlx_multiply(mlx_array_t* a, mlx_array_t* b);
mlx_array_t* mlx_divide(mlx_array_t* a, mlx_array_t* b);
mlx_array_t* mlx_matmul(mlx_array_t* a, mlx_array_t* b);

// Activation functions
mlx_array_t* mlx_relu(mlx_array_t* array);
mlx_array_t* mlx_gelu(mlx_array_t* array);
mlx_array_t* mlx_sigmoid(mlx_array_t* array);
mlx_array_t* mlx_tanh(mlx_array_t* array);
mlx_array_t* mlx_softmax(mlx_array_t* array);

// RNG seeding (for deterministic dropout/sampling)
// Sets MLX's global random seed from a 32-byte seed buffer (HKDF-derived)
// Note: MLX's backend may not guarantee full execution order determinism,
// but seeded operations (dropout, sampling) will be deterministic.
void mlx_set_seed(const uint8_t* seed, size_t seed_len);

// LoRA operations
mlx_array_t* mlx_lora_forward(mlx_array_t* input, mlx_array_t* lora_a, mlx_array_t* lora_b, float alpha, float rank);
mlx_array_t* mlx_lora_combine(mlx_array_t* base_output, mlx_array_t* lora_output, float gate);

// Multi-adapter LoRA routing (K-sparse)
// Apply multiple LoRA adapters with routing gates (max K=8)
// Parameters:
//   input: input tensor to transform
//   lora_a_list: array of LoRA A matrices (low-rank down-projection)
//   lora_b_list: array of LoRA B matrices (low-rank up-projection)
//   num_adapters: number of adapters (K)
//   gates_q15: array of Q15 quantized gate weights (u16, 0-32767)
//   alpha: LoRA scaling factor
//   rank: LoRA rank dimension
// Returns: combined output = base + sum(gate_i * lora_i(input))
mlx_array_t* mlx_multi_lora_forward(
    mlx_array_t* input,
    mlx_array_t** lora_a_list,
    mlx_array_t** lora_b_list,
    int num_adapters,
    const uint16_t* gates_q15,
    float alpha,
    float rank
);

// Error handling
const char* mlx_get_last_error(void);
void mlx_clear_error(void);

// Memory management
void mlx_gc_collect(void);
size_t mlx_memory_usage(void);
size_t mlx_allocation_count(void);
void mlx_memory_reset(void);
void mlx_memory_stats(size_t* out_total_bytes, size_t* out_allocation_count);

#ifdef __cplusplus
}
#endif
