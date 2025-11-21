//! Unified database access interface for AdapterOS
//!
//! Provides a centralized interface for all database operations across
//! the system with consistent error handling, connection management,
//! and transaction support.
//!
//! # Citations
//! - CONTRIBUTING.md L118-122: "Follow Rust naming conventions", "Use `cargo clippy` for linting"
//! - CLAUDE.md L50-55: "Database access patterns with SQLite"

use adapteros_core::{AosError, HealthCheckResult, HealthStatus, Result};
use async_trait::async_trait;
use chrono;
use serde::{Deserialize, Serialize};
use sqlx::Column;
use sqlx::Row;
use std::collections::HashMap;
use tracing::{debug, error, info};

/// Database health status - wraps canonical HealthCheckResult with chrono timestamp
pub type DbHealthStatus = HealthCheckResult;

/// Unified database access interface
#[async_trait]
pub trait DatabaseAccess {
    /// Execute a query and return results
    async fn execute_query<T>(&self, query: &str, params: &[&(dyn ToSql + Sync)]) -> Result<Vec<T>>
    where
        T: for<'de> Deserialize<'de>
            + Send
            + Sync
            + Unpin
            + for<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow>;

    /// Execute a query and return a single result
    async fn execute_query_one<T>(
        &self,
        query: &str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Option<T>>
    where
        T: for<'de> Deserialize<'de>
            + Send
            + Sync
            + Unpin
            + for<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow>;

    /// Execute a command (INSERT, UPDATE, DELETE)
    async fn execute_command(&self, command: &str, params: &[&(dyn ToSql + Sync)]) -> Result<u64>;

    /// Begin a transaction
    async fn begin_transaction(&self) -> Result<Box<dyn Transaction + Send + Sync>>;

    /// Get database connection info
    async fn get_connection_info(&self) -> Result<ConnectionInfo>;

    /// Check database health
    async fn health_check(&self) -> Result<HealthCheckResult>;

    /// Get database statistics
    async fn get_statistics(&self) -> Result<DatabaseStatistics>;
}

/// Transaction interface
#[async_trait]
pub trait Transaction: Send + Sync {
    /// Execute a query within the transaction (returns JSON values)
    async fn execute_query(
        &mut self,
        query: &str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<serde_json::Value>>;

    /// Execute a command within the transaction
    async fn execute_command(
        &mut self,
        command: &str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<u64>;

    /// Commit the transaction
    async fn commit(&mut self) -> Result<()>;

    /// Rollback the transaction
    async fn rollback(&mut self) -> Result<()>;
}

/// Database connection information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    /// Database type
    pub database_type: DatabaseType,

    /// Connection string
    pub connection_string: String,

    /// Connection pool size
    pub pool_size: u32,

    /// Active connections
    pub active_connections: u32,

    /// Idle connections
    pub idle_connections: u32,

    /// Connection timeout
    pub connection_timeout_ms: u64,

    /// Query timeout
    pub query_timeout_ms: u64,
}

/// Database types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DatabaseType {
    /// SQLite database
    Sqlite,

    /// PostgreSQL database
    Postgres,

    /// MySQL database
    Mysql,

    /// In-memory database
    InMemory,
}

// Note: HealthStatus and HealthCheckResult are now imported from adapteros_core
// The old HealthStatus struct has been replaced with DbHealthStatus type alias above
// The old HealthState enum is replaced by adapteros_core::HealthStatus

/// Database statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseStatistics {
    /// Total queries executed
    pub total_queries: u64,

    /// Total commands executed
    pub total_commands: u64,

    /// Total transactions
    pub total_transactions: u64,

    /// Failed queries
    pub failed_queries: u64,

    /// Failed commands
    pub failed_commands: u64,

    /// Failed transactions
    pub failed_transactions: u64,

    /// Average query time in milliseconds
    pub average_query_time_ms: f64,

    /// Average command time in milliseconds
    pub average_command_time_ms: f64,

    /// Database size in bytes
    pub database_size_bytes: u64,

    /// Table count
    pub table_count: u32,

    /// Index count
    pub index_count: u32,

    /// Statistics timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// SQL parameter trait
pub trait ToSql {
    /// Convert to SQL parameter
    fn to_sql(&self) -> Result<SqlParameter>;
}

/// SQL parameter types
#[derive(Debug, Clone)]
pub enum SqlParameter {
    /// String parameter
    String(String),

