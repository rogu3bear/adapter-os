// MLX C++ wrapper implementation (Real)
// Provides C-compatible interface for MLX functionality using real MLX C++ API

#include "wrapper.h"
#include <memory>
#include <string>
#include <vector>
#include <unordered_map>
#include <set>
#include <iostream>
#include <cstring>
#include <cstdlib>
#include <fstream>
#include <atomic>
#include <mutex>
#include <cstdint>

// Only compile with real MLX if MLX_REAL is defined (set by build.rs)
#ifdef MLX_REAL

// Real MLX headers
#include <mlx/mlx.h>
#include <mlx/ops.h>
#include <mlx/array.h>
#include <mlx/random.h>
#include <mlx/io.h>
#include <mlx/fast.h>
#include <mlx/backend/metal/metal.h>

namespace mx = mlx::core;

// Global error state
static thread_local std::string g_last_error;

// Memory tracking state
static std::atomic<size_t> g_total_memory_used(0);      // Total bytes allocated
static std::atomic<size_t> g_allocation_count(0);        // Total allocations
static std::mutex g_memory_mutex;                         // Lock for tracking updates
static std::unordered_map<uintptr_t, size_t> g_allocation_map;  // Track individual allocations

// Runtime state
static std::atomic<bool> g_initialized(false);
static mlx_device_type_t g_current_device_type = MLX_DEVICE_AUTO;

// LoRA adapter cache
struct LoRACacheEntry {
    mx::array lora_a;
    mx::array lora_b;
    uint64_t last_access;  // For LRU eviction

    // Default constructor required for unordered_map
    LoRACacheEntry() : lora_a(mx::array(0.0f)), lora_b(mx::array(0.0f)), last_access(0) {}

    // Constructor with values
    LoRACacheEntry(mx::array a, mx::array b, uint64_t access)
        : lora_a(std::move(a)), lora_b(std::move(b)), last_access(access) {}
};
static std::mutex g_lora_cache_mutex;
static std::unordered_map<std::string, LoRACacheEntry> g_lora_cache;
static size_t g_lora_cache_limit = 32;
static uint64_t g_lora_access_counter = 0;

/// Calculate bytes used by an MLX array dtype
static inline size_t get_dtype_size(mx::Dtype dtype) {
    if (dtype == mx::float32) return sizeof(float);
    if (dtype == mx::float16) return 2;
    if (dtype == mx::int32) return sizeof(int32_t);
    if (dtype == mx::uint32) return sizeof(uint32_t);
    return 1; // Default fallback
}

/// Calculate total memory used by an MLX array
static inline size_t calculate_array_memory(const mx::array& arr) {
    try {
        size_t element_count = arr.size();
        size_t dtype_size = get_dtype_size(arr.dtype());
        return element_count * dtype_size;
    } catch (...) {
        return 0;
    }
}

/// Record allocation
static inline void record_allocation(uintptr_t ptr, size_t bytes) {
    if (bytes > 0) {
        std::lock_guard<std::mutex> lock(g_memory_mutex);
        g_allocation_map[ptr] = bytes;
        g_total_memory_used.fetch_add(bytes, std::memory_order_relaxed);
        g_allocation_count.fetch_add(1, std::memory_order_relaxed);
    }
}

/// Unrecord deallocation
static inline void unrecord_allocation(uintptr_t ptr) {
    std::lock_guard<std::mutex> lock(g_memory_mutex);
    auto it = g_allocation_map.find(ptr);
    if (it != g_allocation_map.end()) {
        size_t bytes = it->second;
        g_allocation_map.erase(it);
        g_total_memory_used.fetch_sub(bytes, std::memory_order_relaxed);
    }
}

// Helper function to create Shape from dimensions
inline mx::Shape make_shape(int32_t size) {
    mx::Shape shape;
    shape.push_back(size);
    return shape;
}

inline mx::Shape make_shape(int32_t d1, int32_t d2) {
    mx::Shape shape;
    shape.push_back(d1);
    shape.push_back(d2);
    return shape;
}

inline mx::Shape make_shape(int32_t d1, int32_t d2, int32_t d3) {
    mx::Shape shape;
    shape.push_back(d1);
    shape.push_back(d2);
    shape.push_back(d3);
    return shape;
}

inline mx::Shape make_shape(int32_t d1, int32_t d2, int32_t d3, int32_t d4) {
    mx::Shape shape;
    shape.push_back(d1);
    shape.push_back(d2);
    shape.push_back(d3);
    shape.push_back(d4);
    return shape;
}

// GELU activation function implementation
inline mx::array mlx_gelu_approx(const mx::array& x) {
    // GELU(x) = x * 0.5 * (1 + tanh(sqrt(2/pi) * (x + 0.044715 * x^3)))
    // Simplified approximation: x * sigmoid(1.702 * x)
    return mx::multiply(x, mx::sigmoid(mx::multiply(x, mx::array(1.702f))));
}

// Wrapper structure for MLX arrays
struct MLXArrayWrapper {
    mx::array arr;
    size_t allocated_bytes;  // Track bytes for this array

    explicit MLXArrayWrapper(const mx::array& a) : arr(a) {
        allocated_bytes = calculate_array_memory(arr);
        record_allocation(reinterpret_cast<uintptr_t>(this), allocated_bytes);
    }

    ~MLXArrayWrapper() {
        unrecord_allocation(reinterpret_cast<uintptr_t>(this));
    }
};

// Model wrapper for MLX modules
struct MLXModelWrapper {
    std::string model_path;
    std::unordered_map<std::string, mx::array> weights;  // Loaded weights
    std::vector<std::pair<std::string, mx::array>> hidden_states_vec;  // Use vector for hidden states
    size_t total_weight_bytes;  // Track total weight memory

    // Model architecture config (loaded from config.json)
    int num_attention_heads = 32;    // Q heads
    int num_key_value_heads = 32;    // KV heads (for GQA, may differ from Q heads)
    int hidden_size = 4096;
    int num_hidden_layers = 32;
    int intermediate_size = 11008;
    int vocab_size = 32000;
    int head_dim = 128;              // hidden_size / num_attention_heads
    bool config_loaded = false;

    explicit MLXModelWrapper(const std::string& path)
        : model_path(path), total_weight_bytes(0) {}

    // Load model configuration from config.json
    bool load_config() {
        std::string config_path = model_path + "/config.json";
        std::ifstream config_file(config_path);
        if (!config_file.good()) {
            // Config not found, use defaults
            std::cerr << "[MLX] Config file not found at " << config_path << ", using defaults" << std::endl;
            head_dim = hidden_size / num_attention_heads;
            return false;
        }

        try {
            // Simple JSON parsing for the fields we need
            std::string content((std::istreambuf_iterator<char>(config_file)),
                               std::istreambuf_iterator<char>());
            config_file.close();

            auto parse_int = [&content](const std::string& key) -> int {
                size_t pos = content.find("\"" + key + "\"");
                if (pos == std::string::npos) return -1;
                pos = content.find(":", pos);
                if (pos == std::string::npos) return -1;
                pos++;
                while (pos < content.size() && (content[pos] == ' ' || content[pos] == '\t')) pos++;
                int value = 0;
                bool negative = false;
                if (content[pos] == '-') { negative = true; pos++; }
                while (pos < content.size() && content[pos] >= '0' && content[pos] <= '9') {
                    value = value * 10 + (content[pos] - '0');
                    pos++;
                }
                return negative ? -value : value;
            };

            int val;
            if ((val = parse_int("num_attention_heads")) > 0) num_attention_heads = val;
            if ((val = parse_int("num_key_value_heads")) > 0) num_key_value_heads = val;
            if ((val = parse_int("hidden_size")) > 0) hidden_size = val;
            if ((val = parse_int("num_hidden_layers")) > 0) num_hidden_layers = val;
            if ((val = parse_int("intermediate_size")) > 0) intermediate_size = val;
            if ((val = parse_int("vocab_size")) > 0) vocab_size = val;

            // Calculate head_dim
            head_dim = hidden_size / num_attention_heads;

            std::cerr << "[MLX] Loaded config: heads=" << num_attention_heads
                      << ", kv_heads=" << num_key_value_heads
                      << ", hidden=" << hidden_size
                      << ", layers=" << num_hidden_layers
                      << ", head_dim=" << head_dim << std::endl;

            config_loaded = true;
            return true;
        } catch (const std::exception& e) {
            std::cerr << "[MLX] Failed to parse config.json: " << e.what() << std::endl;
            head_dim = hidden_size / num_attention_heads;
            return false;
        }
    }

    // Load weights from safetensors format (supports sharded models)
    bool load_weights() {
        try {
            // Load config first to get model architecture parameters
            load_config();

            // First check for sharded model (model.safetensors.index.json)
            std::string index_path = model_path + "/model.safetensors.index.json";
            std::ifstream index_file(index_path);

            if (index_file.good()) {
                // Sharded model - parse index and load all shards
                index_file.close();
                return load_sharded_weights(index_path);
            }
            index_file.close();

            // Check for single model file
            std::string safetensors_path = model_path + "/model.safetensors";

            // Try alternative naming if primary doesn't exist
            std::ifstream test_file(safetensors_path);
            if (!test_file.good()) {
                test_file.close();
                safetensors_path = model_path + "/pytorch_model.bin.safetensors";
                test_file.open(safetensors_path);
                if (!test_file.good()) {
                    g_last_error = "Model file not found: tried '" + model_path + "/model.safetensors', '" + model_path + "/pytorch_model.bin.safetensors', and '" + model_path + "/model.safetensors.index.json'";
                    return false;
                }
            }
            test_file.close();

            // Load safetensors using MLX
            auto [loaded_weights, metadata] = mx::load_safetensors(safetensors_path);
            weights = std::move(loaded_weights);

            // Validate that we have required keys
            if (weights.empty()) {
                g_last_error = "No weights loaded from safetensors file";
                return false;
            }

            // Calculate and track memory usage for loaded weights
            total_weight_bytes = 0;
            for (const auto& [name, arr] : weights) {
                size_t bytes = calculate_array_memory(arr);
                total_weight_bytes += bytes;
            }
            record_allocation(reinterpret_cast<uintptr_t>(this), total_weight_bytes);

            return true;
        } catch (const std::exception& e) {
            g_last_error = std::string("Failed to load weights: ") + e.what();
            return false;
        }
    }

