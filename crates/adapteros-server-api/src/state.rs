use adapteros_crypto::Keypair;
use adapteros_db::Db;
use adapteros_lora_lifecycle::LifecycleManager;
use adapteros_orchestrator::{TrainingService};
#[cfg(feature = "cdp")]
use adapteros_orchestrator::CodeJobManager;
use adapteros_verify::StrictnessLevel;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use tokio::sync::Mutex;

/// Runtime configuration subset needed by API handlers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    pub metrics: MetricsConfig,
    /// Optional CAB golden gate configuration
    #[serde(default)]
    pub golden_gate: Option<GoldenGateConfigApi>,
    /// Root directory where replay bundles are stored
    pub bundles_root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub bearer_token: String,
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
    pub file_change_tx: Option<Arc<tokio::sync::broadcast::Sender<adapteros_api_types::git::FileChangeEvent>>>,
    pub crypto: Arc<CryptoState>,
    pub lifecycle_manager: Option<Arc<Mutex<LifecycleManager>>>,
    #[cfg(feature = "cdp")]
    pub code_job_manager: Option<Arc<CodeJobManager>>,
    /// JWT validation mode
    pub jwt_mode: JwtMode,
    /// Optional Ed25519 public key PEM for JWT validation
    pub jwt_public_key_pem: Option<String>,
    /// Optional runtime for base model backends (e.g., MLX FFI)
    pub model_runtime: Option<Arc<tokio::sync::Mutex<crate::model_runtime::ModelRuntime>>>,
}

impl AppState {
    pub fn new(
        db: Db,
        jwt_secret: Vec<u8>,
        config: Arc<RwLock<ApiConfig>>,
        metrics_exporter: Arc<adapteros_metrics_exporter::MetricsExporter>,
        training_service: Arc<TrainingService>,
    ) -> Self {
        Self {
            db,
            jwt_secret: Arc::new(jwt_secret),
            config,
            metrics_exporter,
            training_service,
            git_subsystem: None,
            file_change_tx: None,
            crypto: Arc::new(CryptoState::new()),
            lifecycle_manager: None,
            #[cfg(feature = "cdp")]
            code_job_manager: None,
            jwt_mode: JwtMode::Hmac,
            jwt_public_key_pem: None,
            model_runtime: Some(Arc::new(tokio::sync::Mutex::new(
                crate::model_runtime::ModelRuntime::new(),
            ))),
        }
    }

    pub fn with_lifecycle(mut self, lifecycle_manager: Arc<Mutex<LifecycleManager>>) -> Self {
        self.lifecycle_manager = Some(lifecycle_manager);
        self
    }

    pub fn with_git(
        mut self,
        git_subsystem: Arc<adapteros_git::GitSubsystem>,
        file_change_tx: Arc<tokio::sync::broadcast::Sender<adapteros_api_types::git::FileChangeEvent>>,
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
