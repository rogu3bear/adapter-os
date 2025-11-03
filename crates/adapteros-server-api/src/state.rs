use crate::types::ReplayVerificationResponse;
use adapteros_crypto::Keypair;
use adapteros_db::{self as db, Db};
use adapteros_lora_lifecycle::LifecycleManager;
use adapteros_lora_router::Router;
#[cfg(feature = "cdp")]
use adapteros_orchestrator::CodeJobManager;
use adapteros_orchestrator::TrainingService;
use adapteros_policy::PolicyPackManager;
use adapteros_system_metrics::SystemMetricsCollector;
use adapteros_telemetry::metrics::{MetricsCollector, MetricsRegistry};
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
    /// Production mode flag - when true, dev bypass is disabled
    #[serde(default = "default_false")]
    pub production_mode: bool,
}

fn default_false() -> bool {
    false
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
    /// Policy pack manager enforcing all production rules
    pub policy_manager: Arc<PolicyPackManager>,
    /// Router for K-sparse LoRA adapter selection
    pub router: Arc<Router>,
    /// Optional runtime for base model backends (e.g., MLX FFI)
    pub model_runtime: Option<Arc<tokio::sync::Mutex<crate::model_runtime::ModelRuntime>>>,
    /// Training session metadata cache for UI features
    pub training_sessions: Arc<AsyncRwLock<HashMap<String, TrainingSessionMetadata>>>,
    /// In-memory telemetry buffer for recent events
    pub telemetry_buffer: Arc<LogBuffer>,
    /// Broadcast channel for live telemetry streaming
    pub telemetry_tx: broadcast::Sender<UnifiedTelemetryEvent>,
    /// Broadcast channel for telemetry bundle updates
    pub telemetry_bundles_tx: broadcast::Sender<crate::types::TelemetryBundleResponse>,
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
        _system_metrics_collector: Option<Arc<std::sync::Mutex<SystemMetricsCollector>>>,
        telemetry_tx: Option<tokio::sync::broadcast::Sender<UnifiedTelemetryEvent>>,
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
        let trace_buffer_capacity = config.read().unwrap().metrics.trace_buffer_capacity;
        let trace_buffer = Arc::new(TraceBuffer::new(trace_buffer_capacity));

        // Create broadcast channel for telemetry bundle updates
        let (bundles_tx, _bundles_rx) =
            broadcast::channel::<crate::types::TelemetryBundleResponse>(256);

        // Initialize router with default weights and deterministic seed
        let router_seed = [42u8; 32]; // Fixed seed for deterministic routing
        let router_weights = vec![1.0; 10]; // Placeholder weights - should be configurable
        let router = Arc::new(Router::new(router_weights, 3, 1.0, 0.02, router_seed));

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
            policy_manager: Arc::new(PolicyPackManager::new()),
            router,
            model_runtime: Some(Arc::new(tokio::sync::Mutex::new(
                crate::model_runtime::ModelRuntime::new(),
            ))),
            training_sessions: Arc::new(AsyncRwLock::new(HashMap::new())),
            telemetry_buffer,
            telemetry_tx,
            telemetry_bundles_tx: bundles_tx,
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
            db.into(),
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

    pub fn with_policy_manager(mut self, policy_manager: Arc<PolicyPackManager>) -> Self {
        self.policy_manager = policy_manager;
        self
    }

    pub fn with_crypto(mut self, crypto: CryptoState) -> Self {
        self.crypto = Arc::new(crypto);
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
            production_mode: false,
            rate_limits: None,
            path_policy: PathPolicyConfig {
                allowlist: default_path_allowlist(),
                denylist: default_path_denylist(),
            },
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
            },
            golden_gate: None,
            bundles_root: temp_dir.path().display().to_string(),
            production_mode: false,
            rate_limits: None,
            path_policy: PathPolicyConfig {
                allowlist: default_path_allowlist(),
                denylist: default_path_denylist(),
            },
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
