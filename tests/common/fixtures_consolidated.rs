//! Consolidated test fixtures for adapterOS
//!
//! Provides reusable test fixtures to eliminate duplication across test files.
//! All fixtures follow consistent naming and behavior patterns.

#![allow(dead_code)]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::unused_enumerate_index)]
#![allow(clippy::clone_on_copy)]

use adapteros_core::{AosError, Result};
use adapteros_db::{users::Role, Db};
use std::sync::Arc;

/// Test database configuration
#[derive(Debug, Clone)]
pub struct TestDbConfig {
    /// Database URL (default: :memory:)
    pub db_url: String,
    /// Whether to run migrations
    pub run_migrations: bool,
    /// Whether to seed test data
    pub seed_data: bool,
}

impl Default for TestDbConfig {
    fn default() -> Self {
        Self {
            db_url: ":memory:".to_string(),
            run_migrations: true,
            seed_data: true,
        }
    }
}

/// Test database builder with common patterns
pub struct TestDbBuilder {
    config: TestDbConfig,
    tenants: Vec<(String, String)>, // (id, name)
    users: Vec<TestUser>,
}

#[derive(Debug, Clone)]
pub struct TestUser {
    pub email: String,
    pub display_name: String,
    pub password: String,
    pub role: Role,
    pub tenant_id: String,
}

impl TestDbBuilder {
    /// Create new test database builder
    pub fn new() -> Self {
        Self {
            config: TestDbConfig::default(),
            tenants: Vec::new(),
            users: Vec::new(),
        }
    }

    /// Use custom database URL
    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.config.db_url = url.into();
        self
    }

    /// Skip migrations
    pub fn skip_migrations(mut self) -> Self {
        self.config.run_migrations = false;
        self
    }

    /// Add a tenant
    pub fn with_tenant(mut self, id: impl Into<String>, name: impl Into<String>) -> Self {
        self.tenants.push((id.into(), name.into()));
        self
    }

    /// Add default tenant
    pub fn with_default_tenant(self) -> Self {
        self.with_tenant("default", "Default Test Tenant")
    }

    /// Add a user
    pub fn with_user(mut self, user: TestUser) -> Self {
        self.users.push(user);
        self
    }

    /// Add default admin user
    pub fn with_default_admin(self) -> Self {
        self.with_user(TestUser {
            email: "testadmin@example.com".to_string(),
            display_name: "Test Admin".to_string(),
            password: "test-password-123".to_string(),
            role: Role::Admin,
            tenant_id: "default".to_string(),
        })
    }

    /// Build and setup the database
    pub async fn build(self) -> Result<Db> {
        let db = Db::connect(&self.config.db_url).await?;

        if self.config.run_migrations {
            db.migrate().await?;
        }

        if self.config.seed_data {
            // Seed tenants
            for (id, name) in &self.tenants {
                sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES (?, ?, ?)")
                    .bind(id)
                    .bind(name)
                    .bind(0)
                    .execute(db.pool())
                    .await
                    .map_err(|e| AosError::Database(format!("Failed to seed tenant: {}", e)))?;
            }

            // Seed users
            for user in &self.users {
                let password_hash = adapteros_server_api::auth::hash_password(&user.password)
                    .map_err(|e| AosError::Auth(format!("Failed to hash password: {}", e)))?;

                db.create_user(
                    &user.email,
                    &user.display_name,
                    &password_hash,
                    user.role.clone(),
                    &user.tenant_id,
                )
                .await?;
            }
        }

        Ok(db)
    }
}

/// Test AppState builder for API tests
pub struct TestAppStateBuilder {
    db: Option<Db>,
    jwt_secret: Vec<u8>,
}

impl TestAppStateBuilder {
    /// Create new app state builder
    pub fn new() -> Self {
        Self {
            db: None,
            jwt_secret: b"test-jwt-secret-key-32-bytes-long".to_vec(),
        }
    }

    /// Use existing database
    pub fn with_db(mut self, db: Db) -> Self {
        self.db = Some(db);
        self
    }

    /// Use custom JWT secret
    pub fn with_jwt_secret(mut self, secret: Vec<u8>) -> Self {
        self.jwt_secret = secret;
        self
    }

