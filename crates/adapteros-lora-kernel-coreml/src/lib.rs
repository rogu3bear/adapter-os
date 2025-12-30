//! CoreML kernel implementation for Neural Engine acceleration
//!
//! This crate provides the CoreML backend for AdapterOS, enabling inference
//! on Apple Neural Engine (ANE) for improved power efficiency and performance.

#![allow(dead_code)]

// =============================================================================
// Release build feature flag validation
// Prevents deploying stub backends to production
// =============================================================================
#[cfg(all(not(debug_assertions), feature = "coreml-stub"))]
compile_error!(
    "coreml-stub feature must not be enabled in release builds. \
     Stub mode is for testing only."
);

use crate::export::validate_coreml_fusion;
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_lora_kernel_api::{
    attestation, BackendHealth, BackendMetrics, FusedKernels, GpuBufferFingerprint, IoBuffers,
    RouterRing,
};
use adapteros_types::coreml::CoreMLPlacementSpec;
use std::collections::{HashMap, HashSet};
use std::ffi::{c_void, CStr};
use std::path::{Path, PathBuf};
use std::process::Command;
use tokio::sync::{oneshot, RwLock};

pub mod aos_loader;
pub mod config;
pub mod export;
pub mod ffi;
pub mod fusion;
pub mod hybrid;
pub mod matmul;
pub mod moe;
pub mod placement;

pub use placement::{
    resolve_placement, CoreMLGraph, CoreMLGraphNode, PlacementMetrics, PlacementResolution,
};

pub use config::{ComputeUnits, CoreMLConfig, CoreMLModelParams};
pub use ffi::{
    capabilities, AneCheckResult, ComputeUnitPreference, CoreMLAsyncCallback, MLTensorHandle,
    MltensorApiVersion, OperationType,
};
// Re-export main types (correct casing per naming contract)
pub use moe::{
    MoEAdapterWeights, MoEConfig, MoEGpuFingerprint, MoELoRAStrategy, MoELoRATarget, MoELoRAWeights,
};

// Re-export deprecated aliases for backwards compatibility
#[allow(deprecated)]
pub use moe::{
    MoeAdapterWeights, MoeConfig, MoeGpuFingerprint, MoeLoraStrategy, MoeLoraTarget, MoeLoraWeights,
};

pub use hybrid::{HybridCoreMLBackend, LmHeadLoRA};
pub use matmul::{axpy, matmul_accelerate, matvec_accelerate};

// TensorBridgeType is defined below in this module

// =============================================================================
// MLTensor Safe Wrapper API (macOS 15+) with Runtime Dispatch
// =============================================================================

/// Bridge implementation type for MLTensor operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TensorBridgeType {
    /// Swift bridge (better performance on macOS 15+)
    Swift,
    /// Objective-C++ bridge (fallback)
    ObjCpp,
}

/// Check once at module load if Swift bridge is available
#[cfg(target_os = "macos")]
fn swift_bridge_available() -> bool {
    static SWIFT_AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *SWIFT_AVAILABLE.get_or_init(|| unsafe { ffi::swift_coreml_supports_mltensor() })
}

#[cfg(not(target_os = "macos"))]
fn swift_bridge_available() -> bool {
    false
}

/// Get the MLTensor API version level
///
/// Returns which version of the MLTensor API is available:
/// - `NotAvailable`: pre-macOS 15
/// - `Sequoia`: macOS 15.x - Basic MLTensor API
/// - `Tahoe`: macOS 26.x - Enhanced MLComputePolicy API
#[cfg(target_os = "macos")]
pub fn get_mltensor_api_version() -> MltensorApiVersion {
    static API_VERSION: std::sync::OnceLock<MltensorApiVersion> = std::sync::OnceLock::new();
    *API_VERSION.get_or_init(|| {
        let version = unsafe { ffi::swift_coreml_mltensor_api_version() };
        MltensorApiVersion::from(version)
    })
}

#[cfg(not(target_os = "macos"))]
pub fn get_mltensor_api_version() -> MltensorApiVersion {
    MltensorApiVersion::NotAvailable
}

/// Get system capabilities bitmask
///
/// Returns a bitmask with:
/// - Bit 0: MLTensor available (macOS 15+)
/// - Bit 1: Enhanced APIs (macOS 26+)
/// - Bit 2: Neural Engine available
/// - Bit 3: GPU available
///
/// Use the `capabilities` module constants to check specific capabilities.
#[cfg(target_os = "macos")]
pub fn get_system_capabilities() -> i32 {
    static CAPABILITIES: std::sync::OnceLock<i32> = std::sync::OnceLock::new();
    *CAPABILITIES.get_or_init(|| unsafe { ffi::swift_coreml_system_capabilities() })
}

#[cfg(not(target_os = "macos"))]
pub fn get_system_capabilities() -> i32 {
    0
}

/// Check if macOS 26+ (Tahoe) enhanced APIs are available
pub fn has_enhanced_api() -> bool {
    get_system_capabilities() & capabilities::ENHANCED_API != 0
}

/// Check if Neural Engine (ANE) is available
pub fn has_neural_engine() -> bool {
    get_system_capabilities() & capabilities::NEURAL_ENGINE != 0
}

/// Safe wrapper for MLTensor operations (macOS 15+)
///
/// MLTensor provides high-performance tensor operations using CoreML's modern API,
/// with automatic memory management and type safety. Automatically dispatches to
/// Swift bridge when available for better performance.
pub struct MLTensor {
    /// For Swift bridge: raw pointer to Swift-managed MLTensor
    /// For ObjC++ bridge: uses MLTensorHandle
    #[cfg(target_os = "macos")]
    swift_handle: *mut std::ffi::c_void,
    #[cfg(target_os = "macos")]
    objc_handle: ffi::MLTensorHandle,
    #[cfg(target_os = "macos")]
    bridge_type: TensorBridgeType,
    #[cfg(not(target_os = "macos"))]
    _phantom: std::marker::PhantomData<()>,
}

// MLTensor is Send + Sync because the underlying CoreML objects are thread-safe
unsafe impl Send for MLTensor {}
unsafe impl Sync for MLTensor {}

impl MLTensor {
    /// Check if MLTensor API is available (requires macOS 15+)
    pub fn is_available() -> bool {
        #[cfg(target_os = "macos")]
        {
            unsafe { ffi::coreml_supports_mltensor() }
        }

        #[cfg(not(target_os = "macos"))]
        false
    }

    /// Get the bridge type being used for this tensor
    #[cfg(target_os = "macos")]
    pub fn bridge_type(&self) -> TensorBridgeType {
        self.bridge_type
    }

    /// Create tensor from float slice with given shape
    ///
    /// # Arguments
    /// * `data` - The float data to initialize the tensor with
    /// * `shape` - The dimensions of the tensor
    ///
    /// # Errors
    /// Returns `AosError::Kernel` if:
    /// - MLTensor API is not available
    /// - Shape has more than 16 dimensions
    /// - Data length doesn't match shape product
    /// - Tensor creation fails
    pub fn from_floats(data: &[f32], shape: &[usize]) -> Result<Self> {
        #[cfg(target_os = "macos")]
        {
            if !Self::is_available() {
                return Err(AosError::Kernel(
                    "MLTensor API not available (requires macOS 15+)".to_string(),
                ));
            }

            if shape.len() > 16 {
                return Err(AosError::Kernel(format!(
                    "Shape has {} dimensions, maximum is 16",
                    shape.len()
                )));
            }

            let expected_len: usize = shape.iter().product();
            if data.len() != expected_len {
                return Err(AosError::Kernel(format!(
                    "Data length {} doesn't match shape product {}",
                    data.len(),
                    expected_len
                )));
            }

            // Try Swift bridge first (better performance)
            if swift_bridge_available() {
                let swift_ptr = unsafe {
                    ffi::swift_coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len())
                };
                if !swift_ptr.is_null() {
                    // Cache shape in objc_handle for shape/num_elements queries
                    let mut cached_handle = ffi::MLTensorHandle {
                        rank: shape.len() as u32,
                        ..Default::default()
                    };
                    for (i, &dim) in shape.iter().enumerate() {
                        cached_handle.shape[i] = dim;
                    }
                    return Ok(Self {
                        swift_handle: swift_ptr,
                        objc_handle: cached_handle,
                        bridge_type: TensorBridgeType::Swift,
                    });
                }
                // Swift bridge failed, fall through to ObjC++
            }

            // Fall back to Obj-C++ implementation
            let handle = unsafe {
                ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len())
            };

            if !handle.is_valid() {
                return Err(AosError::Kernel("Failed to create MLTensor".to_string()));
            }

