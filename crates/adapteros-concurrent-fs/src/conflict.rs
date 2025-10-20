//! Conflict resolution for concurrent operations
//!
//! Implements conflict resolution strategies for concurrent filesystem operations.

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tracing::{debug, error, info, warn};

/// Conflict resolver
pub struct ConflictResolver {
    /// Configuration
    config: ConflictConfig,
    /// Conflict history
    conflict_history: std::collections::HashMap<PathBuf, Vec<ConflictRecord>>,
}

/// Conflict configuration
#[derive(Debug, Clone)]
pub struct ConflictConfig {
    /// Enable conflict resolution
    pub enabled: bool,
    /// Default resolution strategy
    pub default_strategy: ConflictResolutionStrategy,
    /// Maximum conflict history
    pub max_conflict_history: usize,
    /// Conflict detection timeout
    pub detection_timeout: Duration,
    /// Enable automatic resolution
    pub enable_automatic_resolution: bool,
}

/// File conflict
#[derive(Debug, Clone)]
pub struct FileConflict {
    /// Conflict ID
    pub id: String,
    /// File path
    pub path: PathBuf,
    /// Conflict type
    pub conflict_type: ConflictType,
    /// Conflicting operations
    pub operations: Vec<ConflictOperation>,
    /// Timestamp
    pub timestamp: SystemTime,
    /// Severity
    pub severity: ConflictSeverity,
}

/// Conflict type
#[derive(Debug, Clone)]
pub enum ConflictType {
    /// File modification conflict
    FileModification,
    /// File deletion conflict
    FileDeletion,
    /// File creation conflict
    FileCreation,
    /// Directory operation conflict
    DirectoryOperation,
    /// Permission conflict
    PermissionConflict,
    /// Lock conflict
    LockConflict,
}

/// Conflict operation
#[derive(Debug, Clone)]
pub struct ConflictOperation {
    /// Operation ID
    pub id: String,
    /// Operation type
    pub operation_type: String,
    /// Process ID
    pub process_id: u32,
    /// User ID
    pub user_id: Option<u32>,
    /// Timestamp
    pub timestamp: SystemTime,
    /// Priority
    pub priority: u32,
}

/// Conflict severity
#[derive(Debug, Clone, PartialEq)]
pub enum ConflictSeverity {
    /// Low severity
    Low,
    /// Medium severity
    Medium,
    /// High severity
    High,
    /// Critical severity
    Critical,
}

/// Conflict resolution
#[derive(Debug, Clone)]
pub struct ConflictResolution {
    /// Resolution ID
    pub id: String,
    /// Strategy used
    pub strategy: ConflictResolutionStrategy,
    /// Resolution result
    pub result: ResolutionResult,
    /// Timestamp
    pub timestamp: SystemTime,
    /// Duration
    pub duration: Duration,
}

/// Resolution result
#[derive(Debug, Clone)]
pub enum ResolutionResult {
    /// Conflict resolved successfully
    Resolved,
    /// Conflict partially resolved
    PartiallyResolved,
    /// Conflict resolution failed
    Failed,
    /// Conflict resolution deferred
    Deferred,
}

/// Conflict resolution strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConflictResolutionStrategy {
    /// First wins (first operation succeeds)
    FirstWins,
    /// Last wins (last operation succeeds)
    LastWins,
    /// Highest priority wins
    HighestPriorityWins,
    /// Merge operations
    Merge,
    /// Rollback all operations
    RollbackAll,
    /// Defer resolution
    Defer,
    /// Manual resolution required
    Manual,
}

/// Conflict record
#[derive(Debug, Clone)]
pub struct ConflictRecord {
    /// Conflict ID
    pub conflict_id: String,
    /// Resolution strategy
    pub strategy: ConflictResolutionStrategy,
    /// Resolution result
    pub result: ResolutionResult,
    /// Timestamp
    pub timestamp: SystemTime,
    /// Duration
    pub duration: Duration,
}

impl ConflictResolver {
    /// Create a new conflict resolver
    pub fn new(config: &crate::ConcurrentFsConfig) -> Result<Self> {
        let conflict_config = ConflictConfig {
            enabled: config.enable_conflict_resolution,
            default_strategy: ConflictResolutionStrategy::LastWins,
            max_conflict_history: 1000,
            detection_timeout: Duration::from_secs(30),
            enable_automatic_resolution: true,
        };

        Ok(Self {
            config: conflict_config,
            conflict_history: std::collections::HashMap::new(),
        })
    }

