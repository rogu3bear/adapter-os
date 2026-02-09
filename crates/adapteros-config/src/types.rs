//! Configuration types and structures

use crate::path_resolver::DEV_MODEL_PATH;
use adapteros_core::defaults::DEFAULT_DB_PATH;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// Consolidated Business Logic Structs (formerly adapteros-config-types)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub port: u16,
    #[serde(default = "default_bind")]
    pub bind: String,
    #[serde(default)]
    pub production_mode: bool,
    #[serde(default)]
    pub uds_socket: Option<String>,
    /// Optional webhook URL invoked after a review is successfully submitted.
    ///
    /// This is a best-effort notification mechanism (fire-and-forget).
    #[serde(default)]
    pub review_webhook_url: Option<String>,
    /// Timeout in seconds for draining in-flight requests during shutdown (default: 30)
    #[serde(default = "default_drain_timeout")]
    pub drain_timeout_secs: u64,
    /// Timeout in seconds for the entire boot sequence (default: 300)
    #[serde(default = "default_boot_timeout")]
    pub boot_timeout_secs: u64,
    /// Timeout in milliseconds for health check database probe (default: 2000)
    #[serde(default = "default_health_check_db_timeout_ms")]
    pub health_check_db_timeout_ms: u64,
    /// Timeout in milliseconds for health check worker probe (default: 2000)
    #[serde(default = "default_health_check_worker_timeout_ms")]
    pub health_check_worker_timeout_ms: u64,
    /// Timeout in milliseconds for health check models probe (default: 2000)
    #[serde(default = "default_health_check_models_timeout_ms")]
    pub health_check_models_timeout_ms: u64,
    /// Skip worker readiness check in /readyz endpoint (default: false)
    /// When true, the control plane can report ready without worker connectivity.
    /// Useful for deployments where control plane starts independently of workers.
    #[serde(default = "default_false")]
    pub skip_worker_check: bool,
    /// Expected heartbeat interval for workers (default: 30s)
    #[serde(default = "default_worker_heartbeat_interval_secs")]
    pub worker_heartbeat_interval_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub path: String,
    #[serde(default = "default_pool_size")]
    pub pool_size: u32,
    #[serde(default = "default_storage_mode")]
    pub storage_mode: String,
    #[serde(default = "default_kv_path")]
    pub kv_path: String,
    #[serde(default)]
    pub kv_tantivy_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(default = "default_true")]
    pub require_pf_deny: bool,
    #[serde(default = "default_false")]
    pub mtls_required: bool,
    pub jwt_secret: String,
    #[serde(default = "default_jwt_ttl_hours")]
    pub jwt_ttl_hours: u32,
    #[serde(default = "default_key_provider_mode")]
    pub key_provider_mode: String,
    #[serde(default)]
    pub key_file_path: Option<String>,
    #[serde(default = "default_jwt_issuer")]
    pub jwt_issuer: String,
    #[serde(default)]
    pub jwt_audience: Option<String>,
    /// Enable dev login bypass (defaults to false for security)
    #[serde(default = "default_false")]
    pub dev_login_enabled: bool,
    /// MFA requirement (defaults to false)
    #[serde(default)]
    pub require_mfa: Option<bool>,
    /// Token TTL in seconds (defaults to 8 hours). Legacy, superseded by access/session TTLs.
    #[serde(default)]
    pub token_ttl_seconds: Option<u64>,
    /// Access token TTL in seconds (short-lived; defaults to 15 minutes)
    #[serde(default = "default_access_token_ttl_seconds")]
    pub access_token_ttl_seconds: u64,
    /// Session/cookie TTL in seconds (defaults to 12 hours)
    #[serde(default = "default_session_ttl_seconds")]
    pub session_ttl_seconds: u64,
    /// JWT algorithm mode (eddsa or hs256)
    #[serde(default)]
    pub jwt_mode: Option<String>,
    /// Additional Ed25519 public keys (PEM) accepted for JWT verification
    #[serde(default)]
    pub jwt_additional_ed25519_public_keys: Option<Vec<String>>,
    /// Additional HMAC secrets accepted for JWT verification
    #[serde(default)]
    pub jwt_additional_hmac_secrets: Option<Vec<String>>,
    /// Cookie SameSite policy: "Lax", "Strict", or "None" (default: Lax)
    #[serde(default = "default_cookie_same_site")]
    pub cookie_same_site: String,
    /// Optional cookie domain for split-origin dev setups
    #[serde(default)]
    pub cookie_domain: Option<String>,
    /// Force Secure flag on cookies (defaults to true in production_mode)
    #[serde(default)]
    pub cookie_secure: Option<bool>,
    /// Clock skew tolerance in seconds for token/session validation (default: 60)
    #[serde(default = "default_clock_skew_seconds")]
    pub clock_skew_seconds: u64,
    /// Enable dev auth bypass - skip all authentication (debug builds only)
    #[serde(default = "default_false")]
    pub dev_bypass: bool,
    /// Allow user self-registration (defaults to false)
    #[serde(default)]
    pub allow_registration: Option<bool>,
    /// Allowed CI attestation public keys (hex or PEM). Required for CI attestation verification.
    #[serde(default)]
    pub ci_attestation_public_keys: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuthConfig {
    /// JWT algorithm to use in development (hs256/hmac)
    #[serde(default = "default_dev_algo")]
    pub dev_algo: String,
    /// JWT algorithm to use in production (eddsa/ed25519)
    #[serde(default = "default_prod_algo")]
    pub prod_algo: String,
    /// Session lifetime in seconds
    #[serde(default = "default_session_ttl_seconds")]
    pub session_lifetime: u64,
    /// Failed attempts before lockout
    #[serde(default = "default_lockout_threshold")]
    pub lockout_threshold: u32,
    /// Lockout cooldown in seconds
    #[serde(default = "default_lockout_cooldown")]
    pub lockout_cooldown: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathsConfig {
    pub artifacts_root: String,
    pub bundles_root: String,
    #[serde(default = "default_adapters_root")]
    pub adapters_root: String,
    #[serde(default = "default_plan_dir")]
    pub plan_dir: String,
    #[serde(default = "default_datasets_root")]
    pub datasets_root: String,
    #[serde(default = "default_documents_root")]
    pub documents_root: String,
    /// Path to synthesis model for training data generation
    #[serde(default)]
    pub synthesis_model_path: Option<String>,
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

pub fn default_drain_timeout() -> u64 {
    30
}

pub fn default_boot_timeout() -> u64 {
    300
}

pub fn default_health_check_db_timeout_ms() -> u64 {
    2000
}

pub fn default_health_check_worker_timeout_ms() -> u64 {
    2000
}

pub fn default_health_check_models_timeout_ms() -> u64 {
    2000
}

pub fn default_worker_heartbeat_interval_secs() -> u64 {
    30
}

pub fn default_bind() -> String {
    "127.0.0.1".to_string()
}

pub fn default_storage_mode() -> String {
    "sql_only".to_string()
}

pub fn default_pool_size() -> u32 {
    20
}

pub fn default_kv_path() -> String {
    "var/aos-kv.redb".to_string()
}

pub fn default_true() -> bool {
    true
}

pub fn default_false() -> bool {
    false
}

pub fn default_jwt_ttl_hours() -> u32 {
    8
}

pub fn default_key_provider_mode() -> String {
    "keychain".to_string()
}

pub fn default_jwt_issuer() -> String {
    "adapteros".to_string()
}

pub fn default_access_token_ttl_seconds() -> u64 {
    15 * 60
}

pub fn default_session_ttl_seconds() -> u64 {
    12 * 3600
}

pub fn default_cookie_same_site() -> String {
    "Lax".to_string()
}

pub fn default_clock_skew_seconds() -> u64 {
    60
}

pub fn default_dev_algo() -> String {
    "hs256".to_string()
}

pub fn default_prod_algo() -> String {
    "eddsa".to_string()
}

pub fn default_lockout_threshold() -> u32 {
    5
}

pub fn default_lockout_cooldown() -> u64 {
    300
}

pub fn default_adapters_root() -> String {
    "var/adapters/repo".to_string()
}

pub fn default_plan_dir() -> String {
    "plan".to_string()
}

pub fn default_datasets_root() -> String {
    "var/datasets".to_string()
}

pub fn default_documents_root() -> String {
    "var/documents".to_string()
}

// ============================================================================
// Circuit Breaker Configuration
// ============================================================================

/// Configuration for circuit breaker behavior.
///
/// Circuit breakers protect the system from cascading failures by temporarily
/// disabling operations that are repeatedly failing. When the failure threshold
/// is reached, the circuit "opens" and requests are rejected immediately without
/// attempting the operation. After a reset timeout, the circuit enters "half-open"
/// state where a limited number of requests are allowed through to test recovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before the circuit breaker opens
    /// Default: 5
    #[serde(default = "default_failure_threshold")]
    pub failure_threshold: u32,

    /// Time in seconds to wait before attempting recovery (reset timeout)
    /// Default: 60
    #[serde(default = "default_reset_timeout_secs")]
    pub reset_timeout_secs: u64,

    /// Maximum number of calls allowed in half-open state before deciding
    /// whether to close (success) or re-open (failure) the circuit
    /// Default: 3
    #[serde(default = "default_half_open_max_calls")]
    pub half_open_max_calls: u32,

    /// Deadline in seconds for worker operations before considering them failed
    /// This is used to timeout operations that hang indefinitely
    /// Default: 600 (10 minutes)
    #[serde(default = "default_worker_deadline_secs")]
    pub worker_deadline_secs: u64,

    /// Enable automatic fallback to stub mode when circuit is open
    /// Default: true
    #[serde(default = "default_true")]
    pub enable_stub_fallback: bool,

    /// Health check interval in seconds when circuit is open
    /// Default: 30
    #[serde(default = "default_health_check_interval_secs")]
    pub health_check_interval_secs: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: default_failure_threshold(),
            reset_timeout_secs: default_reset_timeout_secs(),
            half_open_max_calls: default_half_open_max_calls(),
            worker_deadline_secs: default_worker_deadline_secs(),
            enable_stub_fallback: true,
            health_check_interval_secs: default_health_check_interval_secs(),
        }
    }
}

