//! CoreML Bridge Implementation (Objective-C++)
//!
//! Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.
//!
//! This file provides the Objective-C++ bridge between Rust and CoreML.
//! All Apple frameworks (CoreML, MLModel) are accessed through this C FFI boundary.

#import <Foundation/Foundation.h>
#import <CoreML/CoreML.h>
#include "coreml_ffi.h"
#include <string>
#include <map>
#include <vector>
#include <mutex>

// ============================================================================
// Thread-Local Error Handling
// ============================================================================

static thread_local std::string g_last_error;
static std::mutex g_error_mutex;
static bool g_verbose_logging = false;

static void set_error(const std::string& error) {
    std::lock_guard<std::mutex> lock(g_error_mutex);
    g_last_error = error;
    if (g_verbose_logging) {
        NSLog(@"[CoreML FFI Error] %s", error.c_str());
    }
}

static void clear_error() {
    std::lock_guard<std::mutex> lock(g_error_mutex);
    g_last_error.clear();
}

// ============================================================================
// Opaque Type Wrappers
// ============================================================================

struct CoreMLModel {
    MLModel* model;
    NSString* path;
    MLModelConfiguration* config;

    CoreMLModel(MLModel* m, NSString* p, MLModelConfiguration* c)
        : model(m), path(p), config(c) {}

    ~CoreMLModel() {
        // ARC handles cleanup automatically
    }
};

struct CoreMLArray {
    MLMultiArray* array;

    CoreMLArray(MLMultiArray* a) : array(a) {}

    ~CoreMLArray() {
        // ARC handles cleanup automatically
    }
};

struct CoreMLPrediction {
    id<MLFeatureProvider> features;
    std::map<std::string, CoreMLArray*> outputs;

    CoreMLPrediction(id<MLFeatureProvider> f) : features(f) {}

    ~CoreMLPrediction() {
        // ARC handles features cleanup automatically
        for (auto& pair : outputs) {
            delete pair.second;
        }
    }
};

// ============================================================================
// Helper Functions
// ============================================================================

static NSArray<NSNumber*>* shape_to_nsarray(const CoreMLShape* shape) {
    NSMutableArray* array = [NSMutableArray arrayWithCapacity:shape->rank];
    for (size_t i = 0; i < shape->rank; i++) {
        [array addObject:@(shape->dimensions[i])];
    }
    return array;
}

static CoreMLShape nsarray_to_shape(NSArray<NSNumber*>* nsarray) {
    CoreMLShape shape;
    shape.rank = [nsarray count];
    shape.dimensions = (size_t*)malloc(shape.rank * sizeof(size_t));
    for (size_t i = 0; i < shape.rank; i++) {
        shape.dimensions[i] = [nsarray[i] unsignedLongValue];
    }
    return shape;
}

// ============================================================================
// Model Management
// ============================================================================

CoreMLModel* coreml_model_load(
    const char* path,
    bool use_gpu,
    bool use_ane,
    CoreMLErrorCode* error_code
) {
    @autoreleasepool {
        clear_error();

        if (!path) {
            set_error("Model path is NULL");
            if (error_code) *error_code = COREML_ERROR_INVALID_MODEL;
            return nullptr;
        }

        NSString* nsPath = [NSString stringWithUTF8String:path];
        NSURL* modelURL = [NSURL fileURLWithPath:nsPath];

        // Check if path exists
        if (![[NSFileManager defaultManager] fileExistsAtPath:nsPath]) {
            set_error(std::string("Model file not found: ") + path);
            if (error_code) *error_code = COREML_ERROR_IO;
            return nullptr;
        }

        // Configure model
        MLModelConfiguration* config = [[MLModelConfiguration alloc] init];

        // Set compute units based on acceleration preferences
        if (use_ane) {
            // Neural Engine preferred
            if (@available(macOS 13.0, *)) {
                config.computeUnits = MLComputeUnitsAll;
            } else {
                config.computeUnits = MLComputeUnitsCPUAndGPU;
            }
        } else if (use_gpu) {
            // GPU only
            config.computeUnits = MLComputeUnitsCPUAndGPU;
        } else {
            // CPU only
            config.computeUnits = MLComputeUnitsCPUOnly;
        }

        NSError* error = nil;
        MLModel* model = [MLModel modelWithContentsOfURL:modelURL
                                           configuration:config
                                                   error:&error];

        if (error) {
            std::string errorMsg = std::string("Failed to load CoreML model: ") +
                                   [[error localizedDescription] UTF8String];
            set_error(errorMsg);
            // ARC handles config cleanup automatically
            if (error_code) *error_code = COREML_ERROR_INVALID_MODEL;
            return nullptr;
        }

        if (g_verbose_logging) {
            NSLog(@"[CoreML FFI] Loaded model from: %@", nsPath);
            NSLog(@"[CoreML FFI] Compute units: %ld", (long)config.computeUnits);
        }

        if (error_code) *error_code = COREML_SUCCESS;
        return new CoreMLModel(model, nsPath, config);
    }
}

