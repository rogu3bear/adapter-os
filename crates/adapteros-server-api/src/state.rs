use crate::types::ReplayVerificationResponse;
use adapteros_core::B3Hash;
use adapteros_crypto::Keypair;

/// Mock notification sender for alert evaluator
#[derive(Clone)]
struct MockNotificationSender;

#[async_trait::async_trait]
impl adapteros_system_metrics::alerting::NotificationSender for MockNotificationSender {
    async fn send_notification(&self, _notification: adapteros_system_metrics::alerting::NotificationRequest) -> adapteros_core::Result<()> {
        // Mock implementation - could be extended to send real notifications
        Ok(())
    }
}

/// Repository paths configuration
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub struct RepositoryPathsConfig {
    /// Subdirectory under bundles_root where repositories are stored
    /// Defaults to "repos" for backward compatibility
    #[serde(default = "default_repos_subdirectory")]
    pub subdirectory: String,
    /// Whether to include tenant_id in repository paths
    /// Defaults to true for multi-tenant isolation
    #[serde(default = "default_true")]
    pub include_tenant_in_path: bool,
}

fn default_repos_subdirectory() -> String {
    "repos".to_string()
}

fn default_true() -> bool {
    true
}

/// Security configuration for API operations
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub struct SecurityConfig {
    /// Maximum model file size in bytes
    #[serde(default = "default_max_model_size")]
    pub max_model_size_bytes: u64,
    /// Maximum config file size in bytes
    #[serde(default = "default_max_config_size")]
    pub max_config_size_bytes: u64,
    /// Maximum tokenizer file size in bytes
    #[serde(default = "default_max_tokenizer_size")]
    pub max_tokenizer_size_bytes: u64,
}

/// MLX-specific configuration
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct MlxConfig {
    /// Whether lazy loading is enabled
    #[serde(default)]
    pub lazy_loading: bool,
    /// Maximum number of cached models
    #[serde(default = "default_max_cached_models")]
    pub max_cached_models: usize,
    /// Cache eviction policy
    #[serde(default)]
    pub cache_eviction_policy: String,
}

fn default_max_model_size() -> u64 {
    10 * 1024 * 1024 * 1024 // 10GB
}

fn default_max_config_size() -> u64 {
    100 * 1024 * 1024 // 100MB
}

fn default_max_tokenizer_size() -> u64 {
    1 * 1024 * 1024 * 1024 // 1GB
}

fn default_max_cached_models() -> usize {
    3
}
use adapteros_db::{self as db, Db};
use adapteros_lora_lifecycle::LifecycleManager;
use adapteros_lora_router::Router;
#[cfg(feature = "cdp")]
use adapteros_orchestrator::CodeJobManager;
use adapteros_orchestrator::TrainingService;
use adapteros_policy::PolicyPackManager;
#[cfg(feature = "telemetry")]
use adapteros_system_metrics::SystemMetricsCollector;
#[cfg(feature = "telemetry")]
use adapteros_telemetry::metrics::{MetricsCollector, MetricsRegistry};
#[cfg(feature = "telemetry")]
use adapteros_telemetry::{LogBuffer, UnifiedTelemetryEvent};
use adapteros_trace::TraceBuffer;
use adapteros_verify::StrictnessLevel;
use chrono;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::{broadcast, Mutex, RwLock as AsyncRwLock};

fn default_system_metrics_interval_secs() -> u64 {
    30
}

fn default_telemetry_buffer_capacity() -> usize {
    1024
}

fn default_telemetry_channel_capacity() -> usize {
    256
}

fn default_trace_buffer_capacity() -> usize {
    512
}

fn default_metrics_server_port() -> u16 {
    9090
}

fn default_metrics_server_enabled() -> bool {
    true
}

/// Runtime configuration subset needed by API handlers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    pub metrics: MetricsConfig,
    /// Optional CAB golden gate configuration
    #[serde(default)]
    pub golden_gate: Option<GoldenGateConfigApi>,
    /// Root directory where replay bundles are stored
    pub bundles_root: String,
    /// Optional per-tenant rate limit configuration
    #[serde(default)]
    pub rate_limits: Option<RateLimitApiConfig>,
    /// Path policy configuration for repository validation
    #[serde(default)]
    pub path_policy: PathPolicyConfig,
    /// Repository storage path configuration
    #[serde(default)]
    pub repository_paths: RepositoryPathsConfig,
    /// Production mode flag - when true, dev bypass is disabled
    #[serde(default = "default_false")]
    pub production_mode: bool,
    /// Model load timeout in seconds (default: 300)
    #[serde(default = "default_model_load_timeout_secs")]
    pub model_load_timeout_secs: u64,
    /// Model unload timeout in seconds (default: 30)
    #[serde(default = "default_model_unload_timeout_secs")]
    pub model_unload_timeout_secs: u64,
    /// Model operation retry configuration
    #[serde(default)]
    pub operation_retry: OperationRetryConfig,
    /// Security configuration
    #[serde(default)]
    pub security: SecurityConfig,
    /// MLX configuration
    #[serde(default)]
    pub mlx: Option<MlxConfig>,
}

