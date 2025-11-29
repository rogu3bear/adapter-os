//! Consolidated test fixtures for AdapterOS
//!
//! Provides reusable test fixtures to eliminate duplication across test files.
//! All fixtures follow consistent naming and behavior patterns.

use adapteros_core::{AosError, Result};
use adapteros_db::{users::Role, Db};
use std::collections::HashMap;
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
    bundles_root: String,
    metrics_enabled: bool,
}

impl TestAppStateBuilder {
    /// Create new app state builder
    pub fn new() -> Self {
        Self {
            db: None,
            jwt_secret: b"test-jwt-secret-key-32-bytes-long".to_vec(),
            bundles_root: "test-bundles".to_string(),
            metrics_enabled: true,
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

    /// Disable metrics
    pub fn without_metrics(mut self) -> Self {
        self.metrics_enabled = false;
        self
    }

    /// Build the app state
    pub async fn build(self) -> Result<adapteros_server_api::AppState> {
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

        let api_config = Arc::new(std::sync::RwLock::new(
            adapteros_server_api::state::ApiConfig {
                metrics: adapteros_server_api::state::MetricsConfig {
                    enabled: self.metrics_enabled,
                    bearer_token: "test-bearer-token".to_string(),
                    system_metrics_interval_secs: 30,
                },
                golden_gate: None,
                bundles_root: self.bundles_root,
                rate_limits: None,
            },
        ));

        let metrics_exporter = Arc::new(
            adapteros_metrics_exporter::MetricsExporter::new(vec![0.1, 0.5, 1.0])
                .map_err(|e| AosError::Metrics(format!("Failed to create metrics exporter: {}", e)))?,
        );

        let metrics_collector = Arc::new(adapteros_telemetry::MetricsCollector::new(
            adapteros_telemetry::metrics::MetricsConfig::default(),
        ));

        let metrics_registry =
            Arc::new(adapteros_server_api::telemetry::MetricsRegistry::new());

        let training_service = Arc::new(adapteros_orchestrator::TrainingService::new());

        Ok(adapteros_server_api::AppState::with_sqlite(
            db,
            self.jwt_secret,
            api_config,
            metrics_exporter,
            metrics_collector,
            metrics_registry,
            training_service,
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

    /// Create JWT token for testing
    pub fn create_jwt_token(
        email: &str,
        role: Role,
        tenant_id: &str,
        secret: &[u8],
    ) -> Result<String> {
        use jsonwebtoken::{encode, EncodingKey, Header};

        let claims = adapteros_server_api::auth::Claims {
            sub: email.to_string(),
            role: role.to_string(),
            tenant_id: tenant_id.to_string(),
            exp: (chrono::Utc::now() + chrono::Duration::hours(8)).timestamp() as usize,
        };

        encode(&Header::default(), &claims, &EncodingKey::from_secret(secret))
            .map_err(|e| AosError::Auth(format!("Failed to encode JWT: {}", e)))
    }

    /// Create default admin JWT token
    pub fn default_admin_token() -> Result<String> {
        Self::create_jwt_token(
            Self::DEFAULT_EMAIL,
            Role::Admin,
            "default",
            Self::DEFAULT_JWT_SECRET,
        )
    }

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
    pub async fn create_adapter(
        db: &Db,
        adapter_id: &str,
        tenant_id: &str,
    ) -> Result<()> {
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
            "INSERT INTO adapters (id, tenant_id, hash, tier, rank, activation_pct, created_at)
             VALUES (?, ?, ?, ?, ?, ?, datetime('now'))",
        )
        .bind(adapter_id)
        .bind(tenant_id)
        .bind(&hash)
        .bind(tier)
        .bind(rank)
        .bind(0.0)
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
    pub async fn create_dataset(
        db: &Db,
        dataset_id: &str,
        name: &str,
    ) -> Result<()> {
        let hash = format!("{:0>64}", dataset_id);

        sqlx::query(
            "INSERT INTO training_datasets (id, hash_b3, name, validation_status, created_at)
             VALUES (?, ?, ?, ?, datetime('now'))",
        )
        .bind(dataset_id)
        .bind(&hash)
        .bind(name)
        .bind("valid")
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

        sqlx::query(
            "INSERT INTO training_datasets (id, hash_b3, name, validation_status, created_at)
             VALUES (?, ?, ?, ?, datetime('now'))",
        )
        .bind(dataset_id)
        .bind(&hash)
        .bind(name)
        .bind(validation_status)
        .execute(db.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to create test dataset: {}", e)))?;

        Ok(())
    }

    /// Create multiple test datasets
    pub async fn create_datasets(
        db: &Db,
        count: usize,
        prefix: &str,
    ) -> Result<Vec<String>> {
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
    /// Create test training job
    pub async fn create_job(
        db: &Db,
        job_id: &str,
        dataset_id: &str,
        adapter_id: &str,
        status: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO training_jobs
             (id, dataset_id, adapter_id, status, progress_pct, loss, created_at)
             VALUES (?, ?, ?, ?, ?, ?, datetime('now'))",
        )
        .bind(job_id)
        .bind(dataset_id)
        .bind(adapter_id)
        .bind(status)
        .bind(if status == "completed" { 100 } else { 0 })
        .bind(if status == "completed" { 0.05 } else { 0.0 })
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
            a, b, diff, epsilon
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

        for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
            Self::assert_approx_eq(
                *x,
                *y,
                epsilon,
            );
        }
    }

    /// Compute L2 distance between two vectors
    pub fn l2_distance(a: &[f32], b: &[f32]) -> f32 {
        assert_eq!(a.len(), b.len(), "Vectors must have same length");

        let sum_sq_diff: f32 = a
            .iter()
            .zip(b.iter())
            .map(|(x, y)| (x - y).powi(2))
            .sum();

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

    #[tokio::test]
    async fn test_auth_jwt_creation() {
        let token = TestAuth::default_admin_token().expect("Failed to create JWT");
        assert!(!token.is_empty());
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