    // Load weights from sharded safetensors files
    // Parses the index JSON to find all shard files and merges them
    bool load_sharded_weights(const std::string& index_path) {
        try {
            // Read the index file
            std::ifstream file(index_path);
            if (!file.good()) {
                g_last_error = "Cannot open sharded model index: " + index_path;
                return false;
            }

            std::string content((std::istreambuf_iterator<char>(file)),
                                std::istreambuf_iterator<char>());
            file.close();

            // Simple JSON parsing to extract unique shard filenames from weight_map
            // Format: "weight_map": { "name": "model-00001-of-00003.safetensors", ... }
            std::set<std::string> shard_files;

            // Find all occurrences of "model-XXXXX-of-XXXXX.safetensors"
            std::string search_prefix = "model-";
            std::string search_suffix = ".safetensors";
            size_t pos = 0;

            while ((pos = content.find(search_prefix, pos)) != std::string::npos) {
                // Find the end of the filename
                size_t suffix_pos = content.find(search_suffix, pos);
                if (suffix_pos == std::string::npos) {
                    pos++;
                    continue;
                }

                // Extract the filename
                std::string filename = content.substr(pos, suffix_pos - pos + search_suffix.length());

                // Validate it looks like a shard filename (model-NNNNN-of-NNNNN.safetensors)
                if (filename.length() > 25 && filename.find("-of-") != std::string::npos) {
                    shard_files.insert(filename);
                }

                pos = suffix_pos + 1;
            }

            if (shard_files.empty()) {
                g_last_error = "No shard files found in index: " + index_path;
                return false;
            }

            std::cout << "[MLX] Loading sharded model with " << shard_files.size() << " shards..." << std::endl;

            // Load each shard and merge weights
            total_weight_bytes = 0;
            int shard_num = 0;

            for (const auto& shard_filename : shard_files) {
                shard_num++;
                std::string shard_path = model_path + "/" + shard_filename;

                std::cout << "[MLX] Loading shard " << shard_num << "/" << shard_files.size()
                          << ": " << shard_filename << std::endl;

                // Check file exists
                std::ifstream test_file(shard_path);
                if (!test_file.good()) {
                    g_last_error = "Shard file not found: " + shard_path;
                    return false;
                }
                test_file.close();

                // Load the shard
                auto [shard_weights, metadata] = mx::load_safetensors(shard_path);

                // Merge weights into main weights map
                for (auto& [name, arr] : shard_weights) {
                    size_t bytes = calculate_array_memory(arr);
                    total_weight_bytes += bytes;
                    weights.insert_or_assign(name, std::move(arr));
                }

                std::cout << "[MLX] Loaded " << shard_weights.size() << " weights from shard " << shard_num << std::endl;
            }

            // Track memory allocation
            record_allocation(reinterpret_cast<uintptr_t>(this), total_weight_bytes);

            std::cout << "[MLX] Successfully loaded " << weights.size() << " total weights ("
                      << (total_weight_bytes / (1024 * 1024)) << " MB)" << std::endl;

            return true;
        } catch (const std::exception& e) {
            g_last_error = std::string("Failed to load sharded weights: ") + e.what();
            return false;
        }
    }

    // Destructor to clean up tracked memory
    ~MLXModelWrapper() {
        unrecord_allocation(reinterpret_cast<uintptr_t>(this));
    }

    // Helper to find weight by name (tries multiple naming conventions)
    mx::array* find_weight(const std::string& name) {
        // Direct lookup
        auto it = weights.find(name);
        if (it != weights.end()) {
            return &it->second;
        }

        // Try common naming variations
        std::vector<std::string> alternatives;
        if (name == "token_embeddings.weight") {
            alternatives = {"model.embed_tokens.weight", "embeddings.word_embeddings.weight"};
        } else if (name == "output.weight") {
            alternatives = {"lm_head.weight", "output_projection.weight"};
        }

        for (const auto& alt : alternatives) {
            it = weights.find(alt);
            if (it != weights.end()) {
                return &it->second;
            }
        }

        return nullptr;
    }

    // Real transformer forward pass
    mx::array forward(const mx::array& input_ids) {
        try {
            // Get embedding weights
            auto embed_weight_ptr = find_weight("model.embed_tokens.weight");
            if (!embed_weight_ptr) {
                throw std::runtime_error("Embedding weights not found");
            }

            // Embedding lookup: [batch_size, seq_len] -> [batch_size, seq_len, hidden_size]
            mx::array hidden = mx::take(*embed_weight_ptr, input_ids, 0);

            // Process through transformer layers (simplified single layer)
            hidden = process_transformer_layer(hidden, 0);

            // Final layer norm (simplified)
            auto ln_weight_ptr = find_weight("model.norm.weight");
            if (ln_weight_ptr) {
                // Simple layer norm: (x - mean) / sqrt(var + eps) * weight + bias
                auto mean_val = mx::mean(hidden, -1, true);
                auto var_val = mx::var(hidden, -1, true);
                mx::array eps_const = mx::array(1e-5f);
                hidden = mx::multiply(*ln_weight_ptr, mx::divide(mx::subtract(hidden, mean_val), mx::sqrt(mx::add(var_val, eps_const))));
            }

            // Language modeling head
            auto lm_head_ptr = find_weight("lm_head.weight");
            if (!lm_head_ptr) {
                throw std::runtime_error("LM head weights not found");
            }

            // Project to vocabulary: [batch_size, seq_len, hidden_size] -> [batch_size, seq_len, vocab_size]
            mx::array logits = mx::matmul(hidden, mx::transpose(*lm_head_ptr));

            return logits;
        } catch (const std::exception& e) {
            g_last_error = std::string("Forward pass failed: ") + e.what();
            throw;
        }
    }

    // Basic transformer layer processing (simplified single layer)
    mx::array process_transformer_layer(const mx::array& hidden, int layer_idx) {
        std::string prefix = "model.layers." + std::to_string(layer_idx);

        // Self-attention
        mx::array attn_output = self_attention(hidden, prefix + ".self_attn");

        // Residual connection
        mx::array residual = hidden + attn_output;

        // Layer norm
        residual = layer_norm(residual, prefix + ".input_layernorm");

        // MLP
        mx::array mlp_output = mlp_forward(residual, prefix + ".mlp");

        // Final residual
        return residual + mlp_output;
    }

    // Self-attention with hidden state capture using scaled dot-product attention
    // Supports Grouped Query Attention (GQA) where num_key_value_heads < num_attention_heads
    mx::array self_attention_with_hidden_states(const mx::array& hidden, const std::string& prefix) {
        int batch_size = hidden.shape(0);
        int seq_len = hidden.shape(1);

        // Use config values for head counts (supports GQA)
        int n_heads = num_attention_heads;      // Q heads
        int n_kv_heads = num_key_value_heads;   // KV heads (may be fewer for GQA)
        int hd = head_dim;                       // head dimension
        int n_rep = n_heads / n_kv_heads;        // GQA repetition factor

        // QKV projections
        mx::array q = linear_projection(hidden, prefix + ".q_proj");
        mx::array k = linear_projection(hidden, prefix + ".k_proj");
        mx::array v = linear_projection(hidden, prefix + ".v_proj");

        // Capture QKV projections as hidden states
        mx::eval(q);
        hidden_states_vec.push_back({prefix + ".q_proj", q});
        mx::eval(k);
        hidden_states_vec.push_back({prefix + ".k_proj", k});
        mx::eval(v);
        hidden_states_vec.push_back({prefix + ".v_proj", v});

        // Reshape Q for multi-head attention: [batch, seq, n_heads * head_dim] -> [batch, seq, n_heads, head_dim]
        q = mx::reshape(q, {batch_size, seq_len, n_heads, hd});

        // Reshape K,V for GQA: [batch, seq, n_kv_heads * head_dim] -> [batch, seq, n_kv_heads, head_dim]
        k = mx::reshape(k, {batch_size, seq_len, n_kv_heads, hd});
        v = mx::reshape(v, {batch_size, seq_len, n_kv_heads, hd});

        // GQA: Repeat K,V heads to match Q heads if needed
        if (n_rep > 1) {
            // Expand K: [batch, seq, n_kv_heads, head_dim] -> [batch, seq, n_kv_heads, n_rep, head_dim]
            k = mx::expand_dims(k, 3);
            k = mx::repeat(k, n_rep, 3);
            k = mx::reshape(k, {batch_size, seq_len, n_heads, hd});

            // Expand V: same transformation
            v = mx::expand_dims(v, 3);
            v = mx::repeat(v, n_rep, 3);
            v = mx::reshape(v, {batch_size, seq_len, n_heads, hd});
        }

        // Transpose for attention: [batch, seq, heads, head_dim] -> [batch, heads, seq, head_dim]
        q = mx::transpose(q, {0, 2, 1, 3});
        k = mx::transpose(k, {0, 2, 1, 3});
        v = mx::transpose(v, {0, 2, 1, 3});

        // Create causal mask for autoregressive attention
        std::vector<float> mask_data(seq_len * seq_len, 0.0f);
        for (int i = 0; i < seq_len; ++i) {
            for (int j = i + 1; j < seq_len; ++j) {
                mask_data[i * seq_len + j] = -1e9f;
            }
        }
        mx::array causal_mask = mx::array(mask_data.data(), {seq_len, seq_len}, mx::float32);

        // Scaled dot-product attention: softmax(Q @ K^T * scale + mask) @ V
        float scale = 1.0f / std::sqrt(static_cast<float>(hd));
        mx::array k_transposed = mx::transpose(k, {0, 1, 3, 2});
        mx::array scores = mx::matmul(q, k_transposed);
        scores = mx::multiply(scores, mx::array(scale));
        scores = mx::add(scores, causal_mask);
        mx::array attn_weights = mx::softmax(scores, -1);
        mx::array attn_output = mx::matmul(attn_weights, v);

        // Reshape back: [batch, heads, seq, head_dim] -> [batch, seq, hidden_size]
        attn_output = mx::transpose(attn_output, {0, 2, 1, 3});
        attn_output = mx::reshape(attn_output, {batch_size, seq_len, n_heads * hd});

        // Output projection
        mx::array output = linear_projection(attn_output, prefix + ".o_proj");

        // Capture attention output as hidden state
        mx::eval(output);
        hidden_states_vec.push_back({prefix + ".o_proj", output});

        return output;
    }