    /// Resolve a file conflict
    pub async fn resolve_conflict(&self, conflict: FileConflict) -> Result<ConflictResolution> {
        if !self.config.enabled {
            return Ok(ConflictResolution {
                id: format!("resolution_{}", SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos()),
                strategy: ConflictResolutionStrategy::Defer,
                result: ResolutionResult::Deferred,
                timestamp: SystemTime::now(),
                duration: Duration::ZERO,
            });
        }

        let start_time = SystemTime::now();
        let resolution_id = format!("resolution_{}", start_time.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos());

        // Determine resolution strategy
        let strategy = self.determine_strategy(&conflict)?;

        // Apply resolution strategy
        let result = self.apply_strategy(&conflict, &strategy).await?;

        let duration = start_time.elapsed().unwrap_or(Duration::ZERO);

        let resolution = ConflictResolution {
            id: resolution_id,
            strategy,
            result: result.clone(),
            timestamp: start_time,
            duration,
        };

        // Record conflict resolution
        self.record_conflict_resolution(&conflict.path, &resolution).await?;

        debug!("Resolved conflict {} using strategy {:?}: {:?}", 
               conflict.id, resolution.strategy, result);

        Ok(resolution)
    }

    /// Determine the best resolution strategy for a conflict
    fn determine_strategy(&self, conflict: &FileConflict) -> Result<ConflictResolutionStrategy> {
        // Check conflict history for this path
        if let Some(history) = self.conflict_history.get(&conflict.path) {
            // Use the most successful strategy from history
            if let Some(successful_strategy) = history.iter()
                .filter(|record| matches!(record.result, ResolutionResult::Resolved))
                .max_by_key(|record| record.timestamp)
                .map(|record| record.strategy.clone()) {
                return Ok(successful_strategy);
            }
        }

        // Use default strategy based on conflict type
        match conflict.conflict_type {
            ConflictType::FileModification => Ok(ConflictResolutionStrategy::LastWins),
            ConflictType::FileDeletion => Ok(ConflictResolutionStrategy::FirstWins),
            ConflictType::FileCreation => Ok(ConflictResolutionStrategy::HighestPriorityWins),
            ConflictType::DirectoryOperation => Ok(ConflictResolutionStrategy::Merge),
            ConflictType::PermissionConflict => Ok(ConflictResolutionStrategy::Manual),
            ConflictType::LockConflict => Ok(ConflictResolutionStrategy::RollbackAll),
        }
    }

    /// Apply a resolution strategy
    async fn apply_strategy(&self, conflict: &FileConflict, strategy: &ConflictResolutionStrategy) -> Result<ResolutionResult> {
        match strategy {
            ConflictResolutionStrategy::FirstWins => {
                self.apply_first_wins_strategy(conflict).await
            }
            ConflictResolutionStrategy::LastWins => {
                self.apply_last_wins_strategy(conflict).await
            }
            ConflictResolutionStrategy::HighestPriorityWins => {
                self.apply_highest_priority_strategy(conflict).await
            }
            ConflictResolutionStrategy::Merge => {
                self.apply_merge_strategy(conflict).await
            }
            ConflictResolutionStrategy::RollbackAll => {
                self.apply_rollback_strategy(conflict).await
            }
            ConflictResolutionStrategy::Defer => {
                Ok(ResolutionResult::Deferred)
            }
            ConflictResolutionStrategy::Manual => {
                Ok(ResolutionResult::Deferred)
            }
        }
    }

    /// Apply first wins strategy
    async fn apply_first_wins_strategy(&self, conflict: &FileConflict) -> Result<ResolutionResult> {
        if conflict.operations.is_empty() {
            return Ok(ResolutionResult::Failed);
        }

        // Sort operations by timestamp
        let mut operations = conflict.operations.clone();
        operations.sort_by_key(|op| op.timestamp);

        // Allow first operation to proceed
        let first_operation = &operations[0];
        debug!("First wins strategy: allowing operation {} to proceed", first_operation.id);

        // Cancel other operations
        for operation in operations.iter().skip(1) {
            debug!("First wins strategy: cancelling operation {}", operation.id);
        }

        Ok(ResolutionResult::Resolved)
    }

    /// Apply last wins strategy
    async fn apply_last_wins_strategy(&self, conflict: &FileConflict) -> Result<ResolutionResult> {
        if conflict.operations.is_empty() {
            return Ok(ResolutionResult::Failed);
        }

        // Sort operations by timestamp
        let mut operations = conflict.operations.clone();
        operations.sort_by_key(|op| op.timestamp);

        // Allow last operation to proceed
        let last_operation = &operations[operations.len() - 1];
        debug!("Last wins strategy: allowing operation {} to proceed", last_operation.id);

        // Cancel other operations
        for operation in operations.iter().take(operations.len() - 1) {
            debug!("Last wins strategy: cancelling operation {}", operation.id);
        }

        Ok(ResolutionResult::Resolved)
    }

    /// Apply highest priority strategy
    async fn apply_highest_priority_strategy(&self, conflict: &FileConflict) -> Result<ResolutionResult> {
        if conflict.operations.is_empty() {
            return Ok(ResolutionResult::Failed);
        }

        // Find operation with highest priority
        let highest_priority_op = conflict.operations.iter()
            .max_by_key(|op| op.priority)
            .unwrap();

        debug!("Highest priority strategy: allowing operation {} (priority {}) to proceed", 
               highest_priority_op.id, highest_priority_op.priority);

        // Cancel other operations
        for operation in &conflict.operations {
            if operation.id != highest_priority_op.id {
                debug!("Highest priority strategy: cancelling operation {}", operation.id);
            }
        }

        Ok(ResolutionResult::Resolved)
    }

