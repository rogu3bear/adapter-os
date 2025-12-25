//! MLTensor Safe Wrapper API (macOS 15+) with Runtime Dispatch
//!
//! This module provides a safe Rust wrapper around CoreML's MLTensor API,
//! which is available on macOS 15+ (Sequoia). MLTensor enables high-performance
//! tensor operations with automatic memory management and type safety.

use crate::ffi::{self, MLTensorHandle, MltensorApiVersion};
use adapteros_core::{AosError, Result};

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
    get_system_capabilities() & crate::ffi::capabilities::ENHANCED_API != 0
}

/// Check if Neural Engine (ANE) is available
pub fn has_neural_engine() -> bool {
    get_system_capabilities() & crate::ffi::capabilities::NEURAL_ENGINE != 0
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
        compute_units: crate::ffi::ComputeUnitPreference,
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
        compute_units: crate::ffi::ComputeUnitPreference,
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
        compute_units: crate::ffi::ComputeUnitPreference,
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
