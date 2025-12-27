use adapteros_core::AosError;
use thiserror::Error;

/// Kernel-specific errors for Metal execution safety
#[derive(Debug, Error)]
pub enum KernelError {
    #[error("Buffer '{buffer}' too small: required {required} bytes, available {available} bytes")]
    BufferTooSmall {
        buffer: &'static str,
        required: usize,
        available: usize,
    },
}

impl KernelError {
    pub fn into_aos(self) -> AosError {
        AosError::Kernel(self.to_string())
    }
}
