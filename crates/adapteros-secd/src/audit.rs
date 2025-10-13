//! Audit logging for enclave operations

use adapteros_db::Db;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Audit logger for enclave operations
#[derive(Clone)]
pub struct AuditLogger {
    db: Arc<Mutex<Option<Db>>>,
}

impl AuditLogger {
    /// Create a new audit logger
    pub fn new(db: Option<Db>) -> Self {
        Self {
            db: Arc::new(Mutex::new(db)),
        }
    }

    /// Log an operation to the audit trail
    pub async fn log_operation(
        &self,
        operation: &str,
        artifact_hash: Option<&str>,
        result: Result<(), String>,
    ) {
        let db = self.db.lock().await;
        if let Some(db) = db.as_ref() {
            let (result_str, error_msg) = match result {
                Ok(_) => ("success", None),
                Err(e) => ("error", Some(e)),
            };

            if let Err(e) = db
                .log_enclave_operation(
                    operation,
                    None,
                    artifact_hash,
                    result_str,
                    error_msg.as_deref(),
                )
                .await
            {
                tracing::error!("Failed to log enclave operation to database: {}", e);
            }
        } else {
            tracing::debug!("Audit logger: no database connection, skipping operation log");
        }
    }

    /// Log a successful operation
    pub async fn log_success(&self, operation: &str, artifact_hash: Option<&str>) {
        self.log_operation(operation, artifact_hash, Ok(())).await;
    }

    /// Log a failed operation
    pub async fn log_error(&self, operation: &str, artifact_hash: Option<&str>, error: &str) {
        self.log_operation(operation, artifact_hash, Err(error.to_string()))
            .await;
    }
}
