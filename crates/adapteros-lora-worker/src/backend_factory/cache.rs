use super::model_config::{
    resolve_config_toml_path, resolve_model_cache_budget_bytes, PinConflictMode,
};
use crate::model_handle_cache::ModelHandleCache;
use adapteros_config::ConfigLoader;
use adapteros_core::{constants::BYTES_PER_MB, AosError, Result};
use adapteros_telemetry::TelemetryWriter;
use once_cell::sync::Lazy;
use tracing::{debug, error, info};

/// Per-worker model cache singleton
///
/// This cache ensures that the same model is only loaded once per worker process.
/// Budget must be explicitly provided (env or TOML); missing/zero budgets return errors
/// at first use rather than panicking at initialization.
static MODEL_CACHE: Lazy<std::result::Result<ModelHandleCache, String>> =
    Lazy::new(|| match resolve_model_cache_budget_bytes() {
        Ok(max_bytes) => {
            let max_mb = max_bytes / BYTES_PER_MB;
            info!(
                max_memory_mb = max_mb,
                "Initializing per-worker model cache with explicit budget"
            );
            Ok(ModelHandleCache::new(max_bytes))
        }
        Err(err) => {
            error!(
                error = %err,
                "Model cache budget missing or invalid; model loading will fail"
            );
            Err(err.to_string())
        }
    });

/// Get reference to the model cache, returning an error if initialization failed
///
/// This function is public to allow cleanup during worker shutdown.
pub fn get_model_cache() -> Result<&'static ModelHandleCache> {
    MODEL_CACHE
        .as_ref()
        .map_err(|e| AosError::Config(format!("Model cache initialization failed: {}", e)))
}

/// Validate model cache budget is configured and return the budget in bytes.
///
/// Call this early in worker startup to fail fast with a clear error message
/// rather than discovering the problem during backend creation.
///
/// # Returns
/// - `Ok(budget_bytes)` if the cache is properly configured
/// - `Err(AosError::Config(...))` if budget is missing or invalid
pub fn validate_model_cache_budget() -> Result<u64> {
    // Check environment variable first
    let env_value = std::env::var("AOS_MODEL_CACHE_MAX_MB").ok();

    // Check config file
    let toml_path = resolve_config_toml_path()?;
    let config_value = ConfigLoader::new()
        .load(Vec::new(), toml_path.clone())
        .ok()
        .and_then(|cfg| cfg.get("model.cache.max.mb").map(String::from));

    debug!(
        env_var = ?env_value,
        config_value = ?config_value,
        "Model cache budget configuration check"
    );

    match get_model_cache() {
        Ok(cache) => {
            let budget_bytes = cache.max_memory_bytes();
            let budget_mb = budget_bytes / BYTES_PER_MB;
            info!(
                budget_mb = budget_mb,
                budget_bytes = budget_bytes,
                source = if env_value.is_some() {
                    "AOS_MODEL_CACHE_MAX_MB"
                } else {
                    "config file"
                },
                "Model cache budget validated"
            );
            Ok(budget_bytes)
        }
        Err(_e) => {
            // Build a helpful error message with context
            let mut error_msg = String::from("Model cache budget not configured.\n\n");

            error_msg.push_str("Configuration Status:\n");
            if let Some(ref val) = env_value {
                error_msg.push_str(&format!(
                    "  - AOS_MODEL_CACHE_MAX_MB: '{}' (found but may be invalid)\n",
                    val
                ));
            } else {
                error_msg.push_str("  - AOS_MODEL_CACHE_MAX_MB: not set\n");
            }

            if let Some(ref val) = config_value {
                error_msg.push_str(&format!(
                    "  - model.cache.max.mb (config): '{}' (found but may be invalid)\n",
                    val
                ));
            } else {
                error_msg.push_str("  - model.cache.max.mb (config): not set\n");
            }

            error_msg.push_str("\nHow to fix:\n");
            error_msg.push_str("  1. Set environment variable:\n");
            error_msg.push_str("     export AOS_MODEL_CACHE_MAX_MB=8192  # For 8GB cache\n\n");
            error_msg.push_str("  2. Or set in config TOML:\n");
            error_msg.push_str("     model.cache.max.mb = \"8192\"\n\n");
            error_msg.push_str("Recommended minimums:\n");
            error_msg.push_str("  - 7B models: 8192 MB (8GB)\n");
            error_msg.push_str("  - 13B models: 16384 MB (16GB)\n");
            error_msg.push_str("  - 70B models: 40960 MB (40GB)\n");

            Err(AosError::Config(error_msg))
        }
    }
}

/// Configure telemetry writer for model cache failure reporting.
pub fn configure_model_cache_telemetry(writer: TelemetryWriter) {
    if let Ok(cache) = get_model_cache() {
        cache.set_telemetry(writer);
    }
}

/// Configuration for base model pinning.
pub struct BaseModelPinConfig {
    pub enabled: bool,
    pub budget_bytes: Option<u64>,
    pub model_id: Option<String>,
    pub conflict_mode: PinConflictMode,
}

/// Configure base model pinning state in the model cache.
pub fn configure_model_cache_pinning(config: BaseModelPinConfig) -> Result<()> {
    let cache = get_model_cache()?;
    cache.configure_base_model_pinning(
        config.enabled,
        config.budget_bytes,
        config.model_id,
        config.conflict_mode,
    );
    Ok(())
}
