use adapteros_core::{BackendKind, DeterminismMode, SeedMode};
use adapteros_crypto::Keypair;
use adapteros_db::git::FileChangeEvent;
use adapteros_db::{sqlx, Db, KvIsolationScanReport, ProtectedDb, WriteCapableDb};
use adapteros_deterministic_exec::global_ledger::GlobalTickLedger;
use adapteros_lora_kernel_api::FusedKernels;
use adapteros_lora_lifecycle::LifecycleManager;
use adapteros_lora_rag::EmbeddingModel;
use adapteros_lora_worker::memory::UmaPressureMonitor;
use adapteros_lora_worker::signal::Signal;
use adapteros_lora_worker::Worker;
use adapteros_orchestrator::{CodeJobManager, FederationDaemon, TrainingService};
use adapteros_policy::PolicyPackManager;
use adapteros_telemetry::{BundleStore, MetricsCollector, RetentionPolicy};

use crate::auth::{derive_kid_from_bytes, derive_kid_from_str};
use crate::boot_state::BootStateManager;
use crate::load_coordinator::LoadCoordinator;
use crate::runtime_mode::RuntimeMode;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, RwLock};
use tokio::sync::{broadcast, Mutex};

use crate::caching::DashboardCache;
use crate::config::PathsConfig;
use crate::handlers::chunked_upload::UploadSessionManager;
use crate::idempotency::IdempotencyStore;
use crate::sse::SseEventManager;
use crate::telemetry::{MetricsRegistry, TelemetryBuffer, TelemetrySender, TraceBuffer};
use adapteros_registry::Registry;

type WorkerHandle = Arc<Mutex<Worker<Box<dyn FusedKernels + Send + Sync>>>>;

/// RAG system status indicating whether embedding model is available
#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone, Default)]
pub struct WorkerRuntimeInfo {
    pub backend: Option<String>,
    pub model_hash: Option<String>,
    pub capabilities: Vec<String>,
    /// Current cache memory usage in MB
    pub cache_used_mb: Option<u32>,
    /// Maximum cache memory budget in MB
    pub cache_max_mb: Option<u32>,
    /// Number of pinned cache entries (cannot be evicted)
    pub cache_pinned_entries: Option<u32>,
    /// Number of active cache entries (in-use, cannot be evicted)
    pub cache_active_entries: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BackgroundTaskEntry {
    pub name: String,
    pub critical: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BackgroundTaskFailure {
    pub name: String,
    pub error: String,
    pub critical: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, utoipa::ToSchema)]
pub struct BackgroundTaskSnapshot {
    #[serde(default)]
    pub spawned: Vec<BackgroundTaskEntry>,
    #[serde(default)]
    pub failed: Vec<BackgroundTaskFailure>,
}

#[derive(Debug, Default)]
pub struct BackgroundTaskTracker {
    tasks: std::sync::RwLock<BTreeMap<String, BackgroundTaskRecord>>,
}

#[derive(Debug, Clone)]
struct BackgroundTaskRecord {
    critical: bool,
    status: BackgroundTaskStatus,
}

#[derive(Debug, Clone)]
enum BackgroundTaskStatus {
    Spawned,
    Failed { error: String },
}

impl BackgroundTaskTracker {
    pub fn record_spawned(&self, name: &str, critical: bool) {
        let mut tasks = self.tasks.write().unwrap_or_else(|e| e.into_inner());
        let entry = tasks
            .entry(name.to_string())
            .or_insert(BackgroundTaskRecord {
                critical,
                status: BackgroundTaskStatus::Spawned,
            });
        entry.critical = entry.critical || critical;
        entry.status = BackgroundTaskStatus::Spawned;
    }

    pub fn record_failed(&self, name: &str, error: &str, critical: bool) {
        let mut tasks = self.tasks.write().unwrap_or_else(|e| e.into_inner());
        let entry = tasks
            .entry(name.to_string())
            .or_insert(BackgroundTaskRecord {
                critical,
                status: BackgroundTaskStatus::Failed {
                    error: error.to_string(),
                },
            });
        entry.critical = entry.critical || critical;
        entry.status = BackgroundTaskStatus::Failed {
            error: error.to_string(),
        };
    }

    pub fn snapshot(&self) -> BackgroundTaskSnapshot {
        let tasks = self.tasks.read().unwrap_or_else(|e| e.into_inner());
        let mut spawned = Vec::new();
        let mut failed = Vec::new();

        for (name, record) in tasks.iter() {
            match &record.status {
                BackgroundTaskStatus::Spawned => spawned.push(BackgroundTaskEntry {
                    name: name.clone(),
                    critical: record.critical,
                }),
                BackgroundTaskStatus::Failed { error } => failed.push(BackgroundTaskFailure {
                    name: name.clone(),
                    error: error.clone(),
                    critical: record.critical,
                }),
            }
        }

        BackgroundTaskSnapshot { spawned, failed }
    }

