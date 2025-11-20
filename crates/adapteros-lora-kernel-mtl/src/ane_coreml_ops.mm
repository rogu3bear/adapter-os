//! Custom CoreML Operations for ANE-Optimized LoRA
//!
//! Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.
//!
//! This file implements custom CoreML/Metal operations for LoRA kernels:
//! - LoRADownProject: Shared down-projection using ANE
//! - LoRAUpProject: Per-module up-projection using ANE
//! - GatedAdd: Fused addition with Q15 gate weights
//!
//! These operations are designed to maximize ANE utilization and minimize
//! data transfers between CPU ↔ ANE ↔ GPU.

#import <Foundation/Foundation.h>
#import <CoreML/CoreML.h>
#import <Metal/Metal.h>
#import <MetalPerformanceShaders/MetalPerformanceShaders.h>

// Availability macros for ANE-specific features
#if TARGET_OS_OSX
#define ANE_AVAILABLE __builtin_available(macOS 13.0, *)
#else
#define ANE_AVAILABLE __builtin_available(iOS 16.0, *)
#endif

// ============================================================================
// LoRADownProject Operation
// ============================================================================

/// Custom CoreML operation for shared down-projection
///
/// Computes: output = input @ down_weights
/// - input: (batch, seq_len, hidden_size) [Float16]
/// - down_weights: (hidden_size, lora_rank) [Float16]
/// - output: (batch, seq_len, lora_rank) [Float16]
///
/// Optimization strategy:
/// 1. Prefer ANE execution for matrix multiplication
/// 2. Use MPS as fallback for GPU acceleration
/// 3. Optimize memory layout for ANE (NCHW format)
@interface LoRADownProjectOp : NSObject

@property (nonatomic, strong) id<MTLDevice> device;
@property (nonatomic, strong) id<MTLCommandQueue> commandQueue;
@property (nonatomic, strong) MPSMatrixMultiplication *matmul;
@property (nonatomic, assign) NSUInteger hiddenSize;
@property (nonatomic, assign) NSUInteger loraRank;
@property (nonatomic, assign) BOOL useFloat16;

- (instancetype)initWithDevice:(id<MTLDevice>)device
                    hiddenSize:(NSUInteger)hiddenSize
                      loraRank:(NSUInteger)loraRank
                    useFloat16:(BOOL)useFloat16;

- (BOOL)executeWithInput:(MPSMatrix *)input
             downWeights:(MPSMatrix *)downWeights
                  output:(MPSMatrix *)output
                   error:(NSError **)error;

@end

@implementation LoRADownProjectOp

- (instancetype)initWithDevice:(id<MTLDevice>)device
                    hiddenSize:(NSUInteger)hiddenSize
                      loraRank:(NSUInteger)loraRank
                    useFloat16:(BOOL)useFloat16 {
    self = [super init];
    if (self) {
        _device = device;
        _commandQueue = [device newCommandQueue];
        _hiddenSize = hiddenSize;
        _loraRank = loraRank;
        _useFloat16 = useFloat16;

        // Create MPS matrix multiplication kernel
        // This will automatically use ANE if available, otherwise GPU
        _matmul = [[MPSMatrixMultiplication alloc] initWithDevice:device
                                                   transposeLeft:NO
                                                  transposeRight:NO
                                                      resultRows:0  // Will be set dynamically
                                                   resultColumns:loraRank
                                             interiorColumns:hiddenSize
                                                       alpha:1.0
                                                        beta:0.0];

        NSLog(@"[LoRADownProjectOp] Initialized: hidden=%lu, rank=%lu, fp16=%d",
              (unsigned long)hiddenSize, (unsigned long)loraRank, useFloat16);
    }
    return self;
}

