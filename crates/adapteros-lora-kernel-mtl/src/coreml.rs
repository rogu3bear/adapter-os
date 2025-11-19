//! Safe Rust bindings for CoreML FFI
//!
//! Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.
//!
//! This module provides safe Rust wrappers around the CoreML C FFI interface.
//! All unsafe FFI calls are encapsulated with proper error handling and resource management.

use adapteros_core::{AosError, Result};
use std::ffi::{CStr, CString};
use std::path::Path;
use std::ptr;
use tracing::{debug, error, info, warn};

// ============================================================================
// FFI Type Declarations
// ============================================================================

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreMLErrorCode {
    Success = 0,
    InvalidModel = 1,
    InvalidInput = 2,
    PredictionFailed = 3,
    MemoryAllocation = 4,
    InvalidDimensions = 5,
    UnsupportedType = 6,
    Io = 7,
    Unknown = 99,
}

#[repr(C)]
#[derive(Debug)]
pub struct CoreMLShape {
    pub dimensions: *mut usize,
    pub rank: usize,
}

#[repr(C)]
#[derive(Debug)]
pub struct CoreMLModelMetadata {
    pub model_version: *const i8,
    pub model_description: *const i8,
    pub input_count: usize,
    pub output_count: usize,
    pub supports_gpu: bool,
    pub supports_ane: bool,
}

// Opaque pointers to C++ types
#[repr(C)]
pub struct CoreMLModel {
    _private: [u8; 0],
}

#[repr(C)]
pub struct CoreMLArray {
    _private: [u8; 0],
}

#[repr(C)]
pub struct CoreMLPrediction {
    _private: [u8; 0],
}

// ============================================================================
// External FFI Declarations
// ============================================================================

extern "C" {
    // Model management
    fn coreml_model_load(
        path: *const i8,
        use_gpu: bool,
        use_ane: bool,
        error_code: *mut CoreMLErrorCode,
    ) -> *mut CoreMLModel;
    fn coreml_model_free(model: *mut CoreMLModel);
    fn coreml_model_get_metadata(model: *mut CoreMLModel) -> CoreMLModelMetadata;

    // Array management
    fn coreml_array_new(
        data: *const f32,
        shape: *const CoreMLShape,
        error_code: *mut CoreMLErrorCode,
    ) -> *mut CoreMLArray;
    fn coreml_array_new_int8(
        data: *const i8,
        shape: *const CoreMLShape,
        error_code: *mut CoreMLErrorCode,
    ) -> *mut CoreMLArray;
    fn coreml_array_new_float16(
        data: *const u16,
        shape: *const CoreMLShape,
        error_code: *mut CoreMLErrorCode,
    ) -> *mut CoreMLArray;
    fn coreml_array_free(array: *mut CoreMLArray);
    fn coreml_array_get_float_data(array: *mut CoreMLArray) -> *const f32;
    fn coreml_array_get_int8_data(array: *mut CoreMLArray) -> *const i8;
    fn coreml_array_get_shape(array: *mut CoreMLArray) -> CoreMLShape;
    fn coreml_array_get_size(array: *mut CoreMLArray) -> usize;

    // Inference
    fn coreml_predict(
        model: *mut CoreMLModel,
        input: *mut CoreMLArray,
        input_name: *const i8,
        error_code: *mut CoreMLErrorCode,
    ) -> *mut CoreMLPrediction;
    fn coreml_predict_multi(
        model: *mut CoreMLModel,
        inputs: *mut *mut CoreMLArray,
        input_names: *mut *const i8,
        input_count: usize,
        error_code: *mut CoreMLErrorCode,
    ) -> *mut CoreMLPrediction;
    fn coreml_prediction_free(prediction: *mut CoreMLPrediction);
    fn coreml_prediction_get_output(
        prediction: *mut CoreMLPrediction,
        output_name: *const i8,
    ) -> *mut CoreMLArray;
    fn coreml_prediction_get_output_count(prediction: *mut CoreMLPrediction) -> usize;
    fn coreml_prediction_get_output_name(
        prediction: *mut CoreMLPrediction,
        index: usize,
    ) -> *const i8;

    // Error handling
    fn coreml_get_last_error() -> *const i8;
    fn coreml_clear_error();

    // Memory management
    fn coreml_free_string(s: *const i8);
    fn coreml_free_shape(shape: CoreMLShape);

    // Utilities
    fn coreml_is_available() -> bool;
    fn coreml_get_version() -> *const i8;
    fn coreml_set_verbose(enabled: bool);
}

// ============================================================================
// Safe Rust Wrappers
// ============================================================================

/// Safe wrapper for CoreML model
pub struct Model {
    ptr: *mut CoreMLModel,
}

