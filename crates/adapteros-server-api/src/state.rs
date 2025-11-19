use adapteros_crypto::Keypair;
use adapteros_db::git::FileChangeEvent;
use adapteros_db::{sqlx, Db};
use adapteros_lora_kernel_api::FusedKernels;
use adapteros_lora_lifecycle::LifecycleManager;
use adapteros_lora_worker::UmaPressureMonitor;
use adapteros_lora_worker::Worker;
use adapteros_orchestrator::{CodeJobManager, TrainingService};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::{broadcast, Mutex};

/// Runtime configuration subset needed by API handlers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    pub metrics: MetricsConfig,
    /// Timeout in seconds for directory analysis operations (default: 120)
    #[serde(default = "default_directory_analysis_timeout")]
    pub directory_analysis_timeout_secs: u64,
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
    pub event_type: String,  // "upload", "validation", "statistics"
    pub current_file: Option<String>,
    pub percentage_complete: f32,  // 0.0 to 100.0
    pub total_files: Option<i32>,
    pub files_processed: Option<i32>,
    pub message: String,
    pub timestamp: String,  // ISO8601 format
}

/// Shared application state passed to all handlers
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
    pub plugin_registry: Arc<adapteros_server::PluginRegistry>,
    pub uma_monitor: Arc<UmaPressureMonitor>,
    pub response_validator: Arc<crate::validation::response_schemas::ResponseSchemaValidator>,
    // Telemetry fields
    pub metrics_collector: Arc<crate::telemetry::MetricsCollector>,
    pub metrics_registry: Arc<crate::telemetry::MetricsRegistry>,
    pub telemetry_buffer: Arc<crate::telemetry::TelemetryBuffer>,
    pub telemetry_tx: Arc<crate::telemetry::TelemetrySender>,
    pub trace_buffer: Arc<crate::telemetry::TraceBuffer>,
    pub dataset_progress_tx: Option<Arc<broadcast::Sender<DatasetProgressEvent>>>,
    // Enhanced security fields
    pub use_ed25519: bool,
    pub ed25519_keypair: Keypair,
    pub ed25519_public_key: String,
}

impl AppState {
    pub fn new(
        db: Db,
        jwt_secret: Vec<u8>,
        config: Arc<RwLock<ApiConfig>>,
        metrics_exporter: Arc<adapteros_metrics_exporter::MetricsExporter>,
        uma_monitor: Arc<UmaPressureMonitor>,
    ) -> Self {
        let db_pool = db.pool().clone(); // Get the pool from the Db struct

        // Initialize telemetry components
        let metrics_collector = Arc::new(
            crate::telemetry::MetricsCollector::new()
                .expect("Failed to initialize metrics collector")
        );
        let metrics_registry = Arc::new(crate::telemetry::MetricsRegistry::default());
        let telemetry_buffer = Arc::new(crate::telemetry::TelemetryBuffer::default());
        let (telemetry_tx, _telemetry_rx) = crate::telemetry::telemetry_channel();
        let telemetry_tx = Arc::new(telemetry_tx);
        let trace_buffer = Arc::new(crate::telemetry::TraceBuffer::default());

        let crypto_state = CryptoState::new();
        let ed25519_keypair = crypto_state.jwt_keypair.clone();
        let ed25519_public_key = hex::encode(ed25519_keypair.public_key().to_bytes());

        Self {
            db,
            jwt_secret: Arc::new(jwt_secret),
            config,
            metrics_exporter,
            training_service: Arc::new(TrainingService::new()),
            git_subsystem: None,
            file_change_tx: None,
            crypto: Arc::new(CryptoState::new()),
            lifecycle_manager: None,
            code_job_manager: None,
            worker: None,
            active_stack: Arc::new(RwLock::new(HashMap::new())),
            db_pool,
            plugin_registry: Arc::new(adapteros_server::PluginRegistry::new(db.clone())),
            uma_monitor,
            response_validator: Arc::new(crate::validation::response_schemas::ResponseSchemaValidator::new(None)),
            metrics_collector,
            metrics_registry,
            telemetry_buffer,
            telemetry_tx,
            trace_buffer,
            dataset_progress_tx: None,
            use_ed25519: true,  // Default to Ed25519 for production
            ed25519_keypair: ed25519_keypair.clone(),
            ed25519_public_key,
        }
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

    pub fn with_plugin_registry(mut self, registry: Arc<adapteros_server::PluginRegistry>) -> Self {
        self.plugin_registry = registry;
        self
    }

    pub fn with_dataset_progress(mut self, tx: broadcast::Sender<DatasetProgressEvent>) -> Self {
        self.dataset_progress_tx = Some(Arc::new(tx));
        self
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
