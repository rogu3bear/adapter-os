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

// ============================================================================
// Runtime initialization and backend info
// ============================================================================

// Device type enumeration for device selection
typedef enum {
    MLX_DEVICE_CPU = 0,    // CPU backend
    MLX_DEVICE_GPU = 1,    // GPU backend (Metal on macOS)
    MLX_DEVICE_ANE = 2,    // Apple Neural Engine (if available)
    MLX_DEVICE_AUTO = 3    // Auto-select best available device
} mlx_device_type_t;

// Backend capabilities structure
typedef struct {
    bool gpu_available;           // GPU (Metal) available
    bool ane_available;           // Apple Neural Engine available
    bool metal_compute;           // Metal compute shaders supported
    bool unified_memory;          // Unified memory architecture
    int max_threads_per_group;    // Maximum threads per threadgroup
    size_t max_buffer_size;       // Maximum buffer size in bytes
    char device_name[256];        // GPU device name
    char mlx_version[64];         // MLX version string
    char metal_version[64];       // Metal version string
} mlx_backend_capabilities_t;

// Initialize MLX runtime with specified device type
// Returns 0 on success, -1 on error (check mlx_get_last_error())
// This should be called once before using any other MLX functions
int mlx_init(mlx_device_type_t device_type);

// Initialize MLX with default settings (auto device selection)
// Returns 0 on success, -1 on error
int mlx_init_default(void);

// Shutdown MLX runtime and release resources
void mlx_shutdown(void);

// Check if MLX runtime is initialized
bool mlx_is_initialized(void);

// Get current device type
mlx_device_type_t mlx_get_device_type(void);

// Set device type (switch between CPU/GPU)
// Returns 0 on success, -1 on error
int mlx_set_device(mlx_device_type_t device_type);

// Get backend capabilities and version information
// Fills the provided structure with capability information
// Returns 0 on success, -1 on error
int mlx_backend_info(mlx_backend_capabilities_t* capabilities);

// Get MLX version string
// Returns pointer to static string (do not free)
const char* mlx_get_version(void);

// ============================================================================
// Context management
// ============================================================================

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
mlx_model_t* mlx_model_load_from_buffer(const uint8_t* buffer, size_t buffer_len, const char* config_json);
mlx_array_t* mlx_model_forward(mlx_model_t* model, mlx_array_t* input);
mlx_array_t* mlx_model_forward_with_hidden_states(mlx_model_t* model, mlx_array_t* input, mlx_array_t** hidden_states, int* num_hidden);
void mlx_model_free(mlx_model_t* model);
void mlx_hidden_states_free(mlx_array_t* hidden_states, int num_hidden);

// Hidden states access
// Get the name of a hidden state at the given index
// Parameters:
//   model: model handle
//   index: hidden state index (0-based)
//   out_name: output buffer for name (can be NULL to just get length)
//   out_name_len: length of output buffer
// Returns: length of name (excluding null terminator), or 0 if invalid index
int mlx_model_get_hidden_state_name(mlx_model_t* model, int index, char* out_name, int out_name_len);

// Get the number of hidden states stored in the model
// Returns: number of hidden states, or 0 if model is NULL
int mlx_model_get_hidden_state_count(mlx_model_t* model);

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

