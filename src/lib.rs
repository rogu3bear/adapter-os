//! AdapterOS Integration Tests
//!
//! This file enables workspace-level integration tests to be discovered by cargo.
//! The actual test implementations are in the tests/ directory.

// Re-export key modules for integration tests
pub use adapteros_lora_kernel_api::FusedKernels;

// Metal kernels are only available on macOS
#[cfg(target_os = "macos")]
pub use adapteros_lora_kernel_mtl::{GqaConfig, LoraConfig, MetalKernels};

#[cfg(test)]
mod tests {
    // Integration tests are in tests/ directory
    // This module exists only to enable cargo test discovery
}