void coreml_model_free(CoreMLModel* model) {
    if (model) {
        if (g_verbose_logging) {
            NSLog(@"[CoreML FFI] Freeing model");
        }
        delete model;
    }
}

CoreMLModelMetadata coreml_model_get_metadata(CoreMLModel* model) {
    CoreMLModelMetadata metadata = {nullptr, nullptr, 0, 0, false, false};

    if (!model || !model->model) {
        return metadata;
    }

    @autoreleasepool {
        MLModelDescription* desc = model->model.modelDescription;

        // Note: Caller must free these strings
        NSString* versionStr = [desc.metadata objectForKey:MLModelVersionStringKey];
        NSString* descStr = [desc.metadata objectForKey:MLModelDescriptionKey];

        metadata.model_version = strdup([versionStr UTF8String] ?: "unknown");
        metadata.model_description = strdup([descStr UTF8String] ?: "");

        metadata.input_count = [[desc inputDescriptionsByName] count];
        metadata.output_count = [[desc outputDescriptionsByName] count];

        // Check compute unit support
        MLComputeUnits units = model->config.computeUnits;
        metadata.supports_gpu = (units == MLComputeUnitsCPUAndGPU || units == MLComputeUnitsAll);
        metadata.supports_ane = (units == MLComputeUnitsAll);
    }

    return metadata;
}

// ============================================================================
// Array Management
// ============================================================================

CoreMLArray* coreml_array_new(
    const float* data,
    const CoreMLShape* shape,
    CoreMLErrorCode* error_code
) {
    @autoreleasepool {
        clear_error();

        if (!data || !shape || shape->rank == 0) {
            set_error("Invalid array parameters");
            if (error_code) *error_code = COREML_ERROR_INVALID_INPUT;
            return nullptr;
        }

        NSArray<NSNumber*>* nsShape = shape_to_nsarray(shape);

        NSError* error = nil;
        MLMultiArray* array = [[MLMultiArray alloc] initWithShape:nsShape
                                                         dataType:MLMultiArrayDataTypeFloat32
                                                            error:&error];

        if (error) {
            set_error(std::string("Failed to create MLMultiArray: ") +
                     [[error localizedDescription] UTF8String]);
            if (error_code) *error_code = COREML_ERROR_MEMORY_ALLOCATION;
            return nullptr;
        }

        // Copy data
        size_t total_size = 1;
        for (size_t i = 0; i < shape->rank; i++) {
            total_size *= shape->dimensions[i];
        }

        float* arrayData = (float*)array.dataPointer;
        memcpy(arrayData, data, total_size * sizeof(float));

        if (error_code) *error_code = COREML_SUCCESS;
        return new CoreMLArray(array);
    }
}

CoreMLArray* coreml_array_new_int8(
    const int8_t* data,
    const CoreMLShape* shape,
    CoreMLErrorCode* error_code
) {
    @autoreleasepool {
        clear_error();

        if (!data || !shape || shape->rank == 0) {
            set_error("Invalid array parameters");
            if (error_code) *error_code = COREML_ERROR_INVALID_INPUT;
            return nullptr;
        }

        NSArray<NSNumber*>* nsShape = shape_to_nsarray(shape);

        NSError* error = nil;
        MLMultiArray* array = [[MLMultiArray alloc] initWithShape:nsShape
                                                         dataType:MLMultiArrayDataTypeInt8
                                                            error:&error];

        if (error) {
            set_error(std::string("Failed to create MLMultiArray (int8): ") +
                     [[error localizedDescription] UTF8String]);
            if (error_code) *error_code = COREML_ERROR_MEMORY_ALLOCATION;
            return nullptr;
        }

        // Copy data
        size_t total_size = 1;
        for (size_t i = 0; i < shape->rank; i++) {
            total_size *= shape->dimensions[i];
        }

        int8_t* arrayData = (int8_t*)array.dataPointer;
        memcpy(arrayData, data, total_size * sizeof(int8_t));

        if (error_code) *error_code = COREML_SUCCESS;
        return new CoreMLArray(array);
    }
}

