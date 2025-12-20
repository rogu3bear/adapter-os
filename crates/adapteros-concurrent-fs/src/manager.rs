//! Concurrent filesystem operation manager
//!
//! Manages concurrent filesystem operations with conflict resolution
//! and operation tracking for AdapterOS.

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Information about an active filesystem operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationInfo {
    /// Path being operated on
    pub path: PathBuf,
    /// Type of operation (read, write, delete, etc.)
    pub operation_type: String,
    /// When the operation started
    pub started_at: SystemTime,
    /// Operation identifier
    pub operation_id: String,
    /// Optional timeout for the operation
    pub timeout: Option<Duration>,
}

/// Concurrent filesystem operation manager
pub struct ConcurrentManager {
    /// Map of active operations by operation ID
    active_operations: RwLock<HashMap<String, OperationInfo>>,
    /// Map of path locks by path
    path_locks: RwLock<HashMap<PathBuf, String>>,
    /// Default operation timeout
    default_timeout: Duration,
}

impl ConcurrentManager {
    /// Create a new concurrent manager
    pub fn new() -> Self {
        Self {
            active_operations: RwLock::new(HashMap::new()),
            path_locks: RwLock::new(HashMap::new()),
            default_timeout: Duration::from_secs(300), // 5 minutes
        }
    }

    /// Create a new concurrent manager with custom timeout
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            active_operations: RwLock::new(HashMap::new()),
            path_locks: RwLock::new(HashMap::new()),
            default_timeout: timeout,
        }
    }

    /// Register a new operation
    pub async fn register_operation(
        &self,
        path: PathBuf,
        operation_type: String,
        operation_id: Option<String>,
    ) -> Result<String> {
        let op_id = operation_id.unwrap_or_else(|| {
            format!(
                "op_{}_{}",
                operation_type,
                SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )
        });

        let operation_info = OperationInfo {
            path: path.clone(),
            operation_type,
            started_at: SystemTime::now(),
            operation_id: op_id.clone(),
            timeout: Some(self.default_timeout),
        };

        // Check for conflicts
        self.check_conflicts(&path, &operation_info).await?;

        // Register the operation
        {
            let mut operations = self.active_operations.write().await;
            operations.insert(op_id.clone(), operation_info);
        }

        {
            let mut locks = self.path_locks.write().await;
            locks.insert(path.clone(), op_id.clone());
        }

        info!("Registered operation {} for path {}", op_id, path.display());
        Ok(op_id)
    }

    /// Unregister an operation
    pub async fn unregister_operation(&self, operation_id: &str) -> Result<()> {
        let mut removed_path = None;

        {
            let mut operations = self.active_operations.write().await;
            if let Some(operation) = operations.remove(operation_id) {
                removed_path = Some(operation.path);
                debug!("Unregistered operation {}", operation_id);
            }
        }

        if let Some(path) = removed_path {
            let mut locks = self.path_locks.write().await;
            locks.remove(&path);
        }

        Ok(())
    }

    /// Get information about an operation
    pub async fn get_operation(&self, operation_id: &str) -> Option<OperationInfo> {
        let operations = self.active_operations.read().await;
        operations.get(operation_id).cloned()
    }

    /// List all active operations
    pub async fn list_operations(&self) -> Vec<OperationInfo> {
        let operations = self.active_operations.read().await;
        operations.values().cloned().collect()
    }

    /// Check for operation conflicts
    async fn check_conflicts(&self, path: &PathBuf, operation: &OperationInfo) -> Result<()> {
        let locks = self.path_locks.read().await;

        if let Some(existing_op_id) = locks.get(path) {
            let operations = self.active_operations.read().await;
            if let Some(existing_op) = operations.get(existing_op_id) {
                // Check if operations are compatible
                if !self
                    .operations_compatible(&operation.operation_type, &existing_op.operation_type)
                {
                    return Err(AosError::Concurrency(format!(
                        "Operation conflict: {} conflicts with existing {} on path {}",
                        operation.operation_type,
                        existing_op.operation_type,
                        path.display()
                    )));
                }
            }
        }

        Ok(())
    }

    /// Check if two operation types are compatible
    fn operations_compatible(&self, op1: &str, op2: &str) -> bool {
        match (op1, op2) {
            ("read", "read") => true,
            ("read", "write") => false,
            ("write", "read") => false,
            ("write", "write") => false,
            ("delete", _) => false,
            (_, "delete") => false,
            _ => true, // Unknown operations are considered compatible
        }
    }

    /// Clean up expired operations
    pub async fn cleanup_expired(&self) -> Result<()> {
        let now = SystemTime::now();
        let mut expired_operations = Vec::new();

        {
            let operations = self.active_operations.read().await;
            for (op_id, operation) in operations.iter() {
                if let Some(timeout) = operation.timeout {
                    if now
                        .duration_since(operation.started_at)
                        .unwrap_or(Duration::ZERO)
                        > timeout
                    {
                        expired_operations.push(op_id.clone());
                    }
                }
            }
        }

        for op_id in expired_operations {
            warn!("Cleaning up expired operation: {}", op_id);
            self.unregister_operation(&op_id).await?;
        }

        Ok(())
    }

    /// Get operation statistics
    pub async fn get_stats(&self) -> OperationStats {
        let operations = self.active_operations.read().await;
        let locks = self.path_locks.read().await;

        let mut stats = OperationStats {
            total_operations: operations.len(),
            active_locks: locks.len(),
            operation_types: HashMap::new(),
        };

        for operation in operations.values() {
            *stats
                .operation_types
                .entry(operation.operation_type.clone())
                .or_insert(0) += 1;
        }

        stats
    }
}

impl Default for ConcurrentManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about concurrent operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationStats {
    /// Total number of active operations
    pub total_operations: usize,
    /// Number of active path locks
    pub active_locks: usize,
    /// Count of operations by type
    pub operation_types: HashMap<String, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn new_test_tempdir() -> Result<TempDir> {
        let root = PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&root)?;
        Ok(TempDir::new_in(&root)?)
    }

    #[tokio::test]
    async fn test_register_operation() -> Result<()> {
        let manager = ConcurrentManager::new();
        let temp_dir = new_test_tempdir()?;
        let test_path = temp_dir.path().join("test.txt");

        let op_id = manager
            .register_operation(test_path, "read".to_string(), None)
            .await?;
        assert!(!op_id.is_empty());

        let operation = manager.get_operation(&op_id).await;
        assert!(operation.is_some());

        manager.unregister_operation(&op_id).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_operation_conflict() -> Result<()> {
        let manager = ConcurrentManager::new();
        let temp_dir = new_test_tempdir()?;
        let test_path = temp_dir.path().join("test.txt");

        // Register read operation
        let read_op = manager
            .register_operation(test_path.clone(), "read".to_string(), None)
            .await?;

        // Try to register conflicting write operation
        let result = manager
            .register_operation(test_path, "write".to_string(), None)
            .await;
        assert!(result.is_err());

        manager.unregister_operation(&read_op).await?;
        Ok(())
    }
}