    pub fn critical_failures(&self) -> Vec<BackgroundTaskFailure> {
        let tasks = self.tasks.read().unwrap_or_else(|e| e.into_inner());
        let mut failures = Vec::new();

        for (name, record) in tasks.iter() {
            if !record.critical {
                continue;
            }
            if let BackgroundTaskStatus::Failed { error } = &record.status {
                failures.push(BackgroundTaskFailure {
                    name: name.clone(),
                    error: error.clone(),
                    critical: record.critical,
                });
            }
        }

        failures
    }
}

/// Capacity limits configuration
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CapacityLimits {
    /// Maximum models per worker
    pub models_per_worker: Option<usize>,
    /// Maximum models per tenant
    pub models_per_tenant: Option<usize>,
    /// Maximum concurrent requests
    pub concurrent_requests: Option<usize>,
    /// Maximum concurrent training jobs (default: 5)
    #[serde(default = "default_max_concurrent_training_jobs")]
    pub max_concurrent_training_jobs: usize,
}

fn default_max_concurrent_training_jobs() -> usize {
    5
}

impl Default for CapacityLimits {
    fn default() -> Self {
        Self {
            models_per_worker: Some(10),
            models_per_tenant: Some(5),
            concurrent_requests: Some(100),
            max_concurrent_training_jobs: 5,
        }
    }
}

/// Runtime configuration subset needed by API handlers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    pub metrics: MetricsConfig,
    /// Timeout in seconds for directory analysis operations (default: 120)
    #[serde(default = "default_directory_analysis_timeout")]
    pub directory_analysis_timeout_secs: u64,
    /// Whether to fall back to session.stack_id when no explicit adapters/stack_id are provided
    #[serde(default)]
    pub use_session_stack_for_routing: bool,
    /// Capacity limits configuration
    #[serde(default)]
    pub capacity_limits: CapacityLimits,
    /// General configuration
    #[serde(default)]
    pub general: Option<GeneralConfig>,
    /// Server configuration
    #[serde(default)]
    pub server: ServerConfigApi,
    /// Security configuration
    #[serde(default)]
    pub security: SecurityConfigApi,
    /// Authentication configuration
    #[serde(default)]
    pub auth: AuthConfigApi,
    /// Self-hosting agent configuration
    #[serde(default)]
    pub self_hosting: SelfHostingConfigApi,
    /// Performance configuration
    #[serde(default)]
    pub performance: PerformanceConfigApi,
    /// Paths configuration for storage locations
    pub paths: PathsConfig,
    /// Chat context configuration for multi-turn conversations
    #[serde(default)]
    pub chat_context: ChatContextConfig,
    /// Seed mode for request-scoped derivation
    #[serde(default)]
    pub seed_mode: SeedMode,
    /// Backend profile to request for execution
    #[serde(default)]
    pub backend_profile: BackendKind,
    /// Worker identifier used in seed derivation
    #[serde(default)]
    pub worker_id: u32,
}

