use crate::model_key::ModelCacheIdentity;
use adapteros_config::schema::{parse_bool, parse_byte_size};
use adapteros_config::{reject_tmp_persistent_path, ConfigLoader, ModelConfig};
use adapteros_core::{AosError, Result};
use adapteros_lora_kernel_api::attestation::BackendType;
use std::path::Path;
use std::str::FromStr;
use tracing::{debug, error, warn};

#[cfg(any(target_os = "macos", feature = "multi-backend"))]
use adapteros_core::B3Hash;
#[cfg(any(target_os = "macos", feature = "multi-backend"))]
use serde_json::Value;

pub(crate) fn resolve_config_toml_path() -> Result<Option<String>> {
    if let Ok(val) = std::env::var("AOS_CONFIG_TOML") {
        if !val.is_empty() {
            reject_tmp_persistent_path(Path::new(&val), "config-toml")?;
            return Ok(Some(val));
        }
    }

    let default_path = Path::new("configs/cp.toml");
    if default_path.exists() {
        reject_tmp_persistent_path(default_path, "config-toml")?;
        return Ok(Some(default_path.to_string_lossy().to_string()));
    }

    Ok(None)
}

/// Resolve model cache budget in bytes (env overrides config).
pub(crate) fn resolve_model_cache_budget_bytes() -> Result<u64> {
    // 1) Environment override
    if let Ok(raw) = std::env::var("AOS_MODEL_CACHE_MAX_MB") {
        let parsed: u64 = raw.parse().map_err(|_| {
            AosError::Config(format!(
                "Invalid AOS_MODEL_CACHE_MAX_MB '{}': must be a positive integer (MB)",
                raw
            ))
        })?;
        if parsed == 0 {
            return Err(AosError::Config(
                "AOS_MODEL_CACHE_MAX_MB must be greater than zero".to_string(),
            ));
        }
        return Ok(parsed * 1024 * 1024);
    }

    // 2) Config file (env + TOML via ConfigLoader). Prefer explicit path override if provided.
    let toml_path = resolve_config_toml_path()?;

    let config = ConfigLoader::new()
        .load(Vec::new(), toml_path.clone())
        .map_err(|e| {
            let scope = toml_path
                .as_ref()
                .map(|p| format!(" from {}", p))
                .unwrap_or_default();
            AosError::Config(format!(
                "Failed to load configuration{} for model cache budget: {}",
                scope, e
            ))
        })?;

    if let Some(raw) = config.get("model.cache.max.mb") {
        let parsed: u64 = raw.parse().map_err(|_| {
            AosError::Config(format!(
                "Invalid model.cache.max.mb '{}': must be a positive integer (MB)",
                raw
            ))
        })?;
        if parsed == 0 {
            return Err(AosError::Config(
                "model.cache.max.mb must be greater than zero".to_string(),
            ));
        }
        return Ok(parsed * 1024 * 1024);
    }

    Err(AosError::Config(
        "Model cache budget not configured. Set AOS_MODEL_CACHE_MAX_MB or model.cache.max.mb in the config TOML (MB).".to_string(),
    ))
}

/// Behavior when base-model pinning encounters a pin-limit conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PinConflictMode {
    /// Keep serving by loading the incoming model unpinned.
    #[default]
    Shadow,
    /// Fail closed unless an existing pinned model can be deterministically displaced.
    Enforce,
}

impl PinConflictMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Shadow => "shadow",
            Self::Enforce => "enforce",
        }
    }
}

impl std::fmt::Display for PinConflictMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for PinConflictMode {
    type Err = String;

    fn from_str(raw: &str) -> std::result::Result<Self, Self::Err> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "shadow" => Ok(Self::Shadow),
            "enforce" => Ok(Self::Enforce),
            _ => Err("must be one of: shadow, enforce".to_string()),
        }
    }
}

