use adapteros_verify::StrictnessLevel;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tokio::fs as tokio_fs;
use tracing::{debug, info, warn};
use hex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub db: DatabaseConfig,
    pub security: SecurityConfig,
    pub paths: PathsConfig,
    pub rate_limits: RateLimitsConfig,
    pub metrics: MetricsConfig,
    pub alerting: AlertingConfig,
    #[serde(default)]
    pub git: Option<adapteros_git::GitConfig>,
    #[serde(default)]
    pub policies: PoliciesConfig,
    #[serde(default)]
    pub orchestrator: OrchestratorConfig,
    #[serde(default)]
    pub cab: Option<CabConfig>,
    #[serde(default)]
    pub mlx: Option<MlxConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub port: u16,
    #[serde(default = "default_bind")]
    pub bind: String,
    /// Optional Unix Domain Socket path for UDS-only serving (M1+). If set, TCP is disabled.
    #[serde(default)]
    pub uds_socket: Option<String>,
    /// Production mode: enforces UDS-only, Ed25519 JWTs, zero egress (M1 hardening)
    #[serde(default = "default_false")]
    pub production_mode: bool,
    /// Enable memory-mapped adapter loading
    #[serde(default = "default_false")]
    pub enable_mmap_adapters: bool,
    /// Maximum cache size for memory-mapped adapters (MB)
    #[serde(default = "default_mmap_cache_size")]
    pub mmap_cache_size_mb: usize,
    /// Enable hot-swap capabilities
    #[serde(default = "default_false")]
    pub enable_hot_swap: bool,
    /// Maximum adapter file size in bytes (default: 500MB)
    #[serde(default = "default_max_adapter_size")]
    pub max_adapter_size_bytes: u64,
    /// Maximum model file size in bytes (default: 10GB)
    #[serde(default = "default_max_model_size")]
    pub max_model_size_bytes: u64,
}

fn default_bind() -> String {
    "127.0.0.1".to_string()
}

fn default_mmap_cache_size() -> usize {
    512
}

fn default_max_adapter_size() -> u64 {
    500 * 1024 * 1024 // 500MB
}

fn default_max_config_size() -> u64 {
    1024 * 1024 // 1MB
}

fn default_max_tokenizer_size() -> u64 {
    10 * 1024 * 1024 // 10MB
}

fn default_max_model_size() -> u64 {
    10 * 1024 * 1024 * 1024 // 10GB
}

fn default_streaming_header_size() -> usize {
    1024 * 1024 // 1MB
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(default = "default_true")]
    pub require_pf_deny: bool,
    #[serde(default = "default_false")]
    pub mtls_required: bool,
    pub jwt_secret: String,
    /// Optional path to a file containing the HS256 secret. If set, takes precedence over jwt_secret.
    #[serde(default)]
    pub jwt_secret_file: Option<String>,
    /// JWT mode: "hmac" (default) or "eddsa". In production_mode, must be "eddsa".
    #[serde(default)]
    pub jwt_mode: Option<String>,
    /// Optional Ed25519 public key in PEM for JWT validation when jwt_mode = "eddsa"
    #[serde(default)]
    pub jwt_public_key_pem: Option<String>,
    /// Optional path to PEM file holding the Ed25519 public key when jwt_mode = "eddsa"
    #[serde(default)]
    pub jwt_public_key_pem_file: Option<String>,
    /// Optional path to a 32-byte hex-encoded Ed25519 signing key for JWT issuance
    #[serde(default)]
    pub jwt_signing_key_path: Option<String>,
    /// The global seed for the deterministic executor (32-byte hex string)
    pub global_seed: String,
    /// Maximum config.json file size in bytes (default: 1MB)
    #[serde(default = "default_max_config_size")]
    pub max_config_size_bytes: u64,
    /// Maximum tokenizer.json file size in bytes (default: 10MB)
    #[serde(default = "default_max_tokenizer_size")]
    pub max_tokenizer_size_bytes: u64,
    /// Maximum model weights file size in bytes (default: 10GB)
    #[serde(default = "default_max_model_size")]
    pub max_model_size_bytes: u64,
    /// Enable streaming validation for large files
    #[serde(default = "default_true")]
    pub enable_streaming_validation: bool,
    /// Maximum header size to read for streaming validation (default: 1MB)
    #[serde(default = "default_streaming_header_size")]
    pub streaming_header_size_bytes: usize,
    /// Per-tenant file size limits (tenant_id -> max_bytes)
    #[serde(default)]
    pub per_tenant_limits: std::collections::HashMap<String, u64>,
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathsConfig {
    pub artifacts_root: String,
    pub bundles_root: String,
    pub adapters_root: String,
    #[serde(default = "default_plan_dir")]
    pub plan_dir: String,
}