pub fn default_failure_threshold() -> u32 {
    5
}

pub fn default_reset_timeout_secs() -> u64 {
    60
}

pub fn default_half_open_max_calls() -> u32 {
    3
}

pub fn default_worker_deadline_secs() -> u64 {
    600
}

pub fn default_health_check_interval_secs() -> u64 {
    30
}

// ============================================================================
// Model Server Configuration
// ============================================================================

/// Configuration for Model Server mode (shared model inference).
///
/// When enabled, workers connect to a shared Model Server instead of loading
/// the model locally. This reduces GPU memory usage by ~65% for multi-worker
/// deployments (one model copy instead of N workers × model).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelServerConfig {
    /// Enable Model Server mode (workers connect to shared server)
    /// Default: false (workers load models directly - legacy mode)
    #[serde(default)]
    pub enabled: bool,

    /// gRPC server address (e.g., "http://127.0.0.1:50051")
    /// Default: "http://127.0.0.1:50051"
    #[serde(default = "default_model_server_addr")]
    pub server_addr: String,

    /// Maximum number of KV cache sessions (conversation contexts)
    /// Default: 32
    #[serde(default = "default_max_kv_cache_sessions")]
    pub max_kv_cache_sessions: u32,

    /// Hot adapter promotion threshold (0.0 to 1.0)
    /// Adapters with activation rate above this threshold are cached in Model Server
    /// Default: 0.10 (10%)
    #[serde(default = "default_hot_adapter_threshold")]
    pub hot_adapter_threshold: f32,

    /// KV cache memory limit in MB (0 = automatic based on available GPU memory)
    /// Default: 0 (automatic)
    #[serde(default)]
    pub kv_cache_limit_mb: u64,
}