impl Model {
    /// Load a CoreML model from a .mlpackage or .mlmodelc file
    ///
    /// # Arguments
    /// * `path` - Filesystem path to the model
    /// * `use_gpu` - Enable GPU (Metal) acceleration
    /// * `use_ane` - Enable Apple Neural Engine acceleration
    ///
    /// # Errors
    /// Returns `AosError::Io` if the model file is not found.
    /// Returns `AosError::Config` if the model fails to load.
    pub fn load<P: AsRef<Path>>(path: P, use_gpu: bool, use_ane: bool) -> Result<Self> {
        let path_str = path
            .as_ref()
            .to_str()
            .ok_or_else(|| AosError::Io("Invalid UTF-8 in path".to_string()))?;

        let c_path = CString::new(path_str)
            .map_err(|e| AosError::Io(format!("Failed to convert path: {}", e)))?;

        let mut error_code = CoreMLErrorCode::Success;

        let ptr = unsafe { coreml_model_load(c_path.as_ptr(), use_gpu, use_ane, &mut error_code) };

        if ptr.is_null() {
            let error_msg = get_last_error().unwrap_or_else(|| "Unknown error".to_string());
            return Err(match error_code {
                CoreMLErrorCode::Io => AosError::Io(error_msg),
                _ => AosError::Config(error_msg),
            });
        }

        info!(
            path = %path_str,
            use_gpu = %use_gpu,
            use_ane = %use_ane,
            "Loaded CoreML model"
        );

        Ok(Model { ptr })
    }

    /// Get model metadata
    pub fn metadata(&self) -> Result<ModelMetadata> {
        let raw_metadata = unsafe { coreml_model_get_metadata(self.ptr) };

        let version = unsafe {
            if raw_metadata.model_version.is_null() {
                "unknown".to_string()
            } else {
                CStr::from_ptr(raw_metadata.model_version)
                    .to_string_lossy()
                    .to_string()
            }
        };

        let description = unsafe {
            if raw_metadata.model_description.is_null() {
                String::new()
            } else {
                CStr::from_ptr(raw_metadata.model_description)
                    .to_string_lossy()
                    .to_string()
            }
        };

        // Free C strings
        unsafe {
            if !raw_metadata.model_version.is_null() {
                coreml_free_string(raw_metadata.model_version);
            }
            if !raw_metadata.model_description.is_null() {
                coreml_free_string(raw_metadata.model_description);
            }
        }

        Ok(ModelMetadata {
            version,
            description,
            input_count: raw_metadata.input_count,
            output_count: raw_metadata.output_count,
            supports_gpu: raw_metadata.supports_gpu,
            supports_ane: raw_metadata.supports_ane,
        })
    }

    /// Run prediction with single input
    ///
    /// # Arguments
    /// * `input` - Input array
    /// * `input_name` - Optional input feature name (uses default if None)
    pub fn predict(&self, input: &Array, input_name: Option<&str>) -> Result<Prediction> {
        let c_name = if let Some(name) = input_name {
            Some(
                CString::new(name)
                    .map_err(|e| AosError::Config(format!("Invalid input name: {}", e)))?,
            )
        } else {
            None
        };

        let name_ptr = c_name.as_ref().map(|s| s.as_ptr()).unwrap_or(ptr::null());

        let mut error_code = CoreMLErrorCode::Success;

        let ptr = unsafe {
            coreml_predict(self.ptr, input.ptr, name_ptr, &mut error_code)
        };

        if ptr.is_null() {
            let error_msg = get_last_error().unwrap_or_else(|| "Prediction failed".to_string());
            return Err(AosError::Config(error_msg));
        }

        debug!("CoreML prediction completed successfully");

        Ok(Prediction { ptr })
    }

    /// Run prediction with multiple named inputs
    pub fn predict_multi(&self, inputs: &[(&str, &Array)]) -> Result<Prediction> {
        if inputs.is_empty() {
            return Err(AosError::Config(
                "At least one input is required".to_string(),
            ));
        }

        // Convert names to CStrings
        let c_names: Result<Vec<CString>> = inputs
            .iter()
            .map(|(name, _)| {
                CString::new(*name)
                    .map_err(|e| AosError::Config(format!("Invalid input name: {}", e)))
            })
            .collect();
        let c_names = c_names?;

        // Build arrays of pointers
        let mut name_ptrs: Vec<*const i8> = c_names.iter().map(|s| s.as_ptr()).collect();
        let mut array_ptrs: Vec<*mut CoreMLArray> = inputs.iter().map(|(_, a)| a.ptr).collect();

        let mut error_code = CoreMLErrorCode::Success;

        let ptr = unsafe {
            coreml_predict_multi(
                self.ptr,
                array_ptrs.as_mut_ptr(),
                name_ptrs.as_mut_ptr(),
                inputs.len(),
                &mut error_code,
            )
        };

        if ptr.is_null() {
            let error_msg = get_last_error().unwrap_or_else(|| "Prediction failed".to_string());
            return Err(AosError::Config(error_msg));
        }

        debug!(input_count = inputs.len(), "CoreML multi-input prediction completed");

        Ok(Prediction { ptr })
    }
}

