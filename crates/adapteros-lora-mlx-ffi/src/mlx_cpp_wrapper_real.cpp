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
#include <tuple>
#include <algorithm>
#include <optional>

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

// Variadic helper to create Shape from dimensions
template<typename... Dims>
inline mx::Shape make_shape(Dims... dims) {
    return mx::Shape{static_cast<int32_t>(dims)...};
}

// Debug helper to print array shape
inline std::string shape_str(const mx::array& arr) {
    std::string s = "[";
    for (size_t i = 0; i < arr.ndim(); ++i) {
        if (i > 0) s += ", ";
        s += std::to_string(arr.shape(i));
    }
    s += "]";
    return s;
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

    struct MoEConfig {
        bool enabled = false;
        int num_experts = 0;
        int num_experts_per_tok = 0;
        int hidden_size = 0;
        int intermediate_size = 0;
        bool norm_topk_prob = false;
        int decoder_sparse_step = 1;
        int quant_bits = 0;
        int quant_group_size = 0;
    } moe;

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

            auto parse_bool = [&content](const std::string& key) -> bool {
                size_t pos = content.find("\"" + key + "\"");
                if (pos == std::string::npos) return false;
                pos = content.find(":", pos);
                if (pos == std::string::npos) return false;
                pos++;
                while (pos < content.size() && (content[pos] == ' ' || content[pos] == '\t')) pos++;
                if (content.compare(pos, 4, "true") == 0) return true;
                if (content.compare(pos, 5, "false") == 0) return false;
                return false;
            };

            int val;
            if ((val = parse_int("num_attention_heads")) > 0) num_attention_heads = val;
            if ((val = parse_int("num_key_value_heads")) > 0) num_key_value_heads = val;
            if ((val = parse_int("hidden_size")) > 0) hidden_size = val;
            if ((val = parse_int("num_hidden_layers")) > 0) num_hidden_layers = val;
            if ((val = parse_int("intermediate_size")) > 0) intermediate_size = val;
            if ((val = parse_int("vocab_size")) > 0) vocab_size = val;
            if ((val = parse_int("num_experts")) > 0) moe.num_experts = val;
            if ((val = parse_int("num_experts_per_tok")) > 0) moe.num_experts_per_tok = val;
            if ((val = parse_int("moe_intermediate_size")) > 0) moe.intermediate_size = val;
            if ((val = parse_int("decoder_sparse_step")) > 0) moe.decoder_sparse_step = val;
            moe.norm_topk_prob = parse_bool("norm_topk_prob");
            moe.hidden_size = hidden_size;

            // Quantization hints (optional)
            if ((val = parse_int("quant_bits")) > 0) moe.quant_bits = val;
            if ((val = parse_int("quant_group_size")) > 0) moe.quant_group_size = val;
            if ((val = parse_int("group_size")) > 0 && moe.quant_group_size == 0)
                moe.quant_group_size = val;
            if ((val = parse_int("bits")) > 0 && moe.quant_bits == 0) moe.quant_bits = val;

            // Calculate head_dim
            head_dim = hidden_size / num_attention_heads;

            // Enable MoE path if config says so
            moe.enabled = (moe.num_experts > 0) && (moe.num_experts_per_tok > 0);
            if (moe.intermediate_size == 0) {
                moe.intermediate_size = intermediate_size;
            }

            std::cerr << "[MLX] Loaded config: heads=" << num_attention_heads
                      << ", kv_heads=" << num_key_value_heads
                      << ", hidden=" << hidden_size
                      << ", moe_experts=" << moe.num_experts
                      << ", moe_top_k=" << moe.num_experts_per_tok
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
                // Some exports keep an index file but store weights in a single file.
                const std::string single_filename = "model.safetensors";
                bool index_has_single = content.find(single_filename) != std::string::npos ||
                                        content.find("pytorch_model.bin.safetensors") != std::string::npos;

                if (index_has_single) {
                    std::string single_path = model_path + "/" + single_filename;
                    std::ifstream single_file(single_path);
                    if (!single_file.good()) {
                        single_file.close();
                        single_path = model_path + "/pytorch_model.bin.safetensors";
                        single_file.open(single_path);
                    }

                    if (single_file.good()) {
                        single_file.close();

                        auto [single_weights, metadata] = mx::load_safetensors(single_path);
                        weights = std::move(single_weights);

                        if (weights.empty()) {
                            g_last_error = "No weights loaded from safetensors file";
                            return false;
                        }

                        total_weight_bytes = 0;
                        for (const auto& [name, arr] : weights) {
                            size_t bytes = calculate_array_memory(arr);
                            total_weight_bytes += bytes;
                        }
                        record_allocation(reinterpret_cast<uintptr_t>(this), total_weight_bytes);
                        std::cout << "[MLX] Loaded unsharded model referenced by index: " << single_path << std::endl;

                        return true;
                    }
                }

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

    // Get embedding weights with dequantization if quantized
    mx::array get_embedding_weights() {
        auto embed_weight_ptr = find_weight("model.embed_tokens.weight");
        if (!embed_weight_ptr) {
            throw std::runtime_error("Embedding weights not found");
        }

        // Check if embeddings are quantized (have companion scales)
        auto scales_it = weights.find("model.embed_tokens.scales");
        if (scales_it == weights.end()) {
            // Not quantized, return raw weights
            return *embed_weight_ptr;
        }

        // Dequantize embeddings
        mx::array weight = *embed_weight_ptr;
        mx::array scales = scales_it->second;
        mx::array biases = mx::zeros({static_cast<int>(weight.shape(0))}, scales.dtype());
        auto biases_it = weights.find("model.embed_tokens.biases");
        if (biases_it != weights.end()) {
            biases = biases_it->second;
        }

        int bits = moe.quant_bits > 0 ? moe.quant_bits : 4;
        int group_size = moe.quant_group_size > 0 ? moe.quant_group_size : 64;
        return mx::dequantize(weight, scales, biases, group_size, bits);
    }

    // Get LM head weights with dequantization if quantized
    mx::array get_lm_head_weights() {
        auto lm_head_ptr = find_weight("lm_head.weight");
        if (!lm_head_ptr) {
            throw std::runtime_error("LM head weights not found");
        }

        // Check if lm_head is quantized (has companion scales)
        auto scales_it = weights.find("lm_head.scales");
        if (scales_it == weights.end()) {
            // Not quantized, return raw weights
            return *lm_head_ptr;
        }

        // Dequantize lm_head
        mx::array weight = *lm_head_ptr;
        mx::array scales = scales_it->second;
        mx::array biases = mx::zeros({static_cast<int>(weight.shape(0))}, scales.dtype());
        auto biases_it = weights.find("lm_head.biases");
        if (biases_it != weights.end()) {
            biases = biases_it->second;
        }

        int bits = moe.quant_bits > 0 ? moe.quant_bits : 4;
        int group_size = moe.quant_group_size > 0 ? moe.quant_group_size : 64;
        return mx::dequantize(weight, scales, biases, group_size, bits);
    }

    // Real transformer forward pass
    mx::array forward(const mx::array& input_ids) {
        try {
            // Get embedding weights (dequantized if necessary)
            mx::array embed_weights = get_embedding_weights();

            // Embedding lookup: [seq_len] -> [seq_len, hidden_size]
            mx::array hidden = mx::take(embed_weights, input_ids, 0);

            // Ensure hidden has batch dimension [batch, seq_len, hidden_dim]
            // Input_ids may be [seq_len] resulting in hidden [seq_len, hidden_dim]
            // We need [1, seq_len, hidden_dim] for transformer layers
            if (hidden.ndim() == 2) {
                hidden = mx::expand_dims(hidden, 0);  // Add batch dimension
            }

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

            // Language modeling head (dequantized if necessary)
            mx::array lm_head = get_lm_head_weights();

            // Project to vocabulary: [batch_size, seq_len, hidden_size] -> [batch_size, seq_len, vocab_size]
            mx::array logits = mx::matmul(hidden, mx::transpose(lm_head));

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
        mx::array mlp_output = mlp_forward(residual, prefix + ".mlp", layer_idx);

        // Final residual
        return residual + mlp_output;
    }

    // Self-attention mechanism using scaled dot-product attention
    // Supports Grouped Query Attention (GQA) where num_key_value_heads < num_attention_heads
    // Set capture_hidden=true to record QKV and output projections for analysis
    mx::array self_attention_impl(const mx::array& hidden, const std::string& prefix, bool capture_hidden) {
        int batch_size = hidden.shape(0);
        int seq_len = hidden.shape(1);
        int n_heads = num_attention_heads;
        int n_kv_heads = num_key_value_heads;
        int hd = head_dim;
        int n_rep = n_heads / n_kv_heads;

        // QKV projections
        mx::array q = linear_projection(hidden, prefix + ".q_proj");
        mx::array k = linear_projection(hidden, prefix + ".k_proj");
        mx::array v = linear_projection(hidden, prefix + ".v_proj");

        if (capture_hidden) {
            mx::eval(q); hidden_states_vec.push_back({prefix + ".q_proj", q});
            mx::eval(k); hidden_states_vec.push_back({prefix + ".k_proj", k});
            mx::eval(v); hidden_states_vec.push_back({prefix + ".v_proj", v});
        }

        // Reshape for multi-head attention
        q = mx::reshape(q, {batch_size, seq_len, n_heads, hd});
        k = mx::reshape(k, {batch_size, seq_len, n_kv_heads, hd});
        v = mx::reshape(v, {batch_size, seq_len, n_kv_heads, hd});

        // GQA: Repeat K,V heads to match Q heads if needed
        if (n_rep > 1) {
            k = mx::reshape(mx::repeat(mx::expand_dims(k, 3), n_rep, 3), {batch_size, seq_len, n_heads, hd});
            v = mx::reshape(mx::repeat(mx::expand_dims(v, 3), n_rep, 3), {batch_size, seq_len, n_heads, hd});
        }

        // Transpose for attention: [batch, seq, heads, head_dim] -> [batch, heads, seq, head_dim]
        q = mx::transpose(q, {0, 2, 1, 3});
        k = mx::transpose(k, {0, 2, 1, 3});
        v = mx::transpose(v, {0, 2, 1, 3});

        // Create causal mask
        std::vector<float> mask_data(seq_len * seq_len, 0.0f);
        for (int i = 0; i < seq_len; ++i)
            for (int j = i + 1; j < seq_len; ++j)
                mask_data[i * seq_len + j] = -1e9f;
        mx::array causal_mask = mx::array(mask_data.data(), {seq_len, seq_len}, mx::float32);

        // Scaled dot-product attention
        float scale = 1.0f / std::sqrt(static_cast<float>(hd));
        mx::array scores = mx::multiply(mx::matmul(q, mx::transpose(k, {0, 1, 3, 2})), mx::array(scale));
        mx::array attn_output = mx::matmul(mx::softmax(mx::add(scores, causal_mask), -1), v);

        // Reshape back: [batch, heads, seq, head_dim] -> [batch, seq, hidden_size]
        attn_output = mx::reshape(mx::transpose(attn_output, {0, 2, 1, 3}), {batch_size, seq_len, n_heads * hd});

        // Output projection
        mx::array output = linear_projection(attn_output, prefix + ".o_proj");

        if (capture_hidden) {
            mx::eval(output);
            hidden_states_vec.push_back({prefix + ".o_proj", output});
        }

        return output;
    }

    mx::array self_attention(const mx::array& hidden, const std::string& prefix) {
        return self_attention_impl(hidden, prefix, false);
    }

    mx::array self_attention_with_hidden_states(const mx::array& hidden, const std::string& prefix) {
        return self_attention_impl(hidden, prefix, true);
    }

    // Linear projection helper with dequantization support
    mx::array linear_projection(const mx::array& input, const std::string& weight_key) {
        // Check if this weight exists
        auto weight_ptr = find_weight(weight_key + ".weight");
        if (!weight_ptr) return input;  // Fallback if weight not found

        // Check if weight is quantized (has companion scales)
        auto scales_it = weights.find(weight_key + ".scales");
        if (scales_it != weights.end()) {
            // Use dequantized weight for quantized models
            mx::array weight = get_weight_with_dequant(weight_key);
            return mx::matmul(input, mx::transpose(weight));
        }

        // Non-quantized weight, use directly
        return mx::matmul(input, mx::transpose(*weight_ptr));
    }

    // Check if the current layer is an MoE layer by inspecting stacked switch MLP weights
    bool is_moe_layer(int layer_idx) const {
        if (!moe.enabled) return false;
        std::string up_key =
            "model.layers." + std::to_string(layer_idx) + ".mlp.switch_mlp.up_proj.weight";
        return weights.find(up_key) != weights.end();
    }

    // Dequantize a weight if companion scales (and optional biases) are present
    mx::array get_weight_with_dequant(const std::string& base_key) {
        auto weight_it = weights.find(base_key + ".weight");
        if (weight_it == weights.end()) {
            throw std::runtime_error("Weight not found: " + base_key + ".weight");
        }

        auto scales_it = weights.find(base_key + ".scales");
        if (scales_it == weights.end()) {
            return weight_it->second;
        }

        mx::array weight = weight_it->second;
        mx::array scales = scales_it->second;
        mx::array biases = mx::zeros({1}, scales.dtype());
        auto biases_it = weights.find(base_key + ".biases");
        if (biases_it != weights.end()) {
            biases = biases_it->second;
        } else {
            // Bias shape follows the leading dims of the weight tensor
            if (weight.ndim() == 3) {
                biases =
                    mx::zeros({static_cast<int>(weight.shape(0)), static_cast<int>(weight.shape(1))},
                              scales.dtype());
            } else {
                biases = mx::zeros({static_cast<int>(weight.shape(0))}, scales.dtype());
            }
        }

        int bits = moe.quant_bits > 0 ? moe.quant_bits : 4;
        int group_size = moe.quant_group_size > 0 ? moe.quant_group_size : 64;
        return mx::dequantize(weight, scales, biases, group_size, bits);
    }

    // MLX C++ topk returns values only; build indices via argsort for MoE routing.
    std::pair<mx::array, mx::array> topk_with_indices(
        const mx::array& input, int k, int axis = -1) {
        int ndim = static_cast<int>(input.ndim());
        int resolved_axis = axis < 0 ? ndim + axis : axis;
        int axis_dim = static_cast<int>(input.shape(resolved_axis));
        int k_clamped = std::min(k, axis_dim);

        mx::array sorted_indices = mx::argsort(mx::negative(input), resolved_axis);

        mx::Shape start(ndim, 0);
        mx::Shape stop = input.shape();
        stop[resolved_axis] = k_clamped;

        mx::array top_indices = mx::slice(sorted_indices, start, stop);
        mx::array top_values = mx::take_along_axis(input, top_indices, resolved_axis);
        return {top_values, top_indices};
    }

    // Expert router: compute top-k expert indices and scores
    std::pair<mx::array, mx::array> moe_router(const mx::array& x, const std::string& prefix) {
        mx::array gate_weight = get_weight_with_dequant(prefix + ".gate");
        mx::array logits = mx::matmul(x, mx::transpose(gate_weight));

        // Add bias if present (biases are per-expert)
        auto bias_it = weights.find(prefix + ".gate.biases");
        if (bias_it != weights.end()) {
            // Broadcast bias over batch/seq
            mx::array bias = bias_it->second;
            logits = logits + mx::reshape(bias, {1, 1, bias.shape(0)});
        }

        mx::array scores = mx::softmax(logits, -1);
        int axis = static_cast<int>(scores.ndim()) - 1;
        int last_dim = scores.shape(axis);
        int k = std::min(moe.num_experts_per_tok, last_dim);

        auto [top_scores, inds] = topk_with_indices(scores, k, axis);
        if (moe.norm_topk_prob) {
            top_scores = mx::divide(top_scores, mx::sum(top_scores, axis, true));
        }

        return {top_scores, inds};
    }

    // SwitchLinear gather for stacked expert weights
    mx::array apply_switch_linear(
        const mx::array& x_expanded, const std::string& base_key, const mx::array& inds) {
        mx::array weight = get_weight_with_dequant(base_key);

        // swap last two dims to align for matmul inside gather_mm
        mx::array rhs = mx::transpose(weight, {0, 2, 1});
        mx::array out = mx::gather_mm(x_expanded, rhs, inds);

        auto bias_it = weights.find(base_key + ".biases");
        if (bias_it != weights.end()) {
            mx::array bias = bias_it->second;
            // bias shape: [num_experts, output_dim]
            // gather along expert axis then expand to match out
            mx::array gathered = mx::take(bias, inds, 0);  // [B,L,k,out]
            out = out + gathered;
        }
        return out;
    }

    // SwitchGLU forward for MoE experts
    mx::array switch_glu_forward(
        const mx::array& x, const std::string& prefix, const mx::array& inds,
        const mx::array& scores) {
        // Expand for gather_mm: [B, L, 1, 1, D]
        mx::array x_exp = mx::expand_dims(mx::expand_dims(x, 2), 3);

        mx::array up_out = apply_switch_linear(x_exp, prefix + ".switch_mlp.up_proj", inds);
        mx::array gate_out = apply_switch_linear(x_exp, prefix + ".switch_mlp.gate_proj", inds);

        // SwiGLU: SiLU(gate) * up = gate * sigmoid(gate) * up
        mx::array silu_gate = mx::multiply(gate_out, mx::sigmoid(gate_out));
        mx::array activated = mx::multiply(silu_gate, up_out);
        mx::array activated_exp = mx::expand_dims(activated, 3);

        mx::array down_out =
            apply_switch_linear(activated_exp, prefix + ".switch_mlp.down_proj", inds);

        mx::array scores_exp = mx::expand_dims(scores, -1);
        return mx::sum(mx::multiply(down_out, scores_exp), 2);
    }

    // Full MoE MLP block: router + SwitchGLU
    mx::array mlp_moe_forward(const mx::array& input, const std::string& prefix) {
        auto [top_scores, inds] = moe_router(input, prefix);
        return switch_glu_forward(input, prefix, inds, top_scores);
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
    mx::array mlp_forward(const mx::array& input, const std::string& prefix,
                          int layer_idx = -1) {
        if (layer_idx >= 0 && is_moe_layer(layer_idx)) {
            return mlp_moe_forward(input, prefix);
        }

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
            // Get embedding weights (dequantized if necessary)
            mx::array embed_weights = get_embedding_weights();

            // Embedding lookup
            mx::array hidden = mx::take(embed_weights, input_ids, 0);

            // Ensure hidden has batch dimension [batch, seq_len, hidden_dim]
            // Input_ids may be [seq_len] resulting in hidden [seq_len, hidden_dim]
            // We need [1, seq_len, hidden_dim] for transformer layers
            if (hidden.ndim() == 2) {
                hidden = mx::expand_dims(hidden, 0);  // Add batch dimension
            }

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
                mx::array mlp_output = mlp_forward(
                    hidden, std::string("model.layers.") + std::to_string(layer_idx) + ".mlp",
                    layer_idx);

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

            // Language modeling head (dequantized if necessary)
            mx::array lm_head = get_lm_head_weights();

            mx::array logits = mx::matmul(hidden, mx::transpose(lm_head));
            return logits;
        } catch (const std::exception& e) {
            g_last_error = std::string("Forward with hidden states failed: ") + e.what();
            throw;
        }
    }
};

// Array creation (only from_ints is actually used)
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

// Array property access (only data, size, free are used)
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
    } catch (const std::string& e) {
        g_last_error = e;
        return nullptr;
    } catch (const char* e) {
        g_last_error = e;
        return nullptr;
    } catch (...) {
        g_last_error = "Failed to load MLX model: unknown exception";
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
    } catch (const std::string& e) {
        g_last_error = e;
        return nullptr;
    } catch (const char* e) {
        g_last_error = e;
        return nullptr;
    } catch (...) {
        g_last_error = "Failed to load model from buffer: unknown exception";
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

// Get the name of a hidden state at the given index
// Returns the actual names stored in the model's hidden_states_vec during forward pass
extern "C" int mlx_model_get_hidden_state_name(
    mlx_model_t* model,
    int index,
    char* out_name,
    int out_name_len
) {
    if (!model || index < 0) return 0;

    auto model_wrapper = reinterpret_cast<MLXModelWrapper*>(model);
    const auto& hidden_states_vec = model_wrapper->hidden_states_vec;

    if (static_cast<size_t>(index) >= hidden_states_vec.size()) return 0;

    const std::string& name = hidden_states_vec[index].first;
    int name_len = static_cast<int>(name.length());

    // If buffer provided and large enough, copy the name
    if (out_name && out_name_len > name_len) {
        std::memcpy(out_name, name.c_str(), name_len + 1); // Include null terminator
    }

    return name_len;
}

// Get the number of hidden states stored in the model
extern "C" int mlx_model_get_hidden_state_count(mlx_model_t* model) {
    if (!model) return 0;
    auto model_wrapper = reinterpret_cast<MLXModelWrapper*>(model);
    return static_cast<int>(model_wrapper->hidden_states_vec.size());
}

// Get a specific weight tensor from the model by name
extern "C" mlx_array_t* mlx_model_get_weight(mlx_model_t* model, const char* weight_name) {
    if (!model || !weight_name) {
        g_last_error = "Model and weight_name are required";
        return nullptr;
    }

    try {
        auto model_w = reinterpret_cast<MLXModelWrapper*>(model);
        std::string name(weight_name);

        // Look up the weight in the model's weight map
        auto it = model_w->weights.find(name);
        if (it == model_w->weights.end()) {
            // Try with "model." prefix (some models use this convention)
            it = model_w->weights.find("model." + name);
        }

        if (it == model_w->weights.end()) {
            g_last_error = "Weight not found: " + name;
            return nullptr;
        }

        // Return a copy of the weight tensor
        return reinterpret_cast<mlx_array_t*>(new MLXArrayWrapper(it->second));

    } catch (const std::exception& e) {
        g_last_error = std::string("Failed to get weight: ") + e.what();
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
// Tensor operations (required by Rust tensor module)
// ============================================================================

extern "C" mlx_array_t* mlx_add(mlx_array_t* a, mlx_array_t* b) {
    if (!a || !b) return nullptr;
    try {
        auto wrapper_a = reinterpret_cast<MLXArrayWrapper*>(a);
        auto wrapper_b = reinterpret_cast<MLXArrayWrapper*>(b);
        mx::array result = mx::add(wrapper_a->arr, wrapper_b->arr);
        return reinterpret_cast<mlx_array_t*>(new MLXArrayWrapper(result));
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

extern "C" mlx_array_t* mlx_array_copy(mlx_array_t* array) {
    if (!array) return nullptr;
    try {
        auto wrapper = reinterpret_cast<MLXArrayWrapper*>(array);
        // MLX arrays use reference counting; create copy via multiply by 1
        mx::array copy = mx::multiply(wrapper->arr, mx::array(1.0f));
        mx::synchronize();  // Force materialization
        return reinterpret_cast<mlx_array_t*>(new MLXArrayWrapper(copy));
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

extern "C" int mlx_array_dtype(mlx_array_t* array) {
    if (!array) return -1;
    try {
        auto wrapper = reinterpret_cast<MLXArrayWrapper*>(array);
        mx::Dtype dtype = wrapper->arr.dtype();
        // Map MLX dtypes to Rust TensorDtype enum codes
        // TensorDtype { Float32 = 0, Int32 = 2, UInt32 = 3 }
        if (dtype == mx::float32) return 0;  // Float32
        if (dtype == mx::int32) return 2;    // Int32
        if (dtype == mx::uint32) return 3;   // UInt32
        // Return Float32 as default for other types
        return 0;
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return -1;
    }
}

extern "C" mlx_array_t* mlx_array_from_data(const float* data, int size) {
    if (!data || size <= 0) return nullptr;
    try {
        std::vector<float> vec(data, data + size);
        mx::array arr = mx::array(vec.begin(), make_shape(size), mx::float32);
        return reinterpret_cast<mlx_array_t*>(new MLXArrayWrapper(arr));
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

extern "C" int mlx_array_ndim(mlx_array_t* array) {
    if (!array) return 0;
    try {
        auto wrapper = reinterpret_cast<MLXArrayWrapper*>(array);
        return static_cast<int>(wrapper->arr.ndim());
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return 0;
    }
}

extern "C" mlx_array_t* mlx_array_reshape(mlx_array_t* array, const int* shape, int ndim) {
    if (!array || !shape || ndim <= 0) return nullptr;
    try {
        auto wrapper = reinterpret_cast<MLXArrayWrapper*>(array);
        mx::Shape new_shape;
        for (int i = 0; i < ndim; ++i) {
            new_shape.push_back(static_cast<int32_t>(shape[i]));
        }
        mx::array result = mx::reshape(wrapper->arr, new_shape);
        return reinterpret_cast<mlx_array_t*>(new MLXArrayWrapper(result));
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

extern "C" int mlx_array_shape(mlx_array_t* array, int* out_shape, int max_dims) {
    if (!array || !out_shape || max_dims <= 0) return 0;
    try {
        auto wrapper = reinterpret_cast<MLXArrayWrapper*>(array);
        const auto& shape = wrapper->arr.shape();
        int ndim = std::min(static_cast<int>(shape.size()), max_dims);
        for (int i = 0; i < ndim; ++i) {
            out_shape[i] = static_cast<int>(shape[i]);
        }
        return ndim;
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return 0;
    }
}

extern "C" mlx_array_t* mlx_array_transpose(mlx_array_t* array) {
    if (!array) return nullptr;
    try {
        auto wrapper = reinterpret_cast<MLXArrayWrapper*>(array);
        mx::array result = mx::transpose(wrapper->arr);
        return reinterpret_cast<mlx_array_t*>(new MLXArrayWrapper(result));
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

extern "C" mlx_array_t* mlx_multiply(mlx_array_t* a, mlx_array_t* b) {
    if (!a || !b) return nullptr;
    try {
        auto wrapper_a = reinterpret_cast<MLXArrayWrapper*>(a);
        auto wrapper_b = reinterpret_cast<MLXArrayWrapper*>(b);
        mx::array result = mx::multiply(wrapper_a->arr, wrapper_b->arr);
        return reinterpret_cast<mlx_array_t*>(new MLXArrayWrapper(result));
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

extern "C" mlx_array_t* mlx_matmul(mlx_array_t* a, mlx_array_t* b) {
    if (!a || !b) return nullptr;
    try {
        auto wrapper_a = reinterpret_cast<MLXArrayWrapper*>(a);
        auto wrapper_b = reinterpret_cast<MLXArrayWrapper*>(b);
        mx::array result = mx::matmul(wrapper_a->arr, wrapper_b->arr);
        return reinterpret_cast<mlx_array_t*>(new MLXArrayWrapper(result));
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}

// ============================================================================
// Training Operations - Loss Functions
// ============================================================================

extern "C" mlx_array_t* mlx_cross_entropy_loss(
    mlx_array_t* logits,
    mlx_array_t* targets,
    int ignore_index
) {
    if (!logits || !targets) {
        g_last_error = "logits and targets are required";
        return nullptr;
    }

    try {
        auto logits_w = reinterpret_cast<MLXArrayWrapper*>(logits);
        auto targets_w = reinterpret_cast<MLXArrayWrapper*>(targets);

        mx::array log_probs = logits_w->arr;
        mx::array target_ids = targets_w->arr;

        // Get shapes for reshaping
        int vocab_size = static_cast<int>(log_probs.shape(-1));

        // Flatten logits to [N, vocab_size] and targets to [N]
        mx::array flat_logits = mx::reshape(log_probs, {-1, vocab_size});
        mx::array flat_targets = mx::reshape(target_ids, {-1});
        int num_tokens = static_cast<int>(flat_targets.size());

        // Log softmax for numerical stability
        mx::array log_softmax_result = mx::subtract(
            flat_logits,
            mx::expand_dims(mx::logsumexp(flat_logits, -1), -1)
        );

        // Compute negative log probs
        mx::array neg_log_probs = mx::negative(log_softmax_result);

        // Create mask for valid tokens (not ignore_index)
        mx::array mask = mx::ones({num_tokens}, mx::float32);
        if (ignore_index >= 0) {
            mask = mx::astype(
                mx::not_equal(flat_targets, mx::array(ignore_index)),
                mx::float32
            );
        }

        // Compute loss per token by gathering target log probs directly
        // Instead of one-hot encoding, use take_along_axis for efficiency
        // Reshape flat_targets to [seq_len, 1] for gathering
        mx::array target_indices = mx::reshape(flat_targets, {static_cast<int>(flat_targets.size()), 1});
        mx::array target_log_probs = mx::squeeze(
            mx::take_along_axis(neg_log_probs, target_indices, 1),
            1
        );

        // Apply mask and compute mean
        mx::array masked_loss = mx::multiply(target_log_probs, mask);
        mx::array valid_count = mx::maximum(mx::sum(mask), mx::array(1.0f));
        mx::array loss = mx::divide(mx::sum(masked_loss), valid_count);

        // Force computation (MLX lazy evaluation)
        mx::synchronize();
        return reinterpret_cast<mlx_array_t*>(new MLXArrayWrapper(loss));

    } catch (const std::exception& e) {
        g_last_error = std::string("Cross entropy loss failed: ") + e.what();
        return nullptr;
    }
}

extern "C" mlx_array_t* mlx_mse_loss(
    mlx_array_t* predictions,
    mlx_array_t* targets
) {
    if (!predictions || !targets) {
        g_last_error = "predictions and targets are required";
        return nullptr;
    }

    try {
        auto pred_w = reinterpret_cast<MLXArrayWrapper*>(predictions);
        auto targ_w = reinterpret_cast<MLXArrayWrapper*>(targets);

        // MSE: mean((pred - targ)^2)
        mx::array diff = mx::subtract(pred_w->arr, targ_w->arr);
        mx::array squared = mx::multiply(diff, diff);
        mx::array loss = mx::mean(squared);

        // Force computation
        mx::synchronize();
        return reinterpret_cast<mlx_array_t*>(new MLXArrayWrapper(loss));

    } catch (const std::exception& e) {
        g_last_error = std::string("MSE loss failed: ") + e.what();
        return nullptr;
    }
}

// ============================================================================
// Training Operations - Gradient Computation
// ============================================================================

extern "C" int mlx_lora_backward(
    mlx_array_t* hidden,
    mlx_array_t* targets,
    mlx_array_t* lora_a,
    mlx_array_t* lora_b,
    float alpha,
    int rank,
    float* out_loss,
    mlx_array_t** out_grad_a,
    mlx_array_t** out_grad_b
) {
    if (!hidden || !targets || !lora_a || !lora_b || !out_loss || !out_grad_a || !out_grad_b) {
        g_last_error = "All parameters are required for mlx_lora_backward";
        return -1;
    }

    try {
        auto hidden_w = reinterpret_cast<MLXArrayWrapper*>(hidden);
        auto targets_w = reinterpret_cast<MLXArrayWrapper*>(targets);
        auto lora_a_w = reinterpret_cast<MLXArrayWrapper*>(lora_a);
        auto lora_b_w = reinterpret_cast<MLXArrayWrapper*>(lora_b);

        // Capture arrays for closure
        mx::array h = hidden_w->arr;
        mx::array t = targets_w->arr;
        float scale = alpha / static_cast<float>(rank);

        // Define the loss function that takes LoRA params
        auto loss_fn = [&h, &t, scale](const std::vector<mx::array>& params) -> mx::array {
            const mx::array& a = params[0];  // LoRA A: [rank, hidden_dim]
            const mx::array& b = params[1];  // LoRA B: [hidden_dim, rank]

            // LoRA forward: output = hidden + (hidden @ A^T @ B^T) * scale
            mx::array lora_out = mx::matmul(
                mx::matmul(h, mx::transpose(a)),
                mx::transpose(b)
            );
            lora_out = mx::multiply(lora_out, mx::array(scale));
            mx::array output = mx::add(h, lora_out);

            // MSE loss against targets
            mx::array diff = mx::subtract(output, t);
            return mx::mean(mx::multiply(diff, diff));
        };

        // Create parameter vector
        std::vector<mx::array> params = {lora_a_w->arr, lora_b_w->arr};

        // Use value_and_grad to compute loss and gradients
        auto grad_fn = mx::value_and_grad(loss_fn);
        auto [loss, grads] = grad_fn(params);

        // Force computation and synchronization for determinism
        // Use mx::synchronize() which waits for all pending computations
        mx::synchronize();

        // Extract scalar loss value
        *out_loss = loss.item<float>();

        // Wrap gradients
        *out_grad_a = reinterpret_cast<mlx_array_t*>(new MLXArrayWrapper(grads[0]));
        *out_grad_b = reinterpret_cast<mlx_array_t*>(new MLXArrayWrapper(grads[1]));

        return 0;

    } catch (const std::exception& e) {
        g_last_error = std::string("LoRA backward failed: ") + e.what();
        return -1;
    }
}

// LoRA backward pass with cross-entropy loss for language model training
// This variant projects hidden states to vocabulary logits before computing loss
extern "C" int mlx_lora_backward_ce(
    mlx_array_t* hidden,
    mlx_array_t* output_proj,
    mlx_array_t* targets,
    mlx_array_t* lora_a,
    mlx_array_t* lora_b,
    float alpha,
    int rank,
    int ignore_index,
    float* out_loss,
    mlx_array_t** out_grad_a,
    mlx_array_t** out_grad_b
) {
    if (!hidden || !output_proj || !targets || !lora_a || !lora_b || !out_loss || !out_grad_a || !out_grad_b) {
        g_last_error = "All parameters are required for mlx_lora_backward_ce";
        return -1;
    }

    try {
        auto hidden_w = reinterpret_cast<MLXArrayWrapper*>(hidden);
        auto output_proj_w = reinterpret_cast<MLXArrayWrapper*>(output_proj);
        auto targets_w = reinterpret_cast<MLXArrayWrapper*>(targets);
        auto lora_a_w = reinterpret_cast<MLXArrayWrapper*>(lora_a);
        auto lora_b_w = reinterpret_cast<MLXArrayWrapper*>(lora_b);

        // Capture arrays for closure
        mx::array h = hidden_w->arr;           // [batch, seq_len, hidden_dim] or [seq_len, hidden_dim]
        mx::array proj = output_proj_w->arr;   // [vocab_size, hidden_dim] - lm_head weights
        mx::array t = targets_w->arr;          // [batch, seq_len] or [seq_len] - target token IDs
        float scale = alpha / static_cast<float>(rank);
        int ignore_idx = ignore_index;

        // Get shapes for loss computation
        int vocab_size = static_cast<int>(proj.shape(0));

        // Define the loss function that takes LoRA params
        auto loss_fn = [&h, &proj, &t, scale, vocab_size, ignore_idx](const std::vector<mx::array>& params) -> mx::array {
            const mx::array& a = params[0];  // LoRA A: [rank, hidden_dim]
            const mx::array& b = params[1];  // LoRA B: [hidden_dim, rank]

            // LoRA forward: h' = hidden + (hidden @ A^T @ B^T) * scale
            mx::array lora_out = mx::matmul(
                mx::matmul(h, mx::transpose(a)),
                mx::transpose(b)
            );
            lora_out = mx::multiply(lora_out, mx::array(scale));
            mx::array h_prime = mx::add(h, lora_out);

            // Project to vocabulary: logits = h' @ proj^T
            // proj is [vocab_size, hidden_dim], so we need h' @ proj^T = [seq_len, vocab_size]
            mx::array logits = mx::matmul(h_prime, mx::transpose(proj));

            // Cross-entropy loss computation
            // Flatten for loss computation
            mx::array flat_logits = mx::reshape(logits, {-1, vocab_size});
            mx::array flat_targets = mx::reshape(t, {-1});
            int num_tokens = static_cast<int>(flat_targets.size());

            // Log softmax for numerical stability
            mx::array log_softmax_result = mx::subtract(
                flat_logits,
                mx::expand_dims(mx::logsumexp(flat_logits, -1), -1)
            );

            // Compute negative log probs
            mx::array neg_log_probs = mx::negative(log_softmax_result);

            // Create mask for valid tokens (not ignore_index)
            mx::array mask = mx::ones({num_tokens}, mx::float32);
            if (ignore_idx >= 0) {
                mask = mx::astype(
                    mx::not_equal(flat_targets, mx::array(ignore_idx)),
                    mx::float32
                );
            }

            // Gather target log probs using take_along_axis
            mx::array target_indices = mx::reshape(flat_targets, {num_tokens, 1});
            mx::array target_log_probs = mx::squeeze(
                mx::take_along_axis(neg_log_probs, target_indices, 1),
                1
            );

            // Apply mask and compute mean
            mx::array masked_loss = mx::multiply(target_log_probs, mask);
            mx::array valid_count = mx::maximum(mx::sum(mask), mx::array(1.0f));
            return mx::divide(mx::sum(masked_loss), valid_count);
        };

        // Create parameter vector
        std::vector<mx::array> params = {lora_a_w->arr, lora_b_w->arr};

        // Use value_and_grad to compute loss and gradients
        auto grad_fn = mx::value_and_grad(loss_fn);
        auto [loss, grads] = grad_fn(params);

        // Force computation and synchronization for determinism
        mx::eval({loss, grads[0], grads[1]});
        mx::synchronize();

        // Extract scalar loss value
        *out_loss = loss.item<float>();

        // Wrap gradients
        *out_grad_a = reinterpret_cast<mlx_array_t*>(new MLXArrayWrapper(grads[0]));
        *out_grad_b = reinterpret_cast<mlx_array_t*>(new MLXArrayWrapper(grads[1]));

        return 0;

    } catch (const std::exception& e) {
        g_last_error = std::string("LoRA backward (CE) failed: ") + e.what();
        return -1;
    }
}

// ============================================================================
// Training Operations - Optimizer State
// ============================================================================

struct MLXOptimizerState {
    mlx_optimizer_type_t type;
    float learning_rate;
    float momentum;
    float weight_decay;
    float beta1;
    float beta2;
    float eps;
    int step_count;
    std::vector<mx::array> m;  // First moment (Adam) or velocity (SGD)
    std::vector<mx::array> v;  // Second moment (Adam only)
    bool initialized;
    size_t allocated_bytes;

    MLXOptimizerState() : type(MLX_OPTIM_SGD), learning_rate(0.001f),
                          momentum(0.0f), weight_decay(0.0f),
                          beta1(0.9f), beta2(0.999f), eps(1e-8f),
                          step_count(0), initialized(false), allocated_bytes(0) {
        record_allocation(reinterpret_cast<uintptr_t>(this), sizeof(MLXOptimizerState));
    }

    ~MLXOptimizerState() {
        unrecord_allocation(reinterpret_cast<uintptr_t>(this));
    }
};

extern "C" mlx_optimizer_t* mlx_optimizer_sgd(
    float learning_rate,
    float momentum,
    float weight_decay
) {
    try {
        auto opt = new MLXOptimizerState();
        opt->type = MLX_OPTIM_SGD;
        opt->learning_rate = learning_rate;
        opt->momentum = momentum;
        opt->weight_decay = weight_decay;
        return reinterpret_cast<mlx_optimizer_t*>(opt);
    } catch (const std::exception& e) {
        g_last_error = std::string("Failed to create SGD optimizer: ") + e.what();
        return nullptr;
    }
}

extern "C" mlx_optimizer_t* mlx_optimizer_adam(
    float learning_rate,
    float beta1,
    float beta2,
    float eps,
    float weight_decay
) {
    try {
        auto opt = new MLXOptimizerState();
        opt->type = MLX_OPTIM_ADAM;
        opt->learning_rate = learning_rate;
        opt->beta1 = beta1;
        opt->beta2 = beta2;
        opt->eps = eps;
        opt->weight_decay = weight_decay;
        return reinterpret_cast<mlx_optimizer_t*>(opt);
    } catch (const std::exception& e) {
        g_last_error = std::string("Failed to create Adam optimizer: ") + e.what();
        return nullptr;
    }
}

extern "C" int mlx_optimizer_step(
    mlx_optimizer_t* optimizer,
    mlx_array_t** params,
    mlx_array_t** grads,
    int num_params
) {
    if (!optimizer || !params || !grads || num_params <= 0) {
        g_last_error = "Invalid optimizer step arguments";
        return -1;
    }

    try {
        auto opt = reinterpret_cast<MLXOptimizerState*>(optimizer);
        opt->step_count++;

        // Initialize state vectors if needed
        // Note: mx::array has no default constructor, so we initialize with scalar zeros
        // and check size() == 1 to detect uninitialized state (will be replaced with zeros_like)
        if (!opt->initialized) {
            opt->m.clear();
            opt->m.reserve(num_params);
            for (int j = 0; j < num_params; ++j) {
                opt->m.push_back(mx::array(0.0f));  // Scalar placeholder
            }
            if (opt->type == MLX_OPTIM_ADAM || opt->type == MLX_OPTIM_ADAMW) {
                opt->v.clear();
                opt->v.reserve(num_params);
                for (int j = 0; j < num_params; ++j) {
                    opt->v.push_back(mx::array(0.0f));  // Scalar placeholder
                }
            }
            opt->initialized = true;
        }

        for (int i = 0; i < num_params; ++i) {
            if (!params[i] || !grads[i]) continue;

            auto param_w = reinterpret_cast<MLXArrayWrapper*>(params[i]);
            auto grad_w = reinterpret_cast<MLXArrayWrapper*>(grads[i]);

            mx::array& p = param_w->arr;
            mx::array g = grad_w->arr;

            // Apply weight decay (L2 regularization)
            if (opt->weight_decay > 0.0f && opt->type != MLX_OPTIM_ADAMW) {
                g = mx::add(g, mx::multiply(p, mx::array(opt->weight_decay)));
            }

            if (opt->type == MLX_OPTIM_SGD) {
                if (opt->momentum > 0.0f) {
                    // Check if state needs initialization (scalar placeholder has size 1)
                    if (opt->m[i].size() <= 1) {
                        opt->m[i] = mx::zeros_like(p);
                    }
                    opt->m[i] = mx::add(
                        mx::multiply(opt->m[i], mx::array(opt->momentum)),
                        g
                    );
                    p = mx::subtract(p, mx::multiply(opt->m[i], mx::array(opt->learning_rate)));
                } else {
                    p = mx::subtract(p, mx::multiply(g, mx::array(opt->learning_rate)));
                }

            } else if (opt->type == MLX_OPTIM_ADAM || opt->type == MLX_OPTIM_ADAMW) {
                // Check if state needs initialization (scalar placeholder has size 1)
                if (opt->m[i].size() <= 1) {
                    opt->m[i] = mx::zeros_like(p);
                    opt->v[i] = mx::zeros_like(p);
                }

                if (opt->type == MLX_OPTIM_ADAMW && opt->weight_decay > 0.0f) {
                    p = mx::subtract(p, mx::multiply(p, mx::array(opt->learning_rate * opt->weight_decay)));
                }

                opt->m[i] = mx::add(
                    mx::multiply(opt->m[i], mx::array(opt->beta1)),
                    mx::multiply(g, mx::array(1.0f - opt->beta1))
                );

                opt->v[i] = mx::add(
                    mx::multiply(opt->v[i], mx::array(opt->beta2)),
                    mx::multiply(mx::multiply(g, g), mx::array(1.0f - opt->beta2))
                );

                float bc1 = 1.0f - std::pow(opt->beta1, static_cast<float>(opt->step_count));
                float bc2 = 1.0f - std::pow(opt->beta2, static_cast<float>(opt->step_count));

                mx::array m_hat = mx::divide(opt->m[i], mx::array(bc1));
                mx::array v_hat = mx::divide(opt->v[i], mx::array(bc2));

                mx::array update = mx::divide(
                    m_hat,
                    mx::add(mx::sqrt(v_hat), mx::array(opt->eps))
                );
                p = mx::subtract(p, mx::multiply(update, mx::array(opt->learning_rate)));
            }
        }

        // Force computation for determinism
        mx::synchronize();
        return 0;

    } catch (const std::exception& e) {
        g_last_error = std::string("Optimizer step failed: ") + e.what();
        return -1;
    }
}

extern "C" void mlx_optimizer_set_lr(mlx_optimizer_t* optimizer, float lr) {
    if (!optimizer) return;
    auto opt = reinterpret_cast<MLXOptimizerState*>(optimizer);
    opt->learning_rate = lr;
}

extern "C" float mlx_optimizer_get_lr(mlx_optimizer_t* optimizer) {
    if (!optimizer) return 0.0f;
    auto opt = reinterpret_cast<MLXOptimizerState*>(optimizer);
    return opt->learning_rate;
}

extern "C" void mlx_optimizer_reset(mlx_optimizer_t* optimizer) {
    if (!optimizer) return;
    auto opt = reinterpret_cast<MLXOptimizerState*>(optimizer);
    opt->step_count = 0;
    opt->m.clear();
    opt->v.clear();
    opt->initialized = false;
}

extern "C" void mlx_optimizer_free(mlx_optimizer_t* optimizer) {
    if (optimizer) {
        delete reinterpret_cast<MLXOptimizerState*>(optimizer);
    }
}

// ============================================================================
// Training Operations - Gradient Utilities
// ============================================================================

extern "C" float mlx_clip_grad_norm(
    mlx_array_t** grads,
    int num_grads,
    float max_norm
) {
    if (!grads || num_grads <= 0 || max_norm <= 0.0f) {
        return 0.0f;
    }

    try {
        mx::array total_norm_sq = mx::array(0.0f);

        for (int i = 0; i < num_grads; ++i) {
            if (!grads[i]) continue;
            auto grad_w = reinterpret_cast<MLXArrayWrapper*>(grads[i]);
            mx::array flat = mx::reshape(grad_w->arr, {-1});
            total_norm_sq = mx::add(total_norm_sq, mx::sum(mx::multiply(flat, flat)));
        }

        mx::array total_norm = mx::sqrt(total_norm_sq);
        mx::synchronize();

        float norm_val = total_norm.item<float>();

        if (norm_val > max_norm) {
            float scale = max_norm / norm_val;

            for (int i = 0; i < num_grads; ++i) {
                if (!grads[i]) continue;
                auto grad_w = reinterpret_cast<MLXArrayWrapper*>(grads[i]);
                grad_w->arr = mx::multiply(grad_w->arr, mx::array(scale));
            }
            mx::synchronize();
        }

        return norm_val;

    } catch (const std::exception& e) {
        g_last_error = std::string("Gradient clipping failed: ") + e.what();
        return 0.0f;
    }
}

extern "C" void mlx_zero_grad(
    mlx_array_t** grads,
    int num_grads
) {
    if (!grads || num_grads <= 0) return;

    try {
        for (int i = 0; i < num_grads; ++i) {
            if (!grads[i]) continue;
            auto grad_w = reinterpret_cast<MLXArrayWrapper*>(grads[i]);
            grad_w->arr = mx::zeros_like(grad_w->arr);
        }
        mx::synchronize();

    } catch (const std::exception& e) {
        g_last_error = std::string("Zero grad failed: ") + e.what();
    }
}

#else
// If MLX_REAL is not defined, fall back to stub
#warning "Compiling without real MLX support - using stub implementation"
// The stub implementation should be compiled separately
#endif