impl Default for ModelServerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            server_addr: default_model_server_addr(),
            max_kv_cache_sessions: default_max_kv_cache_sessions(),
            hot_adapter_threshold: default_hot_adapter_threshold(),
            kv_cache_limit_mb: 0,
        }
    }
}

/// Default Model Server gRPC address.
/// Returns "http://127.0.0.1:50051".
pub fn default_model_server_addr() -> String {
    "http://127.0.0.1:50051".to_string()
}

/// Resolve Model Server address with environment variable override.
///
/// Checks `AOS_MODEL_SERVER_ADDR` environment variable first, falling back
/// to the default address if not set. This is useful for containerized
/// deployments where the Model Server address is injected via environment.
///
/// # Example
///
/// ```bash
/// # Override in container deployment
/// export AOS_MODEL_SERVER_ADDR=http://model-server.internal:50051
/// ```
pub fn resolve_model_server_addr() -> String {
    std::env::var("AOS_MODEL_SERVER_ADDR").unwrap_or_else(|_| default_model_server_addr())
}

pub fn default_max_kv_cache_sessions() -> u32 {
    32
}

pub fn default_hot_adapter_threshold() -> f32 {
    0.10
}

// ============================================================================
// Boot Invariants Configuration
// ============================================================================

/// Configuration for boot-time invariant checks.
/// Allows operators to disable specific checks during incidents.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InvariantsConfig {
    /// Explicit acknowledgement to allow disabling invariants in production
    #[serde(default)]
    pub i_understand_security_risk: bool,
    /// Disable SEC-001: Dev auth bypass check (NOT RECOMMENDED)
    #[serde(default)]
    pub disable_sec_001_dev_bypass: bool,
    /// Disable SEC-002: Dual-write strict mode check (NOT RECOMMENDED)
    #[serde(default)]
    pub disable_sec_002_dual_write: bool,
    /// Disable SEC-003: Executor manifest seed check (NOT RECOMMENDED)
    #[serde(default)]
    pub disable_sec_003_executor_seed: bool,
    /// Disable SEC-005: Cookie security check
    #[serde(default)]
    pub disable_sec_005_cookie_security: bool,
    /// Disable SEC-006: JWT algorithm configuration check
    #[serde(default)]
    pub disable_sec_006_jwt_verify: bool,
    /// Disable SEC-015: Signature bypass env var check (NOT RECOMMENDED)
    #[serde(default)]
    pub disable_sec_015_signature_bypass: bool,
    /// Disable DAT-002: Foreign key constraints check
    #[serde(default)]
    pub disable_dat_002_foreign_keys: bool,
    /// Disable DAT-006: Migration ordering check
    #[serde(default)]
    pub disable_dat_006_migration_order: bool,
    /// Disable DAT-007: Audit chain initialization check (FAILS OPEN - warning only)
    #[serde(default)]
    pub disable_dat_007_audit_chain: bool,
    /// Disable LIF-002: Executor initialization check
    #[serde(default)]
    pub disable_lif_002_executor_init: bool,
    /// Disable SEC-008: RBAC permission configuration check
    #[serde(default)]
    pub disable_sec_008_rbac_config: bool,
    /// Disable SEC-014: Brute force protection configuration check
    #[serde(default)]
    pub disable_sec_014_brute_force: bool,
    /// Disable DAT-005: Storage mode enum validation check
    #[serde(default)]
    pub disable_dat_005_storage_mode: bool,
    /// Disable CFG-002: Session TTL hierarchy validation check
    #[serde(default)]
    pub disable_cfg_002_session_ttl: bool,
    /// Disable SEC-007: Tenant isolation configuration check
    #[serde(default)]
    pub disable_sec_007_tenant_isolation: bool,
    /// Disable MEM-003: Memory headroom configuration check
    #[serde(default)]
    pub disable_mem_003_memory_headroom: bool,
    /// Disable LIF-001: Boot phase ordering check (advisory)
    #[serde(default)]
    pub disable_lif_001_boot_ordering: bool,
    /// Disable DAT-001: Archive state machine triggers check (requires DB)
    #[serde(default)]
    pub disable_dat_001_archive_triggers: bool,
    /// Disable LIF-004: Connection pool drain configuration check
    #[serde(default)]
    pub disable_lif_004_pool_drain: bool,
    // =========================================================================
    // Additional Boot Invariants (28 total)
    // =========================================================================
    /// Disable AUTH-001: JWT signing key configured check
    #[serde(default)]
    pub disable_auth_001_jwt_key: bool,
    /// Disable AUTH-002: HMAC secret non-default check
    #[serde(default)]
    pub disable_auth_002_hmac_secret: bool,
    /// Disable AUTH-003: Session store initialized check
    #[serde(default)]
    pub disable_auth_003_session_store: bool,
    /// Disable AUTH-004: JWT secret must not be placeholder check (NOT RECOMMENDED)
    #[serde(default)]
    pub disable_auth_004_jwt_secret_placeholder: bool,
    /// Disable AUTHZ-001: RBAC tables populated check
    #[serde(default)]
    pub disable_authz_001_rbac_tables: bool,
    /// Disable AUTHZ-002: Default admin role defined check
    #[serde(default)]
    pub disable_authz_002_admin_role: bool,
    /// Disable CRYPTO-001: Worker keypair exists check
    #[serde(default)]
    pub disable_crypto_001_worker_keypair: bool,
    /// Disable CRYPTO-002: Entropy source available check
    #[serde(default)]
    pub disable_crypto_002_entropy_source: bool,
    /// Disable CRYPTO-003: Signing algorithm matches config check
    #[serde(default)]
    pub disable_crypto_003_algo_match: bool,
    /// Disable FED-001: Quorum keys loaded check (if federated mode)
    #[serde(default)]
    pub disable_fed_001_quorum_keys: bool,
    /// Disable FED-002: Peer certificates valid check (if federated mode)
    #[serde(default)]
    pub disable_fed_002_peer_certs: bool,
    /// Disable ADAPT-001: Bundle signature verification check
    #[serde(default)]
    pub disable_adapt_001_bundle_sig: bool,
    /// Disable ADAPT-002: Manifest hash verification check
    #[serde(default)]
    pub disable_adapt_002_manifest_hash: bool,
    /// Disable POL-001: Default policy pack loaded check
    #[serde(default)]
    pub disable_pol_001_default_pack: bool,
    /// Disable POL-002: Enforcement mode set check
    #[serde(default)]
    pub disable_pol_002_enforcement_mode: bool,
    // =========================================================================
    // Code Hygiene Invariants
    // =========================================================================
    /// Disable HYGIENE-001: No credentials in repo check (NOT RECOMMENDED)
    #[serde(default)]
    pub disable_hygiene_001_no_credentials: bool,
    /// Disable HYGIENE-002: Critical handlers committed check (warning only)
    #[serde(default)]
    pub disable_hygiene_002_handlers_committed: bool,
    /// Disable HYGIENE-003: Panic density check (warning only)
    #[serde(default)]
    pub disable_hygiene_003_panic_density: bool,
}