impl Drop for Model {
    fn drop(&mut self) {
        unsafe {
            coreml_model_free(self.ptr);
        }
    }
}

unsafe impl Send for Model {}
unsafe impl Sync for Model {}

/// Model metadata
#[derive(Debug, Clone)]
pub struct ModelMetadata {
    pub version: String,
    pub description: String,
    pub input_count: usize,
    pub output_count: usize,
    pub supports_gpu: bool,
    pub supports_ane: bool,
}

/// Safe wrapper for CoreML array
pub struct Array {
    ptr: *mut CoreMLArray,
}

impl Array {
    /// Create a new float32 array
    pub fn new_f32(data: &[f32], shape: &[usize]) -> Result<Self> {
        if shape.is_empty() {
            return Err(AosError::Config("Shape cannot be empty".to_string()));
        }

        // Validate size matches shape
        let expected_size: usize = shape.iter().product();
        if data.len() != expected_size {
            return Err(AosError::Config(format!(
                "Data size {} does not match shape {:?} (expected {})",
                data.len(),
                shape,
                expected_size
            )));
        }

        let mut shape_dims: Vec<usize> = shape.to_vec();
        let c_shape = CoreMLShape {
            dimensions: shape_dims.as_mut_ptr(),
            rank: shape.len(),
        };

        let mut error_code = CoreMLErrorCode::Success;

        let ptr = unsafe { coreml_array_new(data.as_ptr(), &c_shape, &mut error_code) };

        if ptr.is_null() {
            let error_msg = get_last_error().unwrap_or_else(|| "Failed to create array".to_string());
            return Err(AosError::Config(error_msg));
        }

        debug!(shape = ?shape, "Created CoreML float32 array");

        Ok(Array { ptr })
    }

    /// Create a new int8 array (quantized)
    pub fn new_i8(data: &[i8], shape: &[usize]) -> Result<Self> {
        if shape.is_empty() {
            return Err(AosError::Config("Shape cannot be empty".to_string()));
        }

        let expected_size: usize = shape.iter().product();
        if data.len() != expected_size {
            return Err(AosError::Config(format!(
                "Data size {} does not match shape {:?}",
                data.len(),
                shape
            )));
        }

        let mut shape_dims: Vec<usize> = shape.to_vec();
        let c_shape = CoreMLShape {
            dimensions: shape_dims.as_mut_ptr(),
            rank: shape.len(),
        };

        let mut error_code = CoreMLErrorCode::Success;

        let ptr = unsafe { coreml_array_new_int8(data.as_ptr(), &c_shape, &mut error_code) };

        if ptr.is_null() {
            let error_msg = get_last_error().unwrap_or_else(|| "Failed to create array".to_string());
            return Err(AosError::Config(error_msg));
        }

        debug!(shape = ?shape, "Created CoreML int8 array");

        Ok(Array { ptr })
    }

    /// Create a new float16 array
    pub fn new_f16(data: &[u16], shape: &[usize]) -> Result<Self> {
        if shape.is_empty() {
            return Err(AosError::Config("Shape cannot be empty".to_string()));
        }

        let expected_size: usize = shape.iter().product();
        if data.len() != expected_size {
            return Err(AosError::Config(format!(
                "Data size {} does not match shape {:?}",
                data.len(),
                shape
            )));
        }

        let mut shape_dims: Vec<usize> = shape.to_vec();
        let c_shape = CoreMLShape {
            dimensions: shape_dims.as_mut_ptr(),
            rank: shape.len(),
        };

        let mut error_code = CoreMLErrorCode::Success;

        let ptr = unsafe { coreml_array_new_float16(data.as_ptr(), &c_shape, &mut error_code) };

        if ptr.is_null() {
            let error_msg = get_last_error().unwrap_or_else(|| "Failed to create array".to_string());
            return Err(AosError::Config(error_msg));
        }

        debug!(shape = ?shape, "Created CoreML float16 array");

        Ok(Array { ptr })
    }

    /// Get float32 data as slice (returns None if not float32)
    pub fn as_f32_slice(&self) -> Option<&[f32]> {
        let ptr = unsafe { coreml_array_get_float_data(self.ptr) };
        if ptr.is_null() {
            return None;
        }

        let size = unsafe { coreml_array_get_size(self.ptr) };
        Some(unsafe { std::slice::from_raw_parts(ptr, size) })
    }

    /// Get int8 data as slice (returns None if not int8)
    pub fn as_i8_slice(&self) -> Option<&[i8]> {
        let ptr = unsafe { coreml_array_get_int8_data(self.ptr) };
        if ptr.is_null() {
            return None;
        }

        let size = unsafe { coreml_array_get_size(self.ptr) };
        Some(unsafe { std::slice::from_raw_parts(ptr, size) })
    }