            Ok(Self {
                swift_handle: std::ptr::null_mut(),
                objc_handle: handle,
                bridge_type: TensorBridgeType::ObjCpp,
            })
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = (data, shape);
            Err(AosError::Kernel(
                "MLTensor only available on macOS".to_string(),
            ))
        }
    }

    /// Create tensor from float slice with given shape and compute unit preference (macOS 26+)
    ///
    /// On macOS 26+ (Tahoe), this method allows explicit selection of compute units
    /// for optimal performance. On earlier versions, falls back to default behavior.
    ///
    /// # Arguments
    /// * `data` - The float data to initialize the tensor with
    /// * `shape` - The dimensions of the tensor
    /// * `compute_units` - Preferred compute units for tensor operations
    ///
    /// # Errors
    /// Returns `AosError::Kernel` if creation fails
    pub fn from_floats_with_compute_units(
        data: &[f32],
        shape: &[usize],
        compute_units: ComputeUnitPreference,
    ) -> Result<Self> {
        #[cfg(target_os = "macos")]
        {
            if !Self::is_available() {
                return Err(AosError::Kernel(
                    "MLTensor API not available (requires macOS 15+)".to_string(),
                ));
            }

            if shape.len() > 16 {
                return Err(AosError::Kernel(format!(
                    "Shape has {} dimensions, maximum is 16",
                    shape.len()
                )));
            }

            let expected_len: usize = shape.iter().product();
            if data.len() != expected_len {
                return Err(AosError::Kernel(format!(
                    "Data length {} doesn't match shape product {}",
                    data.len(),
                    expected_len
                )));
            }

            // Use v2 API for compute unit selection (macOS 26+ optimized)
            if swift_bridge_available() {
                let swift_ptr = unsafe {
                    ffi::swift_coreml_create_tensor_f32_v2(
                        data.as_ptr(),
                        shape.as_ptr(),
                        shape.len(),
                        compute_units as i32,
                    )
                };
                if !swift_ptr.is_null() {
                    let mut cached_handle = ffi::MLTensorHandle {
                        rank: shape.len() as u32,
                        ..Default::default()
                    };
                    for (i, &dim) in shape.iter().enumerate() {
                        cached_handle.shape[i] = dim;
                    }
                    return Ok(Self {
                        swift_handle: swift_ptr,
                        objc_handle: cached_handle,
                        bridge_type: TensorBridgeType::Swift,
                    });
                }
            }

            // Fall back to basic creation if v2 not available
            Self::from_floats(data, shape)
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = (data, shape, compute_units);
            Err(AosError::Kernel(
                "MLTensor only available on macOS".to_string(),
            ))
        }
    }

    /// Apply softmax along specified dimension
    ///
    /// # Arguments
    /// * `dim` - Dimension for softmax (-1 for last dimension)
    ///
    /// # Errors
    /// Returns `AosError::Kernel` if the operation fails
    pub fn softmax(&self, dim: i32) -> Result<Self> {
        #[cfg(target_os = "macos")]
        {
            match self.bridge_type {
                TensorBridgeType::Swift => {
                    let result =
                        unsafe { ffi::swift_coreml_tensor_softmax(self.swift_handle, dim) };
                    if result.is_null() {
                        return Err(AosError::Kernel("Softmax operation failed".to_string()));
                    }
                    // Softmax preserves shape, copy from input
                    Ok(Self {
                        swift_handle: result,
                        objc_handle: self.objc_handle,
                        bridge_type: TensorBridgeType::Swift,
                    })
                }
                TensorBridgeType::ObjCpp => {
                    let handle = unsafe { ffi::coreml_tensor_softmax(self.objc_handle, dim) };
                    if !handle.is_valid() {
                        return Err(AosError::Kernel("Softmax operation failed".to_string()));
                    }
                    Ok(Self {
                        swift_handle: std::ptr::null_mut(),
                        objc_handle: handle,
                        bridge_type: TensorBridgeType::ObjCpp,
                    })
                }
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = dim;
            Err(AosError::Kernel(
                "MLTensor only available on macOS".to_string(),
            ))
        }
    }

    /// Apply softmax with compute unit preference (macOS 26+ optimized)
    ///
    /// On macOS 26+ (Tahoe), this method allows explicit selection of compute units.
    /// Softmax benefits from GPU execution due to its transcendental operations.
    /// On earlier versions, falls back to default softmax.
    ///
    /// # Arguments
    /// * `dim` - Dimension for softmax (-1 for last dimension)
    /// * `compute_units` - Preferred compute units (e.g., CpuAndGpu for softmax)
    ///
    /// # Errors
    /// Returns `AosError::Kernel` if the operation fails
    pub fn softmax_with_compute_units(
        &self,
        dim: i32,
        compute_units: ComputeUnitPreference,
    ) -> Result<Self> {
        #[cfg(target_os = "macos")]
        {
            match self.bridge_type {
                TensorBridgeType::Swift => {
                    // Use v2 API with compute unit preference (macOS 26+ optimized)
                    let result = unsafe {
                        ffi::swift_coreml_tensor_softmax_v2(
                            self.swift_handle,
                            dim,
                            compute_units as i32,
                        )
                    };
                    if result.is_null() {
                        return Err(AosError::Kernel("Softmax operation failed".to_string()));
                    }
                    // Softmax preserves shape
                    Ok(Self {
                        swift_handle: result,
                        objc_handle: self.objc_handle,
                        bridge_type: TensorBridgeType::Swift,
                    })
                }
                TensorBridgeType::ObjCpp => {
                    // ObjC++ doesn't have v2 API, fall back to regular softmax
                    self.softmax(dim)
                }
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = (dim, compute_units);
            Err(AosError::Kernel(
                "MLTensor only available on macOS".to_string(),
            ))
        }
    }

    /// Add two tensors element-wise
    ///
    /// # Errors
    /// Returns `AosError::Kernel` if the operation fails (e.g., shape mismatch)
    pub fn add(&self, other: &Self) -> Result<Self> {
        #[cfg(target_os = "macos")]
        {
            // Both tensors must use the same bridge type
            if self.bridge_type != other.bridge_type {
                return Err(AosError::Kernel(
                    "Cannot add tensors from different bridge types".to_string(),
                ));
            }

            match self.bridge_type {
                TensorBridgeType::Swift => {
                    let result = unsafe {
                        ffi::swift_coreml_tensor_add(self.swift_handle, other.swift_handle)
                    };
                    if result.is_null() {
                        return Err(AosError::Kernel("Tensor addition failed".to_string()));
                    }
                    // Element-wise add preserves shape, copy from input
                    Ok(Self {
                        swift_handle: result,
                        objc_handle: self.objc_handle,
                        bridge_type: TensorBridgeType::Swift,
                    })
                }
                TensorBridgeType::ObjCpp => {
                    let handle =
                        unsafe { ffi::coreml_tensor_add(self.objc_handle, other.objc_handle) };
                    if !handle.is_valid() {
                        return Err(AosError::Kernel("Tensor addition failed".to_string()));
                    }
                    Ok(Self {
                        swift_handle: std::ptr::null_mut(),
                        objc_handle: handle,
                        bridge_type: TensorBridgeType::ObjCpp,
                    })
                }
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = other;
            Err(AosError::Kernel(
                "MLTensor only available on macOS".to_string(),
            ))
        }
    }

    /// Scale tensor by scalar value
    ///
    /// # Arguments
    /// * `factor` - Scalar multiplier
    ///
    /// # Errors
    /// Returns `AosError::Kernel` if the operation fails
    pub fn scale(&self, factor: f32) -> Result<Self> {
        #[cfg(target_os = "macos")]
        {
            match self.bridge_type {
                TensorBridgeType::Swift => {
                    let result =
                        unsafe { ffi::swift_coreml_tensor_scale(self.swift_handle, factor) };
                    if result.is_null() {
                        return Err(AosError::Kernel("Tensor scaling failed".to_string()));
                    }
                    // Scale preserves shape, copy from input
                    Ok(Self {
                        swift_handle: result,
                        objc_handle: self.objc_handle,
                        bridge_type: TensorBridgeType::Swift,
                    })
                }
                TensorBridgeType::ObjCpp => {
                    let handle = unsafe { ffi::coreml_tensor_scale(self.objc_handle, factor) };
                    if !handle.is_valid() {
                        return Err(AosError::Kernel("Tensor scaling failed".to_string()));
                    }
                    Ok(Self {
                        swift_handle: std::ptr::null_mut(),
                        objc_handle: handle,
                        bridge_type: TensorBridgeType::ObjCpp,
                    })
                }
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = factor;
            Err(AosError::Kernel(
                "MLTensor only available on macOS".to_string(),
            ))
        }
    }

    /// Matrix multiplication of two tensors
    ///
    /// # Errors
    /// Returns `AosError::Kernel` if the operation fails (e.g., incompatible shapes)
    pub fn matmul(&self, other: &Self) -> Result<Self> {
        #[cfg(target_os = "macos")]
        {
            // Both tensors must use the same bridge type
            if self.bridge_type != other.bridge_type {
                return Err(AosError::Kernel(
                    "Cannot multiply tensors from different bridge types".to_string(),
                ));
            }

            match self.bridge_type {
                TensorBridgeType::Swift => {
                    let result = unsafe {
                        ffi::swift_coreml_tensor_matmul(self.swift_handle, other.swift_handle)
                    };
                    if result.is_null() {
                        return Err(AosError::Kernel("Matrix multiplication failed".to_string()));
                    }
                    // Compute result shape for matmul: [M, K] x [K, N] = [M, N]
                    // For 2D: result shape is [self.shape[0], other.shape[1]]
                    let mut result_handle = ffi::MLTensorHandle::default();
                    let self_rank = self.objc_handle.rank as usize;
                    let other_rank = other.objc_handle.rank as usize;
                    if self_rank >= 2 && other_rank >= 2 {
                        result_handle.rank = self_rank as u32;
                        // Copy batch dimensions from self (if any)
                        for i in 0..self_rank.saturating_sub(2) {
                            result_handle.shape[i] = self.objc_handle.shape[i];
                        }
                        // Result dims: M from self, N from other
                        result_handle.shape[self_rank - 2] = self.objc_handle.shape[self_rank - 2];
                        result_handle.shape[self_rank - 1] =
                            other.objc_handle.shape[other_rank - 1];
                    }
                    Ok(Self {
                        swift_handle: result,
                        objc_handle: result_handle,
                        bridge_type: TensorBridgeType::Swift,
                    })
                }
                TensorBridgeType::ObjCpp => {
                    let handle =
                        unsafe { ffi::coreml_tensor_matmul(self.objc_handle, other.objc_handle) };
                    if !handle.is_valid() {
                        return Err(AosError::Kernel("Matrix multiplication failed".to_string()));
                    }
                    Ok(Self {
                        swift_handle: std::ptr::null_mut(),
                        objc_handle: handle,
                        bridge_type: TensorBridgeType::ObjCpp,
                    })
                }
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = other;
            Err(AosError::Kernel(
                "MLTensor only available on macOS".to_string(),
            ))
        }
    }

    /// Matrix multiplication with compute unit preference (macOS 26+ optimized)
    ///
    /// On macOS 26+ (Tahoe), this method allows explicit selection of compute units
    /// for ANE acceleration. On earlier versions, falls back to default matmul.
    ///
    /// # Arguments
    /// * `other` - The second tensor to multiply
    /// * `compute_units` - Preferred compute units (e.g., CpuAndNeuralEngine for ANE)
    ///
    /// # Errors
    /// Returns `AosError::Kernel` if the operation fails
    pub fn matmul_with_compute_units(
        &self,
        other: &Self,
        compute_units: ComputeUnitPreference,
    ) -> Result<Self> {
        #[cfg(target_os = "macos")]
        {
            if self.bridge_type != other.bridge_type {
                return Err(AosError::Kernel(
                    "Cannot multiply tensors from different bridge types".to_string(),
                ));
            }

            match self.bridge_type {
                TensorBridgeType::Swift => {
                    // Use v2 API with compute unit preference (macOS 26+ optimized)
                    let result = unsafe {
                        ffi::swift_coreml_tensor_matmul_v2(
                            self.swift_handle,
                            other.swift_handle,
                            compute_units as i32,
                        )
                    };
                    if result.is_null() {
                        return Err(AosError::Kernel("Matrix multiplication failed".to_string()));
                    }
                    // Compute result shape for matmul
                    let mut result_handle = ffi::MLTensorHandle::default();
                    let self_rank = self.objc_handle.rank as usize;
                    let other_rank = other.objc_handle.rank as usize;
                    if self_rank >= 2 && other_rank >= 2 {
                        result_handle.rank = self_rank as u32;
                        for i in 0..self_rank.saturating_sub(2) {
                            result_handle.shape[i] = self.objc_handle.shape[i];
                        }
                        result_handle.shape[self_rank - 2] = self.objc_handle.shape[self_rank - 2];
                        result_handle.shape[self_rank - 1] =
                            other.objc_handle.shape[other_rank - 1];
                    }
                    Ok(Self {
                        swift_handle: result,
                        objc_handle: result_handle,
                        bridge_type: TensorBridgeType::Swift,
                    })
                }
                TensorBridgeType::ObjCpp => {
                    // ObjC++ doesn't have v2 API, fall back to regular matmul
                    self.matmul(other)
                }
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = (other, compute_units);
            Err(AosError::Kernel(
                "MLTensor only available on macOS".to_string(),
            ))
        }
    }

    /// Materialize tensor to Vec<f32>
    ///
    /// # Errors
    /// Returns `AosError::Kernel` if materialization fails
    pub fn to_vec(&self) -> Result<Vec<f32>> {
        #[cfg(target_os = "macos")]
        {
            match self.bridge_type {
                TensorBridgeType::Swift => {
                    // For Swift bridge, we need to get the size first
                    // We'll use a reasonable max size and let the FFI return actual count
                    let num_elements = self.num_elements();
                    if num_elements == 0 {
                        return Ok(Vec::new());
                    }

                    let mut output = vec![0.0f32; num_elements];
                    let result = unsafe {
                        ffi::swift_coreml_tensor_to_floats(
                            self.swift_handle,
                            output.as_mut_ptr(),
                            num_elements,
                        )
                    };

                    if result < 0 {
                        return Err(AosError::Kernel(format!(
                            "Failed to materialize tensor: error code {}",
                            result
                        )));
                    }

                    Ok(output)
                }
                TensorBridgeType::ObjCpp => {
                    let num_elements = self.objc_handle.num_elements();
                    if num_elements == 0 {
                        return Ok(Vec::new());
                    }

                    let mut output = vec![0.0f32; num_elements];
                    let result = unsafe {
                        ffi::coreml_tensor_to_floats(
                            self.objc_handle,
                            output.as_mut_ptr(),
                            num_elements,
                        )
                    };

                    if result < 0 {
                        return Err(AosError::Kernel(format!(
                            "Failed to materialize tensor: error code {}",
                            result
                        )));
                    }

                    Ok(output)
                }
            }
        }

        #[cfg(not(target_os = "macos"))]
        Err(AosError::Kernel(
            "MLTensor only available on macOS".to_string(),
        ))
    }

    /// Materialize tensor using async API (macOS 26+ optimized)
    ///
    /// On macOS 26+ (Tahoe), this uses the async `shapedArray(of:)` API for
    /// better integration with the compute pipeline. On earlier versions,
    /// falls back to synchronous materialization.
    ///
    /// # Arguments
    /// * `use_async` - If true on macOS 26+, uses async materialization
    ///
    /// # Errors
    /// Returns `AosError::Kernel` if materialization fails
    pub fn to_vec_async(&self, use_async: bool) -> Result<Vec<f32>> {
        #[cfg(target_os = "macos")]
        {
            match self.bridge_type {
                TensorBridgeType::Swift => {
                    let num_elements = self.num_elements();
                    if num_elements == 0 {
                        return Ok(Vec::new());
                    }

                    let mut output = vec![0.0f32; num_elements];
                    let result = unsafe {
                        ffi::swift_coreml_tensor_to_floats_v2(
                            self.swift_handle,
                            output.as_mut_ptr(),
                            num_elements,
                            use_async,
                        )
                    };

                    if result < 0 {
                        return Err(AosError::Kernel(format!(
                            "Failed to materialize tensor: error code {}",
                            result
                        )));
                    }

                    Ok(output)
                }
                TensorBridgeType::ObjCpp => {
                    // ObjC++ doesn't have async API, fall back to regular to_vec
                    self.to_vec()
                }
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = use_async;
            Err(AosError::Kernel(
                "MLTensor only available on macOS".to_string(),
            ))
        }
    }

    /// Get the shape of the tensor
    #[cfg(target_os = "macos")]
    pub fn shape(&self) -> Vec<usize> {
        match self.bridge_type {
            TensorBridgeType::Swift => {
                // For Swift tensors, we store shape in objc_handle as a cache
                // This is populated during creation
                let rank = self.objc_handle.rank as usize;
                self.objc_handle.shape[..rank].to_vec()
            }
            TensorBridgeType::ObjCpp => {
                let rank = self.objc_handle.rank as usize;
                self.objc_handle.shape[..rank].to_vec()
            }
        }
    }

    /// Get the total number of elements
    #[cfg(target_os = "macos")]
    pub fn num_elements(&self) -> usize {
        match self.bridge_type {
            TensorBridgeType::Swift => {
                // Use cached shape from objc_handle
                self.objc_handle.num_elements()
            }
            TensorBridgeType::ObjCpp => self.objc_handle.num_elements(),
        }
    }
}

impl Drop for MLTensor {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        {
            match self.bridge_type {
                TensorBridgeType::Swift => {
                    if !self.swift_handle.is_null() {
                        unsafe { ffi::swift_coreml_tensor_free(self.swift_handle) }
                    }
                }
                TensorBridgeType::ObjCpp => {
                    if self.objc_handle.is_valid() {
                        unsafe { ffi::coreml_tensor_free(self.objc_handle) }
                    }
                }
            }
        }
    }
}

/// ANE availability status
#[derive(Debug, Clone)]
pub struct AneStatus {
    pub available: bool,
    pub generation: Option<u8>,
    pub max_batch_size: usize,
    pub deterministic: bool,
}

/// Memory baseline statistics for anomaly detection (Welford's algorithm)
#[derive(Debug, Clone, Default)]
struct MemoryBaseline {
    mean: f64,
    m2: f64,
    count: usize,
}

impl MemoryBaseline {
    fn update(&mut self, value: f64) {
        self.count += 1;
        let delta = value - self.mean;
        self.mean += delta / self.count as f64;
        let delta2 = value - self.mean;
        self.m2 += delta * delta2;
    }

    fn stddev(&self) -> f64 {
        if self.count < 2 {
            0.0
        } else {
            (self.m2 / (self.count - 1) as f64).sqrt()
        }
    }

    fn z_score(&self, value: f64) -> f64 {
        let std = self.stddev();
        if std == 0.0 {
            0.0
        } else {
            (value - self.mean) / std
        }
    }
}

/// CoreML adapter artifact semantics for hot-swap (PRD 3).
///
/// - `SidecarDelta`: base CoreML package stays resident; LoRA deltas are
///   attached/detached at runtime without recompiling.
/// - `FusedPackage`: pre-fused `.mlmodelc` produced by the export pipeline; the
///   backend can switch to this compiled bundle without restarting the process.
#[derive(Debug, Clone)]
pub enum CoreMLAdapterArtifact {
    SidecarDelta {
        /// Number of floats held in-memory for this adapter.
        len: usize,
        /// Provenance for observability and routing decisions.
        source: CoreMLAdapterSource,
    },
    FusedPackage {
        /// Path to the compiled CoreML bundle that already contains the adapter.
        model_path: PathBuf,
        /// Optional hash of the compiled bundle for identity tracking.
        model_hash: Option<B3Hash>,
    },
}

/// Source of CoreML adapter payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreMLAdapterSource {
    /// Safetensors sidecar from the canonical segment (default hot-swap path).
    CanonicalSidecar,
    /// CoreML-specific segment (fused/sidecar emitted by PRD 3 pipeline).
    CoremlSegment,
}

/// CoreML backend for Neural Engine acceleration.
///
/// Hot-swap semantics:
/// - Adapters can arrive as sidecar LoRA deltas or pre-fused CoreML bundles.
///   Sidecars stay in `adapter_cache` and are toggled via `attach_adapter` /
///   `detach_adapter` without recompiling the base package.
/// - Fused packages are registered in `adapter_artifacts` and activated through
///   `switch_adapter`, which swaps the compiled bundle in place via
///   `load_model_internal` **without** overwriting the recorded `base_model_path`.
/// - `detach_adapter` removes the adapter from memory and, if a fused bundle was
///   active, restores the base model handle from `base_model_path` (files on disk
///   are never deleted).
/// - `active_fused_adapter` tracks the last fused slot so callers can restore the
///   base package deterministically after a fused-hop.
///
/// This backend automatically detects and uses MLTensor operations when available
/// (macOS 15+) for better performance. The `use_mltensor` field indicates whether
/// MLTensor is supported and will be used for inference operations.
pub struct CoreMLBackend {
    model_handle: *mut std::ffi::c_void,
    model_hash: Option<B3Hash>,
    compute_units: ComputeUnits,
    ane_status: AneStatus,
    device_name: String,
    metrics: BackendMetrics,
    adapter_cache: HashMap<u16, Vec<f32>>,
    gpu_fingerprints: HashMap<u32, GpuBufferFingerprint>,
    /// Memory baselines per adapter for anomaly detection
    memory_baselines: RwLock<HashMap<u16, MemoryBaseline>>,
    /// Whether production mode is enabled (requires ANE-only)
    production_mode: bool,
    /// Whether MLTensor API is available (macOS 15+)
    /// When true, the backend will use MLTensor operations for better performance
    use_mltensor: bool,
    /// MLTensor API version (macOS 15+; Tahoe required for deterministic policies)
    mltensor_api_version: MltensorApiVersion,
    /// Which tensor bridge implementation is being used
    tensor_bridge: TensorBridgeType,
    /// Model-specific parameters (optional override from config.json)
    /// If None, defaults to Qwen2.5-7B parameters
    model_params: Option<CoreMLModelParams>,
    /// Base CoreML model path used to restore after fused switches.
    base_model_path: Option<PathBuf>,
    /// Adapter artifacts keyed by adapter slot (sidecar vs fused bundle).
    adapter_artifacts: HashMap<u16, CoreMLAdapterArtifact>,
    /// Currently attached adapters for routing (sidecar path).
    attached_adapters: HashSet<u16>,
    /// Active fused adapter slot (if switched to a fused bundle).
    active_fused_adapter: Option<u16>,
    /// Optional CoreML placement spec loaded from the manifest.
    placement_spec: Option<CoreMLPlacementSpec>,
    /// Placement resolution against the loaded CoreML graph.
    placement_resolution: Option<PlacementResolution>,
    /// MoE configuration (if loaded model is an MoE model)
    moe_config: Option<MoEConfig>,
    /// MoE adapter cache keyed by adapter slot
    moe_adapter_cache: HashMap<u16, MoEAdapterWeights>,
}

unsafe impl Send for CoreMLBackend {}
unsafe impl Sync for CoreMLBackend {}

impl CoreMLBackend {
    #[inline]
    fn gate_q15_to_f32(gate: i16) -> f32 {
        gate as f32 / 32767.0
    }

    #[cfg(any(test, debug_assertions, feature = "coreml-stub"))]
    /// Create a new CoreML backend in stub mode
    ///
    /// This constructor creates a backend that operates in stub/fallback mode,
    /// allowing the crate to be used for testing and development when the native
    /// FFI bridge (coreml_bridge.mm) is not available.
    ///
    /// # Arguments
    /// * `compute_units` - The compute units configuration (for compatibility)
    ///
    /// # Note
    /// In stub mode, `run_step()` will:
    /// - Generate deterministic dummy logits for testing
    /// - Apply LoRA adapter fusion using the adapter cache
    /// - Update position counters correctly
    ///
    /// This allows development and testing of the full inference pipeline
    /// while the native FFI implementation is in progress.
    pub fn new_stub(compute_units: ComputeUnits) -> Result<Self> {
        let ane_status = AneStatus {
            available: false,
            generation: None,
            max_batch_size: 1,
            deterministic: true, // Stub is deterministic
        };

        let device_name = "CoreML (Stub Mode)".to_string();

        tracing::info!(
            device = %device_name,
            compute_units = ?compute_units,
            "Initialized CoreML backend in stub mode (native FFI not available)"
        );

        Ok(Self {
            model_handle: std::ptr::null_mut(),
            model_hash: None,
            compute_units,
            ane_status,
            device_name,
            metrics: BackendMetrics::default(),
            adapter_cache: HashMap::new(),
            gpu_fingerprints: HashMap::new(),
            memory_baselines: RwLock::new(HashMap::new()),
            production_mode: false,
            use_mltensor: false,
            mltensor_api_version: MltensorApiVersion::NotAvailable,
            tensor_bridge: TensorBridgeType::ObjCpp,
            model_params: None,
            base_model_path: None,
            adapter_artifacts: HashMap::new(),
            attached_adapters: HashSet::new(),
            active_fused_adapter: None,
            placement_spec: None,
            placement_resolution: None,
            moe_config: None,
            moe_adapter_cache: HashMap::new(),
        })
    }

    /// Check if this backend is operating in stub mode
    #[cfg(any(test, debug_assertions, feature = "coreml-stub"))]
    pub fn is_stub_mode(&self) -> bool {
        self.model_handle.is_null() && self.device_name.contains("Stub")
    }

    /// Stub mode is disabled in release/production builds
    #[cfg(not(any(test, debug_assertions, feature = "coreml-stub")))]
    #[inline]
    pub fn is_stub_mode(&self) -> bool {
        false
    }

    /// Create a new CoreML backend
    ///
    /// # Arguments
    /// * `compute_units` - The compute units to use for inference
    /// * `production_mode` - If true, enforces ANE-only mode for guaranteed determinism
    ///
    /// # Note
    /// Native CoreML FFI is required; stub mode is only compiled for debug/tests
    /// (or when `coreml-stub` feature is explicitly enabled) and is not available
    /// in release/production builds.
    pub fn new(compute_units: ComputeUnits, production_mode: bool) -> Result<Self> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (compute_units, production_mode); // Suppress unused warning
            return Err(AosError::Kernel(
                "CoreML backend only available on macOS".to_string(),
            ));
        }

        #[cfg(target_os = "macos")]
        {
            let is_available = unsafe { ffi::coreml_is_available() };
            if !is_available {
                return Err(AosError::Kernel(
                    "CoreML framework not available".to_string(),
                ));
            }

            let ane_status = Self::check_ane_status()?;

            // In production mode, require ANE to be available
            if production_mode && !ane_status.available {
                return Err(AosError::Kernel(
                    "Production mode requires ANE to be available for guaranteed determinism"
                        .to_string(),
                ));
            }

            // In production mode, enforce ANE-only compute units
            let effective_compute_units = if production_mode {
                if !matches!(
                    compute_units,
                    ComputeUnits::CpuAndNeuralEngine | ComputeUnits::CpuOnly
                ) {
                    tracing::warn!(
                        requested = ?compute_units,
                        enforced = ?ComputeUnits::CpuAndNeuralEngine,
                        "Production mode requires ANE-only compute units, overriding configuration"
                    );
                }
                ComputeUnits::CpuAndNeuralEngine
            } else {
                compute_units
            };

            let device_name = if ane_status.available {
                format!("CoreML (ANE Gen {})", ane_status.generation.unwrap_or(0))
            } else {
                "CoreML (GPU/CPU)".to_string()
            };

            // Check if MLTensor API is available (macOS 15+)
            let mltensor_api_version = get_mltensor_api_version();
            let mut use_mltensor = unsafe { ffi::coreml_supports_mltensor() };

            if production_mode && use_mltensor && mltensor_api_version != MltensorApiVersion::Tahoe
            {
                use_mltensor = false;
                tracing::warn!(
                    mltensor_api_version = ?mltensor_api_version,
                    "Disabling MLTensor in production mode; deterministic ANE scheduling requires macOS 26+"
                );
            }

            // Determine which bridge to use
            let tensor_bridge = if swift_bridge_available() {
                TensorBridgeType::Swift
            } else {
                TensorBridgeType::ObjCpp
            };

            tracing::info!(
                device = %device_name,
                ane_available = ane_status.available,
                compute_units = ?effective_compute_units,
                production_mode = production_mode,
                use_mltensor = use_mltensor,
                mltensor_api_version = ?mltensor_api_version,
                tensor_bridge = ?tensor_bridge,
                "Initialized CoreML backend"
            );

            Ok(Self {
                model_handle: std::ptr::null_mut(),
                model_hash: None,
                compute_units: effective_compute_units,
                ane_status,
                device_name,
                metrics: BackendMetrics::default(),
                adapter_cache: HashMap::new(),
                gpu_fingerprints: HashMap::new(),
                memory_baselines: RwLock::new(HashMap::new()),
                production_mode,
                use_mltensor,
                mltensor_api_version,
                tensor_bridge,
                model_params: None,
                base_model_path: None,
                adapter_artifacts: HashMap::new(),
                attached_adapters: HashSet::new(),
                active_fused_adapter: None,
                placement_spec: None,
                placement_resolution: None,
                moe_config: None,
                moe_adapter_cache: HashMap::new(),
            })
        }
    }

    /// Create a new CoreML backend with default (non-production) mode
    ///
    /// This is a convenience constructor for development/testing.
    pub fn new_default(compute_units: ComputeUnits) -> Result<Self> {
        Self::new(compute_units, false)
    }

    /// Set model-specific parameters for inference
    ///
    /// This allows configuring the backend with parameters from the model's config.json
    /// file, enabling correct attention head dimensions, GQA group sizes, etc.
    ///
    /// # Example
    /// ```ignore
    /// use adapteros_lora_kernel_coreml::{CoreMLBackend, CoreMLModelParams, ComputeUnits};
    ///
    /// let mut backend = CoreMLBackend::new(ComputeUnits::CpuAndNeuralEngine, true)?;
    /// backend.set_model_params(CoreMLModelParams::new(
    ///     3584,      // hidden_size
    ///     28,        // num_attention_heads
    ///     4,         // num_key_value_heads
    ///     18944,     // intermediate_size
    ///     1000000.0, // rope_theta
    ///     32768,     // max_seq_len
    /// ));
    /// ```
    pub fn set_model_params(&mut self, params: CoreMLModelParams) {
        tracing::info!(
            hidden_size = params.hidden_size,
            num_attention_heads = params.num_attention_heads,
            num_key_value_heads = params.num_key_value_heads,
            head_dim = params.head_dim(),
            kv_groups = params.kv_groups(),
            "Setting CoreML model parameters"
        );
        self.model_params = Some(params);
    }

    /// Resolve and record CoreML placement spec against a logical graph.
    pub fn apply_placement_spec(
        &mut self,
        graph: &CoreMLGraph,
        spec: CoreMLPlacementSpec,
    ) -> PlacementMetrics {
        let resolution = resolve_placement(graph, &spec);
        let metrics = resolution.metrics();

        if metrics.missing > 0 {
            tracing::warn!(
                missing = metrics.missing,
                resolved = metrics.resolved,
                "CoreML placement: some bindings did not resolve"
            );
        }
        if metrics.shape_mismatches > 0 {
            tracing::warn!(
                mismatches = metrics.shape_mismatches,
                resolved = metrics.resolved,
                "CoreML placement: binding shapes differ from graph dims"
            );
        }

        self.placement_resolution = Some(resolution);
        self.placement_spec = Some(spec);
        metrics
    }

    /// Convenience helper: import graph from disk and resolve placement.
    pub fn import_graph_and_apply(
        &mut self,
        model_path: impl AsRef<Path>,
        spec: CoreMLPlacementSpec,
    ) -> Result<PlacementMetrics> {
        let graph = CoreMLGraph::from_package(model_path)?;
        Ok(self.apply_placement_spec(&graph, spec))
    }

    /// Dump the resolved placement map for debugging.
    pub fn dump_placement_map(&self) -> Option<String> {
        self.placement_resolution.as_ref().map(|r| r.dump())
    }

    /// Placement metrics (if a spec has been applied).
    pub fn placement_metrics(&self) -> Option<PlacementMetrics> {
        self.placement_resolution.as_ref().map(|r| r.metrics())
    }

    /// Get current model parameters
    ///
    /// Returns the configured model parameters, or defaults if not set.
    pub fn model_params(&self) -> CoreMLModelParams {
        self.model_params.clone().unwrap_or_default()
    }

    #[cfg(target_os = "macos")]
    fn check_ane_status() -> Result<AneStatus> {
        let result = unsafe { ffi::coreml_check_ane() };

        Ok(AneStatus {
            available: result.available,
            generation: if result.generation > 0 {
                Some(result.generation)
            } else {
                None
            },
            max_batch_size: if result.available { 128 } else { 1 },
            deterministic: result.available,
        })
    }

    /// Load CoreML model from .mlpackage or .mlmodelc
    pub fn load_model(&mut self, model_path: &Path) -> Result<()> {
        self.load_model_internal(model_path, true)
    }

    fn load_model_internal(&mut self, model_path: &Path, update_base: bool) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            // Unload any previously loaded model before switching handles.
            if !self.model_handle.is_null() {
                unsafe { ffi::coreml_unload_model(self.model_handle) };
                self.model_handle = std::ptr::null_mut();
            }

            // Remember the base model path when requested.
            if update_base {
                self.base_model_path = Some(model_path.to_path_buf());
            }

            let (hash_path, load_path) = if model_path.is_dir() {
                (model_path.join("Manifest.json"), model_path.to_path_buf())
            } else {
                (model_path.to_path_buf(), model_path.to_path_buf())
            };

            let model_bytes = std::fs::read(&hash_path)
                .map_err(|e| AosError::Io(format!("Failed to read model: {}", e)))?;
            self.model_hash = Some(B3Hash::hash(&model_bytes));

            let compiled_path = Self::compile_model_if_needed(&load_path)?;
            let path_str = compiled_path.to_string_lossy();
            let compute_unit_int = match self.compute_units {
                ComputeUnits::CpuOnly => 0,
                ComputeUnits::CpuAndGpu => 1,
                ComputeUnits::CpuAndNeuralEngine => 2,
                ComputeUnits::All => 3,
            };

            let handle = unsafe {
                ffi::coreml_load_model(
                    path_str.as_ptr() as *const i8,
                    path_str.len(),
                    compute_unit_int,
                )
            };

            if handle.is_null() {
                let mut err_buf = [0i8; 512];
                let len =
                    unsafe { ffi::coreml_get_last_error(err_buf.as_mut_ptr(), err_buf.len()) };
                let reason = if len > 0 {
                    let slice = &err_buf[..len.min(err_buf.len())];
                    unsafe { CStr::from_ptr(slice.as_ptr()) }
                        .to_string_lossy()
                        .into_owned()
                } else {
                    "Failed to load CoreML model".to_string()
                };
                return Err(AosError::Kernel(reason));
            }

            self.model_handle = handle;

            tracing::info!(
                model_path = %load_path.display(),
                compiled_path = %compiled_path.display(),
                hash = %self.model_hash.as_ref().unwrap().to_short_hex(),
                hash_source = %hash_path.display(),
                "Loaded CoreML model"
            );

            Ok(())
        }

        #[cfg(not(target_os = "macos"))]
        Err(AosError::Kernel("CoreML not available".to_string()))
    }

    fn restore_base_model_if_needed(&mut self) -> Result<()> {
        if let Some(base_path) = self.base_model_path.clone() {
            if self.active_fused_adapter.is_some() {
                tracing::info!(
                    adapter = ?self.active_fused_adapter,
                    path = %base_path.display(),
                    "Switching CoreML backend back to base model after fused adapter"
                );
                self.load_model(&base_path)?;
                self.active_fused_adapter = None;
            }
        } else {
            tracing::debug!("No base model path recorded; skip restore");
        }
        Ok(())
    }

    /// Register a fused CoreML package for an adapter using fusion metadata.
    /// Verifies the metadata hashes on disk before recording.
    pub fn register_fused_adapter_from_metadata(
        &mut self,
        id: u16,
        metadata_path: &Path,
    ) -> Result<()> {
        let metadata = validate_coreml_fusion(metadata_path)?;

        if let Some(current_base) = self.model_hash {
            if current_base != metadata.base_manifest_hash {
                return Err(AosError::Validation(format!(
                    "Base manifest hash mismatch for fused adapter {}: expected {}, got {}",
                    id,
                    current_base.to_short_hex(),
                    metadata.base_manifest_hash.to_short_hex()
                )));
            }
        }

        self.adapter_artifacts.insert(
            id,
            CoreMLAdapterArtifact::FusedPackage {
                model_path: metadata.fused_package.clone(),
                model_hash: Some(metadata.fused_manifest_hash),
            },
        );

        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn compile_model_if_needed(model_path: &Path) -> Result<PathBuf> {
        use adapteros_platform::common::PlatformUtils;

        if model_path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("mlmodelc"))
            .unwrap_or(false)
        {
            return Ok(model_path.to_path_buf());
        }

        let hash = B3Hash::hash(model_path.to_string_lossy().as_bytes()).to_hex();
        let cache_dir = PlatformUtils::temp_dir()
            .join("adapteros-coremlc")
            .join(hash);

        if let Some(compiled) = Self::find_compiled_model(&cache_dir)? {
            return Ok(compiled);
        }

        std::fs::create_dir_all(&cache_dir).map_err(|e| {
            AosError::Io(format!(
                "Failed to create CoreML compile cache at {}: {}",
                cache_dir.display(),
                e
            ))
        })?;

        let model_str = model_path
            .to_str()
            .ok_or_else(|| AosError::Io(format!("Invalid model path: {}", model_path.display())))?;
        let cache_str = cache_dir
            .to_str()
            .ok_or_else(|| AosError::Io(format!("Invalid cache path: {}", cache_dir.display())))?;

        let status = Command::new("xcrun")
            .args(["coremlc", "compile", model_str, cache_str])
            .status()
            .map_err(|e| AosError::Kernel(format!("Failed to spawn coremlc: {}", e)))?;

        if !status.success() {
            return Err(AosError::Kernel(format!(
                "coremlc compile failed (status {:?}) for {}",
                status.code(),
                model_path.display()
            )));
        }

        Self::find_compiled_model(&cache_dir)?.ok_or_else(|| {
            AosError::Kernel(format!(
                "coremlc compile produced no .mlmodelc under {}",
                cache_dir.display()
            ))
        })
    }

    #[cfg(target_os = "macos")]
    fn find_compiled_model(dir: &Path) -> Result<Option<PathBuf>> {
        if !dir.exists() {
            return Ok(None);
        }

        let entries = std::fs::read_dir(dir).map_err(|e| {
            AosError::Io(format!(
                "Failed to read compiled model dir {}: {}",
                dir.display(),
                e
            ))
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                AosError::Io(format!("Failed to read entry in {}: {}", dir.display(), e))
            })?;
            let path = entry.path();
            if path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("mlmodelc"))
                .unwrap_or(false)
            {
                return Ok(Some(path));
            }
        }

        Ok(None)
    }

    pub fn set_compute_units(&mut self, units: ComputeUnits) {
        self.compute_units = units;
    }

    pub fn ane_status(&self) -> &AneStatus {
        &self.ane_status
    }

    /// Check if MLTensor API is supported (macOS 15+)
    ///
    /// When true, the backend will automatically use MLTensor operations
    /// instead of raw MLMultiArray for better performance.
    pub fn supports_mltensor(&self) -> bool {
        self.use_mltensor
    }

    /// Get the tensor bridge type being used
    ///
    /// Returns which bridge implementation (Swift or ObjC++) is being used
    /// for tensor operations. Swift bridge provides better performance on macOS 15+.
    pub fn tensor_bridge(&self) -> TensorBridgeType {
        self.tensor_bridge
    }

    /// Async prediction using CoreML's async API
    ///
    /// This method bridges the C callback-based async API to Rust's async/await
    /// using a oneshot channel. The callback is invoked on a CoreML dispatch queue
    /// when the prediction completes.
    ///
    /// # Arguments
    /// * `input_ids` - Input token IDs for the model
    ///
    /// # Returns
    /// A Future that resolves to the output logits on success
    ///
    /// # Errors
    /// - `AosError::Kernel` if the model is not loaded
    /// - `AosError::Kernel` if prediction fails
    /// - `AosError::Internal` if the channel is closed unexpectedly
    #[cfg(target_os = "macos")]
    pub async fn predict_async(&self, input_ids: &[u32]) -> Result<Vec<f32>> {
        if self.model_handle.is_null() {
            return Err(AosError::Kernel("Model not loaded".to_string()));
        }

        let (tx, rx) = oneshot::channel::<Result<Vec<f32>>>();
        let tx_ptr = Box::into_raw(Box::new(tx));

        /// Callback function invoked by CoreML when async prediction completes
        extern "C" fn prediction_callback(
            status: i32,
            output: *mut f32,
            output_len: usize,
            user_data: *mut c_void,
        ) {
            // SAFETY: user_data was created from Box::into_raw and is only used once
            let tx = unsafe { Box::from_raw(user_data as *mut oneshot::Sender<Result<Vec<f32>>>) };

            let result = if status == 0 {
                if output.is_null() || output_len == 0 {
                    Err(AosError::Kernel(
                        "Prediction returned empty output".to_string(),
                    ))
                } else {
                    // SAFETY: CoreML guarantees output is valid for output_len elements
                    let output_slice = unsafe { std::slice::from_raw_parts(output, output_len) };
                    let result = output_slice.to_vec();
                    // Free the malloc'd buffer from C (allocated in coreml_bridge.mm)
                    unsafe {
                        libc::free(output as *mut std::ffi::c_void);
                    }
                    Ok(result)
                }
            } else {
                Err(AosError::Kernel(format!(
                    "Async prediction failed with status {}",
                    status
                )))
            };

            // Send result through channel (ignore error if receiver dropped)
            let _ = tx.send(result);
        }

        // Initiate async prediction
        // Note: coreml_predict_async returns void; errors are reported via callback
        unsafe {
            ffi::coreml_predict_async(
                self.model_handle,
                input_ids.as_ptr(),
                input_ids.len(),
                prediction_callback,
                tx_ptr as *mut c_void,
            )
        };

        // Await the result from the callback
        rx.await
            .map_err(|_| AosError::Internal("Prediction channel closed unexpectedly".to_string()))?
    }

    /// Async prediction (non-macOS stub)
    #[cfg(not(target_os = "macos"))]
    pub async fn predict_async(&self, _input_ids: &[u32]) -> Result<Vec<f32>> {
        Err(AosError::Kernel("CoreML not available".to_string()))
    }

    /// MLTensor inference path (macOS 15+)
    ///
    /// Uses MLTensor operations for adapter fusion, providing better performance
    /// through CoreML's optimized tensor operations. On macOS 26+ (Tahoe), uses
    /// enhanced APIs with compute unit preference for ANE acceleration.
    #[cfg(target_os = "macos")]
    fn run_step_mltensor(&self, io: &mut IoBuffers, indices: &[u16], gates: &[i16]) -> Result<i32> {
        // Step 1: Run base model inference to get initial logits
        let base_result = unsafe {
            ffi::coreml_run_inference(
                self.model_handle,
                io.input_ids.as_ptr(),
                io.input_ids.len(),
                io.output_logits.as_mut_ptr(),
                io.output_logits.len(),
                std::ptr::null(), // No adapter indices for base pass
                std::ptr::null(), // No gates for base pass
                0,
            )
        };

        if base_result != 0 {
            return Ok(base_result);
        }

        // Step 2: Apply adapter fusion using MLTensor operations
        // Check for macOS 26+ enhanced APIs (Tahoe)
        let use_enhanced_api = has_enhanced_api();

        // Per-operation compute unit scheduling based on operation characteristics:
        // - TensorOp (data movement): Let CoreML decide optimal placement
        // - MatMul: ANE-optimized (high throughput matrix operations)
        // - Softmax: GPU-preferred (transcendental functions more efficient on GPU)
        // - ElementWise (scale, add): GPU-preferred in dev mode
        // In production mode, all operations use ANE for determinism
        let tensor_compute_units =
            OperationType::TensorOp.preferred_compute_units(self.production_mode);
        // Ready for use when element-wise ops with compute unit selection are added
        let _elementwise_compute_units =
            OperationType::ElementWise.preferred_compute_units(self.production_mode);
        // Ready for use when matmul ops in adapter fusion are added
        let _matmul_compute_units =
            OperationType::MatMul.preferred_compute_units(self.production_mode);

        tracing::trace!(
            production_mode = self.production_mode,
            tensor_compute_units = ?tensor_compute_units,
            elementwise_compute_units = ?_elementwise_compute_units,
            matmul_compute_units = ?_matmul_compute_units,
            "Per-operation ANE scheduling: {} mode",
            if self.production_mode { "production (ANE-only)" } else { "development (mixed)" }
        );

        // Create tensor from base logits using appropriate API
        let logits_shape = &[1, io.output_logits.len()];
        let mut base_logits = if use_enhanced_api {
            // macOS 26+: Use v2 API with per-operation compute unit preference
            MLTensor::from_floats_with_compute_units(
                &io.output_logits,
                logits_shape,
                tensor_compute_units,
            )?
        } else {
            // macOS 15-25: Standard MLTensor creation
            MLTensor::from_floats(&io.output_logits, logits_shape)?
        };

        // Step 3: Apply adapter fusion
        // For each active adapter, compute: output = base + gate * adapter_delta
        for (i, &adapter_idx) in indices.iter().enumerate() {
            let gate = gates[i];
            if gate == 0 {
                continue;
            }

            // Get adapter weights from cache
            if let Some(adapter_weights) = self.adapter_cache.get(&adapter_idx) {
                // Adapter weights might be larger than logits if they include
                // intermediate layer weights. We only use the output projection.
                if adapter_weights.len() >= io.output_logits.len() {
                    // Take the last `output_logits.len()` elements as output projection
                    let output_projection =
                        &adapter_weights[adapter_weights.len() - io.output_logits.len()..];

                    // Create adapter delta tensor using appropriate API
                    // Use same compute units as base tensor for consistency in adapter fusion
                    let adapter_tensor = if use_enhanced_api {
                        MLTensor::from_floats_with_compute_units(
                            output_projection,
                            logits_shape,
                            tensor_compute_units,
                        )?
                    } else {
                        MLTensor::from_floats(output_projection, logits_shape)?
                    };

                    // Convert Q15 gate to float: gate / 32767.0 (router invariant)
                    let gate_float = Self::gate_q15_to_f32(gate);

                    // Scale adapter delta by gate value
                    let scaled_adapter = adapter_tensor.scale(gate_float)?;

                    // Add scaled adapter to base logits
                    // On macOS 26+, tensor operations automatically use ANE when possible
                    base_logits = base_logits.add(&scaled_adapter)?;
                } else {
                    tracing::warn!(
                        adapter_idx = adapter_idx,
                        adapter_len = adapter_weights.len(),
                        output_len = io.output_logits.len(),
                        "Adapter weights smaller than output size, skipping"
                    );
                }
            } else {
                tracing::debug!(adapter_idx = adapter_idx, "Adapter not in cache, skipping");
            }
        }

        // Step 4: Materialize result back to output buffer
        // On macOS 26+, use async materialization for better pipeline integration
        let result_vec = if use_enhanced_api {
            base_logits.to_vec_async(true)?
        } else {
            base_logits.to_vec()?
        };

        // Copy results to output buffer
        let copy_len = result_vec.len().min(io.output_logits.len());
        io.output_logits[..copy_len].copy_from_slice(&result_vec[..copy_len]);

        tracing::trace!(
            num_adapters = indices.len(),
            use_mltensor = true,
            use_enhanced_api = use_enhanced_api,
            compute_units = ?tensor_compute_units,
            "Completed MLTensor inference step with per-operation ANE scheduling"
        );

        Ok(0)
    }

    /// Legacy FFI path with LoRA adapters
    #[cfg(target_os = "macos")]
    fn run_step_ffi_with_lora(
        &self,
        io: &mut IoBuffers,
        indices: &[u16],
        gates: &[i16],
        ring_len: usize,
    ) -> Result<i32> {
        // Pre-compute LoRA deltas from adapter_cache for each selected adapter
        let mut lora_delta_ptrs: Vec<*const f32> = Vec::with_capacity(indices.len());
        let mut delta_lens: Vec<usize> = Vec::with_capacity(indices.len());

        for &idx in indices.iter() {
            if let Some(weights) = self.adapter_cache.get(&idx) {
                lora_delta_ptrs.push(weights.as_ptr());
                delta_lens.push(weights.len());
            } else {
                // Adapter not in cache - use null pointer with zero length
                lora_delta_ptrs.push(std::ptr::null());
                delta_lens.push(0);
            }
        }

        let result = unsafe {
            ffi::coreml_run_inference_with_lora(
                self.model_handle,
                io.input_ids.as_ptr(),
                io.input_ids.len(),
                io.output_logits.as_mut_ptr(),
                io.output_logits.len(),
                indices.as_ptr(),
                gates.as_ptr(),
                ring_len,
                lora_delta_ptrs.as_ptr(),
                delta_lens.as_ptr(),
            )
        };

        Ok(result)
    }

    /// Legacy FFI path for standard inference (no adapters)
    #[cfg(target_os = "macos")]
    fn run_step_ffi_standard(
        &self,
        io: &mut IoBuffers,
        indices: &[u16],
        gates: &[i16],
        ring_len: usize,
    ) -> Result<i32> {
        let result = unsafe {
            ffi::coreml_run_inference(
                self.model_handle,
                io.input_ids.as_ptr(),
                io.input_ids.len(),
                io.output_logits.as_mut_ptr(),
                io.output_logits.len(),
                indices.as_ptr(),
                gates.as_ptr(),
                ring_len,
            )
        };

        Ok(result)
    }

    /// Stub mode execution path for development/testing
    ///
    /// Generates deterministic logits and applies LoRA adapter fusion
    /// using the adapter cache. This enables full pipeline testing
    /// without the native FFI bridge.
    #[cfg(any(test, debug_assertions, feature = "coreml-stub"))]
    fn run_step_stub(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
        tracing::trace!(
            position = io.position,
            input_len = io.input_ids.len(),
            k = ring.k,
            "CoreML stub mode: executing inference step"
        );

        // Step 1: Generate deterministic base logits
        // Use a position-based pattern for reproducibility
        let vocab_size = io.output_logits.len();
        for (i, logit) in io.output_logits.iter_mut().enumerate() {
            // Deterministic pattern based on position and vocab index
            let position_factor = (io.position as f32 * 0.001).sin();
            let vocab_factor = (i as f32 * 0.0001).cos();
            *logit = position_factor * 0.5 + vocab_factor * 0.5;
        }

        // Step 2: Apply LoRA adapter fusion based on router decisions
        let indices = ring.active_indices();
        let gates = ring.active_gates();

        for (adapter_slot, (&adapter_idx, &gate_q15)) in
            indices.iter().zip(gates.iter()).enumerate()
        {
            // Skip inactive adapters (zero gate)
            if gate_q15 == 0 {
                continue;
            }

            // Convert Q15 gate to float: gate / 32767.0 (router invariant)
            let gate_float = Self::gate_q15_to_f32(gate_q15);

            // Apply adapter weights from cache if available
            if let Some(adapter_weights) = self.adapter_cache.get(&adapter_idx) {
                // Adapter weights might be larger than logits if they include
                // intermediate layer weights. We only use the output projection.
                let weights_to_use = if adapter_weights.len() >= vocab_size {
                    // Take the last `vocab_size` elements as output projection
                    &adapter_weights[adapter_weights.len() - vocab_size..]
                } else {
                    // Use all weights if fewer than vocab_size
                    adapter_weights.as_slice()
                };

                // Apply LoRA delta: output += gate * adapter_delta
                let apply_len = weights_to_use.len().min(vocab_size);
                for (i, &delta) in weights_to_use.iter().enumerate().take(apply_len) {
                    io.output_logits[i] += gate_float * delta;
                }

                tracing::trace!(
                    adapter_idx = adapter_idx,
                    adapter_slot = adapter_slot,
                    gate = gate_float,
                    weights_len = weights_to_use.len(),
                    "Applied LoRA adapter delta"
                );
            } else {
                // No cached weights - apply a deterministic adapter effect
                // This simulates adapter impact without actual weights
                let adapter_signature = (adapter_idx as f32 * 0.1).sin();
                for (i, logit) in io.output_logits.iter_mut().enumerate() {
                    let adapter_effect = adapter_signature * (i as f32 * 0.00001).cos() * 0.01;
                    *logit += gate_float * adapter_effect;
                }

                tracing::trace!(
                    adapter_idx = adapter_idx,
                    adapter_slot = adapter_slot,
                    gate = gate_float,
                    "Applied synthetic adapter effect (no cached weights)"
                );
            }
        }

        // Step 3: Normalize logits to softmax-like distribution for realism
        // Find max for numerical stability
        let max_logit = io
            .output_logits
            .iter()
            .cloned()
            .fold(f32::NEG_INFINITY, f32::max);

        // Apply softmax normalization
        let mut sum_exp = 0.0f32;
        for logit in io.output_logits.iter_mut() {
            *logit = (*logit - max_logit).exp();
            sum_exp += *logit;
        }

        if sum_exp > 0.0 {
            for logit in io.output_logits.iter_mut() {
                *logit /= sum_exp;
            }
        }

        // Step 4: Update state
        io.position += 1;
        self.metrics.total_operations += 1;
        self.metrics.successful_operations += 1;

        tracing::debug!(
            position = io.position,
            active_adapters = indices
                .iter()
                .zip(gates.iter())
                .filter(|(_, &g)| g != 0)
                .count(),
            vocab_size = vocab_size,
            "CoreML stub mode: inference step complete"
        );

        Ok(())
    }

    // ==================== MoE Support Methods ====================

    /// Check if the loaded model is an MoE (Mixture of Experts) model
    pub fn is_moe_model(&self) -> bool {
        self.moe_config.is_some()
    }

    /// Set MoE configuration for the loaded model
    pub fn set_moe_config(&mut self, config: MoEConfig) {
        tracing::info!(
            num_experts = config.num_experts,
            num_experts_per_token = config.num_experts_per_token,
            hidden_size = config.hidden_size,
            moe_intermediate_size = config.moe_intermediate_size,
            "Setting MoE configuration for CoreML backend"
        );
        self.moe_config = Some(config);
    }

    /// Get the current MoE configuration
    pub fn moe_config(&self) -> Option<&MoEConfig> {
        self.moe_config.as_ref()
    }

    /// Clear MoE configuration
    pub fn clear_moe_config(&mut self) {
        self.moe_config = None;
        self.moe_adapter_cache.clear();
    }

    /// Load an MoE adapter into the cache
    ///
    /// This loads the adapter weights and precomputes deltas for faster inference.
    pub fn load_moe_adapter(&mut self, adapter_id: u16, weights: MoEAdapterWeights) -> Result<()> {
        if self.moe_config.is_none() {
            return Err(AosError::Kernel(
                "Cannot load MoE adapter: no MoE configuration set".to_string(),
            ));
        }

        let mut weights = weights;
        weights.precompute_all_deltas();

        let memory_bytes = weights.memory_bytes();
        tracing::info!(
            adapter_id = adapter_id,
            num_targets = weights.targets.len(),
            memory_bytes = memory_bytes,
            strategy = ?weights.strategy,
            "Loading MoE adapter into CoreML backend"
        );

        self.moe_adapter_cache.insert(adapter_id, weights);
        Ok(())
    }

    /// Unload an MoE adapter from the cache
    pub fn unload_moe_adapter(&mut self, adapter_id: u16) -> bool {
        let removed = self.moe_adapter_cache.remove(&adapter_id).is_some();
        if removed {
            tracing::info!(adapter_id = adapter_id, "Unloaded MoE adapter");
        }
        removed
    }

    /// Get total MoE adapter memory usage
    pub fn moe_adapter_memory_bytes(&self) -> usize {
        self.moe_adapter_cache
            .values()
            .map(|w| w.memory_bytes())
            .sum()
    }

    /// Apply MoE LoRA fusion to expert outputs
    ///
    /// This implements the routing-weighted shared LoRA formula:
    /// `expert_out += (Q15_gate / 32767.0) * routing_score[e] * (alpha/rank) * delta @ x`
    ///
    /// # Arguments
    /// * `expert_outputs` - Mutable slice of expert output tensors (num_active_experts x out_features)
    /// * `expert_inputs` - Input to the expert layer (batch_size x in_features)
    /// * `routing_scores` - Expert routing scores from the router (num_active_experts,)
    /// * `adapter_id` - The adapter slot ID
    /// * `target` - Which layer target (GateProj, UpProj, DownProj)
    /// * `gate_q15` - The Q15 gate value for this adapter
    #[allow(dead_code)]
    fn apply_moe_lora_fusion(
        &self,
        expert_outputs: &mut [f32],
        expert_inputs: &[f32],
        routing_scores: &[f32],
        adapter_id: u16,
        target: MoELoRATarget,
        gate_q15: i16,
    ) -> Result<()> {
        // Get the adapter weights
        let adapter = self
            .moe_adapter_cache
            .get(&adapter_id)
            .ok_or_else(|| AosError::Kernel(format!("MoE adapter {} not loaded", adapter_id)))?;

        // Get the target weights
        let weights = adapter.targets.get(&target).ok_or_else(|| {
            AosError::Kernel(format!(
                "MoE adapter {} missing target {:?}",
                adapter_id, target
            ))
        })?;

        // Get precomputed delta (B @ A)
        let delta = weights.precomputed_delta.as_ref().ok_or_else(|| {
            AosError::Kernel(format!(
                "MoE adapter {} target {:?} missing precomputed delta",
                adapter_id, target
            ))
        })?;

        // Compute gate scale: Q15_gate / 32767.0
        let gate_scale = Self::gate_q15_to_f32(gate_q15);

        // Compute LoRA scale: alpha / rank
        let lora_scale = weights.lora_scale();

        // Check if we should use routing weights
        let use_routing = matches!(
            adapter.strategy,
            MoELoRAStrategy::RoutingWeightedShared {
                use_routing_weights: true
            }
        );

        let out_features = weights.out_features;
        let in_features = weights.in_features;

        // Apply LoRA contribution to each expert's output
        for (e, score) in routing_scores.iter().enumerate() {
            // Compute per-expert weight
            let expert_weight = if use_routing {
                gate_scale * *score * lora_scale
            } else {
                gate_scale * lora_scale
            };

            // Skip if weight is effectively zero
            if expert_weight.abs() < 1e-8 {
                continue;
            }

            // Apply delta: expert_out[e] += weight * (delta @ x)
            // delta: (out_features, in_features)
            // x: (in_features,)
            // result: (out_features,)
            let expert_out_start = e * out_features;
            let expert_out_end = expert_out_start + out_features;
            let expert_out = &mut expert_outputs[expert_out_start..expert_out_end];

            for out_idx in 0..out_features {
                let mut sum = 0.0f32;
                for in_idx in 0..in_features {
                    sum += delta[out_idx * in_features + in_idx] * expert_inputs[in_idx];
                }
                expert_out[out_idx] += expert_weight * sum;
            }
        }

        Ok(())
    }

    /// Run a step with MoE LoRA fusion (stub implementation for testing)
    #[cfg(any(test, debug_assertions, feature = "coreml-stub"))]
    #[allow(dead_code)]
    fn run_step_moe_with_lora_stub(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
        let moe_config = self.moe_config.as_ref().ok_or_else(|| {
            AosError::Kernel("MoE run_step called but no MoE config set".to_string())
        })?;

        tracing::debug!(
            num_experts = moe_config.num_experts,
            num_experts_per_token = moe_config.num_experts_per_token,
            num_adapters = self.moe_adapter_cache.len(),
            "MoE stub inference step"
        );

        // For stub mode, just run the regular stub inference
        // Real MoE LoRA fusion would happen in the native FFI layer
        self.run_step_stub(ring, io)
    }
}