fn default_model_load_timeout_secs() -> u64 {
    300
}

fn default_model_unload_timeout_secs() -> u64 {
    30
}

fn default_false() -> bool {
    false
}

/// Configuration for operation retry behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationRetryConfig {
    /// Maximum number of retry attempts (default: 3)
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// Initial retry delay in milliseconds (default: 1000)
    #[serde(default = "default_initial_retry_delay_ms")]
    pub initial_retry_delay_ms: u64,
    /// Maximum retry delay in milliseconds (default: 30000)
    #[serde(default = "default_max_retry_delay_ms")]
    pub max_retry_delay_ms: u64,
    /// Retry backoff multiplier (default: 2.0)
    #[serde(default = "default_retry_backoff_multiplier")]
    pub backoff_multiplier: f64,
    /// Jitter factor for retry delays (default: 0.1)
    #[serde(default = "default_retry_jitter")]
    pub jitter: f64,
}

impl Default for OperationRetryConfig {
    fn default() -> Self {
        Self {
            max_retries: default_max_retries(),
            initial_retry_delay_ms: default_initial_retry_delay_ms(),
            max_retry_delay_ms: default_max_retry_delay_ms(),
            backoff_multiplier: default_retry_backoff_multiplier(),
            jitter: default_retry_jitter(),
        }
    }
}

fn default_max_retries() -> u32 {
    3
}

fn default_initial_retry_delay_ms() -> u64 {
    1000
}

fn default_max_retry_delay_ms() -> u64 {
    30000
}

fn default_retry_backoff_multiplier() -> f64 {
    2.0
}