    // Self-attention mechanism using scaled dot-product attention
    // Supports Grouped Query Attention (GQA) where num_key_value_heads < num_attention_heads
    mx::array self_attention(const mx::array& hidden, const std::string& prefix) {
        int batch_size = hidden.shape(0);
        int seq_len = hidden.shape(1);

        // Use config values for head counts (supports GQA)
        int n_heads = num_attention_heads;      // Q heads
        int n_kv_heads = num_key_value_heads;   // KV heads (may be fewer for GQA)
        int hd = head_dim;                       // head dimension
        int n_rep = n_heads / n_kv_heads;        // GQA repetition factor

        // QKV projections
        mx::array q = linear_projection(hidden, prefix + ".q_proj");
        mx::array k = linear_projection(hidden, prefix + ".k_proj");
        mx::array v = linear_projection(hidden, prefix + ".v_proj");

        // Reshape Q for multi-head attention: [batch, seq, n_heads * head_dim] -> [batch, seq, n_heads, head_dim]
        q = mx::reshape(q, {batch_size, seq_len, n_heads, hd});

        // Reshape K,V for GQA: [batch, seq, n_kv_heads * head_dim] -> [batch, seq, n_kv_heads, head_dim]
        k = mx::reshape(k, {batch_size, seq_len, n_kv_heads, hd});
        v = mx::reshape(v, {batch_size, seq_len, n_kv_heads, hd});

        // GQA: Repeat K,V heads to match Q heads if needed
        if (n_rep > 1) {
            // Expand K: [batch, seq, n_kv_heads, head_dim] -> [batch, seq, n_kv_heads, n_rep, head_dim]
            k = mx::expand_dims(k, 3);
            k = mx::repeat(k, n_rep, 3);
            k = mx::reshape(k, {batch_size, seq_len, n_heads, hd});

            // Expand V: same transformation
            v = mx::expand_dims(v, 3);
            v = mx::repeat(v, n_rep, 3);
            v = mx::reshape(v, {batch_size, seq_len, n_heads, hd});
        }

        // Transpose for attention: [batch, seq, heads, head_dim] -> [batch, heads, seq, head_dim]
        q = mx::transpose(q, {0, 2, 1, 3});
        k = mx::transpose(k, {0, 2, 1, 3});
        v = mx::transpose(v, {0, 2, 1, 3});

        // Create causal mask for autoregressive attention
        std::vector<float> mask_data(seq_len * seq_len, 0.0f);
        for (int i = 0; i < seq_len; ++i) {
            for (int j = i + 1; j < seq_len; ++j) {
                mask_data[i * seq_len + j] = -1e9f;
            }
        }
        mx::array causal_mask = mx::array(mask_data.data(), {seq_len, seq_len}, mx::float32);

        // Scaled dot-product attention: softmax(Q @ K^T * scale + mask) @ V
        float scale = 1.0f / std::sqrt(static_cast<float>(hd));
        mx::array k_transposed = mx::transpose(k, {0, 1, 3, 2});
        mx::array scores = mx::matmul(q, k_transposed);
        scores = mx::multiply(scores, mx::array(scale));
        scores = mx::add(scores, causal_mask);
        mx::array attn_weights = mx::softmax(scores, -1);
        mx::array attn_output = mx::matmul(attn_weights, v);

        // Reshape back: [batch, heads, seq, head_dim] -> [batch, seq, hidden_size]
        attn_output = mx::transpose(attn_output, {0, 2, 1, 3});
        attn_output = mx::reshape(attn_output, {batch_size, seq_len, n_heads * hd});

        // Output projection
        return linear_projection(attn_output, prefix + ".o_proj");
    }

    // Linear projection helper
    mx::array linear_projection(const mx::array& input, const std::string& weight_key) {
        auto weight_ptr = find_weight(weight_key + ".weight");
        if (!weight_ptr) return input;  // Fallback if weight not found

        return mx::matmul(input, mx::transpose(*weight_ptr));
    }

    // Layer normalization
    mx::array layer_norm(const mx::array& input, const std::string& prefix) {
        auto weight_ptr = find_weight(prefix + ".weight");
        auto bias_ptr = find_weight(prefix + ".bias");

        if (!weight_ptr) return input;

        // RMSNorm: y = x / sqrt(mean(x^2) + eps) * weight
        mx::array eps_arr = mx::array(1e-5f);
        mx::array squared = mx::multiply(input, input);
        mx::array mean_sq = mx::mean(squared, -1, true);  // keepdims
        mx::array rms = mx::sqrt(mx::add(mean_sq, eps_arr));
        mx::array normalized = mx::divide(input, rms);
        mx::array output = mx::multiply(normalized, *weight_ptr);

        if (bias_ptr) {
            output = mx::add(output, *bias_ptr);
        }

        return output;
    }

    // MLP forward pass
    mx::array mlp_forward(const mx::array& input, const std::string& prefix) {
        // Up projection
        mx::array up = linear_projection(input, prefix + ".up_proj");
        up = mlx_gelu_approx(up);

        // Gate projection (simplified - would use silu activation)
        mx::array gate = linear_projection(input, prefix + ".gate_proj");
        up = mx::multiply(up, gate);  // Element-wise gating

        // Down projection
        return linear_projection(up, prefix + ".down_proj");
    }

    // Forward pass with hidden state capture
    mx::array forward_with_hidden_states(const mx::array& input_ids) {
        hidden_states_vec.clear();

        try {
            // Get embedding weights
            auto embed_weight_ptr = find_weight("model.embed_tokens.weight");
            if (!embed_weight_ptr) {
                throw std::runtime_error("Embedding weights not found");
            }

            // Embedding lookup
            mx::array hidden = mx::take(*embed_weight_ptr, input_ids, 0);
            mx::eval(hidden);
            hidden_states_vec.push_back({"embeddings", hidden});

            // Count actual number of transformer layers from loaded weights
            // Look for model.layers.N.self_attn pattern
            int num_layers = 0;
            for (const auto& [name, _] : weights) {
                // Match pattern: model.layers.N.self_attn.q_proj.weight
                if (name.find("model.layers.") == 0 && name.find(".self_attn.q_proj.weight") != std::string::npos) {
                    // Extract layer number
                    size_t dot_pos = name.find('.', 13); // After "model.layers."
                    if (dot_pos != std::string::npos) {
                        int layer_num = std::stoi(name.substr(13, dot_pos - 13));
                        num_layers = std::max(num_layers, layer_num + 1);
                    }
                }
            }

            // Fallback: if no layers found, try alternative patterns or use default
            if (num_layers == 0) {
                // Try counting any layer pattern
                for (const auto& [name, _] : weights) {
                    if (name.find("model.layers.") == 0) {
                        size_t dot_pos = name.find('.', 13);
                        if (dot_pos != std::string::npos) {
                            try {
                                int layer_num = std::stoi(name.substr(13, dot_pos - 13));
                                num_layers = std::max(num_layers, layer_num + 1);
                            } catch (...) {
                                // Skip non-numeric layer names
                            }
                        }
                    }
                }
            }

            // Final fallback for dummy weights or minimal models
            if (num_layers == 0) {
                num_layers = 1;
            }

            for (int layer_idx = 0; layer_idx < num_layers; ++layer_idx) {
                // Capture pre-attention hidden state
                mx::eval(hidden);
                hidden_states_vec.push_back({std::string("layer_") + std::to_string(layer_idx) + "_pre_attn", hidden});

                // Self-attention with hidden state capture
                mx::array attn_output = self_attention_with_hidden_states(hidden, std::string("model.layers.") + std::to_string(layer_idx) + ".self_attn");

                // Residual + layer norm
                hidden = hidden + attn_output;
                hidden = layer_norm(hidden, std::string("model.layers.") + std::to_string(layer_idx) + ".input_layernorm");

                // Capture post-attention hidden state
                mx::eval(hidden);
                hidden_states_vec.push_back({std::string("layer_") + std::to_string(layer_idx) + "_post_attn", hidden});

                // MLP
                mx::array mlp_output = mlp_forward(hidden, std::string("model.layers.") + std::to_string(layer_idx) + ".mlp");
                hidden = hidden + mlp_output;

                // Capture post-MLP hidden state
                mx::eval(hidden);
                hidden_states_vec.push_back({std::string("layer_") + std::to_string(layer_idx) + "_output", hidden});
            }

            // Final layer norm
            auto ln_weight_ptr = find_weight("model.norm.weight");
            if (ln_weight_ptr) {
                hidden = layer_norm(hidden, "model.norm");
            }

            // Language modeling head
            auto lm_head_ptr = find_weight("lm_head.weight");
            if (!lm_head_ptr) {
                throw std::runtime_error("LM head weights not found");
            }

            mx::array logits = mx::matmul(hidden, mx::transpose(*lm_head_ptr));
            return logits;
        } catch (const std::exception& e) {
            g_last_error = std::string("Forward with hidden states failed: ") + e.what();
            throw;
        }
    }
};

// KV Cache wrapper for efficient autoregressive generation
struct MLXKVCache {
    int num_layers;
    int num_heads;
    int head_dim;
    int max_seq_len;
    int current_seq_len;
    std::vector<mx::array> keys;    // Per-layer key cache
    std::vector<mx::array> values;  // Per-layer value cache

    MLXKVCache(int layers, int heads, int dim, int max_len)
        : num_layers(layers), num_heads(heads), head_dim(dim),
          max_seq_len(max_len), current_seq_len(0) {
        // Initialize empty caches for each layer
        keys.reserve(layers);
        values.reserve(layers);
        for (int i = 0; i < layers; ++i) {
            // Initialize with empty arrays (will be populated on first update)
            keys.push_back(mx::zeros({1, heads, 0, dim}, mx::float32));
            values.push_back(mx::zeros({1, heads, 0, dim}, mx::float32));
        }
    }

    bool update(int layer_idx, const mx::array& new_keys, const mx::array& new_values) {
        if (layer_idx < 0 || layer_idx >= num_layers) {
            return false;
        }

        // Concatenate new keys/values along sequence dimension (axis 2)
        if (keys[layer_idx].shape(2) == 0) {
            // First update for this layer
            keys[layer_idx] = new_keys;
            values[layer_idx] = new_values;
        } else {
            keys[layer_idx] = mx::concatenate({keys[layer_idx], new_keys}, 2);
            values[layer_idx] = mx::concatenate({values[layer_idx], new_values}, 2);
        }

        // Update sequence length (use layer 0 as reference)
        if (layer_idx == 0) {
            current_seq_len = keys[0].shape(2);
        }

        return true;
    }

    void reset() {
        keys.clear();
        values.clear();
        keys.reserve(num_layers);
        values.reserve(num_layers);
        for (int i = 0; i < num_layers; ++i) {
            keys.push_back(mx::zeros({1, num_heads, 0, head_dim}, mx::float32));
            values.push_back(mx::zeros({1, num_heads, 0, head_dim}, mx::float32));
        }
        current_seq_len = 0;
    }
};

// Weights container for SafeTensors loading
struct MLXWeightsWrapper {
    std::unordered_map<std::string, mx::array> weights;
    std::vector<std::string> weight_names;  // For iteration

    bool load(const std::string& path) {
        try {
            auto [loaded_weights, metadata] = mx::load_safetensors(path);
            weights = std::move(loaded_weights);

            // Build name list for iteration
            weight_names.clear();
            for (const auto& [name, _] : weights) {
                weight_names.push_back(name);
            }

            return !weights.empty();
        } catch (const std::exception& e) {
            g_last_error = std::string("Failed to load safetensors: ") + e.what();
            return false;
        }
    }

    mx::array* get(const std::string& name) {
        auto it = weights.find(name);
        if (it != weights.end()) {
            return &it->second;
        }
        return nullptr;
    }
};

