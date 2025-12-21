//! Metrics types

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::schema_version;

/// Quality metrics response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct QualityMetricsResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub arr: f32,  // Answer Relevance Rate
    pub ecs5: f32, // Evidence Citation Score @ 5
    pub hlr: f32,  // Hallucination Rate
    pub cr: f32,   // Contradiction Rate
    pub timestamp: String,
}

/// Adapter metrics response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct AdapterMetricsResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub adapters: Vec<AdapterPerformance>,
}

/// Adapter performance metrics
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct AdapterPerformance {
    pub adapter_id: String,
    pub name: String,
    pub activation_rate: f64,
    pub avg_gate_value: f64,
    pub total_requests: i64,
}

/// System metrics response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct SystemMetricsResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
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
}

/// Load average response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct LoadAverageResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub load_1min: f64,
    pub load_5min: f64,
    pub load_15min: f64,
}

/// Network I/O metrics
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct NetworkMetrics {
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub packets_in: u64,
    pub packets_out: u64,
}