impl FusedKernels for CoreMLBackend {
    fn load(&mut self, plan_bytes: &[u8]) -> Result<()> {
        let model_path_str = std::str::from_utf8(plan_bytes)
            .map_err(|_| AosError::Kernel("Invalid plan bytes encoding".to_string()))?;

        let model_path = PathBuf::from(model_path_str.trim());
        // Reset adapter state when a new base model is loaded.
        self.base_model_path = Some(model_path.clone());
        self.active_fused_adapter = None;
        self.adapter_artifacts.clear();
        self.attached_adapters.clear();
        self.adapter_cache.clear();
        self.load_model(&model_path)
    }

    fn run_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
        let model_hash_snapshot = self.model_hash;

        #[cfg(any(test, debug_assertions, feature = "coreml-stub"))]
        {
            // Check for stub mode first - this works on all platforms
            if self.is_stub_mode() {
                let result = self.run_step_stub(ring, io);
                if cfg!(debug_assertions) {
                    debug_assert_eq!(
                        model_hash_snapshot,
                        self.model_hash,
                        "CoreML base model hash changed in stub path; base parameters must stay immutable"
                    );
                }
                return result;
            }
        }

        #[cfg(not(any(test, debug_assertions, feature = "coreml-stub")))]
        {
            debug_assert!(
                !self.is_stub_mode(),
                "CoreML stub mode is disabled in release builds"
            );
        }

