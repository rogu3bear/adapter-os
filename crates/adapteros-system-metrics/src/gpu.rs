//! GPU metrics collection integration
//!
//! Provides GPU metrics collection using Metal profiler and MLX integration.
//! Follows AdapterOS patterns for Metal kernel profiling and MLX device monitoring.

use crate::GpuMetrics;
use adapteros_core::Result;
use tracing::debug;

#[cfg(feature = "mlx")]
use adapteros_lora_mlx_ffi::{
    memory, mlx_get_backend_capabilities, mlx_runtime_init, mlx_runtime_is_initialized,
};

#[cfg(target_os = "macos")]
use metal::Device;

/// GPU metrics collector
pub struct GpuMetricsCollector {
    #[cfg(target_os = "macos")]
    metal_device: Option<Device>,
    #[cfg(target_os = "macos")]
    counters_available: bool,
    #[cfg(feature = "mlx")]
    mlx_available: bool,
    #[cfg(feature = "mlx")]
    mlx_device_name: Option<String>,
}

impl GpuMetricsCollector {
    /// Create a new GPU metrics collector
    pub fn new() -> Self {
        #[cfg(target_os = "macos")]
        {
            let metal_device = Device::system_default();
            let counters_available = metal_device
                .as_ref()
                .map(|device| {
                    device
                        .supports_counter_sampling(metal::MTLCounterSamplingPoint::AtStageBoundary)
                })
                .unwrap_or(false);

            debug!(
                "GPU metrics collector initialized: Metal device available={}, counters available={}",
                metal_device.is_some(),
                counters_available
            );

            Self {
                metal_device,
                counters_available,
                #[cfg(feature = "mlx")]
                mlx_available: false,
                #[cfg(feature = "mlx")]
                mlx_device_name: None,
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            warn!("GPU metrics collection not supported on this platform");
            Self {
                #[cfg(feature = "mlx")]
                mlx_available: false,
                #[cfg(feature = "mlx")]
                mlx_device_name: None,
            }
        }
        .with_mlx_probe()
    }

    /// Collect GPU metrics
    pub fn collect_metrics(&self) -> GpuMetrics {
        let metrics = {
            #[cfg(target_os = "macos")]
            {
                self.collect_metal_metrics()
            }

            #[cfg(not(target_os = "macos"))]
            {
                GpuMetrics::default()
            }
        };

        self.collect_mlx_metrics(metrics)
    }

    #[cfg(target_os = "macos")]
    fn collect_metal_metrics(&self) -> GpuMetrics {
        let mut metrics = GpuMetrics::default();

        if let Some(ref device) = self.metal_device {
            // Get device name for identification
            let device_name = device.name();
            debug!("Collecting metrics for Metal device: {}", device_name);

            // Get device memory information
            if let Ok(memory_info) = self.get_device_memory_info(device) {
                metrics.memory_total = Some(memory_info.total);
                metrics.memory_used = Some(memory_info.used);

                // Calculate utilization as percentage of memory used
                if memory_info.total > 0 {
                    let utilization_pct =
                        (memory_info.used as f64 / memory_info.total as f64) * 100.0;
                    metrics.utilization = Some(utilization_pct);
                }
            }

            // Get GPU utilization from IOKit (macOS specific)
            if let Some(utilization) = self.get_gpu_utilization_iokit() {
                metrics.utilization = Some(utilization);
            }

            // Get temperature if available
            if let Some(temp) = self.get_gpu_temperature() {
                metrics.temperature = Some(temp);
            }

            // Get power usage if available
            if let Some(power) = self.get_gpu_power_usage() {
                metrics.power_usage = Some(power);
            }
        }

        metrics
    }

    #[cfg(feature = "mlx")]
    fn collect_mlx_metrics(&self, mut metrics: GpuMetrics) -> GpuMetrics {
        if !self.mlx_available {
            return metrics;
        }

        // Memory usage via MLX FFI (works in stub and real modes)
        let used_bytes = memory::memory_usage() as u64;
        metrics.mlx_memory_used = Some(used_bytes);

        // If total memory is unknown from Metal path, try to populate from MLX capabilities
        if metrics.memory_total.is_none() {
            if let Ok(caps) = mlx_get_backend_capabilities() {
                if caps.max_buffer_size > 0 {
                    metrics.memory_total = Some(caps.max_buffer_size as u64);
                }
                if metrics.mlx_utilization.is_none() && caps.gpu_available {
                    // Utilization not yet exposed by MLX; remain None to signal absence
                    metrics.mlx_utilization = None;
                }
            }
        }

        // If general memory_used is still empty, reuse MLX reading as a best-effort proxy
        if metrics.memory_used.is_none() {
            metrics.memory_used = Some(used_bytes);
        }

        metrics
    }

    #[cfg(not(feature = "mlx"))]
    fn collect_mlx_metrics(&self, metrics: GpuMetrics) -> GpuMetrics {
        metrics
    }

    #[cfg(feature = "mlx")]
    fn with_mlx_probe(mut self) -> Self {
        // Initialize MLX runtime if available; ignore errors to avoid breaking non-MLX hosts
        let initialized = mlx_runtime_is_initialized() || mlx_runtime_init().is_ok();
        self.mlx_available = initialized;

        if initialized {
            if let Ok(caps) = mlx_get_backend_capabilities() {
                if !caps.device_name_str().is_empty() {
                    self.mlx_device_name = Some(caps.device_name_str().to_string());
                }
            }
        } else {
            warn!("MLX runtime not initialized; MLX GPU metrics will be skipped");
        }

        self
    }

    #[cfg(not(feature = "mlx"))]
    fn with_mlx_probe(self) -> Self {
        self
    }

    #[cfg(target_os = "macos")]
    fn get_device_memory_info(&self, device: &Device) -> Result<DeviceMemoryInfo> {
        // Query unified memory size on Apple Silicon
        // Metal on Apple Silicon uses unified memory, so we check system memory

        use std::process::Command;

        let total = if let Ok(output) = Command::new("sysctl").arg("-n").arg("hw.memsize").output()
        {
            let memsize_str = String::from_utf8_lossy(&output.stdout);
            memsize_str.trim().parse::<u64>().unwrap_or(0)
        } else {
            // Fallback: estimate from recommended max working set size
            device.recommended_max_working_set_size()
        };

        // Get current allocated size
        // Metal doesn't expose used memory directly, so we track allocations
        // For now, use current_allocated_size as a proxy
        let used = device.current_allocated_size();

        debug!("Metal device memory: used={} total={}", used, total);

        Ok(DeviceMemoryInfo { total, used })
    }

    #[cfg(target_os = "macos")]
    fn get_gpu_utilization_iokit(&self) -> Option<f64> {
        // Query GPU utilization via IOKit
        // This requires platform-specific IOKit integration
        // For now, we'll use powermetrics if available

        use std::process::Command;

        if let Ok(output) = Command::new("sh")
            .arg("-c")
            .arg("powermetrics --samplers gpu_power -i 1 -n 1 2>/dev/null | grep 'GPU active residency' | awk '{print $4}' | tr -d '%'")
            .output()
        {
            if output.status.success() {
                let utilization_str = String::from_utf8_lossy(&output.stdout);
                if let Ok(utilization) = utilization_str.trim().parse::<f64>() {
                    return Some(utilization);
                }
            }
        }

        None
    }

    #[cfg(target_os = "macos")]
    fn get_gpu_temperature(&self) -> Option<f64> {
        // Query GPU temperature via IOKit/powermetrics
        // This is platform-specific and may require additional permissions

        use std::process::Command;

        if let Ok(output) = Command::new("sh")
            .arg("-c")
            .arg("powermetrics --samplers smc -i 1 -n 1 2>/dev/null | grep 'GPU die temperature' | awk '{print $4}'")
            .output()
        {
            if output.status.success() {
                let temp_str = String::from_utf8_lossy(&output.stdout);
                if let Ok(temp) = temp_str.trim().parse::<f64>() {
                    return Some(temp);
                }
            }
        }

        None
    }

    #[cfg(target_os = "macos")]
    fn get_gpu_power_usage(&self) -> Option<f64> {
        // Query GPU power usage via powermetrics

        use std::process::Command;

        if let Ok(output) = Command::new("sh")
            .arg("-c")
            .arg("powermetrics --samplers gpu_power -i 1 -n 1 2>/dev/null | grep 'GPU Power' | awk '{print $3}'")
            .output()
        {
            if output.status.success() {
                let power_str = String::from_utf8_lossy(&output.stdout);
                if let Ok(power) = power_str.trim().parse::<f64>() {
                    return Some(power);
                }
            }
        }

        None
    }

    /// Check if GPU metrics collection is available
    pub fn is_available(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.metal_device.is_some()
        }

        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    /// Get GPU device information
    pub fn get_device_info(&self) -> Option<GpuDeviceInfo> {
        #[cfg(target_os = "macos")]
        {
            self.metal_device.as_ref().map(|device| GpuDeviceInfo {
                name: device.name().to_string(),
                vendor: "Apple".to_string(),
                device_type: "GPU".to_string(),
                supports_counters: self.counters_available,
            })
        }

        #[cfg(not(target_os = "macos"))]
        {
            None
        }
    }
}

impl Default for GpuMetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// GPU device memory information
#[derive(Debug, Clone)]
pub struct DeviceMemoryInfo {
    pub total: u64,
    pub used: u64,
}

/// GPU device information
#[derive(Debug, Clone)]
pub struct GpuDeviceInfo {
    pub name: String,
    pub vendor: String,
    pub device_type: String,
    pub supports_counters: bool,
}

/// MLX device integration (placeholder for future implementation)
#[cfg(feature = "mlx")]
pub struct MlxDevice {
    // Placeholder for MLX device integration
    // This would be implemented when MLX crate is available
}

#[cfg(feature = "mlx")]
impl MlxDevice {
    pub fn new() -> Result<Self> {
        // Placeholder implementation
        Ok(Self {})
    }