// ============================================================================
// Infrastructure Types (Restored from original adapteros-config/types.rs)
// ============================================================================

/// Configuration precedence levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PrecedenceLevel {
    /// Manifest file (lowest priority)
    Manifest = 0,
    /// Environment variables (medium priority)
    Environment = 1,
    /// CLI arguments (highest priority)
    Cli = 2,
}

/// Configuration source information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigFieldSource {
    pub level: PrecedenceLevel,
    pub source: String, // "manifest", "env", "cli"
    pub key: String,
    pub value: String,
}

/// Configuration metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigMetadata {
    pub frozen_at: String, // ISO timestamp
    pub hash: String,      // BLAKE3 hash of frozen config
    pub sources: Vec<ConfigFieldSource>,
    pub manifest_path: Option<String>,
    pub cli_args: Vec<String>,
}

/// Configuration validation error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigValidationError {
    pub key: String,
    pub message: String,
    pub expected_type: String,
    pub actual_value: String,
}

/// Configuration freeze error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigFreezeError {
    pub message: String,
    pub attempted_operation: String,
    pub stack_trace: Option<String>,
}

/// Feature flag definition
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FeatureFlag {
    /// Unique identifier for the feature
    pub name: String,
    /// Whether the feature is enabled
    pub enabled: bool,
    /// Description of the feature
    pub description: Option<String>,
    /// Conditions for automatic enablement
    pub conditions: Option<FeatureFlagConditions>,
}

