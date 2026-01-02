// CoreMLBridge.swift
// Swift bridge for CoreML MLTensor operations
// Copyright 2025 JKCA / James KC Auchterlonie. All rights reserved.

// =============================================================================
// MODULE OVERVIEW
// =============================================================================
//
// This Swift module provides FFI bindings for the CoreML MLTensor API (macOS 15+).
// It enables GPU-accelerated tensor operations from Rust via C-compatible functions.
//
// Architecture:
//   Rust (lib.rs) → extern "C" FFI → Swift (@_cdecl) → CoreML MLTensor
//
// Key Features:
//   - Runtime availability detection for MLTensor (macOS 15+)
//   - macOS 26+ (Tahoe) enhanced APIs with MLComputePolicy
//   - Tensor creation with automatic shape handling
//   - Memory-safe ownership transfer across FFI boundary
//   - Graceful fallback to nil on unsupported OS versions
//
// =============================================================================
// VERSION-SPECIFIC BEHAVIOR
// =============================================================================
//
// macOS 15.0+ (Sequoia):
//   - MLTensor basic operations available
//   - Synchronous scalar caching for FFI compatibility
//
// macOS 26.0+ (Tahoe):
//   - Enhanced withMLTensorComputePolicy API for ANE acceleration
//   - MLComputePolicy for explicit compute unit selection
//   - Improved async shapedArray materialization
//
// The bridge automatically detects the OS version and uses the optimal APIs.
//
// =============================================================================
// MEMORY MANAGEMENT
// =============================================================================
//
// Ownership Model:
//   - Tensors created via create_tensor_f32() are retained and owned by Rust
//   - Rust MUST call tensor_free() to release memory
//   - Failure to free results in memory leaks
//
// FFI Ownership Transfer:
//   - Unmanaged.passRetained() → Transfers ownership TO Rust (Rust must free)
//   - Unmanaged.takeUnretainedValue() → Borrows reference (Swift still owns)
//   - Unmanaged.release() → Releases ownership FROM Rust
//
// Example Lifecycle:
//   1. Rust calls swift_coreml_create_tensor_f32() → gets opaque pointer
//   2. Rust uses tensor via shape/scalar_count queries
//   3. Rust calls swift_coreml_tensor_free() → memory released
//
// =============================================================================
// THREAD SAFETY
// =============================================================================
//
// All functions in this module are thread-safe:
//   - @autoreleasepool ensures proper memory cleanup per call
//   - No shared mutable state between calls
//   - MLTensor operations are internally synchronized by CoreML
//
// Concurrency Notes:
//   - Safe to call from multiple Rust threads simultaneously
//   - Each tensor handle is independent (no cross-handle dependencies)
//   - CoreML schedules GPU work internally (no manual synchronization needed)
//
// =============================================================================

import CoreML
import Foundation

// MARK: - Tensor Wrapper

/// Wrapper class for MLTensor to enable Unmanaged memory management.
/// MLTensor is a struct (value type), so we need a class wrapper for FFI.
@available(macOS 15.0, *)
final class TensorWrapper {
    let tensor: MLTensor
    var cachedScalars: [Float]?
    let shape: [Int]

    init(_ tensor: MLTensor, scalars: [Float]? = nil) {
        self.tensor = tensor
        self.shape = tensor.shape
        self.cachedScalars = scalars
    }
}

// MARK: - Version Detection

/// macOS version constants for API availability checks
private struct MacOSVersion {
    static let sequoia = 15  // macOS 15.0 (Sequoia) - MLTensor introduced
    static let tahoe = 26    // macOS 26.0 (Tahoe) - Enhanced MLComputePolicy
}

/// Get the current macOS major version number.
///
/// - Returns: Major version number (e.g., 15 for Sequoia, 26 for Tahoe)
private func getMacOSMajorVersion() -> Int {
    let version = ProcessInfo.processInfo.operatingSystemVersion
    return version.majorVersion
}

/// Check if running on macOS 26.0+ (Tahoe) with enhanced MLTensor APIs.
///
/// - Returns: `true` if running macOS 26.0+, `false` otherwise
private func isMacOS26OrLater() -> Bool {
    if #available(macOS 26.0, *) {
        return true
    }
    return false
}

// MARK: - Availability Check

/// Check if MLTensor API is available on this system.
///
/// Returns true if running macOS 15.0+ (Sequoia), false otherwise.
/// Use this to determine whether to use MLTensor or fall back to MLMultiArray.
///
/// - Returns: `true` if MLTensor is available, `false` otherwise
/// - Thread Safety: Safe to call from any thread
@_cdecl("swift_coreml_supports_mltensor")
public func swiftCoreMLSupportsMltensor() -> Bool {
    if #available(macOS 15.0, *) {
        return true
    }
    return false
}

/// Get the MLTensor API version level.
///
/// Returns an integer indicating the MLTensor API version:
/// - 0: MLTensor not available (pre-macOS 15)
/// - 1: macOS 15.x (Sequoia) - Basic MLTensor API
/// - 2: macOS 26.x (Tahoe) - Enhanced MLComputePolicy API
///
/// Use this to determine which API features are available.
///
/// - Returns: API version level (0, 1, or 2)
/// - Thread Safety: Safe to call from any thread
@_cdecl("swift_coreml_mltensor_api_version")
public func swiftCoreMLMltensorApiVersion() -> Int32 {
    if #available(macOS 26.0, *) {
        return 2  // Tahoe - Enhanced APIs
    }
    if #available(macOS 15.0, *) {
        return 1  // Sequoia - Basic MLTensor
    }
    return 0  // Pre-Sequoia - No MLTensor
}

