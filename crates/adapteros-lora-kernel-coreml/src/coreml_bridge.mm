// coreml_bridge.mm - Objective-C++ FFI bridge for CoreML
// Copyright 2025 JKCA / James KC Auchterlonie. All rights reserved.

#import <CoreML/CoreML.h>
#import <Foundation/Foundation.h>
#import <Metal/Metal.h>
#import <Accelerate/Accelerate.h>
#include "coreml_ffi.h"
#include <cstring>
#include <AvailabilityMacros.h>
#include <stdatomic.h>

// Check if we have macOS 15+ SDK for MLTensor support
#ifndef MAC_OS_X_VERSION_15_0
#define MAC_OS_X_VERSION_15_0 150000
#endif

// MLTensor and MLState are Swift-only types in CoreML (macOS 15+)
// They are not available in Objective-C/C++, so we disable this code path
// To use MLTensor operations, call through Swift bridging code
#define HAS_MLTENSOR 0

static thread_local char g_last_error[1024] = {0};

extern "C" {

bool coreml_is_available() {
    if (@available(macOS 10.13, *)) {
        return true;
    }
    return false;
}

typedef struct {
    bool available;
    uint8_t generation;          // 4=M1, 5=M2, 6=M3, 7=M4
    uint32_t gpu_core_count;     // 40 for M4 Max
    uint32_t ane_tops;           // 38 for M4
    bool supports_mlstate;       // macOS 15+
    bool supports_mltensor;      // macOS 15+
    uint8_t gpu_family;          // Apple9 for M4
} DetailedAneInfo;

AneCheckResult coreml_check_ane() {
    AneCheckResult result = {false, 0};

    if (@available(macOS 10.16, *)) {
        id<MTLDevice> device = MTLCreateSystemDefaultDevice();
        if (device) {
            NSString *name = device.name;
            if ([name containsString:@"Apple"]) {
                result.available = true;

                if ([name containsString:@"M4"]) {
                    result.generation = 7;
                } else if ([name containsString:@"M3"]) {
                    result.generation = 6;
                } else if ([name containsString:@"M2"]) {
                    result.generation = 5;
                } else if ([name containsString:@"M1"]) {
                    result.generation = 4;
                } else {
                    result.generation = 1;
                }
            }
        }
    }

    return result;
}

DetailedAneInfo coreml_get_detailed_info() {
    DetailedAneInfo info = {false, 0, 0, 0, false, false, 0};

    if (@available(macOS 10.16, *)) {
        id<MTLDevice> device = MTLCreateSystemDefaultDevice();
        if (device) {
            NSString *name = device.name;
            if ([name containsString:@"Apple"]) {
                info.available = true;

                // Detect chip generation and capabilities
                if ([name containsString:@"M4"]) {
                    info.generation = 7;
                    info.ane_tops = 38;
                    info.gpu_family = 9;  // Apple9

                    // Detect M4 variant by GPU core count
                    // M4: 10 cores, M4 Pro: 20 cores, M4 Max: 40 cores
                    if ([name containsString:@"Max"]) {
                        info.gpu_core_count = 40;
                    } else if ([name containsString:@"Pro"]) {
                        info.gpu_core_count = 20;
                    } else {
                        info.gpu_core_count = 10;
                    }
                } else if ([name containsString:@"M3"]) {
                    info.generation = 6;
                    info.ane_tops = 18;
                    info.gpu_family = 8;

                    if ([name containsString:@"Max"]) {
                        info.gpu_core_count = 40;
                    } else if ([name containsString:@"Pro"]) {
                        info.gpu_core_count = 18;
                    } else {
                        info.gpu_core_count = 10;
                    }
                } else if ([name containsString:@"M2"]) {
                    info.generation = 5;
                    info.ane_tops = 15;
                    info.gpu_family = 7;

                    if ([name containsString:@"Ultra"]) {
                        info.gpu_core_count = 76;
                    } else if ([name containsString:@"Max"]) {
                        info.gpu_core_count = 38;
                    } else if ([name containsString:@"Pro"]) {
                        info.gpu_core_count = 19;
                    } else {
                        info.gpu_core_count = 10;
                    }
                } else if ([name containsString:@"M1"]) {
                    info.generation = 4;
                    info.ane_tops = 11;
                    info.gpu_family = 6;

                    if ([name containsString:@"Ultra"]) {
                        info.gpu_core_count = 64;
                    } else if ([name containsString:@"Max"]) {
                        info.gpu_core_count = 32;
                    } else if ([name containsString:@"Pro"]) {
                        info.gpu_core_count = 16;
                    } else {
                        info.gpu_core_count = 8;
                    }
                } else {
                    // Pre-M1 Apple Silicon (A-series in Mac)
                    info.generation = 1;
                    info.ane_tops = 5;
                    info.gpu_family = 5;
                    info.gpu_core_count = 4;
                }
            }
        }
    }

    // Check for modern API availability (macOS 15+)
    if (@available(macOS 15.0, *)) {
        info.supports_mlstate = true;
        info.supports_mltensor = true;
    }

    return info;
}

void* coreml_load_model(const char* path, size_t path_len, int32_t compute_units) {
    @autoreleasepool {
        NSString *modelPath = [[NSString alloc] initWithBytes:path
                                                       length:path_len
                                                     encoding:NSUTF8StringEncoding];
        NSURL *modelURL = [NSURL fileURLWithPath:modelPath];

        MLModelConfiguration *config = [[MLModelConfiguration alloc] init];
        switch (compute_units) {
            case 0:
                config.computeUnits = MLComputeUnitsCPUOnly;
                break;
            case 1:
                config.computeUnits = MLComputeUnitsCPUAndGPU;
                break;
            case 2:
                if (@available(macOS 12.0, *)) {
                    config.computeUnits = MLComputeUnitsCPUAndNeuralEngine;
                } else {
                    config.computeUnits = MLComputeUnitsCPUOnly;
                }
                break;
            case 3:
            default:
                config.computeUnits = MLComputeUnitsAll;
                break;
        }

        NSError *error = nil;
        MLModel *model = [MLModel modelWithContentsOfURL:modelURL
                                           configuration:config
                                                   error:&error];

        if (error) {
            snprintf(g_last_error, sizeof(g_last_error), "%s",
                    [[error localizedDescription] UTF8String]);
            return nullptr;
        }

        return (__bridge_retained void*)model;
    }
}

void coreml_unload_model(void* handle) {
    @autoreleasepool {
        if (handle) {
            MLModel *model = (__bridge_transfer MLModel*)handle;
            model = nil;
        }
    }
}

// LoRA delta structure for pre-computed adapter contributions
// Each adapter provides a delta array of the same size as output logits
typedef struct {
    const float* deltas;      // Pre-computed LoRA delta values
    size_t delta_len;         // Length of delta array (should match output_len)
} LoraAdapterDelta;

int32_t coreml_run_inference(
    void* handle,
    const uint32_t* input_ids,
    size_t input_len,
    float* output_logits,
    size_t output_len,
    const uint16_t* adapter_indices,
    const int16_t* adapter_gates,
    size_t num_adapters
) {
    @autoreleasepool {
        if (!handle) {
            snprintf(g_last_error, sizeof(g_last_error), "Null model handle");
            return -1;
        }

        MLModel *model = (__bridge MLModel*)handle;
        NSError *error = nil;

        NSArray<NSNumber*> *shape = @[@(1), @(input_len)];
        MLMultiArray *inputArray = [[MLMultiArray alloc] initWithShape:shape
                                                              dataType:MLMultiArrayDataTypeInt32
                                                                 error:&error];
        if (error) {
            snprintf(g_last_error, sizeof(g_last_error), "Failed to create input array: %s",
                    [[error localizedDescription] UTF8String]);
            return -2;
        }

        int32_t *inputPtr = (int32_t*)inputArray.dataPointer;
        for (size_t i = 0; i < input_len; i++) {
            inputPtr[i] = (int32_t)input_ids[i];
        }

        MLDictionaryFeatureProvider *inputProvider =
            [[MLDictionaryFeatureProvider alloc] initWithDictionary:@{@"input_ids": inputArray}
                                                              error:&error];
        if (error) {
            snprintf(g_last_error, sizeof(g_last_error), "Failed to create feature provider: %s",
                    [[error localizedDescription] UTF8String]);
            return -3;
        }

        id<MLFeatureProvider> outputProvider = [model predictionFromFeatures:inputProvider
                                                                       error:&error];
        if (error) {
            snprintf(g_last_error, sizeof(g_last_error), "Prediction failed: %s",
                    [[error localizedDescription] UTF8String]);
            return -4;
        }

        MLFeatureValue *outputValue = [outputProvider featureValueForName:@"logits"];
        if (!outputValue || outputValue.type != MLFeatureTypeMultiArray) {
            snprintf(g_last_error, sizeof(g_last_error), "Output logits not found");
            return -5;
        }

        MLMultiArray *outputArray = outputValue.multiArrayValue;
        float *outputPtr = (float*)outputArray.dataPointer;
        size_t copyLen = output_len < (size_t)outputArray.count ? output_len : (size_t)outputArray.count;
        memcpy(output_logits, outputPtr, copyLen * sizeof(float));

        // Note: LoRA adapter application requires pre-computed deltas
        // Use coreml_run_inference_with_lora for full adapter support
        // This function preserves backward compatibility

        return 0;
    }
}

// Generic inference function with configurable output name
// Used for hybrid models that output hidden_states instead of logits
int32_t coreml_run_inference_named_output(
    void* handle,
    const uint32_t* input_ids,
    size_t input_len,
    float* output_buffer,
    size_t output_len,
    const char* output_name,
    size_t output_name_len
) {
    @autoreleasepool {
        if (!handle) {
            snprintf(g_last_error, sizeof(g_last_error), "Null model handle");
            return -1;
        }

        MLModel *model = (__bridge MLModel*)handle;
        NSError *error = nil;

        // Create input array
        NSArray<NSNumber*> *shape = @[@(1), @(input_len)];
        MLMultiArray *inputArray = [[MLMultiArray alloc] initWithShape:shape
                                                              dataType:MLMultiArrayDataTypeInt32
                                                                 error:&error];
        if (error) {
            snprintf(g_last_error, sizeof(g_last_error), "Failed to create input array: %s",
                    [[error localizedDescription] UTF8String]);
            return -2;
        }

        int32_t *inputPtr = (int32_t*)inputArray.dataPointer;
        for (size_t i = 0; i < input_len; i++) {
            inputPtr[i] = (int32_t)input_ids[i];
        }

        // Create feature provider
        MLDictionaryFeatureProvider *inputProvider =
            [[MLDictionaryFeatureProvider alloc] initWithDictionary:@{@"input_ids": inputArray}
                                                              error:&error];
        if (error) {
            snprintf(g_last_error, sizeof(g_last_error), "Failed to create feature provider: %s",
                    [[error localizedDescription] UTF8String]);
            return -3;
        }

        // Run prediction
        id<MLFeatureProvider> outputProvider = [model predictionFromFeatures:inputProvider
                                                                       error:&error];
        if (error) {
            snprintf(g_last_error, sizeof(g_last_error), "Prediction failed: %s",
                    [[error localizedDescription] UTF8String]);
            return -4;
        }

        // Get output with configurable name
        NSString *outputNameStr = [[NSString alloc] initWithBytes:output_name
                                                           length:output_name_len
                                                         encoding:NSUTF8StringEncoding];
        MLFeatureValue *outputValue = [outputProvider featureValueForName:outputNameStr];
        if (!outputValue || outputValue.type != MLFeatureTypeMultiArray) {
            // Try fallback names
            if (!outputValue) {
                outputValue = [outputProvider featureValueForName:@"final_ln_output"];
            }
            if (!outputValue) {
                outputValue = [outputProvider featureValueForName:@"hidden_states"];
            }
            if (!outputValue) {
                outputValue = [outputProvider featureValueForName:@"logits"];
            }
            if (!outputValue || outputValue.type != MLFeatureTypeMultiArray) {
                snprintf(g_last_error, sizeof(g_last_error), "Output '%s' not found", output_name);
                return -5;
            }
        }

        // Copy output data - handle both FP16 and FP32
        MLMultiArray *outputArray = outputValue.multiArrayValue;
        size_t totalElements = (size_t)outputArray.count;
        size_t copyLen = output_len < totalElements ? output_len : totalElements;

        if (outputArray.dataType == MLMultiArrayDataTypeFloat32) {
            float *outputPtr = (float*)outputArray.dataPointer;
            memcpy(output_buffer, outputPtr, copyLen * sizeof(float));
        } else if (outputArray.dataType == MLMultiArrayDataTypeFloat16) {
            // Convert FP16 to FP32
            __fp16 *fp16Ptr = (__fp16*)outputArray.dataPointer;
            for (size_t i = 0; i < copyLen; i++) {
                output_buffer[i] = (float)fp16Ptr[i];
            }
        } else {
            snprintf(g_last_error, sizeof(g_last_error), "Unsupported output data type");
            return -6;
        }

        return (int32_t)copyLen;
    }
}

// Create default Metal device
// Returns retained id<MTLDevice> cast to void*
void* coreml_create_metal_device() {
    @autoreleasepool {
        id<MTLDevice> device = MTLCreateSystemDefaultDevice();
        if (device) {
            return (__bridge_retained void*)device;
        }
        return nullptr;
    }
}

// Release Metal device
void coreml_release_metal_device(void* device) {
    if (device) {
        CFRelease(device);
    }
}

// Export MLMultiArray output to Metal buffer
// Returns a retained MTLBuffer* cast to void*
void* coreml_export_output_to_metal(void* output_handle, void* device_handle) {
    @autoreleasepool {
        if (!output_handle || !device_handle) {
            snprintf(g_last_error, sizeof(g_last_error), "Invalid handles provided");
            return nullptr;
        }

        MLMultiArray* outputArray = (__bridge MLMultiArray*)output_handle;
        id<MTLDevice> device = (__bridge id<MTLDevice>)device_handle;

        size_t length = outputArray.count * sizeof(float);
        if (outputArray.dataType != MLMultiArrayDataTypeFloat32) {
             snprintf(g_last_error, sizeof(g_last_error), "Output must be Float32");
             return nullptr;
        }

        // Create a shared buffer (accessible by CPU and GPU)
        id<MTLBuffer> buffer = [device newBufferWithBytes:outputArray.dataPointer
                                                   length:length
                                                  options:MTLResourceStorageModeShared];

        if (!buffer) {
            snprintf(g_last_error, sizeof(g_last_error), "Failed to allocate Metal buffer");
            return nullptr;
        }

        // Return retained reference
        return (__bridge_retained void*)buffer;
    }
}

// Release Metal buffer
void coreml_release_buffer(void* buffer) {
    if (buffer) {
        CFRelease(buffer);
    }
}

// Copy MLMultiArray output to existing Metal buffer
void coreml_copy_output_to_metal(void* output_handle, void* buffer_handle) {
    @autoreleasepool {
        if (!output_handle || !buffer_handle) return;

        MLMultiArray* outputArray = (__bridge MLMultiArray*)output_handle;
        id<MTLBuffer> buffer = (__bridge id<MTLBuffer>)buffer_handle;

        size_t length = outputArray.count * sizeof(float);
        if (buffer.length < length) return;

        memcpy(buffer.contents, outputArray.dataPointer, length);
    }
}

// Extended inference function with LoRA adapter support
// Applies LoRA deltas using Q15 quantized gates
// Formula: output = base_output + sum(gate_i * lora_delta_i)
int32_t coreml_run_inference_with_lora(
    void* handle,
    const uint32_t* input_ids,
    size_t input_len,
    float* output_logits,
    size_t output_len,
    const uint16_t* adapter_indices,
    const int16_t* adapter_gates,
    size_t num_adapters,
    const float* const* lora_deltas,    // Array of pointers to pre-computed LoRA deltas
    const size_t* delta_lens            // Length of each delta array
) {
    @autoreleasepool {
        if (!handle) {
            snprintf(g_last_error, sizeof(g_last_error), "Null model handle");
            return -1;
        }

        MLModel *model = (__bridge MLModel*)handle;
        NSError *error = nil;

        // Create input array
        NSArray<NSNumber*> *shape = @[@(1), @(input_len)];
        MLMultiArray *inputArray = [[MLMultiArray alloc] initWithShape:shape
                                                              dataType:MLMultiArrayDataTypeInt32
                                                                 error:&error];
        if (error) {
            snprintf(g_last_error, sizeof(g_last_error), "Failed to create input array: %s",
                    [[error localizedDescription] UTF8String]);
            return -2;
        }

        int32_t *inputPtr = (int32_t*)inputArray.dataPointer;
        for (size_t i = 0; i < input_len; i++) {
            inputPtr[i] = (int32_t)input_ids[i];
        }

        // Create feature provider
        MLDictionaryFeatureProvider *inputProvider =
            [[MLDictionaryFeatureProvider alloc] initWithDictionary:@{@"input_ids": inputArray}
                                                              error:&error];
        if (error) {
            snprintf(g_last_error, sizeof(g_last_error), "Failed to create feature provider: %s",
                    [[error localizedDescription] UTF8String]);
            return -3;
        }

        // Run base model inference
        id<MLFeatureProvider> outputProvider = [model predictionFromFeatures:inputProvider
                                                                       error:&error];
        if (error) {
            snprintf(g_last_error, sizeof(g_last_error), "Prediction failed: %s",
                    [[error localizedDescription] UTF8String]);
            return -4;
        }

        MLFeatureValue *outputValue = [outputProvider featureValueForName:@"logits"];
        if (!outputValue || outputValue.type != MLFeatureTypeMultiArray) {
            snprintf(g_last_error, sizeof(g_last_error), "Output logits not found");
            return -5;
        }

        // Copy base model output
        MLMultiArray *outputArray = outputValue.multiArrayValue;
        float *outputPtr = (float*)outputArray.dataPointer;
        size_t copyLen = output_len < (size_t)outputArray.count ? output_len : (size_t)outputArray.count;
        memcpy(output_logits, outputPtr, copyLen * sizeof(float));

        // Apply LoRA adapter contributions
        // Formula: output = base_output + sum(gate_i * lora_delta_i)
        if (num_adapters > 0 && adapter_gates && lora_deltas && delta_lens) {
            // Q15 dequantization constant: divide by 32767.0 to get [-1.0, 1.0] range
            const float q15_scale = 1.0f / 32767.0f;

            for (size_t adapter_idx = 0; adapter_idx < num_adapters; adapter_idx++) {
                // Dequantize Q15 gate value to float
                float gate = (float)adapter_gates[adapter_idx] * q15_scale;

                // Skip adapters with zero or negligible gate values
                if (gate == 0.0f) {
                    continue;
                }

                const float* delta = lora_deltas[adapter_idx];
                size_t delta_len = delta_lens[adapter_idx];

                if (!delta || delta_len == 0) {
                    continue;
                }

                // Apply scaled LoRA delta to output logits
                // Limit to the smaller of output_len and delta_len
                size_t apply_len = copyLen < delta_len ? copyLen : delta_len;

                for (size_t i = 0; i < apply_len; i++) {
                    output_logits[i] += gate * delta[i];
                }
            }
        }

        return 0;
    }
}

int32_t coreml_health_check(void* handle) {
    if (!handle) {
        return -1;
    }

    MLModel *model = (__bridge MLModel*)handle;
    return model.modelDescription ? 0 : -2;
}

size_t coreml_get_last_error(char* buffer, size_t buffer_len) {
    size_t error_len = strlen(g_last_error);
    if (buffer && buffer_len > 0) {
        size_t copy_len = error_len < buffer_len - 1 ? error_len : buffer_len - 1;
        memcpy(buffer, g_last_error, copy_len);
        buffer[copy_len] = '\0';
        return copy_len;
    }
    return error_len;
}

// MLState handle for stateful prediction
#if HAS_MLTENSOR
void* coreml_create_state(void* handle) {
    @autoreleasepool {
        if (!handle) {
            snprintf(g_last_error, sizeof(g_last_error), "Null model handle");
            return nullptr;
        }

        if (@available(macOS 15.0, *)) {
            MLModel *model = (__bridge MLModel*)handle;
            NSError *error = nil;
            MLState *state = [model newStateWithError:&error];
            if (error) {
                snprintf(g_last_error, sizeof(g_last_error), "Failed to create state: %s",
                        [[error localizedDescription] UTF8String]);
                return nullptr;
            }
            return (__bridge_retained void*)state;
        }

        snprintf(g_last_error, sizeof(g_last_error), "MLState requires macOS 15.0+");
        return nullptr;
    }
}
#else
void* coreml_create_state(void* handle) {
    (void)handle;
    snprintf(g_last_error, sizeof(g_last_error), "MLState not available - SDK < macOS 15");
    return nullptr;
}
#endif

#if HAS_MLTENSOR
int32_t coreml_predict_with_state(
    void* handle,
    void* state_handle,
    const uint32_t* input_ids,
    size_t input_len,
    float* output_logits,
    size_t output_len
) {
    @autoreleasepool {
        if (!handle || !state_handle) {
            snprintf(g_last_error, sizeof(g_last_error), "Null handle");
            return -1;
        }

        if (@available(macOS 15.0, *)) {
            MLModel *model = (__bridge MLModel*)handle;
            MLState *state = (__bridge MLState*)state_handle;
            NSError *error = nil;

            // Create input array
            NSArray<NSNumber*> *shape = @[@(1), @(input_len)];
            MLMultiArray *inputArray = [[MLMultiArray alloc] initWithShape:shape
                                                                  dataType:MLMultiArrayDataTypeInt32
                                                                     error:&error];
            if (error) {
                snprintf(g_last_error, sizeof(g_last_error), "Failed to create input: %s",
                        [[error localizedDescription] UTF8String]);
                return -2;
            }

            int32_t *inputPtr = (int32_t*)inputArray.dataPointer;
            for (size_t i = 0; i < input_len; i++) {
                inputPtr[i] = (int32_t)input_ids[i];
            }

            MLDictionaryFeatureProvider *inputProvider =
                [[MLDictionaryFeatureProvider alloc] initWithDictionary:@{@"input_ids": inputArray}
                                                                  error:&error];
            if (error) {
                snprintf(g_last_error, sizeof(g_last_error), "Failed to create provider: %s",
                        [[error localizedDescription] UTF8String]);
                return -3;
            }

            // Stateful prediction - keeps KV cache GPU-resident
            id<MLFeatureProvider> outputProvider = [model predictionFromFeatures:inputProvider
                                                                     usingState:state
                                                                          error:&error];
            if (error) {
                snprintf(g_last_error, sizeof(g_last_error), "Stateful prediction failed: %s",
                        [[error localizedDescription] UTF8String]);
                return -4;
            }

            MLFeatureValue *outputValue = [outputProvider featureValueForName:@"logits"];
            if (!outputValue || outputValue.type != MLFeatureTypeMultiArray) {
                snprintf(g_last_error, sizeof(g_last_error), "Output logits not found");
                return -5;
            }

            MLMultiArray *outputArray = outputValue.multiArrayValue;
            float *outputPtr = (float*)outputArray.dataPointer;
            size_t copyLen = output_len < (size_t)outputArray.count ? output_len : (size_t)outputArray.count;
            memcpy(output_logits, outputPtr, copyLen * sizeof(float));

            return 0;
        }

        snprintf(g_last_error, sizeof(g_last_error), "MLState requires macOS 15.0+");
        return -100;
    }
}

void coreml_free_state(void* state_handle) {
    if (state_handle) {
        if (@available(macOS 15.0, *)) {
            MLState *state = (__bridge_transfer MLState*)state_handle;
            state = nil;
        }
    }
}
#else
int32_t coreml_predict_with_state(
    void* handle,
    void* state_handle,
    const uint32_t* input_ids,
    size_t input_len,
    float* output_logits,
    size_t output_len
) {
    (void)handle; (void)state_handle; (void)input_ids; (void)input_len; (void)output_logits; (void)output_len;
    snprintf(g_last_error, sizeof(g_last_error), "MLState not available - SDK < macOS 15");
    return -100;
}

void coreml_free_state(void* state_handle) {
    (void)state_handle;
}
#endif

// ========== MLTensor API (macOS 15+) ==========

typedef struct {
    void* tensor_ptr;
    size_t shape[16];
    uint32_t rank;
} MLTensorHandle;

bool coreml_supports_mltensor() {
    if (@available(macOS 15.0, *)) {
        return true;
    }
    return false;
}

#if HAS_MLTENSOR
MLTensorHandle coreml_create_tensor_f32(
    const float* scalars,
    const size_t* shape,
    size_t rank
) {
    MLTensorHandle handle = {nullptr, {0}, 0};

    @autoreleasepool {
        if (!scalars || !shape || rank == 0 || rank > 16) {
            snprintf(g_last_error, sizeof(g_last_error), "Invalid tensor parameters");
            return handle;
        }

        if (@available(macOS 15.0, *)) {
            // Convert shape to NSArray
            NSMutableArray<NSNumber*> *nsShape = [NSMutableArray arrayWithCapacity:rank];
            size_t total_elements = 1;
            for (size_t i = 0; i < rank; i++) {
                [nsShape addObject:@(shape[i])];
                total_elements *= shape[i];
            }

            // Create data buffer
            NSData *data = [NSData dataWithBytes:scalars length:total_elements * sizeof(float)];

            NSError *error = nil;
            MLShapedArray<NSNumber*> *shapedArray = [[MLShapedArray alloc] initWithData:data
                                                                                  shape:nsShape
                                                                               dataType:MLShapedArrayDataTypeFloat32];

            if (!shapedArray) {
                snprintf(g_last_error, sizeof(g_last_error), "Failed to create shaped array");
                return handle;
            }

            MLTensor *tensor = [[MLTensor alloc] initWithShapedArray:shapedArray];

            if (!tensor) {
                snprintf(g_last_error, sizeof(g_last_error), "Failed to create MLTensor");
                return handle;
            }

            handle.tensor_ptr = (__bridge_retained void*)tensor;
            handle.rank = (uint32_t)rank;
            for (size_t i = 0; i < rank; i++) {
                handle.shape[i] = shape[i];
            }

            return handle;
        } else {
            snprintf(g_last_error, sizeof(g_last_error), "MLTensor requires macOS 15.0+");
            return handle;
        }
    }
}

MLTensorHandle coreml_tensor_softmax(
    MLTensorHandle tensor_handle,
    int32_t dim
) {
    MLTensorHandle result = {nullptr, {0}, 0};

    @autoreleasepool {
        if (!tensor_handle.tensor_ptr) {
            snprintf(g_last_error, sizeof(g_last_error), "Null tensor handle");
            return result;
        }

        if (@available(macOS 15.0, *)) {
            MLTensor *tensor = (__bridge MLTensor*)tensor_handle.tensor_ptr;
            MLTensor *softmax = [tensor softmaxAlongAxis:dim];

            result.tensor_ptr = (__bridge_retained void*)softmax;
            result.rank = tensor_handle.rank;
            for (uint32_t i = 0; i < tensor_handle.rank; i++) {
                result.shape[i] = tensor_handle.shape[i];
            }
            return result;
        } else {
            snprintf(g_last_error, sizeof(g_last_error), "MLTensor requires macOS 15.0+");
            return result;
        }
    }
}

MLTensorHandle coreml_tensor_add(
    MLTensorHandle tensor1_handle,
    MLTensorHandle tensor2_handle
) {
    MLTensorHandle result = {nullptr, {0}, 0};

    @autoreleasepool {
        if (!tensor1_handle.tensor_ptr || !tensor2_handle.tensor_ptr) {
            snprintf(g_last_error, sizeof(g_last_error), "Null tensor handle");
            return result;
        }

        if (@available(macOS 15.0, *)) {
            MLTensor *t1 = (__bridge MLTensor*)tensor1_handle.tensor_ptr;
            MLTensor *t2 = (__bridge MLTensor*)tensor2_handle.tensor_ptr;
            MLTensor *added = [t1 adding:t2];

            result.tensor_ptr = (__bridge_retained void*)added;
            result.rank = tensor1_handle.rank;
            for (uint32_t i = 0; i < tensor1_handle.rank; i++) {
                result.shape[i] = tensor1_handle.shape[i];
            }
            return result;
        } else {
            snprintf(g_last_error, sizeof(g_last_error), "MLTensor requires macOS 15.0+");
            return result;
        }
    }
}

MLTensorHandle coreml_tensor_scale(
    MLTensorHandle tensor_handle,
    float scale
) {
    MLTensorHandle result = {nullptr, {0}, 0};

    @autoreleasepool {
        if (!tensor_handle.tensor_ptr) {
            snprintf(g_last_error, sizeof(g_last_error), "Null tensor handle");
            return result;
        }

        if (@available(macOS 15.0, *)) {
            MLTensor *tensor = (__bridge MLTensor*)tensor_handle.tensor_ptr;
            MLTensor *scaled = [tensor multiplyingBy:@(scale)];

            result.tensor_ptr = (__bridge_retained void*)scaled;
            result.rank = tensor_handle.rank;
            for (uint32_t i = 0; i < tensor_handle.rank; i++) {
                result.shape[i] = tensor_handle.shape[i];
            }
            return result;
        } else {
            snprintf(g_last_error, sizeof(g_last_error), "MLTensor requires macOS 15.0+");
            return result;
        }
    }
}

MLTensorHandle coreml_tensor_matmul(
    MLTensorHandle tensor1_handle,
    MLTensorHandle tensor2_handle
) {
    MLTensorHandle result = {nullptr, {0}, 0};

    @autoreleasepool {
        if (!tensor1_handle.tensor_ptr || !tensor2_handle.tensor_ptr) {
            snprintf(g_last_error, sizeof(g_last_error), "Null tensor handle");
            return result;
        }

        if (@available(macOS 15.0, *)) {
            MLTensor *t1 = (__bridge MLTensor*)tensor1_handle.tensor_ptr;
            MLTensor *t2 = (__bridge MLTensor*)tensor2_handle.tensor_ptr;
            MLTensor *product = [t1 matrixMultiplyingBy:t2];

            result.tensor_ptr = (__bridge_retained void*)product;
            // Output shape for matmul: [m, k] x [k, n] = [m, n]
            result.rank = 2;
            result.shape[0] = tensor1_handle.shape[0];
            result.shape[1] = tensor2_handle.shape[1];
            return result;
        } else {
            snprintf(g_last_error, sizeof(g_last_error), "MLTensor requires macOS 15.0+");
            return result;
        }
    }
}

int32_t coreml_tensor_to_floats(
    MLTensorHandle tensor_handle,
    float* output,
    size_t output_len
) {
    @autoreleasepool {
        if (!tensor_handle.tensor_ptr || !output) {
            snprintf(g_last_error, sizeof(g_last_error), "Invalid parameters");
            return -1;
        }

        if (@available(macOS 15.0, *)) {
            MLTensor *tensor = (__bridge MLTensor*)tensor_handle.tensor_ptr;

            // Use synchronous materialization with semaphore
            __block int32_t result_code = -2;
            dispatch_semaphore_t sem = dispatch_semaphore_create(0);

            [tensor shapedArrayWithCompletionHandler:^(MLShapedArray *array, NSError *error) {
                if (error) {
                    snprintf(g_last_error, sizeof(g_last_error),
                            "Failed to materialize tensor: %s",
                            [[error localizedDescription] UTF8String]);
                    result_code = -3;
                } else {
                    const float *src = (const float *)array.bytes;
                    size_t available = array.count;
                    size_t copied = available < output_len ? available : output_len;
                    memcpy(output, src, copied * sizeof(float));
                    result_code = (int32_t)copied;
                }
                dispatch_semaphore_signal(sem);
            }];

            // Wait with 5 second timeout
            if (dispatch_semaphore_wait(sem, dispatch_time(DISPATCH_TIME_NOW, 5LL * NSEC_PER_SEC)) != 0) {
                snprintf(g_last_error, sizeof(g_last_error), "Tensor materialization timeout");
                return -4;
            }

            return result_code;
        } else {
            snprintf(g_last_error, sizeof(g_last_error), "MLTensor requires macOS 15.0+");
            return -100;
        }
    }
}

void coreml_tensor_free(MLTensorHandle handle) {
    if (handle.tensor_ptr) {
        @autoreleasepool {
            if (@available(macOS 15.0, *)) {
                MLTensor *tensor = (__bridge_transfer MLTensor*)handle.tensor_ptr;
                tensor = nil;
            }
        }
    }
}

#else
// Stub implementations when SDK < macOS 15
// Uses MLMultiArray instead of MLTensor for compatibility
MLTensorHandle coreml_create_tensor_f32(const float* scalars, const size_t* shape, size_t rank) {
    MLTensorHandle handle = {nullptr, {0}, 0};

    @autoreleasepool {
        if (!scalars || !shape || rank == 0 || rank > 16) {
            snprintf(g_last_error, sizeof(g_last_error), "Invalid tensor parameters");
            return handle;
        }

        // Convert shape to NSArray and calculate total elements
        NSMutableArray<NSNumber*> *nsShape = [NSMutableArray arrayWithCapacity:rank];
        size_t total_elements = 1;
        for (size_t i = 0; i < rank; i++) {
            [nsShape addObject:@(shape[i])];
            total_elements *= shape[i];
        }

        // Create MLMultiArray with the specified shape
        NSError *error = nil;
        MLMultiArray *array = [[MLMultiArray alloc] initWithShape:nsShape
                                                         dataType:MLMultiArrayDataTypeFloat32
                                                            error:&error];
        if (error || !array) {
            snprintf(g_last_error, sizeof(g_last_error), "Failed to create MLMultiArray: %s",
                    error ? [[error localizedDescription] UTF8String] : "unknown error");
            return handle;
        }

        // Copy scalar data into the MLMultiArray
        float *dataPtr = (float*)array.dataPointer;
        if (!dataPtr) {
            snprintf(g_last_error, sizeof(g_last_error), "Failed to get MLMultiArray data pointer");
            return handle;
        }
        memcpy(dataPtr, scalars, total_elements * sizeof(float));

        // Store the MLMultiArray pointer in the handle
        handle.tensor_ptr = (__bridge_retained void*)array;
        handle.rank = (uint32_t)rank;
        for (size_t i = 0; i < rank; i++) {
            handle.shape[i] = shape[i];
        }

        return handle;
    }
}

MLTensorHandle coreml_tensor_softmax(MLTensorHandle tensor_handle, int32_t dim) {
    (void)dim;  // Note: dim parameter reserved for future axis-specific softmax
    MLTensorHandle result = {nullptr, {0}, 0};

    @autoreleasepool {
        if (!tensor_handle.tensor_ptr) {
            snprintf(g_last_error, sizeof(g_last_error), "Null tensor handle");
            return result;
        }

        // Get the MLMultiArray from the tensor handle
        MLMultiArray *inputArray = (__bridge MLMultiArray*)tensor_handle.tensor_ptr;

        // Calculate total elements
        size_t total_elements = 1;
        for (uint32_t i = 0; i < tensor_handle.rank; i++) {
            total_elements *= tensor_handle.shape[i];
        }

        if (total_elements == 0) {
            snprintf(g_last_error, sizeof(g_last_error), "Empty tensor");
            return result;
        }

        // Get input data pointer
        float *inputPtr = (float*)inputArray.dataPointer;

        // Create output MLMultiArray with same shape
        NSMutableArray<NSNumber*> *nsShape = [NSMutableArray arrayWithCapacity:tensor_handle.rank];
        for (uint32_t i = 0; i < tensor_handle.rank; i++) {
            [nsShape addObject:@(tensor_handle.shape[i])];
        }

        NSError *error = nil;
        MLMultiArray *outputArray = [[MLMultiArray alloc] initWithShape:nsShape
                                                               dataType:MLMultiArrayDataTypeFloat32
                                                                  error:&error];
        if (error || !outputArray) {
            snprintf(g_last_error, sizeof(g_last_error), "Failed to create output array: %s",
                    error ? [[error localizedDescription] UTF8String] : "unknown error");
            return result;
        }

        float *outputPtr = (float*)outputArray.dataPointer;

        // Compute softmax using Accelerate framework
        // For simplicity, compute softmax over the entire flattened tensor
        // (full axis-aware implementation would require stride calculations)

        // Allocate temporary buffer for exp values
        float *expBuffer = (float*)malloc(total_elements * sizeof(float));
        if (!expBuffer) {
            snprintf(g_last_error, sizeof(g_last_error), "Failed to allocate exp buffer");
            return result;
        }

        // Find max for numerical stability (prevents overflow in exp)
        float maxVal = inputPtr[0];
        for (size_t i = 1; i < total_elements; i++) {
            if (inputPtr[i] > maxVal) {
                maxVal = inputPtr[i];
            }
        }

        // Subtract max and compute exp: expBuffer[i] = exp(input[i] - max)
        // First subtract max
        float negMax = -maxVal;
        vDSP_vsadd(inputPtr, 1, &negMax, expBuffer, 1, (vDSP_Length)total_elements);

        // Compute exp using vvexpf (from vecLib, part of Accelerate)
        int n = (int)total_elements;
        vvexpf(expBuffer, expBuffer, &n);

        // Sum all exp values using vDSP_sve
        float sum = 0.0f;
        vDSP_sve(expBuffer, 1, &sum, (vDSP_Length)total_elements);

        // Divide by sum to get softmax: output[i] = exp[i] / sum
        vDSP_vsdiv(expBuffer, 1, &sum, outputPtr, 1, (vDSP_Length)total_elements);

        free(expBuffer);

        // Set up result handle
        result.tensor_ptr = (__bridge_retained void*)outputArray;
        result.rank = tensor_handle.rank;
        for (uint32_t i = 0; i < tensor_handle.rank; i++) {
            result.shape[i] = tensor_handle.shape[i];
        }

        return result;
    }
}

MLTensorHandle coreml_tensor_add(MLTensorHandle tensor1_handle, MLTensorHandle tensor2_handle) {
    MLTensorHandle result = {nullptr, {0}, 0};

    @autoreleasepool {
        if (!tensor1_handle.tensor_ptr || !tensor2_handle.tensor_ptr) {
            snprintf(g_last_error, sizeof(g_last_error), "Null tensor handle");
            return result;
        }

        // Get MLMultiArray from tensor handles
        MLMultiArray *array1 = (__bridge MLMultiArray*)tensor1_handle.tensor_ptr;
        MLMultiArray *array2 = (__bridge MLMultiArray*)tensor2_handle.tensor_ptr;

        // Verify shapes match
        if (array1.count != array2.count) {
            snprintf(g_last_error, sizeof(g_last_error), "Tensor shapes do not match for addition");
            return result;
        }

        // Calculate total elements
        size_t total_elements = 1;
        for (uint32_t i = 0; i < tensor1_handle.rank; i++) {
            total_elements *= tensor1_handle.shape[i];
        }

        // Create result MLMultiArray with same shape
        NSMutableArray<NSNumber*> *nsShape = [NSMutableArray arrayWithCapacity:tensor1_handle.rank];
        for (uint32_t i = 0; i < tensor1_handle.rank; i++) {
            [nsShape addObject:@(tensor1_handle.shape[i])];
        }

        NSError *error = nil;
        MLMultiArray *resultArray = [[MLMultiArray alloc] initWithShape:nsShape
                                                               dataType:MLMultiArrayDataTypeFloat32
                                                                  error:&error];
        if (error || !resultArray) {
            snprintf(g_last_error, sizeof(g_last_error), "Failed to create result array: %s",
                    error ? [[error localizedDescription] UTF8String] : "unknown error");
            return result;
        }

        // Get data pointers
        const float *ptr1 = (const float*)array1.dataPointer;
        const float *ptr2 = (const float*)array2.dataPointer;
        float *resultPtr = (float*)resultArray.dataPointer;

        // Use vDSP_vadd for vectorized element-wise addition
        // vDSP_vadd(A, strideA, B, strideB, C, strideC, N)
        // C[i] = A[i] + B[i]
        vDSP_vadd(ptr1, 1, ptr2, 1, resultPtr, 1, (vDSP_Length)total_elements);

        // Set up result handle
        result.tensor_ptr = (__bridge_retained void*)resultArray;
        result.rank = tensor1_handle.rank;
        for (uint32_t i = 0; i < tensor1_handle.rank; i++) {
            result.shape[i] = tensor1_handle.shape[i];
        }

        return result;
    }
}

MLTensorHandle coreml_tensor_scale(MLTensorHandle tensor_handle, float scale) {
    MLTensorHandle result = {nullptr, {0}, 0};

    @autoreleasepool {
        if (!tensor_handle.tensor_ptr) {
            snprintf(g_last_error, sizeof(g_last_error), "Null tensor handle");
            return result;
        }

        // Get source MLMultiArray from handle
        MLMultiArray *srcArray = (__bridge MLMultiArray*)tensor_handle.tensor_ptr;

        // Verify data type is Float32
        if (srcArray.dataType != MLMultiArrayDataTypeFloat32) {
            snprintf(g_last_error, sizeof(g_last_error), "Tensor must be Float32 for vDSP operations");
            return result;
        }

        // Get data pointer and element count
        float *srcPtr = (float*)srcArray.dataPointer;
        size_t count = (size_t)srcArray.count;

        // Create result MLMultiArray with same shape
        NSError *error = nil;
        MLMultiArray *dstArray = [[MLMultiArray alloc] initWithShape:srcArray.shape
                                                            dataType:MLMultiArrayDataTypeFloat32
                                                               error:&error];
        if (error || !dstArray) {
            snprintf(g_last_error, sizeof(g_last_error), "Failed to create result array: %s",
                    error ? [[error localizedDescription] UTF8String] : "unknown error");
            return result;
        }

        float *dstPtr = (float*)dstArray.dataPointer;

        // Use vDSP_vsmul for scalar multiplication
        // vDSP_vsmul(A, IA, B, C, IC, N)
        // C[i] = A[i] * B for i in 0..N
        vDSP_vsmul(srcPtr, 1, &scale, dstPtr, 1, (vDSP_Length)count);

        // Copy shape information to result handle
        result.tensor_ptr = (__bridge_retained void*)dstArray;
        result.rank = tensor_handle.rank;
        for (uint32_t i = 0; i < tensor_handle.rank && i < 16; i++) {
            result.shape[i] = tensor_handle.shape[i];
        }

        return result;
    }
}

MLTensorHandle coreml_tensor_matmul(MLTensorHandle tensor1_handle, MLTensorHandle tensor2_handle) {
    MLTensorHandle result = {nullptr, {0}, 0};

    @autoreleasepool {
        // Validate inputs
        if (!tensor1_handle.tensor_ptr || !tensor2_handle.tensor_ptr) {
            snprintf(g_last_error, sizeof(g_last_error), "Null tensor handle");
            return result;
        }

        // Require 2D matrices for matmul
        if (tensor1_handle.rank != 2 || tensor2_handle.rank != 2) {
            snprintf(g_last_error, sizeof(g_last_error), "Matmul requires 2D tensors");
            return result;
        }

        // Get dimensions: A is (M x K), B is (K x N), result is (M x N)
        size_t M = tensor1_handle.shape[0];
        size_t K = tensor1_handle.shape[1];
        size_t K2 = tensor2_handle.shape[0];
        size_t N = tensor2_handle.shape[1];

        // Verify inner dimensions match
        if (K != K2) {
            snprintf(g_last_error, sizeof(g_last_error),
                    "Matmul dimension mismatch: %zu x %zu and %zu x %zu", M, K, K2, N);
            return result;
        }

        // Get data pointers from MLMultiArray tensors
        MLMultiArray *arr1 = (__bridge MLMultiArray*)tensor1_handle.tensor_ptr;
        MLMultiArray *arr2 = (__bridge MLMultiArray*)tensor2_handle.tensor_ptr;

        const float *A = (const float*)arr1.dataPointer;
        const float *B = (const float*)arr2.dataPointer;

        if (!A || !B) {
            snprintf(g_last_error, sizeof(g_last_error), "Failed to get tensor data pointers");
            return result;
        }

        // Create output MLMultiArray (M x N)
        NSError *error = nil;
        NSArray<NSNumber*> *outputShape = @[@(M), @(N)];
        MLMultiArray *outputArray = [[MLMultiArray alloc] initWithShape:outputShape
                                                               dataType:MLMultiArrayDataTypeFloat32
                                                                  error:&error];
        if (error || !outputArray) {
            snprintf(g_last_error, sizeof(g_last_error), "Failed to create output array: %s",
                    error ? [[error localizedDescription] UTF8String] : "unknown error");
            return result;
        }

        float *C = (float*)outputArray.dataPointer;
        if (!C) {
            snprintf(g_last_error, sizeof(g_last_error), "Failed to get output data pointer");
            return result;
        }

        // Perform matrix multiplication using cblas_sgemm from Accelerate framework
        // C = alpha * A * B + beta * C
        // With alpha=1.0, beta=0.0: C = A * B
        cblas_sgemm(
            CblasRowMajor,    // Row-major storage
            CblasNoTrans,     // Don't transpose A
            CblasNoTrans,     // Don't transpose B
            (int)M,           // Rows of A and C
            (int)N,           // Columns of B and C
            (int)K,           // Columns of A, rows of B
            1.0f,             // alpha
            A,                // Matrix A
            (int)K,           // Leading dimension of A
            B,                // Matrix B
            (int)N,           // Leading dimension of B
            0.0f,             // beta
            C,                // Matrix C (output)
            (int)N            // Leading dimension of C
        );

        // Set up result handle
        result.tensor_ptr = (__bridge_retained void*)outputArray;
        result.rank = 2;
        result.shape[0] = M;
        result.shape[1] = N;

        return result;
    }
}

int32_t coreml_tensor_to_floats(MLTensorHandle tensor_handle, float* output, size_t output_len) {
    @autoreleasepool {
        if (!tensor_handle.tensor_ptr || !output) {
            snprintf(g_last_error, sizeof(g_last_error), "Invalid parameters");
            return -1;
        }

        // In non-MLTensor mode, tensor_ptr holds an MLMultiArray
        MLMultiArray *array = (__bridge MLMultiArray*)tensor_handle.tensor_ptr;

        // Access the raw data pointer
        float *dataPtr = (float*)array.dataPointer;
        if (!dataPtr) {
            snprintf(g_last_error, sizeof(g_last_error), "Failed to access tensor data pointer");
            return -2;
        }

        // Get number of elements available
        size_t available = (size_t)array.count;
        size_t copyLen = available < output_len ? available : output_len;

        // Copy data to output buffer
        memcpy(output, dataPtr, copyLen * sizeof(float));

        // Return number of elements copied
        return (int32_t)copyLen;
    }
}

void coreml_tensor_free(MLTensorHandle handle) {
    if (handle.tensor_ptr) {
        @autoreleasepool {
            // Release the MLMultiArray
            MLMultiArray *array = (__bridge_transfer MLMultiArray*)handle.tensor_ptr;
            array = nil;
        }
    }
}
#endif

}

