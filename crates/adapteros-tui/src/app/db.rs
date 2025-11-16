use anyhow::Result;
use sqlx::{sqlite::SqlitePool, Row};
use tracing::{debug, error, info};

/// Database client for TUI
///
/// Provides direct SQLite access without compile-time query validation.
/// All queries use runtime validation for maximum flexibility.
pub struct DbClient {
    pool: Option<SqlitePool>,
}

impl DbClient {
    /// Create a new database client from environment or config file
    pub async fn new() -> Result<Self> {
        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:var/aos.db".to_string());

        info!(database_url = %database_url, "Connecting to database");

        match SqlitePool::connect(&database_url).await {
            Ok(pool) => {
                info!("Database connection established");
                Ok(Self { pool: Some(pool) })
            }
            Err(e) => {
                error!(error = %e, "Failed to connect to database");
                // Return client with no connection - graceful degradation
                Ok(Self { pool: None })
            }
        }
    }

    /// Check if database is connected
    pub fn is_connected(&self) -> bool {
        self.pool.is_some()
    }

    /// Get training jobs count
    pub async fn get_training_jobs_count(&self) -> Result<i64> {
        let pool = match &self.pool {
            Some(p) => p,
            None => return Ok(0),
        };

        let row = sqlx::query("SELECT COUNT(*) as count FROM training_jobs")
            .fetch_one(pool)
            .await?;

        let count: i64 = row.try_get("count")?;
        debug!(count = count, "Training jobs count");
        Ok(count)
    }

    /// Get active training jobs count
    pub async fn get_active_training_jobs_count(&self) -> Result<i64> {
        let pool = match &self.pool {
            Some(p) => p,
            None => return Ok(0),
        };

        let row = sqlx::query(
            "SELECT COUNT(*) as count FROM training_jobs WHERE status IN ('queued', 'running')",
        )
        .fetch_one(pool)
        .await?;

        let count: i64 = row.try_get("count")?;
        debug!(count = count, "Active training jobs count");
        Ok(count)
    }

    /// Get adapters count
    pub async fn get_adapters_count(&self) -> Result<i64> {
        let pool = match &self.pool {
            Some(p) => p,
            None => return Ok(0),
        };

        let row = sqlx::query("SELECT COUNT(*) as count FROM adapters")
            .fetch_one(pool)
            .await?;

        let count: i64 = row.try_get("count")?;
        debug!(count = count, "Adapters count");
        Ok(count)
    }

    /// Get tenants count
    pub async fn get_tenants_count(&self) -> Result<i64> {
        let pool = match &self.pool {
            Some(p) => p,
            None => return Ok(0),
        };

        let row = sqlx::query("SELECT COUNT(*) as count FROM tenants")
            .fetch_one(pool)
            .await?;

        let count: i64 = row.try_get("count")?;
        debug!(count = count, "Tenants count");
        Ok(count)
    }

    /// Get recent training jobs
    pub async fn get_recent_training_jobs(&self, limit: i64) -> Result<Vec<TrainingJobRow>> {
        let pool = match &self.pool {
            Some(p) => p,
            None => return Ok(vec![]),
        };

        let rows = sqlx::query(
            "SELECT id, tenant_id, status, created_at, started_at, completed_at
             FROM training_jobs
             ORDER BY created_at DESC
             LIMIT ?",
        )
        .bind(limit)
        .fetch_all(pool)
        .await?;

        let jobs: Vec<TrainingJobRow> = rows
            .iter()
            .map(|row| TrainingJobRow {
                id: row.try_get("id").unwrap_or_default(),
                tenant_id: row.try_get("tenant_id").unwrap_or_default(),
                status: row.try_get("status").unwrap_or_default(),
                created_at: row.try_get("created_at").ok(),
                started_at: row.try_get("started_at").ok(),
                completed_at: row.try_get("completed_at").ok(),
            })
            .collect();

        debug!(count = jobs.len(), "Fetched recent training jobs");
        Ok(jobs)
    }

    /// Get recent adapters
    pub async fn get_recent_adapters(&self, limit: i64) -> Result<Vec<AdapterRow>> {
        let pool = match &self.pool {
            Some(p) => p,
            None => return Ok(vec![]),
        };

        let rows = sqlx::query(
            "SELECT id, name, version, tenant_id, created_at
             FROM adapters
             ORDER BY created_at DESC
             LIMIT ?",
        )
        .bind(limit)
        .fetch_all(pool)
        .await?;

        let adapters: Vec<AdapterRow> = rows
            .iter()
            .map(|row| AdapterRow {
                id: row.try_get("id").unwrap_or_default(),
                name: row.try_get("name").unwrap_or_default(),
                version: row.try_get("version").unwrap_or_default(),
                tenant_id: row.try_get("tenant_id").unwrap_or_default(),
                created_at: row.try_get("created_at").ok(),
            })
            .collect();

        debug!(count = adapters.len(), "Fetched recent adapters");
        Ok(adapters)
    }

    /// Get database stats summary
    pub async fn get_stats_summary(&self) -> Result<DbStatsSummary> {
        if self.pool.is_none() {
            return Ok(DbStatsSummary {
                total_adapters: 0,
                total_training_jobs: 0,
                active_training_jobs: 0,
                total_tenants: 0,
                database_connected: false,
            });
        }

        // Run all queries in parallel
        let (adapters_count, training_count, active_training_count, tenants_count) = tokio::try_join!(
            self.get_adapters_count(),
            self.get_training_jobs_count(),
            self.get_active_training_jobs_count(),
            self.get_tenants_count(),
        )?;

        Ok(DbStatsSummary {
            total_adapters: adapters_count,
            total_training_jobs: training_count,
            active_training_jobs: active_training_count,
            total_tenants: tenants_count,
            database_connected: true,
        })
    }
}

// Data types for database rows

#[derive(Debug, Clone)]
pub struct TrainingJobRow {
    pub id: String,
    pub tenant_id: String,
    pub status: String,
    pub created_at: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AdapterRow {
    pub id: String,
    pub name: String,
    pub version: String,
    pub tenant_id: String,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DbStatsSummary {
    pub total_adapters: i64,
    pub total_training_jobs: i64,
    pub active_training_jobs: i64,
    pub total_tenants: i64,
    pub database_connected: bool,
}
