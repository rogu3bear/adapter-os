//! Helper functions for converting server configs to orchestrator configs
//!
//! This module provides type-safe conversions between the server's configuration
//! structures and the orchestrator's configuration structures, avoiding circular
//! dependencies while maintaining proper field mappings.

use adapteros_orchestrator::{code_jobs::PathsConfig, OrchestratorConfig};
use adapteros_server::config::{
    Config, OrchestratorConfig as ServerOrchestratorConfig, PathsConfig as ServerPathsConfig,
};

/// Convert server PathsConfig to orchestrator PathsConfig
///
/// Maps server path fields to orchestrator path fields:
/// - `artifacts_root` → `artifacts_dir` and `artifacts_root`
/// - `adapters_root` → `adapters_root`
/// - `temp_dir` and `cache_dir` are derived from `artifacts_root`
pub fn convert_paths_config(server_paths: &ServerPathsConfig) -> PathsConfig {
    PathsConfig {
        artifacts_dir: server_paths.artifacts_root.clone(),
        temp_dir: format!("{}/temp", server_paths.artifacts_root),
        cache_dir: format!("{}/cache", server_paths.artifacts_root),
        adapters_root: server_paths.adapters_root.clone(),
        artifacts_root: server_paths.artifacts_root.clone(),
    }
}

/// Convert server OrchestratorConfig to orchestrator OrchestratorConfig
///
/// Only maps fields that are actually used by CodeJobManager:
/// - `base_model` → `base_model`
/// - `ephemeral_adapter_ttl_hours` → `ephemeral_adapter_ttl_hours` (u64 → i32)
///
/// Other fields use defaults appropriate for server usage:
/// - `continue_on_error`: false (fail fast in server context)
/// - `cpid`: empty string (set per-operation, not globally)
/// - `db_path`: from DATABASE_URL env var or config.db.path
/// - `bundles_path`: from config.paths.bundles_root
/// - `manifests_path`: "manifests" (standard location)
pub fn convert_orchestrator_config(
    server_config: &Config,
    server_orchestrator: &ServerOrchestratorConfig,
) -> OrchestratorConfig {
    // Get database path from DATABASE_URL env var, falling back to config.db.path
    // Remove sqlite:// prefix if present to get filesystem path
    let db_path = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| {
            // Fallback to config value, adding sqlite:// prefix if not present
            let path = &server_config.db.path;
            if path.starts_with("sqlite://") || path.starts_with("sqlite::") {
                path.clone()
            } else {
                format!("sqlite://{}", path)
            }
        })
        .replace("sqlite://", "")
        .replace("sqlite::", "");

    OrchestratorConfig {
        continue_on_error: false,
        cpid: String::new(),
        db_path,
        bundles_path: server_config.paths.bundles_root.clone(),
        manifests_path: "manifests".to_string(),
        base_model: server_orchestrator.base_model.clone(),
        ephemeral_adapter_ttl_hours: server_orchestrator.ephemeral_adapter_ttl_hours as i32,
        gate_timeout_secs: OrchestratorConfig::default().gate_timeout_secs,
    }
}
