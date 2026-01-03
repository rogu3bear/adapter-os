// MLX C++ wrapper stub implementation
// Used when MLX is not available (non-Apple-Silicon builds)
// Provides minimal CPU-backed behavior for test coverage.

#include "wrapper.h"

#include <algorithm>
#include <cmath>
#include <cstring>
#include <limits>
#include <string>
#include <vector>

static thread_local std::string g_last_error =
    "MLX not available (stub implementation)";

// Define opaque structs for stub usage
struct mlx_array {
    std::vector<float> data;
    std::vector<int> shape;
    int dtype;
};

namespace {
constexpr int kDtypeFloat32 = 0;
constexpr int kDtypeInt32 = 2;
constexpr int kDtypeUInt32 = 3;

void set_error(const std::string& msg) { g_last_error = msg; }

size_t shape_size(const std::vector<int>& shape) {
    size_t size = 1;
    for (int dim : shape) {
        if (dim <= 0) {
            return 0;
        }
        size *= static_cast<size_t>(dim);
    }
    return size;
}

mlx_array* make_array(std::vector<float> data, std::vector<int> shape, int dtype) {
    return new mlx_array{std::move(data), std::move(shape), dtype};
}

mlx_array* as_array(mlx_array_t* array) {
    return reinterpret_cast<mlx_array*>(array);
}

const mlx_array* as_array(const mlx_array_t* array) {
    return reinterpret_cast<const mlx_array*>(array);
}

std::vector<int> shape_from_ptr(const int* shape, int ndim) {
    if (!shape || ndim <= 0) {
        return {};
    }
    std::vector<int> result;
    result.reserve(static_cast<size_t>(ndim));
    for (int i = 0; i < ndim; ++i) {
        result.push_back(shape[i]);
    }
    return result;
}
}  // namespace

// Error handling
extern "C" const char* mlx_get_last_error(void) { return g_last_error.c_str(); }
extern "C" void mlx_clear_error(void) {
    g_last_error = "MLX not available (stub implementation)";
}

// Runtime - always report as not available
extern "C" int mlx_init(mlx_device_type_t) {
    set_error("MLX not available");
    return -1;
}
extern "C" int mlx_init_default(void) { return mlx_init(MLX_DEVICE_AUTO); }
extern "C" void mlx_shutdown(void) {}
extern "C" bool mlx_is_initialized(void) { return false; }
extern "C" mlx_device_type_t mlx_get_device_type(void) { return MLX_DEVICE_CPU; }
extern "C" int mlx_set_device(mlx_device_type_t) { return -1; }
extern "C" int mlx_backend_info(mlx_backend_capabilities_t* cap) {
    if (cap) {
        std::memset(cap, 0, sizeof(*cap));
    }
    return -1;
}
extern "C" const char* mlx_get_version(void) { return "stub-0.0.0"; }

// Model operations - all fail
extern "C" mlx_model_t* mlx_model_load(const char*) { return nullptr; }
extern "C" mlx_model_t* mlx_model_load_from_buffer(const uint8_t*, size_t, const char*) {
    return nullptr;
}
extern "C" void mlx_model_free(mlx_model_t*) {}
extern "C" mlx_array_t* mlx_model_forward(mlx_model_t*, mlx_array_t*) { return nullptr; }
extern "C" mlx_array_t* mlx_model_forward_with_hidden_states(
    mlx_model_t*, mlx_array_t*, mlx_array_t**, int*) {
    return nullptr;
}
extern "C" void mlx_hidden_states_free(mlx_array_t*, int) {}
extern "C" int mlx_model_get_hidden_state_name(mlx_model_t*, int, char*, int) { return 0; }
extern "C" int mlx_model_get_hidden_state_count(mlx_model_t*) { return 0; }

// Array operations
extern "C" mlx_array_t* mlx_array_from_data(const float* data, int size) {
    if (!data || size <= 0) {
        set_error("Invalid data for mlx_array_from_data");
        return nullptr;
    }
    std::vector<float> vec(data, data + size);
    return reinterpret_cast<mlx_array_t*>(make_array(std::move(vec), {size}, kDtypeFloat32));
}

extern "C" mlx_array_t* mlx_array_from_ints(const int* data, int size) {
    if (!data || size <= 0) {
        set_error("Invalid data for mlx_array_from_ints");
        return nullptr;
    }
    std::vector<float> vec;
    vec.reserve(static_cast<size_t>(size));
    for (int i = 0; i < size; ++i) {
        vec.push_back(static_cast<float>(data[i]));
    }
    return reinterpret_cast<mlx_array_t*>(make_array(std::move(vec), {size}, kDtypeInt32));
}

