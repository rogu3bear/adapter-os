//! Training service - extracts training business logic from handlers
//!
//! This module provides a service layer for training job management,
//! separating business logic (validation, capacity checks, policy enforcement)
//! from HTTP handler concerns.
//!
//! Pattern: Service wraps orchestrator's TrainingService and adds API-level concerns
//! like tenant isolation, capacity limits, and policy enforcement.

use crate::state::AppState;
use adapteros_core::error::AosError;
use adapteros_lora_worker::memory::MemoryPressureLevel;
use adapteros_orchestrator::TrainingService as OrchestratorTrainingService;
use async_trait::async_trait;
use std::sync::Arc;
use tracing::{error, warn};

pub type Result<T> = std::result::Result<T, AosError>;

/// Training job validation result
#[derive(Debug, Clone)]
pub struct TrainingValidationResult {
    pub is_valid: bool,
    pub error_message: Option<String>,
    pub error_code: Option<String>,
}

/// Training capacity information
#[derive(Debug, Clone)]
pub struct TrainingCapacityInfo {
    pub running_jobs: usize,
    pub max_concurrent_jobs: usize,
    pub available_slots: usize,
    pub memory_pressure: String,
    pub can_start_new_job: bool,
}

fn trust_state_block_rule(state: &str) -> Option<(&'static str, &'static str)> {
    match state {
        "allowed" | "allowed_with_warning" => None,
        "blocked" => Some((
            "DATASET_TRUST_BLOCKED",
            "Dataset trust_state is blocked; override or adjust the dataset to proceed.",
        )),
        "needs_approval" | "unknown" => Some((
            "DATASET_TRUST_NEEDS_APPROVAL",
            "Dataset trust_state requires approval or validation before training.",
        )),
        _ => Some((
            "DATASET_TRUST_NEEDS_APPROVAL",
            "Dataset trust_state requires approval or validation before training.",
        )),
    }
}

/// Training service trait for job management
///
/// This trait defines API-level operations for training jobs,
/// including validation, capacity checks, and policy enforcement.
///
/// Implementations should:
/// - Validate tenant isolation
/// - Check capacity limits (concurrent jobs, memory pressure)
/// - Enforce policy requirements (evidence policy, dataset validation)
/// - Delegate actual training execution to orchestrator's TrainingService
#[async_trait]
pub trait TrainingService: Send + Sync {
    /// Validate training request prerequisites
    ///
    /// Checks:
    /// - Dataset exists and is validated
    /// - Collection exists (if specified)
    /// - Tenant has access to dataset/collection
    /// - Evidence policy compliance (if enforced)
    ///
    /// # Arguments
    /// * `tenant_id` - Tenant requesting training
    /// * `dataset_id` - Optional dataset to validate
    /// * `collection_id` - Optional collection to validate
    /// * `check_evidence_policy` - Whether to enforce evidence policy
    ///
    /// # Returns
    /// Validation result with error details if invalid
    async fn validate_training_request(
        &self,
        tenant_id: &str,
        dataset_id: Option<&str>,
        collection_id: Option<&str>,
        check_evidence_policy: bool,
    ) -> Result<TrainingValidationResult>;

    /// Check training capacity and resource availability
    ///
    /// Checks:
    /// - Number of running jobs vs max concurrent limit
    /// - Memory pressure level (blocks if Critical)
    /// - Available training slots
    ///
    /// # Returns
    /// Capacity information including whether new jobs can start
    async fn check_training_capacity(&self) -> Result<TrainingCapacityInfo>;

    /// Check if training can proceed based on capacity and memory
    ///
    /// Convenience method that checks both capacity limits and memory pressure.
    ///
    /// # Returns
    /// Ok(()) if training can proceed, Err with specific reason if blocked
    async fn can_start_training(&self) -> Result<()>;
}

/// Default implementation of TrainingService using AppState
///
/// This implementation wraps the orchestrator's TrainingService and adds
/// API-level validation, capacity checks, and policy enforcement.
pub struct DefaultTrainingService {
    state: Arc<AppState>,
}

impl DefaultTrainingService {
    /// Create a new training service
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    /// Get the underlying orchestrator training service
    pub fn orchestrator(&self) -> &Arc<OrchestratorTrainingService> {
        &self.state.training_service
    }
}