fn default_directory_analysis_timeout() -> u64 {
    120
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub bearer_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SelfHostingConfigApi {
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub repo_allowlist: Vec<String>,
    #[serde(default)]
    pub promotion_threshold: f64,
    #[serde(default)]
    pub require_human_approval: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub system_name: Option<String>,
    pub environment: Option<String>,
    pub api_base_url: Option<String>,
    /// Global default determinism mode (strict, besteffort, relaxed)
    #[serde(default)]
    pub determinism_mode: Option<DeterminismMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServerConfigApi {
    #[serde(default)]
    pub http_port: Option<u16>,
    #[serde(default)]
    pub https_port: Option<u16>,
    #[serde(default)]
    pub uds_socket: Option<String>,
    #[serde(default)]
    pub production_mode: bool,
    /// Timeout in milliseconds for health check database probe (default: 2000)
    #[serde(default = "default_health_check_db_timeout_ms")]
    pub health_check_db_timeout_ms: u64,
    /// Timeout in milliseconds for health check worker probe (default: 2000)
    #[serde(default = "default_health_check_worker_timeout_ms")]
    pub health_check_worker_timeout_ms: u64,
    /// Timeout in milliseconds for health check models probe (default: 2000)
    #[serde(default = "default_health_check_models_timeout_ms")]
    pub health_check_models_timeout_ms: u64,
}

fn default_health_check_db_timeout_ms() -> u64 {
    2000
}

fn default_health_check_worker_timeout_ms() -> u64 {
    2000
}

fn default_health_check_models_timeout_ms() -> u64 {
    2000
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecurityConfigApi {
    #[serde(default)]
    pub jwt_mode: Option<String>,
    #[serde(default)]
    pub token_ttl_seconds: Option<u64>,
    #[serde(default)]
    pub access_token_ttl_seconds: Option<u64>,
    #[serde(default)]
    pub session_ttl_seconds: Option<u64>,
    #[serde(default)]
    pub jwt_additional_ed25519_public_keys: Option<Vec<String>>,
    #[serde(default)]
    pub jwt_additional_hmac_secrets: Option<Vec<String>>,
    #[serde(default)]
    pub require_mfa: Option<bool>,
    #[serde(default)]
    pub require_pf_deny: bool,
    #[serde(default)]
    pub dev_login_enabled: bool,
    #[serde(default)]
    pub cookie_same_site: Option<String>,
    #[serde(default)]
    pub cookie_domain: Option<String>,
    #[serde(default)]
    pub cookie_secure: Option<bool>,
    #[serde(default = "default_clock_skew_seconds")]
    pub clock_skew_seconds: u64,
    /// Dev bypass: skip all authentication (debug builds only)
    #[serde(default)]
    pub dev_bypass: bool,
}

fn default_clock_skew_seconds() -> u64 {
    300 // 5 minutes default
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuthConfigApi {
    #[serde(default)]
    pub dev_algo: String,
    #[serde(default)]
    pub prod_algo: String,
    #[serde(default)]
    pub session_lifetime: u64,
    #[serde(default)]
    pub lockout_threshold: u32,
    #[serde(default)]
    pub lockout_cooldown: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PerformanceConfigApi {
    #[serde(default)]
    pub max_adapters: Option<usize>,
    #[serde(default)]
    pub max_workers: Option<usize>,
    #[serde(default)]
    pub memory_threshold_pct: Option<f64>,
    #[serde(default)]
    pub cache_size_mb: Option<usize>,
}

/// Chat context configuration for multi-turn conversations.
///
/// Controls how chat history is loaded and formatted when building
/// prompts for inference with a `session_id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatContextConfig {
    /// Maximum number of history messages to include (default: 20)
    #[serde(default = "default_max_history_messages")]
    pub max_history_messages: usize,
    /// Maximum token budget for history (default: 4096, ~4 chars/token heuristic)
    #[serde(default = "default_max_history_tokens")]
    pub max_history_tokens: usize,
    /// Whether to include system messages in history (default: true)
    #[serde(default = "default_include_system_messages")]
    pub include_system_messages: bool,
}

fn default_max_history_messages() -> usize {
    20
}

fn default_max_history_tokens() -> usize {
    4096
}

fn default_include_system_messages() -> bool {
    true
}

impl Default for ChatContextConfig {
    fn default() -> Self {
        Self {
            max_history_messages: default_max_history_messages(),
            max_history_tokens: default_max_history_tokens(),
            include_system_messages: default_include_system_messages(),
        }
    }
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            metrics: MetricsConfig {
                enabled: true,
                bearer_token: String::new(),
            },
            directory_analysis_timeout_secs: default_directory_analysis_timeout(),
            use_session_stack_for_routing: false,
            capacity_limits: Default::default(),
            general: None,
            server: Default::default(),
            security: Default::default(),
            auth: Default::default(),
            self_hosting: Default::default(),
            performance: Default::default(),
            paths: crate::config::PathsConfig {
                artifacts_root: "var/artifacts".to_string(),
                bundles_root: "var/bundles".to_string(),
                adapters_root: "var/adapters/repo".to_string(),
                plan_dir: "var/plan".to_string(),
                datasets_root: "var/datasets".to_string(),
                documents_root: "var/documents".to_string(),
            },
            chat_context: Default::default(),
            seed_mode: if cfg!(debug_assertions) {
                SeedMode::BestEffort
            } else {
                SeedMode::Strict
            },
            backend_profile: BackendKind::default_inference_backend(),
            worker_id: 0,
        }
    }
}

/// Cryptographic state for signing and verification
pub struct CryptoState {
    pub signing_keypair: Keypair,
    pub jwt_keypair: Keypair,
}

impl CryptoState {
    pub fn new() -> Self {
        Self::new_with_path("var/keys")
    }

    pub fn new_with_path(keys_dir: &str) -> Self {
        use std::fs;
        use std::path::PathBuf;

        let keys_path = PathBuf::from(keys_dir);
        let jwt_key_path = keys_path.join("jwt_signing.key");
        let policy_key_path = keys_path.join("policy_signing.key");

        // Create keys directory if it doesn't exist
        if !keys_path.exists() {
            if let Err(e) = fs::create_dir_all(&keys_path) {
                tracing::warn!(
                    path = %keys_path.display(),
                    error = %e,
                    "Failed to create keys directory, using ephemeral keys"
                );
                return Self::generate_ephemeral();
            }
        }

        // Try to load existing keys
        let jwt_keypair = match Self::load_key(&jwt_key_path) {
            Ok(keypair) => {
                tracing::info!(
                    path = %jwt_key_path.display(),
                    "Loaded existing JWT signing key"
                );
                keypair
            }
            Err(e) => {
                tracing::warn!(
                    path = %jwt_key_path.display(),
                    error = %e,
                    "Failed to load JWT signing key, generating new key"
                );
                let keypair = Keypair::generate();
                if let Err(save_err) = Self::save_key(&jwt_key_path, &keypair) {
                    tracing::error!(
                        path = %jwt_key_path.display(),
                        error = %save_err,
                        "Failed to save new JWT signing key"
                    );
                }
                keypair
            }
        };

        let signing_keypair = match Self::load_key(&policy_key_path) {
            Ok(keypair) => {
                tracing::info!(
                    path = %policy_key_path.display(),
                    "Loaded existing policy signing key"
                );
                keypair
            }
            Err(e) => {
                tracing::warn!(
                    path = %policy_key_path.display(),
                    error = %e,
                    "Failed to load policy signing key, generating new key"
                );
                let keypair = Keypair::generate();
                if let Err(save_err) = Self::save_key(&policy_key_path, &keypair) {
                    tracing::error!(
                        path = %policy_key_path.display(),
                        error = %save_err,
                        "Failed to save new policy signing key"
                    );
                }
                keypair
            }
        };

        Self {
            signing_keypair,
            jwt_keypair,
        }
    }

    fn generate_ephemeral() -> Self {
        tracing::warn!(
            "Generating ephemeral keys - all existing tokens will be invalidated on restart"
        );
        Self {
            signing_keypair: Keypair::generate(),
            jwt_keypair: Keypair::generate(),
        }
    }

    fn load_key(path: &std::path::Path) -> adapteros_core::Result<Keypair> {
        use std::fs;

        let key_bytes = fs::read(path)
            .map_err(|e| adapteros_core::AosError::Io(format!("Failed to read key file: {}", e)))?;

        if key_bytes.len() != 32 {
            return Err(adapteros_core::AosError::Crypto(format!(
                "Invalid key length: expected 32 bytes, got {}",
                key_bytes.len()
            )));
        }

        let mut key_array = [0u8; 32];
        key_array.copy_from_slice(&key_bytes);

        Ok(Keypair::from_bytes(&key_array))
    }

    fn save_key(path: &std::path::Path, keypair: &Keypair) -> adapteros_core::Result<()> {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let key_bytes = keypair.to_bytes();

        // Write the key to a temporary file first
        let temp_path = path.with_extension("tmp");
        fs::write(&temp_path, key_bytes).map_err(|e| {
            adapteros_core::AosError::Io(format!("Failed to write key file: {}", e))
        })?;

        // Set restrictive permissions (0600 - owner read/write only)
        let metadata = fs::metadata(&temp_path).map_err(|e| {
            adapteros_core::AosError::Io(format!("Failed to get file metadata: {}", e))
        })?;
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(&temp_path, permissions).map_err(|e| {
            adapteros_core::AosError::Io(format!("Failed to set file permissions: {}", e))
        })?;

        // Atomically rename to the final path
        fs::rename(&temp_path, path).map_err(|e| {
            adapteros_core::AosError::Io(format!("Failed to rename key file: {}", e))
        })?;

        Ok(())
    }

    pub fn from_keypairs(signing: Keypair, jwt: Keypair) -> Self {
        Self {
            signing_keypair: signing,
            jwt_keypair: jwt,
        }
    }
}

impl Default for CryptoState {
    fn default() -> Self {
        Self::new()
    }
}

/// Phases for dataset ingestion progress tracking.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IngestionPhase {
    Scanning,
    Parsing,
    Analyzing,
    Generating,
    Uploading,
    Validating,
    ComputingStatistics,
    Completed,
    Failed,
}

impl std::fmt::Display for IngestionPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            IngestionPhase::Scanning => "scanning",
            IngestionPhase::Parsing => "parsing",
            IngestionPhase::Analyzing => "analyzing",
            IngestionPhase::Generating => "generating",
            IngestionPhase::Uploading => "uploading",
            IngestionPhase::Validating => "validating",
            IngestionPhase::ComputingStatistics => "computing_statistics",
            IngestionPhase::Completed => "completed",
            IngestionPhase::Failed => "failed",
        };
        write!(f, "{}", label)
    }
}

