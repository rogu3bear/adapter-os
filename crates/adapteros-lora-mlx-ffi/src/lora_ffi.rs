//! Safe Rust wrapper for C++ LoRA adapter FFI handles.
//!
//! Provides RAII lifetime management for mlx_lora_adapter_t pointers
//! returned by the C++ FFI layer.

use adapteros_core::{AosError, Result};
use std::ffi::CString;

/// Safe wrapper around a C++ LoRA adapter handle.
///
/// This struct owns the underlying C++ allocation and frees it on drop.
/// It is NOT Send/Sync because the C++ model wrapper is not thread-safe;
/// access must be serialized through the model's inference_lock.
pub struct FFILoraAdapter {
    ptr: *mut crate::mlx_lora_adapter_t,
    adapter_id: i32,
    num_layers: usize,
}

impl FFILoraAdapter {
    /// Create a new FFI LoRA adapter handle.
    ///
    /// # Arguments
    /// * `adapter_id` - Unique identifier for this adapter
    /// * `num_layers` - Number of transformer layers this adapter covers
    /// * `scale` - Global LoRA scale factor (alpha / rank)
    pub fn new(adapter_id: i32, num_layers: usize, scale: f32) -> Result<Self> {
        crate::ffi_error::clear_ffi_error();
        let ptr = unsafe { crate::mlx_lora_adapter_new(adapter_id, num_layers as i32, scale) };
        if ptr.is_null() {
            let error = crate::ffi_error::get_ffi_error_or("Failed to create LoRA adapter");
            return Err(AosError::Mlx(error));
        }
        Ok(Self {
            ptr,
            adapter_id,
            num_layers,
        })
    }

    /// Set LoRA weights for a specific module in a specific layer.
    ///
    /// # Arguments
    /// * `layer_idx` - Transformer layer index (0-based)
    /// * `module_name` - Projection name (e.g., "q_proj", "k_proj")
    /// * `lora_a` - LoRA A matrix data [rank, in_features]
    /// * `lora_b` - LoRA B matrix data [out_features, rank]
    /// * `a_shape` - Shape of A matrix [rows, cols]
    /// * `b_shape` - Shape of B matrix [rows, cols]
    pub fn set_module(
        &mut self,
        layer_idx: usize,
        module_name: &str,
        lora_a: &[f32],
        lora_b: &[f32],
        a_shape: [usize; 2],
        b_shape: [usize; 2],
    ) -> Result<()> {
        if layer_idx >= self.num_layers {
            return Err(AosError::Validation(format!(
                "Layer index {} out of range (num_layers={})",
                layer_idx, self.num_layers
            )));
        }

        // Validate data sizes match declared shapes
        let expected_a = a_shape[0] * a_shape[1];
        let expected_b = b_shape[0] * b_shape[1];
        if lora_a.len() != expected_a {
            return Err(AosError::Validation(format!(
                "LoRA A data length {} does not match shape {:?} (expected {})",
                lora_a.len(),
                a_shape,
                expected_a
            )));
        }
        if lora_b.len() != expected_b {
            return Err(AosError::Validation(format!(
                "LoRA B data length {} does not match shape {:?} (expected {})",
                lora_b.len(),
                b_shape,
                expected_b
            )));
        }

        let c_module_name = CString::new(module_name)
            .map_err(|e| AosError::Validation(format!("Invalid module name: {}", e)))?;

        crate::ffi_error::clear_ffi_error();

        // Create MLX arrays from flat data
        let a_ptr = unsafe { crate::mlx_array_from_data(lora_a.as_ptr(), lora_a.len() as i32) };
        if a_ptr.is_null() {
            return Err(AosError::Mlx("Failed to create LoRA A array".to_string()));
        }

        // Reshape A to [a_shape[0], a_shape[1]]
        let a_shape_i32 = [a_shape[0] as i32, a_shape[1] as i32];
        let a_reshaped = unsafe { crate::mlx_array_reshape(a_ptr, a_shape_i32.as_ptr(), 2) };
        if a_reshaped.is_null() {
            unsafe { crate::mlx_array_free(a_ptr) };
            return Err(AosError::Mlx("Failed to reshape LoRA A".to_string()));
        }
        // Free the original flat array (reshape creates a new view/copy)
        unsafe { crate::mlx_array_free(a_ptr) };

        let b_ptr = unsafe { crate::mlx_array_from_data(lora_b.as_ptr(), lora_b.len() as i32) };
        if b_ptr.is_null() {
            unsafe { crate::mlx_array_free(a_reshaped) };
            return Err(AosError::Mlx("Failed to create LoRA B array".to_string()));
        }

        let b_shape_i32 = [b_shape[0] as i32, b_shape[1] as i32];
        let b_reshaped = unsafe { crate::mlx_array_reshape(b_ptr, b_shape_i32.as_ptr(), 2) };
        if b_reshaped.is_null() {
            unsafe {
                crate::mlx_array_free(a_reshaped);
                crate::mlx_array_free(b_ptr);
            }
            return Err(AosError::Mlx("Failed to reshape LoRA B".to_string()));
        }
        unsafe { crate::mlx_array_free(b_ptr) };

        let result = unsafe {
            crate::mlx_lora_adapter_set_module(
                self.ptr,
                layer_idx as i32,
                c_module_name.as_ptr(),
                a_reshaped,
                b_reshaped,
            )
        };

        // The C++ side stores mx::array by value (reference-counted internally).
        // The set_module call copies/retains the arrays, so we must NOT free them
        // here — the C++ LoraAdapter now owns the references.

        if result != 0 {
            let error = crate::ffi_error::get_ffi_error_or("Failed to set LoRA module");
            return Err(AosError::Mlx(error));
        }

        Ok(())
    }

    /// Get the raw pointer for passing to FFI functions.
    pub fn as_ptr(&self) -> *mut crate::mlx_lora_adapter_t {
        self.ptr
    }

    /// Get the adapter ID.
    pub fn adapter_id(&self) -> i32 {
        self.adapter_id
    }

    /// Get the number of layers.
    pub fn num_layers(&self) -> usize {
        self.num_layers
    }
}

impl Drop for FFILoraAdapter {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe { crate::mlx_lora_adapter_free(self.ptr) };
            self.ptr = std::ptr::null_mut();
        }
    }
}

// SAFETY: The raw pointer is not thread-safe, but all access is serialized
// through MLXFFIModel's inference_lock (a parking_lot::Mutex) and the backend's
// &mut self requirement on run_step. This matches the pattern used by MLXFFIModel
// itself (see lib.rs:1474-1492).
unsafe impl Send for FFILoraAdapter {}
unsafe impl Sync for FFILoraAdapter {}