/// Conditions for automatic feature flag enablement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFlagConditions {
    /// Enable only in specific environments
    pub environments: Option<Vec<String>>,
    /// Enable for specific tenant IDs
    pub tenant_ids: Option<Vec<String>>,
    /// Enable after a specific date (ISO 8601)
    pub enabled_after: Option<String>,
    /// Enable before a specific date (ISO 8601)
    pub enabled_before: Option<String>,
    /// Percentage rollout (0-100)
    pub rollout_percentage: Option<u8>,
}

/// Configuration loader options
#[derive(Debug, Clone)]
pub struct LoaderOptions {
    pub strict_mode: bool,
    pub validate_types: bool,
    pub allow_unknown_keys: bool,
    pub env_prefix: String,
    /// Fail if manifest_path is provided but file is missing or unreadable.
    /// When true, explicitly provided config paths are treated as required.
    pub require_manifest: bool,
    /// Fail on empty/whitespace environment variable overrides in production mode.
    /// When true, empty AOS_* env vars cause an error instead of being silently skipped.
    pub reject_empty_env_vars: bool,
}

impl Default for LoaderOptions {
    fn default() -> Self {
        Self {
            strict_mode: true,
            validate_types: true,
            allow_unknown_keys: false,
            env_prefix: "ADAPTEROS_".to_string(),
            require_manifest: true,
            reject_empty_env_vars: true,
        }
    }
}