/// Session-based dataset ingestion progress event for SSE streaming.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionProgressEvent {
    pub session_id: String,
    pub dataset_id: Option<String>,
    pub phase: IngestionPhase,
    pub sub_phase: Option<String>,
    pub current_file: Option<String>,
    pub percentage_complete: f32, // 0.0 to 100.0
    pub phase_percentage: Option<f32>,
    pub total_files: Option<i32>,
    pub files_processed: Option<i32>,
    pub total_bytes: Option<u64>,
    pub bytes_processed: Option<u64>,
    pub message: String,
    pub error: Option<String>,
    pub timestamp: String, // ISO8601 format
    pub metadata: Option<serde_json::Value>,
}

/// Dataset progress event for SSE streaming
#[derive(Clone, Debug, Serialize)]
pub struct DatasetProgressEvent {
    pub dataset_id: String,
    pub event_type: String, // "upload", "validation", "statistics"
    pub current_file: Option<String>,
    pub percentage_complete: f32, // 0.0 to 100.0
    pub total_files: Option<i32>,
    pub files_processed: Option<i32>,
    pub message: String,
    pub timestamp: String, // ISO8601 format
}

/// Shared application state passed to all handlers
///
/// Central state container for the AdapterOS API server, containing
/// all services, configurations, and shared resources needed by handlers.
///
/// [source: crates/adapteros-server-api/src/state.rs L76-115]
/// [source: crates/adapteros-server-api/src/main.rs L45-67]
/// [source: docs/ARCHITECTURE.md#architecture-components]
#[derive(Clone)]
pub struct AppState {
    pub db: ProtectedDb,
    pub jwt_secret: Arc<Vec<u8>>,
    pub config: Arc<RwLock<ApiConfig>>,
    pub metrics_exporter: Arc<adapteros_metrics_exporter::MetricsExporter>,
    pub training_service: Arc<TrainingService>,
    pub git_subsystem: Option<Arc<adapteros_git::GitSubsystem>>,
    pub file_change_tx: Option<Arc<tokio::sync::broadcast::Sender<FileChangeEvent>>>,
    pub crypto: Arc<CryptoState>,
    pub lifecycle_manager: Option<Arc<Mutex<LifecycleManager>>>,
    pub code_job_manager: Option<Arc<CodeJobManager>>,
    pub worker: Option<WorkerHandle>,
    pub active_stack: Arc<RwLock<HashMap<String, Option<String>>>>,
    pub db_pool: sqlx::SqlitePool,
    pub plugin_registry: Arc<crate::plugin_registry::PluginRegistry>,
    pub policy_manager: Arc<PolicyPackManager>,
    pub uma_monitor: Arc<UmaPressureMonitor>,
    pub response_validator: Arc<crate::validation::response_schemas::ResponseSchemaValidator>,
    // Enhanced security fields
    pub use_ed25519: bool,
    pub ed25519_keypair: Keypair,
    pub ed25519_public_key: String,
    pub ed25519_public_keys: Vec<(String, String)>,
    pub hmac_keys: Vec<(String, Vec<u8>)>,
    pub jwt_primary_kid: String,
    // Worker authentication (Ed25519 keypair for CP->Worker tokens)
    pub worker_signing_keypair: Option<Arc<ed25519_dalek::SigningKey>>,
    pub worker_signing_public: Option<Arc<ed25519_dalek::VerifyingKey>>,
    pub worker_key_kid: Option<String>,
    // Telemetry and metrics fields
    pub metrics_collector: Arc<MetricsCollector>,
    pub metrics_registry: Arc<MetricsRegistry>,
    pub telemetry_buffer: Arc<TelemetryBuffer>,
    pub trace_buffer: Arc<TraceBuffer>,
    pub telemetry_tx: TelemetrySender,
    // Registry for adapter management
    pub registry: Option<Arc<Registry>>,
    // Dataset progress SSE channel
    pub dataset_progress_tx: Option<Arc<broadcast::Sender<DatasetProgressEvent>>>,
    // Session progress SSE channel
    pub session_progress_tx: Option<Arc<broadcast::Sender<SessionProgressEvent>>>,
    // Signal broadcast channels for SSE streaming
    pub training_signal_tx: Arc<broadcast::Sender<Signal>>,
    pub discovery_signal_tx: Arc<broadcast::Sender<Signal>>,
    pub contact_signal_tx: Arc<broadcast::Sender<Signal>>,
    // Federation daemon for consensus ledger
    pub federation_daemon: Option<Arc<FederationDaemon>>,
    // Telemetry bundle store for tenant hydration
    pub telemetry_bundle_store: Arc<std::sync::RwLock<BundleStore>>,
    // Chunked upload session manager
    pub upload_session_manager: Arc<UploadSessionManager>,
    // Boot lifecycle state manager
    pub boot_state: Option<BootStateManager>,
    // Runtime mode (dev/staging/prod)
    pub runtime_mode: Option<RuntimeMode>,
    // Strict mode (fail-closed on errors)
    pub strict_mode: bool,
    // In-flight request counter for graceful shutdown
    pub in_flight_requests: Arc<AtomicUsize>,
    // Plugin event bus for dispatching events to plugins
    pub event_bus: Option<Arc<crate::event_bus::EventBus>>,
    // Dashboard cache for tenant validation and system overview
    pub dashboard_cache: Arc<DashboardCache>,
    // Load coordinator for thundering herd protection
    pub load_coordinator: Arc<LoadCoordinator>,
    // Embedding model for RAG retrieval (optional, loaded from config)
    pub embedding_model: Option<Arc<dyn EmbeddingModel + Send + Sync>>,
    // Global tick ledger for inference tracking (optional, for deterministic execution)
    pub tick_ledger: Option<Arc<GlobalTickLedger>>,
    // Worker health monitor for health-aware routing (optional, initialized at startup)
    pub health_monitor: Option<Arc<crate::worker_health::WorkerHealthMonitor>>,
    // PRD-02: Manifest hash for replay key capture (set from loaded manifest)
    pub manifest_hash: Option<String>,
    // PRD-02: Backend name for replay key capture (CoreML, MLX, Metal)
    pub backend_name: Option<String>,
    // Crypto audit logger for cryptographic operations (optional, initialized at startup)
    pub crypto_audit_logger: Option<Arc<adapteros_crypto::audit::CryptoAuditLogger>>,
    // RAG status indicating whether embedding model is available and why if not
    pub rag_status: Option<RagStatus>,
    // Worker runtime metadata cache (populated during /v1/workers/register)
    pub worker_runtime: Arc<DashMap<String, WorkerRuntimeInfo>>,
    // KV isolation scan state
    pub kv_isolation_snapshot: Arc<RwLock<KvIsolationSnapshot>>,
    pub kv_isolation_lock: Arc<tokio::sync::Mutex<()>>,
    // Background task spawn tracking
    pub background_tasks: Arc<BackgroundTaskTracker>,
    // SSE event manager for reliable streaming with replay support
    pub sse_manager: Arc<SseEventManager>,
    // Idempotency store for safe request retries
    pub idempotency_store: Arc<IdempotencyStore>,
}

