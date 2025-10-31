use crate::types::{ReplayDivergence, ReplayVerificationResponse};
use adapteros_crypto::Keypair;
use chrono;
use adapteros_db::{self as db, Db, PostgresDb};
use adapteros_lora_lifecycle::LifecycleManager;
#[cfg(feature = "cdp")]
use adapteros_orchestrator::CodeJobManager;
use adapteros_orchestrator::TrainingService;
use adapteros_telemetry::{
    LogBuffer, MetricsCollector, MetricsRegistry, SystemMetricsProvider, SystemMetricsSnapshot,
    UnifiedTelemetryEvent,
};
use adapteros_trace::TraceBuffer;
use adapteros_verify::StrictnessLevel;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::{broadcast, Mutex, RwLock as AsyncRwLock};

/// System metrics provider implementation
struct SystemMetricsProviderImpl {
    collector: std::sync::Mutex<adapteros_system_metrics::SystemMetricsCollector>,
    telemetry_logger: Option<std::sync::Mutex<adapteros_system_metrics::SystemMetricsTelemetry>>,
}

impl SystemMetricsProviderImpl {
    fn new() -> Self {
        Self {
            collector: std::sync::Mutex::new(
                adapteros_system_metrics::SystemMetricsCollector::new(),
            ),
            telemetry_logger: None, // Will be set later if telemetry is available
        }
    }

    fn with_telemetry_logger(
        mut self,
        logger: adapteros_system_metrics::SystemMetricsTelemetry,
    ) -> Self {
        self.telemetry_logger = Some(std::sync::Mutex::new(logger));
        self
    }
}