    /// Build the app state
    pub async fn build(self) -> Result<adapteros_server_api::AppState> {
        use adapteros_core::{BackendKind, SeedMode};
        use adapteros_lora_worker::memory::UmaPressureMonitor;
        use adapteros_server_api::config::PathsConfig;
        use adapteros_server_api::state::{ApiConfig, MetricsConfig};

        let db = match self.db {
            Some(db) => db,
            None => {
                TestDbBuilder::new()
                    .with_default_tenant()
                    .with_default_admin()
                    .build()
                    .await?
            }
        };

        // Create paths config with test defaults
        let paths_config = PathsConfig {
            artifacts_root: "var/artifacts".to_string(),
            bundles_root: "var/bundles".to_string(),
            adapters_root: "var/adapters".to_string(),
            plan_dir: "plan".to_string(),
            datasets_root: "var/datasets".to_string(),
            documents_root: "var/documents".to_string(),
            synthesis_model_path: None,
        };

        let mut api_config_inner = ApiConfig::default();
        api_config_inner.metrics = MetricsConfig {
            enabled: true,
            bearer_token: "test-bearer-token".to_string(),
        };
        api_config_inner.directory_analysis_timeout_secs = 120;
        api_config_inner.use_session_stack_for_routing = false;
        api_config_inner.paths = paths_config;
        api_config_inner.seed_mode = SeedMode::BestEffort;
        api_config_inner.backend_profile = BackendKind::Auto;
        api_config_inner.worker_id = 0;

        let api_config = Arc::new(std::sync::RwLock::new(api_config_inner));

        let metrics_exporter = Arc::new(
            adapteros_metrics_exporter::MetricsExporter::new(vec![0.1, 0.5, 1.0]).map_err(|e| {
                AosError::Internal(format!("Failed to create metrics exporter: {}", e))
            })?,
        );

        let metrics_collector = Arc::new(adapteros_telemetry::MetricsCollector::new(
            adapteros_telemetry::metrics::MetricsConfig::default(),
        ));

        let metrics_registry = Arc::new(adapteros_server_api::telemetry::MetricsRegistry::new());

        let uma_monitor = Arc::new(UmaPressureMonitor::new(15, None));

        Ok(adapteros_server_api::AppState::new(
            db,
            self.jwt_secret,
            api_config,
            metrics_exporter,
            metrics_collector,
            metrics_registry,
            uma_monitor,
        ))
    }
}

/// Test authentication helpers
pub struct TestAuth;

impl TestAuth {
    /// Default test credentials
    pub const DEFAULT_EMAIL: &'static str = "testadmin@example.com";
    pub const DEFAULT_PASSWORD: &'static str = "test-password-123";
    pub const DEFAULT_JWT_SECRET: &'static [u8] = b"test-jwt-secret-key-32-bytes-long";

    /// Hash password for testing
    pub fn hash_password(password: &str) -> Result<String> {
        adapteros_server_api::auth::hash_password(password)
            .map_err(|e| AosError::Auth(format!("Failed to hash password: {}", e)))
    }
}

/// Test adapter factory
pub struct TestAdapterFactory;