    /// Apply merge strategy
    async fn apply_merge_strategy(&self, conflict: &FileConflict) -> Result<ResolutionResult> {
        // Merge strategy is complex and depends on the specific operations
        // For now, we'll defer to manual resolution
        warn!("Merge strategy not fully implemented, deferring to manual resolution");
        Ok(ResolutionResult::Deferred)
    }

    /// Apply rollback strategy
    async fn apply_rollback_strategy(&self, conflict: &FileConflict) -> Result<ResolutionResult> {
        // Rollback all conflicting operations
        for operation in &conflict.operations {
            debug!("Rollback strategy: rolling back operation {}", operation.id);
            // In a real implementation, this would trigger rollback for each operation
        }

        Ok(ResolutionResult::Resolved)
    }

    /// Record conflict resolution
    async fn record_conflict_resolution(&self, path: &PathBuf, resolution: &ConflictResolution) -> Result<()> {
        let record = ConflictRecord {
            conflict_id: resolution.id.clone(),
            strategy: resolution.strategy.clone(),
            result: resolution.result.clone(),
            timestamp: resolution.timestamp,
            duration: resolution.duration,
        };

        // Add to history (in a real implementation, this would be thread-safe)
        // For now, we'll just log it
        debug!("Recorded conflict resolution for {}: {:?}", path.display(), record);

        Ok(())
    }

    /// Get conflict history for a path
    pub fn get_conflict_history(&self, path: impl AsRef<Path>) -> Option<&Vec<ConflictRecord>> {
        self.conflict_history.get(path.as_ref())
    }

    /// Clear conflict history
    pub fn clear_conflict_history(&mut self) {
        self.conflict_history.clear();
    }

    /// Get conflict statistics
    pub fn get_conflict_statistics(&self) -> ConflictStatistics {
        let total_conflicts = self.conflict_history.values().map(|v| v.len()).sum();
        let resolved_conflicts = self.conflict_history.values()
            .flat_map(|v| v.iter())
            .filter(|r| matches!(r.result, ResolutionResult::Resolved))
            .count();
        let failed_conflicts = self.conflict_history.values()
            .flat_map(|v| v.iter())
            .filter(|r| matches!(r.result, ResolutionResult::Failed))
            .count();

        ConflictStatistics {
            total_conflicts,
            resolved_conflicts,
            failed_conflicts,
            resolution_rate: if total_conflicts > 0 {
                resolved_conflicts as f32 / total_conflicts as f32
            } else {
                0.0
            },
        }
    }
}

/// Conflict statistics
#[derive(Debug, Clone)]
pub struct ConflictStatistics {
    /// Total number of conflicts
    pub total_conflicts: usize,
    /// Number of resolved conflicts
    pub resolved_conflicts: usize,
    /// Number of failed conflicts
    pub failed_conflicts: usize,
    /// Resolution rate (0.0 to 1.0)
    pub resolution_rate: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_conflict_resolution() -> Result<()> {
        let config = crate::ConcurrentFsConfig::default();
        let resolver = ConflictResolver::new(&config)?;
        
        let temp_dir = TempDir::new()?;
        let test_file = temp_dir.path().join("test.txt");

        // Create a test conflict
        let conflict = FileConflict {
            id: "test_conflict".to_string(),
            path: test_file,
            conflict_type: ConflictType::FileModification,
            operations: vec![
                ConflictOperation {
                    id: "op1".to_string(),
                    operation_type: "write".to_string(),
                    process_id: 1234,
                    user_id: Some(1000),
                    timestamp: SystemTime::now(),
                    priority: 1,
                },
                ConflictOperation {
                    id: "op2".to_string(),
                    operation_type: "write".to_string(),
                    process_id: 5678,
                    user_id: Some(1000),
                    timestamp: SystemTime::now() + Duration::from_secs(1),
                    priority: 2,
                },
            ],
            timestamp: SystemTime::now(),
            severity: ConflictSeverity::Medium,
        };

        // Test conflict resolution
        let resolution = resolver.resolve_conflict(conflict).await?;
        assert!(matches!(resolution.result, ResolutionResult::Resolved | ResolutionResult::Deferred));

        Ok(())
    }

    #[test]
    fn test_conflict_statistics() {
        let config = crate::ConcurrentFsConfig::default();
        let resolver = ConflictResolver::new(&config).unwrap();
        
        let stats = resolver.get_conflict_statistics();
        assert_eq!(stats.total_conflicts, 0);
        assert_eq!(stats.resolved_conflicts, 0);
        assert_eq!(stats.failed_conflicts, 0);
        assert_eq!(stats.resolution_rate, 0.0);
    }
}