/// Configuration schema definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeterministicSchema {
    pub version: String,
    pub fields: HashMap<String, FieldDefinition>,
}

/// Field definition for configuration validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDefinition {
    pub field_type: String, // "string", "integer", "boolean", "float"
    pub required: bool,
    pub default_value: Option<String>,
    pub description: Option<String>,
    pub validation_rules: Option<Vec<String>>,
}

impl Default for DeterministicSchema {
    fn default() -> Self {
        let mut fields = HashMap::new();

        // Core server configuration
        fields.insert(
            "server.host".to_string(),
            FieldDefinition {
                field_type: "string".to_string(),
                required: false,
                default_value: Some("127.0.0.1".to_string()),
                description: Some("Server bind address".to_string()),
                validation_rules: Some(vec!["ip_address".to_string()]),
            },
        );

        fields.insert(
            "server.port".to_string(),
            FieldDefinition {
                field_type: "integer".to_string(),
                required: false,
                default_value: Some("8080".to_string()),
                description: Some("Server port number".to_string()),
                validation_rules: Some(vec!["range:1-65535".to_string()]),
            },
        );

        fields.insert(
            "server.workers".to_string(),
            FieldDefinition {
                field_type: "integer".to_string(),
                required: false,
                default_value: Some("4".to_string()),
                description: Some("Number of worker threads".to_string()),
                validation_rules: Some(vec!["range:1-64".to_string()]),
            },
        );

        // Database configuration
        fields.insert(
            "database.url".to_string(),
            FieldDefinition {
                field_type: "string".to_string(),
                required: true,
                default_value: Some(DEFAULT_DB_PATH.to_string()),
                description: Some("Database connection URL".to_string()),
                validation_rules: Some(vec!["url".to_string()]),
            },
        );

        fields.insert(
            "database.pool_size".to_string(),
            FieldDefinition {
                field_type: "integer".to_string(),
                required: false,
                default_value: Some("20".to_string()),
                description: Some("Database connection pool size".to_string()),
                validation_rules: Some(vec!["range:1-100".to_string()]),
            },
        );

        fields.insert(
            "database.storage_mode".to_string(),
            FieldDefinition {
                field_type: "string".to_string(),
                required: false,
                default_value: Some("sql_only".to_string()),
                description: Some(
                    "Storage mode: sql_only, dual_write, kv_primary, kv_only".to_string(),
                ),
                validation_rules: Some(vec![
                    "enum:sql_only,dual_write,kv_primary,kv_only".to_string()
                ]),
            },
        );

        fields.insert(
            "database.kv_path".to_string(),
            FieldDefinition {
                field_type: "string".to_string(),
                required: false,
                default_value: Some("var/aos-kv.redb".to_string()),
                description: Some("Path to KV (redb) file".to_string()),
                validation_rules: None,
            },
        );

        fields.insert(
            "database.kv_tantivy_path".to_string(),
            FieldDefinition {
                field_type: "string".to_string(),
                required: false,
                default_value: Some("var/aos-kv-index".to_string()),
                description: Some("Path to KV search index (Tantivy)".to_string()),
                validation_rules: None,
            },
        );

        // Policy configuration
        fields.insert(
            "policy.strict_mode".to_string(),
            FieldDefinition {
                field_type: "boolean".to_string(),
                required: false,
                default_value: Some("true".to_string()),
                description: Some("Enable strict policy enforcement".to_string()),
                validation_rules: None,
            },
        );

        fields.insert(
            "policy.audit_logging".to_string(),
            FieldDefinition {
                field_type: "boolean".to_string(),
                required: false,
                default_value: Some("true".to_string()),
                description: Some("Enable policy audit logging".to_string()),
                validation_rules: None,
            },
        );

        // Logging configuration
        fields.insert(
            "logging.level".to_string(),
            FieldDefinition {
                field_type: "string".to_string(),
                required: false,
                default_value: Some("info".to_string()),
                description: Some("Logging level".to_string()),
                validation_rules: Some(vec!["enum:debug,info,warn,error".to_string()]),
            },
        );

        fields.insert(
            "logging.format".to_string(),
            FieldDefinition {
                field_type: "string".to_string(),
                required: false,
                default_value: Some("json".to_string()),
                description: Some("Logging format".to_string()),
                validation_rules: Some(vec!["enum:json,text".to_string()]),
            },
        );

        // Authentication configuration
        fields.insert(
            "auth.dev_algo".to_string(),
            FieldDefinition {
                field_type: "string".to_string(),
                required: false,
                default_value: Some("hs256".to_string()),
                description: Some("JWT algorithm in development (hs256/hmac)".to_string()),
                validation_rules: Some(vec!["enum:hs256,hmac,eddsa,ed25519".to_string()]),
            },
        );

        fields.insert(
            "auth.prod_algo".to_string(),
            FieldDefinition {
                field_type: "string".to_string(),
                required: false,
                default_value: Some("eddsa".to_string()),
                description: Some("JWT algorithm in production (eddsa/ed25519)".to_string()),
                validation_rules: Some(vec!["enum:hs256,hmac,eddsa,ed25519".to_string()]),
            },
        );

        fields.insert(
            "auth.session_lifetime".to_string(),
            FieldDefinition {
                field_type: "integer".to_string(),
                required: false,
                default_value: Some((12 * 3600).to_string()),
                description: Some("Session lifetime in seconds".to_string()),
                validation_rules: Some(vec!["range:60-86400".to_string()]),
            },
        );

        fields.insert(
            "auth.lockout_threshold".to_string(),
            FieldDefinition {
                field_type: "integer".to_string(),
                required: false,
                default_value: Some("5".to_string()),
                description: Some("Failed login attempts before lockout".to_string()),
                validation_rules: Some(vec!["range:1-100".to_string()]),
            },
        );

        fields.insert(
            "auth.lockout_cooldown".to_string(),
            FieldDefinition {
                field_type: "integer".to_string(),
                required: false,
                default_value: Some("300".to_string()),
                description: Some("Lockout cooldown in seconds".to_string()),
                validation_rules: Some(vec!["range:60-86400".to_string()]),
            },
        );

        // Model configuration
        fields.insert(
            "model.path".to_string(),
            FieldDefinition {
                field_type: "string".to_string(),
                required: false,
                default_value: Some(DEV_MODEL_PATH.to_string()),
                description: Some("Path to the model directory".to_string()),
                validation_rules: None,
            },
        );

        fields.insert(
            "model.backend".to_string(),
            FieldDefinition {
                field_type: "string".to_string(),
                required: false,
                default_value: Some("mlx".to_string()),
                description: Some("Model backend selection".to_string()),
                validation_rules: Some(vec!["enum:auto,coreml,metal,mlx".to_string()]),
            },
        );

        fields.insert(
            "model.architecture".to_string(),
            FieldDefinition {
                field_type: "string".to_string(),
                required: false,
                default_value: Some("qwen2.5".to_string()),
                description: Some("Model architecture type".to_string()),
                validation_rules: None,
            },
        );

        Self {
            version: "1.0.0".to_string(),
            fields,
        }
    }
}

