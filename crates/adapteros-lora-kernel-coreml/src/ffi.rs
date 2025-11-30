//! Foreign Function Interface for CoreML.framework

/// ANE check result - FFI-safe struct
#[repr(C)]
pub struct AneCheckResult {
    pub available: bool,
    pub generation: u8,
}

/// MLTensor handle for modern tensor operations (macOS 15+)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MLTensorHandle {
    /// Opaque pointer to MLTensor object
    pub tensor_ptr: *mut std::ffi::c_void,
    /// Shape dimensions (max 16)
    pub shape: [usize; 16],
    /// Number of dimensions
    pub rank: u32,
}

impl Default for MLTensorHandle {
    fn default() -> Self {
        Self {
            tensor_ptr: std::ptr::null_mut(),
            shape: [0; 16],
            rank: 0,
        }
    }
}

impl MLTensorHandle {
    /// Check if handle is valid
    pub fn is_valid(&self) -> bool {
        !self.tensor_ptr.is_null() && self.rank > 0
    }

    /// Get total number of elements
    pub fn num_elements(&self) -> usize {
        if self.rank == 0 {
            return 0;
        }
        self.shape[..self.rank as usize].iter().product()
    }
}

#[cfg(target_os = "macos")]
extern "C" {
    /// Check if CoreML framework is available
    pub fn coreml_is_available() -> bool;

    /// Check Neural Engine availability
    pub fn coreml_check_ane() -> AneCheckResult;

    /// Load a CoreML model
    pub fn coreml_load_model(
        path: *const i8,
        path_len: usize,
        compute_units: i32,
    ) -> *mut std::ffi::c_void;

    /// Unload a CoreML model
    pub fn coreml_unload_model(handle: *mut std::ffi::c_void);

    /// Run inference on loaded model
    pub fn coreml_run_inference(
        handle: *mut std::ffi::c_void,
        input_ids: *const u32,
        input_len: usize,
        output_logits: *mut f32,
        output_len: usize,
        adapter_indices: *const u16,
        adapter_gates: *const i16,
        num_adapters: usize,
    ) -> i32;

    /// Run inference with LoRA adapter support
    pub fn coreml_run_inference_with_lora(
        handle: *mut std::ffi::c_void,
        input_ids: *const u32,
        input_len: usize,
        output_logits: *mut f32,
        output_len: usize,
        adapter_indices: *const u16,
        adapter_gates: *const i16,
        num_adapters: usize,
        lora_deltas: *const *const f32,
        delta_lens: *const usize,
    ) -> i32;

    /// Perform health check on model
    pub fn coreml_health_check(handle: *mut std::ffi::c_void) -> i32;

    /// Get last error message
    pub fn coreml_get_last_error(buffer: *mut i8, buffer_len: usize) -> usize;

    // ========== MLTensor API (macOS 15+) ==========

    /// Check if MLTensor API is available (requires macOS 15+)
    pub fn coreml_supports_mltensor() -> bool;

    /// Create MLTensor from float array
    ///
    /// # Arguments
    /// * `scalars` - Pointer to float data
    /// * `shape` - Pointer to shape dimensions
    /// * `rank` - Number of dimensions
    ///
    /// # Returns
    /// MLTensorHandle with valid tensor_ptr on success, null on failure
    pub fn coreml_create_tensor_f32(
        scalars: *const f32,
        shape: *const usize,
        rank: usize,
    ) -> MLTensorHandle;

    /// Apply softmax to tensor along dimension
    ///
    /// # Arguments
    /// * `tensor` - Input tensor handle
    /// * `dim` - Dimension for softmax (-1 for last dimension)
    ///
    /// # Returns
    /// New tensor with softmax applied
    pub fn coreml_tensor_softmax(tensor: MLTensorHandle, dim: i32) -> MLTensorHandle;

    /// Add two tensors element-wise
    pub fn coreml_tensor_add(tensor1: MLTensorHandle, tensor2: MLTensorHandle) -> MLTensorHandle;

    /// Scale tensor by scalar value
    pub fn coreml_tensor_scale(tensor: MLTensorHandle, scale: f32) -> MLTensorHandle;

    /// Matrix multiplication of two tensors
    pub fn coreml_tensor_matmul(tensor1: MLTensorHandle, tensor2: MLTensorHandle)
        -> MLTensorHandle;

    /// Materialize tensor to float array
    ///
    /// # Arguments
    /// * `tensor` - Tensor to materialize
    /// * `output` - Output buffer for floats
    /// * `output_len` - Size of output buffer
    ///
    /// # Returns
    /// Number of elements copied on success, negative error code on failure
    pub fn coreml_tensor_to_floats(
        tensor: MLTensorHandle,
        output: *mut f32,
        output_len: usize,
    ) -> i32;

    /// Free MLTensor handle
    pub fn coreml_tensor_free(handle: MLTensorHandle);

    // ========== Async Prediction API ==========

    /// Async prediction with callback
    ///
    /// # Arguments
    /// * `handle` - Model handle
    /// * `input_ids` - Input token IDs
    /// * `input_len` - Length of input
    /// * `callback` - Callback function invoked when prediction completes
    /// * `user_data` - User data passed to callback
    ///
    /// # Safety
    /// - `handle` must be valid model from `coreml_load_model`
    /// - `input_ids` must point to valid u32 array of size `input_len`
    /// - `callback` will be called exactly once with results
    /// - `user_data` passed to callback unchanged (can be null)
    /// - The output buffer passed to callback must be freed by caller using libc::free
    pub fn coreml_predict_async(
        handle: *mut std::ffi::c_void,
        input_ids: *const u32,
        input_len: usize,
        callback: CoreMLAsyncCallback,
        user_data: *mut std::ffi::c_void,
    );

    /// Async prediction with LoRA adapter support
    ///
    /// # Arguments
    /// * `handle` - Model handle
    /// * `input_ids` - Input token IDs
    /// * `input_len` - Length of input
    /// * `adapter_indices` - Adapter index array
    /// * `adapter_gates` - Q15 quantized gate values
    /// * `num_adapters` - Number of adapters
    /// * `lora_deltas` - Array of pointers to pre-computed LoRA deltas
    /// * `delta_lens` - Array of delta lengths
    /// * `callback` - Callback function invoked when prediction completes
    /// * `user_data` - User data passed to callback
    ///
    /// # Safety
    /// Same as `coreml_predict_async`, plus:
    /// - `adapter_indices`, `adapter_gates` must point to arrays of size `num_adapters`
    /// - `lora_deltas` must point to array of `num_adapters` float pointers
    /// - `delta_lens` must point to array of `num_adapters` sizes
    pub fn coreml_predict_async_with_lora(
        handle: *mut std::ffi::c_void,
        input_ids: *const u32,
        input_len: usize,
        adapter_indices: *const u16,
        adapter_gates: *const i16,
        num_adapters: usize,
        lora_deltas: *const *const f32,
        delta_lens: *const usize,
        callback: CoreMLAsyncCallback,
        user_data: *mut std::ffi::c_void,
    );
}

