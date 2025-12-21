//! Test utilities for CoreML backend tests
//!
//! This module re-exports shared kernel testing utilities from adapteros-testing.
//! All test utilities have been consolidated to reduce duplication.

#![cfg(target_os = "macos")]

// Re-export all shared kernel testing utilities
pub use adapteros_testing::kernel_testing::*;

// CoreML-specific test macros
/// Skip test if CoreML is not available
#[macro_export]
macro_rules! require_coreml {
    () => {
        if !adapteros_lora_kernel_coreml::is_coreml_available() {
            eprintln!("Skipping test - CoreML not available");
            return;
        }
    };
}

/// Skip test if MLTensor is not available
#[macro_export]
macro_rules! require_mltensor {
    () => {
        if !adapteros_lora_kernel_coreml::MLTensor::is_available() {
            eprintln!("Skipping test - MLTensor not available (requires macOS 15+)");
            return;
        }
    };
}

/// Skip test if ANE is not available
#[macro_export]
macro_rules! require_ane {
    () => {
        if !adapteros_lora_kernel_coreml::has_neural_engine() {
            eprintln!("Skipping test - ANE not available");
            return;
        }
    };
}
