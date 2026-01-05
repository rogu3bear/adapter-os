//! Integration test harness for axum-test based API testing
//!
//! Provides a complete test harness for testing AdapterOS API endpoints with:
//! - In-memory SQLite test database setup
//! - Automatic authentication token management
//! - Request/response helpers for common patterns
//! - Test data fixtures and cleanup

#![allow(dead_code)]
#![allow(clippy::io_other_error)]

use adapteros_core::{BackendKind, SeedMode};
use adapteros_db::Db;
use adapteros_server_api::routes;
use adapteros_server_api::AppState;
use axum::Router;
use std::sync::Arc;

/// Complete integration test harness for API testing
pub struct ApiTestHarness {
    pub app: Router,
    pub state: AppState,
    pub auth_token: Option<String>,
}

impl ApiTestHarness {
    /// Initialize a new test harness with in-memory database and default auth
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Self::with_db_url("sqlite::memory:").await
    }

    /// Initialize a test harness with a custom database URL
    pub async fn with_db_url(db_url: &str) -> Result<Self, Box<dyn std::error::Error>> {
        use adapteros_lora_worker::memory::UmaPressureMonitor;
        use adapteros_server_api::config::PathsConfig;
        use adapteros_server_api::state::{ApiConfig, MetricsConfig};

        // Connect to database (in-memory or file-based)
        let db = Db::connect(db_url).await?;

        // Run migrations
        db.migrate().await?;

        // Create default tenant
        sqlx::query("INSERT INTO tenants (id, name, itar_flag, created_at) VALUES (?, ?, ?, datetime('now'))")
            .bind("default")
            .bind("Default Test Tenant")
            .bind(0)
            .execute(db.pool())
            .await?;

        // Create default test user with admin role
        let password_hash = adapteros_server_api::auth::hash_password("test-password-123")?;
        db.create_user(
            "testadmin@example.com",
            "Test Admin",
            &password_hash,
            adapteros_db::users::Role::Admin,
            "default",
        )
        .await?;

        // Create paths config with test defaults
        let paths_config = PathsConfig {
            artifacts_root: "var/artifacts".to_string(),
            bundles_root: "var/bundles".to_string(),
            adapters_root: "var/adapters/repo".to_string(),
            plan_dir: "plan".to_string(),
            datasets_root: "var/datasets".to_string(),
            documents_root: "var/documents".to_string(),
        };

        let api_config = Arc::new(std::sync::RwLock::new(ApiConfig {
            metrics: MetricsConfig {
                enabled: true,
                bearer_token: "test-bearer-token".to_string(),
            },
            directory_analysis_timeout_secs: 120,
            use_session_stack_for_routing: false,
            capacity_limits: Default::default(),
            general: None,
            server: Default::default(),
            security: Default::default(),
            auth: Default::default(),
            performance: Default::default(),
            paths: paths_config,
            chat_context: Default::default(),
            seed_mode: SeedMode::BestEffort,
            backend_profile: BackendKind::Auto,
            worker_id: 0,
            self_hosting: Default::default(),
        }));

        let metrics_exporter = Arc::new(adapteros_metrics_exporter::MetricsExporter::new(vec![
            0.1, 0.5, 1.0,
        ])?);

        let metrics_collector = Arc::new(adapteros_telemetry::MetricsCollector::new(
            adapteros_telemetry::metrics::MetricsConfig::default(),
        ));

        let metrics_registry = Arc::new(adapteros_server_api::telemetry::MetricsRegistry::new());

        let uma_monitor = Arc::new(UmaPressureMonitor::new(15, None));

        // Create app state using the new() constructor
        let state = AppState::new(
            db,
            b"test-jwt-secret-key-32-bytes-long".to_vec(),
            api_config,
            metrics_exporter,
            metrics_collector,
            metrics_registry,
            uma_monitor,
        )
        .with_manifest_info(
            "test-manifest-hash".to_string(),
            std::env::var("AOS_MODEL_BACKEND").unwrap_or_else(|_| "mlx".to_string()),
        );

        // Build router
        let app = routes::build(state.clone());

        Ok(ApiTestHarness {
            app,
            state,
            auth_token: None,
        })
    }

    /// Authenticate using default credentials and store token
    pub async fn authenticate(&mut self) -> Result<String, Box<dyn std::error::Error>> {
        let token = self
            .login("testadmin@example.com", "test-password-123")
            .await?;
        self.auth_token = Some(token.clone());
        Ok(token)
    }

    /// Login with specific credentials and return token
    pub async fn login(
        &self,
        email: &str,
        password: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        use super::auth::login_user;

        let response = login_user(&self.app, email, password).await.map_err(|e| {
            Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))
                as Box<dyn std::error::Error>
        })?;

        // LoginResponse has field `token`, not `access_token`
        Ok(response.token)
    }

    /// Create an adapter in the test database
    pub async fn create_test_adapter(
        &self,
        adapter_id: &str,
        tenant_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use super::fixtures_consolidated::TestAdapterFactory;
        TestAdapterFactory::create_adapter(&self.state.db, adapter_id, tenant_id)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
    }

    /// Create a dataset in the test database
    pub async fn create_test_dataset(
        &self,
        dataset_id: &str,
        name: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use super::fixtures_consolidated::TestDatasetFactory;
        TestDatasetFactory::create_dataset(&self.state.db, dataset_id, name)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
    }

    /// Create a training job in the test database
    pub async fn create_test_training_job(
        &self,
        job_id: &str,
        dataset_id: &str,
        adapter_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use super::fixtures_consolidated::TestTrainingJobFactory;
        TestTrainingJobFactory::create_completed_job(&self.state.db, job_id, dataset_id, adapter_id)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
    }

    /// Get the current database connection for manual queries
    pub fn db(&self) -> &Db {
        &self.state.db
    }

    /// Get a mutable reference to app state
    pub fn state_mut(&mut self) -> &mut AppState {
        &mut self.state
    }

    /// Get immutable reference to app state
    pub fn state_ref(&self) -> &AppState {
        &self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_harness_initialization() {
        let harness = ApiTestHarness::new().await;
        assert!(harness.is_ok(), "Failed to initialize test harness");
    }

    #[tokio::test]
    async fn test_harness_authentication() {
        let mut harness = ApiTestHarness::new().await.expect("Failed to init harness");
        let result = harness.authenticate().await;
        assert!(result.is_ok(), "Failed to authenticate");
        assert!(harness.auth_token.is_some(), "Token should be stored");
    }

    #[tokio::test]
    async fn test_harness_adapter_creation() {
        let harness = ApiTestHarness::new().await.expect("Failed to init harness");

        let result = harness
            .create_test_adapter("test-adapter-1", "default")
            .await;
        assert!(result.is_ok(), "Failed to create test adapter");
    }

    #[tokio::test]
    async fn test_harness_dataset_creation() {
        let harness = ApiTestHarness::new().await.expect("Failed to init harness");

        let result = harness
            .create_test_dataset("test-dataset-1", "Test Dataset")
            .await;
        assert!(result.is_ok(), "Failed to create test dataset");
    }

    #[tokio::test]
    async fn test_harness_training_job_creation() {
        let harness = ApiTestHarness::new().await.expect("Failed to init harness");

        // Create prerequisite dataset
        harness
            .create_test_dataset("test-dataset-1", "Test Dataset")
            .await
            .expect("Failed to create dataset");

        // Create prerequisite adapter
        harness
            .create_test_adapter("test-adapter-1", "default")
            .await
            .expect("Failed to create adapter");

        // Create training job
        let result = harness
            .create_test_training_job("test-job-1", "test-dataset-1", "test-adapter-1")
            .await;
        assert!(
            result.is_ok(),
            "Failed to create training job: {:?}",
            result.err()
        );
    }
}