#[async_trait::async_trait]
impl SystemMetricsProvider for SystemMetricsProviderImpl {
    async fn collect_system_metrics(&self) -> SystemMetricsSnapshot {
        let mut collector = self.collector.lock().unwrap();
        let metrics = collector.collect_metrics();

        // Calculate actual memory usage in MB from percentage and total memory
        let total_memory_kb = sysinfo::System::new().total_memory();
        let total_memory_mb = total_memory_kb as f64 / 1024.0;
        let memory_mb = (metrics.memory_usage / 100.0) * total_memory_mb;

        // Log telemetry events if logger is available
        if let Some(logger) = &self.telemetry_logger {
            let mut logger = logger.lock().unwrap();
            if let Err(e) = logger.log_metrics(&metrics) {
                tracing::warn!("Failed to log system metrics telemetry: {}", e);
            }
        }

        SystemMetricsSnapshot {
            cpu_usage_percent: metrics.cpu_usage,
            memory_usage_mb,
            disk_io_utilization: metrics.disk_io.usage_percent as f64,
            network_bandwidth_mbps: metrics.network_io.bandwidth_mbps as f64,
            gpu_utilization: metrics.gpu_metrics.utilization,
            gpu_memory_used_mb: metrics.gpu_metrics.memory_used.map(|m| m as f64 / (1024.0 * 1024.0)),
            gpu_temperature: metrics.gpu_metrics.temperature,
        }
    }
}

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
    pub training_service: Arc<TrainingService>,
    pub git_subsystem: Option<Arc<adapteros_git::GitSubsystem>>,
    pub file_change_tx:
        Option<Arc<tokio::sync::broadcast::Sender<adapteros_api_types::git::FileChangeEvent>>>,
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
    /// Training session metadata cache for UI features
    pub training_sessions: Arc<AsyncRwLock<HashMap<String, TrainingSessionMetadata>>>,
    /// In-memory telemetry buffer for recent events
    pub telemetry_buffer: Arc<LogBuffer>,
    /// Broadcast channel for live telemetry streaming
    pub telemetry_tx: broadcast::Sender<UnifiedTelemetryEvent>,
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
        system_metrics_collector: Option<
            Arc<std::sync::Mutex<adapteros_system_metrics::SystemMetricsCollector>>,
        >,
        telemetry_tx: Option<tokio::sync::broadcast::Sender<UnifiedTelemetryEvent>>,
    ) -> Self {
        // Bounded buffer avoids unbounded telemetry growth while keeping recent history handy.
        let telemetry_buffer_capacity = config.read().unwrap().metrics.telemetry_buffer_capacity;
        let telemetry_buffer = Arc::new(LogBuffer::new(telemetry_buffer_capacity));
        let telemetry_tx = telemetry_tx.unwrap_or_else(|| {
            // Limit broadcast backlog so slow subscribers can't leak memory.
            let telemetry_channel_capacity = config.read().unwrap().metrics.telemetry_channel_capacity;
            let (tx, _rx) = broadcast::channel(telemetry_channel_capacity);
            tx
        });
        let trace_buffer_capacity = config.read().unwrap().metrics.trace_buffer_capacity;
        let trace_buffer = Arc::new(TraceBuffer::new(trace_buffer_capacity));

        Self {
            db,
            jwt_secret: Arc::new(jwt_secret),
            config,
            metrics_exporter,
            metrics_collector,
            metrics_registry,
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
            training_sessions: Arc::new(AsyncRwLock::new(HashMap::new())),
            telemetry_buffer,
            telemetry_tx,
            trace_buffer,
        }
    }

    /// Create AppState with SQLite database (development)
    pub fn with_sqlite(
        db: Db,
        jwt_secret: Vec<u8>,
        config: Arc<RwLock<ApiConfig>>,
        metrics_exporter: Arc<adapteros_metrics_exporter::MetricsExporter>,
        metrics_collector: Arc<MetricsCollector>,
        metrics_registry: Arc<MetricsRegistry>,
        training_service: Arc<TrainingService>,
    ) -> Self {
        Self::new(
            db::Database::Sqlite(db),
            jwt_secret,
            config,
            metrics_exporter,
            metrics_collector,
            metrics_registry,
            training_service,
            None, // No telemetry_tx by default
        )
    }

    /// Create AppState with PostgreSQL database (production)
    pub fn with_postgres(
        db: PostgresDb,
        jwt_secret: Vec<u8>,
        config: Arc<RwLock<ApiConfig>>,
        metrics_exporter: Arc<adapteros_metrics_exporter::MetricsExporter>,
        metrics_collector: Arc<MetricsCollector>,
        metrics_registry: Arc<MetricsRegistry>,
        training_service: Arc<TrainingService>,
    ) -> Self {
        Self::new(
            db::Database::Postgres(db),
            jwt_secret,
            config,
            metrics_exporter,
            metrics_collector,
            metrics_registry,
            training_service,
            None, // No telemetry_tx by default
        )
    }

    pub fn with_lifecycle(mut self, lifecycle_manager: Arc<Mutex<LifecycleManager>>) -> Self {
        self.lifecycle_manager = Some(lifecycle_manager);
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
            },
            golden_gate: None,
            bundles_root: "/tmp".to_string(),
            rate_limits: None,
        };
        let config = Arc::new(RwLock::new(api_config));

        // Create minimal AppState components
        let metrics_exporter = Arc::new(
            adapteros_metrics_exporter::MetricsExporter::new(vec![0.1, 0.5, 1.0])
                .expect("metrics exporter"),
        );

        let system_metrics_provider: Option<Box<dyn adapteros_telemetry::SystemMetricsProvider>> =
            Some(Box::new(SystemMetricsProviderImpl::new()));

        let metrics_collector = Arc::new(
            adapteros_telemetry::MetricsCollector::new_with_system_provider(
                system_metrics_provider,
            )
            .expect("metrics collector"),
        );
        let metrics_registry = Arc::new(adapteros_telemetry::MetricsRegistry::new(
            metrics_collector.clone(),
        ));

        let training_service = Arc::new(TrainingService::new());

        // Create AppState with the tiny buffer capacity
        let app_state = AppState::new_with_system_collector(
            db::Database::Sqlite(
                adapteros_db::Db::connect(":memory:").await.expect("db connect")
            ),
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
            TelemetryEventBuilder::new(EventType::Info, LogLevel::Info, "Event 1".to_string()).build(),
            TelemetryEventBuilder::new(EventType::Info, LogLevel::Info, "Event 2".to_string()).build(),
            TelemetryEventBuilder::new(EventType::Info, LogLevel::Info, "Event 3".to_string()).build(),
            TelemetryEventBuilder::new(EventType::Info, LogLevel::Info, "Event 4".to_string()).build(),
            TelemetryEventBuilder::new(EventType::Info, LogLevel::Info, "Event 5".to_string()).build(),
        ];

        // Add all events to buffer
        for event in &events {
            app_state.telemetry_buffer.push(event.clone());
        }

        // Query all events (should only get the last 3 due to eviction)
        let filters = adapteros_telemetry::TelemetryFilters {
            limit: Some(10), // Ask for more than capacity
            tenant_id: None,
            event_type: None,
            level: None,
            component: None,
            since: None,
            until: None,
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
        let system_metrics_provider: Option<Box<dyn adapteros_telemetry::SystemMetricsProvider>> =
            Some(Box::new(SystemMetricsProviderImpl::new()));

        let metrics_collector = Arc::new(
            adapteros_telemetry::MetricsCollector::new_with_system_provider(
                system_metrics_provider,
            )
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
            },
            golden_gate: None,
            bundles_root: temp_dir.path().display().to_string(),
            rate_limits: None,
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
}