// ============================================================================
// Worker Safety Configuration
// ============================================================================

/// Configuration for worker safety mechanisms including timeouts and resource limits.
///
/// Maps to the `[worker.safety]` section in cp.toml. These values control timeout
/// behavior for different worker operations to prevent runaway processes and enforce
/// resource limits.
///
/// # Example (cp.toml)
///
/// ```toml
/// [worker.safety]
/// inference_timeout_secs = 30
/// evidence_timeout_secs = 5
/// router_timeout_ms = 100
/// policy_timeout_ms = 50
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSafetyConfig {
    /// Timeout for inference operations in seconds (default: 30)
    #[serde(default = "default_inference_timeout_secs")]
    pub inference_timeout_secs: u64,

    /// Timeout for evidence collection in seconds (default: 5)
    #[serde(default = "default_evidence_timeout_secs")]
    pub evidence_timeout_secs: u64,

    /// Timeout for router operations in milliseconds (default: 100)
    #[serde(default = "default_router_timeout_ms")]
    pub router_timeout_ms: u64,

    /// Timeout for policy checks in milliseconds (default: 50)
    #[serde(default = "default_policy_timeout_ms")]
    pub policy_timeout_ms: u64,

    /// Circuit breaker failure threshold (default: 5)
    #[serde(default = "default_failure_threshold")]
    pub circuit_breaker_threshold: u32,

    /// Circuit breaker timeout in seconds (default: 60)
    #[serde(default = "default_reset_timeout_secs")]
    pub circuit_breaker_timeout_secs: u64,

    /// Maximum concurrent requests (default: 10)
    #[serde(default = "default_max_concurrent_requests")]
    pub max_concurrent_requests: u32,

    /// Maximum tokens per second (default: 40)
    #[serde(default = "default_max_tokens_per_second")]
    pub max_tokens_per_second: u32,

    /// Maximum memory per request in MB (default: 50)
    #[serde(default = "default_max_memory_per_request_mb")]
    pub max_memory_per_request_mb: u64,

    /// Maximum CPU time per request in seconds (default: 30)
    #[serde(default = "default_max_cpu_time_per_request_secs")]
    pub max_cpu_time_per_request_secs: u64,

    /// Maximum requests per minute (default: 100)
    #[serde(default = "default_max_requests_per_minute")]
    pub max_requests_per_minute: u32,

    /// Health check interval in seconds (default: 30)
    #[serde(default = "default_health_check_interval_secs")]
    pub health_check_interval_secs: u64,

    /// Maximum response time in seconds (default: 60)
    #[serde(default = "default_max_response_time_secs")]
    pub max_response_time_secs: u64,

    /// Maximum memory growth in MB (default: 100)
    #[serde(default = "default_max_memory_growth_mb")]
    pub max_memory_growth_mb: u64,

    /// Maximum CPU time in seconds (default: 300)
    #[serde(default = "default_max_cpu_time_secs")]
    pub max_cpu_time_secs: u64,

    /// Maximum consecutive failures (default: 3)
    #[serde(default = "default_max_consecutive_failures")]
    pub max_consecutive_failures: u32,
}

