//! Settings override loader
//!
//! Loads settings overrides from var/settings_override.json and applies them
//! to the base configuration. This allows runtime settings updates via the API
//! while maintaining the deterministic configuration system.

use crate::state::ApiConfig;
use adapteros_api_types::{
    GeneralSettings, PerformanceSettings, SecuritySettings, ServerSettings, UpdateSettingsRequest,
};
use std::fs;
use std::path::Path;
use std::sync::{Arc, RwLock};

/// Load settings overrides from file and apply to ApiConfig
///
/// This function loads the settings_override.json file (if it exists) and applies
/// the overrides to the provided ApiConfig. It's designed to be called during
/// server initialization after the base config is loaded.
///
/// # Arguments
/// * `api_config` - The base API configuration to apply overrides to
///
/// # Returns
/// * `Ok(true)` if overrides were loaded and applied
/// * `Ok(false)` if no override file exists
/// * `Err` if the file exists but couldn't be read or parsed
pub fn load_and_apply_overrides(api_config: &Arc<RwLock<ApiConfig>>) -> Result<bool, String> {
    let override_path = "var/settings_override.json";

    // Check if override file exists
    if !Path::new(override_path).exists() {
        tracing::debug!("No settings override file found at {}", override_path);
        return Ok(false);
    }

    // Read override file
    let content = fs::read_to_string(override_path)
        .map_err(|e| format!("Failed to read settings override file: {}", e))?;

    // Parse override request
    let overrides: UpdateSettingsRequest = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse settings override file: {}", e))?;

    // Apply overrides to ApiConfig
    // Note: ApiConfig only contains a subset of settings (metrics, timeouts, capacity)
    // Full settings are in adapteros-server Config and require restart to take effect

    let mut applied_count = 0;

    if overrides.performance.is_some() {
        if let Ok(mut cfg) = api_config.write() {
            if let Some(ref perf) = overrides.performance {
                cfg.capacity_limits.models_per_worker = Some(perf.max_adapters as usize);
                cfg.capacity_limits.models_per_tenant = Some((perf.max_adapters / 2) as usize);
                applied_count += 1;
            }
        }
    }

    tracing::info!(
        override_path = override_path,
        applied_count = applied_count,
        "Settings overrides loaded and applied"
    );

    Ok(true)
}

/// Load full settings including overrides
///
/// This is a helper that can be used to get the complete settings state,
/// combining the base configuration with any overrides from the file.
///
/// # Arguments
/// * `base_general` - Base general settings
/// * `base_server` - Base server settings
/// * `base_security` - Base security settings
/// * `base_performance` - Base performance settings
///
/// # Returns
/// The merged settings with overrides applied
pub fn load_full_settings_with_overrides(
    base_general: GeneralSettings,
    base_server: ServerSettings,
    base_security: SecuritySettings,
    base_performance: PerformanceSettings,
) -> Result<
    (
        GeneralSettings,
        ServerSettings,
        SecuritySettings,
        PerformanceSettings,
    ),
    String,
> {
    let override_path = "var/settings_override.json";

    // Return base settings if no override file
    if !Path::new(override_path).exists() {
        return Ok((base_general, base_server, base_security, base_performance));
    }

    // Read and parse override file
    let content = fs::read_to_string(override_path)
        .map_err(|e| format!("Failed to read settings override file: {}", e))?;

    let overrides: UpdateSettingsRequest = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse settings override file: {}", e))?;

    // Apply overrides (override values take precedence)
    let general = overrides.general.unwrap_or(base_general);
    let server = overrides.server.unwrap_or(base_server);
    let security = overrides.security.unwrap_or(base_security);
    let performance = overrides.performance.unwrap_or(base_performance);

    Ok((general, server, security, performance))
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_core::{BackendKind, SeedMode};

    #[test]
    fn test_load_overrides_no_file() {
        let api_config = Arc::new(RwLock::new(ApiConfig {
            metrics: crate::state::MetricsConfig {
                enabled: true,
                bearer_token: "test".to_string(),
            },
            directory_analysis_timeout_secs: 120,
            use_session_stack_for_routing: false,
            capacity_limits: Default::default(),
            general: None,
            server: Default::default(),
            security: Default::default(),
            auth: Default::default(),
            self_hosting: Default::default(),
            performance: Default::default(),
            streaming: Default::default(),
            paths: crate::config::PathsConfig {
                artifacts_root: "var/artifacts".to_string(),
                bundles_root: "var/bundles".to_string(),
                adapters_root: "var/adapters/repo".to_string(),
                plan_dir: "var/plans".to_string(),
                datasets_root: "var/datasets".to_string(),
                documents_root: "var/documents".to_string(),
            },
            chat_context: Default::default(),
            seed_mode: SeedMode::BestEffort,
            backend_profile: BackendKind::default_inference_backend(),
            worker_id: 0,
        }));

        let result = load_and_apply_overrides(&api_config);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), false);
    }

    #[test]
    fn test_load_full_settings_no_overrides() {
        let general = GeneralSettings {
            system_name: "Test".to_string(),
            environment: "test".to_string(),
            api_base_url: "http://localhost".to_string(),
        };
        let server = ServerSettings {
            http_port: 8080,
            https_port: None,
            uds_socket_path: None,
            production_mode: false,
        };
        let security = SecuritySettings {
            jwt_mode: "eddsa".to_string(),
            token_ttl_seconds: 3600,
            require_mfa: false,
            egress_enabled: true,
            require_pf_deny: false,
        };
        let performance = PerformanceSettings {
            max_adapters: 100,
            max_workers: 10,
            memory_threshold_pct: 0.85,
            cache_size_mb: 1024,
        };

        let result = load_full_settings_with_overrides(
            general.clone(),
            server.clone(),
            security.clone(),
            performance.clone(),
        );

        assert!(result.is_ok());
        let (g, s, sec, p) = result.unwrap();
        assert_eq!(g.system_name, "Test");
        assert_eq!(s.http_port, 8080);
        assert_eq!(sec.jwt_mode, "eddsa");
        assert_eq!(p.max_adapters, 100);
    }
}