impl AppState {
    pub fn new(
        db: Db,
        jwt_secret: Vec<u8>,
        config: Arc<RwLock<ApiConfig>>,
        metrics_exporter: Arc<adapteros_metrics_exporter::MetricsExporter>,
        metrics_collector: Arc<MetricsCollector>,
        metrics_registry: Arc<MetricsRegistry>,
        uma_monitor: Arc<UmaPressureMonitor>,
    ) -> Self {
        let db = ProtectedDb::new(db);
        let db_pool = db.pool().clone(); // Get the pool from the Db struct
        let keys_dir = "var/keys".to_string();
        let crypto_state = CryptoState::new_with_path(&keys_dir);
        let ed25519_keypair = crypto_state.jwt_keypair.clone();
        let ed25519_public_key =
            crate::auth::encode_ed25519_public_key_pem(&ed25519_keypair.public_key().to_bytes());

        // Create signal broadcast channels for SSE streaming
        // Increased capacity from 100 to 1000 to prevent buffer overflow under load
        let (training_signal_tx, _) = broadcast::channel(1000);
        let (discovery_signal_tx, _) = broadcast::channel(1000);
        let (contact_signal_tx, _) = broadcast::channel(1000);

        // Create telemetry broadcast channel
        let (telemetry_tx, _) = broadcast::channel(1000);

        // JWT algorithm selection: respect jwt_mode config, with build-type defaults
        // Must compute before struct init since config is moved
        let use_ed25519 = {
            // STABILITY: Use poison-safe lock access to avoid panics
            let cfg = config.read().unwrap_or_else(|e| {
                tracing::warn!("Config lock was poisoned during state init, recovering");
                e.into_inner()
            });
            let preferred_mode = if cfg!(debug_assertions) {
                cfg.auth.dev_algo.clone()
            } else {
                cfg.auth.prod_algo.clone()
            };
            let mode = cfg
                .security
                .jwt_mode
                .clone()
                .unwrap_or_else(|| preferred_mode.clone());
            match mode.to_lowercase().as_str() {
                "hmac" | "hs256" => {
                    tracing::info!("JWT mode configured as HMAC-SHA256");
                    false
                }
                "eddsa" | "ed25519" => {
                    tracing::info!("JWT mode configured as Ed25519");
                    true
                }
                other => {
                    tracing::warn!(jwt_mode = %other, "Unknown jwt_mode value, defaulting to Ed25519");
                    true
                }
            }
        };

        // Primary key identifiers for kid-based selection
        let primary_ed_kid = derive_kid_from_str(&ed25519_public_key);
        let mut ed25519_public_keys = vec![(primary_ed_kid.clone(), ed25519_public_key.clone())];
        if let Some(extra_keys) = config
            .read()
            .unwrap()
            .security
            .jwt_additional_ed25519_public_keys
            .clone()
        {
            for pem in extra_keys {
                let kid = derive_kid_from_str(&pem);
                ed25519_public_keys.push((kid, pem));
            }
        }

        let mut hmac_keys = vec![(derive_kid_from_bytes(&jwt_secret), jwt_secret.clone())];
        if let Some(extra_secrets) = config
            .read()
            .unwrap()
            .security
            .jwt_additional_hmac_secrets
            .clone()
        {
            for secret in extra_secrets {
                let bytes = secret.into_bytes();
                let kid = derive_kid_from_bytes(&bytes);
                hmac_keys.push((kid, bytes));
            }
        }

        Self {
            db: db.clone(),
            jwt_secret: Arc::new(jwt_secret),
            config,
            metrics_exporter,
            training_service: Arc::new(TrainingService::new()),
            git_subsystem: None,
            file_change_tx: None,
            crypto: Arc::new(crypto_state),
            lifecycle_manager: None,
            code_job_manager: None,
            worker: None,
            active_stack: Arc::new(RwLock::new(HashMap::new())),
            db_pool,
            plugin_registry: Arc::new(crate::plugin_registry::PluginRegistry::new(db.clone())),
            policy_manager: Arc::new(PolicyPackManager::new()),
            uma_monitor,
            response_validator: Arc::new(
                crate::validation::response_schemas::ResponseSchemaValidator::new(None),
            ),
            use_ed25519,
            ed25519_keypair: ed25519_keypair.clone(),
            ed25519_public_key,
            ed25519_public_keys,
            hmac_keys,
            jwt_primary_kid: primary_ed_kid,
            // Worker auth initialized to None - set via set_worker_signing_keypair()
            worker_signing_keypair: None,
            worker_signing_public: None,
            worker_key_kid: None,
            metrics_collector,
            metrics_registry,
            telemetry_buffer: Arc::new(TelemetryBuffer::default()),
            trace_buffer: Arc::new(TraceBuffer::new(1000)),
            telemetry_tx,
            registry: None,
            dataset_progress_tx: None,
            session_progress_tx: None,
            training_signal_tx: Arc::new(training_signal_tx),
            discovery_signal_tx: Arc::new(discovery_signal_tx),
            contact_signal_tx: Arc::new(contact_signal_tx),
            federation_daemon: None,
            telemetry_bundle_store: Arc::new(std::sync::RwLock::new(
                BundleStore::new("var/telemetry/bundles", RetentionPolicy::default())
                    .expect("Failed to create telemetry bundle store"),
            )),
            // Default to 1000 max concurrent upload sessions
            upload_session_manager: Arc::new(UploadSessionManager::new(1000)),
            // Boot state, runtime mode, and strict mode are set later via builder methods
            boot_state: None,
            runtime_mode: None,
            // Strict mode defaults to false, set via with_strict_mode
            strict_mode: false,
            // Initialize in-flight request counter
            in_flight_requests: Arc::new(AtomicUsize::new(0)),
            // Event bus is set later via with_event_bus
            event_bus: None,
            // Dashboard cache for tenant validation and system overview
            dashboard_cache: Arc::new(DashboardCache::new()),
            // Load coordinator for thundering herd protection
            load_coordinator: Arc::new(LoadCoordinator::new()),
            // Embedding model initialized via with_embedding_model
            embedding_model: None,
            // Tick ledger initialized via with_tick_ledger
            tick_ledger: None,
            // Health monitor initialized via with_health_monitor
            health_monitor: None,
            // PRD-02: Manifest hash and backend name set via with_manifest_info
            manifest_hash: None,
            backend_name: None,
            // Crypto audit logger initialized via with_crypto_audit_logger
            crypto_audit_logger: None,
            // RAG status initialized via with_rag_status
            rag_status: None,
            worker_runtime: Arc::new(DashMap::new()),
            kv_isolation_snapshot: Arc::new(RwLock::new(KvIsolationSnapshot::default())),
            kv_isolation_lock: Arc::new(tokio::sync::Mutex::new(())),
            background_tasks: Arc::new(BackgroundTaskTracker::default()),
            sse_manager: Arc::new(SseEventManager::new()),
            idempotency_store: Arc::new(IdempotencyStore::new()),
        }
    }