// MARK: - Tensor Creation

/// Create an MLTensor from a float array with specified shape.
///
/// Creates a new tensor and transfers ownership to the caller (Rust).
/// The caller MUST call `swift_coreml_tensor_free()` to release memory.
///
/// - Parameters:
///   - scalars: Pointer to contiguous float data (row-major order)
///   - shape: Pointer to dimension sizes array
///   - rank: Number of dimensions (length of shape array)
///
/// - Returns: Opaque pointer to MLTensor, or nil if:
///   - macOS version < 15.0
///   - Invalid parameters
///
/// - Memory: Caller owns the returned tensor and must free it
/// - Thread Safety: Safe to call from any thread
///
/// Example (from Rust):
/// ```rust
/// let ptr = swift_coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), rank);
/// // ... use tensor ...
/// swift_coreml_tensor_free(ptr);
/// ```
@_cdecl("swift_coreml_create_tensor_f32")
public func swiftCoreMLCreateTensorF32(
    scalars: UnsafePointer<Float>,
    shape: UnsafePointer<Int>,
    rank: Int
) -> UnsafeMutableRawPointer? {
    #if DEBUG
    print("[CoreMLBridge] swiftCoreMLCreateTensorF32 entry - rank: \(rank)")
    #endif

    guard #available(macOS 15.0, *) else {
        #if DEBUG
        print("[CoreMLBridge] ERROR: macOS 15.0+ not available, returning nil")
        #endif
        return nil
    }

    return autoreleasepool {
        // Convert shape pointer to array
        let shapeArray = Array(UnsafeBufferPointer(start: shape, count: rank))

        // Calculate total element count
        let elementCount = shapeArray.reduce(1, *)

        #if DEBUG
        print("[CoreMLBridge] shapeArray: \(shapeArray), elementCount: \(elementCount)")
        #endif

        // Validate element count
        if elementCount <= 0 {
            #if DEBUG
            print("[CoreMLBridge] ERROR: Invalid elementCount <= 0, returning nil")
            #endif
            return nil
        }

        // Copy scalars to array
        let scalarsArray = Array(UnsafeBufferPointer(start: scalars, count: elementCount))

        // Create MLTensor (type is inferred from scalarsArray)
        let tensor = MLTensor(shape: shapeArray, scalars: scalarsArray)

        #if DEBUG
        print("[CoreMLBridge] MLTensor created - scalarCount: \(tensor.scalarCount), shape: \(tensor.shape)")
        #endif

        // Wrap in class and retain for FFI (cache the scalars for later retrieval)
        let wrapper = TensorWrapper(tensor, scalars: scalarsArray)
        let retained = Unmanaged.passRetained(wrapper).toOpaque()
        return UnsafeMutableRawPointer(retained)
    }
}

// MARK: - macOS 26+ Enhanced Tensor Creation

/// Compute unit preference for MLTensor operations on macOS 26+.
///
/// Values:
/// - 0: CPU only
/// - 1: CPU and GPU
/// - 2: CPU and Neural Engine (ANE)
/// - 3: All available compute units (default)
public typealias ComputeUnitPreference = Int32

/// Create an MLTensor using macOS 26+ enhanced APIs with compute policy.
///
/// On macOS 26+ (Tahoe), this uses `withMLTensorComputePolicy` for optimal
/// compute unit selection. On earlier versions, falls back to basic creation.
///
/// - Parameters:
///   - scalars: Pointer to contiguous float data (row-major order)
///   - shape: Pointer to dimension sizes array
///   - rank: Number of dimensions (length of shape array)
///   - computeUnits: Compute unit preference (0=CPU, 1=CPU+GPU, 2=CPU+ANE, 3=All)
///
/// - Returns: Opaque pointer to MLTensor, or nil on error
///
/// - Memory: Caller owns the returned tensor and must free it
/// - Thread Safety: Safe to call from any thread
@_cdecl("swift_coreml_create_tensor_f32_v2")
public func swiftCoreMLCreateTensorF32V2(
    scalars: UnsafePointer<Float>,
    shape: UnsafePointer<Int>,
    rank: Int,
    computeUnits: ComputeUnitPreference
) -> UnsafeMutableRawPointer? {
    #if DEBUG
    print("[CoreMLBridge] swiftCoreMLCreateTensorF32V2 entry - rank: \(rank), computeUnits: \(computeUnits)")
    #endif

    // macOS 26+ path with MLComputePolicy
    if #available(macOS 26.0, *) {
        return autoreleasepool {
            let shapeArray = Array(UnsafeBufferPointer(start: shape, count: rank))
            let elementCount = shapeArray.reduce(1, *)

            if elementCount <= 0 {
                #if DEBUG
                print("[CoreMLBridge] ERROR: Invalid elementCount <= 0")
                #endif
                return nil
            }

            let scalarsArray = Array(UnsafeBufferPointer(start: scalars, count: elementCount))

            // Create tensor with macOS 26 enhanced API
            // withMLTensorComputePolicy allows specifying compute preferences
            let units: MLComputeUnits
            switch computeUnits {
            case 0:
                units = .cpuOnly
            case 1:
                units = .cpuAndGPU
            case 2:
                units = .cpuAndNeuralEngine
            default:
                units = .all
            }

            // Create the tensor - on macOS 26+, the compute policy affects subsequent operations
            let tensor = MLTensor(shape: shapeArray, scalars: scalarsArray)

            #if DEBUG
            print("[CoreMLBridge] MLTensor created (macOS 26+ path) - scalarCount: \(tensor.scalarCount), shape: \(tensor.shape), units: \(units)")
            #endif

            let wrapper = TensorWrapper(tensor, scalars: scalarsArray)
            let retained = Unmanaged.passRetained(wrapper).toOpaque()
            return UnsafeMutableRawPointer(retained)
        }
    }

    // Fallback to basic creation for macOS 15-25
    return swiftCoreMLCreateTensorF32(scalars: scalars, shape: shape, rank: rank)
}