fn default_plan_dir() -> String {
    "plan".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitsConfig {
    pub requests_per_minute: u32,
    pub burst_size: u32,
    pub inference_per_minute: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub bearer_token: String,
    pub include_histogram: bool,
    pub histogram_buckets: Vec<f64>,
    #[serde(default = "default_system_metrics_interval_secs")]
    pub system_metrics_interval_secs: u64,
    #[serde(default = "default_metrics_server_port")]
    pub server_port: u16,
    #[serde(default = "default_metrics_server_enabled")]
    pub server_enabled: bool,
}

fn default_system_metrics_interval_secs() -> u64 {
    30
}

fn default_metrics_server_port() -> u16 {
    9090
}

fn default_metrics_server_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertingConfig {
    pub enabled: bool,
    pub alert_dir: String,
    pub max_alerts_per_file: usize,
    pub rotate_size_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PoliciesConfig {
    #[serde(default)]
    pub drift: adapteros_core::DriftPolicy,
}

/// Telemetry retention configuration for bundle GC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryRetentionConfig {
    /// Keep last K bundles per CPID
    #[serde(default = "TelemetryRetentionConfig::default_keep")]
    pub keep_bundles_per_cpid: usize,
    /// Keep incident bundles from GC
    #[serde(default = "default_true")]
    pub keep_incident_bundles: bool,
    /// Keep promotion bundles from GC
    #[serde(default = "default_true")]
    pub keep_promotion_bundles: bool,
}

impl TelemetryRetentionConfig {
    fn default_keep() -> usize {
        12
    }
}

impl Default for TelemetryRetentionConfig {
    fn default() -> Self {
        Self {
            keep_bundles_per_cpid: Self::default_keep(),
            keep_incident_bundles: true,
            keep_promotion_bundles: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorConfig {
    #[serde(default = "default_ephemeral_ttl_hours")]
    pub ephemeral_adapter_ttl_hours: u64,
    #[serde(default = "default_base_model")]
    pub base_model: String,
}

fn default_base_model() -> String {
    "qwen2.5-7b".to_string()
}

fn default_ephemeral_ttl_hours() -> u64 {
    24
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            ephemeral_adapter_ttl_hours: default_ephemeral_ttl_hours(),
            base_model: default_base_model(),
        }
    }
}

impl Config {
    pub fn load(path: &str) -> Result<Self> {
        let contents = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&contents)?;
        Ok(config)
    }

    /// Canonicalize a path, handling relative paths relative to current working directory
    fn canonicalize_path(path: &str, description: &str) -> Result<String> {
        let path_buf = PathBuf::from(path);
        let canonical = if path_buf.is_absolute() {
            path_buf.canonicalize()
        } else {
            // For relative paths, resolve relative to current working directory
            std::env::current_dir()
                .map_err(|e| anyhow::anyhow!("Failed to get current directory: {}", e))?
                .join(path_buf)
                .canonicalize()
        }
        .with_context(|| format!("Failed to canonicalize {} path: {}", description, path))?;

        canonical
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("{} path contains invalid UTF-8: {}", description, path))
            .map(|s| s.to_string())
    }

    /// Validate and canonicalize a directory path, creating it if it doesn't exist
    fn validate_and_canonicalize_dir(path: &str, description: &str) -> Result<String> {
        let path_buf = PathBuf::from(path);
        let full_path = if path_buf.is_absolute() {
            path_buf
        } else {
            std::env::current_dir()
                .map_err(|e| anyhow::anyhow!("Failed to get current directory: {}", e))?
                .join(path_buf)
        };

        // Create directory if it doesn't exist
        if !full_path.exists() {
            fs::create_dir_all(&full_path).with_context(|| {
                format!(
                    "{} path {} does not exist and cannot be created",
                    description, path
                )
            })?;
        }

        // Verify it's a directory
        if !full_path.is_dir() {
            return Err(anyhow::anyhow!(
                "{} path {} is not a directory",
                description,
                full_path.display()
            ));
        }

        // Canonicalize
        let canonical = full_path
            .canonicalize()
            .with_context(|| format!("Failed to canonicalize {} path: {}", description, path))?;

        // Check readability
        fs::metadata(&canonical).with_context(|| {
            format!("{} path {} is not accessible", description, canonical.display())
        })?;

        canonical
            .to_str()
            .ok_or_else(|| {
                anyhow::anyhow!("{} path contains invalid UTF-8: {}", description, path)
            })
            .map(|s| s.to_string())
    }

    /// Validate and canonicalize a file path
    fn validate_and_canonicalize_file(path: &str, description: &str) -> Result<String> {
        let path_buf = PathBuf::from(path);
        let full_path = if path_buf.is_absolute() {
            path_buf
        } else {
            std::env::current_dir()
                .map_err(|e| anyhow::anyhow!("Failed to get current directory: {}", e))?
                .join(path_buf)
        };

        if !full_path.exists() {
            return Err(anyhow::anyhow!(
                "{} file {} does not exist",
                description,
                full_path.display()
            ));
        }

        if !full_path.is_file() {
            return Err(anyhow::anyhow!(
                "{} path {} is not a file",
                description,
                full_path.display()
            ));
        }

        let canonical = full_path
            .canonicalize()
            .with_context(|| format!("Failed to canonicalize {} file: {}", description, path))?;

        // Check readability
        fs::metadata(&canonical).with_context(|| {
            format!("{} file {} is not accessible", description, canonical.display())
        })?;

        canonical
            .to_str()
            .ok_or_else(|| {
                anyhow::anyhow!("{} file contains invalid UTF-8: {}", description, path)
            })
            .map(|s| s.to_string())
    }

    /// Validate configuration values for security and operational correctness.
    ///
    /// This method performs comprehensive startup validation:
    /// - Validates and canonicalizes all directory paths (relative paths resolved to absolute)
    /// - Validates and canonicalizes all file paths
    /// - Validates MLX model paths when MLX backend is enabled
    /// - Validates required environment variables (e.g., AOS_MLX_FFI_MODEL when MLX enabled)
    /// - Validates model files exist (config.json, tokenizer.json)
    /// - Ensures all paths are accessible and properly formatted
    ///
    /// Paths are canonicalized in-place to ensure consistent resolution regardless of
    /// working directory changes. This prevents runtime failures due to relative path issues.
    pub fn validate(&mut self) -> Result<()> {

        // Validate file size limits
        if self.server.max_adapter_size_bytes == 0 {
            return Err(anyhow::anyhow!(
                "max_adapter_size_bytes must be greater than 0"
            ));
        }
        if self.server.max_adapter_size_bytes > 10 * 1024 * 1024 * 1024 {
            // 10GB max
            return Err(anyhow::anyhow!(
                "max_adapter_size_bytes {} exceeds maximum 10GB",
                self.server.max_adapter_size_bytes
            ));
        }
        if self.server.max_model_size_bytes == 0 {
            return Err(anyhow::anyhow!(
                "max_model_size_bytes must be greater than 0"
            ));
        }
        if self.server.max_model_size_bytes > 100 * 1024 * 1024 * 1024 {
            // 100GB max
            return Err(anyhow::anyhow!(
                "max_model_size_bytes {} exceeds maximum 100GB",
                self.server.max_model_size_bytes
            ));
        }

        // Validate and canonicalize directory paths
        self.paths.adapters_root =
            Self::validate_and_canonicalize_dir(&self.paths.adapters_root, "adapters_root")?;
        self.paths.artifacts_root =
            Self::validate_and_canonicalize_dir(&self.paths.artifacts_root, "artifacts_root")?;
        self.paths.bundles_root =
            Self::validate_and_canonicalize_dir(&self.paths.bundles_root, "bundles_root")?;
        self.paths.plan_dir =
            Self::validate_and_canonicalize_dir(&self.paths.plan_dir, "plan_dir")?;

        // Validate alert_dir
        self.alerting.alert_dir =
            Self::validate_and_canonicalize_dir(&self.alerting.alert_dir, "alert_dir")?;

        // Validate database path parent directory exists and canonicalize
        let db_path = PathBuf::from(&self.db.path);
        let db_full_path = if db_path.is_absolute() {
            db_path.clone()
        } else {
            std::env::current_dir()
                .map_err(|e| anyhow::anyhow!("Failed to get current directory: {}", e))?
                .join(db_path)
        };

        if let Some(parent) = db_full_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).with_context(|| {
                    format!(
                        "Database path parent directory {} does not exist and cannot be created",
                        parent.display()
                    )
                })?;
            }
            // Canonicalize parent and reconstruct path
            let canonical_parent = parent
                .canonicalize()
                .with_context(|| {
                    format!(
                        "Failed to canonicalize database parent directory: {}",
                        parent.display()
                    )
                })?;
            let db_file_name = db_full_path
                .file_name()
                .ok_or_else(|| anyhow::anyhow!("Database path has no filename"))?;
            self.db.path = canonical_parent
                .join(db_file_name)
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Database path contains invalid UTF-8"))?
                .to_string();
        } else {
            // Root path, canonicalize directly
            self.db.path = Self::canonicalize_path(&self.db.path, "database")?;
        }

        // Validate security settings
        if self.security.jwt_secret.len() < 32 {
            return Err(anyhow::anyhow!(
                "jwt_secret must be at least 32 characters long"
            ));
        }

        // Validate global_seed format (must be 64 hex characters = 32 bytes)
        if self.security.global_seed.len() != 64 {
            return Err(anyhow::anyhow!(
                "global_seed must be a 64-character hex string (32 bytes), got {} characters",
                self.security.global_seed.len()
            ));
        }
        if !self.security.global_seed.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(anyhow::anyhow!(
                "global_seed must contain only hexadecimal characters"
            ));
        }

        // Validate Ed25519 keys if jwt_mode is eddsa
        if self.security.jwt_mode.as_deref() == Some("eddsa") {
            if self.security.jwt_public_key_pem.is_none()
                && self.security.jwt_public_key_pem_file.is_none()
            {
                return Err(anyhow::anyhow!(
                    "jwt_mode='eddsa' requires jwt_public_key_pem or jwt_public_key_pem_file"
                ));
            }
        }

        // Validate and canonicalize file paths
        if let Some(ref jwt_secret_file) = self.security.jwt_secret_file {
            self.security.jwt_secret_file = Some(
                Self::validate_and_canonicalize_file(jwt_secret_file, "jwt_secret_file")?,
            );
        }

        if let Some(ref jwt_public_key_pem_file) = self.security.jwt_public_key_pem_file {
            self.security.jwt_public_key_pem_file = Some(
                Self::validate_and_canonicalize_file(
                    jwt_public_key_pem_file,
                    "jwt_public_key_pem_file",
                )?,
            );
        }

        if let Some(ref jwt_signing_key_path) = self.security.jwt_signing_key_path {
            self.security.jwt_signing_key_path = Some(
                Self::validate_and_canonicalize_file(jwt_signing_key_path, "jwt_signing_key_path")?,
            );
        }

        // Validate MLX model path if MLX is enabled
        if let Some(ref mut mlx_config) = self.mlx {
            if mlx_config.enabled {
                // Check if model path is set in config or environment
                let model_path = if let Ok(env_path) = std::env::var("AOS_MLX_FFI_MODEL") {
                    // Environment variable takes precedence - validate it exists
                    let model_path_buf = PathBuf::from(&env_path);
                    let full_model_path = if model_path_buf.is_absolute() {
                        model_path_buf
                    } else {
                        std::env::current_dir()
                            .map_err(|e| {
                                anyhow::anyhow!("Failed to get current directory: {}", e)
                            })?
                            .join(model_path_buf)
                    };

                    if !full_model_path.exists() {
                        return Err(anyhow::anyhow!(
                            "MLX model path from AOS_MLX_FFI_MODEL does not exist: {} (resolved: {})",
                            env_path,
                            full_model_path.display()
                        ));
                    }

                    if !full_model_path.is_dir() {
                        return Err(anyhow::anyhow!(
                            "MLX model path from AOS_MLX_FFI_MODEL is not a directory: {}",
                            full_model_path.display()
                        ));
                    }

                    // Validate required model files exist
                    let config_json = full_model_path.join("config.json");
                    let tokenizer_json = full_model_path.join("tokenizer.json");
                    let weights_safetensors = full_model_path.join("weights.safetensors");
                    let model_safetensors = full_model_path.join("model.safetensors");

                    if !config_json.exists() {
                        return Err(anyhow::anyhow!(
                            "MLX model config.json not found at: {}",
                            config_json.display()
                        ));
                    }

                    if !tokenizer_json.exists() {
                        return Err(anyhow::anyhow!(
                            "MLX model tokenizer.json not found at: {}",
                            tokenizer_json.display()
                        ));
                    }

                    // Validate weights file (either weights.safetensors or model.safetensors)
                    let has_weights = weights_safetensors.exists() && weights_safetensors.is_file();
                    let has_model = model_safetensors.exists() && model_safetensors.is_file();

                    if !has_weights && !has_model {
                        return Err(anyhow::anyhow!(
                            "MLX model weights file not found at: {}. Expected either 'weights.safetensors' or 'model.safetensors'",
                            full_model_path.display()
                        ));
                    }

                    Some(env_path)
                } else if let Some(ref config_path) = mlx_config.model_path {
                    // Validate and canonicalize config path
                    let model_path_buf = PathBuf::from(config_path);
                    let full_model_path = if model_path_buf.is_absolute() {
                        model_path_buf
                    } else {
                        std::env::current_dir()
                            .map_err(|e| {
                                anyhow::anyhow!("Failed to get current directory: {}", e)
                            })?
                            .join(model_path_buf)
                    };

                    if !full_model_path.exists() {
                        return Err(anyhow::anyhow!(
                            "MLX model path does not exist: {} (resolved: {})",
                            config_path,
                            full_model_path.display()
                        ));
                    }

                    if !full_model_path.is_dir() {
                        return Err(anyhow::anyhow!(
                            "MLX model path is not a directory: {}",
                            full_model_path.display()
                        ));
                    }

                    // Validate required model files exist
                    let config_json = full_model_path.join("config.json");
                    let tokenizer_json = full_model_path.join("tokenizer.json");
                    let weights_safetensors = full_model_path.join("weights.safetensors");
                    let model_safetensors = full_model_path.join("model.safetensors");

                    if !config_json.exists() {
                        return Err(anyhow::anyhow!(
                            "MLX model config.json not found at: {}",
                            config_json.display()
                        ));
                    }

                    if !tokenizer_json.exists() {
                        return Err(anyhow::anyhow!(
                            "MLX model tokenizer.json not found at: {}",
                            tokenizer_json.display()
                        ));
                    }

                    // Validate weights file (either weights.safetensors or model.safetensors)
                    let has_weights = weights_safetensors.exists() && weights_safetensors.is_file();
                    let has_model = model_safetensors.exists() && model_safetensors.is_file();

                    if !has_weights && !has_model {
                        return Err(anyhow::anyhow!(
                            "MLX model weights file not found at: {}. Expected either 'weights.safetensors' or 'model.safetensors'",
                            full_model_path.display()
                        ));
                    }

                    // Canonicalize and update model path in config
                    let canonical_model_path = full_model_path
                        .canonicalize()
                        .with_context(|| {
                            format!("Failed to canonicalize MLX model path: {}", config_path)
                        })?;
                    mlx_config.model_path = canonical_model_path
                        .to_str()
                        .map(|s| s.to_string());
                    mlx_config.model_path.clone()
                } else {
                    None
                };

                if model_path.is_none() {
                    return Err(anyhow::anyhow!(
                        "MLX backend is enabled but no model path is configured. \
                         Set mlx.model_path in config or AOS_MLX_FFI_MODEL environment variable"
                    ));
                }
            }
        }

        // Validate CAB Golden Gate bundle_path if present
        if let Some(ref cab_config) = self.cab {
            if let Some(ref golden_gate) = cab_config.golden_gate {
                if golden_gate.enabled {
                    if let Some(ref bundle_path) = golden_gate.bundle_path {
                        // Validate bundle path exists if explicitly set
                        Self::validate_and_canonicalize_file(bundle_path, "golden_gate.bundle_path")?;
                    }
                }
            }
        }

        // Validate UDS socket path if set
        if let Some(ref uds_socket) = self.server.uds_socket {
            let uds_path = PathBuf::from(uds_socket);
            let uds_full_path = if uds_path.is_absolute() {
                uds_path.clone()
            } else {
                std::env::current_dir()
                    .map_err(|e| anyhow::anyhow!("Failed to get current directory: {}", e))?
                    .join(uds_path)
            };

            // Ensure parent directory exists for UDS socket
            if let Some(parent) = uds_full_path.parent() {
                if !parent.exists() {
                    fs::create_dir_all(parent).with_context(|| {
                        format!(
                            "UDS socket parent directory {} does not exist and cannot be created",
                            parent.display()
                        )
                    })?;
                }
            }

            // Canonicalize UDS socket path (store parent + filename)
            if let Some(parent) = uds_full_path.parent() {
                let canonical_parent = parent
                    .canonicalize()
                    .with_context(|| {
                        format!(
                            "Failed to canonicalize UDS socket parent directory: {}",
                            parent.display()
                        )
                    })?;
                let socket_name = uds_full_path
                    .file_name()
                    .ok_or_else(|| anyhow::anyhow!("UDS socket path has no filename"))?;
                self.server.uds_socket = Some(
                    canonical_parent
                        .join(socket_name)
                        .to_str()
                        .ok_or_else(|| anyhow::anyhow!("UDS socket path contains invalid UTF-8"))?
                        .to_string(),
                );
            } else {
                // Root path, canonicalize directly
                self.server.uds_socket = Some(
                    Self::canonicalize_path(uds_socket, "uds_socket")?,
                );
            }
        }

        // Validate production mode requirements
        if self.server.production_mode {
            if self.server.uds_socket.is_none() {
                return Err(anyhow::anyhow!(
                    "production_mode requires uds_socket to be configured"
                ));
            }
            if self.security.jwt_mode.as_deref() != Some("eddsa") {
                return Err(anyhow::anyhow!(
                    "production_mode requires jwt_mode='eddsa'"
                ));
            }
            if !self.security.require_pf_deny {
                return Err(anyhow::anyhow!(
                    "production_mode requires require_pf_deny=true"
                ));
            }
        }

        Ok(())
    }

    /// Comprehensive startup validation that checks paths, permissions, and connections
    /// This is called after basic config validation during server startup
    pub async fn validate_startup_requirements(&self) -> Result<()> {
        info!("Performing comprehensive startup validation...");

        // Validate all configured directory paths exist and are accessible
        self.validate_directory_access().await?;

        // Validate file permissions for critical paths
        self.validate_file_permissions().await?;

        // Test database connectivity
        self.validate_database_connection().await?;

        // Validate security requirements
        self.validate_security_setup()?;

        // Validate external service connections if configured
        self.validate_external_connections().await?;

        info!("✅ Startup validation completed successfully");
        Ok(())
    }

    /// Validate that all configured directories exist and are accessible
    async fn validate_directory_access(&self) -> Result<()> {
        let dirs_to_check = vec![
            (&self.paths.adapters_root, "adapters_root"),
            (&self.paths.artifacts_root, "artifacts_root"),
            (&self.paths.bundles_root, "bundles_root"),
            (&self.paths.plan_dir, "plan_dir"),
            (&self.alerting.alert_dir, "alert_dir"),
        ];

        for (dir_path, dir_name) in dirs_to_check {
            let path = PathBuf::from(dir_path);

            // Check if directory exists
            if !path.exists() {
                return Err(anyhow::anyhow!(
                    "Required directory '{}' does not exist: {}",
                    dir_name,
                    path.display()
                ));
            }

            // Check if it's actually a directory
            if !path.is_dir() {
                return Err(anyhow::anyhow!(
                    "Path '{}' is not a directory: {}",
                    dir_name,
                    path.display()
                ));
            }

            // Test read/write access by attempting to create a temporary file
            let test_file = path.join(".aos_startup_test");
            match tokio::fs::write(&test_file, b"test").await {
                Ok(_) => {
                    // Clean up test file
                    let _ = tokio::fs::remove_file(&test_file).await;
                    debug!("✅ Directory '{}' is writable: {}", dir_name, path.display());
                }
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "Directory '{}' is not writable: {} (error: {})",
                        dir_name,
                        path.display(),
                        e
                    ));
                }
            }
        }

        Ok(())
    }

    /// Validate file permissions for security-critical files
    async fn validate_file_permissions(&self) -> Result<()> {
        // Check JWT secret file permissions if configured
        if let Some(ref jwt_secret_file) = self.security.jwt_secret_file {
            let path = PathBuf::from(jwt_secret_file);
            if path.exists() {
                self.validate_secure_file_permissions(&path, "jwt_secret_file").await?;
            }
        }

        // Check JWT signing key permissions if configured
        if let Some(ref jwt_signing_key) = self.security.jwt_signing_key_path {
            let path = PathBuf::from(jwt_signing_key);
            if path.exists() {
                self.validate_secure_file_permissions(&path, "jwt_signing_key_path").await?;
            }
        }

        Ok(())
    }

    /// Validate that a security-critical file has appropriate permissions
    async fn validate_secure_file_permissions(&self, path: &PathBuf, file_name: &str) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;

        let metadata = tokio::fs::metadata(path).await
            .with_context(|| format!("Failed to read metadata for {}", file_name))?;

        let permissions = metadata.permissions();
        let mode = permissions.mode();

        // Check if file is readable by others (should be 0600 or similar)
        let others_read = mode & 0o004 != 0;
        let others_write = mode & 0o002 != 0;
        let group_read = mode & 0o040 != 0;
        let group_write = mode & 0o020 != 0;

        if others_read || others_write || group_read || group_write {
            warn!(
                "⚠️  Security file '{}' has overly permissive permissions (0{:o}). \
                 Consider restricting to owner-only access (0600)",
                file_name,
                mode & 0o777
            );
        }

        Ok(())
    }

    /// Test database connectivity
    async fn validate_database_connection(&self) -> Result<()> {
        info!("Testing database connectivity...");

        // Create a temporary database connection to test
        let db = Database::new(&self.db.path).await
            .with_context(|| format!("Failed to connect to database at {}", self.db.path))?;

        // Test a simple query
        let result: (i64,) = sqlx::query_as("SELECT 1")
            .fetch_one(db.pool())
            .await
            .context("Failed to execute test query on database")?;

        if result.0 != 1 {
            return Err(anyhow::anyhow!("Database test query returned unexpected result"));
        }

        info!("✅ Database connectivity verified");
        Ok(())
    }

    /// Validate security setup requirements
    fn validate_security_setup(&self) -> Result<()> {
        // Validate JWT secret strength
        if self.security.jwt_secret.len() < 32 {
            return Err(anyhow::anyhow!(
                "JWT secret is too weak: {} characters (minimum 32 required)",
                self.security.jwt_secret.len()
            ));
        }

        // Check for weak JWT secrets (common patterns)
        let secret = &self.security.jwt_secret;
        if secret.chars().all(|c| c.is_ascii_digit()) {
            warn!("⚠️  JWT secret contains only digits - consider using a more complex secret");
        }
        if secret.chars().all(|c| c.is_ascii_alphabetic()) {
            warn!("⚠️  JWT secret contains only letters - consider using a more complex secret");
        }

        // Validate global seed format and entropy
        if self.security.global_seed.len() != 64 {
            return Err(anyhow::anyhow!(
                "Global seed must be 64 hex characters, got {}",
                self.security.global_seed.len()
            ));
        }

        // Check if global seed has good entropy (not all zeros, not sequential)
        let seed_bytes = hex::decode(&self.security.global_seed)
            .context("Invalid hex in global seed")?;

        if seed_bytes.iter().all(|&b| b == 0) {
            return Err(anyhow::anyhow!("Global seed cannot be all zeros"));
        }

        if seed_bytes.windows(2).all(|w| w[1] == w[0] + 1) {
            return Err(anyhow::anyhow!("Global seed cannot be sequential bytes"));
        }

        Ok(())
    }

    /// Validate connections to external services
    async fn validate_external_connections(&self) -> Result<()> {
        // Validate MLX model path if MLX is enabled
        if let Some(ref mlx_config) = self.mlx {
            if mlx_config.enabled {
                if let Ok(model_path) = std::env::var("AOS_MLX_FFI_MODEL") {
                    let path = PathBuf::from(&model_path);
                    if !path.exists() {
                        return Err(anyhow::anyhow!(
                            "MLX model path from environment does not exist: {}",
                            path.display()
                        ));
                    }
                    // Additional MLX-specific validation could go here
                }
            }
        }

        // Add validation for other external services as they are added

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CabConfig {
    #[serde(default)]
    pub golden_gate: Option<GoldenGateConfig>,
}

/// MLX Backend Configuration
/// MLX backend uses C++ FFI (no Python required)
/// Enable with: cargo build --features mlx-ffi-backend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MlxConfig {
    /// Enable MLX backend support (requires --features mlx-ffi-backend)
    #[serde(default = "default_false")]
    pub enabled: bool,
    /// Default model path (can be overridden by AOS_MLX_FFI_MODEL env var)
    #[serde(default)]
    pub model_path: Option<String>,
    /// Default backend selection when both Metal and MLX are available
    /// Options: "metal" (default, production) or "mlx" (development/experimentation)
    #[serde(default = "default_mlx_backend")]
    pub default_backend: String,
    /// Enable lazy loading of models (load on first inference request instead of startup)
    #[serde(default = "default_false")]
    pub lazy_loading: bool,
    /// Maximum number of models to keep cached in memory
    #[serde(default = "default_model_cache_size")]
    pub max_cached_models: usize,
    /// Model cache eviction policy: "lru" (default), "lfu", or "ttl"
    #[serde(default = "default_cache_eviction_policy")]
    pub cache_eviction_policy: String,
}