/// Callback type for async predictions
///
/// # Arguments
/// * `status` - 0 on success, negative error code on failure
/// * `output` - Pointer to output logits (valid only on success, must be freed with libc::free)
/// * `output_len` - Number of output elements
/// * `user_data` - User data passed to coreml_predict_async
pub type CoreMLAsyncCallback = extern "C" fn(
    status: i32,
    output: *mut f32,
    output_len: usize,
    user_data: *mut std::ffi::c_void,
);

// ========== Swift Bridge FFI (macOS 15+ MLTensor) ==========

/// Compute unit preference for macOS 26+ MLTensor operations
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComputeUnitPreference {
    /// CPU only
    CpuOnly = 0,
    /// CPU and GPU
    CpuAndGpu = 1,
    /// CPU and Neural Engine (ANE)
    CpuAndNeuralEngine = 2,
    /// All available compute units (default)
    #[default]
    All = 3,
}

/// Operation type for compute unit scheduling hints
///
/// Different operation types have different optimal compute unit assignments:
/// - MatMul/Attention: Highly optimized on ANE for transformer inference
/// - Softmax/ElementWise: Transcendental ops run better on GPU (no ANE support for exp())
///
/// Use `OperationType::preferred_compute_units()` to get the optimal compute units
/// for each operation type based on hardware capabilities and production mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationType {
    /// Matrix multiplication - ANE optimized
    MatMul,
    /// Softmax operation - GPU preferred (transcendental exp() not on ANE)
    Softmax,
    /// Attention computation - ANE optimized
    Attention,
    /// Element-wise operations (add, mul, scale) - GPU preferred
    ElementWise,
    /// Tensor creation/materialization - follows parent preference
    TensorOp,
}