#[async_trait]
impl TrainingService for DefaultTrainingService {
    async fn validate_training_request(
        &self,
        tenant_id: &str,
        dataset_id: Option<&str>,
        collection_id: Option<&str>,
        check_evidence_policy: bool,
    ) -> Result<TrainingValidationResult> {
        // Validate dataset if provided
        if let Some(ds_id) = dataset_id {
            let dataset = self
                .state
                .db
                .get_training_dataset(ds_id)
                .await
                .map_err(|e| {
                    error!(dataset_id = %ds_id, error = %e, "Failed to get dataset for validation");
                    AosError::Database(format!("Failed to verify dataset: {}", e))
                })?;

            match dataset {
                Some(ds) => {
                    // Check tenant isolation
                    if let Some(ref dataset_tenant_id) = ds.tenant_id {
                        if dataset_tenant_id != tenant_id {
                            return Ok(TrainingValidationResult {
                                is_valid: false,
                                error_message: Some(format!(
                                    "Access denied: dataset belongs to different tenant"
                                )),
                                error_code: Some("TENANT_ISOLATION_ERROR".to_string()),
                            });
                        }
                    }

                    // Ensure a dataset version exists and evaluate trust state
                    let version_id = self
                        .state
                        .db
                        .ensure_dataset_version_exists(ds_id)
                        .await
                        .map_err(|e| {
                            AosError::Database(format!(
                                "Failed to ensure dataset version for {}: {}",
                                ds_id, e
                            ))
                        })?;

                    let effective_trust = self
                        .state
                        .db
                        .get_effective_trust_state(&version_id)
                        .await
                        .map_err(|e| {
                            AosError::Database(format!(
                                "Failed to load trust state for dataset version {}: {}",
                                version_id, e
                            ))
                        })?
                        .unwrap_or_else(|| "needs_approval".to_string());

                    if let Some((code, guidance)) = trust_state_block_rule(&effective_trust) {
                        return Ok(TrainingValidationResult {
                            is_valid: false,
                            error_message: Some(format!(
                                "{} (dataset_version_id: {}, trust_state: {})",
                                guidance, version_id, effective_trust
                            )),
                            error_code: Some(code.to_string()),
                        });
                    }

                    // If evidence policy is enforced, require structural valid
                    if check_evidence_policy && ds.validation_status != "valid" {
                        return Ok(TrainingValidationResult {
                            is_valid: false,
                            error_message: Some(format!(
                                "Dataset {} is not validated (status: {})",
                                ds_id, ds.validation_status
                            )),
                            error_code: Some("VALIDATION_ERROR".to_string()),
                        });
                    }
                }
                None => {
                    return Ok(TrainingValidationResult {
                        is_valid: false,
                        error_message: Some(format!("Dataset not found: {}", ds_id)),
                        error_code: Some("NOT_FOUND".to_string()),
                    });
                }
            }
        }

        // Validate collection if provided
        if let Some(col_id) = collection_id {
            let collection = self.state.db.get_collection(tenant_id, col_id).await.map_err(|e| {
                error!(collection_id = %col_id, error = %e, "Failed to get collection for validation");
                AosError::Database(format!("Failed to verify collection: {}", e))
            })?;

            match collection {
                Some(col) => {
                    // Check tenant isolation
                    if col.tenant_id != tenant_id {
                        return Ok(TrainingValidationResult {
                            is_valid: false,
                            error_message: Some(format!(
                                "Access denied: collection belongs to different tenant"
                            )),
                            error_code: Some("TENANT_ISOLATION_ERROR".to_string()),
                        });
                    }
                }
                None => {
                    return Ok(TrainingValidationResult {
                        is_valid: false,
                        error_message: Some(format!("Collection not found: {}", col_id)),
                        error_code: Some("NOT_FOUND".to_string()),
                    });
                }
            }
        }

        // Check evidence policy if required
        if check_evidence_policy && dataset_id.is_none() {
            return Ok(TrainingValidationResult {
                is_valid: false,
                error_message: Some(
                    "Evidence policy requires a validated dataset for training".to_string(),
                ),
                error_code: Some("POLICY_VIOLATION".to_string()),
            });
        }

        Ok(TrainingValidationResult {
            is_valid: true,
            error_message: None,
            error_code: None,
        })
    }

