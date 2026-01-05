//! Test utilities for Metal kernel tests
//!
//! This module re-exports shared kernel testing utilities from adapteros-testing
//! and provides Metal-specific testing infrastructure.

#![cfg(target_os = "macos")]
#![allow(clippy::manual_slice_size_calculation)]

use metal::{Buffer, Device, MTLResourceOptions};
use std::sync::Arc;

// Re-export all shared kernel testing utilities
pub use adapteros_testing::kernel_testing::*;

// =============================================================================
// Metal-Specific Test Fixture
// =============================================================================

/// Test fixture for Metal kernel testing
pub struct MetalTestContext {
    pub device: Arc<Device>,
    pub queue: metal::CommandQueue,
}

impl MetalTestContext {
    pub fn new() -> Self {
        let device = Device::system_default().expect("Metal device required for tests");
        let queue = device.new_command_queue();
        Self {
            device: Arc::new(device),
            queue,
        }
    }

    /// Create buffer with f32 data
    pub fn buffer_f32(&self, data: &[f32]) -> Buffer {
        self.device.new_buffer_with_data(
            data.as_ptr() as *const _,
            (data.len() * std::mem::size_of::<f32>()) as u64,
            MTLResourceOptions::StorageModeShared,
        )
    }

    /// Create buffer with u32 data
    pub fn buffer_u32(&self, data: &[u32]) -> Buffer {
        self.device.new_buffer_with_data(
            data.as_ptr() as *const _,
            (data.len() * std::mem::size_of::<u32>()) as u64,
            MTLResourceOptions::StorageModeShared,
        )
    }

    /// Create empty buffer for given number of f32 elements
    pub fn empty_f32(&self, count: usize) -> Buffer {
        self.device.new_buffer(
            (count * std::mem::size_of::<f32>()) as u64,
            MTLResourceOptions::StorageModeShared,
        )
    }

    /// Read f32 values from buffer
    pub fn read_f32(&self, buffer: &Buffer, count: usize) -> Vec<f32> {
        unsafe {
            let ptr = buffer.contents() as *const f32;
            std::slice::from_raw_parts(ptr, count).to_vec()
        }
    }

    /// Create a constant buffer with a single value
    pub fn constant_f32(&self, value: f32) -> Buffer {
        self.device.new_buffer_with_data(
            &value as *const f32 as *const _,
            std::mem::size_of::<f32>() as u64,
            MTLResourceOptions::StorageModeShared,
        )
    }

    /// Create a constant buffer with a single u32 value
    pub fn constant_u32(&self, value: u32) -> Buffer {
        self.device.new_buffer_with_data(
            &value as *const u32 as *const _,
            std::mem::size_of::<u32>() as u64,
            MTLResourceOptions::StorageModeShared,
        )
    }
}

impl Default for MetalTestContext {
    fn default() -> Self {
        Self::new()
    }
}