// ========== Async Prediction API ==========

// Callback typedef for async predictions
typedef void (*coreml_prediction_callback)(int32_t status, float* output, size_t output_len, void* user_data);

// Cancellation token for async operations
// Uses atomic operations to ensure thread-safe access
typedef struct {
    _Atomic(bool) cancelled;
    _Atomic(bool) completed;
} coreml_cancellation_token;

// Create a cancellation token
extern "C" coreml_cancellation_token* coreml_create_cancellation_token() {
    coreml_cancellation_token* token = (coreml_cancellation_token*)malloc(sizeof(coreml_cancellation_token));
    if (token) {
        token->cancelled = false;
        token->completed = false;
    }
    return token;
}

// Cancel an async operation
extern "C" void coreml_cancel(coreml_cancellation_token* token) {
    if (token) {
        token->cancelled = true;
    }
}

// Free a cancellation token
extern "C" void coreml_free_cancellation_token(coreml_cancellation_token* token) {
    if (token) {
        free(token);
    }
}

// Default timeout in seconds
static const int64_t COREML_DEFAULT_TIMEOUT_SECS = 30;

// Error codes for async operations
static const int32_t COREML_ERROR_TIMEOUT = -20;
static const int32_t COREML_ERROR_CANCELLED = -21;

// Async prediction with timeout and cancellation support
void coreml_predict_async_with_timeout(
    void* handle,
    const uint32_t* input_ids,
    size_t input_len,
    coreml_prediction_callback callback,
    void* user_data,
    coreml_cancellation_token* cancel_token,
    int64_t timeout_secs
) {
    if (!handle || !callback) {
        if (callback) {
            callback(-1, NULL, 0, user_data);
        }
        return;
    }

    if (timeout_secs <= 0) {
        timeout_secs = COREML_DEFAULT_TIMEOUT_SECS;
    }

    MLModel *model = (__bridge MLModel*)handle;

    uint32_t *input_copy = (uint32_t*)malloc(input_len * sizeof(uint32_t));
    if (!input_copy) {
        callback(-10, NULL, 0, user_data);
        return;
    }
    memcpy(input_copy, input_ids, input_len * sizeof(uint32_t));

    // Shared state for preventing double-callback using atomic operations
    __block _Atomic(bool) callback_invoked = ATOMIC_VAR_INIT(false);
    __block dispatch_semaphore_t completion_sem = dispatch_semaphore_create(0);

    // Schedule timeout handler
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, timeout_secs * NSEC_PER_SEC),
                   dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
        // Check if already completed or cancelled using atomic compare-and-swap
        bool expected = false;
        if (atomic_compare_exchange_strong(&callback_invoked, &expected, true)) {
            snprintf(g_last_error, sizeof(g_last_error), "Prediction timeout after %lld seconds", timeout_secs);
            callback(COREML_ERROR_TIMEOUT, NULL, 0, user_data);
            free(input_copy);
        }
        dispatch_semaphore_signal(completion_sem);
    });

    dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
        @autoreleasepool {
            // Check for cancellation before starting
            if (cancel_token && atomic_load(&cancel_token->cancelled)) {
                bool expected = false;
                if (atomic_compare_exchange_strong(&callback_invoked, &expected, true)) {
                    snprintf(g_last_error, sizeof(g_last_error), "Prediction cancelled");
                    callback(COREML_ERROR_CANCELLED, NULL, 0, user_data);
                    free(input_copy);
                }
                dispatch_semaphore_signal(completion_sem);
                return;
            }

            NSError *error = nil;

            NSArray<NSNumber*> *shape = @[@(1), @(input_len)];
            MLMultiArray *inputArray = [[MLMultiArray alloc] initWithShape:shape
                                                                  dataType:MLMultiArrayDataTypeInt32
                                                                     error:&error];
            if (error) {
                bool expected = false;
                if (atomic_compare_exchange_strong(&callback_invoked, &expected, true)) {
                    snprintf(g_last_error, sizeof(g_last_error), "Failed to create input array: %s",
                            [[error localizedDescription] UTF8String]);
                    callback(-2, NULL, 0, user_data);
                    free(input_copy);
                }
                dispatch_semaphore_signal(completion_sem);
                return;
            }

            int32_t *inputPtr = (int32_t*)inputArray.dataPointer;
            for (size_t i = 0; i < input_len; i++) {
                inputPtr[i] = (int32_t)input_copy[i];
            }

            // Check for cancellation before prediction
            if (cancel_token && atomic_load(&cancel_token->cancelled)) {
                bool expected = false;
                if (atomic_compare_exchange_strong(&callback_invoked, &expected, true)) {
                    snprintf(g_last_error, sizeof(g_last_error), "Prediction cancelled");
                    callback(COREML_ERROR_CANCELLED, NULL, 0, user_data);
                    free(input_copy);
                }
                dispatch_semaphore_signal(completion_sem);
                return;
            }

            MLDictionaryFeatureProvider *inputProvider =
                [[MLDictionaryFeatureProvider alloc] initWithDictionary:@{@"input_ids": inputArray}
                                                                  error:&error];
            if (error) {
                bool expected = false;
                if (atomic_compare_exchange_strong(&callback_invoked, &expected, true)) {
                    snprintf(g_last_error, sizeof(g_last_error), "Failed to create feature provider: %s",
                            [[error localizedDescription] UTF8String]);
                    callback(-3, NULL, 0, user_data);
                    free(input_copy);
                }
                dispatch_semaphore_signal(completion_sem);
                return;
            }

            id<MLFeatureProvider> outputProvider = [model predictionFromFeatures:inputProvider
                                                                           error:&error];

            // Check for timeout/cancellation after prediction
            if (atomic_load(&callback_invoked)) {
                free(input_copy);
                dispatch_semaphore_signal(completion_sem);
                return;
            }

            if (error) {
                bool expected = false;
                if (atomic_compare_exchange_strong(&callback_invoked, &expected, true)) {
                    snprintf(g_last_error, sizeof(g_last_error), "Prediction failed: %s",
                            [[error localizedDescription] UTF8String]);
                    callback(-4, NULL, 0, user_data);
                    free(input_copy);
                }
                dispatch_semaphore_signal(completion_sem);
                return;
            }

            MLFeatureValue *outputValue = [outputProvider featureValueForName:@"logits"];
            if (!outputValue || outputValue.type != MLFeatureTypeMultiArray) {
                bool expected = false;
                if (atomic_compare_exchange_strong(&callback_invoked, &expected, true)) {
                    snprintf(g_last_error, sizeof(g_last_error), "Output logits not found");
                    callback(-5, NULL, 0, user_data);
                    free(input_copy);
                }
                dispatch_semaphore_signal(completion_sem);
                return;
            }

            MLMultiArray *outputArray = outputValue.multiArrayValue;
            float *outputPtr = (float*)outputArray.dataPointer;
            size_t output_len = (size_t)outputArray.count;

            float *output_copy = (float*)malloc(output_len * sizeof(float));
            if (!output_copy) {
                bool expected = false;
                if (atomic_compare_exchange_strong(&callback_invoked, &expected, true)) {
                    snprintf(g_last_error, sizeof(g_last_error), "Failed to allocate output buffer");
                    callback(-10, NULL, 0, user_data);
                    free(input_copy);
                }
                dispatch_semaphore_signal(completion_sem);
                return;
            }
            memcpy(output_copy, outputPtr, output_len * sizeof(float));

            // Mark as completed and invoke callback
            bool expected = false;
            if (atomic_compare_exchange_strong(&callback_invoked, &expected, true)) {
                if (cancel_token) {
                    atomic_store(&cancel_token->completed, true);
                }
                callback(0, output_copy, output_len, user_data);
            } else {
                free(output_copy);
            }
            free(input_copy);
            dispatch_semaphore_signal(completion_sem);
        }
    });
}