extern "C" mlx_array_t* mlx_array_from_uints(const uint32_t* data, int size) {
    if (!data || size <= 0) {
        set_error("Invalid data for mlx_array_from_uints");
        return nullptr;
    }
    std::vector<float> vec;
    vec.reserve(static_cast<size_t>(size));
    for (int i = 0; i < size; ++i) {
        vec.push_back(static_cast<float>(data[i]));
    }
    return reinterpret_cast<mlx_array_t*>(make_array(std::move(vec), {size}, kDtypeUInt32));
}

extern "C" float* mlx_array_data(mlx_array_t* array) {
    if (!array) {
        set_error("Null array in mlx_array_data");
        return nullptr;
    }
    auto* arr = as_array(array);
    if (arr->data.empty()) {
        return nullptr;
    }
    return arr->data.data();
}

extern "C" int mlx_array_size(mlx_array_t* array) {
    if (!array) {
        set_error("Null array in mlx_array_size");
        return 0;
    }
    auto* arr = as_array(array);
    return static_cast<int>(arr->data.size());
}

extern "C" int mlx_array_shape(mlx_array_t* array, int* out_shape, int max_dims) {
    if (!array || !out_shape || max_dims <= 0) {
        return 0;
    }
    auto* arr = as_array(array);
    int ndim = static_cast<int>(arr->shape.size());
    int count = std::min(ndim, max_dims);
    for (int i = 0; i < count; ++i) {
        out_shape[i] = arr->shape[i];
    }
    return count;
}

extern "C" int mlx_array_ndim(mlx_array_t* array) {
    if (!array) {
        set_error("Null array in mlx_array_ndim");
        return 0;
    }
    return static_cast<int>(as_array(array)->shape.size());
}

extern "C" int mlx_array_dtype(mlx_array_t* array) {
    if (!array) {
        set_error("Null array in mlx_array_dtype");
        return -1;
    }
    return as_array(array)->dtype;
}

extern "C" mlx_array_t* mlx_array_copy(mlx_array_t* array) {
    if (!array) {
        set_error("Null array in mlx_array_copy");
        return nullptr;
    }
    auto* arr = as_array(array);
    return reinterpret_cast<mlx_array_t*>(
        make_array(arr->data, arr->shape, arr->dtype));
}

extern "C" mlx_array_t* mlx_array_reshape(mlx_array_t* array, const int* shape, int ndim) {
    if (!array || !shape || ndim <= 0) {
        set_error("Invalid reshape parameters");
        return nullptr;
    }
    auto* arr = as_array(array);
    std::vector<int> new_shape = shape_from_ptr(shape, ndim);
    size_t known_product = 1;
    int unknown_index = -1;
    for (int i = 0; i < ndim; ++i) {
        int dim = new_shape[i];
        if (dim == -1) {
            if (unknown_index != -1) {
                set_error("Reshape has multiple unknown dimensions");
                return nullptr;
            }
            unknown_index = i;
            continue;
        }
        if (dim <= 0) {
            set_error("Reshape dimension must be positive");
            return nullptr;
        }
        known_product *= static_cast<size_t>(dim);
    }

    size_t total = arr->data.size();
    if (unknown_index != -1) {
        if (known_product == 0 || (total % known_product) != 0) {
            set_error("Reshape size mismatch");
            return nullptr;
        }
        int inferred = static_cast<int>(total / known_product);
        if (inferred <= 0) {
            set_error("Reshape inferred dimension invalid");
            return nullptr;
        }
        new_shape[unknown_index] = inferred;
    }

    size_t expected = shape_size(new_shape);
    if (expected == 0 || expected != total) {
        set_error("Reshape size mismatch");
        return nullptr;
    }
    return reinterpret_cast<mlx_array_t*>(
        make_array(arr->data, std::move(new_shape), arr->dtype));
}

extern "C" mlx_array_t* mlx_array_transpose(mlx_array_t* array) {
    if (!array) {
        set_error("Null array in mlx_array_transpose");
        return nullptr;
    }
    auto* arr = as_array(array);
    if (arr->shape.size() != 2) {
        std::vector<int> shape = arr->shape;
        std::reverse(shape.begin(), shape.end());
        return reinterpret_cast<mlx_array_t*>(
            make_array(arr->data, std::move(shape), arr->dtype));
    }

    int rows = arr->shape[0];
    int cols = arr->shape[1];
    std::vector<float> out(static_cast<size_t>(rows * cols));
    for (int r = 0; r < rows; ++r) {
        for (int c = 0; c < cols; ++c) {
            out[static_cast<size_t>(c * rows + r)] =
                arr->data[static_cast<size_t>(r * cols + c)];
        }
    }
    return reinterpret_cast<mlx_array_t*>(
        make_array(std::move(out), {cols, rows}, arr->dtype));
}

