use adapteros_core::{AosError, Result};
use metal::Buffer;

const DEFAULT_ALIGNMENT: usize = 16;

#[derive(Debug, Clone, Copy)]
pub struct LayoutValidator {
    alignment: usize,
}

impl LayoutValidator {
    pub fn new() -> Self {
        Self {
            alignment: DEFAULT_ALIGNMENT,
        }
    }

    pub fn with_alignment(alignment: usize) -> Self {
        Self {
            alignment: alignment.max(1),
        }
    }

    pub fn validate_buffer(
        &self,
        tensor: &str,
        buffer: &Buffer,
        element_size: usize,
        element_count: usize,
    ) -> Result<()> {
        let expected_bytes = element_size.checked_mul(element_count).ok_or_else(|| {
            AosError::Kernel(format!("Layout size overflow for tensor '{}'", tensor))
        })?;
        let buffer_bytes = buffer.length() as usize;

        if expected_bytes != buffer_bytes {
            return Err(AosError::KernelLayoutMismatch {
                tensor: tensor.to_string(),
                expected: format!("{} bytes", expected_bytes),
                got: format!("{} bytes", buffer_bytes),
            });
        }

        self.validate_alignment(tensor, buffer)?;
        Ok(())
    }

    pub fn validate_buffers_uniform(
        &self,
        buffers: &[(&str, &Buffer)],
        element_size: usize,
        element_count: usize,
    ) -> Result<()> {
        for (name, buffer) in buffers {
            self.validate_buffer(name, buffer, element_size, element_count)?;
        }
        Ok(())
    }

    pub fn validate_min_size(&self, tensor: &str, buffer: &Buffer, min_bytes: usize) -> Result<()> {
        let buffer_bytes = buffer.length() as usize;
        if buffer_bytes < min_bytes {
            return Err(AosError::KernelLayoutMismatch {
                tensor: tensor.to_string(),
                expected: format!(">= {} bytes", min_bytes),
                got: format!("{} bytes", buffer_bytes),
            });
        }

        self.validate_alignment(tensor, buffer)?;
        Ok(())
    }

    fn validate_alignment(&self, tensor: &str, buffer: &Buffer) -> Result<()> {
        if self.alignment <= 1 {
            return Ok(());
        }

        let address = buffer.contents() as usize;
        if !address.is_multiple_of(self.alignment) {
            return Err(AosError::KernelLayoutMismatch {
                tensor: tensor.to_string(),
                expected: format!("alignment {} bytes", self.alignment),
                got: format!("offset {} bytes", address % self.alignment),
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
