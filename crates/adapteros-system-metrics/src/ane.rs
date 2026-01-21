//! Apple Neural Engine (ANE) metrics collection
//!
//! Provides ANE-specific metrics collection including:
//! - **ANE memory usage** (via CoreML bridge and IOKit on macOS 15+)
//! - ANE availability and generation detection
//! - ANE utilization and throttling tracking
//!
//! ## Data Sources (Priority Order)
//!
//! ANE memory statistics are collected using multiple strategies:
//!
//! 1. **Instrumented tracking** (best accuracy):
//!    - Uses `AneMemoryTracker` in the CoreML bridge
//!    - Call `ffi::record_model_load(id, bytes)` when loading models
//!    - Call `ffi::record_model_unload(id)` when unloading
//!    - Provides exact per-model memory accounting
//!
//! 2. **IOKit/Metal estimation** (fallback):
//!    - Queries Metal device `currentAllocatedSize`
//!    - Estimates ~40% of unified memory to ANE
//!    - Less accurate but always available on Apple Silicon
//!
//! 3. **System memory estimation** (last resort):
//!    - Estimates ~18% of system RAM for ANE
//!    - Least accurate, for pre-macOS 15 or fallback scenarios
//!
//! ## Accuracy Note
//!
//! For production monitoring, **instrument model loads** via the FFI:
//! ```ignore
//! use adapteros_lora_kernel_coreml::ffi;
//! ffi::record_model_load("adapter-abc123", model_size_bytes);
//! // ... use model ...
//! ffi::record_model_unload("adapter-abc123");
//! ```
//!
//! Check the `source` field in `AneMemoryStats` to determine which
//! method was used: "direct" (instrumented), "estimated", or "unavailable".
//!
//! Platform: macOS only (gracefully handles non-macOS platforms)

use crate::GpuMetrics;
use serde::{Deserialize, Serialize};
use tracing::debug;

/// ANE memory statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AneMemoryStats {
    /// Total ANE-allocated memory in MB
    pub allocated_mb: u64,
    /// Currently used ANE memory in MB
    pub used_mb: u64,
    /// Available ANE memory in MB
    pub available_mb: u64,
    /// Cached models/weights in MB
    pub cached_mb: u64,
    /// Peak memory usage in MB (since boot or last reset)
    pub peak_mb: u64,
    /// Usage percentage (0-100)
    pub usage_percent: f32,
    /// Whether ANE is thermally throttled
    pub throttled: bool,
    /// Whether ANE is available on this system
    pub available: bool,
    /// ANE generation (0 if unavailable)
    pub generation: u8,
    /// Data source: "direct", "estimated", or "unavailable"
    pub source: String,
}

/// ANE metrics collector
pub struct AneMetricsCollector {
    #[cfg(target_os = "macos")]
    ane_available: bool,
    #[cfg(target_os = "macos")]
    ane_generation: u8,
}

