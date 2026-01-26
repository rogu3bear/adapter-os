//! CoreML kernel implementation for Neural Engine acceleration
//!
//! This crate provides the CoreML backend for adapterOS, enabling inference
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

pub use crate::export::validate_coreml_fusion;
use adapteros_core::{AosError, B3Hash, Result, Q15_GATE_DENOMINATOR};
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
pub mod placement;

pub use placement::{
    resolve_placement, CoreMLGraph, CoreMLGraphNode, PlacementMetrics, PlacementResolution,
};

pub use config::{ComputeUnits, CoreMLConfig, CoreMLModelParams};
pub use ffi::{
    capabilities, AneCheckResult, ComputeUnitPreference, CoreMLAsyncCallback, MLTensorHandle,
    MltensorApiVersion, OperationType,
};

pub use hybrid::{HybridCoreMLBackend, LmHeadLoRA};
pub use matmul::{axpy, matmul_accelerate, matvec_accelerate};

pub use export::CoreMLFusionMetadata;
pub use export::{
    export_coreml_adapter, export_coreml_adapter_async, validate_coreml_ops,
    validate_coreml_weights, validate_output_path, CoreMLExportOutcome, CoreMLExportRequest,
};

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

    /// Layer Normalization: (x - mean) / sqrt(var + eps) * weight + bias
    ///
    /// Normalizes the input tensor along the last dimension using standard
    /// layer normalization. Critical for transformer model inference.
    ///
    /// # Arguments
    /// * `weight` - Scale weights (gamma), must match last dimension of tensor
    /// * `bias` - Bias (beta), must match last dimension of tensor
    /// * `eps` - Small constant for numerical stability (typically 1e-5)
    ///
    /// # Errors
    /// Returns `AosError::Kernel` if:
    /// - Operation fails
    /// - Weight/bias length doesn't match tensor's last dimension
    pub fn layernorm(&self, weight: &[f32], bias: &[f32], eps: f32) -> Result<Self> {
        #[cfg(target_os = "macos")]
        {
            match self.bridge_type {
                TensorBridgeType::Swift => {
                    let result = unsafe {
                        ffi::swift_coreml_tensor_layernorm(
                            self.swift_handle,
                            weight.as_ptr(),
                            weight.len(),
                            bias.as_ptr(),
                            bias.len(),
                            eps,
                        )
                    };
                    if result.is_null() {
                        return Err(AosError::Kernel("Layer normalization failed".to_string()));
                    }
                    // LayerNorm preserves shape, copy from input
                    Ok(Self {
                        swift_handle: result,
                        objc_handle: self.objc_handle,
                        bridge_type: TensorBridgeType::Swift,
                    })
                }
                TensorBridgeType::ObjCpp => {
                    // ObjC++ bridge doesn't have layernorm, return error
                    Err(AosError::Kernel(
                        "Layer normalization not available on ObjC++ bridge".to_string(),
                    ))
                }
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = (weight, bias, eps);
            Err(AosError::Kernel(
                "MLTensor only available on macOS".to_string(),
            ))
        }
    }

    /// RMS Normalization: x * rsqrt(mean(x^2) + eps) * weight
    ///
    /// Root Mean Square layer normalization used in LLaMA-style models.
    /// More efficient than LayerNorm as it skips mean subtraction.
    ///
    /// # Arguments
    /// * `weight` - Scale weights (gamma), must match last dimension of tensor
    /// * `eps` - Small constant for numerical stability (typically 1e-5)
    ///
    /// # Errors
    /// Returns `AosError::Kernel` if:
    /// - Operation fails
    /// - Weight length doesn't match tensor's last dimension
    pub fn rms_norm(&self, weight: &[f32], eps: f32) -> Result<Self> {
        #[cfg(target_os = "macos")]
        {
            match self.bridge_type {
                TensorBridgeType::Swift => {
                    let result = unsafe {
                        ffi::swift_coreml_tensor_rms_norm(
                            self.swift_handle,
                            weight.as_ptr(),
                            weight.len(),
                            eps,
                        )
                    };
                    if result.is_null() {
                        return Err(AosError::Kernel("RMS normalization failed".to_string()));
                    }
                    // RMSNorm preserves shape, copy from input
                    Ok(Self {
                        swift_handle: result,
                        objc_handle: self.objc_handle,
                        bridge_type: TensorBridgeType::Swift,
                    })
                }
                TensorBridgeType::ObjCpp => {
                    // ObjC++ bridge doesn't have rms_norm, return error
                    Err(AosError::Kernel(
                        "RMS normalization not available on ObjC++ bridge".to_string(),
                    ))
                }
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = (weight, eps);
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
}

unsafe impl Send for CoreMLBackend {}
unsafe impl Sync for CoreMLBackend {}

impl CoreMLBackend {
    #[inline]
    fn gate_q15_to_f32(gate: i16) -> f32 {
        gate as f32 / Q15_GATE_DENOMINATOR
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
                // Untrack model from ANE memory metrics
                if let Some(hash) = &self.model_hash {
                    ffi::record_model_unload(&hash.to_hex());
                } else if let Some(path) = &self.base_model_path {
                    ffi::record_model_unload(&path.to_string_lossy());
                }

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

            // Track model in ANE memory metrics
            // Estimate footprint based on Manifest.json size as a heuristic (or use 50MB baseline)
            let footprint_bytes = std::fs::metadata(&hash_path)
                .map(|m| m.len().max(50 * 1024 * 1024))
                .unwrap_or(50 * 1024 * 1024);

            if let Some(hash) = &self.model_hash {
                ffi::record_model_load(&hash.to_hex(), footprint_bytes);
            } else {
                ffi::record_model_load(&path_str, footprint_bytes);
            }

            tracing::info!(
                model_path = %load_path.display(),
                compiled_path = %compiled_path.display(),
                hash = %self.model_hash.as_ref().map(|h| h.to_short_hex()).unwrap_or_else(|| "unknown".to_string()),
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
        use adapteros_storage::platform::common::PlatformUtils;

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

        tracing::info!(
            model_path = %model_path.display(),
            cache_dir = %cache_dir.display(),
            "Compiling CoreML model"
        );
        let compile_start = std::time::Instant::now();

        let status = Command::new("xcrun")
            .args(["coremlc", "compile", model_str, cache_str])
            .status()
            .map_err(|e| AosError::Kernel(format!("Failed to spawn coremlc: {}", e)))?;

        let compile_elapsed = compile_start.elapsed();
        if !status.success() {
            tracing::error!(
                model_path = %model_path.display(),
                elapsed_ms = compile_elapsed.as_millis(),
                status_code = ?status.code(),
                "CoreML compilation failed"
            );
            return Err(AosError::Kernel(format!(
                "coremlc compile failed (status {:?}) for {}",
                status.code(),
                model_path.display()
            )));
        }

        tracing::info!(
            model_path = %model_path.display(),
            elapsed_ms = compile_elapsed.as_millis(),
            "CoreML compilation succeeded"
        );

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
        tracing::trace!(
            target: "ffi.coreml",
            input_len = io.input_ids.len(),
            adapter_count = indices.len(),
            ring_len = ring_len,
            "FFI call: coreml_run_inference_with_lora"
        );

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
        tracing::trace!(
            target: "ffi.coreml",
            input_len = io.input_ids.len(),
            ring_len = ring_len,
            "FFI call: coreml_run_inference"
        );

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
            metallib_verified: false,
            manifest: None,
            rng_seed_method,
            floating_point_mode,
            determinism_level: if deterministic {
                attestation::DeterminismLevel::BoundedTolerance
            } else {
                attestation::DeterminismLevel::None
            },
            compiler_flags: vec![],
            deterministic,
            runtime_version: Some(format!("{:?}", self.mltensor_api_version)),
            device_id: Some(self.device_name.clone()),
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
}

impl Drop for CoreMLBackend {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        {
            if !self.model_handle.is_null() {
                // Untrack model from ANE memory metrics
                if let Some(hash) = &self.model_hash {
                    ffi::record_model_unload(&hash.to_hex());
                } else if let Some(path) = &self.base_model_path {
                    ffi::record_model_unload(&path.to_string_lossy());
                }

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
#[path = "tests.rs"]
mod tests;
