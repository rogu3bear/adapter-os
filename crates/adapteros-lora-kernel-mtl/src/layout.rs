//! Runtime buffer layout validation
//!
//! This module validates that Metal buffers match the expected layouts
//! defined by the shader code. It prevents silent corruption by catching
//! stride mismatches and alignment violations before dispatch.
//!
//! Critical: No raw tensor data is included in error messages to prevent
//! sensitive data leaks in logs.

use adapteros_core::{AosError, Result};
use metal::Buffer;

/// Validates buffer layouts before kernel dispatch
pub struct LayoutValidator {
    /// Expected byte alignment for all buffers (Metal requires 16-byte alignment)
    expected_alignment: usize,
}

impl LayoutValidator {
    /// Create a new layout validator with default 16-byte alignment
    pub fn new() -> Self {
        Self {
            expected_alignment: 16,
        }
    }

    /// Create a validator with custom alignment requirement
    pub fn with_alignment(alignment: usize) -> Self {
        Self {
            expected_alignment: alignment,
        }
    }

    /// Validate a buffer's size and alignment
    ///
    /// # Arguments
    /// * `name` - Human-readable tensor name for error reporting
    /// * `buffer` - Metal buffer to validate
    /// * `expected_stride` - Size of each element in bytes
    /// * `expected_count` - Number of elements expected
    ///
    /// # Errors
    /// Returns `KernelLayoutMismatch` if:
    /// - Buffer size doesn't match stride * count
    /// - Buffer address is not properly aligned
    pub fn validate_buffer(
        &self,
        name: &str,
        buffer: &Buffer,
        expected_stride: usize,
        expected_count: usize,
    ) -> Result<()> {
        let actual_size = buffer.length() as usize;
        let expected_size = expected_stride * expected_count;

        // Check size match
        if actual_size != expected_size {
            return Err(AosError::KernelLayoutMismatch {
                tensor: name.to_string(),
                expected: format!(
                    "stride={}, count={}, size={}",
                    expected_stride, expected_count, expected_size
                ),
                got: format!("size={}", actual_size),
            });
        }

        // Check alignment
        let buffer_addr = buffer.contents() as usize;
        if !buffer_addr.is_multiple_of(self.expected_alignment) {
            return Err(AosError::KernelLayoutMismatch {
                tensor: name.to_string(),
                expected: format!("{}-byte aligned", self.expected_alignment),
                got: format!("misaligned pointer: {:p}", buffer.contents()),
            });
        }

        Ok(())
    }

    /// Validate multiple buffers with the same stride
    ///
    /// Convenience method for validating batches of uniform buffers
    pub fn validate_buffers_uniform(
        &self,
        buffers: &[(&str, &Buffer)],
        stride: usize,
        count: usize,
    ) -> Result<()> {
        for (name, buffer) in buffers {
            self.validate_buffer(name, buffer, stride, count)?;
        }
        Ok(())
    }

    /// Validate that a buffer can hold at least `min_bytes`
    ///
    /// Useful for variable-length buffers where exact size checking is not appropriate
    pub fn validate_min_size(&self, name: &str, buffer: &Buffer, min_bytes: usize) -> Result<()> {
        let actual_size = buffer.length() as usize;

        if actual_size < min_bytes {
            return Err(AosError::KernelLayoutMismatch {
                tensor: name.to_string(),
                expected: format!("size >= {}", min_bytes),
                got: format!("size={}", actual_size),
            });
        }

        // Still check alignment
        let buffer_addr = buffer.contents() as usize;
        if !buffer_addr.is_multiple_of(self.expected_alignment) {
            return Err(AosError::KernelLayoutMismatch {
                tensor: name.to_string(),
                expected: format!("{}-byte aligned", self.expected_alignment),
                got: format!("misaligned pointer: {:p}", buffer.contents()),
            });
        }

        Ok(())
    }
}

impl Default for LayoutValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use metal::Device;

    #[test]
    fn test_validator_creation() {
        let validator = LayoutValidator::new();
        assert_eq!(validator.expected_alignment, 16);

        let custom = LayoutValidator::with_alignment(32);
        assert_eq!(custom.expected_alignment, 32);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_valid_buffer() {
        let device = Device::system_default().expect("Metal device should be available for test");
        let validator = LayoutValidator::new();

        // Create a properly sized and aligned buffer
        let buffer = device.new_buffer(1024, metal::MTLResourceOptions::StorageModeShared);

        // Validate with matching parameters
        assert!(validator
            .validate_buffer("test_tensor", &buffer, 4, 256)
            .is_ok());
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_size_mismatch() {
        let device = Device::system_default().expect("Metal device should be available for test");
        let validator = LayoutValidator::new();

        let buffer = device.new_buffer(1024, metal::MTLResourceOptions::StorageModeShared);

        // Try to validate with wrong count (should expect 2048 bytes but buffer is 1024)
        let result = validator.validate_buffer("test_tensor", &buffer, 4, 512);
        assert!(result.is_err());

        if let Err(AosError::KernelLayoutMismatch { tensor, .. }) = result {
            assert_eq!(tensor, "test_tensor");
        } else {
            panic!("Expected KernelLayoutMismatch error");
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_min_size_validation() {
        let device = Device::system_default().expect("Metal device should be available for test");
        let validator = LayoutValidator::new();

        let buffer = device.new_buffer(2048, metal::MTLResourceOptions::StorageModeShared);

        // Should pass: buffer is 2048, min is 1024
        assert!(validator
            .validate_min_size("var_tensor", &buffer, 1024)
            .is_ok());

        // Should fail: buffer is 2048, min is 4096
        assert!(validator
            .validate_min_size("var_tensor", &buffer, 4096)
            .is_err());
    }
}