/// Resolve base model pinning enabled flag (env overrides config).
pub fn resolve_base_model_pin_enabled() -> Result<bool> {
    if let Ok(raw) = std::env::var("AOS_PIN_BASE_MODEL") {
        let parsed = parse_bool(&raw).map_err(|e| {
            AosError::Config(format!("Invalid AOS_PIN_BASE_MODEL '{}': {}", raw, e))
        })?;
        return Ok(parsed);
    }

    let toml_path = resolve_config_toml_path()?;
    let config = ConfigLoader::new()
        .load(Vec::new(), toml_path.clone())
        .map_err(|e| {
            let scope = toml_path
                .as_ref()
                .map(|p| format!(" from {}", p))
                .unwrap_or_default();
            AosError::Config(format!(
                "Failed to load configuration{} for base model pinning: {}",
                scope, e
            ))
        })?;

    if let Some(raw) = config.get("model.cache.pin_base_model") {
        let parsed = parse_bool(raw).map_err(|e| {
            AosError::Config(format!(
                "Invalid model.cache.pin_base_model '{}': {}",
                raw, e
            ))
        })?;
        return Ok(parsed);
    }

    Ok(false)
}

/// Resolve base model pin budget in bytes (env overrides config).
pub fn resolve_base_model_pin_budget_bytes() -> Result<Option<u64>> {
    if let Ok(raw) = std::env::var("AOS_PIN_BUDGET_BYTES") {
        let parsed = parse_byte_size(&raw).map_err(|e| {
            AosError::Config(format!("Invalid AOS_PIN_BUDGET_BYTES '{}': {}", raw, e))
        })?;
        if parsed == 0 {
            return Err(AosError::Config(
                "AOS_PIN_BUDGET_BYTES must be greater than zero".to_string(),
            ));
        }
        return Ok(Some(parsed));
    }

    let toml_path = resolve_config_toml_path()?;
    let config = ConfigLoader::new()
        .load(Vec::new(), toml_path.clone())
        .map_err(|e| {
            let scope = toml_path
                .as_ref()
                .map(|p| format!(" from {}", p))
                .unwrap_or_default();
            AosError::Config(format!(
                "Failed to load configuration{} for base model pin budget: {}",
                scope, e
            ))
        })?;

    if let Some(raw) = config.get("model.cache.pin_budget_bytes") {
        let parsed = parse_byte_size(raw).map_err(|e| {
            AosError::Config(format!(
                "Invalid model.cache.pin_budget_bytes '{}': {}",
                raw, e
            ))
        })?;
        if parsed == 0 {
            return Err(AosError::Config(
                "model.cache.pin_budget_bytes must be greater than zero".to_string(),
            ));
        }
        return Ok(Some(parsed));
    }

    Ok(None)
}

/// Resolve base model pin conflict mode (env overrides config, defaults to shadow).
pub fn resolve_base_model_pin_conflict_mode() -> Result<PinConflictMode> {
    if let Ok(raw) = std::env::var("AOS_BASE_MODEL_PIN_CONFLICT_MODE") {
        let parsed = PinConflictMode::from_str(&raw).map_err(|e| {
            AosError::Config(format!(
                "Invalid AOS_BASE_MODEL_PIN_CONFLICT_MODE '{}': {}",
                raw, e
            ))
        })?;
        return Ok(parsed);
    }

    let toml_path = resolve_config_toml_path()?;
    let config = ConfigLoader::new()
        .load(Vec::new(), toml_path.clone())
        .map_err(|e| {
            let scope = toml_path
                .as_ref()
                .map(|p| format!(" from {}", p))
                .unwrap_or_default();
            AosError::Config(format!(
                "Failed to load configuration{} for base model pin conflict mode: {}",
                scope, e
            ))
        })?;

    if let Some(raw) = config.get("model.cache.pin_conflict_mode") {
        let parsed = PinConflictMode::from_str(raw).map_err(|e| {
            AosError::Config(format!(
                "Invalid model.cache.pin_conflict_mode '{}': {}",
                raw, e
            ))
        })?;
        return Ok(parsed);
    }

    Ok(PinConflictMode::Shadow)
}

