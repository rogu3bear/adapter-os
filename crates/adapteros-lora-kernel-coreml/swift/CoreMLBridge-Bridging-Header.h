//
//  CoreMLBridge-Bridging-Header.h
//  CoreMLSwiftBridge
//
//  Bridging header for C types that Swift needs to see.
//

#ifndef CoreMLBridge_Bridging_Header_h
#define CoreMLBridge_Bridging_Header_h

#include <stdint.h>
#include <stdbool.h>

// FFI result codes
typedef enum {
    CoreMLResultSuccess = 0,
    CoreMLResultErrorModelLoad = 1,
    CoreMLResultErrorInference = 2,
    CoreMLResultErrorInvalidInput = 3,
    CoreMLResultErrorMemory = 4,
    CoreMLResultErrorNotAvailable = 5,
} CoreMLResultCode;

// Opaque handle for CoreML model
typedef struct CoreMLModelHandle CoreMLModelHandle;

// Tensor descriptor for input/output
typedef struct {
    const float *data;
    const int64_t *shape;
    int32_t rank;
} CoreMLTensorDescriptor;

#endif /* CoreMLBridge_Bridging_Header_h */