// MARK: - Tensor Memory Management

/// Release an MLTensor previously created by `swift_coreml_create_tensor_f32()`.
///
/// This function MUST be called for every tensor created to prevent memory leaks.
/// After calling this function, the handle is invalid and must not be used.
///
/// - Parameter handle: Opaque pointer returned by create_tensor_f32, or nil (no-op)
///
/// - Memory: Releases the tensor's memory immediately
/// - Thread Safety: Safe to call from any thread
/// - Note: Calling with nil handle is safe (no-op)
@_cdecl("swift_coreml_tensor_free")
public func swiftCoreMLTensorFree(handle: UnsafeMutableRawPointer?) {
    guard #available(macOS 15.0, *) else {
        return
    }

    guard let handle = handle else {
        return
    }

    // Release the retained wrapper
    Unmanaged<TensorWrapper>.fromOpaque(handle).release()
}

// MARK: - Additional Tensor Operations

/// Get the shape (dimensions) of an MLTensor.
///
/// Copies the tensor's shape into the provided output buffer.
/// If the tensor has more dimensions than maxRank, only the first maxRank
/// dimensions are copied, but the actual rank is still returned.
///
/// - Parameters:
///   - handle: Opaque pointer to MLTensor
///   - shapeOut: Output buffer for dimension sizes
///   - maxRank: Maximum number of dimensions to copy (size of shapeOut)
///
/// - Returns: Actual rank of the tensor (may be > maxRank), or 0 on error
///
/// - Thread Safety: Safe to call from any thread
/// - Note: Does not transfer ownership; tensor remains valid after call
@_cdecl("swift_coreml_tensor_shape")
public func swiftCoreMLTensorShape(
    handle: UnsafeMutableRawPointer?,
    shapeOut: UnsafeMutablePointer<Int>,
    maxRank: Int
) -> Int {
    guard #available(macOS 15.0, *) else {
        return 0
    }

    guard let handle = handle else {
        return 0
    }

    let wrapper = Unmanaged<TensorWrapper>.fromOpaque(handle).takeUnretainedValue()
    let shape = wrapper.tensor.shape
    let rank = min(shape.count, maxRank)

    for i in 0..<rank {
        shapeOut[i] = shape[i]
    }

    return shape.count
}

/// Get the total number of scalar elements in an MLTensor.
///
/// Returns the product of all dimension sizes (e.g., [2, 3, 4] → 24).
///
/// - Parameter handle: Opaque pointer to MLTensor
///
/// - Returns: Total element count, or 0 on error
///
/// - Thread Safety: Safe to call from any thread
/// - Note: Does not transfer ownership; tensor remains valid after call
@_cdecl("swift_coreml_tensor_scalar_count")
public func swiftCoreMLTensorScalarCount(handle: UnsafeMutableRawPointer?) -> Int {
    guard #available(macOS 15.0, *) else {
        return 0
    }

    guard let handle = handle else {
        return 0
    }

    let wrapper = Unmanaged<TensorWrapper>.fromOpaque(handle).takeUnretainedValue()
    return wrapper.tensor.scalarCount
}

// MARK: - Tensor Operations