// Async prediction with LoRA and timeout/cancellation support
void coreml_predict_async_with_lora_timeout(
    void* handle,
    const uint32_t* input_ids,
    size_t input_len,
    const uint16_t* adapter_indices,
    const int16_t* adapter_gates,
    size_t num_adapters,
    const float* const* lora_deltas,
    const size_t* delta_lens,
    coreml_prediction_callback callback,
    void* user_data,
    coreml_cancellation_token* cancel_token,
    int64_t timeout_secs
) {
    if (!handle || !callback) {
        if (callback) {
            callback(-1, NULL, 0, user_data);
        }
        return;
    }

    if (timeout_secs <= 0) {
        timeout_secs = COREML_DEFAULT_TIMEOUT_SECS;
    }

    MLModel *model = (__bridge MLModel*)handle;

    uint32_t *input_copy = (uint32_t*)malloc(input_len * sizeof(uint32_t));
    if (!input_copy) {
        callback(-10, NULL, 0, user_data);
        return;
    }
    memcpy(input_copy, input_ids, input_len * sizeof(uint32_t));

    int16_t *gates_copy = NULL;
    float **deltas_copy = NULL;
    size_t *delta_lens_copy = NULL;

    if (num_adapters > 0 && adapter_gates && lora_deltas && delta_lens) {
        gates_copy = (int16_t*)malloc(num_adapters * sizeof(int16_t));
        deltas_copy = (float**)malloc(num_adapters * sizeof(float*));
        delta_lens_copy = (size_t*)malloc(num_adapters * sizeof(size_t));

        if (!gates_copy || !deltas_copy || !delta_lens_copy) {
            free(input_copy);
            free(gates_copy);
            free(deltas_copy);
            free(delta_lens_copy);
            callback(-10, NULL, 0, user_data);
            return;
        }

        memcpy(gates_copy, adapter_gates, num_adapters * sizeof(int16_t));
        memcpy(delta_lens_copy, delta_lens, num_adapters * sizeof(size_t));

        for (size_t i = 0; i < num_adapters; i++) {
            if (lora_deltas[i] && delta_lens[i] > 0) {
                deltas_copy[i] = (float*)malloc(delta_lens[i] * sizeof(float));
                if (deltas_copy[i]) {
                    memcpy(deltas_copy[i], lora_deltas[i], delta_lens[i] * sizeof(float));
                }
            } else {
                deltas_copy[i] = NULL;
            }
        }
    }

    // Shared state for preventing double-callback using atomic operations
    __block _Atomic(bool) callback_invoked = ATOMIC_VAR_INIT(false);
    __block dispatch_semaphore_t completion_sem = dispatch_semaphore_create(0);

    // Helper block for cleanup
    void (^cleanup_resources)(void) = ^{
        free(input_copy);
        if (gates_copy) free(gates_copy);
        if (delta_lens_copy) free(delta_lens_copy);
        if (deltas_copy) {
            for (size_t i = 0; i < num_adapters; i++) {
                if (deltas_copy[i]) free(deltas_copy[i]);
            }
            free(deltas_copy);
        }
    };

    // Schedule timeout handler
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, timeout_secs * NSEC_PER_SEC),
                   dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
        bool expected = false;
        if (atomic_compare_exchange_strong(&callback_invoked, &expected, true)) {
            snprintf(g_last_error, sizeof(g_last_error), "Prediction timeout after %lld seconds", timeout_secs);
            callback(COREML_ERROR_TIMEOUT, NULL, 0, user_data);
            cleanup_resources();
        }
        dispatch_semaphore_signal(completion_sem);
    });

    dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
        @autoreleasepool {
            // Check for cancellation before starting
            if (cancel_token && atomic_load(&cancel_token->cancelled)) {
                bool expected = false;
                if (atomic_compare_exchange_strong(&callback_invoked, &expected, true)) {
                    snprintf(g_last_error, sizeof(g_last_error), "Prediction cancelled");
                    callback(COREML_ERROR_CANCELLED, NULL, 0, user_data);
                    cleanup_resources();
                }
                dispatch_semaphore_signal(completion_sem);
                return;
            }

            NSError *error = nil;

            NSArray<NSNumber*> *shape = @[@(1), @(input_len)];
            MLMultiArray *inputArray = [[MLMultiArray alloc] initWithShape:shape
                                                                  dataType:MLMultiArrayDataTypeInt32
                                                                     error:&error];
            if (error) {
                bool expected = false;
                if (atomic_compare_exchange_strong(&callback_invoked, &expected, true)) {
                    snprintf(g_last_error, sizeof(g_last_error), "Failed to create input array: %s",
                            [[error localizedDescription] UTF8String]);
                    callback(-2, NULL, 0, user_data);
                    cleanup_resources();
                }
                dispatch_semaphore_signal(completion_sem);
                return;
            }

            int32_t *inputPtr = (int32_t*)inputArray.dataPointer;
            for (size_t i = 0; i < input_len; i++) {
                inputPtr[i] = (int32_t)input_copy[i];
            }

            // Check for cancellation before prediction
            if (cancel_token && atomic_load(&cancel_token->cancelled)) {
                bool expected = false;
                if (atomic_compare_exchange_strong(&callback_invoked, &expected, true)) {
                    snprintf(g_last_error, sizeof(g_last_error), "Prediction cancelled");
                    callback(COREML_ERROR_CANCELLED, NULL, 0, user_data);
                    cleanup_resources();
                }
                dispatch_semaphore_signal(completion_sem);
                return;
            }

            MLDictionaryFeatureProvider *inputProvider =
                [[MLDictionaryFeatureProvider alloc] initWithDictionary:@{@"input_ids": inputArray}
                                                                  error:&error];
            if (error) {
                bool expected = false;
                if (atomic_compare_exchange_strong(&callback_invoked, &expected, true)) {
                    snprintf(g_last_error, sizeof(g_last_error), "Failed to create feature provider: %s",
                            [[error localizedDescription] UTF8String]);
                    callback(-3, NULL, 0, user_data);
                    cleanup_resources();
                }
                dispatch_semaphore_signal(completion_sem);
                return;
            }

            id<MLFeatureProvider> outputProvider = [model predictionFromFeatures:inputProvider
                                                                           error:&error];

            // Check for timeout/cancellation after prediction
            if (atomic_load(&callback_invoked)) {
                cleanup_resources();
                dispatch_semaphore_signal(completion_sem);
                return;
            }

            if (error) {
                bool expected = false;
                if (atomic_compare_exchange_strong(&callback_invoked, &expected, true)) {
                    snprintf(g_last_error, sizeof(g_last_error), "Prediction failed: %s",
                            [[error localizedDescription] UTF8String]);
                    callback(-4, NULL, 0, user_data);
                    cleanup_resources();
                }
                dispatch_semaphore_signal(completion_sem);
                return;
            }

            MLFeatureValue *outputValue = [outputProvider featureValueForName:@"logits"];
            if (!outputValue || outputValue.type != MLFeatureTypeMultiArray) {
                bool expected = false;
                if (atomic_compare_exchange_strong(&callback_invoked, &expected, true)) {
                    snprintf(g_last_error, sizeof(g_last_error), "Output logits not found");
                    callback(-5, NULL, 0, user_data);
                    cleanup_resources();
                }
                dispatch_semaphore_signal(completion_sem);
                return;
            }

            MLMultiArray *outputArray = outputValue.multiArrayValue;
            float *outputPtr = (float*)outputArray.dataPointer;
            size_t output_len = (size_t)outputArray.count;

            float *output_copy = (float*)malloc(output_len * sizeof(float));
            if (!output_copy) {
                bool expected = false;
                if (atomic_compare_exchange_strong(&callback_invoked, &expected, true)) {
                    snprintf(g_last_error, sizeof(g_last_error), "Failed to allocate output buffer");
                    callback(-10, NULL, 0, user_data);
                    cleanup_resources();
                }
                dispatch_semaphore_signal(completion_sem);
                return;
            }
            memcpy(output_copy, outputPtr, output_len * sizeof(float));

            // Apply LoRA deltas using vDSP for fused multiply-add
            if (num_adapters > 0 && gates_copy && deltas_copy && delta_lens_copy) {
                const float q15_scale = 1.0f / 32767.0f;

                for (size_t adapter_idx = 0; adapter_idx < num_adapters; adapter_idx++) {
                    float gate = (float)gates_copy[adapter_idx] * q15_scale;
                    if (gate == 0.0f) continue;

                    const float *delta = deltas_copy[adapter_idx];
                    size_t delta_len = delta_lens_copy[adapter_idx];

                    if (!delta || delta_len == 0) continue;

                    vDSP_Length apply_len = (vDSP_Length)(output_len < delta_len ? output_len : delta_len);
                    vDSP_vsma(delta, 1, &gate, output_copy, 1, output_copy, 1, apply_len);
                }
            }

            // Mark as completed and invoke callback
            bool expected = false;
            if (atomic_compare_exchange_strong(&callback_invoked, &expected, true)) {
                if (cancel_token) {
                    atomic_store(&cancel_token->completed, true);
                }
                callback(0, output_copy, output_len, user_data);
            } else {
                free(output_copy);
            }
            cleanup_resources();
            dispatch_semaphore_signal(completion_sem);
        }
    });
}

