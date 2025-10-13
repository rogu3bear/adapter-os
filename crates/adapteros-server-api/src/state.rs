use adapteros_db::Db;
use adapteros_orchestrator::TrainingService;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};

/// Runtime configuration subset needed by API handlers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    pub metrics: MetricsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub bearer_token: String,
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
    pub file_change_tx: Option<Arc<tokio::sync::broadcast::Sender<adapteros_git::FileChangeEvent>>>,
}

impl AppState {
    pub fn new(
        db: Db,
        jwt_secret: Vec<u8>,
        config: Arc<RwLock<ApiConfig>>,
        metrics_exporter: Arc<adapteros_metrics_exporter::MetricsExporter>,
    ) -> Self {
        Self {
            db,
            jwt_secret: Arc::new(jwt_secret),
            config,
            metrics_exporter,
            training_service: Arc::new(TrainingService::new()),
            git_subsystem: None,
            file_change_tx: None,
        }
    }

    pub fn with_git(
        mut self,
        git_subsystem: Arc<adapteros_git::GitSubsystem>,
        file_change_tx: Arc<tokio::sync::broadcast::Sender<adapteros_git::FileChangeEvent>>,
    ) -> Self {
        self.git_subsystem = Some(git_subsystem);
        self.file_change_tx = Some(file_change_tx);
        self
    }
}
