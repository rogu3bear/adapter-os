//! Integration test harness for axum-test based API testing
//!
//! Provides a complete test harness for testing AdapterOS API endpoints with:
//! - In-memory SQLite test database setup
//! - Automatic authentication token management
//! - Request/response helpers for common patterns
//! - Test data fixtures and cleanup

use adapteros_db::Db;
use adapteros_server_api::routes;
use adapteros_server_api::AppState;
use axum::Router;
use serde_json::Value as JsonValue;
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
        // Connect to database (in-memory or file-based)
        let db = Db::connect(db_url).await?;

        // Run migrations
        db.migrate().await?;

        // Create default tenant
        sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES (?, ?, ?)")
            .bind("default")
            .bind("Default Test Tenant")
            .bind(0)
            .execute(db.pool())
            .await?;

        // Create default test user with admin role
        let password_hash =
            adapteros_server_api::auth::hash_password("test-password-123")?;
        db.create_user(
            "testadmin@example.com",
            "Test Admin",
            &password_hash,
            adapteros_db::users::Role::Admin,
        )
        .await?;

        // Create app state
        let state = AppState::with_sqlite(
            db,
            b"test-jwt-secret-key-32-bytes-long".to_vec(),
            Arc::new(std::sync::RwLock::new(
                adapteros_server_api::state::ApiConfig {
                    metrics: adapteros_server_api::state::MetricsConfig {
                        enabled: true,
                        bearer_token: "test-bearer-token".to_string(),
                        system_metrics_interval_secs: 30,
                    },
                    golden_gate: None,
                    bundles_root: "test-bundles".to_string(),
                    rate_limits: None,
                },
            )),
            Arc::new(
                adapteros_metrics_exporter::MetricsExporter::new(vec![0.1, 0.5, 1.0])?
            ),
            Arc::new(adapteros_telemetry::MetricsCollector::new(
                adapteros_telemetry::metrics::MetricsConfig::default()
            )),
            Arc::new(adapteros_server_api::telemetry::MetricsRegistry::new()),
            Arc::new(adapteros_orchestrator::TrainingService::new()),
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
        let token = self.login("testadmin@example.com", "test-password-123").await?;
        self.auth_token = Some(token.clone());
        Ok(token)
    }

    /// Login with specific credentials and return token
    pub async fn login(
        &self,
        email: &str,
        password: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        use super::auth::{login_user, create_test_app_state};

        // Create a fresh app state for login
        let app_state = create_test_app_state().await;
        let app = routes::build(app_state);

        let response = login_user(&app, email, password)
            .await
            .map_err(|e| Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                e,
            )) as Box<dyn std::error::Error>)?;

        Ok(response.access_token)
    }

    /// Create an adapter in the test database
    pub async fn create_test_adapter(
        &self,
        adapter_id: &str,
        tenant_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let hash = format!("{:0>64}", adapter_id);

        sqlx::query(
            "INSERT INTO adapters (id, tenant_id, hash, tier, rank, activation_pct, created_at)
             VALUES (?, ?, ?, ?, ?, ?, datetime('now'))",
        )
        .bind(adapter_id)
        .bind(tenant_id)
        .bind(&hash)
        .bind("persistent")
        .bind(8)
        .bind(0.0)
        .execute(self.state.db().pool())
        .await?;

        Ok(())
    }

    /// Create a dataset in the test database
    pub async fn create_test_dataset(
        &self,
        dataset_id: &str,
        name: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let hash = format!("{:0>64}", dataset_id);

        sqlx::query(
            "INSERT INTO training_datasets (id, hash_b3, name, validation_status, created_at)
             VALUES (?, ?, ?, ?, datetime('now'))",
        )
        .bind(dataset_id)
        .bind(&hash)
        .bind(name)
        .bind("valid")
        .execute(self.state.db().pool())
        .await?;

        Ok(())
    }

    /// Create a training job in the test database
    pub async fn create_test_training_job(
        &self,
        job_id: &str,
        dataset_id: &str,
        adapter_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        sqlx::query(
            "INSERT INTO training_jobs
             (id, dataset_id, adapter_id, status, progress_pct, loss, created_at)
             VALUES (?, ?, ?, ?, ?, ?, datetime('now'))",
        )
        .bind(job_id)
        .bind(dataset_id)
        .bind(adapter_id)
        .bind("completed")
        .bind(100)
        .bind(0.05)
        .execute(self.state.db().pool())
        .await?;

        Ok(())
    }

    /// Get the current database connection for manual queries
    pub fn db(&self) -> &Db {
        self.state.db()
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
        let harness = ApiTestHarness::new()
            .await
            .expect("Failed to init harness");

        let result = harness.create_test_adapter("test-adapter-1", "default").await;
        assert!(result.is_ok(), "Failed to create test adapter");
    }

    #[tokio::test]
    async fn test_harness_dataset_creation() {
        let harness = ApiTestHarness::new()
            .await
            .expect("Failed to init harness");

        let result = harness.create_test_dataset("test-dataset-1", "Test Dataset").await;
        assert!(result.is_ok(), "Failed to create test dataset");
    }

    #[tokio::test]
    async fn test_harness_training_job_creation() {
        let harness = ApiTestHarness::new()
            .await
            .expect("Failed to init harness");

        // Create prerequisite dataset
        harness.create_test_dataset("test-dataset-1", "Test Dataset")
            .await
            .expect("Failed to create dataset");

        // Create prerequisite adapter
        harness.create_test_adapter("test-adapter-1", "default")
            .await
            .expect("Failed to create adapter");

        // Create training job
        let result = harness.create_test_training_job("test-job-1", "test-dataset-1", "test-adapter-1")
            .await;
        assert!(result.is_ok(), "Failed to create training job");
    }
}
