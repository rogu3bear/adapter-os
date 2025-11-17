//! GPU Memory Collection (Best-Effort)
//!
//! Provides best-effort GPU memory statistics collection via Metal (macOS)
//! or platform-specific APIs. Gracefully degrades if GPU metrics unavailable.
//!
//! # Citations
//! - PRD 5: "gpu_total_bytes: u64 // best effort"
//! - Failure Semantics: "mark gpu_total_bytes = 0 and log a warning"

use tracing::warn;

/// GPU memory statistics (best-effort)
#[derive(Debug, Clone)]
pub struct GpuMemoryStats {
    /// GPU memory used in bytes (0 if unavailable)
    pub used_bytes: u64,
    /// GPU memory total in bytes (0 if unavailable)
    pub total_bytes: u64,
    /// Whether GPU metrics are available
    pub metrics_available: bool,
}

impl GpuMemoryStats {
    /// Create unavailable stats (fallback)
    pub fn unavailable() -> Self {
        Self {
            used_bytes: 0,
            total_bytes: 0,
            metrics_available: false,
        }
    }

    /// Get GPU memory usage percentage
    pub fn usage_pct(&self) -> f32 {
        if !self.metrics_available || self.total_bytes == 0 {
            0.0
        } else {
            (self.used_bytes as f32 / self.total_bytes as f32) * 100.0
        }
    }
}

/// Collect GPU memory statistics (best-effort)
///
/// Returns GPU memory stats if available, otherwise returns unavailable stats.
/// Logs warning on first failure, then silently returns unavailable.
pub fn collect_gpu_memory() -> GpuMemoryStats {
    #[cfg(target_os = "macos")]
    {
        collect_metal_gpu_memory()
    }

    #[cfg(not(target_os = "macos"))]
    {
        // GPU metrics not available on non-macOS platforms
        GpuMemoryStats::unavailable()
    }
}

/// Collect GPU memory via Metal (macOS only)
#[cfg(target_os = "macos")]
fn collect_metal_gpu_memory() -> GpuMemoryStats {
    use std::sync::Once;

    static WARN_ONCE: Once = Once::new();

    // Try to get Metal device
    match metal::Device::system_default() {
        Some(device) => {
            // Metal provides recommended_max_working_set_size as a proxy for total VRAM
            // This is the maximum amount of memory the GPU can efficiently use
            let total_bytes = device.recommended_max_working_set_size();

            // Note: Metal doesn't provide direct "used" memory stats
            // We'd need to track this via VramTracker in practice
            // For now, return total with 0 used as a conservative estimate
            GpuMemoryStats {
                used_bytes: 0, // TODO: Integrate with VramTracker
                total_bytes,
                metrics_available: true,
            }
        }
        None => {
            WARN_ONCE.call_once(|| {
                warn!("Metal GPU not available - GPU memory metrics disabled");
            });
            GpuMemoryStats::unavailable()
        }
    }
}

/// Collect GPU memory from VramTracker (integration helper)
///
/// Combines Metal device info with actual usage from VramTracker
#[cfg(target_os = "macos")]
pub fn collect_gpu_memory_with_vram_tracker(
    total_vram_bytes: u64,
) -> GpuMemoryStats {
    use std::sync::Once;

    static WARN_ONCE: Once = Once::new();

    match metal::Device::system_default() {
        Some(device) => {
            let total_bytes = device.recommended_max_working_set_size();

            GpuMemoryStats {
                used_bytes: total_vram_bytes,
                total_bytes,
                metrics_available: true,
            }
        }
        None => {
            WARN_ONCE.call_once(|| {
                warn!("Metal GPU not available - GPU memory metrics disabled");
            });
            GpuMemoryStats::unavailable()
        }
    }
}

/// Collect GPU memory from VramTracker (non-macOS platforms)
#[cfg(not(target_os = "macos"))]
pub fn collect_gpu_memory_with_vram_tracker(
    _total_vram_bytes: u64,
) -> GpuMemoryStats {
    GpuMemoryStats::unavailable()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unavailable_stats() {
        let stats = GpuMemoryStats::unavailable();
        assert_eq!(stats.used_bytes, 0);
        assert_eq!(stats.total_bytes, 0);
        assert!(!stats.metrics_available);
        assert_eq!(stats.usage_pct(), 0.0);
    }

    #[test]
    fn test_gpu_memory_collection() {
        let stats = collect_gpu_memory();

        // Should either have metrics or be marked unavailable
        if stats.metrics_available {
            assert!(stats.total_bytes > 0);
        } else {
            assert_eq!(stats.total_bytes, 0);
            assert_eq!(stats.used_bytes, 0);
        }
    }

    #[test]
    fn test_usage_pct_calculation() {
        let stats = GpuMemoryStats {
            used_bytes: 4 * 1024 * 1024 * 1024, // 4 GB
            total_bytes: 8 * 1024 * 1024 * 1024, // 8 GB
            metrics_available: true,
        };

        assert_eq!(stats.usage_pct(), 50.0);
    }
}