/// Load and validate model configuration from config.json
///
/// Load and validate model config - returns error on GQA validation failure
///
/// This helper function:
/// 1. Loads the config.json from the model path
/// 2. Validates GQA configuration (attention heads divisible by KV heads)
/// 3. Returns FATAL error on validation failures (BREAKING CHANGE from prior behavior)
///
/// Returns `Ok(Some(config))` if loading and validation succeed.
/// Returns `Ok(None)` if config.json is missing (backend will use defaults).
/// Returns `Err(...)` if validation fails (GQA misconfiguration is FATAL).
#[cfg(any(target_os = "macos", feature = "multi-backend"))]
pub(crate) fn load_and_validate_model_config(model_path: &Path) -> Result<Option<ModelConfig>> {
    match ModelConfig::from_config_json(model_path) {
        Ok(config) => {
            // Validate GQA configuration - this is now FATAL on failure
            config.validate().map_err(|e| {
                error!(
                    model_path = %model_path.display(),
                    error = %e,
                    "Model configuration validation FAILED - cannot proceed with invalid config"
                );
                AosError::Config(format!(
                    "Model config validation failed for '{}': {}",
                    model_path.display(),
                    e
                ))
            })?;
            Ok(Some(config))
        }
        Err(e) => {
            warn!(
                model_path = %model_path.display(),
                error = %e,
                "Failed to load model config.json - backend may use default parameters"
            );
            Ok(None)
        }
    }
}

#[cfg(any(target_os = "macos", feature = "multi-backend"))]
pub(crate) fn build_model_cache_identity(
    backend_type: BackendType,
    model_path: &Path,
) -> ModelCacheIdentity {
    let kernel_version_id = adapteros_core::version::VERSION.to_string();
    let quantization_mode = detect_quantization_mode(model_path).unwrap_or_else(|| {
        crate::model_key::quantization_tag_for_backend(backend_type).to_string()
    });
    let fusion_mode =
        fusion_interval_mode_from_env().unwrap_or_else(crate::model_key::default_fusion_tag);

    ModelCacheIdentity {
        kernel_version_id,
        quantization_mode,
        fusion_mode,
        build_id: Some(adapteros_core::version::VERSION.to_string()),
        adapter_dir_hash: None,
    }
}

#[cfg(any(target_os = "macos", feature = "multi-backend"))]
pub(crate) fn fusion_interval_mode_from_env() -> Option<String> {
    let raw = std::env::var("AOS_FUSION_INTERVAL_MODE")
        .or_else(|_| std::env::var("AOS_FUSION_MODE"))
        .ok()?;

    let normalized = raw.to_lowercase();
    if normalized == "per_request" {
        Some("per_request".to_string())
    } else if normalized == "per_token" {
        Some("per_token".to_string())
    } else if let Some(rest) = normalized.strip_prefix("per_segment:") {
        Some(format!("per_segment-{}", rest))
    } else {
        warn!(
            mode = %raw,
            "Unrecognized fusion interval mode; falling back to default"
        );
        None
    }
}

#[cfg(any(target_os = "macos", feature = "multi-backend"))]
pub(crate) fn detect_quantization_mode(model_path: &Path) -> Option<String> {
    let config_path = if model_path.is_dir() {
        model_path.join("config.json")
    } else {
        model_path.to_path_buf()
    };

    let contents = match std::fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(e) => {
            // Log instead of silently ignoring - this helps diagnose model loading issues
            debug!(
                config_path = %config_path.display(),
                error = %e,
                "Could not read config.json for quantization detection"
            );
            return None;
        }
    };

    let json: Value = match serde_json::from_str(&contents) {
        Ok(j) => j,
        Err(e) => {
            // Log parse errors - malformed config.json is a significant issue
            warn!(
                config_path = %config_path.display(),
                error = %e,
                "Failed to parse config.json - model may have invalid configuration"
            );
            return None;
        }
    };

    if let Some(qcfg) = json.get("quantization_config") {
        let digest = B3Hash::hash(serde_json::to_vec(qcfg).unwrap_or_default().as_slice());
        debug!(
            config_path = %config_path.display(),
            quantization_hash = %&digest.to_hex()[..12],
            "Detected quantization_config in model"
        );
        return Some(format!("quant_cfg-{}", &digest.to_hex()[..12]));
    }

    if let Some(q) = json.get("quantization") {
        if let Some(as_str) = q.as_str() {
            debug!(
                config_path = %config_path.display(),
                quantization = %as_str,
                "Detected quantization string in model"
            );
            return Some(format!("quantization-{as_str}"));
        }
        let digest = B3Hash::hash(serde_json::to_vec(q).unwrap_or_default().as_slice());
        debug!(
            config_path = %config_path.display(),
            quantization_hash = %&digest.to_hex()[..12],
            "Detected quantization object in model"
        );
        return Some(format!("quantization-{}", &digest.to_hex()[..12]));
    }

    debug!(
        config_path = %config_path.display(),
        "No quantization config found in model config.json"
    );
    None
}
