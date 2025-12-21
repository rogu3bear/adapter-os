// coreml_ffi.h - C header for CoreML FFI
// Copyright 2025 JKCA / James KC Auchterlonie. All rights reserved.

#ifndef COREML_FFI_H
#define COREML_FFI_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Check if CoreML framework is available
bool coreml_is_available(void);

// Check Neural Engine availability
// Returns (available, generation)
typedef struct {
    bool available;
    uint8_t generation;
} AneCheckResult;

AneCheckResult coreml_check_ane(void);

// Load a CoreML model
// compute_units: 0=CPU, 1=CPU+GPU, 2=CPU+ANE, 3=All
void* coreml_load_model(const char* path, size_t path_len, int32_t compute_units);

// Unload a CoreML model
void coreml_unload_model(void* handle);

// Run inference on loaded model
int32_t coreml_run_inference(
    void* handle,
    const uint32_t* input_ids,
    size_t input_len,
    float* output_logits,
    size_t output_len,
    const uint16_t* adapter_indices,
    const int16_t* adapter_gates,
    size_t num_adapters
);

// Run inference with LoRA adapter support
int32_t coreml_run_inference_with_lora(
    void* handle,
    const uint32_t* input_ids,
    size_t input_len,
    float* output_logits,
    size_t output_len,
    const uint16_t* adapter_indices,
    const int16_t* adapter_gates,
    size_t num_adapters,
    const float* const* lora_deltas,
    const size_t* delta_lens
);

// Perform health check on model
int32_t coreml_health_check(void* handle);

// Get last error message
size_t coreml_get_last_error(char* buffer, size_t buffer_len);

#ifdef __cplusplus
}
#endif

#endif // COREML_FFI_H