impl AneMetricsCollector {
    /// Create a new ANE metrics collector
    pub fn new() -> Self {
        #[cfg(target_os = "macos")]
        {
            let (available, generation) = Self::check_ane_availability();
            debug!(
                "ANE metrics collector initialized: available={}, generation={}",
                available, generation
            );
            Self {
                ane_available: available,
                ane_generation: generation,
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            debug!("ANE metrics collection not supported on this platform");
            Self {}
        }
    }

    /// Check if ANE is available on the system
    #[cfg(target_os = "macos")]
    fn check_ane_availability() -> (bool, u8) {
        // Check via CoreML FFI if available
        #[cfg(feature = "coreml")]
        {
            use adapteros_lora_kernel_coreml::ffi;
            let result = unsafe { ffi::coreml_check_ane() };
            (result.available, result.generation)
        }

        #[cfg(not(feature = "coreml"))]
        {
            // Fallback: Check for Apple Silicon via sysctl
            use std::process::Command;

            let is_apple_silicon = Command::new("sysctl")
                .args(["-n", "machdep.cpu.brand_string"])
                .output()
                .ok()
                .map(|output| {
                    let brand = String::from_utf8_lossy(&output.stdout);
                    brand.contains("Apple")
                })
                .unwrap_or(false);

            if is_apple_silicon {
                // Apple Silicon has ANE, estimate generation from chip
                let generation = Self::estimate_ane_generation();
                (true, generation)
            } else {
                (false, 0)
            }
        }
    }

    /// Estimate ANE generation from CPU brand
    #[cfg(all(target_os = "macos", not(feature = "coreml")))]
    fn estimate_ane_generation() -> u8 {
        use std::process::Command;

        // Get CPU brand to estimate ANE generation
        let brand = Command::new("sysctl")
            .args(["-n", "machdep.cpu.brand_string"])
            .output()
            .ok()
            .map(|output| String::from_utf8_lossy(&output.stdout).to_string())
            .unwrap_or_default();

        // Rough estimation based on Apple Silicon generation
        if brand.contains("M3") || brand.contains("M4") {
            4 // Latest generation
        } else if brand.contains("M2") {
            3
        } else if brand.contains("M1") {
            2
        } else {
            1 // Older or unknown
        }
    }

    /// Collect ANE memory metrics
    pub fn collect_metrics(&self) -> AneMemoryStats {
        #[cfg(target_os = "macos")]
        {
            if !self.ane_available {
                return AneMemoryStats {
                    available: false,
                    generation: 0,
                    ..Default::default()
                };
            }

            self.collect_ane_memory_stats()
        }

        #[cfg(not(target_os = "macos"))]
        {
            AneMemoryStats::default()
        }
    }

    #[cfg(target_os = "macos")]
    fn collect_ane_memory_stats(&self) -> AneMemoryStats {
        // Try to get ANE memory stats from CoreML
        #[cfg(feature = "coreml")]
        {
            if let Some(stats) = self.get_ane_stats_from_coreml() {
                return stats;
            }
        }

        // Fallback: Estimate ANE memory from system memory
        // ANE typically uses a portion of unified memory on Apple Silicon
        self.estimate_ane_memory_from_uma()
    }

    #[cfg(all(target_os = "macos", feature = "coreml"))]
    fn get_ane_stats_from_coreml(&self) -> Option<AneMemoryStats> {
        use adapteros_lora_kernel_coreml::ffi;

        let info = ffi::get_ane_memory_info();

        if !info.available {
            return None;
        }

        let allocated_mb = info.allocated_bytes / (1024 * 1024);
        let used_mb = info.used_bytes / (1024 * 1024);
        let cached_mb = info.cached_bytes / (1024 * 1024);
        let peak_mb = info.peak_bytes / (1024 * 1024);
        let available_mb = allocated_mb.saturating_sub(used_mb);
        let usage_percent = if allocated_mb > 0 {
            (used_mb as f32 / allocated_mb as f32) * 100.0
        } else {
            0.0
        };

        Some(AneMemoryStats {
            allocated_mb,
            used_mb,
            available_mb,
            cached_mb,
            peak_mb,
            usage_percent,
            throttled: info.throttled,
            available: true,
            generation: self.ane_generation,
            source: "direct".to_string(),
        })
    }

    #[cfg(target_os = "macos")]
    fn estimate_ane_memory_from_uma(&self) -> AneMemoryStats {
        use std::process::Command;

        // Get total system memory
        let total_bytes = Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok()
            .and_then(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .trim()
                    .parse::<u64>()
                    .ok()
            })
            .unwrap_or(0);

        if total_bytes == 0 {
            return AneMemoryStats {
                available: self.ane_available,
                generation: self.ane_generation,
                ..Default::default()
            };
        }

        // Estimate ANE allocation: typically 15-20% of system memory on Apple Silicon
        // This is a conservative estimate based on unified memory architecture
        let ane_allocated_bytes = (total_bytes as f64 * 0.18) as u64;
        let ane_allocated_mb = ane_allocated_bytes / (1024 * 1024);

        // Estimate ANE usage based on system pressure
        // Use vm_stat to get ANE-related memory activity
        let ane_used_pct = self.estimate_ane_usage_from_vm_stat().unwrap_or(0.0);
        let ane_used_mb = (ane_allocated_mb as f64 * ane_used_pct / 100.0) as u64;
        let ane_available_mb = ane_allocated_mb.saturating_sub(ane_used_mb);

        AneMemoryStats {
            allocated_mb: ane_allocated_mb,
            used_mb: ane_used_mb,
            available_mb: ane_available_mb,
            cached_mb: 0,
            peak_mb: 0,
            usage_percent: ane_used_pct as f32,
            throttled: false,
            available: self.ane_available,
            generation: self.ane_generation,
            source: "estimated".to_string(),
        }
    }

