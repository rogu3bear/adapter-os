// MLX C++ wrapper stub implementation
// Used when MLX is not available (non-Apple-Silicon builds)
// All functions return errors/null to indicate MLX is unavailable

#include "wrapper.h"
#include <cstring>
#include <string>

static thread_local std::string g_last_error = "MLX not available (stub implementation)";

// Error handling
extern "C" const char* mlx_get_last_error(void) { return g_last_error.c_str(); }
extern "C" void mlx_clear_error(void) { g_last_error = "MLX not available (stub implementation)"; }

// Runtime - always report as not available
extern "C" int mlx_init(mlx_device_type_t) { g_last_error = "MLX not available"; return -1; }
extern "C" int mlx_init_default(void) { return mlx_init(MLX_DEVICE_AUTO); }
extern "C" void mlx_shutdown(void) {}
extern "C" bool mlx_is_initialized(void) { return false; }
extern "C" mlx_device_type_t mlx_get_device_type(void) { return MLX_DEVICE_CPU; }
extern "C" int mlx_set_device(mlx_device_type_t) { return -1; }
extern "C" int mlx_backend_info(mlx_backend_capabilities_t* cap) {
    if (cap) std::memset(cap, 0, sizeof(*cap));
    return -1;
}
extern "C" const char* mlx_get_version(void) { return "stub-0.0.0"; }

// Model operations - all fail
extern "C" mlx_model_t* mlx_model_load(const char*) { return nullptr; }
extern "C" mlx_model_t* mlx_model_load_from_buffer(const uint8_t*, size_t, const char*) { return nullptr; }
extern "C" void mlx_model_free(mlx_model_t*) {}
extern "C" mlx_array_t* mlx_model_forward(mlx_model_t*, mlx_array_t*) { return nullptr; }
extern "C" mlx_array_t* mlx_model_forward_with_hidden_states(mlx_model_t*, mlx_array_t*, mlx_array_t**, int*) { return nullptr; }
extern "C" void mlx_hidden_states_free(mlx_array_t*, int) {}
extern "C" int mlx_model_get_hidden_state_name(mlx_model_t*, int, char*, int) { return 0; }
extern "C" int mlx_model_get_hidden_state_count(mlx_model_t*) { return 0; }

// Array operations - all fail
extern "C" mlx_array_t* mlx_array_from_ints(const int*, int) { return nullptr; }
extern "C" float* mlx_array_data(mlx_array_t*) { return nullptr; }
extern "C" int mlx_array_size(mlx_array_t*) { return 0; }
extern "C" void mlx_array_free(mlx_array_t*) {}

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

// Sampling - fail
extern "C" int mlx_sample_token(mlx_array_t*, const mlx_sampler_config_t*) { return -1; }
extern "C" void mlx_set_seed(const uint8_t*, size_t) {}