// Multi-adapter K-sparse LoRA routing with Q15 quantized gates
//
// Apply multiple LoRA adapters with K-sparse routing gates (max K=8)
// Uses Q15 fixed-point format for gate weights (i16, 0-32767 maps to 0.0-1.0)
//
// Formula: output = input + sum_i(gate_i * B_i(A_i(input)) * (alpha/rank))
//
// Parameters:
//   input: Input tensor to transform [batch, seq_len, hidden_dim] or [seq_len, hidden_dim]
//   lora_a_list: Array of LoRA A matrices (down-projection) [hidden_dim, rank]
//   lora_b_list: Array of LoRA B matrices (up-projection) [rank, hidden_dim]
//   num_adapters: Number of active adapters (K-sparse, max 8)
//   gates_q15: Array of Q15 quantized gate weights (i16, 0-32767 = 0.0-1.0)
//   alpha: LoRA scaling factor
//   rank: LoRA rank dimension
//
// Returns: Combined output tensor with identity path and weighted LoRA contributions
//          NULL on error (check mlx_get_last_error())
mlx_array_t* mlx_multi_lora_forward(
    mlx_array_t* input,
    mlx_array_t** lora_a_list,
    mlx_array_t** lora_b_list,
    int num_adapters,
    const int16_t* gates_q15,
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

// ============================================================================
// Quantization operations
// ============================================================================

// Quantize array to specified bit width (4-bit or 8-bit)
// Parameters:
//   array: input tensor to quantize
//   group_size: quantization group size (e.g., 64, 128)
//   bits: number of bits (4 or 8)
// Returns: quantized tensor
mlx_array_t* mlx_quantize(mlx_array_t* array, int group_size, int bits);

// Dequantize array back to float
// Parameters:
//   array: quantized tensor
//   scales: scale factors for each group
//   biases: bias values for each group (can be NULL for symmetric quant)
//   group_size: quantization group size
//   bits: number of bits used for quantization
// Returns: dequantized float tensor
mlx_array_t* mlx_dequantize(mlx_array_t* array, mlx_array_t* scales, mlx_array_t* biases, int group_size, int bits);

// ============================================================================
// Attention operations
// ============================================================================

// Scaled dot-product attention (SDPA)
// Computes: softmax(Q @ K^T / sqrt(d_k)) @ V
// Parameters:
//   queries: query tensor [batch, heads, seq_len, head_dim]
//   keys: key tensor [batch, heads, seq_len, head_dim]
//   values: value tensor [batch, heads, seq_len, head_dim]
//   scale: scaling factor (typically 1/sqrt(head_dim))
//   mask: optional attention mask (NULL for no mask)
// Returns: attention output tensor
mlx_array_t* mlx_scaled_dot_product_attention(
    mlx_array_t* queries,
    mlx_array_t* keys,
    mlx_array_t* values,
    float scale,
    mlx_array_t* mask
);

// ============================================================================
// Rotary Position Embedding (RoPE)
// ============================================================================

// Apply rotary position embeddings
// Parameters:
//   array: input tensor to apply RoPE to
//   dims: number of dimensions to rotate (typically head_dim / 2)
//   traditional: if true, use traditional RoPE; if false, use interleaved
//   base: RoPE base frequency (typically 10000.0)
//   scale: scaling factor for positions
//   offset: position offset for KV cache scenarios
// Returns: tensor with RoPE applied
mlx_array_t* mlx_rope(
    mlx_array_t* array,
    int dims,
    bool traditional,
    float base,
    float scale,
    int offset
);

// ============================================================================
// Token generation / sampling
// ============================================================================

// Sampler configuration
typedef struct mlx_sampler_config {
    float temperature;      // Sampling temperature (0.0 = greedy)
    float top_p;           // Top-p (nucleus) sampling threshold
    int top_k;             // Top-k sampling limit (0 = disabled)
    float repetition_penalty; // Penalty for repeated tokens
    uint64_t seed;         // Random seed for reproducibility
} mlx_sampler_config_t;

// Sample a token from logits
// Parameters:
//   logits: model output logits [vocab_size]
//   config: sampling configuration
// Returns: sampled token index
int mlx_sample_token(mlx_array_t* logits, const mlx_sampler_config_t* config);

// ============================================================================
// KV Cache management
// ============================================================================

// KV Cache handle for efficient autoregressive generation
typedef struct mlx_kv_cache mlx_kv_cache_t;

// Create a new KV cache
// Parameters:
//   num_layers: number of transformer layers
//   num_heads: number of attention heads
//   head_dim: dimension per head
//   max_seq_len: maximum sequence length to cache
// Returns: KV cache handle (must be freed with mlx_kv_cache_free)
mlx_kv_cache_t* mlx_kv_cache_new(int num_layers, int num_heads, int head_dim, int max_seq_len);

// Update KV cache with new key/value tensors
// Parameters:
//   cache: KV cache handle
//   layer_idx: layer index to update
//   keys: new key tensor to append
//   values: new value tensor to append
// Returns: 0 on success, -1 on error
int mlx_kv_cache_update(mlx_kv_cache_t* cache, int layer_idx, mlx_array_t* keys, mlx_array_t* values);

// Get cached keys for a layer
mlx_array_t* mlx_kv_cache_get_keys(mlx_kv_cache_t* cache, int layer_idx);

// Get cached values for a layer
mlx_array_t* mlx_kv_cache_get_values(mlx_kv_cache_t* cache, int layer_idx);

// Get current sequence length in cache
int mlx_kv_cache_seq_len(mlx_kv_cache_t* cache);

// Reset/clear the KV cache
void mlx_kv_cache_reset(mlx_kv_cache_t* cache);

// Free KV cache
void mlx_kv_cache_free(mlx_kv_cache_t* cache);

// ============================================================================
// SafeTensors weight loading
// ============================================================================

// Weights container handle
typedef struct mlx_weights mlx_weights_t;

// Load weights from a SafeTensors file
// Parameters:
//   path: path to .safetensors file
// Returns: weights handle (must be freed with mlx_weights_free)
mlx_weights_t* mlx_load_safetensors(const char* path);

// Get a specific tensor by name from loaded weights
// Parameters:
//   weights: weights handle
//   name: tensor name (e.g., "model.layers.0.self_attn.q_proj.weight")
// Returns: tensor array (NULL if not found)
mlx_array_t* mlx_weights_get(mlx_weights_t* weights, const char* name);

// Get list of all tensor names
// Parameters:
//   weights: weights handle
//   names: output array of string pointers (caller allocates)
//   max_names: maximum number of names to return
// Returns: actual number of tensor names
int mlx_weights_list(mlx_weights_t* weights, const char** names, int max_names);

// Free weights container
void mlx_weights_free(mlx_weights_t* weights);

// ============================================================================
// Evaluation and synchronization
// ============================================================================

// Evaluate a single array (force computation)
// MLX uses lazy evaluation; call this to materialize results
void mlx_eval(mlx_array_t* array);

// Evaluate multiple arrays
// Parameters:
//   arrays: array of mlx_array_t pointers
//   num_arrays: number of arrays to evaluate
void mlx_eval_all(mlx_array_t** arrays, int num_arrays);

// Synchronize and wait for all GPU operations to complete
void mlx_synchronize(void);

// ============================================================================
// LoRA Adapter Caching
// ============================================================================

// Cache LoRA adapter weights for efficient reuse
// Parameters:
//   adapter_id: unique identifier for the adapter
//   lora_a: LoRA A matrix (down-projection)
//   lora_b: LoRA B matrix (up-projection)
// Returns: adapter_id on success, NULL on failure
const char* mlx_lora_cache_adapter(const char* adapter_id, mlx_array_t* lora_a, mlx_array_t* lora_b);

// Get cached LoRA adapter
// Parameters:
//   adapter_id: adapter identifier
//   out_lora_a: output pointer for LoRA A matrix
//   out_lora_b: output pointer for LoRA B matrix
// Returns: true if found, false otherwise
bool mlx_lora_get_cached(const char* adapter_id, mlx_array_t** out_lora_a, mlx_array_t** out_lora_b);

// Evict a specific adapter from cache
void mlx_lora_evict_cached(const char* adapter_id);

// Clear all cached adapters
void mlx_lora_clear_cache(void);

// Get number of cached adapters
size_t mlx_lora_cache_size(void);

// Set maximum number of cached adapters (default: 32)
void mlx_lora_set_cache_limit(size_t max_entries);

#ifdef __cplusplus
}
#endif