impl Default for WorkerSafetyConfig {
    fn default() -> Self {
        Self {
            inference_timeout_secs: default_inference_timeout_secs(),
            evidence_timeout_secs: default_evidence_timeout_secs(),
            router_timeout_ms: default_router_timeout_ms(),
            policy_timeout_ms: default_policy_timeout_ms(),
            circuit_breaker_threshold: default_failure_threshold(),
            circuit_breaker_timeout_secs: default_reset_timeout_secs(),
            max_concurrent_requests: default_max_concurrent_requests(),
            max_tokens_per_second: default_max_tokens_per_second(),
            max_memory_per_request_mb: default_max_memory_per_request_mb(),
            max_cpu_time_per_request_secs: default_max_cpu_time_per_request_secs(),
            max_requests_per_minute: default_max_requests_per_minute(),
            health_check_interval_secs: default_health_check_interval_secs(),
            max_response_time_secs: default_max_response_time_secs(),
            max_memory_growth_mb: default_max_memory_growth_mb(),
            max_cpu_time_secs: default_max_cpu_time_secs(),
            max_consecutive_failures: default_max_consecutive_failures(),
        }
    }
}

pub fn default_inference_timeout_secs() -> u64 {
    30
}

pub fn default_evidence_timeout_secs() -> u64 {
    5
}

pub fn default_router_timeout_ms() -> u64 {
    100
}

pub fn default_policy_timeout_ms() -> u64 {
    50
}

pub fn default_max_concurrent_requests() -> u32 {
    10
}

pub fn default_max_tokens_per_second() -> u32 {
    40
}

pub fn default_max_memory_per_request_mb() -> u64 {
    50
}

pub fn default_max_cpu_time_per_request_secs() -> u64 {
    30
}

pub fn default_max_requests_per_minute() -> u32 {
    100
}

pub fn default_max_response_time_secs() -> u64 {
    60
}

pub fn default_max_memory_growth_mb() -> u64 {
    100
}

pub fn default_max_cpu_time_secs() -> u64 {
    300
}

pub fn default_max_consecutive_failures() -> u32 {
    3
}
