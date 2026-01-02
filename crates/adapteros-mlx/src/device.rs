//! MLX device management

/// Compute device for MLX operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Device {
    /// CPU device
    Cpu,
    /// GPU device (Metal on Apple Silicon)
    #[default]
    Gpu,
}

impl Device {
    /// Check if this is a GPU device
    pub fn is_gpu(&self) -> bool {
        matches!(self, Device::Gpu)
    }

    /// Get the default device (GPU on Apple Silicon)
    pub fn default_device() -> Self {
        Device::Gpu
    }
}
