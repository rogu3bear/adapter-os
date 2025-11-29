use adapteros_crypto::Keypair;
use adapteros_db::git::FileChangeEvent;
use adapteros_db::{sqlx, Db};
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

use crate::boot_state::BootStateManager;
use crate::load_coordinator::LoadCoordinator;
use crate::runtime_mode::RuntimeMode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, RwLock};
use tokio::sync::{broadcast, Mutex};

use crate::caching::DashboardCache;
use crate::config::PathsConfig;
use crate::handlers::chunked_upload::UploadSessionManager;
use crate::telemetry::{MetricsRegistry, TelemetryBuffer, TelemetrySender, TraceBuffer};
use adapteros_registry::Registry;

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
    /// Performance configuration
    #[serde(default)]
    pub performance: PerformanceConfigApi,
    /// Paths configuration for storage locations
    pub paths: PathsConfig,
}

fn default_directory_analysis_timeout() -> u64 {
    120
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub bearer_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub system_name: Option<String>,
    pub environment: Option<String>,
    pub api_base_url: Option<String>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecurityConfigApi {
    #[serde(default)]
    pub jwt_mode: Option<String>,
    #[serde(default)]
    pub token_ttl_seconds: Option<u64>,
    #[serde(default)]
    pub require_mfa: Option<bool>,
    #[serde(default)]
    pub require_pf_deny: bool,
    #[serde(default)]
    pub dev_login_enabled: bool,
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
        fs::write(&temp_path, &key_bytes).map_err(|e| {
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
/// [source: docs/ARCHITECTURE_INDEX.md#api-server-architecture]
#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub jwt_secret: Arc<Vec<u8>>,
    pub config: Arc<RwLock<ApiConfig>>,
    pub metrics_exporter: Arc<adapteros_metrics_exporter::MetricsExporter>,
    pub training_service: Arc<TrainingService>,
    pub git_subsystem: Option<Arc<adapteros_git::GitSubsystem>>,
    pub file_change_tx: Option<Arc<tokio::sync::broadcast::Sender<FileChangeEvent>>>,
    pub crypto: Arc<CryptoState>,
    pub lifecycle_manager: Option<Arc<Mutex<LifecycleManager>>>,
    pub code_job_manager: Option<Arc<CodeJobManager>>,
    pub worker: Option<Arc<Mutex<Worker<Box<dyn FusedKernels + Send + Sync>>>>>,
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
        let db_pool = db.pool().clone(); // Get the pool from the Db struct
        let crypto_state = CryptoState::new();
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
            use_ed25519: true, // Default to Ed25519 for production
            ed25519_keypair: ed25519_keypair.clone(),
            ed25519_public_key,
            metrics_collector,
            metrics_registry,
            telemetry_buffer: Arc::new(TelemetryBuffer::default()),
            trace_buffer: Arc::new(TraceBuffer::new(1000)),
            telemetry_tx,
            registry: None,
            dataset_progress_tx: None,
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
            // Boot state and runtime mode are set later via with_boot_state/with_runtime_mode
            boot_state: None,
            runtime_mode: None,
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
        }
    }

    /// Set boot state manager for lifecycle tracking
    pub fn with_boot_state(mut self, boot_state: BootStateManager) -> Self {
        self.boot_state = Some(boot_state);
        self
    }

    /// Set runtime mode for policy enforcement
    pub fn with_runtime_mode(mut self, runtime_mode: RuntimeMode) -> Self {
        self.runtime_mode = Some(runtime_mode);
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

    pub fn with_policy_manager(mut self, policy_manager: Arc<PolicyPackManager>) -> Self {
        self.policy_manager = policy_manager;
        self
    }

    pub fn with_dataset_progress(mut self, tx: broadcast::Sender<DatasetProgressEvent>) -> Self {
        self.dataset_progress_tx = Some(Arc::new(tx));
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