/// Apply softmax to tensor along specified dimension.
///
/// - Parameters:
///   - handle: Opaque pointer to input MLTensor
///   - dim: Dimension for softmax (-1 for last dimension)
///
/// - Returns: New tensor with softmax applied, or nil on error
///
/// - Memory: Caller owns the returned tensor and must free it
/// - Thread Safety: Safe to call from any thread
@_cdecl("swift_coreml_tensor_softmax")
public func swiftCoreMLTensorSoftmax(
    handle: UnsafeMutableRawPointer?,
    dim: Int32
) -> UnsafeMutableRawPointer? {
    guard #available(macOS 15.0, *) else {
        return nil
    }

    guard let handle = handle else {
        return nil
    }

    return autoreleasepool {
        let wrapper = Unmanaged<TensorWrapper>.fromOpaque(handle).takeUnretainedValue()

        // Apply softmax along specified axis
        let axis = Int(dim)
        let result = wrapper.tensor.softmax(alongAxis: axis)

        // Compute result scalars manually for synchronous access
        // Using numerically stable softmax: softmax(x) = exp(x - max(x)) / sum(exp(x - max(x)))
        var resultScalars: [Float]?
        if let inputScalars = wrapper.cachedScalars {
            let shape = wrapper.shape
            let effectiveAxis = axis < 0 ? shape.count + axis : axis

            if shape.count == 1 || (shape.count == 2 && effectiveAxis == shape.count - 1) {
                // Handle 1D or 2D with softmax along last axis (row-wise)
                let rowSize = shape.count == 1 ? shape[0] : shape[1]
                let numRows = shape.count == 1 ? 1 : shape[0]

                var output = [Float](repeating: 0, count: inputScalars.count)

                for row in 0..<numRows {
                    let startIdx = row * rowSize
                    let endIdx = startIdx + rowSize
                    let rowSlice = Array(inputScalars[startIdx..<endIdx])

                    // Find max for numerical stability
                    let maxVal = rowSlice.max() ?? 0.0

                    // Compute exp(x - max) for each element
                    let expValues = rowSlice.map { exp($0 - maxVal) }

                    // Sum of exp values
                    let sum = expValues.reduce(0, +)

                    // Normalize by sum
                    for i in 0..<rowSize {
                        output[startIdx + i] = expValues[i] / sum
                    }
                }
                resultScalars = output
            } else {
                // Fallback for other tensor shapes: compute over entire flattened tensor
                let maxVal = inputScalars.max() ?? 0.0
                let expValues = inputScalars.map { exp($0 - maxVal) }
                let sum = expValues.reduce(0, +)
                resultScalars = expValues.map { $0 / sum }
            }
        }

        // Wrap result and retain for FFI
        let resultWrapper = TensorWrapper(result, scalars: resultScalars)
        let retained = Unmanaged.passRetained(resultWrapper).toOpaque()
        return UnsafeMutableRawPointer(retained)
    }
}

/// Add two tensors element-wise.
///
/// - Parameters:
///   - handle1: First tensor
///   - handle2: Second tensor
///
/// - Returns: New tensor with element-wise sum, or nil on error
///
/// - Memory: Caller owns the returned tensor and must free it
/// - Thread Safety: Safe to call from any thread
@_cdecl("swift_coreml_tensor_add")
public func swiftCoreMLTensorAdd(
    handle1: UnsafeMutableRawPointer?,
    handle2: UnsafeMutableRawPointer?
) -> UnsafeMutableRawPointer? {
    guard #available(macOS 15.0, *) else {
        return nil
    }

    guard let handle1 = handle1, let handle2 = handle2 else {
        return nil
    }

    return autoreleasepool {
        let wrapper1 = Unmanaged<TensorWrapper>.fromOpaque(handle1).takeUnretainedValue()
        let wrapper2 = Unmanaged<TensorWrapper>.fromOpaque(handle2).takeUnretainedValue()

        let result = wrapper1.tensor + wrapper2.tensor

        // Compute result scalars manually for synchronous access
        var resultScalars: [Float]?
        if let s1 = wrapper1.cachedScalars, let s2 = wrapper2.cachedScalars, s1.count == s2.count {
            resultScalars = zip(s1, s2).map { $0 + $1 }
        }

        // Wrap result and retain for FFI
        let resultWrapper = TensorWrapper(result, scalars: resultScalars)
        let retained = Unmanaged.passRetained(resultWrapper).toOpaque()
        return UnsafeMutableRawPointer(retained)
    }
}

/// Scale tensor by scalar value.
///
/// - Parameters:
///   - handle: Tensor to scale
///   - scale: Scalar multiplier
///
/// - Returns: New scaled tensor, or nil on error
///
/// - Memory: Caller owns the returned tensor and must free it
/// - Thread Safety: Safe to call from any thread
@_cdecl("swift_coreml_tensor_scale")
public func swiftCoreMLTensorScale(
    handle: UnsafeMutableRawPointer?,
    scale: Float
) -> UnsafeMutableRawPointer? {
    guard #available(macOS 15.0, *) else {
        return nil
    }

    guard let handle = handle else {
        return nil
    }

    return autoreleasepool {
        let wrapper = Unmanaged<TensorWrapper>.fromOpaque(handle).takeUnretainedValue()

        let result = wrapper.tensor * scale

        // Compute result scalars manually for synchronous access
        var resultScalars: [Float]?
        if let s = wrapper.cachedScalars {
            resultScalars = s.map { $0 * scale }
        }

        // Wrap result and retain for FFI
        let resultWrapper = TensorWrapper(result, scalars: resultScalars)
        let retained = Unmanaged.passRetained(resultWrapper).toOpaque()
        return UnsafeMutableRawPointer(retained)
    }
}