    /// Integer parameter
    Integer(i64),

    /// Float parameter
    Float(f64),

    /// Boolean parameter
    Boolean(bool),

    /// Binary parameter
    Binary(Vec<u8>),

    /// Null parameter
    Null,
}

impl ToSql for String {
    fn to_sql(&self) -> Result<SqlParameter> {
        Ok(SqlParameter::String(self.clone()))
    }
}

impl ToSql for &str {
    fn to_sql(&self) -> Result<SqlParameter> {
        Ok(SqlParameter::String(self.to_string()))
    }
}

impl ToSql for i32 {
    fn to_sql(&self) -> Result<SqlParameter> {
        Ok(SqlParameter::Integer(*self as i64))
    }
}

impl ToSql for i64 {
    fn to_sql(&self) -> Result<SqlParameter> {
        Ok(SqlParameter::Integer(*self))
    }
}

impl ToSql for f64 {
    fn to_sql(&self) -> Result<SqlParameter> {
        Ok(SqlParameter::Float(*self))
    }
}

impl ToSql for bool {
    fn to_sql(&self) -> Result<SqlParameter> {
        Ok(SqlParameter::Boolean(*self))
    }
}

impl ToSql for Vec<u8> {
    fn to_sql(&self) -> Result<SqlParameter> {
        Ok(SqlParameter::Binary(self.clone()))
    }
}

/// Unified database access implementation
pub struct UnifiedDatabaseAccess {
    /// Database connection pool
    connection_pool: sqlx::Pool<sqlx::Sqlite>,

    /// Database statistics
    statistics: std::sync::Arc<tokio::sync::Mutex<DatabaseStatistics>>,

    /// Configuration
    config: DatabaseConfig,
}

/// Database configuration
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    /// Connection string
    pub connection_string: String,

    /// Connection pool size
    pub pool_size: u32,

    /// Connection timeout
    pub connection_timeout_ms: u64,

    /// Query timeout
    pub query_timeout_ms: u64,

    /// Enable query logging
    pub enable_query_logging: bool,

    /// Enable performance monitoring
    pub enable_performance_monitoring: bool,
}

impl UnifiedDatabaseAccess {
    /// Create a new unified database access instance
    pub async fn new(config: DatabaseConfig) -> Result<Self> {
        info!(
            connection_string = %config.connection_string,
            pool_size = config.pool_size,
            "Initializing unified database access"
        );

        let connection_pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(config.pool_size)
            .connect(&config.connection_string)
            .await
            .map_err(|e| AosError::Database(format!("Failed to create connection pool: {}", e)))?;

        let statistics = std::sync::Arc::new(tokio::sync::Mutex::new(DatabaseStatistics {
            total_queries: 0,
            total_commands: 0,
            total_transactions: 0,
            failed_queries: 0,
            failed_commands: 0,
            failed_transactions: 0,
            average_query_time_ms: 0.0,
            average_command_time_ms: 0.0,
            database_size_bytes: 0,
            table_count: 0,
            index_count: 0,
            timestamp: chrono::Utc::now(),
        }));

        info!("Unified database access initialized successfully");

        Ok(Self {
            connection_pool,
            statistics,
            config,
        })
    }

    /// Update statistics
    async fn update_statistics(&self, operation: &str, duration_ms: u64, success: bool) {
        let mut stats = self.statistics.lock().await;
        match operation {
            "query" => {
                stats.total_queries += 1;
                if !success {
                    stats.failed_queries += 1;
                }
                // Update average query time
                let total_time = stats.average_query_time_ms * (stats.total_queries - 1) as f64;
                stats.average_query_time_ms =
                    (total_time + duration_ms as f64) / stats.total_queries as f64;
            }
            "command" => {
                stats.total_commands += 1;
                if !success {
                    stats.failed_commands += 1;
                }
                // Update average command time
                let total_time =
                    stats.average_command_time_ms * (stats.total_commands - 1) as f64;
                stats.average_command_time_ms =
                    (total_time + duration_ms as f64) / stats.total_commands as f64;
            }
            "transaction" => {
                stats.total_transactions += 1;
                if !success {
                    stats.failed_transactions += 1;
                }
            }
            _ => {}
        }
        stats.timestamp = chrono::Utc::now();
    }
}