CoreMLArray* coreml_array_new_float16(
    const uint16_t* data,
    const CoreMLShape* shape,
    CoreMLErrorCode* error_code
) {
    @autoreleasepool {
        clear_error();

        if (!data || !shape || shape->rank == 0) {
            set_error("Invalid array parameters");
            if (error_code) *error_code = COREML_ERROR_INVALID_INPUT;
            return nullptr;
        }

        NSArray<NSNumber*>* nsShape = shape_to_nsarray(shape);

        NSError* error = nil;
        MLMultiArray* array = [[MLMultiArray alloc] initWithShape:nsShape
                                                         dataType:MLMultiArrayDataTypeFloat16
                                                            error:&error];

        if (error) {
            set_error(std::string("Failed to create MLMultiArray (float16): ") +
                     [[error localizedDescription] UTF8String]);
            if (error_code) *error_code = COREML_ERROR_MEMORY_ALLOCATION;
            return nullptr;
        }

        // Copy data
        size_t total_size = 1;
        for (size_t i = 0; i < shape->rank; i++) {
            total_size *= shape->dimensions[i];
        }

        uint16_t* arrayData = (uint16_t*)array.dataPointer;
        memcpy(arrayData, data, total_size * sizeof(uint16_t));

        if (error_code) *error_code = COREML_SUCCESS;
        return new CoreMLArray(array);
    }
}

void coreml_array_free(CoreMLArray* array) {
    if (array) {
        delete array;
    }
}

const float* coreml_array_get_float_data(CoreMLArray* array) {
    if (!array || !array->array) {
        return nullptr;
    }

    if (array->array.dataType != MLMultiArrayDataTypeFloat32) {
        return nullptr;
    }

    return (const float*)array->array.dataPointer;
}

const int8_t* coreml_array_get_int8_data(CoreMLArray* array) {
    if (!array || !array->array) {
        return nullptr;
    }

    if (array->array.dataType != MLMultiArrayDataTypeInt8) {
        return nullptr;
    }

    return (const int8_t*)array->array.dataPointer;
}

CoreMLShape coreml_array_get_shape(CoreMLArray* array) {
    CoreMLShape shape = {nullptr, 0};

    if (!array || !array->array) {
        return shape;
    }

    return nsarray_to_shape(array->array.shape);
}

size_t coreml_array_get_size(CoreMLArray* array) {
    if (!array || !array->array) {
        return 0;
    }

    return [array->array count];
}

// ============================================================================
// Inference
// ============================================================================

CoreMLPrediction* coreml_predict(
    CoreMLModel* model,
    CoreMLArray* input,
    const char* input_name,
    CoreMLErrorCode* error_code
) {
    @autoreleasepool {
        clear_error();

        if (!model || !model->model || !input || !input->array) {
            set_error("Invalid prediction parameters");
            if (error_code) *error_code = COREML_ERROR_INVALID_INPUT;
            return nullptr;
        }

        // Get input feature name
        NSString* featureName;
        if (input_name) {
            featureName = [NSString stringWithUTF8String:input_name];
        } else {
            // Use first input feature name
            MLModelDescription* desc = model->model.modelDescription;
            featureName = [[[desc inputDescriptionsByName] allKeys] firstObject];
        }

        if (!featureName) {
            set_error("Could not determine input feature name");
            if (error_code) *error_code = COREML_ERROR_INVALID_INPUT;
            return nullptr;
        }

        // Create feature provider
        MLFeatureValue* inputValue = [MLFeatureValue featureValueWithMultiArray:input->array];
        NSDictionary* inputDict = @{featureName: inputValue};

        NSError* error = nil;
        id<MLFeatureProvider> inputProvider = [[MLDictionaryFeatureProvider alloc]
                                                initWithDictionary:inputDict
                                                            error:&error];

        if (error) {
            set_error(std::string("Failed to create input provider: ") +
                     [[error localizedDescription] UTF8String]);
            if (error_code) *error_code = COREML_ERROR_INVALID_INPUT;
            return nullptr;
        }

        // Run prediction
        id<MLFeatureProvider> output = [model->model predictionFromFeatures:inputProvider
                                                                      error:&error];

        // ARC handles inputProvider cleanup automatically

        if (error) {
            set_error(std::string("Prediction failed: ") +
                     [[error localizedDescription] UTF8String]);
            if (error_code) *error_code = COREML_ERROR_PREDICTION_FAILED;
            return nullptr;
        }

        if (g_verbose_logging) {
            NSLog(@"[CoreML FFI] Prediction completed successfully");
        }

        if (error_code) *error_code = COREML_SUCCESS;
        return new CoreMLPrediction(output);
    }
}