extern "C" void mlx_array_free(mlx_array_t* array) {
    delete as_array(array);
}

// Arithmetic operations
extern "C" mlx_array_t* mlx_add(mlx_array_t* a, mlx_array_t* b) {
    if (!a || !b) {
        set_error("Null array in mlx_add");
        return nullptr;
    }
    auto* arr_a = as_array(a);
    auto* arr_b = as_array(b);
    if (arr_a->data.size() != arr_b->data.size()) {
        set_error("Shape mismatch in mlx_add");
        return nullptr;
    }
    std::vector<float> out(arr_a->data.size());
    for (size_t i = 0; i < out.size(); ++i) {
        out[i] = arr_a->data[i] + arr_b->data[i];
    }
    return reinterpret_cast<mlx_array_t*>(
        make_array(std::move(out), arr_a->shape, arr_a->dtype));
}

extern "C" mlx_array_t* mlx_multiply(mlx_array_t* a, mlx_array_t* b) {
    if (!a || !b) {
        set_error("Null array in mlx_multiply");
        return nullptr;
    }
    auto* arr_a = as_array(a);
    auto* arr_b = as_array(b);
    if (arr_a->data.size() != arr_b->data.size()) {
        set_error("Shape mismatch in mlx_multiply");
        return nullptr;
    }
    std::vector<float> out(arr_a->data.size());
    for (size_t i = 0; i < out.size(); ++i) {
        out[i] = arr_a->data[i] * arr_b->data[i];
    }
    return reinterpret_cast<mlx_array_t*>(
        make_array(std::move(out), arr_a->shape, arr_a->dtype));
}

extern "C" mlx_array_t* mlx_divide(mlx_array_t* a, mlx_array_t* b) {
    if (!a || !b) {
        set_error("Null array in mlx_divide");
        return nullptr;
    }
    auto* arr_a = as_array(a);
    auto* arr_b = as_array(b);
    if (arr_a->data.size() != arr_b->data.size()) {
        set_error("Shape mismatch in mlx_divide");
        return nullptr;
    }
    std::vector<float> out(arr_a->data.size());
    for (size_t i = 0; i < out.size(); ++i) {
        out[i] = arr_b->data[i] == 0.0f ? std::numeric_limits<float>::infinity()
                                       : arr_a->data[i] / arr_b->data[i];
    }
    return reinterpret_cast<mlx_array_t*>(
        make_array(std::move(out), arr_a->shape, arr_a->dtype));
}

extern "C" mlx_array_t* mlx_matmul(mlx_array_t* a, mlx_array_t* b) {
    if (!a || !b) {
        set_error("Null array in mlx_matmul");
        return nullptr;
    }
    auto* arr_a = as_array(a);
    auto* arr_b = as_array(b);
    if (arr_a->shape.size() != 2 || arr_b->shape.size() != 2) {
        set_error("mlx_matmul expects 2D arrays");
        return nullptr;
    }
    int m = arr_a->shape[0];
    int k = arr_a->shape[1];
    int k2 = arr_b->shape[0];
    int n = arr_b->shape[1];
    if (k != k2) {
        set_error("mlx_matmul shape mismatch");
        return nullptr;
    }
    std::vector<float> out(static_cast<size_t>(m * n), 0.0f);
    for (int i = 0; i < m; ++i) {
        for (int j = 0; j < n; ++j) {
            float acc = 0.0f;
            for (int t = 0; t < k; ++t) {
                acc += arr_a->data[static_cast<size_t>(i * k + t)] *
                       arr_b->data[static_cast<size_t>(t * n + j)];
            }
            out[static_cast<size_t>(i * n + j)] = acc;
        }
    }
    return reinterpret_cast<mlx_array_t*>(
        make_array(std::move(out), {m, n}, arr_a->dtype));
}

