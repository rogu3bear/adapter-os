//! Node Detail Handler
//!
//! Provides detailed information about nodes including:
//! - Hardware specifications
//! - Resource usage metrics
//! - Loaded adapters
//! - Federation role

use crate::permissions::{require_permission, Permission};
use crate::{AppState, Claims, ErrorResponse};
use adapteros_api_types::API_SCHEMA_VERSION;
use adapteros_system_metrics::SystemMetricsCollector;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use utoipa::ToSchema;

/// Node detail response with complete node information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct NodeDetailResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub id: String,
    pub hostname: String,
    pub agent_endpoint: String,
    pub status: String,
    pub last_seen_at: Option<String>,
    pub hardware: HardwareInfo,
    pub resource_usage: NodeResourceUsage,
    pub adapters_loaded: Vec<String>,
    pub federation_role: String,
    pub labels: serde_json::Value,
    pub created_at: String,
}

/// Hardware information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct HardwareInfo {
    pub cpu_cores: usize,
    pub cpu_model: String,
    pub total_memory_gb: f32,
    pub gpu_available: bool,
    pub gpu_model: Option<String>,
    pub gpu_memory_gb: Option<f32>,
    pub ane_available: bool,
}

/// Node resource usage
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct NodeResourceUsage {
    pub cpu_usage_percent: f32,
    pub memory_usage_percent: f32,
    pub memory_used_gb: f32,
    pub memory_available_gb: f32,
    pub disk_usage_percent: f32,
    pub network_rx_bytes: u64,
    pub network_tx_bytes: u64,
    pub gpu_utilization_percent: Option<f32>,
    pub gpu_memory_used_gb: Option<f32>,
    pub timestamp: u64,
}

fn schema_version() -> String {
    API_SCHEMA_VERSION.to_string()
}

/// Get node detail
#[utoipa::path(
    tag = "nodes",
    get,
    path = "/v1/nodes/{node_id}/detail",
    params(
        ("node_id" = String, Path, description = "Node ID")
    ),
    responses(
        (status = 200, description = "Node detail", body = NodeDetailResponse)
    )
)]
pub async fn get_node_detail(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(node_id): Path<String>,
) -> Result<Json<NodeDetailResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Permission check: NodeView required
    require_permission(&claims, Permission::NodeView)?;

    // Fetch node from database
    let node = sqlx::query_as::<_, NodeRecord>(
        "SELECT id, hostname, agent_endpoint, status, last_seen_at, labels_json, created_at
         FROM nodes WHERE id = ?",
    )
    .bind(&node_id)
    .fetch_one(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("node not found")
                    .with_code("NODE_NOT_FOUND")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Collect system metrics
    let mut collector = SystemMetricsCollector::new();
    let metrics = collector.collect_metrics();

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System time before UNIX epoch")
        .as_secs();

    // Get hardware info
    let hardware = get_hardware_info(&mut collector);

    // Get adapters loaded on this node
    let adapters_loaded = sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT a.adapter_id
         FROM workers w
         JOIN adapters a ON a.id IN (SELECT json_extract(value, '$') FROM json_each(w.adapters_loaded_json))
         WHERE w.node_id = ? AND w.status = 'serving'",
    )
    .bind(&node_id)
    .fetch_all(state.db.pool())
    .await
    .unwrap_or_default();

    // Determine federation role
    let federation_role = determine_federation_role(&state, &node_id).await;

    // Calculate memory metrics
    let total_memory = hardware.total_memory_gb * 1_073_741_824.0; // Convert GB to bytes
    let memory_used_gb = (metrics.memory_usage / 100.0) * (hardware.total_memory_gb as f64);
    let memory_available_gb = (hardware.total_memory_gb as f64) - memory_used_gb;

    Ok(Json(NodeDetailResponse {
        schema_version: API_SCHEMA_VERSION.to_string(),
        id: node.id,
        hostname: node.hostname,
        agent_endpoint: node.agent_endpoint,
        status: node.status,
        last_seen_at: node.last_seen_at,
        hardware,
        resource_usage: NodeResourceUsage {
            cpu_usage_percent: metrics.cpu_usage as f32,
            memory_usage_percent: metrics.memory_usage as f32,
            memory_used_gb: memory_used_gb as f32,
            memory_available_gb: memory_available_gb as f32,
            disk_usage_percent: metrics.disk_io.usage_percent,
            network_rx_bytes: metrics.network_io.rx_bytes,
            network_tx_bytes: metrics.network_io.tx_bytes,
            gpu_utilization_percent: metrics.gpu_metrics.utilization.map(|v| v as f32),
            gpu_memory_used_gb: metrics
                .gpu_metrics
                .memory_used
                .map(|v| v as f32 / 1_073_741_824.0),
            timestamp,
        },
        adapters_loaded,
        federation_role,
        labels: serde_json::from_str(&node.labels_json.unwrap_or_else(|| "{}".to_string()))
            .unwrap_or_else(|_| serde_json::json!({})),
        created_at: node.created_at,
    }))
}

/// Node record from database
#[derive(Debug, sqlx::FromRow)]
struct NodeRecord {
    id: String,
    hostname: String,
    agent_endpoint: String,
    status: String,
    last_seen_at: Option<String>,
    labels_json: Option<String>,
    created_at: String,
}

/// Get hardware information
fn get_hardware_info(collector: &mut SystemMetricsCollector) -> HardwareInfo {
    use sysinfo::System;

    let sys = System::new_all();
    let cpus = sys.cpus();
    let cpu_cores = cpus.len();
    let cpu_model = cpus
        .first()
        .map(|cpu| cpu.brand())
        .unwrap_or("Unknown")
        .to_string();
    let total_memory_gb = sys.total_memory() as f32 / 1_073_741_824.0;

    // Collect GPU metrics to determine availability
    let metrics = collector.collect_metrics();
    let gpu_available = metrics.gpu_metrics.memory_total.is_some();
    let gpu_model = if gpu_available {
        Some("Apple GPU".to_string()) // Placeholder - would query actual GPU model
    } else {
        None
    };
    let gpu_memory_gb = metrics
        .gpu_metrics
        .memory_total
        .map(|v| v as f32 / 1_073_741_824.0);

    // Check for ANE availability (macOS only)
    #[cfg(target_os = "macos")]
    let ane_available = {
        // Placeholder - would check actual ANE availability via Metal/CoreML
        true
    };
    #[cfg(not(target_os = "macos"))]
    let ane_available = false;

    HardwareInfo {
        cpu_cores,
        cpu_model,
        total_memory_gb,
        gpu_available,
        gpu_model,
        gpu_memory_gb,
        ane_available,
    }
}

/// Determine federation role for node
async fn determine_federation_role(state: &AppState, node_id: &str) -> String {
    // Check if node is primary in federation
    let is_primary = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM federation_config WHERE primary_node_id = ?",
    )
    .bind(node_id)
    .fetch_one(state.db.pool())
    .await
    .unwrap_or(0);

    if is_primary > 0 {
        "primary".to_string()
    } else {
        "replica".to_string()
    }
}
