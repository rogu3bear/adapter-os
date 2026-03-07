//! Effective configuration with type-safe sections
//!
//! This module provides a unified configuration facade that combines:
//! - TOML config file (cp.toml) as the manifest layer
//! - Environment variables (AOS_*) at higher precedence
//! - CLI arguments at highest precedence
//!
//! # Usage
//!
//! ```rust,ignore
//! use adapteros_config::{init_effective_config, effective_config};
//!
//! // Initialize at startup (call once)
//! init_effective_config("configs/cp.toml")?;
//!
//! // Access anywhere in the codebase
//! let cfg = effective_config();
//! println!("Server port: {}", cfg.server.port);
//! println!("Database: {}", cfg.database.path);
//! ```

use crate::guards::ConfigGuards;
use crate::precedence::DeterministicConfig;
use crate::schema::parse_bool;
use crate::CoreMLComputePreference;
use crate::{ConfigLoader, LoaderOptions};
use adapteros_core::defaults::{DEFAULT_SERVER_HOST, DEFAULT_SERVER_PORT};
use adapteros_core::{AosError, BackendKind, Result, SeedMode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::OnceLock;
use tracing::{debug, warn};

/// Global effective configuration instance
static EFFECTIVE_CONFIG: OnceLock<EffectiveConfig> = OnceLock::new();

/// Unified effective configuration with type-safe sections
///
/// This replaces both the old `adapteros-config::RuntimeConfig` and
/// `adapteros-server-api::Config` with a single source of truth.
#[derive(Debug, Clone)]
pub struct EffectiveConfig {
    /// The underlying deterministic config (for hashing, freeze checks)
    inner: DeterministicConfig,
    /// Server configuration
    pub server: ServerSection,
    /// Database configuration
    pub database: DatabaseSection,
    /// Security configuration
    pub security: SecuritySection,
    /// Path configuration
    pub paths: PathsSection,
    /// Logging configuration
    pub logging: LoggingSection,
    /// Rate limit configuration
    pub rate_limits: RateLimitsSection,
    /// Metrics configuration
    pub metrics: MetricsSection,
    /// Alerting configuration
    pub alerting: AlertingSection,
    /// Model configuration
    pub model: ModelSection,
    /// CoreML-specific configuration
    pub coreml: CoremlSection,
    /// Authentication configuration
    pub auth: AuthSection,
    /// Inference configuration
    pub inference: InferenceSection,
    /// Health configuration (adapter thresholds)
    pub health: HealthSection,
    /// Self-hosting agent configuration
    pub self_hosting: SelfHostingSection,
    /// Diagnostics configuration
    pub diagnostics: DiagnosticsSection,
    /// Uploads configuration
    pub uploads: UploadsSection,
    /// Circuit breaker configuration
    pub circuit_breaker: CircuitBreakerSection,
    /// Model Server configuration (shared model inference)
    pub model_server: ModelServerSection,
    /// Worker safety configuration (timeouts and resource limits)
    pub worker_safety: WorkerSafetySection,
    /// Instance identity (from manifest)
    pub instance: InstanceSection,
    /// Service toggles (from manifest)
    pub services: ServicesSection,
    /// Boot behavior (from manifest)
    pub boot: BootSection,
    /// Source tracking for each config key
    sources: HashMap<String, String>,
    /// Whether running in production mode
    is_production: bool,
}

/// Server section configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSection {
    /// Server port (1-65535, default: 18080)
    pub port: u16,
    /// Bind address (default: 127.0.0.1)
    pub host: String,
    /// Production mode flag
    pub production_mode: bool,
    /// Unix domain socket path (required in production)
    pub uds_socket: Option<PathBuf>,
    /// Drain timeout in seconds during shutdown (default: 30)
    pub drain_timeout_secs: u64,
    /// Boot timeout in seconds for the entire boot sequence (default: 300)
    pub boot_timeout_secs: u64,
    /// Number of worker threads (default: 4)
    pub workers: u16,
    /// Expected heartbeat interval for workers (seconds)
    pub worker_heartbeat_interval_secs: u64,
}

/// Database section configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseSection {
    /// Database connection URL (sqlite://path or postgres://...)
    pub url: String,
    /// Connection pool size (default: 10)
    pub pool_size: u32,
    /// Connection timeout in seconds (default: 30)
    pub timeout_secs: u64,
}

/// Security section configuration
#[derive(Clone, Serialize, Deserialize)]
pub struct SecuritySection {
    /// JWT signing secret (sensitive, redacted in logs)
    pub jwt_secret: String,
    /// JWT signing mode: "eddsa" or "hmac"
    pub jwt_mode: String,
    /// JWT TTL in hours (default: 8)
    pub jwt_ttl_hours: u32,
    /// Require packet filter deny rules
    pub require_pf_deny: bool,
    /// Dev login bypass enabled (should be false in production)
    pub dev_login_enabled: bool,
    /// Dev bypass: skip all authentication (debug builds only)
    pub dev_bypass: bool,
    /// Ed25519 signing key for manifests (sensitive)
    pub signing_key: Option<String>,
}

impl std::fmt::Debug for SecuritySection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecuritySection")
            .field("jwt_secret", &"[REDACTED]")
            .field("jwt_mode", &self.jwt_mode)
            .field("jwt_ttl_hours", &self.jwt_ttl_hours)
            .field("require_pf_deny", &self.require_pf_deny)
            .field("dev_login_enabled", &self.dev_login_enabled)
            .field("dev_bypass", &self.dev_bypass)
            .field(
                "signing_key",
                &self.signing_key.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

/// Path section configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathsSection {
    /// Base var directory (default: var)
    pub var_dir: PathBuf,
    /// Adapters root directory
    pub adapters_root: PathBuf,
    /// Artifacts root directory
    pub artifacts_root: PathBuf,
    /// Datasets root directory
    pub datasets_root: PathBuf,
    /// Documents root directory
    pub documents_root: PathBuf,
    /// Bundles root directory
    pub bundles_root: PathBuf,
    /// Model cache directory
    pub model_cache_dir: PathBuf,
}

impl PathsSection {
    /// Validate all paths in this section against /tmp.
    /// This prevents accidental loss of critical runtime state on system restart.
    pub fn validate(&self) -> adapteros_core::Result<()> {
        use crate::path_resolver::reject_tmp_persistent_path;

        reject_tmp_persistent_path(&self.var_dir, "var-dir")?;
        reject_tmp_persistent_path(&self.adapters_root, "adapters-root")?;
        reject_tmp_persistent_path(&self.artifacts_root, "artifacts-root")?;
        reject_tmp_persistent_path(&self.datasets_root, "datasets-root")?;
        reject_tmp_persistent_path(&self.documents_root, "documents-root")?;
        reject_tmp_persistent_path(&self.bundles_root, "bundles-root")?;
        reject_tmp_persistent_path(&self.model_cache_dir, "model-cache-dir")?;
        Ok(())
    }
}

/// Logging section configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingSection {
    /// Log level filter
    pub level: String,
    /// Log directory (None = stdout only)
    pub log_dir: Option<PathBuf>,
    /// Log file prefix
    pub log_prefix: String,
    /// JSON format output
    pub json_format: bool,
    /// Rotation strategy: hourly, daily, never
    pub rotation: String,
    /// Max rotated log files to keep
    pub max_log_files: usize,
}

/// Rate limits section configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitsSection {
    /// Requests per minute limit (default for unspecified tiers)
    pub requests_per_minute: u32,
    /// Burst size for rate limiting
    pub burst_size: u32,
    /// Inference requests per minute
    pub inference_per_minute: u32,
    /// Per-tier RPM override for health routes (None = unlimited)
    pub health_rpm: Option<u32>,
    /// Per-tier RPM override for public routes
    pub public_rpm: Option<u32>,
    /// Per-tier RPM override for internal routes
    pub internal_rpm: Option<u32>,
    /// Per-tier RPM override for protected routes
    pub protected_rpm: Option<u32>,
}

/// Metrics section configuration
#[derive(Clone, Serialize, Deserialize)]
pub struct MetricsSection {
    /// Metrics enabled
    pub enabled: bool,
    /// Bearer token for metrics endpoint
    pub bearer_token: String,
    /// Include histogram metrics
    pub include_histogram: bool,
}

impl std::fmt::Debug for MetricsSection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MetricsSection")
            .field("enabled", &self.enabled)
            .field(
                "bearer_token",
                &if self.bearer_token.is_empty() {
                    "(empty)"
                } else {
                    "[REDACTED]"
                },
            )
            .field("include_histogram", &self.include_histogram)
            .finish()
    }
}

/// Alerting section configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertingSection {
    /// Alerting enabled
    pub enabled: bool,
    /// Alert directory
    pub alert_dir: PathBuf,
    /// Max alerts per file
    pub max_alerts_per_file: usize,
    /// Rotate size in MB
    pub rotate_size_mb: u64,
}

/// Model section configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSection {
    /// Path to model directory
    pub path: Option<PathBuf>,
    /// Default model backend (auto/coreml/mlx/metal/cpu)
    pub backend: BackendKind,
    /// Canonical base model identifier
    pub base_id: Option<String>,
    /// Root directory containing base models
    pub cache_root: Option<PathBuf>,
    /// Maximum in-process model cache size (MB)
    pub cache_max_mb: Option<u64>,
    /// Tokenizer path
    pub tokenizer_path: Option<PathBuf>,
    /// Manifest path
    pub manifest_path: Option<PathBuf>,
}