    pub fn get_memory_usage(&self) -> Option<u64> {
        // Placeholder implementation
        None
    }

    pub fn get_utilization(&self) -> Option<f32> {
        // Placeholder implementation
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_collector_creation() {
        let _collector = GpuMetricsCollector::new();
        // Verify collector can be created without panicking on any platform
    }

    #[test]
    fn test_gpu_metrics_collection() {
        let collector = GpuMetricsCollector::new();
        let metrics = collector.collect_metrics();

        // Test that metrics are collected (values may be None on unsupported platforms)
        assert!(
            metrics.utilization.is_none()
                || metrics
                    .utilization
                    .is_some_and(|u| (0.0..=100.0).contains(&u))
        );
    }

    #[test]
    fn test_device_info() {
        let collector = GpuMetricsCollector::new();
        let device_info = collector.get_device_info();

        // Device info may be None on unsupported platforms
        if let Some(info) = device_info {
            assert!(!info.name.is_empty());
            assert!(!info.vendor.is_empty());
        }
    }

    #[cfg(feature = "mlx")]
    #[test]
    fn test_mlx_metrics_collection_best_effort() {
        let collector = GpuMetricsCollector::new();
        let metrics = collector.collect_metrics();

        // When MLX runtime is available (real or stub), memory usage should be populated
        if metrics.mlx_memory_used.is_some() {
            assert!(metrics.mlx_memory_used.unwrap() > 0);
        }
    }
}