impl TestAdapterFactory {
    /// Create test adapter in database
    pub async fn create_adapter(db: &Db, adapter_id: &str, tenant_id: &str) -> Result<()> {
        let hash = format!("{:0>64}", adapter_id);

        sqlx::query(
            "INSERT INTO adapters (id, tenant_id, adapter_id, name, tier, hash_b3, rank, alpha, targets_json, lifecycle_state, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))",
        )
        .bind(adapter_id)
        .bind(tenant_id)
        .bind(adapter_id) // Set adapter_id for lookup by pin_adapter
        .bind(format!("Test Adapter {}", adapter_id))
        .bind("persistent")
        .bind(&hash)
        .bind(8)
        .bind(1.0)
        .bind("[]")
        .bind("active") // lifecycle_state
        .execute(db.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to create test adapter: {}", e)))?;

        Ok(())
    }

    /// Create test adapter with custom tier
    pub async fn create_adapter_with_tier(
        db: &Db,
        adapter_id: &str,
        tenant_id: &str,
        tier: &str,
        rank: i64,
    ) -> Result<()> {
        let hash = format!("{:0>64}", adapter_id);

        sqlx::query(
            "INSERT INTO adapters (id, tenant_id, adapter_id, name, tier, hash_b3, rank, alpha, targets_json, lifecycle_state, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))",
        )
        .bind(adapter_id)
        .bind(tenant_id)
        .bind(adapter_id) // Set adapter_id for lookup by pin_adapter
        .bind(format!("Test Adapter {}", adapter_id))
        .bind(tier)
        .bind(&hash)
        .bind(rank)
        .bind(1.0)
        .bind("[]")
        .bind("active") // lifecycle_state
        .execute(db.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to create test adapter: {}", e)))?;

        Ok(())
    }

    /// Create multiple test adapters
    pub async fn create_adapters(
        db: &Db,
        count: usize,
        tenant_id: &str,
        prefix: &str,
    ) -> Result<Vec<String>> {
        let mut adapter_ids = Vec::new();

        for i in 0..count {
            let adapter_id = format!("{}-{:03}", prefix, i);
            Self::create_adapter(db, &adapter_id, tenant_id).await?;
            adapter_ids.push(adapter_id);
        }

        Ok(adapter_ids)
    }
}

/// Test dataset factory
pub struct TestDatasetFactory;

impl TestDatasetFactory {
    /// Create test dataset in database
    pub async fn create_dataset(db: &Db, dataset_id: &str, name: &str) -> Result<()> {
        let hash = format!("{:0>64}", dataset_id);
        let storage_path = format!("var/datasets/{}", dataset_id);

        sqlx::query(
            "INSERT INTO training_datasets (id, hash_b3, name, format, storage_path, validation_status, tenant_id, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'))",
        )
        .bind(dataset_id)
        .bind(&hash)
        .bind(name)
        .bind("jsonl") // default format for test datasets
        .bind(&storage_path)
        .bind("valid")
        .bind("default") // default tenant for test datasets
        .execute(db.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to create test dataset: {}", e)))?;

        Ok(())
    }

    /// Create test dataset with custom validation status
    pub async fn create_dataset_with_status(
        db: &Db,
        dataset_id: &str,
        name: &str,
        validation_status: &str,
    ) -> Result<()> {
        let hash = format!("{:0>64}", dataset_id);
        let storage_path = format!("var/datasets/{}", dataset_id);

        sqlx::query(
            "INSERT INTO training_datasets (id, hash_b3, name, format, storage_path, validation_status, tenant_id, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'))",
        )
        .bind(dataset_id)
        .bind(&hash)
        .bind(name)
        .bind("jsonl") // default format for test datasets
        .bind(&storage_path)
        .bind(validation_status)
        .bind("default") // default tenant for test datasets
        .execute(db.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to create test dataset: {}", e)))?;

        Ok(())
    }

    /// Create multiple test datasets
    pub async fn create_datasets(db: &Db, count: usize, prefix: &str) -> Result<Vec<String>> {
        let mut dataset_ids = Vec::new();

        for i in 0..count {
            let dataset_id = format!("{}-{:03}", prefix, i);
            let name = format!("{} Dataset {:03}", prefix, i);
            Self::create_dataset(db, &dataset_id, &name).await?;
            dataset_ids.push(dataset_id);
        }

        Ok(dataset_ids)
    }
}

/// Test training job factory
pub struct TestTrainingJobFactory;

impl TestTrainingJobFactory {
    /// Create test training job in repository_training_jobs table
    pub async fn create_job(
        db: &Db,
        job_id: &str,
        _dataset_id: &str,
        _adapter_id: &str,
        status: &str,
    ) -> Result<()> {
        // Progress JSON is required - use a minimal valid structure
        let progress_json = serde_json::json!({
            "progress_pct": if status == "completed" { 100.0 } else { 0.0 },
            "current_epoch": if status == "completed" { 3 } else { 0 },
            "total_epochs": 3,
            "current_loss": if status == "completed" { 0.05 } else { 0.0 },
            "learning_rate": 0.001,
            "tokens_per_second": 0.0,
            "error_message": null
        })
        .to_string();

        // Training config JSON - minimal valid structure
        let training_config_json = serde_json::json!({
            "rank": 8,
            "alpha": 16.0,
            "learning_rate": 0.001,
            "batch_size": 4,
            "epochs": 3
        })
        .to_string();

        let test_repo_id = format!("test-repo-{}", job_id);

        // Create test git repository record (FK parent)
        sqlx::query(
            "INSERT OR IGNORE INTO git_repositories
             (id, repo_id, path, branch, analysis_json, evidence_json, security_scan_json, status, created_by)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&test_repo_id)
        .bind(&test_repo_id)
        .bind("var/test-repo")
        .bind("main")
        .bind("{}")
        .bind("{}")
        .bind("{}")
        .bind("analyzed")
        .bind("test@example.com")
        .execute(db.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to create test git repository: {}", e)))?;

        // Create the training job with FK enforcement enabled
        sqlx::query(
            "INSERT INTO repository_training_jobs
             (id, repo_id, training_config_json, status, progress_json, created_by)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(job_id)
        .bind(&test_repo_id)
        .bind(&training_config_json)
        .bind(status)
        .bind(&progress_json)
        .bind("test@example.com")
        .execute(db.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to create test training job: {}", e)))?;

        Ok(())
    }

    /// Create completed training job
    pub async fn create_completed_job(
        db: &Db,
        job_id: &str,
        dataset_id: &str,
        adapter_id: &str,
    ) -> Result<()> {
        Self::create_job(db, job_id, dataset_id, adapter_id, "completed").await
    }
}

/// Common test assertions
pub struct TestAssertions;

impl TestAssertions {
    /// Assert that two f32 values are approximately equal
    pub fn assert_approx_eq(a: f32, b: f32, epsilon: f32) {
        let diff = (a - b).abs();
        assert!(
            diff < epsilon,
            "Values not approximately equal: {} vs {} (diff: {}, epsilon: {})",
            a,
            b,
            diff,
            epsilon
        );
    }

    /// Assert that two vectors are approximately equal
    pub fn assert_vec_approx_eq(a: &[f32], b: &[f32], epsilon: f32) {
        assert_eq!(
            a.len(),
            b.len(),
            "Vectors have different lengths: {} vs {}",
            a.len(),
            b.len()
        );

        for (_i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
            Self::assert_approx_eq(*x, *y, epsilon);
        }
    }

    /// Compute L2 distance between two vectors
    pub fn l2_distance(a: &[f32], b: &[f32]) -> f32 {
        assert_eq!(a.len(), b.len(), "Vectors must have same length");

        let sum_sq_diff: f32 = a.iter().zip(b.iter()).map(|(x, y)| (x - y).powi(2)).sum();

        sum_sq_diff.sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_db_builder_default() {
        let db = TestDbBuilder::new()
            .with_default_tenant()
            .with_default_admin()
            .build()
            .await
            .expect("Failed to build test database");

        // Verify tenant exists
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tenants WHERE id = ?")
            .bind("default")
            .fetch_one(db.pool())
            .await
            .expect("Failed to query tenants");

        assert_eq!(count.0, 1);
    }

    #[tokio::test]
    async fn test_adapter_factory() {
        let db = TestDbBuilder::new()
            .with_default_tenant()
            .build()
            .await
            .expect("Failed to build test database");

        TestAdapterFactory::create_adapter(&db, "test-adapter", "default")
            .await
            .expect("Failed to create adapter");

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM adapters WHERE id = ?")
            .bind("test-adapter")
            .fetch_one(db.pool())
            .await
            .expect("Failed to query adapters");

        assert_eq!(count.0, 1);
    }

    #[test]
    fn test_assertions_approx_eq() {
        TestAssertions::assert_approx_eq(1.0, 1.0001, 0.001);
    }

    #[test]
    #[should_panic]
    fn test_assertions_approx_eq_fails() {
        TestAssertions::assert_approx_eq(1.0, 2.0, 0.001);
    }
}