    async fn check_training_capacity(&self) -> Result<TrainingCapacityInfo> {
        // Get max concurrent jobs from config
        let max_concurrent = {
            let config = self.state.config.read().map_err(|e| {
                error!(error = %e, "Config lock poisoned");
                AosError::Other("Failed to read config".to_string())
            })?;
            config.capacity_limits.max_concurrent_training_jobs
        };

        // Count running jobs
        // Use the canonical training jobs table; prior query pointed to a non-existent table.
        // Counting running jobs here feeds capacity and memory guardrails.
        let running_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM repository_training_jobs WHERE status = 'running'",
        )
        .fetch_one(self.state.db.pool())
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to check running training jobs count");
            AosError::Database(format!("Failed to check training capacity: {}", e))
        })? as usize;

        // Get memory pressure
        let pressure = self.state.uma_monitor.get_current_pressure();
        let pressure_str = match pressure {
            MemoryPressureLevel::Low => "low",
            MemoryPressureLevel::Medium => "medium",
            MemoryPressureLevel::High => "high",
            MemoryPressureLevel::Critical => "critical",
        };

        let available_slots = max_concurrent.saturating_sub(running_count);
        let can_start_new_job =
            running_count < max_concurrent && pressure != MemoryPressureLevel::Critical;

        Ok(TrainingCapacityInfo {
            running_jobs: running_count,
            max_concurrent_jobs: max_concurrent,
            available_slots,
            memory_pressure: pressure_str.to_string(),
            can_start_new_job,
        })
    }

    async fn can_start_training(&self) -> Result<()> {
        let capacity = self.check_training_capacity().await?;

        // Check capacity limit
        if capacity.running_jobs >= capacity.max_concurrent_jobs {
            warn!(
                running_count = capacity.running_jobs,
                max_concurrent = capacity.max_concurrent_jobs,
                "Training job rejected: maximum concurrent training jobs limit reached"
            );
            return Err(AosError::Validation(format!(
                "Maximum concurrent training jobs limit reached ({}/{}). Please wait for existing jobs to complete.",
                capacity.running_jobs, capacity.max_concurrent_jobs
            )));
        }

        // Check memory pressure
        if capacity.memory_pressure == "critical" {
            warn!("Training job blocked due to Critical memory pressure");
            return Err(AosError::Validation(
                "Training jobs are currently blocked due to critical memory pressure. Please wait for memory pressure to decrease.".to_string()
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_result_construction() {
        let valid = TrainingValidationResult {
            is_valid: true,
            error_message: None,
            error_code: None,
        };
        assert!(valid.is_valid);
        assert!(valid.error_message.is_none());

        let invalid = TrainingValidationResult {
            is_valid: false,
            error_message: Some("Test error".to_string()),
            error_code: Some("TEST_ERROR".to_string()),
        };
        assert!(!invalid.is_valid);
        assert_eq!(invalid.error_message.as_deref(), Some("Test error"));
    }

    #[test]
    fn test_capacity_info_construction() {
        let info = TrainingCapacityInfo {
            running_jobs: 3,
            max_concurrent_jobs: 5,
            available_slots: 2,
            memory_pressure: "normal".to_string(),
            can_start_new_job: true,
        };
        assert_eq!(info.running_jobs, 3);
        assert_eq!(info.available_slots, 2);
        assert!(info.can_start_new_job);
    }

    #[test]
    fn trust_block_code_matches_states() {
        assert_eq!(
            trust_state_block_rule("blocked"),
            Some((
                "DATASET_TRUST_BLOCKED",
                "Dataset trust_state is blocked; override or adjust the dataset to proceed."
            ))
        );
        assert_eq!(
            trust_state_block_rule("needs_approval"),
            Some((
                "DATASET_TRUST_NEEDS_APPROVAL",
                "Dataset trust_state requires approval or validation before training."
            ))
        );
        assert_eq!(trust_state_block_rule("allowed"), None);
        assert_eq!(trust_state_block_rule("allowed_with_warning"), None);
        assert_eq!(
            trust_state_block_rule("unknown"),
            Some((
                "DATASET_TRUST_NEEDS_APPROVAL",
                "Dataset trust_state requires approval or validation before training."
            ))
        );
    }
}
