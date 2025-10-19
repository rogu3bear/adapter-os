//! PostgreSQL connection pool and query methods
//!
//! Production backend for AdapterOS registry and state management.
//! Replaces SQLite for multi-node deployments with pgvector support.

use adapteros_core::{AosError, Result};
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;

/// Database connection pool for PostgreSQL
#[derive(Clone)]
pub struct PostgresDb {
    pool: PgPool,
}

impl PostgresDb {
    /// Connect to PostgreSQL database with connection pooling
    ///
    /// # Connection String Format
    /// `postgresql://user:password@host:port/database`
    ///
    /// # Example
    /// ```no_run
    /// use adapteros_db::postgres::PostgresDb;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let db = PostgresDb::connect("postgresql://aos:aos@localhost/adapteros").await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn connect(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(20) // Connection pool size
            .min_connections(2) // Minimum idle connections
            .acquire_timeout(Duration::from_secs(5))
            .idle_timeout(Duration::from_secs(300))
            .max_lifetime(Duration::from_secs(1800))
            .connect(database_url)
            .await
            .map_err(|e| AosError::Database(format!("Failed to connect to PostgreSQL: {}", e)))?;

        tracing::info!("Connected to PostgreSQL database");
        Ok(Self { pool })
    }

    /// Connect using DATABASE_URL environment variable
    ///
    /// Falls back to local PostgreSQL if not set.
    pub async fn connect_env() -> Result<Self> {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://aos:aos@localhost/adapteros".to_string());

        tracing::info!(
            "Connecting to PostgreSQL using: {}",
            database_url.split('@').next_back().unwrap_or("unknown")
        );

        Self::connect(&database_url).await
    }

    /// Run database migrations
    ///
    /// Applies all SQL migrations from the `migrations_postgres/` directory.
    /// Migrations are idempotent and can be run multiple times safely.
    pub async fn migrate(&self) -> Result<()> {
        sqlx::migrate!("../../migrations_postgres")
            .run(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Migration failed: {}", e)))?;

        tracing::info!("Database migrations completed successfully");
        Ok(())
    }

    /// Health check - verify database connectivity
    ///
    /// Returns `Ok(())` if database is reachable and responsive.
    pub async fn health_check(&self) -> Result<()> {
        sqlx::query("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Health check failed: {}", e)))?;

        Ok(())
    }

    /// Get connection pool statistics
    ///
    /// Returns current pool size and idle connections.
    pub fn pool_stats(&self) -> (u32, u32) {
        (self.pool.size(), self.pool.num_idle() as u32)
    }

    /// Seed database with development data
    ///
    /// **WARNING:** Only use in development environments.
    /// Creates sample users, nodes, and tenants for testing.
    pub async fn seed_dev_data(&self) -> Result<()> {
        use argon2::{
            password_hash::{PasswordHasher, SaltString},
            Argon2,
        };
        use rand::rngs::OsRng;

        tracing::info!("Seeding development data...");

        // Create default tenant
        sqlx::query(
            "INSERT INTO tenants (id, name, org_id, isolation_mode, max_memory_gb, status, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, NOW())
             ON CONFLICT (id) DO NOTHING"
        )
        .bind("default")
        .bind("Default Tenant")
        .bind("org-001")
        .bind("process")
        .bind(64)
        .bind("active")
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to create tenant: {}", e)))?;

        // Create development users
        let argon2 = Argon2::default();
        let salt = SaltString::generate(&mut OsRng);
        let dev_password_hash = argon2
            .hash_password(b"aos123", &salt)
            .map_err(|e| AosError::Database(format!("Password hashing failed: {}", e)))?
            .to_string();

        let users = vec![
            (
                "admin@adapteros.dev",
                "Admin User",
                "admin",
                &dev_password_hash,
            ),
            (
                "operator@adapteros.dev",
                "Operator User",
                "operator",
                &dev_password_hash,
            ),
            ("sre@adapteros.dev", "SRE User", "sre", &dev_password_hash),
        ];

        for (email, display_name, role, pwd_hash) in users {
            let username = email.split('@').next().unwrap_or("unknown");

            sqlx::query(
                "INSERT INTO users (id, email, display_name, pw_hash, role, disabled, created_at)
                 VALUES ($1, $2, $3, $4, $5, false, NOW())
                 ON CONFLICT (id) DO NOTHING",
            )
            .bind(format!("{}-user", username))
            .bind(email)
            .bind(display_name)
            .bind(pwd_hash)
            .bind(role)
            .execute(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to create user {}: {}", email, e)))?;
        }

        // Create sample nodes
        let nodes = vec![
            ("node-01", "m1-max-01.local", "M1 Max", 64),
            ("node-02", "m2-ultra-01.local", "M2 Ultra", 128),
            ("node-03", "m3-max-01.local", "M3 Max", 96),
        ];

        for (id, hostname, family, memory) in nodes {
            sqlx::query(
                "INSERT INTO nodes (id, tenant_id, hostname, metal_family, memory_gb, status, last_heartbeat)
                 VALUES ($1, $2, $3, $4, $5, $6, NOW())
                 ON CONFLICT (id) DO NOTHING"
            )
            .bind(id)
            .bind("default")
            .bind(hostname)
            .bind(family)
            .bind(memory)
            .bind("online")
            .execute(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to create node {}: {}", id, e)))?;
        }

        tracing::info!("Development data seeded successfully");
        Ok(())
    }

    /// Get the underlying pool for custom queries
    ///
    /// Use this for complex queries not covered by the API methods.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Close the database connection pool gracefully
    ///
    /// Waits for active connections to complete before closing.
    pub async fn close(self) {
        self.pool.close().await;
        tracing::info!("PostgreSQL connection pool closed");
    }
}

// Sub-modules for specific data models
pub mod adapters;
pub use adapters::AdapterRow;

// Re-export sqlx types for PostgreSQL
pub use sqlx::postgres::{PgQueryResult, PgRow};
pub use sqlx::Row as PgRowTrait;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires PostgreSQL server
    async fn test_postgres_connection() {
        let db = PostgresDb::connect("postgresql://aos:aos@localhost/adapteros_test")
            .await
            .expect("Failed to connect to test database");

        db.health_check().await.expect("Health check failed");

        let (size, idle) = db.pool_stats();
        assert!(size > 0, "Pool should have connections");
        assert!(idle > 0, "Pool should have idle connections");

        db.close().await;
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL server
    async fn test_postgres_migration() {
        let db = PostgresDb::connect("postgresql://aos:aos@localhost/adapteros_test")
            .await
            .expect("Failed to connect");

        db.migrate().await.expect("Migration failed");
        db.close().await;
    }
}