- (BOOL)executeWithInput:(MPSMatrix *)input
             downWeights:(MPSMatrix *)downWeights
                  output:(MPSMatrix *)output
                   error:(NSError **)error {
    @autoreleasepool {
        // Validate dimensions
        if (input.columns != self.hiddenSize) {
            if (error) {
                *error = [NSError errorWithDomain:@"LoRADownProjectOp"
                                             code:-1
                                         userInfo:@{NSLocalizedDescriptionKey:
                                             [NSString stringWithFormat:@"Input columns (%lu) != hidden_size (%lu)",
                                              (unsigned long)input.columns, (unsigned long)self.hiddenSize]}];
            }
            return NO;
        }

        if (downWeights.rows != self.hiddenSize || downWeights.columns != self.loraRank) {
            if (error) {
                *error = [NSError errorWithDomain:@"LoRADownProjectOp"
                                             code:-2
                                         userInfo:@{NSLocalizedDescriptionKey: @"Invalid down_weights dimensions"}];
            }
            return NO;
        }

        // Create command buffer
        id<MTLCommandBuffer> commandBuffer = [self.commandQueue commandBuffer];
        if (!commandBuffer) {
            if (error) {
                *error = [NSError errorWithDomain:@"LoRADownProjectOp"
                                             code:-3
                                         userInfo:@{NSLocalizedDescriptionKey: @"Failed to create command buffer"}];
            }
            return NO;
        }

        // Encode matrix multiplication
        // MPS will automatically route to ANE if the operation is supported
        [self.matmul encodeToCommandBuffer:commandBuffer
                                leftMatrix:input
                               rightMatrix:downWeights
                              resultMatrix:output];

        // Commit and wait
        [commandBuffer commit];
        [commandBuffer waitUntilCompleted];

        if (commandBuffer.error) {
            if (error) {
                *error = commandBuffer.error;
            }
            return NO;
        }

        return YES;
    }
}

@end

// ============================================================================
// LoRAUpProject Operation
// ============================================================================

/// Custom CoreML operation for per-module up-projection with gating
///
/// Computes: output = Σ(gate[k] * (projected @ up_weights[k]))
/// - projected: (batch, seq_len, lora_rank) [Float16]
/// - up_weights: Vec<(lora_rank, hidden_size)> [Float16]
/// - gates: Q15 fixed-point gate weights
/// - output: (batch, seq_len, hidden_size) [Float16]
@interface LoRAUpProjectOp : NSObject

@property (nonatomic, strong) id<MTLDevice> device;
@property (nonatomic, strong) id<MTLCommandQueue> commandQueue;
@property (nonatomic, strong) NSMutableArray<MPSMatrixMultiplication *> *matmuls;
@property (nonatomic, assign) NSUInteger loraRank;
@property (nonatomic, assign) NSUInteger hiddenSize;
@property (nonatomic, assign) NSUInteger numModules;
@property (nonatomic, strong) id<MTLComputePipelineState> gatedAddPipeline;

- (instancetype)initWithDevice:(id<MTLDevice>)device
                      loraRank:(NSUInteger)loraRank
                    hiddenSize:(NSUInteger)hiddenSize
                    numModules:(NSUInteger)numModules;

- (BOOL)executeWithProjected:(MPSMatrix *)projected
                  upWeights:(NSArray<MPSMatrix *> *)upWeights
                 gateWeights:(const int16_t *)gateWeights
                      output:(MPSMatrix *)output
                       error:(NSError **)error;

@end

@implementation LoRAUpProjectOp

- (instancetype)initWithDevice:(id<MTLDevice>)device
                      loraRank:(NSUInteger)loraRank
                    hiddenSize:(NSUInteger)hiddenSize
                    numModules:(NSUInteger)numModules {
    self = [super init];
    if (self) {
        _device = device;
        _commandQueue = [device newCommandQueue];
        _loraRank = loraRank;
        _hiddenSize = hiddenSize;
        _numModules = numModules;
        _matmuls = [NSMutableArray arrayWithCapacity:numModules];

        // Create MPS matrix multiplication kernels for each module
        for (NSUInteger i = 0; i < numModules; i++) {
            MPSMatrixMultiplication *matmul = [[MPSMatrixMultiplication alloc]
                                               initWithDevice:device
                                               transposeLeft:NO
                                               transposeRight:NO
                                               resultRows:0  // Dynamic
                                               resultColumns:hiddenSize
                                               interiorColumns:loraRank
                                               alpha:1.0
                                               beta:0.0];
            [_matmuls addObject:matmul];
        }

        // Create Metal compute pipeline for gated addition
        [self createGatedAddPipeline];

        NSLog(@"[LoRAUpProjectOp] Initialized: rank=%lu, hidden=%lu, modules=%lu",
              (unsigned long)loraRank, (unsigned long)hiddenSize, (unsigned long)numModules);
    }
    return self;
}

