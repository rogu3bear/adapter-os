//! Event types

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Generic event wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event<T> {
    pub ev_type: String,
    pub ts_mono_ns: u128,
    pub node_id: String,
    pub tenant_id: String,
    pub plan_id: String,
    pub cpid: String,
    pub payload: T,
}

/// Kernel profile event (from mplora-kernel-prof)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelProfileEvent {
    pub ts: String,
    pub device: String,
    pub kernel: String,
    pub available: bool,
    pub counters: Value,
}

/// Adapter VRAM attribution event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterVramEvent {
    pub adapter_id: u32,
    pub vram_bytes: u64,
    pub includes_kv_cache: bool,
}

/// Adapter zeroization event (adapter.zeroized)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterZeroizedEvent {
    pub adapter_id: String,
    pub bytes: usize,
    pub ts: u64,
}

/// Adapter reload event (adapter.reload)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterReloadEvent {
    pub adapter_id: String,
    pub source: String, // "cas"
    pub ts: u64,
}

/// Adapter VRAM usage event (adapter.vram_bytes)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterVramUsageEvent {
    pub adapter_id: String,
    pub phase: String, // "start" | "end"
    pub bytes: usize,
}

/// Router K=0 event (router.k0)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterK0Event {
    pub reason: String,
}

/// Key age warning event (security.key_age_warning)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyAgeWarningEvent {
    pub key_label: String,
    pub age_days: u64,
    pub threshold_days: u64,
}

/// Enclave operation event (security.enclave_operation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnclaveOperationEvent {
    pub operation: String,
    pub artifact_hash: Option<String>,
    pub result: String,
}

/// System metrics event (system.metrics)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetricsEvent {
    pub cpu_usage: f32,
    pub memory_usage: f32,
    pub disk_read_bytes: u64,
    pub disk_write_bytes: u64,
    pub network_rx_bytes: u64,
    pub network_tx_bytes: u64,
    pub gpu_utilization: Option<f32>,
    pub gpu_memory_used: Option<u64>,
    pub uptime_seconds: u64,
    pub process_count: usize,
    pub load_average: LoadAverageEvent,
    pub timestamp: u64,
}

/// Load average event data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadAverageEvent {
    pub load_1min: f64,
    pub load_5min: f64,
    pub load_15min: f64,
}

/// System health event (system.health)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealthEvent {
    pub status: String,
    pub checks: Vec<HealthCheckEvent>,
    pub timestamp: u64,
}

/// Health check event data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckEvent {
    pub name: String,
    pub status: String,
    pub message: String,
    pub value: Option<f32>,
    pub threshold: Option<f32>,
}

/// Threshold violation event (system.threshold_violation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdViolationEvent {
    pub metric_name: String,
    pub current_value: f32,
    pub threshold_value: f32,
    pub severity: String,
    pub timestamp: u64,
}

/// Kernel noise tracking event (kernel.noise)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelNoiseEvent {
    /// Layer identifier
    pub layer_id: String,
    /// L2 norm of the error vector
    pub l2_error: f64,
    /// Maximum absolute error
    pub max_error: f64,
    /// Mean absolute error
    pub mean_error: f64,
    /// Number of elements compared
    pub element_count: usize,
    /// Error threshold used
    pub threshold: f64,
    /// Step count when measurement was taken
    pub step_count: u64,
    /// Timestamp (microseconds since epoch)
    pub timestamp_us: u64,
}

impl KernelNoiseEvent {
    pub fn new(
        layer_id: String,
        l2_error: f64,
        max_error: f64,
        mean_error: f64,
        element_count: usize,
        threshold: f64,
        step_count: u64,
    ) -> Self {
        Self {
            layer_id,
            l2_error,
            max_error,
            mean_error,
            element_count,
            threshold,
            step_count,
            timestamp_us: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
        }
    }
}

/// Kernel step summary event (kernel.step)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelStepEvent {
    /// Step count
    pub step_count: u64,
    /// Number of layers tracked in this step
    pub layer_count: usize,
    /// Total L2 error across all layers
    pub total_l2_error: f64,
    /// Maximum layer error
    pub max_layer_error: f64,
    /// Mean layer error
    pub mean_layer_error: f64,
    /// Overall stability score
    pub stability_score: f64,
    /// List of layers that exceeded threshold
    pub threshold_violations: Vec<String>,
    /// Timestamp (microseconds since epoch)
    pub timestamp_us: u64,
}

impl KernelStepEvent {
    pub fn new(
        step_count: u64,
        layer_count: usize,
        total_l2_error: f64,
        max_layer_error: f64,
        mean_layer_error: f64,
        stability_score: f64,
        threshold_violations: Vec<String>,
    ) -> Self {
        Self {
            step_count,
            layer_count,
            total_l2_error,
            max_layer_error,
            mean_layer_error,
            stability_score,
            threshold_violations,
            timestamp_us: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
        }
    }
}
