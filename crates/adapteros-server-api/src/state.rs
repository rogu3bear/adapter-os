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
use tokio::sync::Mutex;

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

    /// Helper to check if lifecycle manager is available
    pub fn has_lifecycle_manager(&self) -> bool {
        self.lifecycle_manager.is_some()
    }
}