/// Inference section configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceSection {
    /// Seed mode for request-scoped derivation
    pub seed_mode: SeedMode,
    /// Backend selection for workers (legacy name preserved)
    pub backend_profile: BackendKind,
    /// Worker identifier used in seed derivation
    pub worker_id: Option<u32>,
    /// Default sampling temperature when not specified by the client (default: 0.7)
    pub default_temperature: f32,
}

/// Health configuration for adapters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthSection {
    /// Adapter-specific health thresholds
    pub adapter: AdapterHealthThresholds,
}

/// Thresholds applied when computing adapter health.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterHealthThresholds {
    /// Drift value above which an adapter is at least degraded
    pub drift_hard_threshold: f64,
    /// Drift value that blocks promotion for high-tier adapters
    pub high_tier_block_threshold: f64,
}

/// CoreML section configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoremlSection {
    /// Preferred CoreML compute units
    pub compute_preference: CoreMLComputePreference,
    /// Whether to enforce production-mode constraints (ANE-only)
    pub production_mode: bool,
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthSection {
    /// Algorithm to use in development (e.g., hs256/hmac)
    pub dev_algo: String,
    /// Algorithm to use in production (e.g., eddsa/ed25519)
    pub prod_algo: String,
    /// Session lifetime in seconds
    pub session_lifetime: u64,
    /// Maximum failed attempts before lockout
    pub lockout_threshold: u32,
    /// Lockout cooldown in seconds
    pub lockout_cooldown: u64,
}

/// Self-hosting agent mode
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SelfHostingMode {
    /// Disabled (no background actions)
    Off,
    /// Enabled with automatic promotions gated by metrics threshold
    On,
    /// Enabled but promotions require human approval
    Safe,
}

impl FromStr for SelfHostingMode {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "on" => Ok(Self::On),
            "safe" => Ok(Self::Safe),
            _ => Ok(Self::Off),
        }
    }
}

/// Self-hosting agent configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfHostingSection {
    /// Mode: off/on/safe
    pub mode: SelfHostingMode,
    /// Allowed repo IDs the agent may manage
    pub repo_allowlist: Vec<String>,
    /// Minimum metric score required for auto-promotion (on mode only)
    pub promotion_threshold: f64,
    /// Whether human approval is required for promotions
    pub require_human_approval: bool,
}

/// Diagnostics verbosity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DiagLevel {
    /// Diagnostics disabled
    #[default]
    Off,
    /// Only error events (StageFailed)
    Errors,
    /// Stage enter/complete/failed events
    Stages,
    /// Stages + router decision events
    Router,
    /// All events including token-level
    Tokens,
}

impl FromStr for DiagLevel {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "off" => Ok(Self::Off),
            "errors" => Ok(Self::Errors),
            "stages" => Ok(Self::Stages),
            "router" => Ok(Self::Router),
            "tokens" => Ok(Self::Tokens),
            _ => Ok(Self::Off),
        }
    }
}

/// Diagnostics section configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticsSection {
    /// Global enable/disable for diagnostics collection
    pub enabled: bool,
    /// Verbosity level (controls which events are emitted)
    pub level: DiagLevel,
    /// Bounded channel capacity (default: 1000)
    pub channel_capacity: usize,
    /// Maximum events to persist per run (prevents runaway writes)
    pub max_events_per_run: u32,
    /// Batch size for writes (flush after N events)
    pub batch_size: usize,
    /// Batch timeout in milliseconds (flush after T ms even if batch incomplete)
    pub batch_timeout_ms: u64,
    /// Max consecutive persist failures before stale batch escalation
    pub stale_batch_max_attempts: u32,
    /// Max retry age in seconds before stale batch escalation
    pub stale_batch_max_age_secs: u64,
}

/// Uploads section configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadsSection {
    /// Require explicit workspace_id for chunked uploads (reject "default" scope)
    /// When false (default), empty workspace_id falls back to "default" with deprecation warning.
    /// Set to true to enforce explicit workspace scoping.
    pub require_explicit_workspace: bool,
    /// Per-workspace hard quota in bytes (default: 5 GiB, 0 = no limit)
    pub workspace_hard_quota_bytes: u64,
    /// Per-workspace soft quota in bytes (default: 80% of hard quota, 0 = no limit)
    pub workspace_soft_quota_bytes: u64,
}

/// Circuit breaker section configuration
///
/// Controls circuit breaker behavior for resilience. When consecutive failures
/// exceed the threshold, the circuit "opens" and requests fail fast. After the
/// reset timeout, limited requests are allowed to test recovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerSection {
    /// Number of consecutive failures before circuit breaker opens (default: 5)
    pub failure_threshold: u32,
    /// Time in seconds to wait before attempting recovery (default: 60)
    pub reset_timeout_secs: u64,
    /// Maximum calls allowed in half-open state to test recovery (default: 3)
    pub half_open_max_calls: u32,
    /// Deadline in seconds for worker operations before timeout (default: 600)
    pub worker_deadline_secs: u64,
    /// Enable automatic fallback to stub mode when circuit is open (default: true)
    pub enable_stub_fallback: bool,
    /// Health check interval in seconds when circuit is open (default: 30)
    pub health_check_interval_secs: u64,
}

/// Model Server section configuration (shared model inference)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelServerSection {
    /// Enable Model Server mode (workers connect to shared server)
    pub enabled: bool,
    /// gRPC server address (e.g., "http://127.0.0.1:18085") for TCP deployments.
    ///
    /// Compatibility path: if `socket_path` is set, worker-side clients prefer UDS and this
    /// address is treated as legacy fallback metadata.
    pub server_addr: String,
    /// Unix domain socket path for UDS-first hardened deployments.
    pub socket_path: Option<PathBuf>,
    /// Maximum KV cache sessions
    pub max_kv_cache_sessions: u32,
    /// Hot adapter promotion threshold (0.0 to 1.0)
    pub hot_adapter_threshold: f32,
    /// KV cache memory limit in MB (0 = automatic)
    pub kv_cache_limit_mb: u64,
}

impl Default for ModelServerSection {
    fn default() -> Self {
        Self {
            enabled: false,
            server_addr: "http://127.0.0.1:18085".to_string(),
            socket_path: None,
            max_kv_cache_sessions: 32,
            hot_adapter_threshold: 0.10,
            kv_cache_limit_mb: 0,
        }
    }
}

/// Worker safety section configuration
///
/// Controls timeout behavior and resource limits for worker operations.
/// Maps to the `[worker.safety]` section in cp.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSafetySection {
    /// Timeout for inference operations in seconds (default: 30)
    pub inference_timeout_secs: u64,
    /// Timeout for evidence collection in seconds (default: 5)
    pub evidence_timeout_secs: u64,
    /// Timeout for router operations in milliseconds (default: 100)
    pub router_timeout_ms: u64,
    /// Timeout for policy checks in milliseconds (default: 50)
    pub policy_timeout_ms: u64,
    /// Circuit breaker failure threshold (default: 5)
    pub circuit_breaker_threshold: u32,
    /// Circuit breaker timeout in seconds (default: 60)
    pub circuit_breaker_timeout_secs: u64,
    /// Maximum concurrent requests (default: 10)
    pub max_concurrent_requests: u32,
    /// Maximum tokens per second (default: 40)
    pub max_tokens_per_second: u32,
    /// Maximum memory per request in MB (default: 50)
    pub max_memory_per_request_mb: u64,
    /// Maximum CPU time per request in seconds (default: 30)
    pub max_cpu_time_per_request_secs: u64,
    /// Maximum requests per minute (default: 100)
    pub max_requests_per_minute: u32,
    /// Health check interval in seconds (default: 30)
    pub health_check_interval_secs: u64,
    /// Maximum response time in seconds (default: 60)
    pub max_response_time_secs: u64,
    /// Maximum memory growth in MB (default: 100)
    pub max_memory_growth_mb: u64,
    /// Maximum CPU time in seconds (default: 300)
    pub max_cpu_time_secs: u64,
    /// Maximum consecutive failures (default: 3)
    pub max_consecutive_failures: u32,
    /// Deadlock detector interval in seconds (default: 5)
    pub deadlock_check_interval_secs: u64,
    /// Maximum lock wait time before deadlock check in seconds (default: 30)
    pub max_wait_time_secs: u64,
    /// Maximum nested lock depth before warning (default: 10)
    pub max_lock_depth: usize,
    /// Timeout for deadlock recovery attempts in seconds (default: 10)
    pub recovery_timeout_secs: u64,
}

impl Default for WorkerSafetySection {
    fn default() -> Self {
        Self {
            inference_timeout_secs: 30,
            evidence_timeout_secs: 5,
            router_timeout_ms: 100,
            policy_timeout_ms: 50,
            circuit_breaker_threshold: 5,
            circuit_breaker_timeout_secs: 60,
            max_concurrent_requests: 10,
            max_tokens_per_second: 40,
            max_memory_per_request_mb: 50,
            max_cpu_time_per_request_secs: 30,
            max_requests_per_minute: 100,
            health_check_interval_secs: 30,
            max_response_time_secs: 60,
            max_memory_growth_mb: 100,
            max_cpu_time_secs: 300,
            max_consecutive_failures: 3,
            deadlock_check_interval_secs: 5,
            max_wait_time_secs: 30,
            max_lock_depth: 10,
            recovery_timeout_secs: 10,
        }
    }
}

/// Instance identity section (from manifest [instance])
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceSection {
    /// Human-readable instance name
    pub name: String,
    /// Profile used to generate this manifest (dev/production/reference)
    pub profile: String,
    /// ISO 8601 timestamp when manifest was generated
    pub generated_at: Option<String>,
    /// Manifest schema version
    pub schema_version: u32,
}