CoreMLPrediction* coreml_predict_multi(
    CoreMLModel* model,
    CoreMLArray** inputs,
    const char** input_names,
    size_t input_count,
    CoreMLErrorCode* error_code
) {
    @autoreleasepool {
        clear_error();

        if (!model || !model->model || !inputs || !input_names || input_count == 0) {
            set_error("Invalid prediction parameters");
            if (error_code) *error_code = COREML_ERROR_INVALID_INPUT;
            return nullptr;
        }

        // Build input dictionary
        NSMutableDictionary* inputDict = [NSMutableDictionary dictionaryWithCapacity:input_count];

        for (size_t i = 0; i < input_count; i++) {
            if (!inputs[i] || !input_names[i]) {
                set_error("Invalid input at index " + std::to_string(i));
                if (error_code) *error_code = COREML_ERROR_INVALID_INPUT;
                return nullptr;
        }

            NSString* name = [NSString stringWithUTF8String:input_names[i]];
            MLFeatureValue* value = [MLFeatureValue featureValueWithMultiArray:inputs[i]->array];
            inputDict[name] = value;
        }

        NSError* error = nil;
        id<MLFeatureProvider> inputProvider = [[MLDictionaryFeatureProvider alloc]
                                                initWithDictionary:inputDict
                                                            error:&error];

        if (error) {
            set_error(std::string("Failed to create input provider: ") +
                     [[error localizedDescription] UTF8String]);
            if (error_code) *error_code = COREML_ERROR_INVALID_INPUT;
            return nullptr;
        }

        // Run prediction
        id<MLFeatureProvider> output = [model->model predictionFromFeatures:inputProvider
                                                                      error:&error];

        // ARC handles inputProvider cleanup automatically

        if (error) {
            set_error(std::string("Prediction failed: ") +
                     [[error localizedDescription] UTF8String]);
            if (error_code) *error_code = COREML_ERROR_PREDICTION_FAILED;
            return nullptr;
        }

        if (error_code) *error_code = COREML_SUCCESS;
        return new CoreMLPrediction(output);
    }
}

void coreml_prediction_free(CoreMLPrediction* prediction) {
    if (prediction) {
        delete prediction;
    }
}

CoreMLArray* coreml_prediction_get_output(
    CoreMLPrediction* prediction,
    const char* output_name
) {
    if (!prediction || !prediction->features || !output_name) {
        return nullptr;
    }

    @autoreleasepool {
        NSString* name = [NSString stringWithUTF8String:output_name];

        // Check cache first
        std::string key(output_name);
        auto it = prediction->outputs.find(key);
        if (it != prediction->outputs.end()) {
            return it->second;
        }

        // Get feature value
        MLFeatureValue* value = [prediction->features featureValueForName:name];
        if (!value || value.type != MLFeatureTypeMultiArray) {
            return nullptr;
        }

        // Create wrapper and cache
        CoreMLArray* array = new CoreMLArray(value.multiArrayValue);
        prediction->outputs[key] = array;
        return array;
    }
}

size_t coreml_prediction_get_output_count(CoreMLPrediction* prediction) {
    if (!prediction || !prediction->features) {
        return 0;
    }

    @autoreleasepool {
        return [[prediction->features featureNames] count];
    }
}

const char* coreml_prediction_get_output_name(
    CoreMLPrediction* prediction,
    size_t index
) {
    if (!prediction || !prediction->features) {
        return nullptr;
    }

    @autoreleasepool {
        NSSet<NSString*>* names = [prediction->features featureNames];
        if (index >= [names count]) {
            return nullptr;
        }

        NSArray<NSString*>* nameArray = [names allObjects];
        return strdup([nameArray[index] UTF8String]);
    }
}

// ============================================================================
// Error Handling
// ============================================================================

const char* coreml_get_last_error(void) {
    std::lock_guard<std::mutex> lock(g_error_mutex);
    return g_last_error.empty() ? nullptr : g_last_error.c_str();
}

void coreml_clear_error(void) {
    clear_error();
}

// ============================================================================
// Memory Management
// ============================================================================

void coreml_free_string(const char* str) {
    if (str) {
        free((void*)str);
    }
}

void coreml_free_shape(CoreMLShape shape) {
    if (shape.dimensions) {
        free(shape.dimensions);
    }
}

// ============================================================================
// Utilities
// ============================================================================

bool coreml_is_available(void) {
    return YES; // CoreML is available on all modern macOS versions
}

const char* coreml_get_version(void) {
    @autoreleasepool {
        // Return macOS version as proxy for CoreML version
        NSProcessInfo* info = [NSProcessInfo processInfo];
        NSOperatingSystemVersion version = [info operatingSystemVersion];

        static char version_str[64];
        snprintf(version_str, sizeof(version_str), "%ld.%ld.%ld",
                 (long)version.majorVersion,
                 (long)version.minorVersion,
                 (long)version.patchVersion);

        return version_str;
    }
}

void coreml_set_verbose(bool enabled) {
    g_verbose_logging = enabled;
}