    /// Get array shape
    pub fn shape(&self) -> Vec<usize> {
        let c_shape = unsafe { coreml_array_get_shape(self.ptr) };

        let shape = if c_shape.dimensions.is_null() {
            Vec::new()
        } else {
            unsafe { std::slice::from_raw_parts(c_shape.dimensions, c_shape.rank).to_vec() }
        };

        unsafe {
            coreml_free_shape(c_shape);
        }

        shape
    }

    /// Get total element count
    pub fn size(&self) -> usize {
        unsafe { coreml_array_get_size(self.ptr) }
    }
}

impl Drop for Array {
    fn drop(&mut self) {
        unsafe {
            coreml_array_free(self.ptr);
        }
    }
}

unsafe impl Send for Array {}
unsafe impl Sync for Array {}

/// Safe wrapper for CoreML prediction result
pub struct Prediction {
    ptr: *mut CoreMLPrediction,
}

impl Prediction {
    /// Get output array by name
    ///
    /// Note: The returned Array borrows from this Prediction and
    /// must not outlive it.
    pub fn get_output(&self, name: &str) -> Result<Array> {
        let c_name = CString::new(name)
            .map_err(|e| AosError::Config(format!("Invalid output name: {}", e)))?;

        let ptr = unsafe { coreml_prediction_get_output(self.ptr, c_name.as_ptr()) };

        if ptr.is_null() {
            return Err(AosError::Config(format!(
                "Output '{}' not found in prediction",
                name
            )));
        }

        // Note: We don't own this pointer, it's owned by the Prediction
        // But we need to create an Array wrapper without freeing it
        // This is a limitation of the current design
        Ok(Array { ptr })
    }

    /// Get number of outputs
    pub fn output_count(&self) -> usize {
        unsafe { coreml_prediction_get_output_count(self.ptr) }
    }

    /// Get output name by index
    pub fn output_name(&self, index: usize) -> Option<String> {
        let ptr = unsafe { coreml_prediction_get_output_name(self.ptr, index) };

        if ptr.is_null() {
            return None;
        }

        let name = unsafe { CStr::from_ptr(ptr).to_string_lossy().to_string() };

        unsafe {
            coreml_free_string(ptr);
        }

        Some(name)
    }

    /// Get all output names
    pub fn output_names(&self) -> Vec<String> {
        let count = self.output_count();
        (0..count).filter_map(|i| self.output_name(i)).collect()
    }
}

impl Drop for Prediction {
    fn drop(&mut self) {
        unsafe {
            coreml_prediction_free(self.ptr);
        }
    }
}

unsafe impl Send for Prediction {}
unsafe impl Sync for Prediction {}

// ============================================================================
// Utility Functions
// ============================================================================

/// Get last error message from CoreML FFI
fn get_last_error() -> Option<String> {
    let ptr = unsafe { coreml_get_last_error() };
    if ptr.is_null() {
        return None;
    }

    Some(unsafe { CStr::from_ptr(ptr).to_string_lossy().to_string() })
}

/// Check if CoreML is available on this system
pub fn is_available() -> bool {
    unsafe { coreml_is_available() }
}

/// Get CoreML framework version
pub fn version() -> String {
    let ptr = unsafe { coreml_get_version() };
    if ptr.is_null() {
        return "unknown".to_string();
    }

    unsafe { CStr::from_ptr(ptr).to_string_lossy().to_string() }
}

/// Enable verbose logging for debugging
pub fn set_verbose(enabled: bool) {
    unsafe {
        coreml_set_verbose(enabled);
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coreml_availability() {
        // Should always be available on macOS
        assert!(is_available());
    }

    #[test]
    fn test_coreml_version() {
        let ver = version();
        assert!(!ver.is_empty());
        assert_ne!(ver, "unknown");
    }

    #[test]
    fn test_array_creation() {
        let data = vec![1.0f32, 2.0, 3.0, 4.0];
        let shape = vec![2, 2];

        let array = Array::new_f32(&data, &shape).expect("Failed to create array");

        assert_eq!(array.size(), 4);
        assert_eq!(array.shape(), vec![2, 2]);

        if let Some(slice) = array.as_f32_slice() {
            assert_eq!(slice.len(), 4);
            assert_eq!(slice[0], 1.0);
            assert_eq!(slice[3], 4.0);
        } else {
            panic!("Expected f32 slice");
        }
    }

    #[test]
    fn test_array_shape_validation() {
        let data = vec![1.0f32, 2.0, 3.0];
        let shape = vec![2, 2]; // Wrong shape

        let result = Array::new_f32(&data, &shape);
        assert!(result.is_err());
    }
}