#[async_trait]
impl DatabaseAccess for UnifiedDatabaseAccess {
    async fn execute_query<T>(&self, query: &str, params: &[&(dyn ToSql + Sync)]) -> Result<Vec<T>>
    where
        T: for<'de> Deserialize<'de>
            + Send
            + Sync
            + Unpin
            + for<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow>,
    {
        let start_time = std::time::Instant::now();

        if self.config.enable_query_logging {
            debug!(query = %query, "Executing database query");
        }

        // Convert parameters to SQLx format
        let mut sqlx_query = sqlx::query_as::<_, T>(query);
        for param in params {
            match param.to_sql()? {
                SqlParameter::String(s) => sqlx_query = sqlx_query.bind(s),
                SqlParameter::Integer(i) => sqlx_query = sqlx_query.bind(i),
                SqlParameter::Float(f) => sqlx_query = sqlx_query.bind(f),
                SqlParameter::Boolean(b) => sqlx_query = sqlx_query.bind(b),
                SqlParameter::Binary(b) => sqlx_query = sqlx_query.bind(b),
                SqlParameter::Null => sqlx_query = sqlx_query.bind(None::<String>),
            }
        }

        let result = sqlx_query.fetch_all(&self.connection_pool).await;

        let duration = start_time.elapsed();
        let success = result.is_ok();

        self.update_statistics("query", duration.as_millis() as u64, success).await;

        match result {
            Ok(rows) => {
                if self.config.enable_query_logging {
                    info!(
                        query = %query,
                        row_count = rows.len(),
                        duration_ms = duration.as_millis(),
                        "Query executed successfully"
                    );
                }
                Ok(rows)
            }
            Err(e) => {
                error!(
                    query = %query,
                    error = %e,
                    duration_ms = duration.as_millis(),
                    "Query execution failed"
                );
                Err(AosError::Database(format!("Query execution failed: {}", e)))
            }
        }
    }