// Reduction operations
extern "C" mlx_array_t* mlx_sum(mlx_array_t* array, int axis) {
    if (!array) {
        set_error("Null array in mlx_sum");
        return nullptr;
    }
    auto* arr = as_array(array);
    int ndim = static_cast<int>(arr->shape.size());
    if (ndim == 1) {
        float total = 0.0f;
        for (float v : arr->data) {
            total += v;
        }
        return reinterpret_cast<mlx_array_t*>(
            make_array({total}, {1}, arr->dtype));
    }
    if (ndim != 2) {
        set_error("mlx_sum supports only 1D or 2D arrays");
        return nullptr;
    }
    int rows = arr->shape[0];
    int cols = arr->shape[1];
    if (axis == -1) {
        axis = 1;
    }
    if (axis == 0) {
        std::vector<float> out(static_cast<size_t>(cols), 0.0f);
        for (int r = 0; r < rows; ++r) {
            for (int c = 0; c < cols; ++c) {
                out[static_cast<size_t>(c)] +=
                    arr->data[static_cast<size_t>(r * cols + c)];
            }
        }
        return reinterpret_cast<mlx_array_t*>(
            make_array(std::move(out), {cols}, arr->dtype));
    }
    if (axis == 1) {
        std::vector<float> out(static_cast<size_t>(rows), 0.0f);
        for (int r = 0; r < rows; ++r) {
            float acc = 0.0f;
            for (int c = 0; c < cols; ++c) {
                acc += arr->data[static_cast<size_t>(r * cols + c)];
            }
            out[static_cast<size_t>(r)] = acc;
        }
        return reinterpret_cast<mlx_array_t*>(
            make_array(std::move(out), {rows}, arr->dtype));
    }
    set_error("mlx_sum unsupported axis");
    return nullptr;
}

extern "C" mlx_array_t* mlx_mean(mlx_array_t* array, int axis) {
    if (!array) {
        set_error("Null array in mlx_mean");
        return nullptr;
    }
    auto* arr = as_array(array);
    mlx_array_t* sum = mlx_sum(array, axis);
    if (!sum) {
        return nullptr;
    }
    auto* sum_arr = as_array(sum);
    float divisor = 1.0f;
    if (arr->shape.size() == 1) {
        divisor = static_cast<float>(arr->data.size());
    } else if (arr->shape.size() == 2) {
        int axis_norm = axis == -1 ? 1 : axis;
        divisor = static_cast<float>(axis_norm == 0 ? arr->shape[0] : arr->shape[1]);
    }
    if (divisor <= 0.0f) {
        delete sum_arr;
        set_error("Invalid divisor in mlx_mean");
        return nullptr;
    }
    for (float& v : sum_arr->data) {
        v /= divisor;
    }
    return sum;
}

extern "C" mlx_array_t* mlx_sqrt(mlx_array_t* array) {
    if (!array) {
        set_error("Null array in mlx_sqrt");
        return nullptr;
    }
    auto* arr = as_array(array);
    std::vector<float> out(arr->data.size());
    for (size_t i = 0; i < out.size(); ++i) {
        out[i] = std::sqrt(arr->data[i]);
    }
    return reinterpret_cast<mlx_array_t*>(
        make_array(std::move(out), arr->shape, arr->dtype));
}

// Indexing operations
extern "C" mlx_array_t* mlx_take(mlx_array_t* array, mlx_array_t* indices, int axis) {
    if (!array || !indices) {
        set_error("Null array in mlx_take");
        return nullptr;
    }
    if (axis != 0) {
        set_error("mlx_take stub supports axis=0 only");
        return nullptr;
    }
    auto* arr = as_array(array);
    auto* idx_arr = as_array(indices);
    if (arr->shape.empty()) {
        set_error("mlx_take on empty shape");
        return nullptr;
    }
    int rows = arr->shape[0];
    int cols = arr->shape.size() > 1 ? arr->shape[1] : 1;
    std::vector<float> out;
    if (arr->shape.size() == 1) {
        out.reserve(idx_arr->data.size());
        for (float idx_f : idx_arr->data) {
            int idx = static_cast<int>(idx_f);
            if (idx < 0 || idx >= rows) {
                set_error("mlx_take index out of range");
                return nullptr;
            }
            out.push_back(arr->data[static_cast<size_t>(idx)]);
        }
        return reinterpret_cast<mlx_array_t*>(
            make_array(std::move(out), {static_cast<int>(idx_arr->data.size())}, arr->dtype));
    }
    out.resize(idx_arr->data.size() * static_cast<size_t>(cols));
    for (size_t i = 0; i < idx_arr->data.size(); ++i) {
        int idx = static_cast<int>(idx_arr->data[i]);
        if (idx < 0 || idx >= rows) {
            set_error("mlx_take index out of range");
            return nullptr;
        }
        for (int c = 0; c < cols; ++c) {
            out[i * static_cast<size_t>(cols) + static_cast<size_t>(c)] =
                arr->data[static_cast<size_t>(idx * cols + c)];
        }
    }
    return reinterpret_cast<mlx_array_t*>(
        make_array(std::move(out),
                   {static_cast<int>(idx_arr->data.size()), cols},
                   arr->dtype));
}