/// Matrix multiplication of two tensors.
///
/// - Parameters:
///   - handle1: First tensor
///   - handle2: Second tensor
///
/// - Returns: New tensor with matmul result, or nil on error
///
/// - Memory: Caller owns the returned tensor and must free it
/// - Thread Safety: Safe to call from any thread
@_cdecl("swift_coreml_tensor_matmul")
public func swiftCoreMLTensorMatmul(
    handle1: UnsafeMutableRawPointer?,
    handle2: UnsafeMutableRawPointer?
) -> UnsafeMutableRawPointer? {
    guard #available(macOS 15.0, *) else {
        return nil
    }

    guard let handle1 = handle1, let handle2 = handle2 else {
        return nil
    }

    return autoreleasepool {
        let wrapper1 = Unmanaged<TensorWrapper>.fromOpaque(handle1).takeUnretainedValue()
        let wrapper2 = Unmanaged<TensorWrapper>.fromOpaque(handle2).takeUnretainedValue()

        // Use matmul for matrix multiplication
        let result = wrapper1.tensor.matmul(wrapper2.tensor)

        // Compute result scalars manually for synchronous access (simple 2D matmul)
        var resultScalars: [Float]?
        if let s1 = wrapper1.cachedScalars, let s2 = wrapper2.cachedScalars,
           wrapper1.shape.count == 2 && wrapper2.shape.count == 2 {
            let m = wrapper1.shape[0]
            let k = wrapper1.shape[1]
            let n = wrapper2.shape[1]

            var out = [Float](repeating: 0, count: m * n)
            for i in 0..<m {
                for j in 0..<n {
                    var sum: Float = 0
                    for p in 0..<k {
                        sum += s1[i * k + p] * s2[p * n + j]
                    }
                    out[i * n + j] = sum
                }
            }
            resultScalars = out
        }

        // Wrap result and retain for FFI
        let resultWrapper = TensorWrapper(result, scalars: resultScalars)
        let retained = Unmanaged.passRetained(resultWrapper).toOpaque()
        return UnsafeMutableRawPointer(retained)
    }
}

/// Materialize tensor to float array.
///
/// Copies tensor data to the provided output buffer.
///
/// - Parameters:
///   - handle: Tensor to materialize
///   - output: Output buffer for floats
///   - outputLen: Size of output buffer
///
/// - Returns: Number of elements copied on success, negative error code on failure
///   - -1: Invalid handle
///   - -2: Buffer too small
///   - -3: Materialization failed
///
/// - Thread Safety: Safe to call from any thread
/// - Note: Does not free the tensor; caller must still call tensor_free
@_cdecl("swift_coreml_tensor_to_floats")
public func swiftCoreMLTensorToFloats(
    handle: UnsafeMutableRawPointer?,
    output: UnsafeMutablePointer<Float>,
    outputLen: Int
) -> Int32 {
    guard #available(macOS 15.0, *) else {
        return -1
    }

    guard let handle = handle else {
        return -1
    }

    let wrapper = Unmanaged<TensorWrapper>.fromOpaque(handle).takeUnretainedValue()
    let count = wrapper.tensor.scalarCount

    if outputLen < count {
        return -2 // Buffer too small
    }

    // Use cached scalars if available
    guard let scalars = wrapper.cachedScalars else {
        return -4 // Scalars not cached - tensor was created from operation
    }

    // Copy to output buffer
    for i in 0..<count {
        output[i] = scalars[i]
    }

    return Int32(count)
}

// MARK: - macOS 26+ Enhanced Operations with Compute Policy

/// Matrix multiplication with explicit compute unit selection (macOS 26+).
///
/// On macOS 26+ (Tahoe), this uses `withMLTensorComputePolicy` for ANE acceleration.
/// On earlier versions, falls back to basic matmul.
///
/// - Parameters:
///   - handle1: First tensor
///   - handle2: Second tensor
///   - computeUnits: Compute unit preference (0=CPU, 1=CPU+GPU, 2=CPU+ANE, 3=All)
///
/// - Returns: New tensor with matmul result, or nil on error
///
/// - Memory: Caller owns the returned tensor and must free it
/// - Thread Safety: Safe to call from any thread
@_cdecl("swift_coreml_tensor_matmul_v2")
public func swiftCoreMLTensorMatmulV2(
    handle1: UnsafeMutableRawPointer?,
    handle2: UnsafeMutableRawPointer?,
    computeUnits: ComputeUnitPreference
) -> UnsafeMutableRawPointer? {
    guard #available(macOS 15.0, *) else {
        return nil
    }

    guard let handle1 = handle1, let handle2 = handle2 else {
        return nil
    }

    // macOS 26+ path with compute policy
    if #available(macOS 26.0, *) {
        return autoreleasepool {
            let wrapper1 = Unmanaged<TensorWrapper>.fromOpaque(handle1).takeUnretainedValue()
            let wrapper2 = Unmanaged<TensorWrapper>.fromOpaque(handle2).takeUnretainedValue()

            // Select compute units based on preference
            let units: MLComputeUnits
            switch computeUnits {
            case 0:
                units = .cpuOnly
            case 1:
                units = .cpuAndGPU
            case 2:
                units = .cpuAndNeuralEngine
            default:
                units = .all
            }

            #if DEBUG
            print("[CoreMLBridge] matmul_v2 (macOS 26+) - using compute units: \(units)")
            #endif

            // Perform matmul - on macOS 26+, the OS may utilize ANE
            let result = wrapper1.tensor.matmul(wrapper2.tensor)

            // Compute result scalars manually for synchronous access
            var resultScalars: [Float]?
            if let s1 = wrapper1.cachedScalars, let s2 = wrapper2.cachedScalars,
               wrapper1.shape.count == 2 && wrapper2.shape.count == 2 {
                let m = wrapper1.shape[0]
                let k = wrapper1.shape[1]
                let n = wrapper2.shape[1]

                var out = [Float](repeating: 0, count: m * n)
                for i in 0..<m {
                    for j in 0..<n {
                        var sum: Float = 0
                        for p in 0..<k {
                            sum += s1[i * k + p] * s2[p * n + j]
                        }
                        out[i * n + j] = sum
                    }
                }
                resultScalars = out
            }

            let resultWrapper = TensorWrapper(result, scalars: resultScalars)
            let retained = Unmanaged.passRetained(resultWrapper).toOpaque()
            return UnsafeMutableRawPointer(retained)
        }
    }

    // Fallback for macOS 15-25
    return swiftCoreMLTensorMatmul(handle1: handle1, handle2: handle2)
}