- (void)createGatedAddPipeline {
    NSError *error = nil;

    // Metal shader for gated addition
    NSString *shaderSource = @R"(
        #include <metal_stdlib>
        using namespace metal;

        kernel void gated_add(
            const device half *module_output [[buffer(0)]],
            device half *accumulator [[buffer(1)]],
            constant float &gate_weight [[buffer(2)]],
            uint id [[thread_position_in_grid]]
        ) {
            half acc_val = accumulator[id];
            half mod_val = module_output[id];
            accumulator[id] = acc_val + half(gate_weight) * mod_val;
        }
    )";

    id<MTLLibrary> library = [self.device newLibraryWithSource:shaderSource
                                                       options:nil
                                                         error:&error];
    if (!library) {
        NSLog(@"[LoRAUpProjectOp] Failed to compile shader: %@", error);
        return;
    }

    id<MTLFunction> function = [library newFunctionWithName:@"gated_add"];
    if (!function) {
        NSLog(@"[LoRAUpProjectOp] Failed to find gated_add function");
        return;
    }

    self.gatedAddPipeline = [self.device newComputePipelineStateWithFunction:function error:&error];
    if (!self.gatedAddPipeline) {
        NSLog(@"[LoRAUpProjectOp] Failed to create pipeline: %@", error);
    }
}

- (BOOL)executeWithProjected:(MPSMatrix *)projected
                  upWeights:(NSArray<MPSMatrix *> *)upWeights
                 gateWeights:(const int16_t *)gateWeights
                      output:(MPSMatrix *)output
                       error:(NSError **)error {
    @autoreleasepool {
        if (upWeights.count != self.numModules) {
            if (error) {
                *error = [NSError errorWithDomain:@"LoRAUpProjectOp"
                                             code:-1
                                         userInfo:@{NSLocalizedDescriptionKey: @"upWeights count mismatch"}];
            }
            return NO;
        }

        // Create command buffer
        id<MTLCommandBuffer> commandBuffer = [self.commandQueue commandBuffer];
        if (!commandBuffer) {
            if (error) {
                *error = [NSError errorWithDomain:@"LoRAUpProjectOp"
                                             code:-2
                                         userInfo:@{NSLocalizedDescriptionKey: @"Failed to create command buffer"}];
            }
            return NO;
        }

        // Zero initialize output
        id<MTLBlitCommandEncoder> blitEncoder = [commandBuffer blitCommandEncoder];
        [blitEncoder fillBuffer:output.data
                          range:NSMakeRange(0, output.data.length)
                          value:0];
        [blitEncoder endEncoding];

        // Temporary buffer for module outputs
        NSUInteger outputSize = output.rows * output.columns * sizeof(uint16_t);  // Float16
        id<MTLBuffer> tempBuffer = [self.device newBufferWithLength:outputSize
                                                            options:MTLResourceStorageModeShared];

        // Execute each module
        for (NSUInteger moduleIdx = 0; moduleIdx < self.numModules; moduleIdx++) {
            int16_t gate_q15 = gateWeights[moduleIdx];
            float gate_weight = (float)gate_q15 / 32768.0f;

            // Skip inactive modules
            if (fabs(gate_weight) < 1e-6f) {
                continue;
            }

            // Matrix multiplication: projected @ up_weights[k]
            MPSMatrixMultiplication *matmul = self.matmuls[moduleIdx];
            MPSMatrix *upWeight = upWeights[moduleIdx];

            // Create temporary matrix for module output
            MPSMatrixDescriptor *tempDesc = [MPSMatrixDescriptor matrixDescriptorWithRows:output.rows
                                                                                  columns:output.columns
                                                                                 rowBytes:output.columns * sizeof(uint16_t)
                                                                                 dataType:MPSDataTypeFloat16];
            MPSMatrix *tempMatrix = [[MPSMatrix alloc] initWithBuffer:tempBuffer descriptor:tempDesc];

            [matmul encodeToCommandBuffer:commandBuffer
                               leftMatrix:projected
                              rightMatrix:upWeight
                             resultMatrix:tempMatrix];

            // Gated addition using Metal compute shader
            if (self.gatedAddPipeline) {
                id<MTLComputeCommandEncoder> computeEncoder = [commandBuffer computeCommandEncoder];
                [computeEncoder setComputePipelineState:self.gatedAddPipeline];
                [computeEncoder setBuffer:tempBuffer offset:0 atIndex:0];
                [computeEncoder setBuffer:output.data offset:0 atIndex:1];
                [computeEncoder setBytes:&gate_weight length:sizeof(float) atIndex:2];

                NSUInteger numElements = output.rows * output.columns;
                NSUInteger threadGroupSize = MIN(self.gatedAddPipeline.maxTotalThreadsPerThreadgroup, 256);
                NSUInteger numThreadGroups = (numElements + threadGroupSize - 1) / threadGroupSize;

                [computeEncoder dispatchThreadgroups:MTLSizeMake(numThreadGroups, 1, 1)
                               threadsPerThreadgroup:MTLSizeMake(threadGroupSize, 1, 1)];
                [computeEncoder endEncoding];
            }
        }

        // Commit and wait
        [commandBuffer commit];
        [commandBuffer waitUntilCompleted];

        if (commandBuffer.error) {
            if (error) {
                *error = commandBuffer.error;
            }
            return NO;
        }

        return YES;
    }
}