        #[cfg(target_os = "macos")]
        {
            if self.model_handle.is_null() {
                return Err(AosError::Kernel("Model not loaded".to_string()));
            }

            let indices = ring.active_indices();
            let gates = ring.active_gates();

            // Check if there are any adapters with non-zero gates
            let has_active_adapters = gates.iter().any(|&g| g != 0);

            // Decide between MLTensor path (macOS 15+) and legacy FFI path
            let result = if self.use_mltensor && has_active_adapters {
                // MLTensor path: Use high-level tensor operations for adapter fusion
                self.run_step_mltensor(io, indices, gates)?
            } else if has_active_adapters {
                // Legacy FFI path with LoRA adapters
                self.run_step_ffi_with_lora(io, indices, gates, ring.len())?
            } else {
                // No adapters active - use standard inference (FFI)
                self.run_step_ffi_standard(io, indices, gates, ring.len())?
            };

            if result != 0 {
                return Err(AosError::Kernel(format!(
                    "CoreML inference failed with code {}",
                    result
                )));
            }

            io.position += 1;
            self.metrics.total_operations += 1;
            self.metrics.successful_operations += 1;

            if cfg!(debug_assertions) {
                debug_assert_eq!(
                    model_hash_snapshot,
                    self.model_hash,
                    "CoreML base model hash changed during inference; base parameters must stay immutable"
                );
            }

            Ok(())
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = (ring, io);
            Err(AosError::Kernel("CoreML not available".to_string()))
        }
    }

    fn device_name(&self) -> &str {
        &self.device_name
    }

    fn attest_determinism(&self) -> Result<attestation::DeterminismReport> {
        // Determinism requires:
        // 1. ANE to be available and used
        // 2. ANE-only compute units (CpuAndNeuralEngine or CpuOnly)
        // 3. MLTensor adapter fusion only when deterministic compute policy is available
        // In production mode, we enforce ANE-only compute units.
        let using_ane_only = matches!(
            self.compute_units,
            ComputeUnits::CpuAndNeuralEngine | ComputeUnits::CpuOnly
        );

        let mltensor_deterministic = if !self.use_mltensor {
            true
        } else {
            self.production_mode && self.mltensor_api_version == MltensorApiVersion::Tahoe
        };

        let deterministic = self.ane_status.available
            && self.ane_status.deterministic
            && using_ane_only
            && mltensor_deterministic;

        let rng_seed_method = if deterministic {
            attestation::RngSeedingMethod::HkdfSeeded
        } else {
            attestation::RngSeedingMethod::SystemEntropy
        };

        let floating_point_mode = if deterministic {
            attestation::FloatingPointMode::Deterministic
        } else {
            attestation::FloatingPointMode::Unknown
        };

        // Log warning if in production mode but not deterministic (shouldn't happen due to checks in new())
        if self.production_mode && !deterministic {
            tracing::error!(
                ane_available = self.ane_status.available,
                ane_deterministic = self.ane_status.deterministic,
                using_ane_only = using_ane_only,
                use_mltensor = self.use_mltensor,
                mltensor_api_version = ?self.mltensor_api_version,
                mltensor_deterministic = mltensor_deterministic,
                "Production mode backend is not deterministic - this should not happen"
            );
        }

        Ok(attestation::DeterminismReport {
            backend_type: attestation::BackendType::CoreML,
            metallib_hash: None,
            manifest: None,
            rng_seed_method,
            floating_point_mode,
            compiler_flags: vec![],
            deterministic,
        })
    }

    fn load_adapter(&mut self, id: u16, weights: &[u8]) -> Result<()> {
        let tensors = safetensors::SafeTensors::deserialize(weights)
            .map_err(|e| AosError::Kernel(format!("Failed to parse adapter weights: {}", e)))?;

        let mut adapter_weights = Vec::new();
        for (_name, tensor) in tensors.tensors() {
            let data = tensor.data();
            let floats: Vec<f32> = data
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect();
            adapter_weights.extend(floats);
        }

        let len = adapter_weights.len();
        self.adapter_cache.insert(id, adapter_weights);
        self.adapter_artifacts.insert(
            id,
            CoreMLAdapterArtifact::SidecarDelta {
                len,
                source: CoreMLAdapterSource::CanonicalSidecar,
            },
        );
        // Default behavior: attach immediately so router can see the adapter.
        self.attached_adapters.insert(id);

        tracing::debug!(
            adapter_id = id,
            weights_len = len,
            "Loaded adapter into CoreML cache (sidecar)"
        );
        Ok(())
    }

    /// Mark a previously loaded adapter as active for routing.
    ///
    /// Sidecar path: keeps the base `.mlpackage` resident and only toggles the
    /// adapter’s entry in `attached_adapters`. Fused path uses `switch_adapter`
    /// to swap the compiled bundle instead of reloading here.
    fn attach_adapter(&mut self, id: u16) -> Result<()> {
        if !self.adapter_cache.contains_key(&id) {
            return Err(AosError::Kernel(format!(
                "Adapter {} not loaded for CoreML attach",
                id
            )));
        }
        self.attached_adapters.insert(id);
        Ok(())
    }

    /// Detach an adapter from routing and clear its cached weights.
    ///
    /// Sidecar path: removes weights from `adapter_cache`.
    /// Fused path: if the active fused slot matches `id`, we restore the base
    /// model handle via `restore_base_model_if_needed` to keep determinism with
    /// the original package.
    fn detach_adapter(&mut self, id: u16) -> Result<()> {
        self.unload_adapter(id)
    }

    /// Switch the active adapter slot.
    ///
    /// Sidecar path: evicts other adapters and ensures the base model remains
    /// loaded. Fused path: swaps to a precompiled `.mlmodelc` bundle without
    /// updating `base_model_path`, so a later detach restores the original base.
    fn switch_adapter(&mut self, id: u16) -> Result<()> {
        // Keep CoreML hot-swap semantics explicit: detach everything else, then
        // attach/switch to the requested adapter.
        let other_ids: Vec<u16> = self
            .adapter_cache
            .keys()
            .copied()
            .filter(|other| *other != id)
            .collect();
        for other in other_ids {
            if let Err(e) = self.detach_adapter(other) {
                tracing::warn!(
                    adapter_id = other,
                    error = %e,
                    "Failed to detach adapter during CoreML switch"
                );
            }
        }

        if !self.adapter_cache.contains_key(&id) {
            return Err(AosError::Kernel(format!(
                "Adapter {} not loaded for CoreML switch",
                id
            )));
        }

        // Fused package path: swap model handle without process restart.
        let fused_model_path =
            self.adapter_artifacts
                .get(&id)
                .and_then(|artifact| match artifact {
                    CoreMLAdapterArtifact::FusedPackage { model_path, .. } => {
                        Some(model_path.clone())
                    }
                    _ => None,
                });

        if let Some(model_path) = fused_model_path {
            #[cfg(target_os = "macos")]
            {
                // Do not overwrite recorded base path when loading fused bundle.
                self.load_model_internal(&model_path, false)?;
                self.active_fused_adapter = Some(id);
            }
            #[cfg(not(target_os = "macos"))]
            {
                return Err(AosError::Kernel("CoreML not available".to_string()));
            }
        } else {
            // Sidecar path: ensure we are on the base model.
            self.restore_base_model_if_needed()?;
            self.active_fused_adapter = None;
        }

        self.attached_adapters.insert(id);
        Ok(())
    }

    fn unload_adapter(&mut self, id: u16) -> Result<()> {
        self.attached_adapters.remove(&id);
        self.adapter_cache.remove(&id);
        self.adapter_artifacts.remove(&id);
        if self.active_fused_adapter == Some(id) {
            // Revert to base model if the fused adapter was active.
            self.restore_base_model_if_needed()?;
        }
        tracing::debug!(adapter_id = id, "Unloaded adapter from CoreML cache");
        Ok(())
    }

    fn get_metrics(&self) -> BackendMetrics {
        self.metrics.clone()
    }

    fn health_check(&self) -> Result<BackendHealth> {
        if self.model_handle.is_null() {
            return Ok(BackendHealth::Degraded {
                reason: "No model loaded".to_string(),
            });
        }

        #[cfg(target_os = "macos")]
        {
            let health = unsafe { ffi::coreml_health_check(self.model_handle) };
            if health == 0 {
                Ok(BackendHealth::Healthy)
            } else {
                Ok(BackendHealth::Degraded {
                    reason: format!("Health check returned code {}", health),
                })
            }
        }

        #[cfg(not(target_os = "macos"))]
        Ok(BackendHealth::Failed {
            reason: "CoreML not available".to_string(),
            recoverable: false,
        })
    }

    fn get_gpu_fingerprints(&self) -> HashMap<u32, GpuBufferFingerprint> {
        self.gpu_fingerprints.clone()
    }
}

