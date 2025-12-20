//! Runtime session and configuration tracking handlers
//!
//! Provides endpoints for querying runtime session information, configuration snapshots,
//! and drift detection.

use crate::api_error::ApiError;
use crate::state::AppState;
use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use utoipa::ToSchema;

/// Query parameters for listing runtime sessions
#[derive(Debug, Deserialize, ToSchema)]
pub struct ListSessionsParams {
    /// Maximum number of sessions to return
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    10
}

/// Runtime session information response
#[derive(Debug, Serialize, ToSchema)]
pub struct RuntimeSessionResponse {
    /// Unique session identifier (UUID)
    pub session_id: String,
    /// Configuration snapshot hash (BLAKE3)
    pub config_hash: String,
    /// Binary version (from Cargo.toml)
    pub binary_version: String,
    /// Session start timestamp (ISO 8601)
    pub started_at: String,
    /// Hostname where server is running
    pub hostname: String,
    /// Runtime mode (dev, staging, prod)
    pub runtime_mode: String,
    /// Uptime in seconds since session start
    pub uptime_seconds: u64,
    /// Whether configuration drift was detected
    pub drift_detected: bool,
    /// Optional drift summary if detected
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drift_summary: Option<DriftSummaryResponse>,
    /// Runtime paths configuration
    pub paths: RuntimePathsResponse,
}

/// Configuration drift summary
#[derive(Debug, Serialize, ToSchema)]
pub struct DriftSummaryResponse {
    /// Previous configuration hash
    pub previous_hash: String,
    /// Number of fields that drifted
    pub field_count: usize,
    /// Individual field changes
    pub fields: Vec<DriftFieldResponse>,
}

/// Individual configuration field drift
#[derive(Debug, Serialize, ToSchema)]
pub struct DriftFieldResponse {
    /// Configuration key
    pub key: String,
    /// Previous value
    pub old_value: String,
    /// New value
    pub new_value: String,
    /// Drift severity (info, warning, critical)
    pub severity: String,
}

/// Runtime paths configuration
#[derive(Debug, Serialize, ToSchema)]
pub struct RuntimePathsResponse {
    /// Base model path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_path: Option<String>,
    /// Adapters storage directory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapters_root: Option<String>,
    /// Database file path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database_path: Option<String>,
    /// Variable data directory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub var_dir: Option<String>,
}

/// Get current runtime session information
///
/// Returns details about the current server runtime session including configuration,
/// version, uptime, and any detected configuration drift.
#[utoipa::path(
    get,
    path = "/v1/runtime/session",
    tag = "runtime",
    responses(
        (status = 200, description = "Current runtime session information", body = RuntimeSessionResponse),
        (status = 500, description = "Internal server error", body = crate::types::ErrorResponse)
    )
)]
pub async fn get_current_session(
    State(state): State<AppState>,
) -> Result<Json<RuntimeSessionResponse>, ApiError> {
    // Generate session ID (in production this would be persisted)
    let session_id = generate_session_id();

    // Get configuration snapshot and hash
    let config_hash = get_config_hash(&state)?;

    // Get binary version
    let binary_version = env!("CARGO_PKG_VERSION").to_string();

    // Calculate uptime (using boot state if available, otherwise system uptime)
    let uptime_seconds = calculate_uptime(&state);

    // Get hostname
    let hostname = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "unknown".to_string());

    // Get runtime mode
    let runtime_mode = state
        .runtime_mode
        .as_ref()
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "dev".to_string());

    // Check for configuration drift
    let (drift_detected, drift_summary) = check_config_drift(&state)?;

    // Get runtime paths
    let paths = get_runtime_paths(&state);

    // Get session start time (approximation based on uptime)
    let started_at = chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::seconds(uptime_seconds as i64))
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339();

    Ok(Json(RuntimeSessionResponse {
        session_id,
        config_hash,
        binary_version,
        started_at,
        hostname,
        runtime_mode,
        uptime_seconds,
        drift_detected,
        drift_summary,
        paths,
    }))
}