fn default_mlx_backend() -> String {
    "metal".to_string()
}

fn default_model_cache_size() -> usize {
    3
}

fn default_cache_eviction_policy() -> String {
    "lru".to_string()
}

impl Default for MlxConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model_path: None,
            default_backend: default_mlx_backend(),
            lazy_loading: false,
            max_cached_models: default_model_cache_size(),
            cache_eviction_policy: default_cache_eviction_policy(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenGateConfig {
    pub enabled: bool,
    pub baseline: String,
    /// Verification strictness level
    pub strictness: StrictnessLevel,
    #[serde(default)]
    pub skip_toolchain: bool,
    #[serde(default)]
    pub skip_signature: bool,
    #[serde(default)]
    pub verify_device: bool,
    #[serde(default)]
    pub bundle_path: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_config_validation_with_valid_paths() {
        let temp_dir = tempdir().expect("Failed to create temp dir");
        let temp_path = temp_dir.path();

        // Create required directories
        fs::create_dir_all(temp_path.join("adapters")).expect("create adapters dir");
        fs::create_dir_all(temp_path.join("artifacts")).expect("create artifacts dir");
        fs::create_dir_all(temp_path.join("bundles")).expect("create bundles dir");
        fs::create_dir_all(temp_path.join("plan")).expect("create plan dir");
        fs::create_dir_all(temp_path.join("alerts")).expect("create alerts dir");

        // Create a config file
        let config_content = format!(r#"
[server]
port = 8080
bind = "127.0.0.1"

[db]
path = "{}/test.db"

[security]
jwt_secret = "test_secret_32_chars_long_enough_for_validation"
global_seed = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

[paths]
adapters_root = "{}/adapters"
artifacts_root = "{}/artifacts"
bundles_root = "{}/bundles"
plan_dir = "{}/plan"

[alerting]
enabled = true
alert_dir = "{}/alerts"
max_alerts_per_file = 1000
rotate_size_mb = 10
"#,
            temp_path.display(),
            temp_path.display(),
            temp_path.display(),
            temp_path.display(),
            temp_path.display()
        );

        let config_path = temp_path.join("test_config.toml");
        fs::write(&config_path, &config_content).expect("write config file");

        // Test loading and validation
        let mut config = Config::load(config_path.to_str().unwrap()).expect("load config");

        // Validation should succeed
        config.validate().expect("config validation should pass");

        // Check that paths were canonicalized
        assert!(config.paths.adapters_root.starts_with('/'), "adapters_root should be absolute");
        assert!(config.paths.artifacts_root.starts_with('/'), "artifacts_root should be absolute");
        assert!(config.paths.bundles_root.starts_with('/'), "bundles_root should be absolute");
        assert!(config.paths.plan_dir.starts_with('/'), "plan_dir should be absolute");
        assert!(config.alerting.alert_dir.starts_with('/'), "alert_dir should be absolute");
    }

    #[test]
    fn test_config_validation_missing_directory() {
        let temp_dir = tempdir().expect("Failed to create temp dir");
        let temp_path = temp_dir.path();

        // Don't create the adapters directory

        let config_content = format!(r#"
[server]
port = 8080

[db]
path = "{}/test.db"

[security]
jwt_secret = "test_secret_32_chars_long_enough_for_validation"
global_seed = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

[paths]
adapters_root = "{}/missing_adapters"
artifacts_root = "{}/artifacts"
bundles_root = "{}/bundles"
plan_dir = "{}/plan"

[alerting]
enabled = true
alert_dir = "{}/alerts"
max_alerts_per_file = 1000
rotate_size_mb = 10
"#,
            temp_path.display(),
            temp_path.display(),
            temp_path.display(),
            temp_path.display(),
            temp_path.display(),
            temp_path.display()
        );

        let config_path = temp_path.join("test_config.toml");
        fs::write(&config_path, &config_content).expect("write config file");

        let mut config = Config::load(config_path.to_str().unwrap()).expect("load config");

        // Validation should fail because missing_adapters directory doesn't exist
        let result = config.validate();
        assert!(result.is_err(), "validation should fail for missing directory");
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("missing_adapters"), "error should mention missing directory");
    }

    #[test]
    fn test_config_validation_invalid_jwt_secret() {
        let temp_dir = tempdir().expect("Failed to create temp dir");
        let temp_path = temp_dir.path();

        // Create required directories
        fs::create_dir_all(temp_path.join("adapters")).expect("create adapters dir");

        let config_content = format!(r#"
[server]
port = 8080

[db]
path = "{}/test.db"

[security]
jwt_secret = "too_short"  # Less than 32 characters
global_seed = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

[paths]
adapters_root = "{}/adapters"
artifacts_root = "{}/artifacts"
bundles_root = "{}/bundles"
plan_dir = "{}/plan"

[alerting]
enabled = true
alert_dir = "{}/alerts"
max_alerts_per_file = 1000
rotate_size_mb = 10
"#,
            temp_path.display(),
            temp_path.display(),
            temp_path.display(),
            temp_path.display(),
            temp_path.display(),
            temp_path.display()
        );

        let config_path = temp_path.join("test_config.toml");
        fs::write(&config_path, &config_content).expect("write config file");

        let mut config = Config::load(config_path.to_str().unwrap()).expect("load config");

        // Validation should fail because JWT secret is too short
        let result = config.validate();
        assert!(result.is_err(), "validation should fail for short jwt_secret");
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("jwt_secret"), "error should mention jwt_secret");
    }
}