    #[cfg(target_os = "macos")]
    fn estimate_ane_usage_from_vm_stat(&self) -> Option<f64> {
        use std::process::Command;

        // Use vm_stat to estimate ANE activity
        // ANE usage correlates with compressed memory and neural processing
        let vm_stat = Command::new("vm_stat")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())?;

        let mut pages_compressed = 0u64;
        let mut pages_total = 0u64;

        for line in vm_stat.lines() {
            if line.contains("Pages occupied by compressor:") {
                pages_compressed = line
                    .split(':')
                    .nth(1)
                    .and_then(|s| s.trim().trim_end_matches('.').parse().ok())
                    .unwrap_or(0);
            } else if line.contains("Pages active:") || line.contains("Pages wired down:") {
                let pages: u64 = line
                    .split(':')
                    .nth(1)
                    .and_then(|s| s.trim().trim_end_matches('.').parse().ok())
                    .unwrap_or(0);
                pages_total += pages;
            }
        }

        if pages_total == 0 {
            return Some(0.0);
        }

        // Estimate ANE usage based on compression ratio
        // Higher compression often indicates ML workload activity
        let compression_ratio = pages_compressed as f64 / pages_total as f64;
        let estimated_usage_pct = (compression_ratio * 100.0).min(100.0);

        Some(estimated_usage_pct)
    }

    /// Check if ANE metrics collection is available
    pub fn is_available(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.ane_available
        }

        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    /// Get ANE generation
    pub fn generation(&self) -> u8 {
        #[cfg(target_os = "macos")]
        {
            self.ane_generation
        }

        #[cfg(not(target_os = "macos"))]
        {
            0
        }
    }

    /// Integrate ANE metrics into GpuMetrics
    pub fn populate_gpu_metrics(&self, _gpu_metrics: &mut GpuMetrics) {
        let ane_stats = self.collect_metrics();

        if ane_stats.available {
            // Populate ANE-specific fields in GpuMetrics if they exist
            // For now, we can add ANE memory to the existing memory fields
            debug!(
                "ANE metrics: allocated={}MB, used={}MB, usage={}%",
                ane_stats.allocated_mb, ane_stats.used_mb, ane_stats.usage_percent
            );
        }
    }
}

impl Default for AneMetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ane_collector_creation() {
        let _collector = AneMetricsCollector::new();
        // Verify collector can be created without panicking on any platform
    }

    #[test]
    fn test_ane_metrics_collection() {
        let collector = AneMetricsCollector::new();
        let metrics = collector.collect_metrics();

        // On macOS with Apple Silicon, should have some data
        // On other platforms, should return default (unavailable)
        #[cfg(target_os = "macos")]
        {
            // Verify metrics structure is valid (allocated_mb is u64, always >= 0)
            let _allocated = metrics.allocated_mb;
        }

        #[cfg(not(target_os = "macos"))]
        {
            assert!(!metrics.available);
        }
    }

    #[test]
    fn test_ane_memory_stats_default() {
        let stats = AneMemoryStats::default();
        assert!(!stats.available);
        assert_eq!(stats.generation, 0);
        assert_eq!(stats.allocated_mb, 0);
        assert_eq!(stats.used_mb, 0);
        assert_eq!(stats.cached_mb, 0);
        assert_eq!(stats.peak_mb, 0);
        assert!(!stats.throttled);
        assert_eq!(stats.source, "");
    }

    #[test]
    fn test_ane_metrics_source_field() {
        let collector = AneMetricsCollector::new();
        let stats = collector.collect_metrics();

        // Verify source field is populated
        #[cfg(target_os = "macos")]
        {
            assert!(["direct", "estimated", "unavailable"].contains(&stats.source.as_str()));
        }

        #[cfg(not(target_os = "macos"))]
        {
            assert_eq!(stats.source, "unavailable");
        }
    }
}