/// List recent runtime sessions
///
/// Returns a list of recent runtime sessions (currently returns only the current session).
/// In future versions, this may track historical sessions.
#[utoipa::path(
    get,
    path = "/v1/runtime/sessions",
    tag = "runtime",
    params(
        ("limit" = Option<usize>, Query, description = "Maximum number of sessions to return")
    ),
    responses(
        (status = 200, description = "List of runtime sessions", body = Vec<RuntimeSessionResponse>),
        (status = 500, description = "Internal server error", body = crate::types::ErrorResponse)
    )
)]
pub async fn list_sessions(
    State(state): State<AppState>,
    Query(params): Query<ListSessionsParams>,
) -> Result<Json<Vec<RuntimeSessionResponse>>, ApiError> {
    // For now, return only the current session
    // In the future, this could query a sessions table in the database
    let current_session = get_current_session(State(state)).await?;

    let sessions = vec![current_session.0];
    let limited = sessions.into_iter().take(params.limit).collect();

    Ok(Json(limited))
}

// Helper functions

/// Generate a session ID (UUID v4)
fn generate_session_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};

    // Simple session ID based on process start time + PID
    // In production, this could use a more sophisticated approach
    static INSTANCE_ID: AtomicU64 = AtomicU64::new(0);

    let instance = INSTANCE_ID.fetch_add(1, Ordering::SeqCst);
    let pid = std::process::id();

    format!("session-{}-{}", pid, instance)
}

/// Get configuration hash
fn get_config_hash(state: &AppState) -> Result<String, ApiError> {
    // Try to get effective config snapshot
    if let Some(cfg) = adapteros_config::try_effective_config() {
        let snapshot = adapteros_config::ConfigSnapshot::from_effective_config(cfg);
        return Ok(snapshot.hash);
    }

    // Fallback: compute hash from available config
    let config = state
        .config
        .read()
        .map_err(|_| ApiError::internal("Failed to read config"))?;
    let config_str = format!(
        "version={};production_mode={};db_pool_size={}",
        env!("CARGO_PKG_VERSION"),
        config.server.production_mode,
        state.db_pool.size()
    );
    let hash = blake3::hash(config_str.as_bytes());
    Ok(hash.to_hex().to_string())
}

/// Calculate uptime in seconds
fn calculate_uptime(state: &AppState) -> u64 {
    // Try boot state first
    if let Some(ref boot_state) = state.boot_state {
        return boot_state.elapsed().as_secs();
    }

    // Fallback: use process uptime (approximation)
    static PROCESS_START: std::sync::OnceLock<SystemTime> = std::sync::OnceLock::new();
    let start_time = PROCESS_START.get_or_init(SystemTime::now);

    SystemTime::now()
        .duration_since(*start_time)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Check for configuration drift
fn check_config_drift(_state: &AppState) -> Result<(bool, Option<DriftSummaryResponse>), ApiError> {
    // Try to detect configuration drift using adapteros-config
    if let Some(cfg) = adapteros_config::try_effective_config() {
        // Create current snapshot
        let _current_snapshot = adapteros_config::ConfigSnapshot::from_effective_config(cfg);

        // In production, we'd compare against a persisted baseline snapshot
        // For now, check if there's a stored baseline (would be in state or db)
        // If no baseline exists, no drift detected

        // TODO: Implement baseline storage and retrieval
        // For now, return no drift
        return Ok((false, None));
    }

    // Config not initialized, can't detect drift
    Ok((false, None))
}

/// Get runtime paths configuration
fn get_runtime_paths(state: &AppState) -> RuntimePathsResponse {
    let config = state.config.read().ok();

    RuntimePathsResponse {
        model_path: std::env::var("AOS_MODEL_PATH").ok(),
        adapters_root: config
            .as_ref()
            .map(|c| c.paths.adapters_root.clone())
            .or_else(|| Some("var/adapters".to_string())),
        database_path: std::env::var("AOS_DATABASE_URL").ok(),
        var_dir: std::env::var("AOS_VAR_DIR").ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_id_generation() {
        let id1 = generate_session_id();
        let id2 = generate_session_id();

        assert!(id1.starts_with("session-"));
        assert!(id2.starts_with("session-"));
        assert_ne!(id1, id2); // Should be unique
    }

    #[test]
    fn test_default_limit() {
        assert_eq!(default_limit(), 10);
    }
}