impl CoreMLBackend {
    /// Return the currently attached adapter slots (hot-swap view).
    #[cfg(any(test, debug_assertions, feature = "coreml-stub"))]
    pub fn attached_adapter_ids(&self) -> Vec<u16> {
        let mut ids: Vec<u16> = self.attached_adapters.iter().copied().collect();
        ids.sort_unstable();
        ids
    }

    // ==================== MoE GPU Fingerprinting ====================

    /// Generate GPU fingerprint for an MoE adapter
    ///
    /// This creates a fingerprint that can be used to verify cross-layer
    /// determinism and adapter integrity for MoE models.
    pub fn generate_moe_fingerprint(&self, adapter_id: u16) -> Option<MoEGpuFingerprint> {
        use adapteros_core::B3Hash;

        let adapter = self.moe_adapter_cache.get(&adapter_id)?;

        let mut all_slices: Vec<&[u8]> = Vec::new();
        let mut expert_fingerprints = std::collections::HashMap::new();
        let mut total_buffer_bytes: u64 = 0;

        // Collect all weight data for hashing
        for (target, weights) in &adapter.targets {
            // Get LoRA A matrix as bytes
            let a_bytes: &[u8] = unsafe {
                std::slice::from_raw_parts(
                    weights.lora_a.as_ptr() as *const u8,
                    weights.lora_a.len() * std::mem::size_of::<f32>(),
                )
            };
            total_buffer_bytes += a_bytes.len() as u64;

            // Get LoRA B matrix as bytes
            let b_bytes: &[u8] = unsafe {
                std::slice::from_raw_parts(
                    weights.lora_b.as_ptr() as *const u8,
                    weights.lora_b.len() * std::mem::size_of::<f32>(),
                )
            };
            total_buffer_bytes += b_bytes.len() as u64;

            // Add to combined hash slices
            all_slices.push(a_bytes);
            all_slices.push(b_bytes);

            // Create per-target fingerprint
            let target_id = match target {
                MoELoRATarget::QProj => 0,
                MoELoRATarget::KProj => 1,
                MoELoRATarget::VProj => 2,
                MoELoRATarget::OProj => 3,
                MoELoRATarget::GateProj => 4,
                MoELoRATarget::UpProj => 5,
                MoELoRATarget::DownProj => 6,
            };

            // Hash just this target's data for per-expert fingerprint
            let target_hash = B3Hash::hash_multi(&[a_bytes, b_bytes]);
            expert_fingerprints.insert(target_id, target_hash);
        }

        // Compute combined hash
        let combined_hash = B3Hash::hash_multi(&all_slices);

        Some(MoEGpuFingerprint {
            adapter_id,
            total_buffer_bytes,
            combined_hash,
            expert_fingerprints,
            loaded_expert_count: adapter.targets.len(),
        })
    }

    /// Get all MoE GPU fingerprints for loaded adapters
    pub fn get_moe_fingerprints(&self) -> HashMap<u16, MoEGpuFingerprint> {
        self.moe_adapter_cache
            .keys()
            .filter_map(|&id| self.generate_moe_fingerprint(id).map(|fp| (id, fp)))
            .collect()
    }

    /// Verify MoE adapter integrity against a known fingerprint
    pub fn verify_moe_fingerprint(
        &self,
        adapter_id: u16,
        expected: &MoEGpuFingerprint,
    ) -> Result<bool> {
        let actual = self
            .generate_moe_fingerprint(adapter_id)
            .ok_or_else(|| AosError::Kernel(format!("MoE adapter {} not loaded", adapter_id)))?;

        // Compare combined hashes
        if actual.combined_hash != expected.combined_hash {
            tracing::warn!(
                adapter_id = adapter_id,
                expected_hash = %expected.combined_hash,
                actual_hash = %actual.combined_hash,
                "MoE adapter fingerprint mismatch (combined hash)"
            );
            return Ok(false);
        }

        // Compare buffer sizes
        if actual.total_buffer_bytes != expected.total_buffer_bytes {
            tracing::warn!(
                adapter_id = adapter_id,
                expected_bytes = expected.total_buffer_bytes,
                actual_bytes = actual.total_buffer_bytes,
                "MoE adapter fingerprint mismatch (buffer size)"
            );
            return Ok(false);
        }

        // Compare expert counts
        if actual.loaded_expert_count != expected.loaded_expert_count {
            tracing::warn!(
                adapter_id = adapter_id,
                expected_count = expected.loaded_expert_count,
                actual_count = actual.loaded_expert_count,
                "MoE adapter fingerprint mismatch (expert count)"
            );
            return Ok(false);
        }

        Ok(true)
    }
}

impl Drop for CoreMLBackend {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        {
            if !self.model_handle.is_null() {
                unsafe { ffi::coreml_unload_model(self.model_handle) };
                self.model_handle = std::ptr::null_mut();
            }
        }
    }
}

/// Check if CoreML is available on the current platform
pub fn is_coreml_available() -> bool {
    #[cfg(target_os = "macos")]
    {
        unsafe { ffi::coreml_is_available() }
    }

    #[cfg(not(target_os = "macos"))]
    false
}

/// Initialize CoreML runtime
pub fn init_coreml() -> Result<()> {
    if !is_coreml_available() {
        return Err(AosError::Kernel("CoreML not available".to_string()));
    }
    tracing::info!("CoreML runtime initialized");
    Ok(())
}