impl OperationType {
    /// Get the preferred compute units for this operation type
    ///
    /// # Arguments
    /// * `production_mode` - If true, prioritizes determinism (ANE) over performance
    ///
    /// # Returns
    /// The recommended `ComputeUnitPreference` for this operation type.
    ///
    /// # Scheduling Strategy
    ///
    /// In **production mode** (determinism required):
    /// - All operations use `CpuAndNeuralEngine` for guaranteed reproducibility
    /// - ANE execution is deterministic across runs
    ///
    /// In **development mode** (maximum performance):
    /// - MatMul/Attention → `CpuAndNeuralEngine` (ANE is 2-3x faster than GPU for these)
    /// - Softmax/ElementWise → `CpuAndGpu` (transcendental ops have no ANE support)
    /// - TensorOp → `All` (let CoreML decide based on data flow)
    ///
    /// # Example
    /// ```rust,ignore
    /// use adapteros_lora_kernel_coreml::ffi::{OperationType, ComputeUnitPreference};
    ///
    /// let matmul_units = OperationType::MatMul.preferred_compute_units(false);
    /// assert_eq!(matmul_units, ComputeUnitPreference::CpuAndNeuralEngine);
    ///
    /// let softmax_units = OperationType::Softmax.preferred_compute_units(false);
    /// assert_eq!(softmax_units, ComputeUnitPreference::CpuAndGpu);
    /// ```
    pub fn preferred_compute_units(&self, production_mode: bool) -> ComputeUnitPreference {
        if production_mode {
            // Production: Always use ANE for determinism
            ComputeUnitPreference::CpuAndNeuralEngine
        } else {
            // Development: Optimize per-operation
            match self {
                // Matrix operations are highly optimized on ANE
                Self::MatMul | Self::Attention => ComputeUnitPreference::CpuAndNeuralEngine,
                // Transcendental ops (exp, log) run on GPU, not ANE
                Self::Softmax | Self::ElementWise => ComputeUnitPreference::CpuAndGpu,
                // Let CoreML decide for tensor ops
                Self::TensorOp => ComputeUnitPreference::All,
            }
        }
    }

    /// Check if this operation type is ANE-optimized
    pub fn is_ane_optimized(&self) -> bool {
        matches!(self, Self::MatMul | Self::Attention)
    }

    /// Check if this operation involves transcendental functions (not supported on ANE)
    pub fn uses_transcendentals(&self) -> bool {
        matches!(self, Self::Softmax)
    }
}

/// MLTensor API version levels
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MltensorApiVersion {
    /// MLTensor not available (pre-macOS 15)
    NotAvailable = 0,
    /// macOS 15.x (Sequoia) - Basic MLTensor API
    Sequoia = 1,
    /// macOS 26.x (Tahoe) - Enhanced MLComputePolicy API
    Tahoe = 2,
}

impl From<i32> for MltensorApiVersion {
    fn from(value: i32) -> Self {
        match value {
            0 => Self::NotAvailable,
            1 => Self::Sequoia,
            2 => Self::Tahoe,
            _ => Self::NotAvailable,
        }
    }
}

/// System capability flags returned by swift_coreml_system_capabilities
pub mod capabilities {
    /// MLTensor available (macOS 15+)
    pub const MLTENSOR_AVAILABLE: i32 = 1;
    /// Enhanced MLComputePolicy API (macOS 26+)
    pub const ENHANCED_API: i32 = 2;
    /// Neural Engine (ANE) available
    pub const NEURAL_ENGINE: i32 = 4;
    /// GPU available
    pub const GPU: i32 = 8;
}