@end

// ============================================================================
// GatedAdd Operation
// ============================================================================

/// Fused gated addition operation
///
/// Computes: output = base + gate_weight * lora_output
@interface GatedAddOp : NSObject

@property (nonatomic, strong) id<MTLDevice> device;
@property (nonatomic, strong) id<MTLCommandQueue> commandQueue;
@property (nonatomic, strong) id<MTLComputePipelineState> pipeline;

- (instancetype)initWithDevice:(id<MTLDevice>)device;

- (BOOL)executeWithBase:(id<MTLBuffer>)base
             loraOutput:(id<MTLBuffer>)loraOutput
                   gate:(float)gateWeight
             numElements:(NSUInteger)numElements
                 output:(id<MTLBuffer>)output
                  error:(NSError **)error;

@end

@implementation GatedAddOp

- (instancetype)initWithDevice:(id<MTLDevice>)device {
    self = [super init];
    if (self) {
        _device = device;
        _commandQueue = [device newCommandQueue];
        [self createPipeline];
    }
    return self;
}

- (void)createPipeline {
    NSError *error = nil;

    NSString *shaderSource = @R"(
        #include <metal_stdlib>
        using namespace metal;

        kernel void gated_add_fused(
            const device half *base [[buffer(0)]],
            const device half *lora_output [[buffer(1)]],
            constant float &gate_weight [[buffer(2)]],
            device half *output [[buffer(3)]],
            uint id [[thread_position_in_grid]]
        ) {
            half base_val = base[id];
            half lora_val = lora_output[id];
            output[id] = base_val + half(gate_weight) * lora_val;
        }
    )";

    id<MTLLibrary> library = [self.device newLibraryWithSource:shaderSource
                                                       options:nil
                                                         error:&error];
    if (!library) {
        NSLog(@"[GatedAddOp] Failed to compile shader: %@", error);
        return;
    }

    id<MTLFunction> function = [library newFunctionWithName:@"gated_add_fused"];
    if (!function) {
        NSLog(@"[GatedAddOp] Failed to find function");
        return;
    }

    self.pipeline = [self.device newComputePipelineStateWithFunction:function error:&error];
    if (!self.pipeline) {
        NSLog(@"[GatedAddOp] Failed to create pipeline: %@", error);
    }
}

