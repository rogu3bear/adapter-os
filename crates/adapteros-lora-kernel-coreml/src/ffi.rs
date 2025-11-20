//! FFI declarations for CoreML backend
//!
//! Copyright: © 2025 JKCA / James KC Auchterlonie. All rights reserved.

use std::ffi::{c_char, c_void};

/// CoreML prediction result
#[repr(C)]
pub struct CoreMLPredictionResult {
    pub success: i32,
    pub used_ane: i32,
}

extern "C" {
    /// Load CoreML model from .mlpackage path
    ///
    /// # Safety
    /// - model_path must be valid UTF-8 C string
    /// - error_buffer must be at least error_size bytes
    /// - ane_available will be set to 1 if ANE detected, 0 otherwise
    pub fn coreml_load_model(
        model_path: *const c_char,
        error_buffer: *mut c_char,
        error_size: usize,
        ane_available: *mut i32,
    ) -> *mut c_void;

    /// Release CoreML model
    pub fn coreml_release_model(model_ptr: *mut c_void);

    /// Run CoreML prediction
    ///
    /// # Safety
    /// - input_ids must be at least input_len elements
    /// - output_logits must be at least output_size elements
    /// - adapter_indices must be at least k elements
    /// - adapter_gates must be at least k elements
    pub fn coreml_predict(
        model_ptr: *mut c_void,
        input_ids: *const u32,
        input_len: usize,
        output_logits: *mut f32,
        output_size: usize,
        adapter_indices: *const u16,
        adapter_gates: *const i16,
        k: usize,
    ) -> CoreMLPredictionResult;

    /// Detect ANE availability without loading a model
    pub fn coreml_detect_ane() -> i32;

    /// Get ANE core count
    pub fn coreml_ane_core_count() -> i32;

    /// Get ANE TOPS (trillions of operations per second)
    pub fn coreml_ane_tops() -> f32;

    // Power management FFI

    /// Get battery level percentage (0-100)
    pub fn get_battery_level() -> f32;

    /// Check if device is plugged into power (1=yes, 0=no)
    pub fn get_is_plugged_in() -> i32;

    /// Get system low power mode state (1=enabled, 0=disabled)
    pub fn get_system_low_power_mode() -> i32;

    /// Get thermal state (0=nominal, 1=fair, 2=serious, 3=critical)
    pub fn get_thermal_state() -> i32;
}
