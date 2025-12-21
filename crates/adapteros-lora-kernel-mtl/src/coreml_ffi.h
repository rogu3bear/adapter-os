//! CoreML FFI C Interface
//!
//! Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.
//!
//! C-compatible FFI interface for CoreML integration.
//! This header defines the C ABI boundary for safe Rust ↔ Objective-C++ interaction.

#ifndef ADAPTEROS_COREML_FFI_H
#define ADAPTEROS_COREML_FFI_H

#include <stddef.h>
#include <stdint.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

// Opaque types for memory safety
typedef struct CoreMLModel CoreMLModel;
typedef struct CoreMLArray CoreMLArray;
typedef struct CoreMLPrediction CoreMLPrediction;

/// Error codes for CoreML operations
typedef enum {
    COREML_SUCCESS = 0,
    COREML_ERROR_INVALID_MODEL = 1,
    COREML_ERROR_INVALID_INPUT = 2,
    COREML_ERROR_PREDICTION_FAILED = 3,
    COREML_ERROR_MEMORY_ALLOCATION = 4,
    COREML_ERROR_INVALID_DIMENSIONS = 5,
    COREML_ERROR_UNSUPPORTED_TYPE = 6,
    COREML_ERROR_IO = 7,
    COREML_ERROR_UNKNOWN = 99,
} CoreMLErrorCode;

/// Array shape descriptor for multi-dimensional arrays
typedef struct {
    size_t* dimensions;
    size_t rank;
} CoreMLShape;

/// Model metadata
typedef struct {
    const char* model_version;
    const char* model_description;
    size_t input_count;
    size_t output_count;
    bool supports_gpu;
    bool supports_ane;
} CoreMLModelMetadata;

// ============================================================================
// Model Management
// ============================================================================

/// Load a CoreML model from .mlpackage or .mlmodelc path
///
/// # Arguments
/// * `path` - Filesystem path to the model (UTF-8 encoded C string)
/// * `use_gpu` - Enable GPU acceleration (Metal)
/// * `use_ane` - Enable Apple Neural Engine acceleration
/// * `error_code` - Output parameter for error code
///
/// # Returns
/// * Opaque pointer to CoreMLModel, or NULL on failure
CoreMLModel* coreml_model_load(
    const char* path,
    bool use_gpu,
    bool use_ane,
    CoreMLErrorCode* error_code
);

/// Free a CoreML model and release resources
void coreml_model_free(CoreMLModel* model);

/// Get model metadata
///
/// # Returns
/// * Metadata struct (caller must free strings with coreml_free_string)
CoreMLModelMetadata coreml_model_get_metadata(CoreMLModel* model);

// ============================================================================
// Array Management
// ============================================================================

/// Create a new CoreML array from raw float data
///
/// # Arguments
/// * `data` - Float array data (will be copied)
/// * `shape` - Array shape descriptor
/// * `error_code` - Output parameter for error code
///
/// # Returns
/// * Opaque pointer to CoreMLArray, or NULL on failure
CoreMLArray* coreml_array_new(
    const float* data,
    const CoreMLShape* shape,
    CoreMLErrorCode* error_code
);

/// Create a new CoreML array from raw int8 data (quantized)
CoreMLArray* coreml_array_new_int8(
    const int8_t* data,
    const CoreMLShape* shape,
    CoreMLErrorCode* error_code
);

/// Create a new CoreML array from raw float16 data
CoreMLArray* coreml_array_new_float16(
    const uint16_t* data,
    const CoreMLShape* shape,
    CoreMLErrorCode* error_code
);

/// Free a CoreML array
void coreml_array_free(CoreMLArray* array);

/// Get raw float data from array (returns NULL if not float32)
/// Data pointer is valid until array is freed
const float* coreml_array_get_float_data(CoreMLArray* array);

/// Get raw int8 data from array (returns NULL if not int8)
const int8_t* coreml_array_get_int8_data(CoreMLArray* array);

/// Get array shape
CoreMLShape coreml_array_get_shape(CoreMLArray* array);

/// Get total element count
size_t coreml_array_get_size(CoreMLArray* array);

// ============================================================================
// Inference
// ============================================================================

/// Run prediction with single input
///
/// # Arguments
/// * `model` - Loaded CoreML model
/// * `input` - Input array
/// * `input_name` - Input feature name (or NULL for default)
/// * `error_code` - Output parameter for error code
///
/// # Returns
/// * Prediction object containing output arrays, or NULL on failure
CoreMLPrediction* coreml_predict(
    CoreMLModel* model,
    CoreMLArray* input,
    const char* input_name,
    CoreMLErrorCode* error_code
);

/// Run prediction with multiple named inputs
CoreMLPrediction* coreml_predict_multi(
    CoreMLModel* model,
    CoreMLArray** inputs,
    const char** input_names,
    size_t input_count,
    CoreMLErrorCode* error_code
);

/// Free a prediction result
void coreml_prediction_free(CoreMLPrediction* prediction);

/// Get output array by name
CoreMLArray* coreml_prediction_get_output(
    CoreMLPrediction* prediction,
    const char* output_name
);

/// Get number of outputs in prediction
size_t coreml_prediction_get_output_count(CoreMLPrediction* prediction);

/// Get output name by index
const char* coreml_prediction_get_output_name(
    CoreMLPrediction* prediction,
    size_t index
);

// ============================================================================
// Error Handling
// ============================================================================

/// Get last error message (thread-local)
/// Returns NULL if no error
const char* coreml_get_last_error(void);

/// Clear last error message
void coreml_clear_error(void);

// ============================================================================
// Memory Management
// ============================================================================

/// Free a string returned by CoreML FFI
void coreml_free_string(const char* str);

/// Free a shape descriptor
void coreml_free_shape(CoreMLShape shape);

// ============================================================================
// Utilities
// ============================================================================

/// Check if CoreML is available on this system
bool coreml_is_available(void);

/// Get CoreML framework version string
const char* coreml_get_version(void);

/// Enable verbose logging for debugging
void coreml_set_verbose(bool enabled);

#ifdef __cplusplus
}
#endif

#endif // ADAPTEROS_COREML_FFI_H