// Async prediction using GCD (legacy, no timeout/cancellation)
void coreml_predict_async(
    void* handle,
    const uint32_t* input_ids,
    size_t input_len,
    coreml_prediction_callback callback,
    void* user_data
) {
    if (!handle || !callback) {
        if (callback) {
            callback(-1, NULL, 0, user_data);
        }
        return;
    }

    MLModel *model = (__bridge MLModel*)handle;

    uint32_t *input_copy = (uint32_t*)malloc(input_len * sizeof(uint32_t));
    if (!input_copy) {
        callback(-10, NULL, 0, user_data);
        return;
    }
    memcpy(input_copy, input_ids, input_len * sizeof(uint32_t));

    dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
        @autoreleasepool {
            NSError *error = nil;

            NSArray<NSNumber*> *shape = @[@(1), @(input_len)];
            MLMultiArray *inputArray = [[MLMultiArray alloc] initWithShape:shape
                                                                  dataType:MLMultiArrayDataTypeInt32
                                                                     error:&error];
            if (error) {
                snprintf(g_last_error, sizeof(g_last_error), "Failed to create input array: %s",
                        [[error localizedDescription] UTF8String]);
                callback(-2, NULL, 0, user_data);
                free(input_copy);
                return;
            }

            int32_t *inputPtr = (int32_t*)inputArray.dataPointer;
            for (size_t i = 0; i < input_len; i++) {
                inputPtr[i] = (int32_t)input_copy[i];
            }

            MLDictionaryFeatureProvider *inputProvider =
                [[MLDictionaryFeatureProvider alloc] initWithDictionary:@{@"input_ids": inputArray}
                                                                  error:&error];
            if (error) {
                snprintf(g_last_error, sizeof(g_last_error), "Failed to create feature provider: %s",
                        [[error localizedDescription] UTF8String]);
                callback(-3, NULL, 0, user_data);
                free(input_copy);
                return;
            }

            id<MLFeatureProvider> outputProvider = [model predictionFromFeatures:inputProvider
                                                                           error:&error];
            if (error) {
                snprintf(g_last_error, sizeof(g_last_error), "Prediction failed: %s",
                        [[error localizedDescription] UTF8String]);
                callback(-4, NULL, 0, user_data);
                free(input_copy);
                return;
            }

            MLFeatureValue *outputValue = [outputProvider featureValueForName:@"logits"];
            if (!outputValue || outputValue.type != MLFeatureTypeMultiArray) {
                snprintf(g_last_error, sizeof(g_last_error), "Output logits not found");
                callback(-5, NULL, 0, user_data);
                free(input_copy);
                return;
            }

            MLMultiArray *outputArray = outputValue.multiArrayValue;
            float *outputPtr = (float*)outputArray.dataPointer;
            size_t output_len = (size_t)outputArray.count;

            float *output_copy = (float*)malloc(output_len * sizeof(float));
            if (!output_copy) {
                snprintf(g_last_error, sizeof(g_last_error), "Failed to allocate output buffer");
                callback(-10, NULL, 0, user_data);
                free(input_copy);
                return;
            }
            memcpy(output_copy, outputPtr, output_len * sizeof(float));

            callback(0, output_copy, output_len, user_data);
            free(input_copy);
        }
    });
}

