//! FFI Bindings for ANE CoreML Custom Operations
//!
//! Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.
//!
//! This module provides safe Rust bindings to the Objective-C++ custom CoreML operations
//! defined in ane_coreml_ops.mm.

use adapteros_core::{AosError, Result};
use std::ffi::c_void;
use tracing::{debug, info};

// ============================================================================
// External C FFI Declarations
// ============================================================================

#[repr(C)]
struct OpaqueLoRADownProject {
    _private: [u8; 0],
}

#[repr(C)]
struct OpaqueLoRAUpProject {
    _private: [u8; 0],
}

#[repr(C)]
struct OpaqueGatedAdd {
    _private: [u8; 0],
}

extern "C" {
    // LoRADownProject operations
    fn lora_down_project_create(
        device_ptr: *mut c_void,
        hidden_size: usize,
        lora_rank: usize,
        use_float16: bool,
    ) -> *mut OpaqueLoRADownProject;

    fn lora_down_project_free(handle: *mut OpaqueLoRADownProject);

    // LoRAUpProject operations
    fn lora_up_project_create(
        device_ptr: *mut c_void,
        lora_rank: usize,
        hidden_size: usize,
        num_modules: usize,
    ) -> *mut OpaqueLoRAUpProject;

    fn lora_up_project_free(handle: *mut OpaqueLoRAUpProject);

    // GatedAdd operations
    fn gated_add_create(device_ptr: *mut c_void) -> *mut OpaqueGatedAdd;

    fn gated_add_free(handle: *mut OpaqueGatedAdd);
}

// ============================================================================
// Safe Rust Wrappers
// ============================================================================

/// Safe wrapper for LoRADownProject operation
pub struct LoRADownProjectFFI {
    handle: *mut OpaqueLoRADownProject,
    hidden_size: usize,
    lora_rank: usize,
}

impl LoRADownProjectFFI {
    /// Create new LoRADownProject operation
    ///
    /// # Arguments
    /// * `device_ptr` - Raw pointer to Metal device
    /// * `hidden_size` - Hidden dimension size
    /// * `lora_rank` - LoRA rank
    /// * `use_float16` - Use Float16 precision
    ///
    /// # Safety
    /// device_ptr must be a valid MTLDevice pointer
    pub unsafe fn new(
        device_ptr: *mut c_void,
        hidden_size: usize,
        lora_rank: usize,
        use_float16: bool,
    ) -> Result<Self> {
        let handle = lora_down_project_create(device_ptr, hidden_size, lora_rank, use_float16);

        if handle.is_null() {
            return Err(AosError::Kernel(
                "Failed to create LoRADownProject operation".to_string(),
            ));
        }

        info!(
            hidden_size = hidden_size,
            lora_rank = lora_rank,
            use_float16 = use_float16,
            "Created LoRADownProject FFI wrapper"
        );

        Ok(Self {
            handle,
            hidden_size,
            lora_rank,
        })
    }

    /// Get hidden size
    pub fn hidden_size(&self) -> usize {
        self.hidden_size
    }

    /// Get LoRA rank
    pub fn lora_rank(&self) -> usize {
        self.lora_rank
    }
}

impl Drop for LoRADownProjectFFI {
    fn drop(&mut self) {
        unsafe {
            lora_down_project_free(self.handle);
        }
        debug!("Dropped LoRADownProject FFI wrapper");
    }
}

unsafe impl Send for LoRADownProjectFFI {}
unsafe impl Sync for LoRADownProjectFFI {}

/// Safe wrapper for LoRAUpProject operation
pub struct LoRAUpProjectFFI {
    handle: *mut OpaqueLoRAUpProject,
    lora_rank: usize,
    hidden_size: usize,
    num_modules: usize,
}

impl LoRAUpProjectFFI {
    /// Create new LoRAUpProject operation
    ///
    /// # Arguments
    /// * `device_ptr` - Raw pointer to Metal device
    /// * `lora_rank` - LoRA rank
    /// * `hidden_size` - Hidden dimension size
    /// * `num_modules` - Number of adapter modules
    ///
    /// # Safety
    /// device_ptr must be a valid MTLDevice pointer
    pub unsafe fn new(
        device_ptr: *mut c_void,
        lora_rank: usize,
        hidden_size: usize,
        num_modules: usize,
    ) -> Result<Self> {
        let handle = lora_up_project_create(device_ptr, lora_rank, hidden_size, num_modules);

        if handle.is_null() {
            return Err(AosError::Kernel(
                "Failed to create LoRAUpProject operation".to_string(),
            ));
        }

        info!(
            lora_rank = lora_rank,
            hidden_size = hidden_size,
            num_modules = num_modules,
            "Created LoRAUpProject FFI wrapper"
        );

        Ok(Self {
            handle,
            lora_rank,
            hidden_size,
            num_modules,
        })
    }