    async fn execute_query_one<T>(
        &self,
        query: &str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Option<T>>
    where
        T: for<'de> Deserialize<'de>
            + Send
            + Sync
            + Unpin
            + for<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow>,
    {
        let start_time = std::time::Instant::now();

        if self.config.enable_query_logging {
            debug!(query = %query, "Executing database query (one)");
        }

        // Convert parameters to SQLx format
        let mut sqlx_query = sqlx::query_as::<_, T>(query);
        for param in params {
            match param.to_sql()? {
                SqlParameter::String(s) => sqlx_query = sqlx_query.bind(s),
                SqlParameter::Integer(i) => sqlx_query = sqlx_query.bind(i),
                SqlParameter::Float(f) => sqlx_query = sqlx_query.bind(f),
                SqlParameter::Boolean(b) => sqlx_query = sqlx_query.bind(b),
                SqlParameter::Binary(b) => sqlx_query = sqlx_query.bind(b),
                SqlParameter::Null => sqlx_query = sqlx_query.bind(None::<String>),
            }
        }

        let result = sqlx_query.fetch_optional(&self.connection_pool).await;

        let duration = start_time.elapsed();
        let success = result.is_ok();

        self.update_statistics("query", duration.as_millis() as u64, success).await;

        match result {
            Ok(row) => {
                if self.config.enable_query_logging {
                    info!(
                        query = %query,
                        found = row.is_some(),
                        duration_ms = duration.as_millis(),
                        "Query executed successfully"
                    );
                }
                Ok(row)
            }
            Err(e) => {
                error!(
                    query = %query,
                    error = %e,
                    duration_ms = duration.as_millis(),
                    "Query execution failed"
                );
                Err(AosError::Database(format!("Query execution failed: {}", e)))
            }
        }
    }

    async fn execute_command(&self, command: &str, params: &[&(dyn ToSql + Sync)]) -> Result<u64> {
        let start_time = std::time::Instant::now();

        if self.config.enable_query_logging {
            debug!(command = %command, "Executing database command");
        }

        // Convert parameters to SQLx format
        let mut sqlx_query = sqlx::query(command);
        for param in params {
            match param.to_sql()? {
                SqlParameter::String(s) => sqlx_query = sqlx_query.bind(s),
                SqlParameter::Integer(i) => sqlx_query = sqlx_query.bind(i),
                SqlParameter::Float(f) => sqlx_query = sqlx_query.bind(f),
                SqlParameter::Boolean(b) => sqlx_query = sqlx_query.bind(b),
                SqlParameter::Binary(b) => sqlx_query = sqlx_query.bind(b),
                SqlParameter::Null => sqlx_query = sqlx_query.bind(None::<String>),
            }
        }

        let result = sqlx_query.execute(&self.connection_pool).await;

        let duration = start_time.elapsed();
        let success = result.is_ok();

        self.update_statistics("command", duration.as_millis() as u64, success).await;

        match result {
            Ok(result) => {
                if self.config.enable_query_logging {
                    info!(
                        command = %command,
                        rows_affected = result.rows_affected(),
                        duration_ms = duration.as_millis(),
                        "Command executed successfully"
                    );
                }
                Ok(result.rows_affected())
            }
            Err(e) => {
                error!(
                    command = %command,
                    error = %e,
                    duration_ms = duration.as_millis(),
                    "Command execution failed"
                );
                Err(AosError::Database(format!(
                    "Command execution failed: {}",
                    e
                )))
            }
        }
    }

    async fn begin_transaction(&self) -> Result<Box<dyn Transaction + Send + Sync>> {
        let start_time = std::time::Instant::now();

        debug!("Beginning database transaction");

        let result = self.connection_pool.begin().await;

        let duration = start_time.elapsed();
        let success = result.is_ok();

        self.update_statistics("transaction", duration.as_millis() as u64, success).await;

        match result {
            Ok(transaction) => {
                info!(
                    duration_ms = duration.as_millis(),
                    "Transaction begun successfully"
                );
                Ok(Box::new(UnifiedTransaction {
                    transaction,
                    connection_pool: self.connection_pool.clone(),
                }) as Box<dyn Transaction + Send + Sync>)
            }
            Err(e) => {
                error!(
                    error = %e,
                    duration_ms = duration.as_millis(),
                    "Failed to begin transaction"
                );
                Err(AosError::Database(format!(
                    "Failed to begin transaction: {}",
                    e
                )))
            }
        }
    }

    async fn get_connection_info(&self) -> Result<ConnectionInfo> {
        Ok(ConnectionInfo {
            database_type: DatabaseType::Sqlite,
            connection_string: self.config.connection_string.clone(),
            pool_size: self.config.pool_size,
            active_connections: self.connection_pool.size(),
            idle_connections: self.connection_pool.num_idle() as u32,
            connection_timeout_ms: self.config.connection_timeout_ms,
            query_timeout_ms: self.config.query_timeout_ms,
        })
    }

    async fn health_check(&self) -> Result<HealthCheckResult> {
        let start_time = std::time::Instant::now();

        let result = sqlx::query("SELECT 1")
            .fetch_one(&self.connection_pool)
            .await;

        let duration = start_time.elapsed();

        match result {
            Ok(_) => {
                info!(
                    response_time_ms = duration.as_millis(),
                    "Database health check passed"
                );
                Ok(HealthCheckResult {
                    status: HealthStatus::Healthy,
                    timestamp: std::time::SystemTime::now(),
                    response_time: duration,
                    error: None,
                    metrics: HashMap::new(),
                })
            }
            Err(e) => {
                error!(
                    error = %e,
                    response_time_ms = duration.as_millis(),
                    "Database health check failed"
                );
                Ok(HealthCheckResult {
                    status: HealthStatus::Unhealthy,
                    timestamp: std::time::SystemTime::now(),
                    response_time: duration,
                    error: Some(e.to_string()),
                    metrics: HashMap::new(),
                })
            }
        }
    }

    async fn get_statistics(&self) -> Result<DatabaseStatistics> {
        let stats = self.statistics.lock().await.clone();
        Ok(stats)
    }
}

/// Unified transaction implementation
pub struct UnifiedTransaction {
    transaction: sqlx::Transaction<'static, sqlx::Sqlite>,
    connection_pool: sqlx::SqlitePool,
}

#[async_trait]
impl Transaction for UnifiedTransaction {
    async fn execute_query(
        &mut self,
        query: &str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<serde_json::Value>> {
        // Convert parameters to SQLx format
        let mut sqlx_query = sqlx::query(query);
        for param in params {
            match param.to_sql()? {
                SqlParameter::String(s) => sqlx_query = sqlx_query.bind(s),
                SqlParameter::Integer(i) => sqlx_query = sqlx_query.bind(i),
                SqlParameter::Float(f) => sqlx_query = sqlx_query.bind(f),
                SqlParameter::Boolean(b) => sqlx_query = sqlx_query.bind(b),
                SqlParameter::Binary(b) => sqlx_query = sqlx_query.bind(b),
                SqlParameter::Null => sqlx_query = sqlx_query.bind(None::<String>),
            }
        }

        let result = sqlx_query.fetch_all(&mut *self.transaction).await;

        match result {
            Ok(rows) => {
                let json_rows: std::result::Result<Vec<serde_json::Value>, _> = rows
                    .into_iter()
                    .map(|row| {
                        let mut map = serde_json::Map::new();
                        for (i, column) in row.columns().iter().enumerate() {
                            let value: serde_json::Value = match row.try_get::<String, _>(i) {
                                Ok(s) => serde_json::Value::String(s),
                                Err(_) => serde_json::Value::Null,
                            };
                            map.insert(column.name().to_string(), value);
                        }
                        Ok(serde_json::Value::Object(map))
                    })
                    .collect();
                Ok(json_rows.map_err(|e: serde_json::Error| AosError::Serialization(e))?)
            }
            Err(e) => Err(AosError::Database(format!(
                "Transaction query failed: {}",
                e
            ))),
        }
    }

    async fn execute_command(
        &mut self,
        command: &str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<u64> {
        // Convert parameters to SQLx format
        let mut sqlx_query = sqlx::query(command);
        for param in params {
            match param.to_sql()? {
                SqlParameter::String(s) => sqlx_query = sqlx_query.bind(s),
                SqlParameter::Integer(i) => sqlx_query = sqlx_query.bind(i),
                SqlParameter::Float(f) => sqlx_query = sqlx_query.bind(f),
                SqlParameter::Boolean(b) => sqlx_query = sqlx_query.bind(b),
                SqlParameter::Binary(b) => sqlx_query = sqlx_query.bind(b),
                SqlParameter::Null => sqlx_query = sqlx_query.bind(None::<String>),
            }
        }

        let result = sqlx_query.execute(&mut *self.transaction).await;

        match result {
            Ok(result) => Ok(result.rows_affected()),
            Err(e) => Err(AosError::Database(format!(
                "Transaction command failed: {}",
                e
            ))),
        }
    }

    async fn commit(&mut self) -> Result<()> {
        let transaction =
            std::mem::replace(&mut self.transaction, self.connection_pool.begin().await?);
        match transaction.commit().await {
            Ok(_) => {
                info!("Transaction committed successfully");
                Ok(())
            }
            Err(e) => {
                error!(error = %e, "Transaction commit failed");
                Err(AosError::Database(format!(
                    "Transaction commit failed: {}",
                    e
                )))
            }
        }
    }

    async fn rollback(&mut self) -> Result<()> {
        let transaction =
            std::mem::replace(&mut self.transaction, self.connection_pool.begin().await?);
        match transaction.rollback().await {
            Ok(_) => {
                info!("Transaction rolled back successfully");
                Ok(())
            }
            Err(e) => {
                error!(error = %e, "Transaction rollback failed");
                Err(AosError::Database(format!(
                    "Transaction rollback failed: {}",
                    e
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_database_access_creation() {
        let config = DatabaseConfig {
            connection_string: "sqlite::memory:".to_string(),
            pool_size: 1,
            connection_timeout_ms: 5000,
            query_timeout_ms: 30000,
            enable_query_logging: true,
            enable_performance_monitoring: true,
        };

        let db_access = UnifiedDatabaseAccess::new(config).await.unwrap();
        assert!(db_access.connection_pool.size() > 0);
    }

    #[tokio::test]
    async fn test_health_check() {
        let config = DatabaseConfig {
            connection_string: "sqlite::memory:".to_string(),
            pool_size: 1,
            connection_timeout_ms: 5000,
            query_timeout_ms: 30000,
            enable_query_logging: false,
            enable_performance_monitoring: false,
        };

        let db_access = UnifiedDatabaseAccess::new(config).await.unwrap();
        let health = db_access.health_check().await.unwrap();
        assert_eq!(health.status, HealthStatus::Healthy);
    }
}