// Async prediction with LoRA adapter support
void coreml_predict_async_with_lora(
    void* handle,
    const uint32_t* input_ids,
    size_t input_len,
    const uint16_t* adapter_indices,
    const int16_t* adapter_gates,
    size_t num_adapters,
    const float* const* lora_deltas,
    const size_t* delta_lens,
    coreml_prediction_callback callback,
    void* user_data
) {
    if (!handle || !callback) {
        if (callback) {
            callback(-1, NULL, 0, user_data);
        }
        return;
    }

    MLModel *model = (__bridge MLModel*)handle;

    uint32_t *input_copy = (uint32_t*)malloc(input_len * sizeof(uint32_t));
    if (!input_copy) {
        callback(-10, NULL, 0, user_data);
        return;
    }
    memcpy(input_copy, input_ids, input_len * sizeof(uint32_t));

    int16_t *gates_copy = NULL;
    float **deltas_copy = NULL;
    size_t *delta_lens_copy = NULL;

    if (num_adapters > 0 && adapter_gates && lora_deltas && delta_lens) {
        gates_copy = (int16_t*)malloc(num_adapters * sizeof(int16_t));
        deltas_copy = (float**)malloc(num_adapters * sizeof(float*));
        delta_lens_copy = (size_t*)malloc(num_adapters * sizeof(size_t));

        if (!gates_copy || !deltas_copy || !delta_lens_copy) {
            free(input_copy);
            free(gates_copy);
            free(deltas_copy);
            free(delta_lens_copy);
            callback(-10, NULL, 0, user_data);
            return;
        }

        memcpy(gates_copy, adapter_gates, num_adapters * sizeof(int16_t));
        memcpy(delta_lens_copy, delta_lens, num_adapters * sizeof(size_t));

        for (size_t i = 0; i < num_adapters; i++) {
            if (lora_deltas[i] && delta_lens[i] > 0) {
                deltas_copy[i] = (float*)malloc(delta_lens[i] * sizeof(float));
                if (deltas_copy[i]) {
                    memcpy(deltas_copy[i], lora_deltas[i], delta_lens[i] * sizeof(float));
                }
            } else {
                deltas_copy[i] = NULL;
            }
        }
    }

    dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
        @autoreleasepool {
            NSError *error = nil;

            NSArray<NSNumber*> *shape = @[@(1), @(input_len)];
            MLMultiArray *inputArray = [[MLMultiArray alloc] initWithShape:shape
                                                                  dataType:MLMultiArrayDataTypeInt32
                                                                     error:&error];
            if (error) {
                snprintf(g_last_error, sizeof(g_last_error), "Failed to create input array: %s",
                        [[error localizedDescription] UTF8String]);
                callback(-2, NULL, 0, user_data);
                free(input_copy);
                if (gates_copy) free(gates_copy);
                if (delta_lens_copy) free(delta_lens_copy);
                if (deltas_copy) {
                    for (size_t i = 0; i < num_adapters; i++) {
                        if (deltas_copy[i]) free(deltas_copy[i]);
                    }
                    free(deltas_copy);
                }
                return;
            }

            int32_t *inputPtr = (int32_t*)inputArray.dataPointer;
            for (size_t i = 0; i < input_len; i++) {
                inputPtr[i] = (int32_t)input_copy[i];
            }

            MLDictionaryFeatureProvider *inputProvider =
                [[MLDictionaryFeatureProvider alloc] initWithDictionary:@{@"input_ids": inputArray}
                                                                  error:&error];
            if (error) {
                snprintf(g_last_error, sizeof(g_last_error), "Failed to create feature provider: %s",
                        [[error localizedDescription] UTF8String]);
                callback(-3, NULL, 0, user_data);
                free(input_copy);
                if (gates_copy) free(gates_copy);
                if (delta_lens_copy) free(delta_lens_copy);
                if (deltas_copy) {
                    for (size_t i = 0; i < num_adapters; i++) {
                        if (deltas_copy[i]) free(deltas_copy[i]);
                    }
                    free(deltas_copy);
                }
                return;
            }

            id<MLFeatureProvider> outputProvider = [model predictionFromFeatures:inputProvider
                                                                           error:&error];
            if (error) {
                snprintf(g_last_error, sizeof(g_last_error), "Prediction failed: %s",
                        [[error localizedDescription] UTF8String]);
                callback(-4, NULL, 0, user_data);
                free(input_copy);
                if (gates_copy) free(gates_copy);
                if (delta_lens_copy) free(delta_lens_copy);
                if (deltas_copy) {
                    for (size_t i = 0; i < num_adapters; i++) {
                        if (deltas_copy[i]) free(deltas_copy[i]);
                    }
                    free(deltas_copy);
                }
                return;
            }

            MLFeatureValue *outputValue = [outputProvider featureValueForName:@"logits"];
            if (!outputValue || outputValue.type != MLFeatureTypeMultiArray) {
                snprintf(g_last_error, sizeof(g_last_error), "Output logits not found");
                callback(-5, NULL, 0, user_data);
                free(input_copy);
                if (gates_copy) free(gates_copy);
                if (delta_lens_copy) free(delta_lens_copy);
                if (deltas_copy) {
                    for (size_t i = 0; i < num_adapters; i++) {
                        if (deltas_copy[i]) free(deltas_copy[i]);
                    }
                    free(deltas_copy);
                }
                return;
            }

            MLMultiArray *outputArray = outputValue.multiArrayValue;
            float *outputPtr = (float*)outputArray.dataPointer;
            size_t output_len = (size_t)outputArray.count;

            float *output_copy = (float*)malloc(output_len * sizeof(float));
            if (!output_copy) {
                snprintf(g_last_error, sizeof(g_last_error), "Failed to allocate output buffer");
                callback(-10, NULL, 0, user_data);
                free(input_copy);
                if (gates_copy) free(gates_copy);
                if (delta_lens_copy) free(delta_lens_copy);
                if (deltas_copy) {
                    for (size_t i = 0; i < num_adapters; i++) {
                        if (deltas_copy[i]) free(deltas_copy[i]);
                    }
                    free(deltas_copy);
                }
                return;
            }
            memcpy(output_copy, outputPtr, output_len * sizeof(float));

            // Apply LoRA deltas using vDSP for fused multiply-add
            // Formula: output = base_output + sum(gate_i * lora_delta_i)
            if (num_adapters > 0 && gates_copy && deltas_copy && delta_lens_copy) {
                const float q15_scale = 1.0f / 32767.0f;

                for (size_t adapter_idx = 0; adapter_idx < num_adapters; adapter_idx++) {
                    // Dequantize Q15 gate value to float [-1.0, 1.0]
                    float gate = (float)gates_copy[adapter_idx] * q15_scale;

                    // Skip adapters with zero gate values
                    if (gate == 0.0f) continue;

                    const float *delta = deltas_copy[adapter_idx];
                    size_t delta_len = delta_lens_copy[adapter_idx];

                    if (!delta || delta_len == 0) continue;

                    // Limit to the smaller of output_len and delta_len
                    vDSP_Length apply_len = (vDSP_Length)(output_len < delta_len ? output_len : delta_len);

                    // vDSP_vsma: Vector scalar multiply and add
                    // output_copy[i] = delta[i] * gate + output_copy[i]
                    // Parameters: A (delta), stride_A, B (scalar gate), C (output_copy), stride_C, D (result), stride_D, N
                    vDSP_vsma(delta, 1, &gate, output_copy, 1, output_copy, 1, apply_len);
                }
            }

            callback(0, output_copy, output_len, user_data);

            free(input_copy);
            if (gates_copy) free(gates_copy);
            if (delta_lens_copy) free(delta_lens_copy);
            if (deltas_copy) {
                for (size_t i = 0; i < num_adapters; i++) {
                    if (deltas_copy[i]) free(deltas_copy[i]);
                }
                free(deltas_copy);
            }
        }
    });
}