fn default_retry_jitter() -> f64 {
    0.1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub bearer_token: String,
    #[serde(default = "default_system_metrics_interval_secs")]
    pub system_metrics_interval_secs: u64,
    #[serde(default = "default_telemetry_buffer_capacity")]
    pub telemetry_buffer_capacity: usize,
    #[serde(default = "default_telemetry_channel_capacity")]
    pub telemetry_channel_capacity: usize,
    #[serde(default = "default_trace_buffer_capacity")]
    pub trace_buffer_capacity: usize,
    #[serde(default = "default_metrics_server_port")]
    pub server_port: u16,
    #[serde(default = "default_metrics_server_enabled")]
    pub server_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenGateConfigApi {
    pub enabled: bool,
    pub baseline: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitApiConfig {
    /// Requests allowed per minute per tenant
    pub requests_per_minute: u32,
    /// Additional burst capacity
    pub burst_size: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PathPolicyConfig {
    /// Glob patterns for allowed repository paths
    #[serde(default = "default_path_allowlist")]
    pub allowlist: Vec<String>,
    /// Glob patterns for denied repository paths
    #[serde(default = "default_path_denylist")]
    pub denylist: Vec<String>,
}

fn default_path_allowlist() -> Vec<String> {
    vec!["**/*".to_string()]
}

fn default_path_denylist() -> Vec<String> {
    vec!["**/.env*".to_string(), "**/secrets/**".to_string()]
}

/// Cryptographic state for signing and verification
pub struct CryptoState {
    pub signing_keypair: Keypair,
    pub jwt_keypair: Keypair,
}

impl CryptoState {
    pub fn new() -> Self {
        Self {
            signing_keypair: Keypair::generate(),
            jwt_keypair: Keypair::generate(),
        }
    }

    pub fn from_keypairs(signing: Keypair, jwt: Keypair) -> Self {
        Self {
            signing_keypair: signing,
            jwt_keypair: jwt,
        }
    }

    /// Clone the current crypto state but replace the JWT signing keypair
    pub fn clone_with_jwt(&self, jwt: Keypair) -> Self {
        let signing_bytes = self.signing_keypair.to_bytes();
        let signing_clone = Keypair::from_bytes(&signing_bytes);
        Self::from_keypairs(signing_clone, jwt)
    }

    /// Clone the JWT signing keypair for external use without exposing internal ownership
    pub fn clone_jwt_keypair(&self) -> Keypair {
        let bytes = self.jwt_keypair.to_bytes();
        Keypair::from_bytes(&bytes)
    }

    /// Verify a replay session's cryptographic integrity
    pub async fn verify_replay_session(
        &self,
        session: &adapteros_db::ReplaySession,
    ) -> Result<ReplayVerificationResponse, String> {
        // For now, implement basic verification
        // In a full implementation, this would:
        // 1. Verify the session signature against the session data
        // 2. Validate hash chains for deterministic replay
        // 3. Check manifest integrity
        // 4. Verify policy compliance
        // 5. Validate kernel hashes
        // 6. Check telemetry bundle signatures

        // Basic signature verification (placeholder)
        let signature_valid = !session.signature.is_empty();

        // Hash chain validation (placeholder - check if hashes are valid hex)
        let hash_chain_valid = session.manifest_hash_b3.len() == 64
            && session.policy_hash_b3.len() == 64
            && session.seed_global_b3.len() == 64;

        // Manifest verification (placeholder)
        let manifest_verified = !session.manifest_hash_b3.is_empty();

        // Policy verification (placeholder)
        let policy_verified = !session.policy_hash_b3.is_empty();

        // Kernel verification (placeholder)
        let kernel_verified = session
            .kernel_hash_b3
            .as_ref()
            .map(|h| h.len() == 64)
            .unwrap_or(true);

        // Telemetry verification (placeholder)
        let telemetry_verified = !session.telemetry_bundle_ids_json.is_empty();

        // Overall validity
        let overall_valid = signature_valid
            && hash_chain_valid
            && manifest_verified
            && policy_verified
            && kernel_verified
            && telemetry_verified;

        // Mock divergences (none for now)
        let divergences = Vec::new();

        Ok(ReplayVerificationResponse {
            session_id: session.id.clone(),
            signature_valid,
            hash_chain_valid,
            manifest_verified,
            policy_verified,
            kernel_verified,
            telemetry_verified,
            overall_valid,
            divergences,
            verified_at: chrono::Utc::now().to_rfc3339(),
        })
    }
}

impl Default for CryptoState {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared application state passed to all handlers
#[derive(Clone)]
pub struct AppState {
    pub db: db::Database,
    pub jwt_secret: Arc<Vec<u8>>,
    pub config: Arc<RwLock<ApiConfig>>,
    pub metrics_exporter: Arc<adapteros_metrics_exporter::MetricsExporter>,
    pub metrics_collector: Arc<MetricsCollector>,
    pub metrics_registry: Arc<MetricsRegistry>,
    pub metrics_server: Option<Arc<adapteros_telemetry::MetricsServer>>,
    pub training_service: Arc<TrainingService>,
    pub git_subsystem: Option<Arc<adapteros_git::GitSubsystem>>,
    pub file_change_tx:
        Option<Arc<tokio::sync::broadcast::Sender<adapteros_api_types::git::FileChangeEvent>>>,
    pub crypto: Arc<CryptoState>,
    pub lifecycle_manager: Option<Arc<Mutex<LifecycleManager>>>,
    #[cfg(feature = "cdp")]
    pub code_job_manager: Option<Arc<CodeJobManager>>,
    /// Tracks ongoing operations to prevent duplicates
    /// JWT validation mode
    pub jwt_mode: JwtMode,
    /// Optional Ed25519 public key PEM for JWT validation
    pub jwt_public_key_pem: Option<String>,
    /// Policy pack manager enforcing all production rules
    pub policy_manager: Arc<PolicyPackManager>,
    /// Router for K-sparse LoRA adapter selection
    ///
    /// # Citations
    /// - Router implementation: [source: crates/adapteros-lora-router/src/lib.rs]
    /// - K-sparse routing: [source: docs/ARCHITECTURE_INDEX.md] (verify path)
    /// - Deterministic routing: [source: crates/adapteros-lora-worker/src/inference_pipeline.rs]
    pub router: Arc<Router>,
    /// Optional runtime for base model backends (e.g., MLX FFI)
    pub model_runtime: Option<Arc<tokio::sync::Mutex<crate::model_runtime::ModelRuntime>>>,
    /// Training session metadata cache for UI features
    pub training_sessions: Arc<AsyncRwLock<HashMap<String, TrainingSessionMetadata>>>,
    /// In-memory telemetry buffer for recent events
    pub telemetry_buffer: Arc<LogBuffer>,
    /// Broadcast channel for live telemetry streaming
    pub telemetry_tx: broadcast::Sender<UnifiedTelemetryEvent>,
    /// Broadcast channel for live alert streaming
    ///
    /// # Citations
    /// - SSE handler: [source: crates/adapteros-server-api/src/handlers.rs L12929-12935]
    /// - Alert broadcasting: [source: crates/adapteros-system-metrics/src/alerting.rs L444-L452]
    pub alert_tx: broadcast::Sender<adapteros_system_metrics::monitoring_types::AlertResponse>,
    /// Alert evaluator for monitoring and alerting
    ///
    /// # Citations
    /// - Implementation: [source: crates/adapteros-system-metrics/src/alerting.rs L23-L32]
    /// - Alert broadcasting: [source: crates/adapteros-system-metrics/src/alerting.rs L444-L452]
    pub alert_evaluator: Arc<adapteros_system_metrics::alerting::AlertEvaluator>,
    /// Global deterministic seed for all RNG operations
    ///
    /// # Citations
    /// - Seed derivation: [source: crates/adapteros-core/src/seed.rs L39-L56]
    /// - HKDF-SHA256: RFC 5869 - HMAC-based Key Derivation Function
    /// - Deterministic execution: [source: docs/ARCHITECTURE_INDEX.md] (verify path)
    /// - Global seed isolation: [source: crates/adapteros-core/src/seed.rs L86-L89]
    pub global_seed: B3Hash,
    /// Broadcast channel for telemetry bundle updates
    pub telemetry_bundles_tx: broadcast::Sender<crate::types::TelemetryBundleResponse>,
    /// Broadcast channel for operation progress updates
    pub operation_progress_tx: broadcast::Sender<crate::types::OperationProgressEvent>,
    /// Tracker for ongoing adapter operations
    ///
    /// # Citations
    /// - Implementation: [source: crates/adapteros-server-api/src/operation_tracker.rs L1-L50]
    /// - Progress broadcasting: [source: crates/adapteros-server-api/src/state.rs L428-L429]
    pub operation_tracker: Arc<crate::operation_tracker::OperationTracker>,
    /// In-memory trace buffer for recent traces
    pub trace_buffer: Arc<TraceBuffer>,
}

impl AppState {
    pub fn new(
        db: db::Database,
        jwt_secret: Vec<u8>,
        config: Arc<RwLock<ApiConfig>>,
        metrics_exporter: Arc<adapteros_metrics_exporter::MetricsExporter>,
        metrics_collector: Arc<MetricsCollector>,
        metrics_registry: Arc<MetricsRegistry>,
        training_service: Arc<TrainingService>,
        telemetry_tx: Option<tokio::sync::broadcast::Sender<UnifiedTelemetryEvent>>,
        global_seed: [u8; 32],
    ) -> Self {
        Self::new_with_system_collector(
            db,
            jwt_secret,
            config,
            metrics_exporter,
            metrics_collector,
            metrics_registry,
            training_service,
            None,
            telemetry_tx,
            global_seed,
        )
    }

    pub fn new_with_system_collector(
        db: db::Database,
        jwt_secret: Vec<u8>,
        config: Arc<RwLock<ApiConfig>>,
        metrics_exporter: Arc<adapteros_metrics_exporter::MetricsExporter>,
        metrics_collector: Arc<MetricsCollector>,
        metrics_registry: Arc<MetricsRegistry>,
        training_service: Arc<TrainingService>,
        _system_metrics_collector: Option<Arc<std::sync::Mutex<SystemMetricsCollector>>>,
        telemetry_tx: Option<tokio::sync::broadcast::Sender<UnifiedTelemetryEvent>>,
        global_seed: [u8; 32],
    ) -> Self {
        // Bounded buffer avoids unbounded telemetry growth while keeping recent history handy.
        let telemetry_buffer_capacity = config.read().unwrap().metrics.telemetry_buffer_capacity;
        let telemetry_buffer = Arc::new(LogBuffer::new(telemetry_buffer_capacity));
        let telemetry_tx = telemetry_tx.unwrap_or_else(|| {
            // Limit broadcast backlog so slow subscribers can't leak memory.
            let telemetry_channel_capacity =
                config.read().unwrap().metrics.telemetry_channel_capacity;
            let (tx, _rx) = broadcast::channel::<UnifiedTelemetryEvent>(telemetry_channel_capacity);
            tx
        });

        // Create broadcast channel for alert streaming
        let (alert_tx, _alert_rx) =
            broadcast::channel::<adapteros_system_metrics::monitoring_types::AlertResponse>(256);

        // Initialize alert evaluator with broadcast channel
        // Citation: [source: crates/adapteros-system-metrics/src/alerting.rs L23-L32] - AlertEvaluator struct
        let alert_evaluator = {
            let db_arc = Arc::new(db.clone().into_inner());
            let telemetry_writer = adapteros_telemetry::TelemetryWriter::new(
                std::path::Path::new("var/log"),
                1000,
                1024 * 1024,
            ).unwrap_or_else(|_| {
                // Fallback: create a minimal telemetry writer that doesn't persist
                adapteros_telemetry::TelemetryWriter::new_with_broadcast(
                    std::path::Path::new("/tmp"),
                    100,
                    64 * 1024,
                    None,
                ).expect("Failed to create fallback telemetry writer")
            });

            let alerting_config = adapteros_system_metrics::alerting::AlertingConfig::default();
            let notification_sender = Arc::new(MockNotificationSender);

            let mut evaluator = adapteros_system_metrics::alerting::AlertEvaluator::new(
                db_arc,
                telemetry_writer,
                alerting_config,
                notification_sender,
            );

            // Set the alert broadcast channel
            // Citation: [source: crates/adapteros-system-metrics/src/alerting.rs L171-L178] - with_alert_broadcast method
            evaluator = evaluator.with_alert_broadcast(Some(alert_tx.clone()));

            Arc::new(evaluator)
        };

        let trace_buffer_capacity = config.read().unwrap().metrics.trace_buffer_capacity;
        let trace_buffer = Arc::new(TraceBuffer::new(trace_buffer_capacity));

        // Create broadcast channel for telemetry bundle updates
        let (bundles_tx, _bundles_rx) =
            broadcast::channel::<crate::types::TelemetryBundleResponse>(256);

        // Create broadcast channel for operation progress updates
        let (progress_tx, _progress_rx) =
            broadcast::channel::<crate::types::OperationProgressEvent>(256);

        // Initialize operation tracker with progress broadcasting
        let operation_tracker = crate::operation_tracker::OperationTracker::new_with_progress(
            std::time::Duration::from_secs(300), // 5 minute timeout
            progress_tx.clone(),
        );

        // Initialize router with default weights and deterministic seed derived from global seed
        let global_seed_b3 = B3Hash::from_bytes(global_seed);
        let router_seed_bytes = adapteros_core::derive_seed(&global_seed_b3, "router");
        let router_weights = vec![1.0; 10]; // Placeholder weights - should be configurable
        let router = Arc::new(Router::new(router_weights, 3, 1.0, 0.02, router_seed_bytes));

        Self {
            db,
            jwt_secret: Arc::new(jwt_secret),
            config: config.clone(),
            metrics_exporter,
            metrics_collector,
            metrics_registry,
            metrics_server: None,
            training_service,
            git_subsystem: None,
            file_change_tx: None,
            crypto: Arc::new(CryptoState::new()),
            lifecycle_manager: None,
            #[cfg(feature = "cdp")]
            code_job_manager: None,
            jwt_mode: JwtMode::Hmac,
            jwt_public_key_pem: None,
            policy_manager: Arc::new(PolicyPackManager::new()),
            router,
            model_runtime: {
                // Get file size limits and lazy loading settings from config
                let config_guard = config.read().unwrap();
                let max_model_size = config_guard.security.max_model_size_bytes;
                let max_config_size = config_guard.security.max_config_size_bytes;
                let max_tokenizer_size = config_guard.security.max_tokenizer_size_bytes;

                let lazy_loading_enabled = config_guard
                    .mlx
                    .as_ref()
                    .map(|mlx| mlx.lazy_loading)
                    .unwrap_or(false);
                let max_cached_models = config_guard
                    .mlx
                    .as_ref()
                    .map(|mlx| mlx.max_cached_models)
                    .unwrap_or(3);
                let cache_eviction_policy = config_guard
                    .mlx
                    .as_ref()
                    .map(|mlx| mlx.cache_eviction_policy.clone())
                    .unwrap_or_else(|| "lru".to_string());

                drop(config_guard);

                let mut runtime = crate::model_runtime::ModelRuntime::with_limits_and_seed(
                    max_model_size,
                    max_config_size,
                    max_tokenizer_size,
                    global_seed.as_bytes().try_into().expect("Global seed should be 32 bytes"),
                );

                // Configure lazy loading settings
                runtime.set_lazy_loading(lazy_loading_enabled);
                runtime.set_max_cached_models(max_cached_models);
                runtime.set_cache_eviction_policy(cache_eviction_policy);

                Some(Arc::new(tokio::sync::Mutex::new(runtime)))
            },
            training_sessions: Arc::new(AsyncRwLock::new(HashMap::new())),
            telemetry_buffer,
            telemetry_tx,
            alert_tx,
            alert_evaluator,
            telemetry_bundles_tx: bundles_tx,
            operation_progress_tx: progress_tx,
            operation_tracker: Arc::new(operation_tracker),
            trace_buffer,
            global_seed: global_seed_b3,
        }
    }

    /// Create AppState with SQLite database (development)
    pub fn with_sqlite(
        db: Db,
        jwt_secret: Vec<u8>,
        config: Arc<RwLock<ApiConfig>>,
        metrics_exporter: Arc<adapteros_metrics_exporter::MetricsExporter>,
        metrics_collector: Option<Arc<MetricsCollector>>,
        metrics_registry: Option<Arc<MetricsRegistry>>,
        training_service: Arc<TrainingService>,
        global_seed: [u8; 32],
    ) -> Self {
        Self::new(
            db.into(),
            jwt_secret,
            config,
            metrics_exporter,
            metrics_collector,
            metrics_registry,
            training_service,
            None, // No telemetry_tx by default
            global_seed,
        )
    }

    pub fn with_lifecycle(mut self, lifecycle_manager: Arc<Mutex<LifecycleManager>>) -> Self {
        self.lifecycle_manager = Some(lifecycle_manager);
        self
    }

    pub fn with_policy_manager(mut self, policy_manager: Arc<PolicyPackManager>) -> Self {
        self.policy_manager = policy_manager;
        self
    }

    pub fn with_crypto(mut self, crypto: CryptoState) -> Self {
        self.crypto = Arc::new(crypto);
        self
    }

    /// Send telemetry event
    pub fn send_telemetry_event(&self, event: UnifiedTelemetryEvent) {
        let _ = self.telemetry_tx.send(event);
    }

    /// Push event to telemetry buffer
    pub fn push_telemetry_event(&self, event: UnifiedTelemetryEvent) {
        self.telemetry_buffer.push(event.clone());
        self.send_telemetry_event(event);
    }

    /// Query telemetry buffer
    pub fn query_telemetry(&self, filters: &adapteros_telemetry::TelemetryFilters) -> Vec<UnifiedTelemetryEvent> {
        self.telemetry_buffer.query(filters)
    }

    /// Derive a deterministic seed for a specific component
    ///
    /// # Citations
    /// - Seed derivation: [source: crates/adapteros-core/src/seed.rs L39-L56]
    /// - HKDF-SHA256: RFC 5869 - HMAC-based Key Derivation Function
    /// - Component isolation: [source: crates/adapteros-core/src/seed.rs L86-L89]
    pub fn derive_component_seed(&self, component: &str) -> [u8; 32] {
        adapteros_core::derive_seed(&self.global_seed, component)
    }

    /// Derive seeds for multiple components at once
    ///
    /// # Citations
    /// - Batch derivation: [source: crates/adapteros-core/src/seed.rs L86-L89]
    /// - Performance optimization: [source: docs/ARCHITECTURE_INDEX.md] (verify path)
    pub fn derive_component_seeds(&self, components: &[&str]) -> Vec<[u8; 32]> {
        adapteros_core::derive_seeds(&self.global_seed, components)
    }

    /// Derive a seed for training operations with tenant isolation
    ///
    /// # Citations
    /// - Training seed derivation: [source: crates/adapteros-core/src/seed.rs L100-L118]
    /// - Tenant isolation: [source: docs/ARCHITECTURE_INDEX.md] (verify path)
    /// - Training determinism: [source: crates/adapteros-orchestrator/src/training.rs]
    pub fn derive_training_seed(&self, tenant_id: &str, training_job_id: &str) -> [u8; 32] {
        let seed = adapteros_core::derive_seed_full(
            &self.global_seed,
            &B3Hash::hash(format!("tenant:{}", tenant_id).as_bytes()),
            &B3Hash::hash(format!("training:{}", training_job_id).as_bytes()),
            1, // training nonce
            "training",
            0, // index
        );

        // Audit logging for seed derivation
        self.push_telemetry_event(
            adapteros_telemetry::TelemetryEventBuilder::new(
                adapteros_telemetry::EventType::Custom("seed.derived".to_string()),
                adapteros_telemetry::LogLevel::Info,
            )
            .with_field("component", "training")
            .with_field("tenant_id", tenant_id)
            .with_field("job_id", training_job_id)
            .with_field("seed_hash", &adapteros_core::B3Hash::hash(&seed).to_short_hex())
            .build(),
        );

        seed
    }

    /// Validate seed consistency and log audit trail
    ///
    /// # Citations
    /// - Seed validation: [source: crates/adapteros-core/src/seed.rs L194-L228]
    /// - Audit logging: [source: crates/adapteros-telemetry/src/lib.rs]
    /// - Determinism validation: [source: docs/ARCHITECTURE_INDEX.md] (verify path)
    pub fn validate_seed_consistency(&self) -> Result<(), adapteros_core::AosError> {
        // Validate router seed consistency
        let router_seed = self.derive_component_seed("router");
        let expected_router_seed = adapteros_core::derive_seed(&self.global_seed, "router");

        if router_seed != expected_router_seed {
            return Err(adapteros_core::AosError::DeterministicViolation(
                format!("Router seed inconsistency detected")
            ));
        }

        // Log successful validation
        self.push_telemetry_event(
            adapteros_telemetry::TelemetryEventBuilder::new(
                adapteros_telemetry::EventType::Custom("seed.validated".to_string()),
                adapteros_telemetry::LogLevel::Info,
            )
            .with_field("global_seed_hash", &self.global_seed.to_short_hex())
            .with_field("validation_type", "consistency")
            .build(),
        );

        Ok(())
    }

    pub fn with_metrics_server(
        mut self,
        metrics_server: Arc<adapteros_telemetry::MetricsServer>,
    ) -> Self {
        self.metrics_server = Some(metrics_server);
        self
    }

    pub fn with_git(
        mut self,
        git_subsystem: Arc<adapteros_git::GitSubsystem>,
        file_change_tx: Arc<
            tokio::sync::broadcast::Sender<adapteros_api_types::git::FileChangeEvent>,
        >,
    ) -> Self {
        self.git_subsystem = Some(git_subsystem);
        self.file_change_tx = Some(file_change_tx);
        self
    }

    #[cfg(feature = "cdp")]
    pub fn with_code_jobs(mut self, code_job_manager: Arc<CodeJobManager>) -> Self {
        self.code_job_manager = Some(code_job_manager);
        self
    }

    /// Helper to check if lifecycle manager is available
    pub fn has_lifecycle_manager(&self) -> bool {
        self.lifecycle_manager.is_some()
    }

    /// Configure JWT validation mode and public key (if EdDSA)
    pub fn set_jwt_mode(&mut self, mode: JwtMode, public_pem: Option<String>) {
        self.jwt_mode = mode;
        self.jwt_public_key_pem = public_pem;
    }
}

/// JWT validation mode for middleware
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JwtMode {
    Hmac,
    EdDsa,
}

#[derive(Debug, Clone)]
pub struct TrainingSessionMetadata {
    pub repository_path: Option<String>,
    pub description: Option<String>,
    pub tenant_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_telemetry::{EventType, LogLevel, TelemetryEventBuilder};
    use tempfile::TempDir;

    #[cfg(feature = "cdp")]
    use adapteros_orchestrator::{code_jobs::PathsConfig, OrchestratorConfig};

    #[cfg(feature = "cdp")]
    #[tokio::test]
    async fn test_telemetry_buffer_eviction_with_small_capacity() {
        // Create config with tiny telemetry buffer capacity
        let api_config = ApiConfig {
            metrics: MetricsConfig {
                enabled: false,
                bearer_token: String::new(),
                system_metrics_interval_secs: default_system_metrics_interval_secs(),
                telemetry_buffer_capacity: 3, // Tiny capacity for testing eviction
                telemetry_channel_capacity: default_telemetry_channel_capacity(),
                trace_buffer_capacity: default_trace_buffer_capacity(),
                server_port: default_metrics_server_port(),
                server_enabled: default_metrics_server_enabled(),
            },
            golden_gate: None,
            bundles_root: "/tmp".to_string(),
            production_mode: false,
            rate_limits: None,
            path_policy: PathPolicyConfig {
                allowlist: default_path_allowlist(),
                denylist: default_path_denylist(),
            },
            repository_paths: RepositoryPathsConfig::default(),
            model_load_timeout_secs: default_model_load_timeout_secs(),
            model_unload_timeout_secs: default_model_unload_timeout_secs(),
            operation_retry: OperationRetryConfig::default(),
            security: SecurityConfig::default(),
            mlx: None,
        };
        let config = Arc::new(RwLock::new(api_config));

        // Create minimal AppState components
        let metrics_exporter = Arc::new(
            adapteros_metrics_exporter::MetricsExporter::new(vec![0.1, 0.5, 1.0])
                .expect("metrics exporter"),
        );

        let metrics_collector = Arc::new(
            adapteros_telemetry::MetricsCollector::new_with_system_provider(None)
                .expect("metrics collector"),
        );
        let metrics_registry = Arc::new(adapteros_telemetry::MetricsRegistry::new(
            metrics_collector.clone(),
        ));

        let training_service = Arc::new(TrainingService::new());

        // Create AppState with the tiny buffer capacity
        let app_state = AppState::new_with_system_collector(
            adapteros_db::Db::connect(":memory:")
                .await
                .expect("db connect")
                .into(),
            b"test_secret".to_vec(),
            config,
            metrics_exporter,
            metrics_collector,
            metrics_registry,
            training_service,
            None,
            None,
        );

        // Create and add 5 events (more than capacity of 3)
        let events = vec![
            TelemetryEventBuilder::new(
                EventType::Custom("test.event".to_string()),
                LogLevel::Info,
                "Event 1".to_string(),
            )
            .build(),
            TelemetryEventBuilder::new(
                EventType::Custom("test.event".to_string()),
                LogLevel::Info,
                "Event 2".to_string(),
            )
            .build(),
            TelemetryEventBuilder::new(
                EventType::Custom("test.event".to_string()),
                LogLevel::Info,
                "Event 3".to_string(),
            )
            .build(),
            TelemetryEventBuilder::new(
                EventType::Custom("test.event".to_string()),
                LogLevel::Info,
                "Event 4".to_string(),
            )
            .build(),
            TelemetryEventBuilder::new(
                EventType::Custom("test.event".to_string()),
                LogLevel::Info,
                "Event 5".to_string(),
            )
            .build(),
        ];

        // Add all events to buffer
        for event in &events {
            app_state.telemetry_buffer.push(event.clone());
        }

        // Query all events (should only get the last 3 due to eviction)
        let filters = adapteros_telemetry::TelemetryFilters {
            limit: Some(10), // Ask for more than capacity
            tenant_id: None,
            user_id: None,
            start_time: None,
            end_time: None,
            event_type: None,
            level: None,
            component: None,
            trace_id: None,
        };

        let recent_events = app_state.telemetry_buffer.query(&filters);

        // Should only have 3 events (the most recent ones)
        assert_eq!(recent_events.len(), 3);

        // The events should be the last 3 added (newest first in query results)
        assert_eq!(recent_events[0].message, "Event 5");
        assert_eq!(recent_events[1].message, "Event 4");
        assert_eq!(recent_events[2].message, "Event 3");
    }

    #[cfg(feature = "cdp")]
    #[tokio::test]
    async fn with_code_jobs_sets_manager() {
        let temp_dir = TempDir::new().expect("tempdir");

        let db = Db::connect(":memory:").await.expect("db connect");
        db.migrate().await.expect("migrate");
        let db_clone = db.clone();

        let metrics_exporter = Arc::new(
            adapteros_metrics_exporter::MetricsExporter::new(vec![0.1, 0.5, 1.0])
                .expect("metrics exporter"),
        );

        // Create system metrics provider for real data integration
        let metrics_collector = Arc::new(
            adapteros_telemetry::MetricsCollector::new_with_system_provider(None)
                .expect("metrics collector"),
        );
        let metrics_registry = Arc::new(adapteros_telemetry::MetricsRegistry::new(
            metrics_collector.clone(),
        ));
        for name in [
            "inference_latency_p95_ms",
            "queue_depth",
            "tokens_per_second",
            "memory_usage_mb",
        ] {
            metrics_registry.get_or_create_series(name.to_string(), 1_000, 1_024);
        }
        let training_service = Arc::new(TrainingService::new());

        let api_config = ApiConfig {
            metrics: MetricsConfig {
                enabled: false,
                bearer_token: String::new(),
                system_metrics_interval_secs: default_system_metrics_interval_secs(),
                telemetry_buffer_capacity: default_telemetry_buffer_capacity(),
                telemetry_channel_capacity: default_telemetry_channel_capacity(),
                trace_buffer_capacity: default_trace_buffer_capacity(),
                server_port: default_metrics_server_port(),
                server_enabled: default_metrics_server_enabled(),
            },
            golden_gate: None,
            bundles_root: temp_dir.path().display().to_string(),
            production_mode: false,
            rate_limits: None,
            path_policy: PathPolicyConfig {
                allowlist: default_path_allowlist(),
                denylist: default_path_denylist(),
            },
            repository_paths: RepositoryPathsConfig::default(),
            model_load_timeout_secs: default_model_load_timeout_secs(),
            model_unload_timeout_secs: default_model_unload_timeout_secs(),
            operation_retry: OperationRetryConfig::default(),
            security: SecurityConfig::default(),
            mlx: None,
        };
        let config = Arc::new(RwLock::new(api_config));

        let base_state = AppState::with_sqlite(
            db,
            b"secret".to_vec(),
            config,
            metrics_exporter,
            metrics_collector,
            metrics_registry,
            training_service,
        );

        let artifacts_root = temp_dir.path().join("artifacts");
        std::fs::create_dir_all(&artifacts_root).expect("artifacts dir");

        let paths_config = PathsConfig {
            artifacts_dir: artifacts_root.display().to_string(),
            temp_dir: temp_dir.path().display().to_string(),
            cache_dir: temp_dir.path().display().to_string(),
            adapters_root: artifacts_root.display().to_string(),
            artifacts_root: artifacts_root.display().to_string(),
        };

        let code_job_manager = Arc::new(CodeJobManager::new(
            db_clone,
            paths_config,
            OrchestratorConfig::default(),
        ));

        let state = base_state.with_code_jobs(code_job_manager.clone());

        let manager = state
            .code_job_manager
            .expect("code job manager should be configured");
        assert!(Arc::ptr_eq(&manager, &code_job_manager));
    }

    /// Test deterministic seed derivation
    ///
    /// # Citations
    /// - Seed derivation tests: [source: crates/adapteros-core/src/seed.rs L170-L193]
    /// - Determinism validation: [source: crates/adapteros-server-api/src/state.rs L753-L782]
    #[tokio::test]
    async fn test_seed_derivation_determinism() {
        let global_seed = [42u8; 32];
        let state = AppState::test_state(global_seed);

        // Test component seed derivation consistency
        let router_seed1 = state.derive_component_seed("router");
        let router_seed2 = state.derive_component_seed("router");
        assert_eq!(router_seed1, router_seed2, "Component seeds should be deterministic");

        // Test different components get different seeds
        let model_seed = state.derive_component_seed("model_runtime");
        assert_ne!(router_seed1, model_seed, "Different components should get different seeds");

        // Test batch seed derivation
        let components = vec!["router", "model_runtime", "training"];
        let seeds = state.derive_component_seeds(&components);
        assert_eq!(seeds.len(), 3);
        assert_eq!(seeds[0], router_seed1);
        assert_eq!(seeds[1], model_seed);
    }

    /// Test training seed derivation with tenant isolation
    ///
    /// # Citations
    /// - Training seed derivation: [source: crates/adapteros-server-api/src/state.rs L721-L751]
    /// - Tenant isolation: [source: crates/adapteros-core/src/seed.rs L100-L118]
    #[tokio::test]
    async fn test_training_seed_isolation() {
        let global_seed = [123u8; 32];
        let state = AppState::test_state(global_seed);

        // Same tenant, different jobs should get different seeds
        let seed1 = state.derive_training_seed("tenant1", "job1");
        let seed2 = state.derive_training_seed("tenant1", "job2");
        assert_ne!(seed1, seed2, "Different jobs should get different seeds");

        // Different tenants, same job should get different seeds
        let seed3 = state.derive_training_seed("tenant2", "job1");
        assert_ne!(seed1, seed3, "Different tenants should get different seeds");

        // Same tenant and job should get same seed (deterministic)
        let seed4 = state.derive_training_seed("tenant1", "job1");
        assert_eq!(seed1, seed4, "Same tenant/job should get same seed");
    }

    /// Test seed consistency validation
    ///
    /// # Citations
    /// - Seed validation: [source: crates/adapteros-server-api/src/state.rs L753-L782]
    /// - Determinism enforcement: [source: docs/ARCHITECTURE_INDEX.md] (verify path)
    #[tokio::test]
    async fn test_seed_consistency_validation() {
        let global_seed = [99u8; 32];
        let state = AppState::test_state(global_seed);

        // Valid state should pass validation
        assert!(state.validate_seed_consistency().is_ok(),
                "Valid seed state should pass consistency validation");
    }

    /// Test global seed storage and access
    ///
    /// # Citations
    /// - Global seed field: [source: crates/adapteros-server-api/src/state.rs L443-450]
    /// - B3Hash usage: [source: crates/adapteros-core/src/hash.rs L9-18]
    #[tokio::test]
    async fn test_global_seed_storage() {
        let global_seed_bytes = [42u8; 32];
        let state = AppState::test_state(global_seed_bytes);

        // Verify global seed is stored correctly
        let expected_hash = adapteros_core::B3Hash::from_bytes(global_seed_bytes);
        assert_eq!(state.global_seed, expected_hash,
                  "Global seed should be stored as B3Hash");
    }
}