    /// Get LoRA rank
    pub fn lora_rank(&self) -> usize {
        self.lora_rank
    }

    /// Get hidden size
    pub fn hidden_size(&self) -> usize {
        self.hidden_size
    }

    /// Get number of modules
    pub fn num_modules(&self) -> usize {
        self.num_modules
    }
}

impl Drop for LoRAUpProjectFFI {
    fn drop(&mut self) {
        unsafe {
            lora_up_project_free(self.handle);
        }
        debug!("Dropped LoRAUpProject FFI wrapper");
    }
}

unsafe impl Send for LoRAUpProjectFFI {}
unsafe impl Sync for LoRAUpProjectFFI {}

/// Safe wrapper for GatedAdd operation
pub struct GatedAddFFI {
    handle: *mut OpaqueGatedAdd,
}

impl GatedAddFFI {
    /// Create new GatedAdd operation
    ///
    /// # Arguments
    /// * `device_ptr` - Raw pointer to Metal device
    ///
    /// # Safety
    /// device_ptr must be a valid MTLDevice pointer
    pub unsafe fn new(device_ptr: *mut c_void) -> Result<Self> {
        let handle = gated_add_create(device_ptr);

        if handle.is_null() {
            return Err(AosError::Kernel(
                "Failed to create GatedAdd operation".to_string(),
            ));
        }

        info!("Created GatedAdd FFI wrapper");

        Ok(Self { handle })
    }
}

impl Drop for GatedAddFFI {
    fn drop(&mut self) {
        unsafe {
            gated_add_free(self.handle);
        }
        debug!("Dropped GatedAdd FFI wrapper");
    }
}

unsafe impl Send for GatedAddFFI {}
unsafe impl Sync for GatedAddFFI {}

// ============================================================================
// Integration with ANE Kernels Module
// ============================================================================

/// ANE CoreML operations manager
///
/// Manages lifecycle of custom CoreML operations for ANE-optimized LoRA
pub struct ANECoreMLOps {
    down_project: Option<LoRADownProjectFFI>,
    up_project: Option<LoRAUpProjectFFI>,
    gated_add: Option<GatedAddFFI>,
}

impl ANECoreMLOps {
    /// Create new ANE CoreML operations manager
    pub fn new() -> Self {
        Self {
            down_project: None,
            up_project: None,
            gated_add: None,
        }
    }

    /// Initialize operations with Metal device
    ///
    /// # Arguments
    /// * `device_ptr` - Raw pointer to Metal device
    /// * `hidden_size` - Hidden dimension size
    /// * `lora_rank` - LoRA rank
    /// * `num_modules` - Number of adapter modules
    ///
    /// # Safety
    /// device_ptr must be a valid MTLDevice pointer
    pub unsafe fn init(
        &mut self,
        device_ptr: *mut c_void,
        hidden_size: usize,
        lora_rank: usize,
        num_modules: usize,
    ) -> Result<()> {
        self.down_project =
            Some(LoRADownProjectFFI::new(device_ptr, hidden_size, lora_rank, true)?);

        self.up_project = Some(LoRAUpProjectFFI::new(
            device_ptr,
            lora_rank,
            hidden_size,
            num_modules,
        )?);

        self.gated_add = Some(GatedAddFFI::new(device_ptr)?);

        info!(
            hidden_size = hidden_size,
            lora_rank = lora_rank,
            num_modules = num_modules,
            "Initialized all ANE CoreML operations"
        );

        Ok(())
    }

    /// Get down-projection operation
    pub fn down_project(&self) -> Option<&LoRADownProjectFFI> {
        self.down_project.as_ref()
    }

    /// Get up-projection operation
    pub fn up_project(&self) -> Option<&LoRAUpProjectFFI> {
        self.up_project.as_ref()
    }

    /// Get gated-add operation
    pub fn gated_add(&self) -> Option<&GatedAddFFI> {
        self.gated_add.as_ref()
    }

    /// Check if all operations are initialized
    pub fn is_initialized(&self) -> bool {
        self.down_project.is_some() && self.up_project.is_some() && self.gated_add.is_some()
    }
}

impl Default for ANECoreMLOps {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ane_coreml_ops_creation() {
        let ops = ANECoreMLOps::new();
        assert!(!ops.is_initialized());
        assert!(ops.down_project().is_none());
        assert!(ops.up_project().is_none());
        assert!(ops.gated_add().is_none());
    }
}