#[cfg(target_os = "macos")]
extern "C" {
    /// Check MLTensor support via Swift bridge
    pub fn swift_coreml_supports_mltensor() -> bool;

    /// Get MLTensor API version level
    ///
    /// # Returns
    /// - 0: Not available (pre-macOS 15)
    /// - 1: macOS 15.x (Sequoia) - Basic MLTensor
    /// - 2: macOS 26.x (Tahoe) - Enhanced APIs
    pub fn swift_coreml_mltensor_api_version() -> i32;

    /// Get system capability bitmask
    ///
    /// # Returns
    /// Bitmask with:
    /// - Bit 0: MLTensor available
    /// - Bit 1: Enhanced APIs (macOS 26+)
    /// - Bit 2: Neural Engine available
    /// - Bit 3: GPU available
    pub fn swift_coreml_system_capabilities() -> i32;

    /// Create MLTensor from floats via Swift
    pub fn swift_coreml_create_tensor_f32(
        scalars: *const f32,
        shape: *const usize,
        rank: usize,
    ) -> *mut std::ffi::c_void;

    /// Create MLTensor with compute unit preference (macOS 26+ enhanced)
    ///
    /// # Arguments
    /// * `scalars` - Float data pointer
    /// * `shape` - Shape dimensions pointer
    /// * `rank` - Number of dimensions
    /// * `compute_units` - Compute unit preference (0=CPU, 1=CPU+GPU, 2=CPU+ANE, 3=All)
    ///
    /// # Note
    /// On macOS 15-25, this falls back to the basic creation method.
    pub fn swift_coreml_create_tensor_f32_v2(
        scalars: *const f32,
        shape: *const usize,
        rank: usize,
        compute_units: i32,
    ) -> *mut std::ffi::c_void;

    /// Free MLTensor via Swift
    pub fn swift_coreml_tensor_free(handle: *mut std::ffi::c_void);

    /// Softmax operation via Swift
    pub fn swift_coreml_tensor_softmax(
        handle: *mut std::ffi::c_void,
        dim: i32,
    ) -> *mut std::ffi::c_void;

    /// Softmax with compute unit preference (macOS 26+ enhanced)
    pub fn swift_coreml_tensor_softmax_v2(
        handle: *mut std::ffi::c_void,
        dim: i32,
        compute_units: i32,
    ) -> *mut std::ffi::c_void;

    /// Add tensors via Swift
    pub fn swift_coreml_tensor_add(
        handle1: *mut std::ffi::c_void,
        handle2: *mut std::ffi::c_void,
    ) -> *mut std::ffi::c_void;

    /// Scale tensor via Swift
    pub fn swift_coreml_tensor_scale(
        handle: *mut std::ffi::c_void,
        scale: f32,
    ) -> *mut std::ffi::c_void;

    /// Matmul via Swift
    pub fn swift_coreml_tensor_matmul(
        handle1: *mut std::ffi::c_void,
        handle2: *mut std::ffi::c_void,
    ) -> *mut std::ffi::c_void;

    /// Matmul with compute unit preference (macOS 26+ enhanced)
    ///
    /// # Arguments
    /// * `handle1` - First tensor
    /// * `handle2` - Second tensor
    /// * `compute_units` - Compute unit preference (0=CPU, 1=CPU+GPU, 2=CPU+ANE, 3=All)
    pub fn swift_coreml_tensor_matmul_v2(
        handle1: *mut std::ffi::c_void,
        handle2: *mut std::ffi::c_void,
        compute_units: i32,
    ) -> *mut std::ffi::c_void;

    /// Materialize tensor to floats via Swift
    pub fn swift_coreml_tensor_to_floats(
        handle: *mut std::ffi::c_void,
        output: *mut f32,
        output_len: usize,
    ) -> i32;

    /// Materialize tensor with async option (macOS 26+ enhanced)
    ///
    /// # Arguments
    /// * `handle` - Tensor handle
    /// * `output` - Output buffer
    /// * `output_len` - Buffer size
    /// * `use_async` - If true on macOS 26+, uses async shapedArray API
    ///
    /// # Returns
    /// Number of elements on success, negative error code on failure:
    /// - -1: Invalid handle
    /// - -2: Buffer too small
    /// - -3: Async failed
    /// - -4: Scalars not cached
    pub fn swift_coreml_tensor_to_floats_v2(
        handle: *mut std::ffi::c_void,
        output: *mut f32,
        output_len: usize,
        use_async: bool,
    ) -> i32;

    /// Batch matrix multiplication (macOS 26+ optimized)
    ///
    /// # Arguments
    /// * `handles1` - Array of first tensor handles
    /// * `handles2` - Array of second tensor handles
    /// * `count` - Number of tensor pairs
    /// * `results_out` - Output array for result handles
    /// * `compute_units` - Compute unit preference
    ///
    /// # Returns
    /// Number of successful operations, or negative error code
    pub fn swift_coreml_batch_matmul(
        handles1: *const *mut std::ffi::c_void,
        handles2: *const *mut std::ffi::c_void,
        count: usize,
        results_out: *mut *mut std::ffi::c_void,
        compute_units: i32,
    ) -> i32;
}