/// Service toggle section (from manifest [services])
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServicesSection {
    /// Start the LoRA worker process
    pub worker: bool,
    /// Start the Secure Enclave Daemon
    pub secd: bool,
    /// Start the Node Agent
    pub node: bool,
    /// Enable quick boot (skip non-essential checks)
    pub quick_boot: bool,
}

/// Boot behavior section (from manifest [boot])
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootSection {
    /// Logging profile (json/plain/debug/trace)
    pub log_profile: String,
    /// Health endpoint timeout in seconds
    pub health_timeout_secs: u64,
    /// Readiness endpoint timeout in seconds
    pub readyz_timeout_secs: u64,
    /// Auto-seed model into database on boot
    pub auto_seed_model: bool,
    /// Verify chat response after readiness
    pub verify_chat: bool,
}

/// Source of a configuration value (for debugging/observability)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfigValueSource {
    /// Value from TOML config file
    Toml(String),
    /// Value from environment variable
    Environment(String),
    /// Value from CLI argument
    Cli,
    /// Default value from schema
    Default,
}

impl std::fmt::Display for ConfigValueSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigValueSource::Toml(path) => write!(f, "toml:{}", path),
            ConfigValueSource::Environment(var) => write!(f, "env:{}", var),
            ConfigValueSource::Cli => write!(f, "cli"),
            ConfigValueSource::Default => write!(f, "default"),
        }
    }
}

impl EffectiveConfig {
    /// Build EffectiveConfig from DeterministicConfig
    pub fn from_deterministic(config: DeterministicConfig) -> Result<Self> {
        debug!(
            config_hash = %config.get_metadata().hash,
            sources_count = config.get_metadata().sources.len(),
            "Building effective configuration"
        );

        let sources = Self::build_sources(&config);
        let is_production = config
            .get("server.production.mode")
            .map(|v| v == "true")
            .unwrap_or(false);

        let server = Self::build_server_section(&config)?;
        let database = Self::build_database_section(&config)?;
        let security = Self::build_security_section(&config)?;
        let paths = Self::build_paths_section(&config)?;
        let logging = Self::build_logging_section(&config)?;
        let rate_limits = Self::build_rate_limits_section(&config);
        let metrics = Self::build_metrics_section(&config);
        let alerting = Self::build_alerting_section(&config);
        let model = Self::build_model_section(&config);
        let coreml = Self::build_coreml_section(&config);
        let auth = Self::build_auth_section(&config);
        let inference = Self::build_inference_section(&config, is_production);
        let health = Self::build_health_section(&config);
        let self_hosting = Self::build_self_hosting_section(&config);
        let diagnostics = Self::build_diagnostics_section(&config);
        let uploads = Self::build_uploads_section(&config);
        let circuit_breaker = Self::build_circuit_breaker_section(&config);
        let model_server = Self::build_model_server_section(&config);
        let worker_safety = Self::build_worker_safety_section(&config);
        let instance = Self::build_instance_section(&config);
        let services = Self::build_services_section(&config);
        let boot = Self::build_boot_section(&config);

        let effective_config = Self {
            inner: config,
            server,
            database,
            security,
            paths,
            logging,
            rate_limits,
            metrics,
            alerting,
            model,
            coreml,
            auth,
            inference,
            health,
            self_hosting,
            diagnostics,
            uploads,
            circuit_breaker,
            model_server,
            worker_safety,
            instance,
            services,
            boot,
            sources,
            is_production,
        };

        // Validate production paths
        effective_config.validate_production_paths()?;

        // Validate critical configuration values
        effective_config.validate_critical_config()?;

        debug!(
            is_production = is_production,
            port = effective_config.server.port,
            db_url = %effective_config.database.url,
            "Effective configuration built successfully"
        );

        Ok(effective_config)
    }

    /// Get the configuration hash (BLAKE3)
    pub fn config_hash(&self) -> &str {
        &self.inner.get_metadata().hash
    }

    /// Check if running in production mode
    pub fn is_production(&self) -> bool {
        self.is_production
    }

    /// Get the source of a configuration value
    pub fn get_source(&self, key: &str) -> Option<&String> {
        self.sources.get(key)
    }

    /// Get all sources for debugging
    pub fn all_sources(&self) -> &HashMap<String, String> {
        &self.sources
    }

    /// Get a raw value by key
    pub fn get(&self, key: &str) -> Option<&String> {
        self.inner.get(key)
    }

    /// Get all raw values
    pub fn all_values(&self) -> &HashMap<String, String> {
        self.inner.get_all()
    }

    /// Get the underlying deterministic config
    pub fn inner(&self) -> &DeterministicConfig {
        &self.inner
    }

    /// Validate that all critical paths are absolute in production mode
    ///
    /// Returns Ok(()) if:
    /// - Not in production mode, OR
    /// - All critical paths are absolute (start with /)
    ///
    /// Returns error if production mode requires absolute paths but relative paths are found.
    pub fn validate_production_paths(&self) -> Result<()> {
        // Skip validation if not in production mode
        if !self.is_production {
            return Ok(());
        }

        let mut errors = Vec::new();

        // Check database URL path (extract from sqlite:// URLs)
        if let Some(db_path) = self.extract_db_path(&self.database.url) {
            let path = std::path::Path::new(&db_path);
            if !Self::is_absolute_or_url(path) {
                errors.push(format!(
                    "Production mode requires absolute path for database.url: got '{}'\nHint: Use absolute paths under AOS_VAR_DIR (e.g., /adapter-os/var/aos-cp.sqlite3)",
                    db_path
                ));
            }
        }

        // Check adapters_root
        if !Self::is_absolute_or_url(&self.paths.adapters_root) {
            errors.push(format!(
                "Production mode requires absolute path for paths.adapters_root: got '{}'\nHint: Use absolute paths under AOS_VAR_DIR (e.g., /adapter-os/var/adapters)",
                self.paths.adapters_root.display()
            ));
        }

        // Check datasets_root
        if !Self::is_absolute_or_url(&self.paths.datasets_root) {
            errors.push(format!(
                "Production mode requires absolute path for paths.datasets_root: got '{}'\nHint: Use absolute paths under AOS_VAR_DIR (e.g., /adapter-os/var/datasets)",
                self.paths.datasets_root.display()
            ));
        }

        // Check documents_root
        if !Self::is_absolute_or_url(&self.paths.documents_root) {
            errors.push(format!(
                "Production mode requires absolute path for paths.documents_root: got '{}'\nHint: Use absolute paths under AOS_VAR_DIR (e.g., /adapter-os/var/documents)",
                self.paths.documents_root.display()
            ));
        }

        // Check log_dir if set
        if let Some(ref log_dir) = self.logging.log_dir {
            if !Self::is_absolute_or_url(log_dir) {
                errors.push(format!(
                    "Production mode requires absolute path for logging.log_dir: got '{}'\nHint: Use absolute paths under AOS_VAR_DIR (e.g., /adapter-os/var/logs)",
                    log_dir.display()
                ));
            }
        }

        if !errors.is_empty() {
            return Err(AosError::Config(errors.join("\n")));
        }

        Ok(())
    }

    /// Minimum required length for JWT secret in production
    const MIN_JWT_SECRET_LENGTH: usize = 64;