    /// Get a lifecycle-scoped database view for adapter state mutations.
    pub fn lifecycle_db(&self) -> WriteCapableDb<'_> {
        self.db.write(self.db.lifecycle_token())
    }

    /// Set boot state manager for lifecycle tracking
    pub fn with_boot_state(mut self, boot_state: BootStateManager) -> Self {
        self.boot_state = Some(boot_state);
        self
    }

    /// Set worker signing keypair for internal authentication (CP -> Worker).
    ///
    /// This enables Ed25519-signed JWTs for worker requests.
    /// The keypair should be loaded from `var/keys/worker_signing.key`.
    pub fn with_worker_signing_keypair(mut self, signing_key: ed25519_dalek::SigningKey) -> Self {
        let verifying_key = signing_key.verifying_key();
        let kid = adapteros_boot::derive_kid_from_verifying_key(&verifying_key);
        self.worker_signing_keypair = Some(Arc::new(signing_key));
        self.worker_signing_public = Some(Arc::new(verifying_key));
        self.worker_key_kid = Some(kid);
        self
    }

    /// Set runtime mode for policy enforcement
    pub fn with_runtime_mode(mut self, runtime_mode: RuntimeMode) -> Self {
        self.runtime_mode = Some(runtime_mode);
        self
    }

    /// Set strict mode (fail-closed on errors)
    pub fn with_strict_mode(mut self, strict: bool) -> Self {
        self.strict_mode = strict;
        self
    }

    pub fn with_federation(mut self, daemon: Arc<FederationDaemon>) -> Self {
        self.federation_daemon = Some(daemon);
        self
    }

    pub fn with_lifecycle(mut self, lifecycle_manager: Arc<Mutex<LifecycleManager>>) -> Self {
        self.lifecycle_manager = Some(lifecycle_manager);
        self
    }

    pub fn with_git(
        mut self,
        git_subsystem: Arc<adapteros_git::GitSubsystem>,
        file_change_tx: Arc<tokio::sync::broadcast::Sender<FileChangeEvent>>,
    ) -> Self {
        self.git_subsystem = Some(git_subsystem);
        self.file_change_tx = Some(file_change_tx);
        self
    }

    pub fn with_code_jobs(mut self, code_job_manager: Arc<CodeJobManager>) -> Self {
        self.code_job_manager = Some(code_job_manager);
        self
    }

    pub fn with_worker(
        mut self,
        worker: Arc<Mutex<Worker<Box<dyn FusedKernels + Send + Sync>>>>,
    ) -> Self {
        self.worker = Some(worker);
        self
    }

    pub fn with_plugin_registry(
        mut self,
        registry: Arc<crate::plugin_registry::PluginRegistry>,
    ) -> Self {
        self.plugin_registry = registry;
        self
    }

    /// Override the training service (e.g., to inject DB/storage-backed orchestration).
    ///
    /// Defaults to the in-memory `TrainingService::new()` created in `AppState::new`.
    /// This helper lets the server wire the orchestrator with persistent storage while
    /// keeping tests free to swap in their own instances.
    pub fn with_training_service(mut self, training_service: Arc<TrainingService>) -> Self {
        self.training_service = training_service;
        self
    }

    pub fn with_policy_manager(mut self, policy_manager: Arc<PolicyPackManager>) -> Self {
        self.policy_manager = policy_manager;
        self
    }

    pub fn with_dataset_progress(mut self, tx: broadcast::Sender<DatasetProgressEvent>) -> Self {
        self.dataset_progress_tx = Some(Arc::new(tx));
        self
    }

    /// Set session progress broadcast channel for codebase ingestion SSE streaming
    pub fn with_session_progress(mut self, tx: broadcast::Sender<SessionProgressEvent>) -> Self {
        self.session_progress_tx = Some(Arc::new(tx));
        self
    }

    pub fn with_telemetry_buffer(mut self, buffer: Arc<TelemetryBuffer>) -> Self {
        self.telemetry_buffer = buffer;
        self
    }

    pub fn with_trace_buffer(mut self, buffer: Arc<TraceBuffer>) -> Self {
        self.trace_buffer = buffer;
        self
    }

    pub fn with_telemetry_tx(mut self, tx: TelemetrySender) -> Self {
        self.telemetry_tx = tx;
        self
    }

    pub fn with_registry(mut self, registry: Arc<Registry>) -> Self {
        self.registry = Some(registry);
        self
    }

    /// Set custom training signal transmitter for SSE streaming
    pub fn with_training_signals(mut self, tx: broadcast::Sender<Signal>) -> Self {
        self.training_signal_tx = Arc::new(tx);
        self
    }

    /// Set custom discovery signal transmitter for SSE streaming
    pub fn with_discovery_signals(mut self, tx: broadcast::Sender<Signal>) -> Self {
        self.discovery_signal_tx = Arc::new(tx);
        self
    }

    /// Set custom contact signal transmitter for SSE streaming
    pub fn with_contact_signals(mut self, tx: broadcast::Sender<Signal>) -> Self {
        self.contact_signal_tx = Arc::new(tx);
        self
    }

    /// Set plugin event bus for dispatching events to plugins
    pub fn with_event_bus(mut self, event_bus: Arc<crate::event_bus::EventBus>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    /// Set custom load coordinator for thundering herd protection
    pub fn with_load_coordinator(mut self, load_coordinator: Arc<LoadCoordinator>) -> Self {
        self.load_coordinator = load_coordinator;
        self
    }

    /// Set embedding model for RAG retrieval
    pub fn with_embedding_model(
        mut self,
        embedding_model: Arc<dyn EmbeddingModel + Send + Sync>,
    ) -> Self {
        self.embedding_model = Some(embedding_model);
        self
    }

    /// Set global tick ledger for inference tracking
    pub fn with_tick_ledger(mut self, tick_ledger: Arc<GlobalTickLedger>) -> Self {
        self.tick_ledger = Some(tick_ledger);
        self
    }

    /// Set worker health monitor for health-aware routing
    pub fn with_health_monitor(
        mut self,
        monitor: Arc<crate::worker_health::WorkerHealthMonitor>,
    ) -> Self {
        self.health_monitor = Some(monitor);
        self
    }

    pub fn with_background_task_tracker(mut self, tracker: Arc<BackgroundTaskTracker>) -> Self {
        self.background_tasks = tracker;
        self
    }

    /// Set custom SSE event manager for reliable streaming with replay support
    pub fn with_sse_manager(mut self, manager: Arc<SseEventManager>) -> Self {
        self.sse_manager = manager;
        self
    }

    /// Set custom idempotency store for request deduplication
    pub fn with_idempotency_store(mut self, store: Arc<IdempotencyStore>) -> Self {
        self.idempotency_store = store;
        self
    }

    /// Get a reference to the idempotency store
    pub fn idempotency_store(&self) -> Arc<IdempotencyStore> {
        Arc::clone(&self.idempotency_store)
    }

    pub fn background_task_tracker(&self) -> Arc<BackgroundTaskTracker> {
        Arc::clone(&self.background_tasks)
    }

    pub fn background_task_snapshot(&self) -> BackgroundTaskSnapshot {
        self.background_tasks.snapshot()
    }

    /// Set manifest hash and backend name for replay key capture (PRD-02)
    pub fn with_manifest_info(mut self, manifest_hash: String, backend_name: String) -> Self {
        self.manifest_hash = Some(manifest_hash);
        self.backend_name = Some(backend_name);
        self
    }

    /// Set crypto audit logger for cryptographic operation logging
    pub fn with_crypto_audit_logger(
        mut self,
        logger: Arc<adapteros_crypto::audit::CryptoAuditLogger>,
    ) -> Self {
        self.crypto_audit_logger = Some(logger);
        self
    }

    /// Log a successful crypto operation
    ///
    /// This is a convenience method for logging successful cryptographic operations.
    /// If no crypto_audit_logger is configured, the call is a no-op.
    pub async fn log_crypto_success(
        &self,
        operation: adapteros_crypto::audit::CryptoOperation,
        key_id: Option<String>,
        user_id: Option<String>,
        metadata: serde_json::Value,
    ) {
        if let Some(ref logger) = self.crypto_audit_logger {
            let _ = logger
                .log_success(operation, key_id, user_id, metadata)
                .await;
        }
    }

    /// Log a failed crypto operation
    ///
    /// This is a convenience method for logging failed cryptographic operations.
    /// If no crypto_audit_logger is configured, the call is a no-op.
    pub async fn log_crypto_failure(
        &self,
        operation: adapteros_crypto::audit::CryptoOperation,
        key_id: Option<String>,
        user_id: Option<String>,
        error: &str,
        metadata: serde_json::Value,
    ) {
        if let Some(ref logger) = self.crypto_audit_logger {
            let _ = logger
                .log_failure(operation, key_id, user_id, error, metadata)
                .await;
        }
    }

    /// Set RAG status indicating whether embedding model is available
    pub fn with_rag_status(mut self, status: RagStatus) -> Self {
        self.rag_status = Some(status);
        self
    }

    /// Get a clone of the training signal sender for broadcasting events
    pub fn training_signal_sender(&self) -> Arc<broadcast::Sender<Signal>> {
        self.training_signal_tx.clone()
    }

    /// Get a clone of the discovery signal sender for broadcasting events
    pub fn discovery_signal_sender(&self) -> Arc<broadcast::Sender<Signal>> {
        self.discovery_signal_tx.clone()
    }

    /// Get a clone of the contact signal sender for broadcasting events
    pub fn contact_signal_sender(&self) -> Arc<broadcast::Sender<Signal>> {
        self.contact_signal_tx.clone()
    }

    /// Helper to check if lifecycle manager is available
    pub fn has_lifecycle_manager(&self) -> bool {
        self.lifecycle_manager.is_some()
    }

    /// Get active stack metadata for telemetry correlation (PRD-03)
    ///
    /// Returns (stack_id, stack_version) for the currently active stack for the given tenant.
    /// Returns None if no stack is active or if stack lookup fails.
    pub async fn get_active_stack_metadata(&self, tenant_id: &str) -> Option<(String, i64)> {
        // Get active stack ID from in-memory map
        let stack_id = {
            let active = self.active_stack.read().ok()?;
            active.get(tenant_id)?.clone()?
        };

        // Query database for stack details including version
        let stack = self.db.get_stack(tenant_id, &stack_id).await.ok()??;

        Some((stack.id, stack.version))
    }

    /// Start background telemetry persistence workers
    ///
    /// Spawns background tasks that:
    /// - Periodically flush telemetry buffer to database (every 30 seconds)
    /// - Flush when buffer is 80% full
    /// - Retry failed events from the dead letter queue
    /// - Monitor telemetry system health
    ///
    /// Returns a join handle that can be used to await worker shutdown.
    pub fn spawn_telemetry_workers(&self) -> tokio::task::JoinHandle<()> {
        use crate::telemetry::{spawn_telemetry_workers, TelemetryWorkerConfig};

        spawn_telemetry_workers(
            self.telemetry_buffer.clone(),
            self.db.clone(),
            TelemetryWorkerConfig::default(),
        )
    }
}

/// Shared snapshot for KV isolation scanning status.
#[derive(Debug, Clone, Serialize, Default)]
pub struct KvIsolationSnapshot {
    pub last_started_at: Option<String>,
    pub last_completed_at: Option<String>,
    pub last_error: Option<String>,
    pub running: bool,
    pub last_report: Option<KvIsolationScanReport>,
}