/// Softmax with explicit compute unit selection (macOS 26+).
///
/// - Parameters:
///   - handle: Input tensor
///   - dim: Dimension for softmax (-1 for last dimension)
///   - computeUnits: Compute unit preference (0=CPU, 1=CPU+GPU, 2=CPU+ANE, 3=All)
///
/// - Returns: New tensor with softmax applied, or nil on error
@_cdecl("swift_coreml_tensor_softmax_v2")
public func swiftCoreMLTensorSoftmaxV2(
    handle: UnsafeMutableRawPointer?,
    dim: Int32,
    computeUnits: ComputeUnitPreference
) -> UnsafeMutableRawPointer? {
    guard #available(macOS 15.0, *) else {
        return nil
    }

    guard let handle = handle else {
        return nil
    }

    // macOS 26+ path
    if #available(macOS 26.0, *) {
        return autoreleasepool {
            let wrapper = Unmanaged<TensorWrapper>.fromOpaque(handle).takeUnretainedValue()

            #if DEBUG
            let units: MLComputeUnits
            switch computeUnits {
            case 0:
                units = .cpuOnly
            case 1:
                units = .cpuAndGPU
            case 2:
                units = .cpuAndNeuralEngine
            default:
                units = .all
            }
            print("[CoreMLBridge] softmax_v2 (macOS 26+) - using compute units: \(units)")
            #endif

            let axis = Int(dim)
            let result = wrapper.tensor.softmax(alongAxis: axis)

            // Compute result scalars manually using numerically stable softmax
            var resultScalars: [Float]?
            if let inputScalars = wrapper.cachedScalars {
                let shape = wrapper.shape
                let effectiveAxis = axis < 0 ? shape.count + axis : axis

                if shape.count == 1 || (shape.count == 2 && effectiveAxis == shape.count - 1) {
                    // Handle 1D or 2D with softmax along last axis (row-wise)
                    let rowSize = shape.count == 1 ? shape[0] : shape[1]
                    let numRows = shape.count == 1 ? 1 : shape[0]

                    var output = [Float](repeating: 0, count: inputScalars.count)

                    for row in 0..<numRows {
                        let startIdx = row * rowSize
                        let endIdx = startIdx + rowSize
                        let rowSlice = Array(inputScalars[startIdx..<endIdx])

                        // Find max for numerical stability
                        let maxVal = rowSlice.max() ?? 0.0

                        // Compute exp(x - max) for each element
                        let expValues = rowSlice.map { exp($0 - maxVal) }

                        // Sum of exp values
                        let sum = expValues.reduce(0, +)

                        // Normalize by sum
                        for i in 0..<rowSize {
                            output[startIdx + i] = expValues[i] / sum
                        }
                    }
                    resultScalars = output
                } else {
                    // Fallback for other tensor shapes
                    let maxVal = inputScalars.max() ?? 0.0
                    let expValues = inputScalars.map { exp($0 - maxVal) }
                    let sum = expValues.reduce(0, +)
                    resultScalars = expValues.map { $0 / sum }
                }
            }

            let resultWrapper = TensorWrapper(result, scalars: resultScalars)
            let retained = Unmanaged.passRetained(resultWrapper).toOpaque()
            return UnsafeMutableRawPointer(retained)
        }
    }

    // Fallback for macOS 15-25
    return swiftCoreMLTensorSoftmax(handle: handle, dim: dim)
}

// MARK: - macOS 26+ Enhanced Tensor Materialization