    /// Known placeholder patterns that must never be used in production JWT secrets.
    const JWT_SECRET_PLACEHOLDER_PATTERNS: &'static [&'static str] = &[
        "CHANGE_ME",
        "change-me-in-production",
        "TODO",
        "PLACEHOLDER",
        "XXXXXXXX",
        "12345678",
        "00000000",
        "secret",
        "password",
        "default",
        "example",
        "insecure",
        "changeme",
        "replace",
        "fixme",
        "your_secret",
        "your-secret",
        "test",
        "development",
    ];

    /// Validate JWT secret strength for production use (AUTH-004).
    fn validate_jwt_secret_strength(jwt_secret: &str) -> Option<String> {
        if jwt_secret.trim().is_empty() {
            return Some("JWT secret is empty or whitespace-only".to_string());
        }
        if jwt_secret.len() < Self::MIN_JWT_SECRET_LENGTH {
            return Some(format!(
                "JWT secret is too short ({} chars, minimum {} required)",
                jwt_secret.len(),
                Self::MIN_JWT_SECRET_LENGTH
            ));
        }
        let secret_lower = jwt_secret.to_lowercase();
        for pattern in Self::JWT_SECRET_PLACEHOLDER_PATTERNS {
            if secret_lower.contains(&pattern.to_lowercase()) {
                return Some(format!(
                    "JWT secret contains placeholder pattern '{}'",
                    pattern
                ));
            }
        }
        if let Some(first_char) = jwt_secret.chars().next() {
            if jwt_secret.chars().all(|c| c == first_char) {
                return Some(format!(
                    "JWT secret has no entropy (all character '{}')",
                    first_char
                ));
            }
        }
        for pattern_len in 1..=8 {
            if jwt_secret.len() >= pattern_len * 4 {
                let pattern = &jwt_secret[..pattern_len];
                let expected_repeats = jwt_secret.len() / pattern_len;
                let expected_full = pattern.repeat(expected_repeats);
                if jwt_secret.starts_with(&expected_full) {
                    return Some(format!(
                        "JWT secret is a simple repetitive pattern ('{}')",
                        pattern
                    ));
                }
            }
        }
        None
    }

    /// Validate critical configuration values
    ///
    /// Ensures that security-critical configuration values are set correctly.
    /// Some checks only apply in production mode.
    pub fn validate_critical_config(&self) -> Result<()> {
        let mut errors = Vec::new();

        // Rate limits must be positive
        if self.rate_limits.requests_per_minute == 0 {
            errors.push("rate_limits.requests_per_minute must be > 0".to_string());
        }
        if self.rate_limits.burst_size == 0 {
            errors.push("rate_limits.burst_size must be > 0".to_string());
        }
        if self.rate_limits.inference_per_minute == 0 {
            errors.push("rate_limits.inference_per_minute must be > 0".to_string());
        }

        // dev_bypass is only allowed in debug builds
        #[cfg(not(debug_assertions))]
        if self.security.dev_bypass {
            errors.push(
                "security.dev_bypass=true is not allowed in release builds\n\
                 Hint: Set security.dev_bypass=false or rebuild in debug mode"
                    .to_string(),
            );
        }

        // Production-only checks
        if self.is_production {
            // AUTH-004: JWT secret validation - comprehensive placeholder and entropy check
            // This is fail-closed in production - any validation failure blocks startup
            if let Some(jwt_error) = Self::validate_jwt_secret_strength(&self.security.jwt_secret) {
                errors.push(format!(
                    "AUTH-004: {}\nHint: Generate with: openssl rand -base64 48",
                    jwt_error
                ));
            }

            // dev_bypass must be false in production
            if self.security.dev_bypass {
                errors.push(
                    "Production mode requires security.dev_bypass=false\n\
                     Hint: Set security.dev_bypass=false in your config"
                        .to_string(),
                );
            }

            // dev_login_enabled must be false in production
            if self.security.dev_login_enabled {
                errors.push(
                    "Production mode requires security.dev_login_enabled=false\n\
                     Hint: Set security.dev_login_enabled=false in your config"
                        .to_string(),
                );
            }

            // require_pf_deny should be true in production (warning only)
            if !self.security.require_pf_deny {
                warn!(
                    "Production mode recommends security.require_pf_deny=true for network isolation\n\
                     Hint: Set security.require_pf_deny=true in your config"
                );
            }

            // Database URL must be set
            if self.database.url.is_empty() {
                errors.push("Production mode requires database.url to be set".to_string());
            }

            if self.inference.seed_mode != SeedMode::Strict {
                errors
                    .push("Production mode requires inference.seed_mode to be strict".to_string());
            }

            if matches!(self.inference.seed_mode, SeedMode::NonDeterministic) {
                errors
                    .push("NonDeterministic seed_mode is not permitted in production".to_string());
            }

            if self.inference.backend_profile == BackendKind::Auto {
                errors.push(
                    "Production mode requires inference.backend_profile to be explicit (coreml|metal|mlx|cpu)"
                        .to_string(),
                );
            }
        }

        if !errors.is_empty() {
            return Err(AosError::Config(errors.join("\n")));
        }

        Ok(())
    }

    /// Check if a path is absolute or a URL
    fn is_absolute_or_url(path: &std::path::Path) -> bool {
        // Check if it's an absolute path
        if path.is_absolute() {
            return true;
        }

        // Check if it's a URL (contains ://)
        if let Some(path_str) = path.to_str() {
            if path_str.contains("://") {
                return true;
            }
        }

        false
    }

    /// Extract filesystem path from database URL
    ///
    /// Handles URLs like:
    /// - "sqlite://var/aos-cp.sqlite3" -> Some("var/aos-cp.sqlite3")
    /// - "sqlite:///adapter-os/var/aos-cp.sqlite3" -> Some("/adapter-os/var/aos-cp.sqlite3")
    /// - "postgres://..." -> None (not a file path)
    fn extract_db_path(&self, url: &str) -> Option<String> {
        url.strip_prefix("sqlite://")
            .map(|stripped| stripped.to_string())
    }

    // Builder methods for each section
    fn build_sources(config: &DeterministicConfig) -> HashMap<String, String> {
        let mut sources = HashMap::new();
        for source in &config.get_metadata().sources {
            sources.insert(source.key.clone(), source.source.clone());
        }
        sources
    }

    fn build_server_section(config: &DeterministicConfig) -> Result<ServerSection> {
        Ok(ServerSection {
            port: config
                .get("server.port")
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_SERVER_PORT),
            host: config
                .get("server.host")
                .cloned()
                .unwrap_or_else(|| DEFAULT_SERVER_HOST.to_string()),
            production_mode: config
                .get("server.production.mode")
                .map(|v| v == "true")
                .unwrap_or(false),
            uds_socket: config.get("server.uds.socket").map(PathBuf::from),
            drain_timeout_secs: config
                .get("server.drain.timeout.secs")
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
            boot_timeout_secs: config
                .get("server.boot.timeout.secs")
                .and_then(|v| v.parse().ok())
                .unwrap_or(300),
            workers: config
                .get("server.workers")
                .and_then(|v| v.parse().ok())
                .unwrap_or(4),
            worker_heartbeat_interval_secs: config
                .get("server.worker.heartbeat.interval.secs")
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
        })
    }

    fn build_database_section(config: &DeterministicConfig) -> Result<DatabaseSection> {
        Ok(DatabaseSection {
            url: config
                .get("database.url")
                .cloned()
                .unwrap_or_else(|| "sqlite://var/aos-cp.sqlite3".to_string()),
            pool_size: config
                .get("database.pool.size")
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
            timeout_secs: config
                .get("database.timeout")
                .and_then(|v| Self::parse_duration_secs(v))
                .unwrap_or(30),
        })
    }

    fn build_security_section(config: &DeterministicConfig) -> Result<SecuritySection> {
        // JWT secret is required - generate a random one if missing and log a warning
        let jwt_secret = match config.get("security.jwt.secret").cloned() {
            Some(secret) if !secret.trim().is_empty() => secret,
            _ => {
                // Generate a random 64-byte secret (base64 encoded)
                use std::time::{SystemTime, UNIX_EPOCH};
                let timestamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                let random_seed = format!(
                    "{:x}{:x}{:x}",
                    timestamp,
                    std::process::id(),
                    std::ptr::addr_of!(timestamp) as usize
                );
                // Create a pseudo-random secret from available entropy
                let generated = format!(
                    "GENERATED-{}-{}",
                    random_seed, "0123456789abcdef0123456789abcdef0123456789abcdef"
                );
                warn!(
                    "JWT secret not configured - generated ephemeral secret. \
                     This is insecure for production. Set AOS_JWT_SECRET or security.jwt.secret"
                );
                generated
            }
        };

        Ok(SecuritySection {
            jwt_secret,
            jwt_mode: config
                .get("security.jwt.mode")
                .cloned()
                .unwrap_or_else(|| "hmac".to_string()),
            jwt_ttl_hours: config
                .get("security.jwt.ttl")
                .and_then(|v| Self::parse_duration_hours(v))
                .unwrap_or(8),
            require_pf_deny: config
                .get("security.pf.deny")
                .map(|v| v == "true")
                .unwrap_or(false),
            dev_login_enabled: config
                .get("security.dev.login.enabled")
                .map(|v| v == "true")
                .unwrap_or(false),
            dev_bypass: config
                .get("security.dev.bypass")
                .map(|v| v == "true")
                .unwrap_or(false),
            signing_key: config.get("signing.key").cloned(),
        })
    }

    fn build_paths_section(config: &DeterministicConfig) -> Result<PathsSection> {
        use adapteros_core::rebase_var_path;
        Ok(PathsSection {
            var_dir: config
                .get("var.dir")
                .map(rebase_var_path)
                .unwrap_or_else(|| rebase_var_path("var")),
            adapters_root: config
                .get("adapters.dir")
                .map(rebase_var_path)
                .unwrap_or_else(|| rebase_var_path("var/adapters/repo")),
            artifacts_root: config
                .get("artifacts.dir")
                .map(rebase_var_path)
                .unwrap_or_else(|| rebase_var_path("var/artifacts")),
            datasets_root: config
                .get("paths.datasets.root")
                .or_else(|| config.get("datasets.root"))
                .map(rebase_var_path)
                .unwrap_or_else(|| rebase_var_path("var/datasets")),
            documents_root: config
                .get("paths.documents.root")
                .or_else(|| config.get("documents.root"))
                .map(rebase_var_path)
                .unwrap_or_else(|| rebase_var_path("var/documents")),
            bundles_root: config
                .get("paths.bundles.root")
                .or_else(|| config.get("bundles.root"))
                .map(rebase_var_path)
                .unwrap_or_else(|| rebase_var_path("var/bundles")),
            model_cache_dir: config
                .get("model.cache.dir")
                .map(rebase_var_path)
                .unwrap_or_else(|| rebase_var_path("var/models")),
        })
    }

    fn build_logging_section(config: &DeterministicConfig) -> Result<LoggingSection> {
        Ok(LoggingSection {
            level: config
                .get("log.level")
                .cloned()
                .unwrap_or_else(|| "info".to_string()),
            log_dir: config.get("log.file").map(PathBuf::from),
            log_prefix: config
                .get("logging.log.prefix")
                .cloned()
                .unwrap_or_else(|| "aos-cp".to_string()),
            json_format: config
                .get("log.format")
                .map(|v| v == "json")
                .unwrap_or(false),
            rotation: config
                .get("logging.rotation")
                .cloned()
                .unwrap_or_else(|| "daily".to_string()),
            max_log_files: config
                .get("logging.max.log.files")
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
        })
    }

    fn build_rate_limits_section(config: &DeterministicConfig) -> RateLimitsSection {
        RateLimitsSection {
            requests_per_minute: config
                .get("rate.limits.requests.per.minute")
                .and_then(|v| v.parse().ok())
                .unwrap_or(100),
            burst_size: config
                .get("rate.limits.burst.size")
                .and_then(|v| v.parse().ok())
                .unwrap_or(20),
            inference_per_minute: config
                .get("rate.limits.inference.per.minute")
                .and_then(|v| v.parse().ok())
                .unwrap_or(60),
            health_rpm: config
                .get("rate.limits.health.rpm")
                .and_then(|v| v.parse().ok()),
            public_rpm: config
                .get("rate.limits.public.rpm")
                .and_then(|v| v.parse().ok()),
            internal_rpm: config
                .get("rate.limits.internal.rpm")
                .and_then(|v| v.parse().ok()),
            protected_rpm: config
                .get("rate.limits.protected.rpm")
                .and_then(|v| v.parse().ok()),
        }
    }

    fn build_metrics_section(config: &DeterministicConfig) -> MetricsSection {
        MetricsSection {
            enabled: config
                .get("metrics.enabled")
                .map(|v| v == "true")
                .unwrap_or(true),
            bearer_token: config
                .get("metrics.bearer.token")
                .cloned()
                .unwrap_or_default(),
            include_histogram: config
                .get("metrics.include.histogram")
                .map(|v| v == "true")
                .unwrap_or(true),
        }
    }

    fn build_alerting_section(config: &DeterministicConfig) -> AlertingSection {
        AlertingSection {
            enabled: config
                .get("alerting.enabled")
                .map(|v| v == "true")
                .unwrap_or(true),
            alert_dir: config
                .get("alerting.alert.dir")
                .map(adapteros_core::rebase_var_path)
                .unwrap_or_else(|| adapteros_core::rebase_var_path("var/alerts")),
            max_alerts_per_file: config
                .get("alerting.max.alerts.per.file")
                .and_then(|v| v.parse().ok())
                .unwrap_or(1000),
            rotate_size_mb: config
                .get("alerting.rotate.size.mb")
                .and_then(|v| v.parse().ok())
                .unwrap_or(100),
        }
    }

    fn build_auth_section(config: &DeterministicConfig) -> AuthSection {
        AuthSection {
            dev_algo: config
                .get("auth.dev_algo")
                .cloned()
                .unwrap_or_else(|| "hs256".to_string()),
            prod_algo: config
                .get("auth.prod_algo")
                .cloned()
                .unwrap_or_else(|| "eddsa".to_string()),
            session_lifetime: config
                .get("auth.session_lifetime")
                .and_then(|v| Self::parse_duration_secs(v))
                .unwrap_or(12 * 3600),
            lockout_threshold: config
                .get("auth.lockout_threshold")
                .and_then(|v| v.parse().ok())
                .unwrap_or(5),
            lockout_cooldown: config
                .get("auth.lockout_cooldown")
                .and_then(|v| Self::parse_duration_secs(v))
                .unwrap_or(300),
        }
    }

    fn build_model_section(config: &DeterministicConfig) -> ModelSection {
        let default_backend = BackendKind::Auto;
        let backend = config.get("model.backend").map(|raw| {
            BackendKind::from_str(raw).map_err(|err| {
                warn!(backend = raw, error = %err, "Invalid model backend, falling back to default");
                err
            })
        });

        let resolved_backend = backend.and_then(|res| res.ok()).unwrap_or(default_backend);

        ModelSection {
            path: config.get("model.path").map(PathBuf::from),
            backend: resolved_backend,
            base_id: config.get("base_model.id").cloned(),
            cache_root: config.get("base_model.cache_root").map(PathBuf::from),
            cache_max_mb: config
                .get("model.cache.max.mb")
                .and_then(|v| v.parse().ok()),
            tokenizer_path: config.get("tokenizer.path").map(PathBuf::from),
            manifest_path: config
                .get("manifest.path")
                .or_else(|| config.get("model.manifest"))
                .map(PathBuf::from),
        }
    }

    fn build_inference_section(
        config: &DeterministicConfig,
        is_production: bool,
    ) -> InferenceSection {
        let backend_default = if is_production {
            BackendKind::Metal
        } else {
            BackendKind::Auto
        };

        let seed_mode = config
            .get("inference.seed.mode")
            .and_then(|v| SeedMode::from_str(v).ok())
            .unwrap_or({
                if is_production {
                    SeedMode::Strict
                } else {
                    SeedMode::BestEffort
                }
            });

        let backend_profile = config
            .get("inference.backend.profile")
            .map(|raw| {
                BackendKind::from_str(raw).map_err(|err| {
                    warn!(
                        backend = raw,
                        error = %err,
                        "Invalid inference backend, falling back to default"
                    );
                    err
                })
            })
            .and_then(|res| res.ok())
            .unwrap_or(backend_default);

        let worker_id = config
            .get("inference.worker.id")
            .and_then(|v| v.parse::<u32>().ok());

        let default_temperature = config
            .get("inference.default_temperature")
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(0.7);

        InferenceSection {
            seed_mode,
            backend_profile,
            worker_id,
            default_temperature,
        }
    }

    fn build_health_section(config: &DeterministicConfig) -> HealthSection {
        let drift_hard_threshold = config
            .get("health.adapter.drift_hard_threshold")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.15);

        let high_tier_block_threshold = config
            .get("health.adapter.high_tier_block_threshold")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.10);

        HealthSection {
            adapter: AdapterHealthThresholds {
                drift_hard_threshold,
                high_tier_block_threshold,
            },
        }
    }

    fn build_self_hosting_section(config: &DeterministicConfig) -> SelfHostingSection {
        let mode = config
            .get("self_hosting.mode")
            .and_then(|v| SelfHostingMode::from_str(v).ok())
            .unwrap_or(SelfHostingMode::Off);

        let repo_allowlist = config
            .get("self_hosting.repo_allowlist")
            .map(|raw| {
                raw.split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let promotion_threshold = config
            .get("self_hosting.promotion_threshold")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0);

        let require_human_approval = matches!(mode, SelfHostingMode::Safe);

        SelfHostingSection {
            mode,
            repo_allowlist,
            promotion_threshold,
            require_human_approval,
        }
    }

    fn build_diagnostics_section(config: &DeterministicConfig) -> DiagnosticsSection {
        let enabled = config
            .get("diag.enabled")
            .map(|v| v == "true")
            .unwrap_or(false);

        let level = config
            .get("diag.level")
            .and_then(|v| DiagLevel::from_str(v).ok())
            .unwrap_or(DiagLevel::Off);

        let channel_capacity = config
            .get("diag.channel_capacity")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1000);

        let max_events_per_run = config
            .get("diag.max_events_per_run")
            .and_then(|v| v.parse().ok())
            .unwrap_or(10000);

        let batch_size = config
            .get("diag.batch_size")
            .and_then(|v| v.parse().ok())
            .unwrap_or(100);

        let batch_timeout_ms = config
            .get("diag.batch_timeout_ms")
            .and_then(|v| v.parse().ok())
            .unwrap_or(500);

        let stale_batch_max_attempts = config
            .get("diag.stale_batch_max_attempts")
            .and_then(|v| v.parse().ok())
            .unwrap_or(5);

        let stale_batch_max_age_secs = config
            .get("diag.stale_batch_max_age_secs")
            .and_then(|v| v.parse().ok())
            .unwrap_or(300);

        DiagnosticsSection {
            enabled,
            level,
            channel_capacity,
            max_events_per_run,
            batch_size,
            batch_timeout_ms,
            stale_batch_max_attempts,
            stale_batch_max_age_secs,
        }
    }

    fn build_uploads_section(config: &DeterministicConfig) -> UploadsSection {
        let require_explicit_workspace = config
            .get("uploads.require_explicit_workspace")
            .map(|v| v == "true")
            .unwrap_or(false);

        // Default workspace hard quota: 5 GiB
        const DEFAULT_WORKSPACE_HARD_QUOTA: u64 = 5 * 1024 * 1024 * 1024;

        let workspace_hard_quota_bytes = config
            .get("uploads.workspace_hard_quota_bytes")
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_WORKSPACE_HARD_QUOTA);

        let workspace_soft_quota_bytes = config
            .get("uploads.workspace_soft_quota_bytes")
            .and_then(|v| v.parse().ok())
            .unwrap_or((workspace_hard_quota_bytes as f64 * 0.8) as u64);

        UploadsSection {
            require_explicit_workspace,
            workspace_hard_quota_bytes,
            workspace_soft_quota_bytes,
        }
    }

    fn build_coreml_section(config: &DeterministicConfig) -> CoremlSection {
        let compute_preference = config
            .get("coreml.compute_preference")
            .and_then(|v| CoreMLComputePreference::from_str(v).ok())
            .unwrap_or_default();

        let production_mode = config
            .get("coreml.production_mode")
            .and_then(|v| parse_bool(v).ok())
            .unwrap_or(false);

        CoremlSection {
            compute_preference,
            production_mode,
        }
    }

    fn build_circuit_breaker_section(config: &DeterministicConfig) -> CircuitBreakerSection {
        let failure_threshold = config
            .get("circuit_breaker.failure_threshold")
            .and_then(|v| v.parse().ok())
            .unwrap_or(5);

        let reset_timeout_secs = config
            .get("circuit_breaker.reset_timeout_secs")
            .and_then(|v| v.parse().ok())
            .unwrap_or(60);

        let half_open_max_calls = config
            .get("circuit_breaker.half_open_max_calls")
            .and_then(|v| v.parse().ok())
            .unwrap_or(3);

        let worker_deadline_secs = config
            .get("circuit_breaker.worker_deadline_secs")
            .and_then(|v| v.parse().ok())
            .unwrap_or(600);

        let enable_stub_fallback = config
            .get("circuit_breaker.enable_stub_fallback")
            .map(|v| v == "true")
            .unwrap_or(true);

        let health_check_interval_secs = config
            .get("circuit_breaker.health_check_interval_secs")
            .and_then(|v| v.parse().ok())
            .unwrap_or(30);

        CircuitBreakerSection {
            failure_threshold,
            reset_timeout_secs,
            half_open_max_calls,
            worker_deadline_secs,
            enable_stub_fallback,
            health_check_interval_secs,
        }
    }

    fn build_model_server_section(config: &DeterministicConfig) -> ModelServerSection {
        let enabled = config
            .get("model_server.enabled")
            .map(|v| v == "true")
            .unwrap_or(false);

        let server_addr = config
            .get("model_server.server_addr")
            .map(|s| s.to_string())
            .unwrap_or_else(|| "http://127.0.0.1:18085".to_string());
        let mut socket_path = config
            .get("model_server.socket_path")
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        if socket_path.is_none() && server_addr.starts_with("unix://") {
            socket_path = Some(PathBuf::from(server_addr.trim_start_matches("unix://")));
        }

        let max_kv_cache_sessions = config
            .get("model_server.max_kv_cache_sessions")
            .and_then(|v| v.parse().ok())
            .unwrap_or(32);

        let hot_adapter_threshold = config
            .get("model_server.hot_adapter_threshold")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.10);

        let kv_cache_limit_mb = config
            .get("model_server.kv_cache_limit_mb")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);

        ModelServerSection {
            enabled,
            server_addr,
            socket_path,
            max_kv_cache_sessions,
            hot_adapter_threshold,
            kv_cache_limit_mb,
        }
    }

    fn build_worker_safety_section(config: &DeterministicConfig) -> WorkerSafetySection {
        let inference_timeout_secs = config
            .get("worker.safety.inference_timeout_secs")
            .and_then(|v| v.parse().ok())
            .unwrap_or(30);

        let evidence_timeout_secs = config
            .get("worker.safety.evidence_timeout_secs")
            .and_then(|v| v.parse().ok())
            .unwrap_or(5);

        let router_timeout_ms = config
            .get("worker.safety.router_timeout_ms")
            .and_then(|v| v.parse().ok())
            .unwrap_or(100);

        let policy_timeout_ms = config
            .get("worker.safety.policy_timeout_ms")
            .and_then(|v| v.parse().ok())
            .unwrap_or(50);

        let circuit_breaker_threshold = config
            .get("worker.safety.circuit_breaker_threshold")
            .and_then(|v| v.parse().ok())
            .unwrap_or(5);

        let circuit_breaker_timeout_secs = config
            .get("worker.safety.circuit_breaker_timeout_secs")
            .and_then(|v| v.parse().ok())
            .unwrap_or(60);

        let max_concurrent_requests = config
            .get("worker.safety.max_concurrent_requests")
            .and_then(|v| v.parse().ok())
            .unwrap_or(10);

        let max_tokens_per_second = config
            .get("worker.safety.max_tokens_per_second")
            .and_then(|v| v.parse().ok())
            .unwrap_or(40);

        let max_memory_per_request_mb = config
            .get("worker.safety.max_memory_per_request_mb")
            .and_then(|v| v.parse().ok())
            .unwrap_or(50);

        let max_cpu_time_per_request_secs = config
            .get("worker.safety.max_cpu_time_per_request_secs")
            .and_then(|v| v.parse().ok())
            .unwrap_or(30);

        let max_requests_per_minute = config
            .get("worker.safety.max_requests_per_minute")
            .and_then(|v| v.parse().ok())
            .unwrap_or(100);

        let health_check_interval_secs = config
            .get("worker.safety.health_check_interval_secs")
            .and_then(|v| v.parse().ok())
            .unwrap_or(30);

        let max_response_time_secs = config
            .get("worker.safety.max_response_time_secs")
            .and_then(|v| v.parse().ok())
            .unwrap_or(60);

        let max_memory_growth_mb = config
            .get("worker.safety.max_memory_growth_mb")
            .and_then(|v| v.parse().ok())
            .unwrap_or(100);

        let max_cpu_time_secs = config
            .get("worker.safety.max_cpu_time_secs")
            .and_then(|v| v.parse().ok())
            .unwrap_or(300);

        let max_consecutive_failures = config
            .get("worker.safety.max_consecutive_failures")
            .and_then(|v| v.parse().ok())
            .unwrap_or(3);

        let deadlock_check_interval_secs = config
            .get("worker.safety.deadlock_check_interval_secs")
            .and_then(|v| v.parse().ok())
            .unwrap_or(5);

        let max_wait_time_secs = config
            .get("worker.safety.max_wait_time_secs")
            .and_then(|v| v.parse().ok())
            .unwrap_or(30);

        let max_lock_depth = config
            .get("worker.safety.max_lock_depth")
            .and_then(|v| v.parse().ok())
            .unwrap_or(10);

        let recovery_timeout_secs = config
            .get("worker.safety.recovery_timeout_secs")
            .and_then(|v| v.parse().ok())
            .unwrap_or(10);

        WorkerSafetySection {
            inference_timeout_secs,
            evidence_timeout_secs,
            router_timeout_ms,
            policy_timeout_ms,
            circuit_breaker_threshold,
            circuit_breaker_timeout_secs,
            max_concurrent_requests,
            max_tokens_per_second,
            max_memory_per_request_mb,
            max_cpu_time_per_request_secs,
            max_requests_per_minute,
            health_check_interval_secs,
            max_response_time_secs,
            max_memory_growth_mb,
            max_cpu_time_secs,
            max_consecutive_failures,
            deadlock_check_interval_secs,
            max_wait_time_secs,
            max_lock_depth,
            recovery_timeout_secs,
        }
    }

    fn build_instance_section(config: &DeterministicConfig) -> InstanceSection {
        InstanceSection {
            name: config
                .get("instance.name")
                .cloned()
                .unwrap_or_else(|| "default".to_string()),
            profile: config
                .get("instance.profile")
                .cloned()
                .unwrap_or_else(|| "dev".to_string()),
            generated_at: config.get("instance.generated_at").cloned(),
            schema_version: config
                .get("instance.schema_version")
                .and_then(|v| v.parse().ok())
                .unwrap_or(1),
        }
    }

    fn build_services_section(config: &DeterministicConfig) -> ServicesSection {
        ServicesSection {
            worker: config
                .get("services.worker")
                .and_then(|v| parse_bool(v).ok())
                .unwrap_or(true),
            secd: config
                .get("services.secd")
                .and_then(|v| parse_bool(v).ok())
                .unwrap_or(false),
            node: config
                .get("services.node")
                .and_then(|v| parse_bool(v).ok())
                .unwrap_or(false),
            quick_boot: config
                .get("services.quick_boot")
                .or_else(|| config.get("boot.quick"))
                .and_then(|v| parse_bool(v).ok())
                .unwrap_or(false),
        }
    }

    fn build_boot_section(config: &DeterministicConfig) -> BootSection {
        BootSection {
            log_profile: config
                .get("boot.log_profile")
                .cloned()
                .unwrap_or_else(|| "json".to_string()),
            health_timeout_secs: config
                .get("boot.health_timeout_secs")
                .and_then(|v| v.parse().ok())
                .unwrap_or(15),
            readyz_timeout_secs: config
                .get("boot.readyz_timeout_secs")
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
            auto_seed_model: config
                .get("boot.auto_seed_model")
                .and_then(|v| parse_bool(v).ok())
                .unwrap_or(false),
            verify_chat: config
                .get("boot.verify_chat")
                .and_then(|v| parse_bool(v).ok())
                .unwrap_or(false),
        }
    }

    // Helper to parse duration strings to seconds
    fn parse_duration_secs(value: &str) -> Option<u64> {
        // Handle "30s", "5m", "1h" etc
        let value = value.trim();
        if let Ok(secs) = value.parse::<u64>() {
            return Some(secs);
        }
        if let Some(stripped) = value.strip_suffix("ms") {
            return stripped.trim().parse::<u64>().ok().map(|ms| ms / 1000);
        }
        if let Some(stripped) = value.strip_suffix('s') {
            return stripped.trim().parse().ok();
        }
        if let Some(stripped) = value.strip_suffix('m') {
            return stripped.trim().parse::<u64>().ok().map(|m| m * 60);
        }
        if let Some(stripped) = value.strip_suffix('h') {
            return stripped.trim().parse::<u64>().ok().map(|h| h * 3600);
        }
        None
    }

    // Helper to parse duration strings to hours
    fn parse_duration_hours(value: &str) -> Option<u32> {
        Self::parse_duration_secs(value).map(|s| (s / 3600) as u32)
    }
}