/// Check if Neural Engine is available
pub fn is_neural_engine_available() -> bool {
    #[cfg(target_os = "macos")]
    {
        let result = unsafe { ffi::coreml_check_ane() };
        result.available
    }

    #[cfg(not(target_os = "macos"))]
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::{
        export_coreml_adapter, validate_coreml_fusion, CoreMLExportRequest, CoreMLFusionMetadata,
    };
    use adapteros_aos::{AosWriter, BackendTag};
    use adapteros_core::B3Hash;
    use adapteros_lora_kernel_api::{attestation, FusedKernels, IoBuffers, RouterRing};
    use adapteros_types::CoreMLOpKind;
    use safetensors::{serialize, tensor::TensorView};
    use std::path::PathBuf;
    use tempfile::tempdir;
    fn simple_adapter_payload(delta: f32) -> Vec<u8> {
        let data = vec![delta; 8];
        let tensor = TensorView::new(safetensors::Dtype::F32, vec![2, 4], unsafe {
            std::slice::from_raw_parts(
                data.as_ptr() as *const u8,
                data.len() * std::mem::size_of::<f32>(),
            )
        })
        .expect("tensor view");
        serialize([("dummy.weight".to_string(), tensor)], &Default::default())
            .expect("serialize sidecar adapter")
    }

    #[test]
    fn test_mltensor_availability() {
        // Just check the function runs without panic
        let _ = MLTensor::is_available();
    }

    #[test]
    fn test_coreml_availability() {
        // Check is_coreml_available runs without panic
        let _ = is_coreml_available();
    }

    #[test]
    fn placement_spec_applies_to_stub_backend() {
        let mut backend = CoreMLBackend::new_stub(ComputeUnits::CpuAndNeuralEngine).unwrap();
        let graph = CoreMLGraph::from_nodes(vec![CoreMLGraphNode {
            name: "layer0.self_attn.q_proj".into(),
            op_kind: Some(CoreMLOpKind::AttentionQ),
            input_dim: Some(8),
            output_dim: Some(8),
            path_hint: None,
        }]);
        let spec = CoreMLPlacementSpec {
            version: 1,
            graph_id: None,
            bindings: vec![adapteros_types::coreml::CoreMLPlacementBinding {
                binding_id: "layer0.q".into(),
                target: adapteros_types::coreml::CoreMLTargetRef {
                    layer: "layer0.self_attn.q_proj".into(),
                    op_kind: CoreMLOpKind::AttentionQ,
                    path_hint: None,
                },
                projection: adapteros_types::coreml::CoreMLProjection::InputToHidden,
                rank: 4,
                alpha: None,
                scale: None,
                gating: None,
                shape: adapteros_types::coreml::CoreMLPlacementShape {
                    input_dim: 8,
                    output_dim: 8,
                },
            }],
        };

        let metrics = backend.apply_placement_spec(&graph, spec);
        assert_eq!(metrics.missing, 0);
        assert_eq!(metrics.resolved, 1);
        assert!(backend.placement_metrics().is_some());
        assert!(backend.dump_placement_map().is_some());
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn stub_lora_preserves_coreml_model_bytes() -> Result<()> {
        let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
            "../../var/model-cache/models/qwen2.5-7b-instruct-fp16-512.mlpackage/Manifest.json",
        );
        if !manifest_path.exists() {
            eprintln!(
                "Skipping CoreML bytes preservation test; fixture missing at {}",
                manifest_path.display()
            );
            return Ok(());
        }
        let base_bytes = std::fs::read(&manifest_path).expect(
            "CoreML fixture at var/model-cache/models/qwen2.5-7b-instruct-fp16-512.mlpackage",
        );
        let hash_before = B3Hash::hash(&base_bytes);

        // Build a minimal adapter payload to hit the LoRA fusion branch.
        let weights = vec![0.1f32; 4];
        let adapter_tensors = [(
            "dummy.weight".to_string(),
            TensorView::new(safetensors::Dtype::F32, vec![2, 2], unsafe {
                std::slice::from_raw_parts(weights.as_ptr() as *const u8, weights.len() * 4)
            })
            .expect("tensor view"),
        )];
        let adapter_bytes =
            serialize(adapter_tensors, &Default::default()).expect("serialize adapter");

        // Wrap adapter payload into a canonical .aos bundle so the export helper can ingest it.
        let tmp = tempdir().expect("temp dir");
        let adapter_path = tmp.path().join("dummy.aos");
        let mut writer = AosWriter::new();
        writer.add_segment(
            BackendTag::Canonical,
            Some("dummy.scope".into()),
            &adapter_bytes,
        )?;
        let adapter_manifest = serde_json::json!({
            "metadata": {
                "scope_path": "dummy.scope",
                "domain": "tests",
                "group": "coreml",
                "operation": "coreml-export"
            },
            "scope": "coreml-export"
        });
        writer.write_archive(&adapter_path, &adapter_manifest)?;

        let output_path = tmp.path().join("fused/Manifest.json");
        let outcome = export_coreml_adapter(&CoreMLExportRequest {
            base_package: manifest_path.clone(),
            adapter_aos: adapter_path.clone(),
            output_package: output_path,
            compute_units: ComputeUnits::CpuAndNeuralEngine,
        })?;

        validate_coreml_fusion(&outcome.metadata_path)?;

        assert_eq!(
            hash_before, outcome.base_manifest_hash,
            "hash should reflect the original manifest"
        );
        assert_eq!(
            outcome.base_manifest_hash, outcome.fused_manifest_hash,
            "export must keep base manifest bytes unchanged"
        );

        Ok(())
    }

    #[test]
    fn test_neural_engine_availability() {
        // Check is_neural_engine_available runs without panic
        let _ = is_neural_engine_available();
    }

    #[test]
    fn q15_gate_conversion_matches_router_invariant() {
        let gates = [0i16, 1, 16384, 32767, -16384];
        for gate in gates {
            let expected = gate as f32 / 32767.0;
            let actual = CoreMLBackend::gate_q15_to_f32(gate);
            assert!(
                (actual - expected).abs() < 1e-6,
                "gate {} expected {}, got {}",
                gate,
                expected,
                actual
            );
        }
    }

    #[test]
    fn stub_attestation_reports_nondeterministic_without_ane() -> Result<()> {
        let backend = CoreMLBackend::new_stub(ComputeUnits::CpuAndNeuralEngine)?;
        let report = backend.attest_determinism()?;
        assert!(
            !report.deterministic,
            "stub backend should never claim determinism"
        );
        assert!(matches!(
            report.rng_seed_method,
            attestation::RngSeedingMethod::SystemEntropy
        ));
        Ok(())
    }

    #[test]
    #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
    fn coreml_stub_hot_swap_sidecar_switches_and_restores() -> Result<()> {
        let mut backend = CoreMLBackend::new_stub(ComputeUnits::CpuAndNeuralEngine)?;

        // Baseline run without adapters
        let mut base_ring = RouterRing::new(0);
        let mut base_io = IoBuffers::new(6);
        base_io.input_ids = vec![1];
        backend.run_step(&mut base_ring, &mut base_io)?;
        let base_logits = base_io.output_logits.clone();

        // Attach adapter A and run twice for determinism
        let adapter_a = simple_adapter_payload(0.05);
        backend.load_adapter(7, &adapter_a)?;
        let mut ring_a = RouterRing::new(1);
        ring_a.set(&[7u16], &[32767]);
        let mut io_a = IoBuffers::new(6);
        io_a.input_ids = vec![1];
        backend.run_step(&mut ring_a, &mut io_a)?;
        let logits_a = io_a.output_logits.clone();

        let mut io_a_repeat = IoBuffers::new(6);
        io_a_repeat.input_ids = vec![1];
        backend.run_step(&mut ring_a, &mut io_a_repeat)?;
        assert_eq!(
            logits_a, io_a_repeat.output_logits,
            "Same adapter + seed must be deterministic"
        );

        // Switch to adapter B and ensure outputs differ
        let adapter_b = simple_adapter_payload(0.15);
        backend.load_adapter(9, &adapter_b)?;
        backend.switch_adapter(9)?;
        let mut ring_b = RouterRing::new(1);
        ring_b.set(&[9u16], &[32767]);
        let mut io_b = IoBuffers::new(6);
        io_b.input_ids = vec![1];
        backend.run_step(&mut ring_b, &mut io_b)?;
        assert_ne!(
            logits_a, io_b.output_logits,
            "Adapter switch should change logits in stub path"
        );

        // Detach all adapters and confirm base-only logits restored
        backend.detach_adapter(9)?;
        let mut clear_ring = RouterRing::new(0);
        let mut base_again = IoBuffers::new(6);
        base_again.input_ids = vec![1];
        backend.run_step(&mut clear_ring, &mut base_again)?;
        assert_eq!(
            base_logits, base_again.output_logits,
            "Detaching should restore base-only behavior"
        );

        Ok(())
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_mltensor_create_and_materialize() {
        if !MLTensor::is_available() {
            return; // Skip on older macOS
        }

        let data = vec![1.0, 2.0, 3.0, 4.0];
        let tensor = MLTensor::from_floats(&data, &[2, 2]).unwrap();
        let result = tensor.to_vec().unwrap();
        assert_eq!(result.len(), 4);
        assert_eq!(result, data);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_mltensor_invalid_shape() {
        if !MLTensor::is_available() {
            return;
        }

        // Data doesn't match shape
        let data = vec![1.0, 2.0, 3.0];
        let result = MLTensor::from_floats(&data, &[2, 2]);
        assert!(result.is_err());
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_mltensor_softmax() {
        if !MLTensor::is_available() {
            return;
        }

        let data = vec![1.0, 2.0, 3.0, 4.0];
        let tensor = MLTensor::from_floats(&data, &[1, 4]).unwrap();
        let softmax_result = tensor.softmax(-1).unwrap();
        let result = softmax_result.to_vec().unwrap();

        // Softmax should sum to ~1.0
        let sum: f32 = result.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5, "Softmax sum was {}", sum);

        // Values should be positive
        assert!(result.iter().all(|&x| x > 0.0));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_mltensor_add() {
        if !MLTensor::is_available() {
            return;
        }

        let data1 = vec![1.0, 2.0, 3.0, 4.0];
        let data2 = vec![5.0, 6.0, 7.0, 8.0];
        let tensor1 = MLTensor::from_floats(&data1, &[2, 2]).unwrap();
        let tensor2 = MLTensor::from_floats(&data2, &[2, 2]).unwrap();

        let sum = tensor1.add(&tensor2).unwrap();
        let result = sum.to_vec().unwrap();

        assert_eq!(result, vec![6.0, 8.0, 10.0, 12.0]);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_mltensor_scale() {
        if !MLTensor::is_available() {
            return;
        }

        let data = vec![1.0, 2.0, 3.0, 4.0];
        let tensor = MLTensor::from_floats(&data, &[2, 2]).unwrap();

        let scaled = tensor.scale(2.0).unwrap();
        let result = scaled.to_vec().unwrap();

        assert_eq!(result, vec![2.0, 4.0, 6.0, 8.0]);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_mltensor_matmul() {
        if !MLTensor::is_available() {
            return;
        }

        // [1, 2]   [5, 6]   [1*5+2*7, 1*6+2*8]   [19, 22]
        // [3, 4] x [7, 8] = [3*5+4*7, 3*6+4*8] = [43, 50]
        let data1 = vec![1.0, 2.0, 3.0, 4.0];
        let data2 = vec![5.0, 6.0, 7.0, 8.0];
        let tensor1 = MLTensor::from_floats(&data1, &[2, 2]).unwrap();
        let tensor2 = MLTensor::from_floats(&data2, &[2, 2]).unwrap();

        let product = tensor1.matmul(&tensor2).unwrap();
        let result = product.to_vec().unwrap();

        assert_eq!(result, vec![19.0, 22.0, 43.0, 50.0]);
    }

    #[test]
    #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
    fn fused_metadata_mismatch_is_rejected() -> Result<()> {
        let tmp = tempdir().expect("tempdir");
        let base = tmp.path().join("base.json");
        let fused = tmp.path().join("fused.json");
        let adapter = tmp.path().join("adapter.bin");
        std::fs::write(&base, b"base-bytes")?;
        std::fs::write(&fused, b"fused-bytes")?;
        std::fs::write(&adapter, b"adapter-bytes")?;

        let metadata = CoreMLFusionMetadata {
            base_manifest_hash: B3Hash::hash(b"wrong-base"),
            fused_manifest_hash: B3Hash::hash(b"fused-bytes"),
            adapter_hash: B3Hash::hash(b"adapter-bytes"),
            base_package: base.clone(),
            fused_package: fused.clone(),
            adapter_path: adapter.clone(),
            fusion_verified: false,
        };
        let metadata_path = tmp.path().join("adapteros_coreml_fusion.json");
        std::fs::write(&metadata_path, serde_json::to_vec(&metadata)?)?;

        let mut backend = CoreMLBackend::new_stub(ComputeUnits::CpuAndNeuralEngine)?;
        let err = backend.register_fused_adapter_from_metadata(5, &metadata_path);
        assert!(
            err.is_err(),
            "mismatched base hash should reject fused adapter"
        );
        Ok(())
    }

    #[test]
    #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
    fn switch_adapter_fails_for_missing_fused_package() -> Result<()> {
        let mut backend = CoreMLBackend::new_stub(ComputeUnits::CpuAndNeuralEngine)?;
        backend.adapter_artifacts.insert(
            11,
            CoreMLAdapterArtifact::FusedPackage {
                model_path: PathBuf::from("/nonexistent/fused.mlmodelc"),
                model_hash: None,
            },
        );

        let result = backend.switch_adapter(11);
        assert!(result.is_err(), "missing fused package should error");
        assert!(
            backend.active_fused_adapter.is_none(),
            "fused activation should not be recorded on failure"
        );
        Ok(())
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_mltensor_shape() {
        if !MLTensor::is_available() {
            return;
        }

        let data = vec![1.0; 24];
        let tensor = MLTensor::from_floats(&data, &[2, 3, 4]).unwrap();

        assert_eq!(tensor.shape(), vec![2, 3, 4]);
        assert_eq!(tensor.num_elements(), 24);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_mltensor_1d() {
        if !MLTensor::is_available() {
            return;
        }

        let data = vec![1.0, 2.0, 3.0];
        let tensor = MLTensor::from_floats(&data, &[3]).unwrap();
        let result = tensor.to_vec().unwrap();

        assert_eq!(result, data);
        assert_eq!(tensor.shape(), vec![3]);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_mltensor_chained_operations() {
        if !MLTensor::is_available() {
            return;
        }

        let data = vec![1.0, 2.0, 3.0, 4.0];
        let tensor = MLTensor::from_floats(&data, &[2, 2]).unwrap();

        // Scale then add to self
        let scaled = tensor.scale(2.0).unwrap();
        let doubled = tensor.add(&scaled).unwrap();
        let result = doubled.to_vec().unwrap();

        // Original + 2*Original = 3*Original
        assert_eq!(result, vec![3.0, 6.0, 9.0, 12.0]);
    }

    #[test]
    fn test_mltensor_not_available_error() {
        // When MLTensor is not available, operations should return errors
        if MLTensor::is_available() {
            return; // Skip if MLTensor is actually available
        }

        let data = vec![1.0, 2.0, 3.0, 4.0];
        let result = MLTensor::from_floats(&data, &[2, 2]);
        assert!(result.is_err());
    }

    #[test]
    fn test_mltensor_handle_validity() {
        let handle = ffi::MLTensorHandle::default();
        assert!(!handle.is_valid());
        assert_eq!(handle.num_elements(), 0);
    }

    #[test]
    fn test_mltensor_handle_num_elements() {
        let mut handle = ffi::MLTensorHandle::default();
        handle.shape[0] = 2;
        handle.shape[1] = 3;
        handle.shape[2] = 4;
        handle.rank = 3;

        assert_eq!(handle.num_elements(), 24);
    }

    // ========== Swift Bridge Tests ==========

    #[test]
    #[cfg(target_os = "macos")]
    fn test_swift_bridge_available() {
        // Test that calling the Swift bridge detection doesn't crash
        let available = unsafe { ffi::coreml_supports_mltensor() };
        // Just check it doesn't crash - result depends on macOS version
        println!("Swift MLTensor bridge available: {}", available);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_swift_tensor_creation() {
        if !unsafe { ffi::coreml_supports_mltensor() } {
            println!("Skipping - Swift bridge not available (requires macOS 15+)");
            return;
        }

        let data = vec![1.0f32, 2.0, 3.0, 4.0];
        let shape = vec![2usize, 2];

        let handle =
            unsafe { ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len()) };

        assert!(
            handle.is_valid(),
            "Failed to create tensor via Swift bridge"
        );
        assert_eq!(handle.rank, 2);
        assert_eq!(handle.shape[0], 2);
        assert_eq!(handle.shape[1], 2);
        assert_eq!(handle.num_elements(), 4);

        // Clean up
        unsafe { ffi::coreml_tensor_free(handle) };
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_swift_tensor_operations() {
        if !unsafe { ffi::coreml_supports_mltensor() } {
            println!("Skipping - Swift bridge not available (requires macOS 15+)");
            return;
        }

        // Test softmax operation
        let data = vec![1.0f32, 2.0, 3.0, 4.0];
        let shape = vec![1usize, 4];

        let handle =
            unsafe { ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len()) };
        assert!(handle.is_valid(), "Failed to create tensor");

        let softmax_handle = unsafe { ffi::coreml_tensor_softmax(handle, -1) };
        assert!(softmax_handle.is_valid(), "Softmax operation failed");

        // Materialize and verify softmax sums to 1
        let mut output = vec![0.0f32; 4];
        let result = unsafe {
            ffi::coreml_tensor_to_floats(softmax_handle, output.as_mut_ptr(), output.len())
        };
        assert!(
            result >= 0,
            "Failed to materialize tensor: error code {}",
            result
        );

        let sum: f32 = output.iter().sum();
        assert!(
            (sum - 1.0).abs() < 1e-5,
            "Softmax sum was {} (expected ~1.0)",
            sum
        );
        assert!(
            output.iter().all(|&x| x > 0.0),
            "Softmax values should be positive"
        );

        // Clean up
        unsafe {
            ffi::coreml_tensor_free(handle);
            ffi::coreml_tensor_free(softmax_handle);
        };
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_swift_tensor_add_operation() {
        if !unsafe { ffi::coreml_supports_mltensor() } {
            println!("Skipping - Swift bridge not available (requires macOS 15+)");
            return;
        }

        let data1 = vec![1.0f32, 2.0, 3.0, 4.0];
        let data2 = vec![5.0f32, 6.0, 7.0, 8.0];
        let shape = vec![2usize, 2];

        let handle1 =
            unsafe { ffi::coreml_create_tensor_f32(data1.as_ptr(), shape.as_ptr(), shape.len()) };
        let handle2 =
            unsafe { ffi::coreml_create_tensor_f32(data2.as_ptr(), shape.as_ptr(), shape.len()) };

        assert!(
            handle1.is_valid() && handle2.is_valid(),
            "Failed to create tensors"
        );

        let sum_handle = unsafe { ffi::coreml_tensor_add(handle1, handle2) };
        assert!(sum_handle.is_valid(), "Add operation failed");

        let mut output = vec![0.0f32; 4];
        let result =
            unsafe { ffi::coreml_tensor_to_floats(sum_handle, output.as_mut_ptr(), output.len()) };
        assert!(result >= 0, "Failed to materialize tensor");

        assert_eq!(output, vec![6.0, 8.0, 10.0, 12.0], "Add result incorrect");

        // Clean up
        unsafe {
            ffi::coreml_tensor_free(handle1);
            ffi::coreml_tensor_free(handle2);
            ffi::coreml_tensor_free(sum_handle);
        };
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_swift_tensor_scale_operation() {
        if !unsafe { ffi::coreml_supports_mltensor() } {
            println!("Skipping - Swift bridge not available (requires macOS 15+)");
            return;
        }

        let data = vec![1.0f32, 2.0, 3.0, 4.0];
        let shape = vec![2usize, 2];

        let handle =
            unsafe { ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len()) };
        assert!(handle.is_valid(), "Failed to create tensor");

        let scaled_handle = unsafe { ffi::coreml_tensor_scale(handle, 2.5) };
        assert!(scaled_handle.is_valid(), "Scale operation failed");

        let mut output = vec![0.0f32; 4];
        let result = unsafe {
            ffi::coreml_tensor_to_floats(scaled_handle, output.as_mut_ptr(), output.len())
        };
        assert!(result >= 0, "Failed to materialize tensor");

        assert_eq!(output, vec![2.5, 5.0, 7.5, 10.0], "Scale result incorrect");

        // Clean up
        unsafe {
            ffi::coreml_tensor_free(handle);
            ffi::coreml_tensor_free(scaled_handle);
        };
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_swift_tensor_matmul_operation() {
        if !unsafe { ffi::coreml_supports_mltensor() } {
            println!("Skipping - Swift bridge not available (requires macOS 15+)");
            return;
        }

        // [1, 2]   [5, 6]   [19, 22]
        // [3, 4] x [7, 8] = [43, 50]
        let data1 = vec![1.0f32, 2.0, 3.0, 4.0];
        let data2 = vec![5.0f32, 6.0, 7.0, 8.0];
        let shape = vec![2usize, 2];

        let handle1 =
            unsafe { ffi::coreml_create_tensor_f32(data1.as_ptr(), shape.as_ptr(), shape.len()) };
        let handle2 =
            unsafe { ffi::coreml_create_tensor_f32(data2.as_ptr(), shape.as_ptr(), shape.len()) };

        assert!(
            handle1.is_valid() && handle2.is_valid(),
            "Failed to create tensors"
        );

        let product_handle = unsafe { ffi::coreml_tensor_matmul(handle1, handle2) };
        assert!(product_handle.is_valid(), "Matmul operation failed");

        let mut output = vec![0.0f32; 4];
        let result = unsafe {
            ffi::coreml_tensor_to_floats(product_handle, output.as_mut_ptr(), output.len())
        };
        assert!(result >= 0, "Failed to materialize tensor");

        assert_eq!(
            output,
            vec![19.0, 22.0, 43.0, 50.0],
            "Matmul result incorrect"
        );

        // Clean up
        unsafe {
            ffi::coreml_tensor_free(handle1);
            ffi::coreml_tensor_free(handle2);
            ffi::coreml_tensor_free(product_handle);
        };
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_swift_tensor_memory_cleanup() {
        if !unsafe { ffi::coreml_supports_mltensor() } {
            println!("Skipping - Swift bridge not available (requires macOS 15+)");
            return;
        }

        // Create and free multiple tensors to verify memory cleanup
        for i in 0..10 {
            let data = vec![i as f32; 100];
            let shape = vec![10usize, 10];

            let handle = unsafe {
                ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len())
            };
            assert!(handle.is_valid(), "Failed to create tensor iteration {}", i);

            // Free immediately
            unsafe { ffi::coreml_tensor_free(handle) };
        }
        println!("Memory cleanup test passed - created and freed 10 tensors");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_swift_tensor_large_tensor() {
        if !unsafe { ffi::coreml_supports_mltensor() } {
            println!("Skipping - Swift bridge not available (requires macOS 15+)");
            return;
        }

        // Test with a reasonably large tensor
        let size = 1024;
        let data: Vec<f32> = (0..size).map(|i| i as f32).collect();
        let shape = vec![32usize, 32];

        let handle =
            unsafe { ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len()) };
        assert!(handle.is_valid(), "Failed to create large tensor");
        assert_eq!(handle.num_elements(), size);

        // Materialize and verify
        let mut output = vec![0.0f32; size];
        let result =
            unsafe { ffi::coreml_tensor_to_floats(handle, output.as_mut_ptr(), output.len()) };
        assert!(result >= 0, "Failed to materialize large tensor");
        assert_eq!(output, data, "Large tensor data mismatch");

        unsafe { ffi::coreml_tensor_free(handle) };
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_swift_tensor_3d_tensor() {
        if !unsafe { ffi::coreml_supports_mltensor() } {
            println!("Skipping - Swift bridge not available (requires macOS 15+)");
            return;
        }

        let data: Vec<f32> = (0..24).map(|i| i as f32).collect();
        let shape = vec![2usize, 3, 4];

        let handle =
            unsafe { ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len()) };
        assert!(handle.is_valid(), "Failed to create 3D tensor");
        assert_eq!(handle.rank, 3);
        assert_eq!(handle.shape[0], 2);
        assert_eq!(handle.shape[1], 3);
        assert_eq!(handle.shape[2], 4);
        assert_eq!(handle.num_elements(), 24);

        let mut output = vec![0.0f32; 24];
        let result =
            unsafe { ffi::coreml_tensor_to_floats(handle, output.as_mut_ptr(), output.len()) };
        assert!(result >= 0, "Failed to materialize 3D tensor");
        assert_eq!(output, data);

        unsafe { ffi::coreml_tensor_free(handle) };
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_runtime_dispatch_mltensor_vs_legacy() {
        // Test that runtime correctly dispatches based on availability
        let supports_mltensor = unsafe { ffi::coreml_supports_mltensor() };

        if supports_mltensor {
            println!("Runtime dispatch: Using MLTensor API (macOS 15+)");
            // Verify we can use MLTensor operations
            let data = vec![1.0f32, 2.0, 3.0, 4.0];
            let shape = vec![2usize, 2];
            let handle = unsafe {
                ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len())
            };
            assert!(handle.is_valid(), "MLTensor should work when supported");
            unsafe { ffi::coreml_tensor_free(handle) };
        } else {
            println!("Runtime dispatch: MLTensor not available (macOS < 15)");
            // On older macOS, the function should return false but not crash
        }

        // CoreML availability is separate from MLTensor
        let coreml_available = unsafe { ffi::coreml_is_available() };
        println!("CoreML available: {}", coreml_available);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_swift_bridge_chained_operations() {
        if !unsafe { ffi::coreml_supports_mltensor() } {
            println!("Skipping - Swift bridge not available (requires macOS 15+)");
            return;
        }

        // Test chaining multiple operations: scale -> add -> softmax
        let data = vec![1.0f32, 2.0, 3.0, 4.0];
        let shape = vec![1usize, 4];

        let handle =
            unsafe { ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len()) };
        assert!(handle.is_valid());

        // Scale by 0.5
        let scaled = unsafe { ffi::coreml_tensor_scale(handle, 0.5) };
        assert!(scaled.is_valid(), "Scale failed");

        // Add original to scaled
        let sum = unsafe { ffi::coreml_tensor_add(handle, scaled) };
        assert!(sum.is_valid(), "Add failed");

        // Apply softmax
        let softmax = unsafe { ffi::coreml_tensor_softmax(sum, -1) };
        assert!(softmax.is_valid(), "Softmax failed");

        // Materialize and verify
        let mut output = vec![0.0f32; 4];
        let result =
            unsafe { ffi::coreml_tensor_to_floats(softmax, output.as_mut_ptr(), output.len()) };
        assert!(result >= 0, "Materialize failed");

        let total: f32 = output.iter().sum();
        assert!(
            (total - 1.0).abs() < 1e-5,
            "Softmax should sum to 1, got {}",
            total
        );

        // Clean up all handles
        unsafe {
            ffi::coreml_tensor_free(handle);
            ffi::coreml_tensor_free(scaled);
            ffi::coreml_tensor_free(sum);
            ffi::coreml_tensor_free(softmax);
        };
    }

    // ========== ObjC++ Direct Path Tests (MLMultiArray Fallback) ==========
    //
    // These tests directly exercise the ObjC++ FFI implementation, skipping
    // the Swift bridge entirely. This helps isolate whether issues are in
    // the Swift bridge or the underlying ObjC++ implementation.

    #[test]
    #[cfg(target_os = "macos")]
    fn test_objcpp_direct_tensor_create_and_read() {
        // Skip if MLTensor not available at all
        if !unsafe { ffi::coreml_supports_mltensor() } {
            println!("Skipping - MLTensor API not available (requires macOS 15+)");
            return;
        }

        // Create tensor via ObjC++ path directly (coreml_* functions)
        let data = vec![1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0];
        let shape = vec![2usize, 3];

        let handle =
            unsafe { ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len()) };

        // Verify handle is valid
        assert!(
            handle.is_valid(),
            "ObjC++ tensor creation failed - handle is invalid"
        );
        assert_eq!(handle.rank, 2, "Expected rank 2, got {}", handle.rank);
        assert_eq!(
            handle.shape[0], 2,
            "Expected shape[0]=2, got {}",
            handle.shape[0]
        );
        assert_eq!(
            handle.shape[1], 3,
            "Expected shape[1]=3, got {}",
            handle.shape[1]
        );
        assert_eq!(
            handle.num_elements(),
            6,
            "Expected 6 elements, got {}",
            handle.num_elements()
        );

        // Read data back via ObjC++ path
        let mut output = vec![0.0f32; 6];
        let result =
            unsafe { ffi::coreml_tensor_to_floats(handle, output.as_mut_ptr(), output.len()) };

        assert!(
            result >= 0,
            "ObjC++ tensor read failed with error code {}",
            result
        );
        assert_eq!(
            output, data,
            "Data mismatch: expected {:?}, got {:?}",
            data, output
        );

        println!("ObjC++ direct path: create and read test PASSED");

        // Clean up
        unsafe { ffi::coreml_tensor_free(handle) };
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_objcpp_direct_softmax() {
        if !unsafe { ffi::coreml_supports_mltensor() } {
            println!("Skipping - MLTensor API not available (requires macOS 15+)");
            return;
        }

        // Test softmax via ObjC++ path
        let data = vec![1.0f32, 2.0, 3.0, 4.0];
        let shape = vec![1usize, 4];

        let input =
            unsafe { ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len()) };
        assert!(input.is_valid(), "Failed to create input tensor");

        // Apply softmax via ObjC++ path
        let softmax_result = unsafe { ffi::coreml_tensor_softmax(input, -1) };
        assert!(
            softmax_result.is_valid(),
            "ObjC++ softmax failed - returned invalid handle"
        );

        // Read result
        let mut output = vec![0.0f32; 4];
        let read_result = unsafe {
            ffi::coreml_tensor_to_floats(softmax_result, output.as_mut_ptr(), output.len())
        };
        assert!(
            read_result >= 0,
            "Failed to read softmax result: error {}",
            read_result
        );

        // Verify softmax properties
        let sum: f32 = output.iter().sum();
        assert!(
            (sum - 1.0).abs() < 1e-4,
            "ObjC++ softmax sum should be ~1.0, got {}",
            sum
        );
        assert!(
            output.iter().all(|&x| x > 0.0 && x < 1.0),
            "ObjC++ softmax values should be in (0,1): {:?}",
            output
        );
        // Verify monotonicity (larger input -> larger softmax)
        for i in 1..output.len() {
            assert!(
                output[i] > output[i - 1],
                "Softmax should preserve ordering: {:?}",
                output
            );
        }

        println!(
            "ObjC++ direct path: softmax test PASSED - output {:?}",
            output
        );

        unsafe {
            ffi::coreml_tensor_free(input);
            ffi::coreml_tensor_free(softmax_result);
        };
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_objcpp_direct_add() {
        if !unsafe { ffi::coreml_supports_mltensor() } {
            println!("Skipping - MLTensor API not available (requires macOS 15+)");
            return;
        }

        let data1 = vec![1.0f32, 2.0, 3.0, 4.0];
        let data2 = vec![10.0f32, 20.0, 30.0, 40.0];
        let shape = vec![2usize, 2];

        let tensor1 =
            unsafe { ffi::coreml_create_tensor_f32(data1.as_ptr(), shape.as_ptr(), shape.len()) };
        let tensor2 =
            unsafe { ffi::coreml_create_tensor_f32(data2.as_ptr(), shape.as_ptr(), shape.len()) };

        assert!(tensor1.is_valid(), "Failed to create tensor1");
        assert!(tensor2.is_valid(), "Failed to create tensor2");

        // Add via ObjC++ path
        let sum = unsafe { ffi::coreml_tensor_add(tensor1, tensor2) };
        assert!(
            sum.is_valid(),
            "ObjC++ add failed - returned invalid handle"
        );

        let mut output = vec![0.0f32; 4];
        let read_result =
            unsafe { ffi::coreml_tensor_to_floats(sum, output.as_mut_ptr(), output.len()) };
        assert!(
            read_result >= 0,
            "Failed to read add result: error {}",
            read_result
        );

        let expected = vec![11.0f32, 22.0, 33.0, 44.0];
        assert_eq!(
            output, expected,
            "ObjC++ add result mismatch: expected {:?}, got {:?}",
            expected, output
        );

        println!("ObjC++ direct path: add test PASSED");

        unsafe {
            ffi::coreml_tensor_free(tensor1);
            ffi::coreml_tensor_free(tensor2);
            ffi::coreml_tensor_free(sum);
        };
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_objcpp_direct_scale() {
        if !unsafe { ffi::coreml_supports_mltensor() } {
            println!("Skipping - MLTensor API not available (requires macOS 15+)");
            return;
        }

        let data = vec![2.0f32, 4.0, 6.0, 8.0];
        let shape = vec![4usize];

        let tensor =
            unsafe { ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len()) };
        assert!(tensor.is_valid(), "Failed to create tensor");

        // Scale by 0.5 via ObjC++ path
        let scaled = unsafe { ffi::coreml_tensor_scale(tensor, 0.5) };
        assert!(
            scaled.is_valid(),
            "ObjC++ scale failed - returned invalid handle"
        );

        let mut output = vec![0.0f32; 4];
        let read_result =
            unsafe { ffi::coreml_tensor_to_floats(scaled, output.as_mut_ptr(), output.len()) };
        assert!(
            read_result >= 0,
            "Failed to read scale result: error {}",
            read_result
        );

        let expected = vec![1.0f32, 2.0, 3.0, 4.0];
        assert_eq!(
            output, expected,
            "ObjC++ scale result mismatch: expected {:?}, got {:?}",
            expected, output
        );

        println!("ObjC++ direct path: scale test PASSED");

        unsafe {
            ffi::coreml_tensor_free(tensor);
            ffi::coreml_tensor_free(scaled);
        };
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_objcpp_direct_matmul() {
        if !unsafe { ffi::coreml_supports_mltensor() } {
            println!("Skipping - MLTensor API not available (requires macOS 15+)");
            return;
        }

        // 2x2 @ 2x2 matrix multiplication
        // [1, 2]   [5, 6]   [1*5+2*7, 1*6+2*8]   [19, 22]
        // [3, 4] @ [7, 8] = [3*5+4*7, 3*6+4*8] = [43, 50]
        let data1 = vec![1.0f32, 2.0, 3.0, 4.0];
        let data2 = vec![5.0f32, 6.0, 7.0, 8.0];
        let shape = vec![2usize, 2];

        let tensor1 =
            unsafe { ffi::coreml_create_tensor_f32(data1.as_ptr(), shape.as_ptr(), shape.len()) };
        let tensor2 =
            unsafe { ffi::coreml_create_tensor_f32(data2.as_ptr(), shape.as_ptr(), shape.len()) };

        assert!(tensor1.is_valid(), "Failed to create tensor1");
        assert!(tensor2.is_valid(), "Failed to create tensor2");

        // Matmul via ObjC++ path
        let product = unsafe { ffi::coreml_tensor_matmul(tensor1, tensor2) };
        assert!(
            product.is_valid(),
            "ObjC++ matmul failed - returned invalid handle"
        );

        let mut output = vec![0.0f32; 4];
        let read_result =
            unsafe { ffi::coreml_tensor_to_floats(product, output.as_mut_ptr(), output.len()) };
        assert!(
            read_result >= 0,
            "Failed to read matmul result: error {}",
            read_result
        );

        let expected = vec![19.0f32, 22.0, 43.0, 50.0];
        assert_eq!(
            output, expected,
            "ObjC++ matmul result mismatch: expected {:?}, got {:?}",
            expected, output
        );

        println!("ObjC++ direct path: matmul test PASSED");

        unsafe {
            ffi::coreml_tensor_free(tensor1);
            ffi::coreml_tensor_free(tensor2);
            ffi::coreml_tensor_free(product);
        };
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_objcpp_direct_chained_operations() {
        if !unsafe { ffi::coreml_supports_mltensor() } {
            println!("Skipping - MLTensor API not available (requires macOS 15+)");
            return;
        }

        // Test chaining operations via ObjC++ path: create -> scale -> add -> read
        let data = vec![1.0f32, 2.0, 3.0, 4.0];
        let shape = vec![2usize, 2];

        let original =
            unsafe { ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len()) };
        assert!(original.is_valid(), "Failed to create original tensor");

        // Scale by 2.0
        let scaled = unsafe { ffi::coreml_tensor_scale(original, 2.0) };
        assert!(scaled.is_valid(), "Scale operation failed");

        // Add original + scaled (should give 3x original)
        let sum = unsafe { ffi::coreml_tensor_add(original, scaled) };
        assert!(sum.is_valid(), "Add operation failed");

        // Read final result
        let mut output = vec![0.0f32; 4];
        let read_result =
            unsafe { ffi::coreml_tensor_to_floats(sum, output.as_mut_ptr(), output.len()) };
        assert!(
            read_result >= 0,
            "Failed to read chained result: error {}",
            read_result
        );

        // original + 2*original = 3*original
        let expected = vec![3.0f32, 6.0, 9.0, 12.0];
        assert_eq!(
            output, expected,
            "ObjC++ chained ops result mismatch: expected {:?}, got {:?}",
            expected, output
        );

        println!("ObjC++ direct path: chained operations test PASSED");

        unsafe {
            ffi::coreml_tensor_free(original);
            ffi::coreml_tensor_free(scaled);
            ffi::coreml_tensor_free(sum);
        };
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_objcpp_direct_1d_tensor() {
        if !unsafe { ffi::coreml_supports_mltensor() } {
            println!("Skipping - MLTensor API not available (requires macOS 15+)");
            return;
        }

        // Test 1D tensor via ObjC++ path
        let data = vec![1.0f32, 2.0, 3.0, 4.0, 5.0];
        let shape = vec![5usize];

        let tensor =
            unsafe { ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len()) };

        assert!(tensor.is_valid(), "Failed to create 1D tensor");
        assert_eq!(tensor.rank, 1, "Expected rank 1 for 1D tensor");
        assert_eq!(tensor.shape[0], 5, "Expected shape[0]=5");
        assert_eq!(tensor.num_elements(), 5, "Expected 5 elements");

        let mut output = vec![0.0f32; 5];
        let read_result =
            unsafe { ffi::coreml_tensor_to_floats(tensor, output.as_mut_ptr(), output.len()) };
        assert!(read_result >= 0, "Failed to read 1D tensor");
        assert_eq!(output, data);

        println!("ObjC++ direct path: 1D tensor test PASSED");

        unsafe { ffi::coreml_tensor_free(tensor) };
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_objcpp_direct_3d_tensor() {
        if !unsafe { ffi::coreml_supports_mltensor() } {
            println!("Skipping - MLTensor API not available (requires macOS 15+)");
            return;
        }

        // Test 3D tensor via ObjC++ path
        let data: Vec<f32> = (0..24).map(|i| i as f32).collect();
        let shape = vec![2usize, 3, 4];

        let tensor =
            unsafe { ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len()) };

        assert!(tensor.is_valid(), "Failed to create 3D tensor");
        assert_eq!(tensor.rank, 3, "Expected rank 3 for 3D tensor");
        assert_eq!(tensor.shape[0], 2, "Expected shape[0]=2");
        assert_eq!(tensor.shape[1], 3, "Expected shape[1]=3");
        assert_eq!(tensor.shape[2], 4, "Expected shape[2]=4");
        assert_eq!(tensor.num_elements(), 24, "Expected 24 elements");

        let mut output = vec![0.0f32; 24];
        let read_result =
            unsafe { ffi::coreml_tensor_to_floats(tensor, output.as_mut_ptr(), output.len()) };
        assert!(read_result >= 0, "Failed to read 3D tensor");
        assert_eq!(output, data);

        println!("ObjC++ direct path: 3D tensor test PASSED");

        unsafe { ffi::coreml_tensor_free(tensor) };
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_objcpp_direct_memory_stability() {
        if !unsafe { ffi::coreml_supports_mltensor() } {
            println!("Skipping - MLTensor API not available (requires macOS 15+)");
            return;
        }

        // Create and free multiple tensors to test memory stability
        println!("ObjC++ direct path: Starting memory stability test...");

        for iteration in 0..20 {
            let size = 64; // 8x8 tensor
            let data: Vec<f32> = (0..size).map(|i| (i + iteration * size) as f32).collect();
            let shape = vec![8usize, 8];

            let tensor = unsafe {
                ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len())
            };
            assert!(
                tensor.is_valid(),
                "Failed to create tensor at iteration {}",
                iteration
            );

            // Verify data
            let mut output = vec![0.0f32; size];
            let read_result =
                unsafe { ffi::coreml_tensor_to_floats(tensor, output.as_mut_ptr(), output.len()) };
            assert!(
                read_result >= 0,
                "Failed to read tensor at iteration {}",
                iteration
            );
            assert_eq!(output, data, "Data mismatch at iteration {}", iteration);

            // Free immediately
            unsafe { ffi::coreml_tensor_free(tensor) };
        }

        println!("ObjC++ direct path: memory stability test PASSED (20 iterations)");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_objcpp_vs_swift_bridge_comparison() {
        // Compare results between ObjC++ and Swift bridges (if both available)
        if !unsafe { ffi::coreml_supports_mltensor() } {
            println!("Skipping - MLTensor API not available (requires macOS 15+)");
            return;
        }

        let swift_available = unsafe { ffi::swift_coreml_supports_mltensor() };
        if !swift_available {
            println!("Swift bridge not available, skipping comparison test");
            return;
        }

        println!("Both ObjC++ and Swift bridges available - running comparison...");

        let data = vec![1.0f32, 2.0, 3.0, 4.0];
        let shape = vec![2usize, 2];

        // Create via ObjC++
        let objc_tensor =
            unsafe { ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len()) };
        assert!(objc_tensor.is_valid(), "ObjC++ tensor creation failed");

        // Create via Swift
        let swift_tensor = unsafe {
            ffi::swift_coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len())
        };
        assert!(!swift_tensor.is_null(), "Swift tensor creation failed");

        // Scale both by 2.5
        let objc_scaled = unsafe { ffi::coreml_tensor_scale(objc_tensor, 2.5) };
        let swift_scaled = unsafe { ffi::swift_coreml_tensor_scale(swift_tensor, 2.5) };

        assert!(objc_scaled.is_valid(), "ObjC++ scale failed");
        assert!(!swift_scaled.is_null(), "Swift scale failed");

        // Read results from both
        let mut objc_output = vec![0.0f32; 4];
        let mut swift_output = vec![0.0f32; 4];

        let objc_read = unsafe {
            ffi::coreml_tensor_to_floats(objc_scaled, objc_output.as_mut_ptr(), objc_output.len())
        };
        let swift_read = unsafe {
            ffi::swift_coreml_tensor_to_floats(
                swift_scaled,
                swift_output.as_mut_ptr(),
                swift_output.len(),
            )
        };

        assert!(objc_read >= 0, "ObjC++ read failed");
        assert!(swift_read >= 0, "Swift read failed");

        // Compare results
        let expected = vec![2.5f32, 5.0, 7.5, 10.0];
        assert_eq!(objc_output, expected, "ObjC++ output mismatch");
        assert_eq!(swift_output, expected, "Swift output mismatch");
        assert_eq!(
            objc_output, swift_output,
            "ObjC++ and Swift outputs differ!"
        );

        println!("ObjC++ vs Swift comparison: BOTH MATCH - {:?}", objc_output);

        // Clean up
        unsafe {
            ffi::coreml_tensor_free(objc_tensor);
            ffi::coreml_tensor_free(objc_scaled);
            ffi::swift_coreml_tensor_free(swift_tensor);
            ffi::swift_coreml_tensor_free(swift_scaled);
        };
    }

    // ========== macOS 26+ (Tahoe) Enhanced API Tests ==========

    #[test]
    #[cfg(target_os = "macos")]
    fn test_api_version_detection() {
        let version = get_mltensor_api_version();
        println!("MLTensor API version: {:?}", version);

        match version {
            MltensorApiVersion::NotAvailable => {
                println!("MLTensor not available (pre-macOS 15)");
            }
            MltensorApiVersion::Sequoia => {
                println!("macOS 15.x (Sequoia) - Basic MLTensor API");
            }
            MltensorApiVersion::Tahoe => {
                println!("macOS 26.x (Tahoe) - Enhanced MLComputePolicy API");
            }
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_system_capabilities() {
        let caps = get_system_capabilities();
        println!("System capabilities bitmask: 0x{:x}", caps);

        if caps & capabilities::MLTENSOR_AVAILABLE != 0 {
            println!("  - MLTensor available (macOS 15+)");
        }
        if caps & capabilities::ENHANCED_API != 0 {
            println!("  - Enhanced APIs available (macOS 26+)");
        }
        if caps & capabilities::NEURAL_ENGINE != 0 {
            println!("  - Neural Engine (ANE) available");
        }
        if caps & capabilities::GPU != 0 {
            println!("  - GPU available");
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_has_enhanced_api() {
        let enhanced = has_enhanced_api();
        println!("Has macOS 26+ enhanced API: {}", enhanced);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_has_neural_engine() {
        let ane = has_neural_engine();
        println!("Has Neural Engine (ANE): {}", ane);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_tensor_with_compute_units() {
        if !MLTensor::is_available() {
            println!("Skipping - MLTensor not available");
            return;
        }

        let data = vec![1.0f32, 2.0, 3.0, 4.0];
        let shape = vec![2, 2];

        // Test with different compute unit preferences
        for units in [
            ComputeUnitPreference::CpuOnly,
            ComputeUnitPreference::CpuAndGpu,
            ComputeUnitPreference::CpuAndNeuralEngine,
            ComputeUnitPreference::All,
        ] {
            let tensor = MLTensor::from_floats_with_compute_units(&data, &shape, units);
            match tensor {
                Ok(t) => {
                    let result = t.to_vec().unwrap();
                    assert_eq!(result, data, "Data mismatch with {:?}", units);
                    println!("Tensor creation with {:?} succeeded", units);
                }
                Err(e) => {
                    println!("Tensor creation with {:?} failed: {}", units, e);
                }
            }
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_matmul_with_compute_units() {
        if !MLTensor::is_available() {
            println!("Skipping - MLTensor not available");
            return;
        }

        let data1 = vec![1.0f32, 2.0, 3.0, 4.0];
        let data2 = vec![5.0f32, 6.0, 7.0, 8.0];
        let shape = vec![2, 2];

        let t1 = MLTensor::from_floats(&data1, &shape).unwrap();
        let t2 = MLTensor::from_floats(&data2, &shape).unwrap();

        // Test matmul with ANE compute units (optimal on Apple Silicon)
        let product = t1.matmul_with_compute_units(&t2, ComputeUnitPreference::CpuAndNeuralEngine);
        match product {
            Ok(p) => {
                let result = p.to_vec().unwrap();
                let expected = vec![19.0f32, 22.0, 43.0, 50.0];
                assert_eq!(result, expected, "Matmul with ANE compute units failed");
                println!("Matmul with ANE compute units: {:?}", result);
            }
            Err(e) => {
                println!("Matmul with ANE compute units failed: {}", e);
            }
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_to_vec_async() {
        if !MLTensor::is_available() {
            println!("Skipping - MLTensor not available");
            return;
        }

        let data = vec![1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let shape = vec![2, 4];

        let tensor = MLTensor::from_floats(&data, &shape).unwrap();

        // Test async materialization (will use async API on macOS 26+)
        let result = tensor.to_vec_async(true);
        match result {
            Ok(r) => {
                assert_eq!(r, data, "Async materialization data mismatch");
                println!("Async materialization succeeded: {:?}", r);
            }
            Err(e) => {
                // May fail if scalars not cached or on older macOS
                println!("Async materialization: {}", e);
                // Fall back to sync
                let sync_result = tensor.to_vec().unwrap();
                assert_eq!(sync_result, data);
            }
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_swift_v2_api_direct() {
        if !swift_bridge_available() {
            println!("Skipping - Swift bridge not available");
            return;
        }

        // Test v2 API functions directly
        let version = unsafe { ffi::swift_coreml_mltensor_api_version() };
        println!("Swift bridge API version: {}", version);

        let caps = unsafe { ffi::swift_coreml_system_capabilities() };
        println!("Swift bridge capabilities: 0x{:x}", caps);

        // Test tensor creation with v2 API
        let data = vec![1.0f32, 2.0, 3.0, 4.0];
        let shape = vec![2usize, 2];

        let handle = unsafe {
            ffi::swift_coreml_create_tensor_f32_v2(
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                ComputeUnitPreference::All as i32,
            )
        };

        if handle.is_null() {
            println!("v2 tensor creation returned null (may fall back to v1)");
        } else {
            println!("v2 tensor creation succeeded");

            // Test v2 matmul
            let data2 = vec![5.0f32, 6.0, 7.0, 8.0];
            let handle2 = unsafe {
                ffi::swift_coreml_create_tensor_f32_v2(
                    data2.as_ptr(),
                    shape.as_ptr(),
                    shape.len(),
                    ComputeUnitPreference::All as i32,
                )
            };

            if !handle2.is_null() {
                let product = unsafe {
                    ffi::swift_coreml_tensor_matmul_v2(
                        handle,
                        handle2,
                        ComputeUnitPreference::CpuAndNeuralEngine as i32,
                    )
                };

                if !product.is_null() {
                    println!("v2 matmul with ANE preference succeeded");
                    unsafe { ffi::swift_coreml_tensor_free(product) };
                }

                unsafe { ffi::swift_coreml_tensor_free(handle2) };
            }

            unsafe { ffi::swift_coreml_tensor_free(handle) };
        }
    }

    // ========== Stub Mode Tests ==========

    #[test]
    fn test_stub_mode_creation() {
        let backend = CoreMLBackend::new_stub(ComputeUnits::All).unwrap();
        assert!(backend.is_stub_mode());
        assert_eq!(backend.device_name(), "CoreML (Stub Mode)");
    }

    #[test]
    fn test_stub_mode_run_step() {
        use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};

        let mut backend = CoreMLBackend::new_stub(ComputeUnits::All).unwrap();

        // Create IO buffers with small vocab size for testing
        let vocab_size = 100;
        let mut io = IoBuffers::new(vocab_size);
        io.input_ids = vec![1, 2, 3];

        // Create router ring with no active adapters
        let ring = RouterRing::new(0);

        // Run step should succeed in stub mode
        let result = backend.run_step(&ring, &mut io);
        assert!(result.is_ok(), "run_step failed: {:?}", result);

        // Position should be incremented
        assert_eq!(io.position, 1);

        // Output logits should be normalized (sum to ~1.0)
        let sum: f32 = io.output_logits.iter().sum();
        assert!(
            (sum - 1.0).abs() < 0.001,
            "Logits not normalized: sum = {}",
            sum
        );

        // Metrics should be updated
        let metrics = backend.get_metrics();
        assert_eq!(metrics.total_operations, 1);
        assert_eq!(metrics.successful_operations, 1);
    }

    #[test]
    fn test_stub_mode_with_adapters() {
        use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};

        let mut backend = CoreMLBackend::new_stub(ComputeUnits::All).unwrap();

        // Manually insert adapter weights into cache
        backend.adapter_cache.insert(1, vec![0.1; 100]);
        backend.adapter_cache.insert(2, vec![0.2; 100]);

        let vocab_size = 100;
        let mut io = IoBuffers::new(vocab_size);
        io.input_ids = vec![1, 2, 3];

        // Create router ring with 2 active adapters
        let mut ring = RouterRing::new(2);
        ring.set(&[1, 2], &[16384, 8192]); // Q15 gates: 0.5 and 0.25

        // Run step should succeed
        let result = backend.run_step(&ring, &mut io);
        assert!(
            result.is_ok(),
            "run_step with adapters failed: {:?}",
            result
        );

        // Position should be incremented
        assert_eq!(io.position, 1);

        // Logits should still be normalized
        let sum: f32 = io.output_logits.iter().sum();
        assert!(
            (sum - 1.0).abs() < 0.001,
            "Logits not normalized: sum = {}",
            sum
        );
    }

    #[test]
    fn test_coreml_hot_swap_attach_switch_detach_stub() {
        fn make_weights(values: &[f32]) -> Vec<u8> {
            let bytes = unsafe {
                std::slice::from_raw_parts(values.as_ptr() as *const u8, values.len() * 4)
            };
            let tensor = TensorView::new(safetensors::Dtype::F32, vec![values.len()], bytes)
                .expect("tensor view");
            serialize(
                vec![("adapter.weight".to_string(), tensor)],
                &Default::default(),
            )
            .expect("serialize adapter weights")
        }

        // Build minimal safetensors payloads for two adapters.
        let weights1 = make_weights(&[1.0, 2.0, 3.0, 4.0]);
        let weights2 = make_weights(&[5.0, 6.0, 7.0, 8.0]);

        let mut backend = CoreMLBackend::new_stub(ComputeUnits::All).unwrap();

        // Base: no adapters attached.
        assert!(backend.attached_adapter_ids().is_empty());

        // Attach first adapter via load (sidecar semantics).
        backend.load_adapter(1, &weights1).unwrap();
        assert_eq!(backend.attached_adapter_ids(), vec![1]);

        // Load a second adapter and confirm both are visible.
        backend.load_adapter(2, &weights2).unwrap();
        assert_eq!(backend.attached_adapter_ids(), vec![1, 2]);

        // Switch to adapter 2, which should detach adapter 1.
        backend.switch_adapter(2).unwrap();
        assert_eq!(backend.attached_adapter_ids(), vec![2]);

        // Detach the active adapter; cache should drop it.
        backend.detach_adapter(2).unwrap();
        assert!(backend.attached_adapter_ids().is_empty());
        assert!(!backend.adapter_cache.contains_key(&2));
    }

    #[test]
    fn test_stub_mode_deterministic() {
        use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};

        let mut backend1 = CoreMLBackend::new_stub(ComputeUnits::All).unwrap();
        let mut backend2 = CoreMLBackend::new_stub(ComputeUnits::All).unwrap();

        let vocab_size = 100;

        // Run same operation on both backends
        let mut io1 = IoBuffers::new(vocab_size);
        io1.input_ids = vec![1, 2, 3];
        let ring1 = RouterRing::new(0);

        let mut io2 = IoBuffers::new(vocab_size);
        io2.input_ids = vec![1, 2, 3];
        let ring2 = RouterRing::new(0);

        backend1.run_step(&ring1, &mut io1).unwrap();
        backend2.run_step(&ring2, &mut io2).unwrap();

        // Results should be identical (deterministic)
        for (i, (l1, l2)) in io1
            .output_logits
            .iter()
            .zip(io2.output_logits.iter())
            .enumerate()
        {
            assert!(
                (l1 - l2).abs() < 1e-6,
                "Non-deterministic output at index {}: {} vs {}",
                i,
                l1,
                l2
            );
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_ane_detection_comprehensive() {
        // Test 1: Basic ANE availability functions
        let ane_available = has_neural_engine();
        let neural_engine_available = is_neural_engine_available();

        println!("\n=== ANE Detection Comprehensive Test ===");
        println!("has_neural_engine(): {}", ane_available);
        println!("is_neural_engine_available(): {}", neural_engine_available);

        // Test 2: Create backend and check ANE status
        match CoreMLBackend::new_default(ComputeUnits::All) {
            Ok(backend) => {
                let ane_status = backend.ane_status();
                println!("\nANE Status from Backend:");
                println!("  Available: {}", ane_status.available);
                println!("  Generation: {:?}", ane_status.generation);
                println!("  Max Batch Size: {}", ane_status.max_batch_size);
                println!("  Deterministic: {}", ane_status.deterministic);

                // Map generation to chip
                if let Some(gen) = ane_status.generation {
                    let chip = match gen {
                        4 => "M1",
                        5 => "M2",
                        6 => "M3",
                        7 => "M4",
                        n if n >= 8 => "M5+",
                        _ => "Unknown",
                    };
                    println!("  Chip: Apple {} (Generation {})", chip, gen);
                }

                // Verify consistency
                assert_eq!(
                    ane_available, ane_status.available,
                    "ANE availability mismatch between detection functions"
                );
            }
            Err(e) => {
                println!(
                    "Note: Failed to create backend (expected on unsupported systems): {}",
                    e
                );
            }
        }
    }

    #[test]
    fn test_ane_detection_handles_non_macos() {
        // On non-macOS, these should return false
        #[cfg(not(target_os = "macos"))]
        {
            assert!(
                !has_neural_engine(),
                "ANE should not be available on non-macOS"
            );
            assert!(
                !is_neural_engine_available(),
                "Neural engine should not be available on non-macOS"
            );
        }
    }
}