// Context management
extern "C" mlx_context_t* mlx_context_new(void) {
    try {
        // MLX doesn't have explicit context management like CUDA
        // We'll use a dummy context for API compatibility
        auto ctx = new int(1);
        return reinterpret_cast<mlx_context_t*>(ctx);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

extern "C" void mlx_context_free(mlx_context_t* ctx) {
    if (ctx) {
        delete reinterpret_cast<int*>(ctx);
    }
}

extern "C" void mlx_set_default_context(mlx_context_t* ctx) {
    // MLX uses global context
    (void)ctx;
}

// Array creation operations
extern "C" mlx_array_t* mlx_array_from_data(const float* data, int size) {
    try {
        // Copy data into vector and construct array using iterator
        std::vector<float> vec(data, data + size);
        mx::array arr = mx::array(vec.begin(), make_shape(size), mx::float32);
        auto wrapper = new MLXArrayWrapper(arr);
        return reinterpret_cast<mlx_array_t*>(wrapper);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

extern "C" mlx_array_t* mlx_array_from_ints(const int* data, int size) {
    try {
        // Copy data into vector and construct array using iterator
        std::vector<int> vec(data, data + size);
        mx::array arr = mx::array(vec.begin(), make_shape(size), mx::int32);
        auto wrapper = new MLXArrayWrapper(arr);
        return reinterpret_cast<mlx_array_t*>(wrapper);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

extern "C" mlx_array_t* mlx_array_from_uints(const uint32_t* data, int size) {
    try {
        // Copy data into vector and construct array using iterator
        std::vector<uint32_t> vec(data, data + size);
        mx::array arr = mx::array(vec.begin(), make_shape(size), mx::uint32);
        auto wrapper = new MLXArrayWrapper(arr);
        return reinterpret_cast<mlx_array_t*>(wrapper);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

extern "C" mlx_array_t* mlx_array_zeros(int size) {
    try {
        mx::array arr = mx::zeros({size}, mx::float32);
        auto wrapper = new MLXArrayWrapper(arr);
        return reinterpret_cast<mlx_array_t*>(wrapper);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

extern "C" mlx_array_t* mlx_array_ones(int size) {
    try {
        mx::array arr = mx::ones({size}, mx::float32);
        auto wrapper = new MLXArrayWrapper(arr);
        return reinterpret_cast<mlx_array_t*>(wrapper);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

extern "C" mlx_array_t* mlx_array_full(int size, float value) {
    try {
        mx::array arr = mx::full({size}, value, mx::float32);
        auto wrapper = new MLXArrayWrapper(arr);
        return reinterpret_cast<mlx_array_t*>(wrapper);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

// Array property access
extern "C" float* mlx_array_data(mlx_array_t* array) {
    if (!array) return nullptr;
    try {
        auto wrapper = reinterpret_cast<MLXArrayWrapper*>(array);
        // Force evaluation and get data pointer
        mx::eval(wrapper->arr);
        return static_cast<float*>(wrapper->arr.data<float>());
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

extern "C" int mlx_array_size(mlx_array_t* array) {
    if (!array) return 0;
    try {
        auto wrapper = reinterpret_cast<MLXArrayWrapper*>(array);
        return wrapper->arr.size();
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return 0;
    }
}

extern "C" int mlx_array_shape(mlx_array_t* array, int* shape, int max_dims) {
    if (!array || !shape) return 0;
    try {
        auto wrapper = reinterpret_cast<MLXArrayWrapper*>(array);
        auto arr_shape = wrapper->arr.shape();
        int ndims = std::min(static_cast<int>(arr_shape.size()), max_dims);
        for (int i = 0; i < ndims; ++i) {
            shape[i] = arr_shape[i];
        }
        return ndims;
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return 0;
    }
}

extern "C" int mlx_array_ndim(mlx_array_t* array) {
    if (!array) return 0;
    try {
        auto wrapper = reinterpret_cast<MLXArrayWrapper*>(array);
        return wrapper->arr.ndim();
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return 0;
    }
}

extern "C" int mlx_array_dtype(mlx_array_t* array) {
    if (!array) return 0;
    try {
        auto wrapper = reinterpret_cast<MLXArrayWrapper*>(array);
        // Map MLX dtype to integer code
        if (wrapper->arr.dtype() == mx::float32) return 0;
        if (wrapper->arr.dtype() == mx::float16) return 1;
        if (wrapper->arr.dtype() == mx::int32) return 2;
        if (wrapper->arr.dtype() == mx::uint32) return 3;
        return -1;
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return -1;
    }
}

// Array operations
extern "C" mlx_array_t* mlx_array_copy(mlx_array_t* array) {
    if (!array) return nullptr;
    try {
        auto wrapper = reinterpret_cast<MLXArrayWrapper*>(array);
        mx::array copy = mx::copy(wrapper->arr);
        auto new_wrapper = new MLXArrayWrapper(copy);
        return reinterpret_cast<mlx_array_t*>(new_wrapper);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

extern "C" mlx_array_t* mlx_array_reshape(mlx_array_t* array, const int* shape, int ndim) {
    if (!array || !shape) return nullptr;
    try {
        auto wrapper = reinterpret_cast<MLXArrayWrapper*>(array);
        // For now, handle common cases directly
        if (ndim == 1) {
            mx::array reshaped = mx::reshape(wrapper->arr, {shape[0]});
            auto new_wrapper = new MLXArrayWrapper(reshaped);
            return reinterpret_cast<mlx_array_t*>(new_wrapper);
        } else if (ndim == 2) {
            mx::array reshaped = mx::reshape(wrapper->arr, {shape[0], shape[1]});
            auto new_wrapper = new MLXArrayWrapper(reshaped);
            return reinterpret_cast<mlx_array_t*>(new_wrapper);
        } else if (ndim == 3) {
            mx::array reshaped = mx::reshape(wrapper->arr, {shape[0], shape[1], shape[2]});
            auto new_wrapper = new MLXArrayWrapper(reshaped);
            return reinterpret_cast<mlx_array_t*>(new_wrapper);
        } else if (ndim == 4) {
            mx::array reshaped = mx::reshape(wrapper->arr, {shape[0], shape[1], shape[2], shape[3]});
            auto new_wrapper = new MLXArrayWrapper(reshaped);
            return reinterpret_cast<mlx_array_t*>(new_wrapper);
        } else {
            g_last_error = "Unsupported number of dimensions for reshape";
            return nullptr;
        }
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

extern "C" mlx_array_t* mlx_array_transpose(mlx_array_t* array) {
    if (!array) return nullptr;
    try {
        auto wrapper = reinterpret_cast<MLXArrayWrapper*>(array);
        mx::array transposed = mx::transpose(wrapper->arr);
        auto new_wrapper = new MLXArrayWrapper(transposed);
        return reinterpret_cast<mlx_array_t*>(new_wrapper);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

extern "C" void mlx_array_free(mlx_array_t* array) {
    if (array) {
        delete reinterpret_cast<MLXArrayWrapper*>(array);
    }
}

// Model operations
extern "C" mlx_model_t* mlx_model_load(const char* path) {
    if (!path) return nullptr;
    try {
        auto model = new MLXModelWrapper(std::string(path));
        if (!model->load_weights()) {
            delete model;
            return nullptr;
        }
        return reinterpret_cast<mlx_model_t*>(model);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

extern "C" mlx_model_t* mlx_model_load_from_buffer(const uint8_t* buffer, size_t buffer_len, const char* config_json) {
    if (!buffer || buffer_len < 4 || !config_json) {
        g_last_error = "Invalid buffer or config";
        return nullptr;
    }

    try {
        // Create model wrapper with empty path (loading from buffer)
        auto model = new MLXModelWrapper("");

        // Parse buffer format:
        // [0-3] num_tensors (u32 LE)
        // For each tensor:
        //   [4 bytes] name_len
        //   [name_len bytes] name
        //   [4 bytes] shape_len
        //   [shape_len * 4 bytes] shape dimensions
        //   [4 bytes] data_len
        //   [data_len * 4 bytes] f32 data

        size_t offset = 0;

        // Read number of tensors
        uint32_t num_tensors = 0;
        std::memcpy(&num_tensors, buffer + offset, 4);
        offset += 4;

        model->total_weight_bytes = 0;

        for (uint32_t i = 0; i < num_tensors && offset < buffer_len; ++i) {
            // Read tensor name
            if (offset + 4 > buffer_len) break;
            uint32_t name_len = 0;
            std::memcpy(&name_len, buffer + offset, 4);
            offset += 4;

            if (offset + name_len > buffer_len) break;
            std::string name(reinterpret_cast<const char*>(buffer + offset), name_len);
            offset += name_len;

            // Read shape
            if (offset + 4 > buffer_len) break;
            uint32_t shape_len = 0;
            std::memcpy(&shape_len, buffer + offset, 4);
            offset += 4;

            std::vector<int> shape(shape_len);
            if (offset + shape_len * 4 > buffer_len) break;
            for (uint32_t j = 0; j < shape_len; ++j) {
                uint32_t dim = 0;
                std::memcpy(&dim, buffer + offset, 4);
                shape[j] = static_cast<int>(dim);
                offset += 4;
            }

            // Read data
            if (offset + 4 > buffer_len) break;
            uint32_t data_len = 0;
            std::memcpy(&data_len, buffer + offset, 4);
            offset += 4;

            if (offset + data_len * 4 > buffer_len) break;

            // Create MLX array from data
            std::vector<float> data(data_len);
            std::memcpy(data.data(), buffer + offset, data_len * sizeof(float));
            offset += data_len * sizeof(float);

            // Convert shape to MLX Shape format
            mx::Shape mlx_shape;
            for (int dim : shape) {
                mlx_shape.push_back(static_cast<int32_t>(dim));
            }

            // Create MLX array from vector iterator
            mx::array arr = mx::array(data.begin(), mlx_shape, mx::float32);
            model->weights.insert_or_assign(name, std::move(arr));

            // Track memory
            size_t bytes = data_len * sizeof(float);
            model->total_weight_bytes += bytes;
        }

        // Track total allocation
        record_allocation(reinterpret_cast<uintptr_t>(model), model->total_weight_bytes);

        return reinterpret_cast<mlx_model_t*>(model);
    } catch (const std::exception& e) {
        g_last_error = std::string("Failed to load model from buffer: ") + e.what();
        return nullptr;
    }
}

extern "C" mlx_array_t* mlx_model_forward(mlx_model_t* model, mlx_array_t* input) {
    if (!model || !input) return nullptr;
    try {
        auto model_wrapper = reinterpret_cast<MLXModelWrapper*>(model);
        auto input_wrapper = reinterpret_cast<MLXArrayWrapper*>(input);

        mx::array output = model_wrapper->forward(input_wrapper->arr);
        mx::eval(output);  // Force evaluation

        auto result_wrapper = new MLXArrayWrapper(output);
        return reinterpret_cast<mlx_array_t*>(result_wrapper);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

extern "C" mlx_array_t* mlx_model_forward_with_hidden_states(
    mlx_model_t* model,
    mlx_array_t* input,
    mlx_array_t** hidden_states,
    int* num_hidden
) {
    if (!model || !input || !hidden_states || !num_hidden) return nullptr;
    try {
        auto model_wrapper = reinterpret_cast<MLXModelWrapper*>(model);
        auto input_wrapper = reinterpret_cast<MLXArrayWrapper*>(input);

        mx::array output = model_wrapper->forward_with_hidden_states(input_wrapper->arr);
        mx::eval(output);  // Force evaluation

        // Extract hidden states from model wrapper
        const auto& hidden_states_vec = model_wrapper->hidden_states_vec;
        *num_hidden = static_cast<int>(hidden_states_vec.size());

        if (*num_hidden > 0) {
            // Allocate array of hidden state pointers
            // IMPORTANT: Caller must free this array and each element
            mlx_array_t** hidden_array = new mlx_array_t*[*num_hidden];

            // Wrap each hidden state array
            for (int i = 0; i < *num_hidden; ++i) {
                auto wrapper = new MLXArrayWrapper(hidden_states_vec[i].second);
                hidden_array[i] = reinterpret_cast<mlx_array_t*>(wrapper);
            }

            *hidden_states = reinterpret_cast<mlx_array_t*>(hidden_array);
        } else {
            *hidden_states = nullptr;
        }

        auto result_wrapper = new MLXArrayWrapper(output);
        return reinterpret_cast<mlx_array_t*>(result_wrapper);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

extern "C" void mlx_model_free(mlx_model_t* model) {
    if (model) {
        auto wrapper = reinterpret_cast<MLXModelWrapper*>(model);
        // Destructor will clean up tracked memory
        delete wrapper;
    }
}

// Free hidden states array returned by mlx_model_forward_with_hidden_states
extern "C" void mlx_hidden_states_free(mlx_array_t* hidden_states, int num_hidden) {
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

// Hidden state names for the 4 target modules
static const char* g_hidden_state_names[] = {
    "layer.0.self_attn.q_proj",
    "layer.0.self_attn.k_proj",
    "layer.0.self_attn.v_proj",
    "layer.0.self_attn.o_proj"
};
static const int g_hidden_state_count = 4;

// Get the name of a hidden state at the given index
extern "C" int mlx_model_get_hidden_state_name(
    mlx_model_t* model,
    int index,
    char* out_name,
    int out_name_len
) {
    if (!model || index < 0 || index >= g_hidden_state_count) return 0;

    const char* name = g_hidden_state_names[index];
    int name_len = static_cast<int>(std::strlen(name));

    // If buffer provided and large enough, copy the name
    if (out_name && out_name_len > name_len) {
        std::memcpy(out_name, name, name_len + 1); // Include null terminator
    }

    return name_len;
}

// Get the number of hidden states stored in the model
extern "C" int mlx_model_get_hidden_state_count(mlx_model_t* model) {
    if (!model) return 0;
    return g_hidden_state_count;
}

// Mathematical operations
extern "C" mlx_array_t* mlx_add(mlx_array_t* a, mlx_array_t* b) {
    if (!a || !b) return nullptr;
    try {
        auto a_wrapper = reinterpret_cast<MLXArrayWrapper*>(a);
        auto b_wrapper = reinterpret_cast<MLXArrayWrapper*>(b);

        mx::array result = mx::add(a_wrapper->arr, b_wrapper->arr);
        auto result_wrapper = new MLXArrayWrapper(result);
        return reinterpret_cast<mlx_array_t*>(result_wrapper);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

extern "C" mlx_array_t* mlx_subtract(mlx_array_t* a, mlx_array_t* b) {
    if (!a || !b) return nullptr;
    try {
        auto a_wrapper = reinterpret_cast<MLXArrayWrapper*>(a);
        auto b_wrapper = reinterpret_cast<MLXArrayWrapper*>(b);

        mx::array result = mx::subtract(a_wrapper->arr, b_wrapper->arr);
        auto result_wrapper = new MLXArrayWrapper(result);
        return reinterpret_cast<mlx_array_t*>(result_wrapper);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

extern "C" mlx_array_t* mlx_multiply(mlx_array_t* a, mlx_array_t* b) {
    if (!a || !b) return nullptr;
    try {
        auto a_wrapper = reinterpret_cast<MLXArrayWrapper*>(a);
        auto b_wrapper = reinterpret_cast<MLXArrayWrapper*>(b);

        mx::array result = mx::multiply(a_wrapper->arr, b_wrapper->arr);
        auto result_wrapper = new MLXArrayWrapper(result);
        return reinterpret_cast<mlx_array_t*>(result_wrapper);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

extern "C" mlx_array_t* mlx_divide(mlx_array_t* a, mlx_array_t* b) {
    if (!a || !b) return nullptr;
    try {
        auto a_wrapper = reinterpret_cast<MLXArrayWrapper*>(a);
        auto b_wrapper = reinterpret_cast<MLXArrayWrapper*>(b);

        mx::array result = mx::divide(a_wrapper->arr, b_wrapper->arr);
        auto result_wrapper = new MLXArrayWrapper(result);
        return reinterpret_cast<mlx_array_t*>(result_wrapper);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

extern "C" mlx_array_t* mlx_matmul(mlx_array_t* a, mlx_array_t* b) {
    if (!a || !b) return nullptr;
    try {
        auto a_wrapper = reinterpret_cast<MLXArrayWrapper*>(a);
        auto b_wrapper = reinterpret_cast<MLXArrayWrapper*>(b);

        mx::array result = mx::matmul(a_wrapper->arr, b_wrapper->arr);
        auto result_wrapper = new MLXArrayWrapper(result);
        return reinterpret_cast<mlx_array_t*>(result_wrapper);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

// Reduction operations
extern "C" mlx_array_t* mlx_sum(mlx_array_t* array, int axis) {
    if (!array) return nullptr;
    try {
        auto wrapper = reinterpret_cast<MLXArrayWrapper*>(array);
        mx::array result = mx::sum(wrapper->arr, axis);
        auto result_wrapper = new MLXArrayWrapper(result);
        return reinterpret_cast<mlx_array_t*>(result_wrapper);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

extern "C" mlx_array_t* mlx_mean(mlx_array_t* array, int axis) {
    if (!array) return nullptr;
    try {
        auto wrapper = reinterpret_cast<MLXArrayWrapper*>(array);
        mx::array result = mx::mean(wrapper->arr, axis);
        auto result_wrapper = new MLXArrayWrapper(result);
        return reinterpret_cast<mlx_array_t*>(result_wrapper);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

extern "C" mlx_array_t* mlx_sqrt(mlx_array_t* array) {
    if (!array) return nullptr;
    try {
        auto wrapper = reinterpret_cast<MLXArrayWrapper*>(array);
        mx::array result = mx::sqrt(wrapper->arr);
        auto result_wrapper = new MLXArrayWrapper(result);
        return reinterpret_cast<mlx_array_t*>(result_wrapper);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

// Indexing operations
extern "C" mlx_array_t* mlx_take(mlx_array_t* array, mlx_array_t* indices, int axis) {
    if (!array || !indices) return nullptr;
    try {
        auto wrapper = reinterpret_cast<MLXArrayWrapper*>(array);
        auto idx_wrapper = reinterpret_cast<MLXArrayWrapper*>(indices);
        mx::array result = mx::take(wrapper->arr, idx_wrapper->arr, axis);
        auto result_wrapper = new MLXArrayWrapper(result);
        return reinterpret_cast<mlx_array_t*>(result_wrapper);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

// Activation functions
extern "C" mlx_array_t* mlx_relu(mlx_array_t* array) {
    if (!array) return nullptr;
    try {
        auto wrapper = reinterpret_cast<MLXArrayWrapper*>(array);
        mx::array result = mx::maximum(wrapper->arr, mx::array(0.0f));
        auto result_wrapper = new MLXArrayWrapper(result);
        return reinterpret_cast<mlx_array_t*>(result_wrapper);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

extern "C" mlx_array_t* mlx_gelu(mlx_array_t* array) {
    if (!array) return nullptr;
    try {
        auto wrapper = reinterpret_cast<MLXArrayWrapper*>(array);
        // GELU(x) = x * 0.5 * (1 + tanh(sqrt(2/pi) * (x + 0.044715 * x^3)))
        // Simplified approximation: x * sigmoid(1.702 * x)
        mx::array x = wrapper->arr;
        mx::array result = mx::multiply(x, mx::sigmoid(mx::multiply(x, mx::array(1.702f))));
        auto result_wrapper = new MLXArrayWrapper(result);
        return reinterpret_cast<mlx_array_t*>(result_wrapper);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

extern "C" mlx_array_t* mlx_sigmoid(mlx_array_t* array) {
    if (!array) return nullptr;
    try {
        auto wrapper = reinterpret_cast<MLXArrayWrapper*>(array);
        mx::array result = mx::sigmoid(wrapper->arr);
        auto result_wrapper = new MLXArrayWrapper(result);
        return reinterpret_cast<mlx_array_t*>(result_wrapper);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

extern "C" mlx_array_t* mlx_tanh(mlx_array_t* array) {
    if (!array) return nullptr;
    try {
        auto wrapper = reinterpret_cast<MLXArrayWrapper*>(array);
        mx::array result = mx::tanh(wrapper->arr);
        auto result_wrapper = new MLXArrayWrapper(result);
        return reinterpret_cast<mlx_array_t*>(result_wrapper);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

extern "C" mlx_array_t* mlx_softmax(mlx_array_t* array) {
    if (!array) return nullptr;
    try {
        auto wrapper = reinterpret_cast<MLXArrayWrapper*>(array);
        mx::array result = mx::softmax(wrapper->arr, -1);  // Apply along last axis
        auto result_wrapper = new MLXArrayWrapper(result);
        return reinterpret_cast<mlx_array_t*>(result_wrapper);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

// Scaled dot-product attention
// Implements: softmax(Q @ K^T * scale + mask) @ V
extern "C" mlx_array_t* mlx_scaled_dot_product_attention(
    mlx_array_t* query,
    mlx_array_t* key,
    mlx_array_t* value,
    float scale,
    mlx_array_t* mask  // nullable - causal or padding mask
) {
    if (!query || !key || !value) {
        g_last_error = "Query, key, and value arrays are required";
        return nullptr;
    }

    try {
        auto q_wrapper = reinterpret_cast<MLXArrayWrapper*>(query);
        auto k_wrapper = reinterpret_cast<MLXArrayWrapper*>(key);
        auto v_wrapper = reinterpret_cast<MLXArrayWrapper*>(value);

        // Q: [batch, heads, seq_q, head_dim]
        // K: [batch, heads, seq_k, head_dim]
        // V: [batch, heads, seq_k, head_dim]

        // Step 1: Compute attention scores: Q @ K^T
        // K^T: [batch, heads, head_dim, seq_k]
        // scores: [batch, heads, seq_q, seq_k]
        mx::array k_transposed = mx::transpose(k_wrapper->arr, {0, 1, 3, 2});
        mx::array scores = mx::matmul(q_wrapper->arr, k_transposed);

        // Step 2: Apply scale
        scores = mx::multiply(scores, mx::array(scale));

        // Step 3: Apply mask if provided
        if (mask) {
            auto mask_wrapper = reinterpret_cast<MLXArrayWrapper*>(mask);
            // Mask should be broadcastable to scores shape
            // Typically 0 for positions to keep, -inf for positions to mask
            scores = mx::add(scores, mask_wrapper->arr);
        }

        // Step 4: Apply softmax along last axis (over keys)
        mx::array attn_weights = mx::softmax(scores, -1);

        // Step 5: Apply attention to values: attn_weights @ V
        // attn_weights: [batch, heads, seq_q, seq_k]
        // V: [batch, heads, seq_k, head_dim]
        // output: [batch, heads, seq_q, head_dim]
        mx::array output = mx::matmul(attn_weights, v_wrapper->arr);

        auto result_wrapper = new MLXArrayWrapper(output);
        return reinterpret_cast<mlx_array_t*>(result_wrapper);

    } catch (const std::exception& e) {
        g_last_error = std::string("Scaled dot-product attention failed: ") + e.what();
        return nullptr;
    }
}

// Create causal attention mask
// Returns a mask with 0 for valid positions and -inf for masked positions
extern "C" mlx_array_t* mlx_create_causal_mask(int seq_len) {
    try {
        // Create upper triangular matrix filled with -inf
        // Lower triangle (including diagonal) = 0, upper triangle = -inf
        mx::array mask = mx::zeros({seq_len, seq_len}, mx::float32);

        // Create indices for upper triangle
        std::vector<float> mask_data(seq_len * seq_len, 0.0f);
        for (int i = 0; i < seq_len; ++i) {
            for (int j = i + 1; j < seq_len; ++j) {
                mask_data[i * seq_len + j] = -1e9f;  // Large negative instead of -inf for numerical stability
            }
        }

        mask = mx::array(mask_data.data(), {seq_len, seq_len}, mx::float32);

        auto result_wrapper = new MLXArrayWrapper(mask);
        return reinterpret_cast<mlx_array_t*>(result_wrapper);

    } catch (const std::exception& e) {
        g_last_error = std::string("Failed to create causal mask: ") + e.what();
        return nullptr;
    }
}

// LoRA operations
extern "C" mlx_array_t* mlx_lora_forward(
    mlx_array_t* input,
    mlx_array_t* lora_a,
    mlx_array_t* lora_b,
    float alpha,
    float rank
) {
    if (!input || !lora_a || !lora_b) return nullptr;
    try {
        auto input_wrapper = reinterpret_cast<MLXArrayWrapper*>(input);
        auto a_wrapper = reinterpret_cast<MLXArrayWrapper*>(lora_a);
        auto b_wrapper = reinterpret_cast<MLXArrayWrapper*>(lora_b);

        // LoRA forward: output = input @ A @ B * (alpha / rank)
        mx::array intermediate = mx::matmul(input_wrapper->arr, a_wrapper->arr);
        mx::array output = mx::matmul(intermediate, b_wrapper->arr);
        mx::array scaled = mx::multiply(output, mx::array(alpha / rank));

        auto result_wrapper = new MLXArrayWrapper(scaled);
        return reinterpret_cast<mlx_array_t*>(result_wrapper);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

extern "C" mlx_array_t* mlx_lora_combine(
    mlx_array_t* base_output,
    mlx_array_t* lora_output,
    float gate
) {
    if (!base_output || !lora_output) return nullptr;
    try {
        auto base_wrapper = reinterpret_cast<MLXArrayWrapper*>(base_output);
        auto lora_wrapper = reinterpret_cast<MLXArrayWrapper*>(lora_output);

        // Combine: result = base + lora * gate
        mx::array gated = mx::multiply(lora_wrapper->arr, mx::array(gate));
        mx::array result = mx::add(base_wrapper->arr, gated);

        auto result_wrapper = new MLXArrayWrapper(result);
        return reinterpret_cast<mlx_array_t*>(result_wrapper);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

// Multi-adapter K-sparse LoRA routing with Q15 quantized gates
//
// Implements K-sparse routing for efficient multi-adapter inference.
// Uses Q15 fixed-point format for gate weights to reduce memory bandwidth.
//
// Formula: output = input + sum_i(gate_i * B_i(A_i(input)) * (alpha/rank))
//
// Parameters:
//   input: Input tensor [batch, seq_len, hidden_dim] or [seq_len, hidden_dim]
//   lora_a_list: Array of LoRA A matrices (down-projection) [hidden_dim, rank]
//   lora_b_list: Array of LoRA B matrices (up-projection) [rank, hidden_dim]
//   num_adapters: Number of active adapters (K-sparse, max 8)
//   gates_q15: Q15 quantized gate weights (i16, 0-32767 maps to 0.0-1.0)
//   alpha: LoRA scaling factor
//   rank: LoRA rank dimension
//
// Returns: Combined output tensor with identity path and weighted LoRA contributions
extern "C" mlx_array_t* mlx_multi_lora_forward(
    mlx_array_t* input,
    mlx_array_t** lora_a_list,
    mlx_array_t** lora_b_list,
    int num_adapters,
    const int16_t* gates_q15,
    float alpha,
    float rank
) {
    // Validate input parameters with specific error messages
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
    // This limit aligns with typical router architectures
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
        auto input_wrapper = reinterpret_cast<MLXArrayWrapper*>(input);

        // Initialize result accumulator with zeros (same shape as input)
        // This will accumulate: sum_i(gate_i * lora_i(input))
        mx::array result = mx::zeros_like(input_wrapper->arr);

        // Precompute LoRA scaling factor: alpha / rank
        const float scaling = alpha / rank;

        // Q15 dequantization constant
        // Q15 format: signed 16-bit integer where 32767 represents 1.0
        // Range [0, 32767] maps to [0.0, 1.0] for gate weights
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
            // This avoids unnecessary computation for adapters not selected by router
            if (gate_weight <= 1e-6f) {
                continue;
            }

            auto a_wrapper = reinterpret_cast<MLXArrayWrapper*>(lora_a_list[i]);
            auto b_wrapper = reinterpret_cast<MLXArrayWrapper*>(lora_b_list[i]);

            // LoRA forward pass: lora_out = B(A(input)) * (alpha/rank)
            //
            // Dimensions:
            //   input: [batch, seq_len, hidden_dim] or [seq_len, hidden_dim]
            //   A: [hidden_dim, rank] (down-projection)
            //   B: [rank, hidden_dim] (up-projection)
            //
            // Step 1: Down-project input through A
            //   intermediate = input @ A  -> [..., rank]
            mx::array intermediate = mx::matmul(input_wrapper->arr, a_wrapper->arr);

            // Step 2: Up-project through B
            //   lora_output = intermediate @ B  -> [..., hidden_dim]
            mx::array lora_output = mx::matmul(intermediate, b_wrapper->arr);

            // Step 3: Apply combined scaling: gate_i * (alpha/rank)
            // Combining gate weight with LoRA scaling in one multiply reduces ops
            float combined_scale = gate_weight * scaling;
            mx::array scaled = mx::multiply(lora_output, mx::array(combined_scale));

            // Step 4: Accumulate weighted LoRA output
            //   result += gate_i * lora_i(input)
            result = mx::add(result, scaled);
        }

        // Add identity path: final = input + sum(gate_i * lora_i(input))
        // This preserves the base model's representation while adding adapter contributions
        result = mx::add(input_wrapper->arr, result);

        // Force evaluation for immediate results (MLX uses lazy evaluation by default)
        // This ensures the computation is complete before returning
        mx::eval(result);

        auto result_wrapper = new MLXArrayWrapper(result);
        return reinterpret_cast<mlx_array_t*>(result_wrapper);

    } catch (const std::exception& e) {
        g_last_error = std::string("mlx_multi_lora_forward failed: ") + e.what();
        return nullptr;
    }
}

// RNG seeding for deterministic dropout/sampling
extern "C" void mlx_set_seed(const uint8_t* seed, size_t seed_len) {
    if (!seed || seed_len == 0) {
        g_last_error = "Invalid seed: pointer is null or length is 0";
        return;
    }

    try {
        // Convert seed bytes to uint64_t
        // MLX's random::seed() takes a uint64_t, so we use the first 8 bytes
        uint64_t seed_value = 0;

        if (seed_len >= 8) {
            // Use first 8 bytes as big-endian uint64
            for (size_t i = 0; i < 8; i++) {
                seed_value = (seed_value << 8) | seed[i];
            }
        } else {
            // Pad shorter seeds with zeros
            for (size_t i = 0; i < seed_len; i++) {
                seed_value = (seed_value << 8) | seed[i];
            }
            // Shift to align if seed_len < 8
            seed_value <<= (8 - seed_len) * 8;
        }

        // Set MLX's global random seed
        mx::random::seed(seed_value);

    } catch (const std::exception& e) {
        g_last_error = std::string("Failed to set MLX seed: ") + e.what();
    }
}

// Error handling
extern "C" const char* mlx_get_last_error(void) {
    return g_last_error.c_str();
}

extern "C" void mlx_clear_error(void) {
    g_last_error.clear();
}

// Memory management
/// Trigger garbage collection in MLX unified memory
/// MLX doesn't expose explicit GC in C++ API, but we can hint to the system
extern "C" void mlx_gc_collect(void) {
    try {
        // MLX uses unified memory managed by the system
        // We can optionally call mx::eval to flush pending operations
        // and let the memory manager reclaim unused buffers

        // Flush any pending operations
        mx::eval(mx::array(0.0f));  // Dummy eval to flush pipeline

        // In a more sophisticated implementation, we could:
        // 1. Track weak references to arrays
        // 2. Compact memory pools
        // 3. Request memory pressure relief from the system

        // For now, just ensure operations are evaluated
    } catch (const std::exception& e) {
        // Log but don't propagate - GC failure shouldn't crash
        g_last_error = std::string("GC hint failed: ") + e.what();
    }
}

/// Get total memory usage by MLX backend in bytes
/// This tracks unified memory allocations made through this wrapper
extern "C" size_t mlx_memory_usage(void) {
    // Return atomic counter of tracked allocations
    // This includes array allocations and model weights
    return g_total_memory_used.load(std::memory_order_relaxed);
}

/// Get number of tracked allocations
/// Useful for debugging and understanding allocation patterns
extern "C" size_t mlx_allocation_count(void) {
    return g_allocation_count.load(std::memory_order_relaxed);
}

/// Reset memory tracking (for testing)
/// Clears all tracked allocations and counters
extern "C" void mlx_memory_reset(void) {
    std::lock_guard<std::mutex> lock(g_memory_mutex);
    g_allocation_map.clear();
    g_total_memory_used.store(0, std::memory_order_relaxed);
    g_allocation_count.store(0, std::memory_order_relaxed);
}

/// Get detailed memory statistics
/// Fills in allocation count and memory usage
extern "C" void mlx_memory_stats(size_t* out_total_bytes, size_t* out_allocation_count) {
    if (out_total_bytes) {
        *out_total_bytes = g_total_memory_used.load(std::memory_order_relaxed);
    }
    if (out_allocation_count) {
        *out_allocation_count = g_allocation_count.load(std::memory_order_relaxed);
    }
}

// ============================================================================
// Runtime initialization and backend info
// ============================================================================

extern "C" int mlx_init(mlx_device_type_t device_type) {
    try {
        // Set device based on requested type
        mx::Device target_device = mx::Device::gpu;  // Default to GPU

        switch (device_type) {
            case MLX_DEVICE_CPU:
                target_device = mx::Device::cpu;
                break;
            case MLX_DEVICE_GPU:
            case MLX_DEVICE_ANE:  // ANE uses GPU path in MLX
            case MLX_DEVICE_AUTO:
            default:
                target_device = mx::Device::gpu;
                break;
        }

        mx::set_default_device(target_device);
        g_current_device_type = device_type;
        g_initialized.store(true, std::memory_order_release);

        return 0;
    } catch (const std::exception& e) {
        g_last_error = std::string("Failed to initialize MLX: ") + e.what();
        return -1;
    }
}

extern "C" int mlx_init_default(void) {
    return mlx_init(MLX_DEVICE_AUTO);
}

extern "C" void mlx_shutdown(void) {
    try {
        // Clear LoRA cache
        {
            std::lock_guard<std::mutex> lock(g_lora_cache_mutex);
            g_lora_cache.clear();
        }

        // Reset memory tracking
        mlx_memory_reset();

        g_initialized.store(false, std::memory_order_release);
    } catch (...) {
        // Ignore errors during shutdown
    }
}

extern "C" bool mlx_is_initialized(void) {
    return g_initialized.load(std::memory_order_acquire);
}

extern "C" mlx_device_type_t mlx_get_device_type(void) {
    return g_current_device_type;
}

extern "C" int mlx_set_device(mlx_device_type_t device_type) {
    try {
        mx::Device target_device = mx::Device::gpu;

        switch (device_type) {
            case MLX_DEVICE_CPU:
                target_device = mx::Device::cpu;
                break;
            case MLX_DEVICE_GPU:
            case MLX_DEVICE_ANE:
            case MLX_DEVICE_AUTO:
            default:
                target_device = mx::Device::gpu;
                break;
        }

        mx::set_default_device(target_device);
        g_current_device_type = device_type;

        return 0;
    } catch (const std::exception& e) {
        g_last_error = std::string("Failed to set device: ") + e.what();
        return -1;
    }
}

extern "C" int mlx_backend_info(mlx_backend_capabilities_t* capabilities) {
    if (!capabilities) {
        g_last_error = "capabilities pointer is null";
        return -1;
    }

    try {
        std::memset(capabilities, 0, sizeof(mlx_backend_capabilities_t));

        // Query MLX metal capabilities
        capabilities->gpu_available = mx::metal::is_available();
        capabilities->unified_memory = true;  // Apple Silicon always has unified memory
        capabilities->metal_compute = capabilities->gpu_available;

        // ANE availability depends on device - assume available on Apple Silicon
        capabilities->ane_available = capabilities->gpu_available;

        if (capabilities->gpu_available) {
            // Get device info from Metal
            capabilities->max_threads_per_group = 1024;  // Standard Metal limit

            // device_info() returns unordered_map<string, variant<string, size_t>>
            auto info = mx::metal::device_info();
            auto it = info.find("max_buffer_length");
            if (it != info.end()) {
                capabilities->max_buffer_size = std::get<size_t>(it->second);
            } else {
                capabilities->max_buffer_size = 256 * 1024 * 1024;  // Default 256MB
            }

            // Get device name
            auto name_it = info.find("device_name");
            if (name_it != info.end()) {
                std::strncpy(capabilities->device_name, std::get<std::string>(name_it->second).c_str(), sizeof(capabilities->device_name) - 1);
            } else {
                std::strncpy(capabilities->device_name, "Apple GPU", sizeof(capabilities->device_name) - 1);
            }
        }

        // Version strings
        std::strncpy(capabilities->mlx_version, "0.16.0", sizeof(capabilities->mlx_version) - 1);
        std::strncpy(capabilities->metal_version, "3.0", sizeof(capabilities->metal_version) - 1);

        return 0;
    } catch (const std::exception& e) {
        g_last_error = std::string("Failed to get backend info: ") + e.what();
        return -1;
    }
}

extern "C" const char* mlx_get_version(void) {
    static const char* version = "0.16.0";
    return version;
}

// ============================================================================
// Quantization operations
// ============================================================================

extern "C" mlx_array_t* mlx_quantize(mlx_array_t* array, int group_size, int bits) {
    if (!array) {
        g_last_error = "array is null";
        return nullptr;
    }

    if (bits != 4 && bits != 8) {
        g_last_error = "bits must be 4 or 8";
        return nullptr;
    }

    try {
        auto wrapper = reinterpret_cast<MLXArrayWrapper*>(array);

        // MLX quantize returns a vector of 3 arrays: [quantized, scales, biases]
        // For simplicity, we'll return just the quantized array
        // A more complete implementation would return all three
        std::vector<mx::array> result = mx::quantize(wrapper->arr, group_size, bits);

        if (result.size() < 1) {
            g_last_error = "Quantize returned empty result";
            return nullptr;
        }

        auto result_wrapper = new MLXArrayWrapper(result[0]);
        return reinterpret_cast<mlx_array_t*>(result_wrapper);

    } catch (const std::exception& e) {
        g_last_error = std::string("Quantize failed: ") + e.what();
        return nullptr;
    }
}

extern "C" mlx_array_t* mlx_dequantize(
    mlx_array_t* array,
    mlx_array_t* scales,
    mlx_array_t* biases,
    int group_size,
    int bits
) {
    if (!array || !scales) {
        g_last_error = "array and scales are required";
        return nullptr;
    }

    try {
        auto arr_wrapper = reinterpret_cast<MLXArrayWrapper*>(array);
        auto scales_wrapper = reinterpret_cast<MLXArrayWrapper*>(scales);

        mx::array result = mx::array(0.0f);  // Initialize with dummy value
        if (biases) {
            auto biases_wrapper = reinterpret_cast<MLXArrayWrapper*>(biases);
            result = mx::dequantize(arr_wrapper->arr, scales_wrapper->arr, biases_wrapper->arr, group_size, bits);
        } else {
            // Use zeros for symmetric quantization
            mx::array zero_biases = mx::zeros_like(scales_wrapper->arr);
            result = mx::dequantize(arr_wrapper->arr, scales_wrapper->arr, zero_biases, group_size, bits);
        }

        auto result_wrapper = new MLXArrayWrapper(result);
        return reinterpret_cast<mlx_array_t*>(result_wrapper);

    } catch (const std::exception& e) {
        g_last_error = std::string("Dequantize failed: ") + e.what();
        return nullptr;
    }
}

// ============================================================================
// RoPE (Rotary Position Embedding)
// ============================================================================

extern "C" mlx_array_t* mlx_rope(
    mlx_array_t* array,
    int dims,
    bool traditional,
    float base,
    float scale,
    int offset
) {
    if (!array) {
        g_last_error = "array is null";
        return nullptr;
    }

    try {
        auto wrapper = reinterpret_cast<MLXArrayWrapper*>(array);

        // Use MLX fast rope implementation
        mx::array result = mx::fast::rope(
            wrapper->arr,
            dims,
            traditional,
            base,
            scale,
            offset
        );

        auto result_wrapper = new MLXArrayWrapper(result);
        return reinterpret_cast<mlx_array_t*>(result_wrapper);

    } catch (const std::exception& e) {
        g_last_error = std::string("RoPE failed: ") + e.what();
        return nullptr;
    }
}

// ============================================================================
// Token sampling
// ============================================================================

extern "C" int mlx_sample_token(mlx_array_t* logits, const mlx_sampler_config_t* config) {
    if (!logits || !config) {
        g_last_error = "logits and config are required";
        return -1;
    }

    try {
        auto wrapper = reinterpret_cast<MLXArrayWrapper*>(logits);
        mx::array probs = wrapper->arr;

        // Get last token's logits if sequence
        if (probs.ndim() > 1) {
            int last_idx = probs.shape(-2) - 1;
            probs = mx::take(probs, mx::array(last_idx), probs.ndim() - 2);
        }

        // Flatten to 1D
        probs = mx::reshape(probs, {-1});

        // Apply temperature
        if (config->temperature > 0.0f) {
            probs = mx::divide(probs, mx::array(config->temperature));
        }

        // Convert logits to probabilities
        probs = mx::softmax(probs, -1);

        // Top-k filtering
        if (config->top_k > 0 && config->top_k < probs.shape(0)) {
            // Sort and get top-k indices
            mx::array sorted_indices = mx::argsort(probs, -1);
            int vocab_size = probs.shape(0);

            // Create mask for top-k
            std::vector<float> mask_data(vocab_size, 0.0f);
            mx::eval(sorted_indices);
            auto* indices_ptr = sorted_indices.data<int>();

            for (int i = vocab_size - config->top_k; i < vocab_size; ++i) {
                mask_data[indices_ptr[i]] = 1.0f;
            }

            mx::array mask = mx::array(mask_data.data(), {vocab_size}, mx::float32);
            probs = mx::multiply(probs, mask);

            // Renormalize
            mx::array sum = mx::sum(probs);
            probs = mx::divide(probs, sum);
        }

        // Top-p (nucleus) sampling
        if (config->top_p > 0.0f && config->top_p < 1.0f) {
            // Sort probabilities in descending order
            mx::array sorted_probs = mx::sort(probs, -1);
            mx::array cumsum = mx::cumsum(sorted_probs, -1);

            // Create mask for top-p
            mx::array mask = mx::less(cumsum, mx::array(config->top_p));

            // Apply mask
            probs = mx::multiply(probs, mx::astype(mask, mx::float32));

            // Renormalize
            mx::array sum = mx::sum(probs);
            probs = mx::divide(probs, sum);
        }

        // Greedy sampling if temperature is 0
        if (config->temperature <= 0.0f) {
            mx::array max_idx = mx::argmax(probs);
            mx::eval(max_idx);
            return static_cast<int>(max_idx.item<int>());
        }

        // Sample from categorical distribution
        mx::array sampled = mx::random::categorical(mx::log(probs));
        mx::eval(sampled);

        return static_cast<int>(sampled.item<int>());

    } catch (const std::exception& e) {
        g_last_error = std::string("Token sampling failed: ") + e.what();
        return -1;
    }
}

// ============================================================================
// KV Cache management
// ============================================================================

extern "C" mlx_kv_cache_t* mlx_kv_cache_new(int num_layers, int num_heads, int head_dim, int max_seq_len) {
    if (num_layers <= 0 || num_heads <= 0 || head_dim <= 0 || max_seq_len <= 0) {
        g_last_error = "All KV cache dimensions must be positive";
        return nullptr;
    }

    try {
        auto cache = new MLXKVCache(num_layers, num_heads, head_dim, max_seq_len);
        return reinterpret_cast<mlx_kv_cache_t*>(cache);
    } catch (const std::exception& e) {
        g_last_error = std::string("Failed to create KV cache: ") + e.what();
        return nullptr;
    }
}

extern "C" int mlx_kv_cache_update(mlx_kv_cache_t* cache, int layer_idx, mlx_array_t* keys, mlx_array_t* values) {
    if (!cache || !keys || !values) {
        g_last_error = "cache, keys, and values are required";
        return -1;
    }

    try {
        auto kv_cache = reinterpret_cast<MLXKVCache*>(cache);
        auto keys_wrapper = reinterpret_cast<MLXArrayWrapper*>(keys);
        auto values_wrapper = reinterpret_cast<MLXArrayWrapper*>(values);

        if (!kv_cache->update(layer_idx, keys_wrapper->arr, values_wrapper->arr)) {
            g_last_error = "Invalid layer index";
            return -1;
        }

        return 0;
    } catch (const std::exception& e) {
        g_last_error = std::string("KV cache update failed: ") + e.what();
        return -1;
    }
}

extern "C" mlx_array_t* mlx_kv_cache_get_keys(mlx_kv_cache_t* cache, int layer_idx) {
    if (!cache) {
        g_last_error = "cache is null";
        return nullptr;
    }

    try {
        auto kv_cache = reinterpret_cast<MLXKVCache*>(cache);

        if (layer_idx < 0 || layer_idx >= kv_cache->num_layers) {
            g_last_error = "Invalid layer index";
            return nullptr;
        }

        auto result_wrapper = new MLXArrayWrapper(kv_cache->keys[layer_idx]);
        return reinterpret_cast<mlx_array_t*>(result_wrapper);

    } catch (const std::exception& e) {
        g_last_error = std::string("Failed to get cached keys: ") + e.what();
        return nullptr;
    }
}

extern "C" mlx_array_t* mlx_kv_cache_get_values(mlx_kv_cache_t* cache, int layer_idx) {
    if (!cache) {
        g_last_error = "cache is null";
        return nullptr;
    }

    try {
        auto kv_cache = reinterpret_cast<MLXKVCache*>(cache);

        if (layer_idx < 0 || layer_idx >= kv_cache->num_layers) {
            g_last_error = "Invalid layer index";
            return nullptr;
        }

        auto result_wrapper = new MLXArrayWrapper(kv_cache->values[layer_idx]);
        return reinterpret_cast<mlx_array_t*>(result_wrapper);

    } catch (const std::exception& e) {
        g_last_error = std::string("Failed to get cached values: ") + e.what();
        return nullptr;
    }
}

extern "C" int mlx_kv_cache_seq_len(mlx_kv_cache_t* cache) {
    if (!cache) return 0;
    auto kv_cache = reinterpret_cast<MLXKVCache*>(cache);
    return kv_cache->current_seq_len;
}

extern "C" void mlx_kv_cache_reset(mlx_kv_cache_t* cache) {
    if (!cache) return;
    auto kv_cache = reinterpret_cast<MLXKVCache*>(cache);
    kv_cache->reset();
}

extern "C" void mlx_kv_cache_free(mlx_kv_cache_t* cache) {
    if (cache) {
        delete reinterpret_cast<MLXKVCache*>(cache);
    }
}

// ============================================================================
// SafeTensors weight loading
// ============================================================================

extern "C" mlx_weights_t* mlx_load_safetensors(const char* path) {
    if (!path) {
        g_last_error = "path is null";
        return nullptr;
    }

    try {
        auto weights = new MLXWeightsWrapper();

        if (!weights->load(std::string(path))) {
            delete weights;
            return nullptr;
        }

        return reinterpret_cast<mlx_weights_t*>(weights);

    } catch (const std::exception& e) {
        g_last_error = std::string("Failed to load safetensors: ") + e.what();
        return nullptr;
    }
}

extern "C" mlx_array_t* mlx_weights_get(mlx_weights_t* weights, const char* name) {
    if (!weights || !name) {
        g_last_error = "weights and name are required";
        return nullptr;
    }

    try {
        auto weights_wrapper = reinterpret_cast<MLXWeightsWrapper*>(weights);
        auto arr = weights_wrapper->get(std::string(name));

        if (!arr) {
            return nullptr;  // Not found, not an error
        }

        auto result_wrapper = new MLXArrayWrapper(*arr);
        return reinterpret_cast<mlx_array_t*>(result_wrapper);

    } catch (const std::exception& e) {
        g_last_error = std::string("Failed to get weight: ") + e.what();
        return nullptr;
    }
}

extern "C" int mlx_weights_list(mlx_weights_t* weights, const char** names, int max_names) {
    if (!weights) return 0;

    auto weights_wrapper = reinterpret_cast<MLXWeightsWrapper*>(weights);
    int count = static_cast<int>(weights_wrapper->weight_names.size());

    if (names && max_names > 0) {
        int to_copy = std::min(count, max_names);
        for (int i = 0; i < to_copy; ++i) {
            names[i] = weights_wrapper->weight_names[i].c_str();
        }
    }

    return count;
}

extern "C" void mlx_weights_free(mlx_weights_t* weights) {
    if (weights) {
        delete reinterpret_cast<MLXWeightsWrapper*>(weights);
    }
}

// ============================================================================
// Evaluation and synchronization
// ============================================================================

extern "C" void mlx_eval(mlx_array_t* array) {
    if (!array) return;

    try {
        auto wrapper = reinterpret_cast<MLXArrayWrapper*>(array);
        mx::eval(wrapper->arr);
    } catch (const std::exception& e) {
        g_last_error = std::string("Eval failed: ") + e.what();
    }
}

extern "C" void mlx_eval_all(mlx_array_t** arrays, int num_arrays) {
    if (!arrays || num_arrays <= 0) return;

    try {
        std::vector<mx::array> to_eval;
        to_eval.reserve(num_arrays);

        for (int i = 0; i < num_arrays; ++i) {
            if (arrays[i]) {
                auto wrapper = reinterpret_cast<MLXArrayWrapper*>(arrays[i]);
                to_eval.push_back(wrapper->arr);
            }
        }

        if (!to_eval.empty()) {
            mx::eval(to_eval);
        }

    } catch (const std::exception& e) {
        g_last_error = std::string("Eval all failed: ") + e.what();
    }
}

extern "C" void mlx_synchronize(void) {
    try {
        mx::synchronize();
    } catch (const std::exception& e) {
        g_last_error = std::string("Synchronize failed: ") + e.what();
    }
}

// ============================================================================
// LoRA Adapter Caching
// ============================================================================

extern "C" const char* mlx_lora_cache_adapter(const char* adapter_id, mlx_array_t* lora_a, mlx_array_t* lora_b) {
    if (!adapter_id || !lora_a || !lora_b) {
        g_last_error = "adapter_id, lora_a, and lora_b are required";
        return nullptr;
    }

    try {
        auto a_wrapper = reinterpret_cast<MLXArrayWrapper*>(lora_a);
        auto b_wrapper = reinterpret_cast<MLXArrayWrapper*>(lora_b);

        std::lock_guard<std::mutex> lock(g_lora_cache_mutex);

        // LRU eviction if at capacity
        if (g_lora_cache.size() >= g_lora_cache_limit) {
            // Find and evict least recently used
            std::string lru_key;
            uint64_t min_access = UINT64_MAX;

            for (const auto& [key, entry] : g_lora_cache) {
                if (entry.last_access < min_access) {
                    min_access = entry.last_access;
                    lru_key = key;
                }
            }

            if (!lru_key.empty()) {
                g_lora_cache.erase(lru_key);
            }
        }

        // Insert or update
        std::string key(adapter_id);
        g_lora_cache[key] = LoRACacheEntry{
            a_wrapper->arr,
            b_wrapper->arr,
            ++g_lora_access_counter
        };

        return adapter_id;

    } catch (const std::exception& e) {
        g_last_error = std::string("Failed to cache adapter: ") + e.what();
        return nullptr;
    }
}

extern "C" bool mlx_lora_get_cached(const char* adapter_id, mlx_array_t** out_lora_a, mlx_array_t** out_lora_b) {
    if (!adapter_id || !out_lora_a || !out_lora_b) {
        return false;
    }

    try {
        std::lock_guard<std::mutex> lock(g_lora_cache_mutex);

        auto it = g_lora_cache.find(std::string(adapter_id));
        if (it == g_lora_cache.end()) {
            return false;
        }

        // Update access time
        it->second.last_access = ++g_lora_access_counter;

        // Return copies of the arrays
        *out_lora_a = reinterpret_cast<mlx_array_t*>(new MLXArrayWrapper(it->second.lora_a));
        *out_lora_b = reinterpret_cast<mlx_array_t*>(new MLXArrayWrapper(it->second.lora_b));

        return true;

    } catch (const std::exception& e) {
        g_last_error = std::string("Failed to get cached adapter: ") + e.what();
        return false;
    }
}

extern "C" void mlx_lora_evict_cached(const char* adapter_id) {
    if (!adapter_id) return;

    std::lock_guard<std::mutex> lock(g_lora_cache_mutex);
    g_lora_cache.erase(std::string(adapter_id));
}

extern "C" void mlx_lora_clear_cache(void) {
    std::lock_guard<std::mutex> lock(g_lora_cache_mutex);
    g_lora_cache.clear();
}

extern "C" size_t mlx_lora_cache_size(void) {
    std::lock_guard<std::mutex> lock(g_lora_cache_mutex);
    return g_lora_cache.size();
}

extern "C" void mlx_lora_set_cache_limit(size_t max_entries) {
    std::lock_guard<std::mutex> lock(g_lora_cache_mutex);
    g_lora_cache_limit = max_entries;

    // Evict if over new limit
    while (g_lora_cache.size() > g_lora_cache_limit) {
        std::string lru_key;
        uint64_t min_access = UINT64_MAX;

        for (const auto& [key, entry] : g_lora_cache) {
            if (entry.last_access < min_access) {
                min_access = entry.last_access;
                lru_key = key;
            }
        }

        if (!lru_key.empty()) {
            g_lora_cache.erase(lru_key);
        } else {
            break;
        }
    }
}

#else
// If MLX_REAL is not defined, fall back to stub
#warning "Compiling without real MLX support - using stub implementation"
// The stub implementation should be compiled separately
#endif