/// Initialize the global effective configuration
///
/// Call this once at startup. The config will be frozen and immutable after initialization.
///
/// # Arguments
/// * `toml_path` - Optional path to the TOML config file (e.g., "configs/cp.toml")
/// * `cli_args` - CLI arguments in `--key value` format
///
/// # Example
/// ```rust,ignore
/// init_effective_config(Some("configs/cp.toml"), vec![])?;
/// ```
pub fn init_effective_config(
    toml_path: Option<&str>,
    cli_args: Vec<String>,
) -> Result<&'static EffectiveConfig> {
    if let Some(existing) = EFFECTIVE_CONFIG.get() {
        return Ok(existing);
    }

    // Load .env file first
    crate::model::load_dotenv();
    ConfigGuards::initialize()?;

    // Load via ConfigLoader with precedence: CLI > ENV > TOML > defaults
    let loader = ConfigLoader::with_options(LoaderOptions {
        allow_unknown_keys: true,
        ..LoaderOptions::default()
    });
    let config = loader.load(cli_args, toml_path.map(String::from))?;

    // Build EffectiveConfig from DeterministicConfig
    let effective = EffectiveConfig::from_deterministic(config)?;

    if EFFECTIVE_CONFIG.set(effective).is_err() {
        if let Some(existing) = EFFECTIVE_CONFIG.get() {
            return Ok(existing);
        }
        return Err(AosError::Config(
            "EffectiveConfig already initialized".to_string(),
        ));
    }

    // Prevent further environment access after init
    ConfigGuards::freeze()?;

    // SAFETY: We just successfully set the config above, so get() cannot fail
    EFFECTIVE_CONFIG.get().ok_or_else(|| {
        AosError::Config(
            "BUG: EffectiveConfig.get() failed immediately after successful set()".to_string(),
        )
    })
}