// Eval/sync - no-ops
extern "C" void mlx_eval(mlx_array_t*) {}
extern "C" void mlx_eval_all(mlx_array_t**, int) {}
extern "C" void mlx_synchronize(void) {}

// Memory - report zero usage
extern "C" void mlx_gc_collect(void) {}
extern "C" size_t mlx_memory_usage(void) { return 0; }
extern "C" size_t mlx_allocation_count(void) { return 0; }
extern "C" void mlx_memory_stats(size_t* bytes, size_t* count) {
    if (bytes) *bytes = 0;
    if (count) *count = 0;
}
extern "C" void mlx_memory_reset(void) {}

// Sampling
extern "C" int mlx_sample_token(mlx_array_t* logits, const mlx_sampler_config_t*) {
    if (!logits) {
        set_error("Null logits in mlx_sample_token");
        return -1;
    }
    auto* arr = as_array(logits);
    if (arr->data.empty()) {
        set_error("Empty logits in mlx_sample_token");
        return -1;
    }
    auto it = std::max_element(arr->data.begin(), arr->data.end());
    return static_cast<int>(std::distance(arr->data.begin(), it));
}

extern "C" int mlx_sample_token_with_metadata(
    mlx_array_t* logits,
    const mlx_sampler_config_t*,
    mlx_token_metadata_t* out_metadata) {
    int token_id = mlx_sample_token(logits, nullptr);
    if (token_id < 0) {
        return -1;
    }
    if (out_metadata) {
        out_metadata->confidence = 1.0f;
        out_metadata->alternatives = nullptr;
        out_metadata->num_alternatives = 0;
    }
    return token_id;
}

extern "C" void mlx_free_token_metadata(mlx_token_metadata_t* metadata) {
    if (!metadata) {
        return;
    }
    if (metadata->alternatives) {
        delete[] metadata->alternatives;
    }
    metadata->alternatives = nullptr;
    metadata->num_alternatives = 0;
    metadata->confidence = 0.0f;
}

// RNG seeding
extern "C" void mlx_set_seed(const uint8_t*, size_t) {}

// Quantization operations - unsupported in stub
extern "C" mlx_array_t* mlx_quantize(mlx_array_t*, int, int) { return nullptr; }
extern "C" mlx_array_t* mlx_dequantize(mlx_array_t*, mlx_array_t*, mlx_array_t*, int, int) {
    return nullptr;
}

// RoPE and attention operations - unsupported in stub
extern "C" mlx_array_t* mlx_rope(
    mlx_array_t*, int, bool, float, float, int) {
    return nullptr;
}
extern "C" mlx_array_t* mlx_scaled_dot_product_attention(
    mlx_array_t*, mlx_array_t*, mlx_array_t*, float, mlx_array_t*) {
    return nullptr;
}
extern "C" mlx_array_t* mlx_create_causal_mask(int) { return nullptr; }

// KV cache - unsupported in stub
extern "C" mlx_kv_cache_t* mlx_kv_cache_new(int, int, int, int) { return nullptr; }
extern "C" int mlx_kv_cache_update(
    mlx_kv_cache_t*, int, mlx_array_t*, mlx_array_t*) {
    return -1;
}
extern "C" mlx_array_t* mlx_kv_cache_get_keys(mlx_kv_cache_t*, int) { return nullptr; }
extern "C" mlx_array_t* mlx_kv_cache_get_values(mlx_kv_cache_t*, int) { return nullptr; }
extern "C" int mlx_kv_cache_seq_len(mlx_kv_cache_t*) { return 0; }
extern "C" void mlx_kv_cache_reset(mlx_kv_cache_t*) {}
extern "C" void mlx_kv_cache_free(mlx_kv_cache_t*) {}

// SafeTensors loading - unsupported in stub
extern "C" mlx_weights_t* mlx_load_safetensors(const char*) { return nullptr; }
extern "C" mlx_array_t* mlx_weights_get(mlx_weights_t*, const char*) { return nullptr; }
extern "C" int mlx_weights_list(mlx_weights_t*, const char**, int) { return 0; }
extern "C" void mlx_weights_free(mlx_weights_t*) {}

// LoRA adapter caching - unsupported in stub
extern "C" const char* mlx_lora_cache_adapter(
    const char*, mlx_array_t*, mlx_array_t*) {
    return nullptr;
}
extern "C" bool mlx_lora_get_cached(
    const char*, mlx_array_t**, mlx_array_t**) {
    return false;
}
extern "C" void mlx_lora_evict_cached(const char*) {}
extern "C" void mlx_lora_clear_cache() {}
extern "C" size_t mlx_lora_cache_size(void) { return 0; }
extern "C" void mlx_lora_set_cache_limit(size_t) {}
