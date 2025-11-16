//! Metrics types

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Quality metrics response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct QualityMetricsResponse {
    pub arr: f32,  // Answer Relevance Rate
    pub ecs5: f32, // Evidence Citation Score @ 5
    pub hlr: f32,  // Hallucination Rate
    pub cr: f32,   // Contradiction Rate
    pub timestamp: String,
}

/// Adapter metrics response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdapterMetricsResponse {
    pub adapters: Vec<AdapterPerformance>,
}

/// Adapter performance metrics
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdapterPerformance {
    pub adapter_id: String,
    pub name: String,
    pub activation_rate: f64,
    pub avg_gate_value: f64,
    pub total_requests: i64,
}

/// System metrics response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SystemMetricsResponse {
    pub cpu_usage: f32,
    pub memory_usage: f32,
    pub active_workers: i32,
    pub requests_per_second: f32,
    pub avg_latency_ms: f32,
    pub disk_usage: f32,
    pub network_bandwidth: f32,
    pub gpu_utilization: f32,
    pub uptime_seconds: u64,
    pub process_count: usize,
    pub load_average: LoadAverageResponse,
    pub timestamp: u64,
<<<<<<< HEAD
    #[serde(default)]
    pub memory_usage_pct: f32,
    #[serde(default)]
    pub adapter_count: i32,
    #[serde(default)]
    pub active_sessions: i32,
    #[serde(default)]
    pub tokens_per_second: f32,
    #[serde(default)]
    pub latency_p95_ms: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_usage_percent: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_usage_percent: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_usage_percent: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_rx_bytes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_tx_bytes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_rx_packets: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_tx_packets: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_bandwidth_mbps: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_utilization_percent: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_memory_used_gb: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_memory_total_gb: Option<f32>,
=======
>>>>>>> integration-branch
}

/// Load average response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LoadAverageResponse {
    pub load_1min: f64,
    pub load_5min: f64,
    pub load_15min: f64,
}

/// Network I/O metrics
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NetworkMetrics {
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub packets_in: u64,
    pub packets_out: u64,
}