/// Get the global effective configuration
///
/// Returns an error if `init_effective_config` hasn't been called yet.
pub fn effective_config() -> Result<&'static EffectiveConfig> {
    EFFECTIVE_CONFIG.get().ok_or_else(|| {
        AosError::Config(
            "EffectiveConfig not initialized. Call init_effective_config() first.".to_string(),
        )
    })
}

/// Try to get the global effective configuration (returns None if not initialized)
pub fn try_effective_config() -> Option<&'static EffectiveConfig> {
    EFFECTIVE_CONFIG.get()
}

/// Check if effective configuration is initialized
pub fn is_effective_initialized() -> bool {
    EFFECTIVE_CONFIG.get().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Valid 64-char JWT secret for production tests (no placeholder patterns)
    const VALID_JWT_SECRET: &str =
        "a9b8c7d6e5f4a3b2c1d0e9f8a7b6c5d4e3f2a1b0c9d8e7f6a5b4c3d2e1f0a9b8";

    #[test]
    fn test_parse_duration_secs() {
        assert_eq!(EffectiveConfig::parse_duration_secs("30"), Some(30));
        assert_eq!(EffectiveConfig::parse_duration_secs("30s"), Some(30));
        assert_eq!(EffectiveConfig::parse_duration_secs("5m"), Some(300));
        assert_eq!(EffectiveConfig::parse_duration_secs("1h"), Some(3600));
        assert_eq!(EffectiveConfig::parse_duration_secs("500ms"), Some(0)); // 500ms = 0s
        assert_eq!(EffectiveConfig::parse_duration_secs("invalid"), None);
    }

    #[test]
    fn test_config_value_source_display() {
        assert_eq!(
            ConfigValueSource::Toml("cp.toml".to_string()).to_string(),
            "toml:cp.toml"
        );
        assert_eq!(
            ConfigValueSource::Environment("AOS_SERVER_PORT".to_string()).to_string(),
            "env:AOS_SERVER_PORT"
        );
        assert_eq!(ConfigValueSource::Cli.to_string(), "cli");
        assert_eq!(ConfigValueSource::Default.to_string(), "default");
    }

    #[test]
    fn coreml_section_uses_defaults_when_unset() {
        use crate::precedence::DeterministicConfig;
        use std::collections::HashMap;

        let empty = DeterministicConfig::new_for_test(HashMap::new());
        let effective =
            EffectiveConfig::from_deterministic(empty).expect("default config should build");

        assert_eq!(
            effective.coreml.compute_preference,
            CoreMLComputePreference::CpuAndGpu
        );
        assert!(
            !effective.coreml.production_mode,
            "production mode should default to false"
        );
    }

    #[test]
    fn coreml_section_applies_config_overrides() {
        use crate::precedence::DeterministicConfig;
        use std::collections::HashMap;

        let mut values = HashMap::new();
        values.insert(
            "coreml.compute_preference".to_string(),
            "cpu_and_ne".to_string(),
        );
        values.insert("coreml.production_mode".to_string(), "true".to_string());

        let config = DeterministicConfig::new_for_test(values);
        let effective =
            EffectiveConfig::from_deterministic(config).expect("coreml config should build");

        assert_eq!(
            effective.coreml.compute_preference,
            CoreMLComputePreference::CpuAndNe
        );
        assert!(effective.coreml.production_mode);
    }

    #[test]
    fn test_production_path_validation() {
        use crate::precedence::DeterministicConfig;
        use std::collections::HashMap;

        // Test case 1: Development mode with relative paths - should pass
        let mut dev_values = HashMap::new();
        dev_values.insert("server.production.mode".to_string(), "false".to_string());
        dev_values.insert(
            "database.url".to_string(),
            "sqlite://var/aos-cp.sqlite3".to_string(),
        );
        dev_values.insert("adapters.dir".to_string(), "var/adapters/repo".to_string());
        dev_values.insert(
            "paths.datasets.root".to_string(),
            "var/datasets".to_string(),
        );
        dev_values.insert(
            "paths.documents.root".to_string(),
            "var/documents".to_string(),
        );
        dev_values.insert("log.file".to_string(), "var/logs".to_string());

        let dev_config = DeterministicConfig::new_for_test(dev_values);
        let dev_effective = EffectiveConfig::from_deterministic(dev_config);
        assert!(
            dev_effective.is_ok(),
            "Development mode should accept relative paths"
        );

        // Test case 2: Production mode with absolute paths - should pass
        let mut prod_abs_values = HashMap::new();
        prod_abs_values.insert("server.production.mode".to_string(), "true".to_string());
        prod_abs_values.insert(
            "database.url".to_string(),
            "sqlite:///adapter-os/var/aos-cp.sqlite3".to_string(),
        );
        prod_abs_values.insert(
            "adapters.dir".to_string(),
            "/adapter-os/var/adapters/repo".to_string(),
        );
        prod_abs_values.insert(
            "paths.datasets.root".to_string(),
            "/adapter-os/var/datasets".to_string(),
        );
        prod_abs_values.insert(
            "paths.documents.root".to_string(),
            "/adapter-os/var/documents".to_string(),
        );
        prod_abs_values.insert("log.file".to_string(), "/adapter-os/var/logs".to_string());
        prod_abs_values.insert(
            "security.jwt.secret".to_string(),
            VALID_JWT_SECRET.to_string(),
        );
        prod_abs_values.insert("inference.backend.profile".to_string(), "metal".to_string());

        let prod_abs_config = DeterministicConfig::new_for_test(prod_abs_values);
        let prod_abs_effective = EffectiveConfig::from_deterministic(prod_abs_config);
        assert!(
            prod_abs_effective.is_ok(),
            "Production mode should accept absolute paths"
        );

        // Test case 3: Production mode with relative database URL - should fail
        // Note: Path configs (adapters.dir, paths.*) are auto-rebased to absolute via
        // rebase_var_path, but database.url is a URL string that keeps relative paths.
        let mut prod_rel_db_values = HashMap::new();
        prod_rel_db_values.insert("server.production.mode".to_string(), "true".to_string());
        prod_rel_db_values.insert(
            "database.url".to_string(),
            "sqlite://var/aos-cp.sqlite3".to_string(), // Relative path in URL
        );
        prod_rel_db_values.insert(
            "security.jwt.secret".to_string(),
            VALID_JWT_SECRET.to_string(),
        );
        prod_rel_db_values.insert("inference.backend.profile".to_string(), "metal".to_string());

        let prod_rel_db_config = DeterministicConfig::new_for_test(prod_rel_db_values);
        let prod_rel_db_effective = EffectiveConfig::from_deterministic(prod_rel_db_config);
        assert!(
            prod_rel_db_effective.is_err(),
            "Production mode should reject relative database URL"
        );

        let err = prod_rel_db_effective.unwrap_err();
        let err_msg = format!("{}", err);
        assert!(
            err_msg.contains("Production mode requires absolute path"),
            "Error should mention production mode"
        );
        assert!(
            err_msg.contains("database.url"),
            "Error should mention database.url"
        );
        assert!(err_msg.contains("Hint:"), "Error should include hints");

        // Test case 4: Production mode with auto-rebased paths - should pass
        // Path configs are auto-rebased to absolute, so this should now succeed.
        let mut prod_rebased_values = HashMap::new();
        prod_rebased_values.insert("server.production.mode".to_string(), "true".to_string());
        prod_rebased_values.insert(
            "database.url".to_string(),
            "sqlite:///adapter-os/var/aos-cp.sqlite3".to_string(), // Absolute
        );
        prod_rebased_values.insert("adapters.dir".to_string(), "var/adapters/repo".to_string()); // Auto-rebased
        prod_rebased_values.insert(
            "paths.datasets.root".to_string(),
            "var/datasets".to_string(), // Auto-rebased
        );
        prod_rebased_values.insert(
            "paths.documents.root".to_string(),
            "var/documents".to_string(), // Auto-rebased
        );
        prod_rebased_values.insert(
            "security.jwt.secret".to_string(),
            VALID_JWT_SECRET.to_string(),
        );
        prod_rebased_values.insert("inference.backend.profile".to_string(), "metal".to_string());

        let prod_rebased_config = DeterministicConfig::new_for_test(prod_rebased_values);
        let prod_rebased_effective = EffectiveConfig::from_deterministic(prod_rebased_config);
        assert!(
            prod_rebased_effective.is_ok(),
            "Production mode should accept auto-rebased paths"
        );
    }

    #[test]
    fn health_section_uses_defaults_when_unset() {
        use crate::precedence::DeterministicConfig;
        use std::collections::HashMap;

        let empty = DeterministicConfig::new_for_test(HashMap::new());
        let effective =
            EffectiveConfig::from_deterministic(empty).expect("default config should build");

        assert!((effective.health.adapter.drift_hard_threshold - 0.15).abs() < f64::EPSILON);
        assert!((effective.health.adapter.high_tier_block_threshold - 0.10).abs() < f64::EPSILON);
    }

    #[test]
    fn diagnostics_section_uses_stale_batch_defaults_and_overrides() {
        use crate::precedence::DeterministicConfig;
        use std::collections::HashMap;

        let empty = DeterministicConfig::new_for_test(HashMap::new());
        let default_effective =
            EffectiveConfig::from_deterministic(empty).expect("default config should build");
        assert_eq!(default_effective.diagnostics.stale_batch_max_attempts, 5);
        assert_eq!(default_effective.diagnostics.stale_batch_max_age_secs, 300);

        let mut values = HashMap::new();
        values.insert("diag.stale_batch_max_attempts".to_string(), "9".to_string());
        values.insert(
            "diag.stale_batch_max_age_secs".to_string(),
            "42".to_string(),
        );
        let overridden = DeterministicConfig::new_for_test(values);
        let overridden_effective = EffectiveConfig::from_deterministic(overridden)
            .expect("diagnostics override config should build");
        assert_eq!(overridden_effective.diagnostics.stale_batch_max_attempts, 9);
        assert_eq!(
            overridden_effective.diagnostics.stale_batch_max_age_secs,
            42
        );
    }

    #[test]
    fn health_section_applies_overrides() {
        use crate::precedence::DeterministicConfig;
        use std::collections::HashMap;

        let mut values = HashMap::new();
        values.insert(
            "health.adapter.drift_hard_threshold".to_string(),
            "0.25".to_string(),
        );
        values.insert(
            "health.adapter.high_tier_block_threshold".to_string(),
            "0.2".to_string(),
        );

        let config = DeterministicConfig::new_for_test(values);
        let effective =
            EffectiveConfig::from_deterministic(config).expect("health config should build");

        assert!((effective.health.adapter.drift_hard_threshold - 0.25).abs() < f64::EPSILON);
        assert!((effective.health.adapter.high_tier_block_threshold - 0.2).abs() < f64::EPSILON);
    }

    /// Helper to build a minimal valid production config
    fn minimal_production_config() -> HashMap<String, String> {
        let mut values = HashMap::new();
        values.insert("server.production.mode".to_string(), "true".to_string());
        values.insert(
            "database.url".to_string(),
            "sqlite:///adapter-os/var/aos-cp.sqlite3".to_string(),
        );
        values.insert(
            "adapters.dir".to_string(),
            "/adapter-os/var/adapters/repo".to_string(),
        );
        values.insert(
            "paths.datasets.root".to_string(),
            "/adapter-os/var/datasets".to_string(),
        );
        values.insert(
            "paths.documents.root".to_string(),
            "/adapter-os/var/documents".to_string(),
        );
        values.insert(
            "security.jwt.secret".to_string(),
            VALID_JWT_SECRET.to_string(),
        );
        values.insert("inference.backend.profile".to_string(), "metal".to_string());
        values.insert("inference.seed.mode".to_string(), "strict".to_string());
        values
    }

    #[test]
    fn test_production_requires_jwt_secret_min_length() {
        use crate::precedence::DeterministicConfig;

        let mut values = minimal_production_config();
        // Use a short secret (11 chars, well under 64)
        values.insert(
            "security.jwt.secret".to_string(),
            "short-secret".to_string(),
        );

        let config = DeterministicConfig::new_for_test(values);
        let result = EffectiveConfig::from_deterministic(config);

        assert!(
            result.is_err(),
            "Short JWT secret should be rejected in production"
        );
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("minimum 64") || err_msg.contains("at least 64"),
            "Error should mention minimum length requirement: {}",
            err_msg
        );
    }

    #[test]
    fn test_production_rejects_dev_bypass() {
        use crate::precedence::DeterministicConfig;

        let mut values = minimal_production_config();
        values.insert("security.dev.bypass".to_string(), "true".to_string());

        let config = DeterministicConfig::new_for_test(values);
        let result = EffectiveConfig::from_deterministic(config);

        assert!(
            result.is_err(),
            "dev_bypass=true should be rejected in production"
        );
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("dev_bypass=false"),
            "Error should mention dev_bypass requirement: {}",
            err_msg
        );
    }

    #[test]
    fn test_production_rejects_dev_login_enabled() {
        use crate::precedence::DeterministicConfig;

        let mut values = minimal_production_config();
        values.insert("security.dev.login.enabled".to_string(), "true".to_string());

        let config = DeterministicConfig::new_for_test(values);
        let result = EffectiveConfig::from_deterministic(config);

        assert!(
            result.is_err(),
            "dev_login_enabled=true should be rejected in production"
        );
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("dev_login_enabled=false"),
            "Error should mention dev_login_enabled requirement: {}",
            err_msg
        );
    }

    #[test]
    fn test_production_accepts_valid_security_config() {
        use crate::precedence::DeterministicConfig;

        let mut values = minimal_production_config();
        // Explicitly set all security settings to safe values
        values.insert("security.dev.bypass".to_string(), "false".to_string());
        values.insert(
            "security.dev.login.enabled".to_string(),
            "false".to_string(),
        );
        values.insert("security.pf.deny".to_string(), "true".to_string());

        let config = DeterministicConfig::new_for_test(values);
        let result = EffectiveConfig::from_deterministic(config);

        assert!(
            result.is_ok(),
            "Valid security config should be accepted: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_development_mode_allows_short_jwt_secret() {
        use crate::precedence::DeterministicConfig;

        let mut values = HashMap::new();
        values.insert("server.production.mode".to_string(), "false".to_string());
        values.insert("security.jwt.secret".to_string(), "short".to_string());

        let config = DeterministicConfig::new_for_test(values);
        let result = EffectiveConfig::from_deterministic(config);

        assert!(
            result.is_ok(),
            "Development mode should allow short JWT secret"
        );
    }

    #[test]
    fn test_development_mode_allows_dev_bypass() {
        use crate::precedence::DeterministicConfig;

        let mut values = HashMap::new();
        values.insert("server.production.mode".to_string(), "false".to_string());
        values.insert("security.dev.bypass".to_string(), "true".to_string());

        let config = DeterministicConfig::new_for_test(values);
        let result = EffectiveConfig::from_deterministic(config);

        // In debug builds, dev_bypass should be allowed in development mode
        #[cfg(debug_assertions)]
        assert!(
            result.is_ok(),
            "Development mode should allow dev_bypass in debug builds"
        );

        // In release builds, dev_bypass is never allowed
        #[cfg(not(debug_assertions))]
        assert!(
            result.is_err(),
            "dev_bypass should be rejected in release builds"
        );
    }

    #[test]
    fn test_security_section_debug_redacts_secrets() {
        let section = SecuritySection {
            jwt_secret: "super-secret-token-value".to_string(),
            jwt_mode: "hmac".to_string(),
            jwt_ttl_hours: 8,
            require_pf_deny: false,
            dev_login_enabled: false,
            dev_bypass: false,
            signing_key: Some("private-signing-key".to_string()),
        };

        let debug_output = format!("{:?}", section);

        // Sensitive fields should be redacted
        assert!(
            !debug_output.contains("super-secret-token-value"),
            "JWT secret should be redacted in debug output"
        );
        assert!(
            !debug_output.contains("private-signing-key"),
            "Signing key should be redacted in debug output"
        );
        assert!(
            debug_output.contains("[REDACTED]"),
            "Debug output should contain [REDACTED] placeholders"
        );

        // Non-sensitive fields should be visible
        assert!(
            debug_output.contains("hmac"),
            "jwt_mode should be visible in debug output"
        );
        assert!(
            debug_output.contains("jwt_ttl_hours"),
            "jwt_ttl_hours should be visible in debug output"
        );
    }

    #[test]
    fn test_metrics_section_debug_redacts_bearer_token() {
        let section = MetricsSection {
            enabled: true,
            bearer_token: "secret-bearer-token".to_string(),
            include_histogram: true,
        };

        let debug_output = format!("{:?}", section);

        // Bearer token should be redacted
        assert!(
            !debug_output.contains("secret-bearer-token"),
            "Bearer token should be redacted in debug output"
        );
        assert!(
            debug_output.contains("[REDACTED]"),
            "Debug output should contain [REDACTED] placeholder"
        );

        // Non-sensitive fields should be visible
        assert!(
            debug_output.contains("enabled"),
            "enabled should be visible in debug output"
        );
        assert!(
            debug_output.contains("include_histogram"),
            "include_histogram should be visible in debug output"
        );
    }

    #[test]
    fn test_metrics_section_debug_shows_empty_token() {
        let section = MetricsSection {
            enabled: true,
            bearer_token: String::new(),
            include_histogram: false,
        };

        let debug_output = format!("{:?}", section);

        // Empty token should show (empty) instead of [REDACTED]
        assert!(
            debug_output.contains("(empty)"),
            "Empty bearer token should show (empty) in debug output"
        );
    }

    #[test]
    fn test_jwt_secret_generates_when_missing() {
        use crate::precedence::DeterministicConfig;

        // Config with no JWT secret set
        let values = HashMap::new();
        let config = DeterministicConfig::new_for_test(values);
        let result = EffectiveConfig::from_deterministic(config);

        assert!(result.is_ok(), "Should succeed even without JWT secret");
        let effective = result.unwrap();
        assert!(
            !effective.security.jwt_secret.is_empty(),
            "JWT secret should be generated when missing"
        );
        assert!(
            effective.security.jwt_secret.starts_with("GENERATED-"),
            "Generated JWT secret should have GENERATED- prefix"
        );
    }
}