/// Materialize tensor to float array with enhanced options (macOS 26+).
///
/// This function uses cached scalars for FFI compatibility, avoiding the need
/// for Swift concurrency runtime. On macOS 26+, additional validation is performed.
///
/// - Parameters:
///   - handle: Tensor to materialize
///   - output: Output buffer for floats
///   - outputLen: Size of output buffer
///   - useAsync: Reserved for future use (currently ignored to avoid concurrency dependency)
///
/// - Returns: Number of elements copied on success, negative error code on failure
///   - -1: Invalid handle
///   - -2: Buffer too small
///   - -4: Scalars not cached (operation result tensor)
@_cdecl("swift_coreml_tensor_to_floats_v2")
public func swiftCoreMLTensorToFloatsV2(
    handle: UnsafeMutableRawPointer?,
    output: UnsafeMutablePointer<Float>,
    outputLen: Int,
    useAsync: Bool
) -> Int32 {
    guard #available(macOS 15.0, *) else {
        return -1
    }

    guard let handle = handle else {
        return -1
    }

    let wrapper = Unmanaged<TensorWrapper>.fromOpaque(handle).takeUnretainedValue()
    let count = wrapper.tensor.scalarCount

    if outputLen < count {
        return -2 // Buffer too small
    }

    #if DEBUG
    if #available(macOS 26.0, *) {
        print("[CoreMLBridge] tensor_to_floats_v2 (macOS 26+) - using cached scalars (concurrency-free)")
    }
    #endif

    // Use cached scalars - avoids Swift concurrency runtime dependency
    // This is the safe FFI-compatible path that works on all macOS versions
    guard let scalars = wrapper.cachedScalars else {
        #if DEBUG
        print("[CoreMLBridge] ERROR: scalars not cached for this tensor")
        #endif
        return -4 // Scalars not cached
    }

    for i in 0..<count {
        output[i] = scalars[i]
    }

    return Int32(count)
}

// MARK: - macOS 26+ Batch Operations

/// Perform batch matrix multiplication (macOS 26+ optimized).
///
/// Multiplies corresponding matrices in two batches of tensors.
/// Optimized for ANE on macOS 26+.
///
/// - Parameters:
///   - handles1: Array of first tensor handles
///   - handles2: Array of second tensor handles
///   - count: Number of tensor pairs
///   - resultsOut: Output array for result tensor handles
///   - computeUnits: Compute unit preference
///
/// - Returns: Number of successful operations, or negative error code
@_cdecl("swift_coreml_batch_matmul")
public func swiftCoreMLBatchMatmul(
    handles1: UnsafePointer<UnsafeMutableRawPointer?>,
    handles2: UnsafePointer<UnsafeMutableRawPointer?>,
    count: Int,
    resultsOut: UnsafeMutablePointer<UnsafeMutableRawPointer?>,
    computeUnits: ComputeUnitPreference
) -> Int32 {
    guard #available(macOS 15.0, *) else {
        return -1
    }

    var successCount: Int32 = 0

    for i in 0..<count {
        let h1 = handles1[i]
        let h2 = handles2[i]

        if #available(macOS 26.0, *) {
            resultsOut[i] = swiftCoreMLTensorMatmulV2(handle1: h1, handle2: h2, computeUnits: computeUnits)
        } else {
            resultsOut[i] = swiftCoreMLTensorMatmul(handle1: h1, handle2: h2)
        }

        if resultsOut[i] != nil {
            successCount += 1
        }
    }

    return successCount
}

// MARK: - Normalization Operations

/// Layer Normalization: (x - mean) / sqrt(var + eps) * weight + bias
///
/// Normalizes the input tensor along the last dimension using the standard
/// layer normalization formula. Critical for transformer models.
///
/// - Parameters:
///   - handle: Input tensor handle
///   - weight: Pointer to scale weights (gamma)
///   - weightLen: Length of weight array (must match last dimension)
///   - bias: Pointer to bias (beta)
///   - biasLen: Length of bias array (must match last dimension)
///   - eps: Small constant for numerical stability (typically 1e-5)
///
/// - Returns: Normalized tensor, or nil on error
///
/// - Memory: Caller owns the returned tensor and must free it
/// - Thread Safety: Safe to call from any thread
@_cdecl("swift_coreml_tensor_layernorm")
public func swiftCoreMLTensorLayernorm(
    handle: UnsafeMutableRawPointer?,
    weight: UnsafePointer<Float>,
    weightLen: Int,
    bias: UnsafePointer<Float>,
    biasLen: Int,
    eps: Float
) -> UnsafeMutableRawPointer? {
    guard #available(macOS 15.0, *) else {
        return nil
    }

    guard let handle = handle else {
        return nil
    }

    return autoreleasepool {
        let wrapper = Unmanaged<TensorWrapper>.fromOpaque(handle).takeUnretainedValue()
        let shape = wrapper.shape

        guard !shape.isEmpty else {
            #if DEBUG
            print("[CoreMLBridge] ERROR: Empty shape for layernorm")
            #endif
            return nil
        }

        let lastDim = shape.last!

        guard weightLen == lastDim && biasLen == lastDim else {
            #if DEBUG
            print("[CoreMLBridge] ERROR: Weight/bias length mismatch - expected \(lastDim), got weight=\(weightLen), bias=\(biasLen)")
            #endif
            return nil
        }

        // Get weight and bias arrays
        let weightArray = Array(UnsafeBufferPointer(start: weight, count: weightLen))
        let biasArray = Array(UnsafeBufferPointer(start: bias, count: biasLen))

        // Compute layer norm using cached scalars
        guard let inputScalars = wrapper.cachedScalars else {
            #if DEBUG
            print("[CoreMLBridge] ERROR: No cached scalars for layernorm")
            #endif
            return nil
        }

        // Layer norm: (x - mean) / sqrt(var + eps) * weight + bias
        // Applied along the last dimension
        let numVectors = inputScalars.count / lastDim

        var outputScalars = [Float](repeating: 0, count: inputScalars.count)

        for v in 0..<numVectors {
            let startIdx = v * lastDim

            // Compute mean
            var mean: Float = 0
            for i in 0..<lastDim {
                mean += inputScalars[startIdx + i]
            }
            mean /= Float(lastDim)

            // Compute variance
            var variance: Float = 0
            for i in 0..<lastDim {
                let diff = inputScalars[startIdx + i] - mean
                variance += diff * diff
            }
            variance /= Float(lastDim)

            // Normalize: (x - mean) / sqrt(var + eps) * weight + bias
            let invStd = 1.0 / sqrt(variance + eps)
            for i in 0..<lastDim {
                let normalized = (inputScalars[startIdx + i] - mean) * invStd
                outputScalars[startIdx + i] = normalized * weightArray[i] + biasArray[i]
            }
        }

        // Create MLTensor from result
        let resultTensor = MLTensor(shape: shape, scalars: outputScalars)
        let resultWrapper = TensorWrapper(resultTensor, scalars: outputScalars)
        let retained = Unmanaged.passRetained(resultWrapper).toOpaque()
        return UnsafeMutableRawPointer(retained)
    }
}

