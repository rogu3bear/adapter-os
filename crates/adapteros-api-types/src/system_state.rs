//! System State Types
//!
//! Ground truth system state types providing hierarchical visibility:
//! Node -> Tenant -> Stack -> Adapter
//!
//! All timestamps use RFC3339 format for consistency.

use serde::{Deserialize, Serialize};

use crate::API_SCHEMA_VERSION;

fn schema_version() -> String {
    API_SCHEMA_VERSION.to_string()
}

/// Returns current timestamp in RFC3339 format
pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// RAG system status indicating whether embedding model is available
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum RagStatus {
    Enabled {
        model_hash: String,
        dimension: usize,
    },
    Disabled {
        reason: String,
    },
}

/// Ground truth system state response
///
/// Provides hierarchical visibility into the entire system state including:
/// - Node hardware and service health
/// - Tenants with their stacks and adapters
/// - Memory pressure and top adapters by usage
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct SystemStateResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    /// RFC3339 timestamp when this response was generated
    pub timestamp: String,
    /// Origin node that produced this data
    pub origin: StateOrigin,
    /// Node-level state (hardware, services)
    pub node: NodeState,
    /// Tenant states with nested stacks and adapters
    pub tenants: Vec<TenantState>,
    /// Memory state summary
    pub memory: MemoryState,
    /// RAG status (whether embedding model is loaded and available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rag_status: Option<RagStatus>,
}

/// Data origin for traceability
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct StateOrigin {
    /// Unique node identifier
    pub node_id: String,
    /// Node hostname
    pub hostname: String,
    /// Federation role (primary, replica, standalone)
    pub federation_role: String,
}

/// Node-level state
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct NodeState {
    /// System uptime in seconds
    pub uptime_seconds: u64,
    /// Current CPU usage percentage
    pub cpu_usage_percent: f32,
    /// Current memory usage percentage
    pub memory_usage_percent: f32,
    /// Whether GPU is available on this node
    pub gpu_available: bool,
    /// Whether ANE (Apple Neural Engine) is available
    pub ane_available: bool,
    /// Health status of critical services
    pub services: Vec<ServiceState>,
}

/// Service health within a node
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct ServiceState {
    /// Service name (e.g., "api_server", "lifecycle_manager")
    pub name: String,
    /// Current health status
    pub status: ServiceHealthStatus,
    /// RFC3339 timestamp of last health check
    pub last_check: String,
}

/// Service health status enumeration
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum ServiceHealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

/// Tenant-level state with nested stacks
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct TenantState {
    /// Tenant unique identifier
    pub tenant_id: String,
    /// Tenant display name
    pub name: String,
    /// Tenant status (active, paused, archived)
    pub status: String,
    /// Total memory usage across all adapters in MB
    pub memory_usage_mb: f32,
    /// Total number of adapters for this tenant
    pub adapter_count: usize,
    /// Stacks belonging to this tenant (includes active stack)
    pub stacks: Vec<StackSummary>,
}

/// Stack summary with nested adapters
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct StackSummary {
    /// Stack unique identifier
    pub stack_id: String,
    /// Stack name (e.g., "prod/main/inference/v3")
    pub name: String,
    /// Whether this stack is currently active
    pub is_active: bool,
    /// Number of adapters in this stack
    pub adapter_count: usize,
    /// Adapters in this stack (may be empty if include_adapters=false)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub adapters: Vec<AdapterSummary>,
}

/// Adapter summary within a stack
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct AdapterSummary {
    /// Adapter unique identifier
    pub adapter_id: String,
    /// Adapter name
    pub name: String,
    /// Current lifecycle state
    pub state: AdapterLifecycleState,
    /// Memory usage in MB
    pub memory_mb: f32,
    /// RFC3339 timestamp of last access (if known)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_access: Option<String>,
    /// Total activation count
    pub activation_count: u64,
    /// Whether adapter is pinned (resident)
    pub pinned: bool,
}

/// Adapter lifecycle state
///
/// Mirrors the internal AdapterState enum from adapteros-lora-lifecycle
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum AdapterLifecycleState {
    /// Not in memory, metadata only
    Unloaded,
    /// Weights loaded, not in active rotation
    Cold,
    /// In rotation pool, occasionally selected
    Warm,
    /// Frequently selected, prioritized
    Hot,
    /// Always active (pinned adapters)
    Resident,
}

impl std::fmt::Display for AdapterLifecycleState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unloaded => write!(f, "unloaded"),
            Self::Cold => write!(f, "cold"),
            Self::Warm => write!(f, "warm"),
            Self::Hot => write!(f, "hot"),
            Self::Resident => write!(f, "resident"),
        }
    }
}

impl From<&str> for AdapterLifecycleState {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "cold" => Self::Cold,
            "warm" => Self::Warm,
            "hot" => Self::Hot,
            "resident" => Self::Resident,
            _ => Self::Unloaded,
        }
    }
}

/// Memory state summary
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct MemoryState {
    /// Total system memory in MB
    pub total_mb: u64,
    /// Used memory in MB
    pub used_mb: u64,
    /// Available memory in MB
    pub available_mb: u64,
    /// Headroom percentage (policy requires >= 15%)
    pub headroom_percent: f32,
    /// Current pressure level
    pub pressure_level: MemoryPressureLevel,
    /// ANE-specific memory state (Apple Silicon only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ane: Option<AneMemoryState>,
    /// Top adapters by memory consumption
    pub top_adapters: Vec<AdapterMemorySummary>,
}

/// Memory pressure level
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum MemoryPressureLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl std::fmt::Display for MemoryPressureLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => write!(f, "low"),
            Self::Medium => write!(f, "medium"),
            Self::High => write!(f, "high"),
            Self::Critical => write!(f, "critical"),
        }
    }
}

impl From<&str> for MemoryPressureLevel {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "low" => Self::Low,
            "medium" => Self::Medium,
            "high" => Self::High,
            "critical" => Self::Critical,
            _ => Self::Low,
        }
    }
}

/// ANE-specific memory state (Apple Silicon only)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct AneMemoryState {
    /// Allocated ANE memory in MB
    pub allocated_mb: u64,
    /// Used ANE memory in MB
    pub used_mb: u64,
    /// Available ANE memory in MB
    pub available_mb: u64,
    /// ANE memory usage percentage
    pub usage_percent: f32,
}

/// Adapter memory summary for top-N display
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct AdapterMemorySummary {
    /// Adapter unique identifier
    pub adapter_id: String,
    /// Adapter name
    pub name: String,
    /// Memory usage in MB
    pub memory_mb: f32,
    /// Current lifecycle state
    pub state: AdapterLifecycleState,
    /// Tenant that owns this adapter
    pub tenant_id: String,
}

/// Query parameters for system state endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema, utoipa::IntoParams))]
#[serde(rename_all = "snake_case")]
pub struct SystemStateQuery {
    /// Include adapter details in stack responses (default: true)
    #[serde(default = "default_include_adapters")]
    pub include_adapters: Option<bool>,
    /// Number of top adapters by memory to include (default: 10)
    #[serde(default = "default_top_adapters_opt")]
    pub top_adapters: Option<u32>,
    /// Filter to specific tenant (Admin can see all, others see own)
    pub tenant_id: Option<String>,
}

fn default_include_adapters() -> Option<bool> {
    Some(true)
}

fn default_top_adapters_opt() -> Option<u32> {
    Some(10)
}

impl Default for SystemStateQuery {
    fn default() -> Self {
        Self {
            include_adapters: Some(true),
            top_adapters: Some(10),
            tenant_id: None,
        }
    }
}
