//! Unified system status types for cockpit view.
//!
//! Aggregates integrity posture, readiness checks, boot lifecycle, and kernel
//! resource summaries into a single contract for the UI.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::API_SCHEMA_VERSION;

fn schema_version() -> String {
    API_SCHEMA_VERSION.to_string()
}

/// Combined system status response for `/v1/system/status`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct SystemStatusResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    /// RFC3339 timestamp when this snapshot was produced.
    pub timestamp: String,
    /// Integrity posture (mode, federation, drift signals).
    pub integrity: IntegrityStatus,
    /// Readiness checks (db, migrations, workers, models).
    pub readiness: ReadinessStatus,
    /// Boot lifecycle details when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boot: Option<BootStatus>,
    /// Kernel/model summary and memory pressure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kernel: Option<KernelStatus>,
}

/// Integrity posture for the control plane.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct IntegrityStatus {
    pub mode: String,
    pub is_federated: bool,
    pub strict_mode: bool,
    pub pf_deny_ok: bool,
    pub drift: DriftStatus,
}

/// Drift severity and optional summary.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct DriftStatus {
    pub level: DriftLevel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

/// Drift severity indicator.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DriftLevel {
    Ok,
    Warn,
    Critical,
}

/// Readiness snapshot across critical dependencies.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct ReadinessStatus {
    pub overall: StatusIndicator,
    pub checks: ReadinessChecks,
}

/// Individual readiness checks.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct ReadinessChecks {
    pub db: ComponentCheck,
    pub migrations: ComponentCheck,
    pub workers: ComponentCheck,
    pub models: ComponentCheck,
}

/// Component readiness with optional metadata.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct ComponentCheck {
    pub status: StatusIndicator,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub critical: Option<bool>,
}

/// Canonical status indicator for checks and overall readiness.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StatusIndicator {
    Ready,
    NotReady,
    Unknown,
}

/// Boot lifecycle summary.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct BootStatus {
    pub phase: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boot_trace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub timings: Vec<BootPhaseTiming>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub degraded: Vec<DegradedReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<BootFailure>,
}

/// Individual boot phase timing.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct BootPhaseTiming {
    pub phase: String,
    pub elapsed_ms: u64,
}

/// Degraded component reason.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct DegradedReason {
    pub component: String,
    pub reason: String,
}

/// Boot failure information.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct BootFailure {
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Kernel/model summary.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct KernelStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelStatusSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<PlanStatusSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapters: Option<AdapterInventory>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<KernelMemorySummary>,
}

/// Current model status (aggregated across tenants).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct ModelStatusSummary {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// Latest plan pointer (best effort).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct PlanStatusSummary {
    pub plan_id: String,
    pub tenant_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

/// Adapter inventory across the cluster.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct AdapterInventory {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_active: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loaded: Option<i64>,
}

/// Memory summary for UMA and ANE where available.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct KernelMemorySummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ane: Option<AneMemorySummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uma: Option<UmaMemorySummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pressure: Option<String>,
}

/// Apple Neural Engine memory stats.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct AneMemorySummary {
    pub allocated_mb: u64,
    pub used_mb: u64,
    pub available_mb: u64,
    pub usage_pct: f32,
}

/// UMA headroom summary.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct UmaMemorySummary {
    pub total_mb: u64,
    pub used_mb: u64,
    pub available_mb: u64,
    pub headroom_pct: f32,
}
