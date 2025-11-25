use adapteros_crypto::Keypair;
use adapteros_db::git::FileChangeEvent;
use adapteros_db::{sqlx, Db};
use adapteros_lora_kernel_api::FusedKernels;
use adapteros_lora_lifecycle::LifecycleManager;
use adapteros_lora_worker::memory::UmaPressureMonitor;
use adapteros_lora_worker::signal::Signal;
use adapteros_lora_worker::Worker;
use adapteros_orchestrator::{CodeJobManager, FederationDaemon, TrainingService};
use adapteros_telemetry::{BundleStore, MetricsCollector, RetentionPolicy};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::{broadcast, Mutex};

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
}

impl Default for CapacityLimits {
    fn default() -> Self {
        Self {
            models_per_worker: Some(10),
            models_per_tenant: Some(5),
            concurrent_requests: Some(100),
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
}

fn default_directory_analysis_timeout() -> u64 {
    120
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub bearer_token: String,
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
        let ed25519_public_key = crate::auth::encode_ed25519_public_key_pem(
            &ed25519_keypair.public_key().to_bytes()
        );

        // Create signal broadcast channels for SSE streaming
        let (training_signal_tx, _) = broadcast::channel(100);
        let (discovery_signal_tx, _) = broadcast::channel(100);
        let (contact_signal_tx, _) = broadcast::channel(100);

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
        }
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