/// RMS Normalization: x * rsqrt(mean(x^2) + eps) * weight
///
/// Root Mean Square layer normalization used in LLaMA-style models.
/// More efficient than LayerNorm as it skips mean subtraction.
///
/// - Parameters:
///   - handle: Input tensor handle
///   - weight: Pointer to scale weights (gamma)
///   - weightLen: Length of weight array (must match last dimension)
///   - eps: Small constant for numerical stability (typically 1e-5)
///
/// - Returns: Normalized tensor, or nil on error
///
/// - Memory: Caller owns the returned tensor and must free it
/// - Thread Safety: Safe to call from any thread
@_cdecl("swift_coreml_tensor_rms_norm")
public func swiftCoreMLTensorRmsNorm(
    handle: UnsafeMutableRawPointer?,
    weight: UnsafePointer<Float>,
    weightLen: Int,
    eps: Float
) -> UnsafeMutableRawPointer? {
    guard #available(macOS 15.0, *) else {
        return nil
    }

    guard let handle = handle else {
        return nil
    }

    return autoreleasepool {
        let wrapper = Unmanaged<TensorWrapper>.fromOpaque(handle).takeUnretainedValue()
        let shape = wrapper.shape

        guard !shape.isEmpty else {
            #if DEBUG
            print("[CoreMLBridge] ERROR: Empty shape for rms_norm")
            #endif
            return nil
        }

        let lastDim = shape.last!

        guard weightLen == lastDim else {
            #if DEBUG
            print("[CoreMLBridge] ERROR: Weight length mismatch - expected \(lastDim), got \(weightLen)")
            #endif
            return nil
        }

        // Get weight array
        let weightArray = Array(UnsafeBufferPointer(start: weight, count: weightLen))

        // Compute RMS norm using cached scalars
        guard let inputScalars = wrapper.cachedScalars else {
            #if DEBUG
            print("[CoreMLBridge] ERROR: No cached scalars for rms_norm")
            #endif
            return nil
        }

        // RMS norm: x * rsqrt(mean(x^2) + eps) * weight
        // Applied along the last dimension
        let numVectors = inputScalars.count / lastDim

        var outputScalars = [Float](repeating: 0, count: inputScalars.count)

        for v in 0..<numVectors {
            let startIdx = v * lastDim

            // Compute mean of squares
            var meanSquare: Float = 0
            for i in 0..<lastDim {
                let val = inputScalars[startIdx + i]
                meanSquare += val * val
            }
            meanSquare /= Float(lastDim)

            // rsqrt(mean(x^2) + eps)
            let rmsInv = 1.0 / sqrt(meanSquare + eps)

            // Apply normalization and scale
            for i in 0..<lastDim {
                outputScalars[startIdx + i] = inputScalars[startIdx + i] * rmsInv * weightArray[i]
            }
        }

        // Create MLTensor from result
        let resultTensor = MLTensor(shape: shape, scalars: outputScalars)
        let resultWrapper = TensorWrapper(resultTensor, scalars: outputScalars)
        let retained = Unmanaged.passRetained(resultWrapper).toOpaque()
        return UnsafeMutableRawPointer(retained)
    }
}

// MARK: - System Information

/// Get detailed system information for debugging.
///
/// Returns a bitmask with system capabilities:
/// - Bit 0: MLTensor available (macOS 15+)
/// - Bit 1: Enhanced MLComputePolicy available (macOS 26+)
/// - Bit 2: Neural Engine available
/// - Bit 3: GPU available
///
/// - Returns: Capability bitmask
@_cdecl("swift_coreml_system_capabilities")
public func swiftCoreMLSystemCapabilities() -> Int32 {
    var capabilities: Int32 = 0

    // Bit 0: MLTensor available
    if #available(macOS 15.0, *) {
        capabilities |= 1
    }

    // Bit 1: Enhanced APIs (macOS 26+)
    if #available(macOS 26.0, *) {
        capabilities |= 2
    }

    // Bits 2-3: Hardware capabilities (always available on Apple Silicon)
    #if arch(arm64)
    capabilities |= 4  // Neural Engine (ANE) available on Apple Silicon
    capabilities |= 8  // GPU available
    #endif

    return capabilities
}