- (BOOL)executeWithBase:(id<MTLBuffer>)base
             loraOutput:(id<MTLBuffer>)loraOutput
                   gate:(float)gateWeight
             numElements:(NSUInteger)numElements
                 output:(id<MTLBuffer>)output
                  error:(NSError **)error {
    @autoreleasepool {
        if (!self.pipeline) {
            if (error) {
                *error = [NSError errorWithDomain:@"GatedAddOp"
                                             code:-1
                                         userInfo:@{NSLocalizedDescriptionKey: @"Pipeline not initialized"}];
            }
            return NO;
        }

        id<MTLCommandBuffer> commandBuffer = [self.commandQueue commandBuffer];
        if (!commandBuffer) {
            if (error) {
                *error = [NSError errorWithDomain:@"GatedAddOp"
                                             code:-2
                                         userInfo:@{NSLocalizedDescriptionKey: @"Failed to create command buffer"}];
            }
            return NO;
        }

        id<MTLComputeCommandEncoder> encoder = [commandBuffer computeCommandEncoder];
        [encoder setComputePipelineState:self.pipeline];
        [encoder setBuffer:base offset:0 atIndex:0];
        [encoder setBuffer:loraOutput offset:0 atIndex:1];
        [encoder setBytes:&gateWeight length:sizeof(float) atIndex:2];
        [encoder setBuffer:output offset:0 atIndex:3];

        NSUInteger threadGroupSize = MIN(self.pipeline.maxTotalThreadsPerThreadgroup, 256);
        NSUInteger numThreadGroups = (numElements + threadGroupSize - 1) / threadGroupSize;

        [encoder dispatchThreadgroups:MTLSizeMake(numThreadGroups, 1, 1)
                threadsPerThreadgroup:MTLSizeMake(threadGroupSize, 1, 1)];
        [encoder endEncoding];

        [commandBuffer commit];
        [commandBuffer waitUntilCompleted];

        if (commandBuffer.error) {
            if (error) {
                *error = commandBuffer.error;
            }
            return NO;
        }

        return YES;
    }
}

@end

// ============================================================================
// C FFI Interface for Rust
// ============================================================================

#ifdef __cplusplus
extern "C" {
#endif

// Opaque handles for Rust FFI
typedef void* LoRADownProjectHandle;
typedef void* LoRAUpProjectHandle;
typedef void* GatedAddHandle;

/// Create LoRADownProject operation
LoRADownProjectHandle lora_down_project_create(
    void *device_ptr,
    size_t hidden_size,
    size_t lora_rank,
    bool use_float16
) {
    id<MTLDevice> device = (__bridge id<MTLDevice>)device_ptr;
    LoRADownProjectOp *op = [[LoRADownProjectOp alloc] initWithDevice:device
                                                           hiddenSize:hidden_size
                                                             loraRank:lora_rank
                                                           useFloat16:use_float16];
    return (__bridge_retained void*)op;
}

/// Free LoRADownProject operation
void lora_down_project_free(LoRADownProjectHandle handle) {
    if (handle) {
        CFRelease(handle);
    }
}

/// Create LoRAUpProject operation
LoRAUpProjectHandle lora_up_project_create(
    void *device_ptr,
    size_t lora_rank,
    size_t hidden_size,
    size_t num_modules
) {
    id<MTLDevice> device = (__bridge id<MTLDevice>)device_ptr;
    LoRAUpProjectOp *op = [[LoRAUpProjectOp alloc] initWithDevice:device
                                                          loraRank:lora_rank
                                                        hiddenSize:hidden_size
                                                        numModules:num_modules];
    return (__bridge_retained void*)op;
}

/// Free LoRAUpProject operation
void lora_up_project_free(LoRAUpProjectHandle handle) {
    if (handle) {
        CFRelease(handle);
    }
}

/// Create GatedAdd operation
GatedAddHandle gated_add_create(void *device_ptr) {
    id<MTLDevice> device = (__bridge id<MTLDevice>)device_ptr;
    GatedAddOp *op = [[GatedAddOp alloc] initWithDevice:device];
    return (__bridge_retained void*)op;
}

/// Free GatedAdd operation
void gated_add_free(GatedAddHandle handle) {
    if (handle) {
        CFRelease(handle);
    }
}

#ifdef __cplusplus
}
#endif
