//! Promotion database operations for golden run promotion workflows
//!
//! This module provides database methods for managing golden run promotions,
//! including promotion requests, gates, approvals, stages, and history.

use crate::query_helpers::db_err;
use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use sqlx::Row;

// ===== Data Models =====

/// Promotion request record from database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionRequest {
    pub request_id: String,
    pub release_id: String,
    pub golden_run_id: String,
    pub target_stage: String,
    pub status: String,
    pub ci_status: String,
    pub ci_run_id: Option<String>,
    pub ci_checked_at: Option<String>,
    pub requester_id: String,
    pub requester_email: String,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Promotion gate result record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionGate {
    pub request_id: String,
    pub gate_name: String,
    pub status: String,
    pub passed: bool,
    pub details: Option<String>,
    pub error_message: Option<String>,
    pub checked_at: String,
}

/// Promotion approval record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionApproval {
    pub request_id: String,
    pub approver_id: String,
    pub approver_email: String,
    pub action: String,
    pub approval_message: String,
    pub signature: String,
    pub public_key: String,
    pub approved_at: String,
}

/// Release correlation record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseCorrelation {
    pub release_id: String,
    pub golden_run_id: Option<String>,
    pub promotion_request_id: Option<String>,
    pub target_stage: Option<String>,
    pub promotion_status: Option<String>,
    pub approval_signature: Option<String>,
    pub build_id: Option<String>,
    pub build_git_sha: Option<String>,
    pub ci_run_id: Option<String>,
    pub ci_status: Option<String>,
    pub ci_checked_at: Option<String>,
    pub image_digest: Option<String>,
    pub bundle_hash: Option<String>,
    pub trace_id: Option<String>,
    pub automation_workflow_id: Option<String>,
    pub automation_execution_id: Option<String>,
    pub config_deployment_id: Option<String>,
    pub trigger_id: Option<String>,
    pub ci_attestation_signature: Option<String>,
    pub ci_attestation_public_key: Option<String>,
    pub metadata_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Golden run stage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenRunStage {
    pub stage_name: String,
    pub active_golden_run_id: String,
    pub previous_golden_run_id: Option<String>,
    pub promoted_at: String,
    pub promoted_by: String,
}

/// Parameters for creating a promotion request
#[derive(Debug, Clone)]
pub struct CreatePromotionRequestParams {
    pub request_id: String,
    pub release_id: String,
    pub golden_run_id: String,
    pub target_stage: String,
    pub requester_id: String,
    pub requester_email: String,
    pub notes: Option<String>,
    pub ci_run_id: Option<String>,
    pub ci_status: String,
}

/// Parameters for recording a gate result
#[derive(Debug, Clone)]
pub struct RecordGateParams {
    pub request_id: String,
    pub gate_name: String,
    pub status: String,
    pub passed: bool,
    pub details: Option<serde_json::Value>,
    pub error_message: Option<String>,
}

/// Parameters for recording an approval
#[derive(Debug, Clone)]
pub struct RecordApprovalParams {
    pub request_id: String,
    pub approver_id: String,
    pub approver_email: String,
    pub action: String,
    pub approval_message: String,
    pub signature: String,
    pub public_key: String,
}

/// Parameters for creating a release correlation record
#[derive(Debug, Clone)]
pub struct CreateReleaseCorrelationParams {
    pub release_id: String,
    pub golden_run_id: Option<String>,
    pub promotion_request_id: Option<String>,
    pub target_stage: Option<String>,
    pub promotion_status: Option<String>,
    pub build_id: Option<String>,
    pub build_git_sha: Option<String>,
    pub ci_run_id: Option<String>,
    pub ci_status: Option<String>,
    pub image_digest: Option<String>,
    pub bundle_hash: Option<String>,
    pub metadata_json: Option<String>,
}

/// Parameters for updating CI attestation data
#[derive(Debug, Clone)]
pub struct UpdateCiAttestationParams {
    pub release_id: String,
    pub ci_run_id: String,
    pub ci_status: String,
    pub ci_attestation_signature: String,
    pub ci_attestation_public_key: String,
    pub build_git_sha: Option<String>,
    pub image_digest: Option<String>,
    pub build_id: Option<String>,
}

/// Parameters for updating promotion status data
#[derive(Debug, Clone)]
pub struct UpdateReleasePromotionStatusParams {
    pub release_id: String,
    pub promotion_status: String,
    pub approval_signature: Option<String>,
}

// ===== Implementation =====

impl Db {
    // ----- Promotion Requests -----

    /// Create a new promotion request
    pub async fn create_promotion_request(
        &self,
        params: CreatePromotionRequestParams,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO golden_run_promotion_requests
             (request_id, release_id, golden_run_id, target_stage, status, ci_status, ci_run_id, requester_id, requester_email, notes, created_at, updated_at)
             VALUES (?, ?, ?, ?, 'pending', ?, ?, ?, ?, ?, datetime('now'), datetime('now'))",
        )
        .bind(&params.request_id)
        .bind(&params.release_id)
        .bind(&params.golden_run_id)
        .bind(&params.target_stage)
        .bind(&params.ci_status)
        .bind(&params.ci_run_id)
        .bind(&params.requester_id)
        .bind(&params.requester_email)
        .bind(&params.notes)
        .execute(self.pool())
        .await
        .map_err(db_err("create promotion request"))?;

        Ok(())
    }

    /// Get the latest promotion request for a golden run
    pub async fn get_latest_promotion_request(
        &self,
        golden_run_id: &str,
    ) -> Result<Option<PromotionRequest>> {
        let row = sqlx::query(
            "SELECT request_id, release_id, golden_run_id, target_stage, status, ci_status, ci_run_id, ci_checked_at, requester_id, requester_email, notes, created_at, updated_at
             FROM golden_run_promotion_requests
             WHERE golden_run_id = ?
             ORDER BY created_at DESC
             LIMIT 1"
        )
        .bind(golden_run_id)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err("fetch promotion request"))?;

        if let Some(row) = row {
            Ok(Some(PromotionRequest {
                request_id: row.try_get("request_id")?,
                release_id: row.try_get("release_id")?,
                golden_run_id: row.try_get("golden_run_id")?,
                target_stage: row.try_get("target_stage")?,
                status: row.try_get("status")?,
                ci_status: row.try_get("ci_status")?,
                ci_run_id: row.try_get("ci_run_id").ok(),
                ci_checked_at: row.try_get("ci_checked_at").ok(),
                requester_id: row.try_get("requester_id")?,
                requester_email: row.try_get("requester_email")?,
                notes: row.try_get("notes").ok(),
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            }))
        } else {
            Ok(None)
        }
    }

    /// Get promotion request by request_id
    pub async fn get_promotion_request_by_id(
        &self,
        request_id: &str,
    ) -> Result<Option<PromotionRequest>> {
        let row = sqlx::query(
            "SELECT request_id, release_id, golden_run_id, target_stage, status, ci_status, ci_run_id, ci_checked_at, requester_id, requester_email, notes, created_at, updated_at
             FROM golden_run_promotion_requests
             WHERE request_id = ?
             LIMIT 1"
        )
        .bind(request_id)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err("fetch promotion request"))?;

        if let Some(row) = row {
            Ok(Some(PromotionRequest {
                request_id: row.try_get("request_id")?,
                release_id: row.try_get("release_id")?,
                golden_run_id: row.try_get("golden_run_id")?,
                target_stage: row.try_get("target_stage")?,
                status: row.try_get("status")?,
                ci_status: row.try_get("ci_status")?,
                ci_run_id: row.try_get("ci_run_id").ok(),
                ci_checked_at: row.try_get("ci_checked_at").ok(),
                requester_id: row.try_get("requester_id")?,
                requester_email: row.try_get("requester_email")?,
                notes: row.try_get("notes").ok(),
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            }))
        } else {
            Ok(None)
        }
    }

    /// Update promotion request status
    pub async fn update_promotion_request_status(
        &self,
        request_id: &str,
        status: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE golden_run_promotion_requests
             SET status = ?, updated_at = datetime('now')
             WHERE request_id = ?",
        )
        .bind(status)
        .bind(request_id)
        .execute(self.pool())
        .await
        .map_err(db_err("update promotion status"))?;

        Ok(())
    }

    /// Update CI status for a promotion request
    pub async fn update_promotion_request_ci_status(
        &self,
        request_id: &str,
        ci_status: &str,
        ci_run_id: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE golden_run_promotion_requests
             SET ci_status = ?, ci_run_id = ?, ci_checked_at = datetime('now'), updated_at = datetime('now')
             WHERE request_id = ?",
        )
        .bind(ci_status)
        .bind(ci_run_id)
        .bind(request_id)
        .execute(self.pool())
        .await
        .map_err(db_err("update promotion ci status"))?;

        Ok(())
    }

    /// Get target stage from a promotion request
    pub async fn get_promotion_target_stage(&self, request_id: &str) -> Result<String> {
        let row = sqlx::query(
            "SELECT target_stage FROM golden_run_promotion_requests WHERE request_id = ?",
        )
        .bind(request_id)
        .fetch_one(self.pool())
        .await
        .map_err(db_err("fetch promotion target stage"))?;

        let target_stage: String = row.try_get("target_stage")?;
        Ok(target_stage)
    }

    // ----- Promotion Gates -----

    /// Record a gate result (validation check)
    pub async fn record_promotion_gate(&self, params: RecordGateParams) -> Result<()> {
        let details_json = params.details.map(|d| d.to_string());

        sqlx::query(
            "INSERT OR REPLACE INTO golden_run_promotion_gates
             (request_id, gate_name, status, passed, details, error_message, checked_at)
             VALUES (?, ?, ?, ?, ?, ?, datetime('now'))",
        )
        .bind(&params.request_id)
        .bind(&params.gate_name)
        .bind(&params.status)
        .bind(params.passed)
        .bind(details_json)
        .bind(params.error_message)
        .execute(self.pool())
        .await
        .map_err(db_err("record promotion gate"))?;

        Ok(())
    }

    /// Get all gates for a promotion request
    pub async fn get_promotion_gates(&self, request_id: &str) -> Result<Vec<PromotionGate>> {
        let rows = sqlx::query(
            "SELECT request_id, gate_name, status, passed, details, error_message, checked_at
             FROM golden_run_promotion_gates
             WHERE request_id = ?
             ORDER BY checked_at ASC",
        )
        .bind(request_id)
        .fetch_all(self.pool())
        .await
        .map_err(db_err("fetch promotion gates"))?;

        let gates = rows
            .iter()
            .map(|row| {
                Ok(PromotionGate {
                    request_id: row.try_get("request_id")?,
                    gate_name: row.try_get("gate_name")?,
                    status: row.try_get("status")?,
                    passed: row.try_get("passed")?,
                    details: row.try_get("details").ok(),
                    error_message: row.try_get("error_message").ok(),
                    checked_at: row.try_get("checked_at")?,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(gates)
    }

    /// Initialize pending gates for a promotion request
    ///
    /// FIXED: Wraps all gate insertions in a transaction to prevent partial failures
    /// leaving the promotion in an inconsistent state (some gates initialized, others not).
    pub async fn init_promotion_gates(&self, request_id: &str, gate_names: &[&str]) -> Result<()> {
        // Use a transaction to ensure all gates are initialized atomically
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(db_err("begin transaction for init_promotion_gates"))?;

        for gate_name in gate_names {
            sqlx::query(
                "INSERT OR IGNORE INTO golden_run_promotion_gates
                 (request_id, gate_name, status, passed, details, error_message, checked_at)
                 VALUES (?, ?, 'pending', 0, NULL, NULL, datetime('now'))",
            )
            .bind(request_id)
            .bind(gate_name)
            .execute(&mut *tx)
            .await
            .map_err(db_err("init promotion gate"))?;
        }

        tx.commit()
            .await
            .map_err(db_err("commit transaction for init_promotion_gates"))?;

        Ok(())
    }

    // ----- Promotion Approvals -----

    /// Record an approval or rejection
    pub async fn record_promotion_approval(&self, params: RecordApprovalParams) -> Result<()> {
        sqlx::query(
            "INSERT INTO golden_run_promotion_approvals
             (request_id, approver_id, approver_email, action, approval_message, signature, public_key, approved_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'))",
        )
        .bind(&params.request_id)
        .bind(&params.approver_id)
        .bind(&params.approver_email)
        .bind(&params.action)
        .bind(&params.approval_message)
        .bind(&params.signature)
        .bind(&params.public_key)
        .execute(self.pool())
        .await
        .map_err(db_err("record promotion approval"))?;

        Ok(())
    }

    /// Get all approvals for a promotion request
    pub async fn get_promotion_approvals(
        &self,
        request_id: &str,
    ) -> Result<Vec<PromotionApproval>> {
        let rows = sqlx::query(
            "SELECT request_id, approver_id, approver_email, action, approval_message, signature, public_key, approved_at
             FROM golden_run_promotion_approvals
             WHERE request_id = ?
             ORDER BY approved_at DESC",
        )
        .bind(request_id)
        .fetch_all(self.pool())
        .await
        .map_err(db_err("fetch promotion approvals"))?;

        let approvals = rows
            .iter()
            .map(|row| {
                Ok(PromotionApproval {
                    request_id: row.try_get("request_id")?,
                    approver_id: row.try_get("approver_id")?,
                    approver_email: row.try_get("approver_email")?,
                    action: row.try_get("action")?,
                    approval_message: row.try_get("approval_message")?,
                    signature: row.try_get("signature")?,
                    public_key: row.try_get("public_key")?,
                    approved_at: row.try_get("approved_at")?,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(approvals)
    }

    // ----- Release Correlations -----

    /// Create or update a release correlation record
    pub async fn upsert_release_correlation(
        &self,
        params: CreateReleaseCorrelationParams,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO release_correlations
             (release_id, golden_run_id, promotion_request_id, target_stage, promotion_status, build_id, build_git_sha, ci_run_id, ci_status, image_digest, bundle_hash, metadata_json, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))
             ON CONFLICT(release_id) DO UPDATE SET
               golden_run_id = COALESCE(excluded.golden_run_id, release_correlations.golden_run_id),
               promotion_request_id = COALESCE(excluded.promotion_request_id, release_correlations.promotion_request_id),
               target_stage = COALESCE(excluded.target_stage, release_correlations.target_stage),
               promotion_status = COALESCE(excluded.promotion_status, release_correlations.promotion_status),
               build_id = COALESCE(excluded.build_id, release_correlations.build_id),
               build_git_sha = COALESCE(excluded.build_git_sha, release_correlations.build_git_sha),
               ci_run_id = COALESCE(excluded.ci_run_id, release_correlations.ci_run_id),
               ci_status = COALESCE(excluded.ci_status, release_correlations.ci_status),
               image_digest = COALESCE(excluded.image_digest, release_correlations.image_digest),
               bundle_hash = COALESCE(excluded.bundle_hash, release_correlations.bundle_hash),
               metadata_json = COALESCE(excluded.metadata_json, release_correlations.metadata_json),
               updated_at = datetime('now')",
        )
        .bind(&params.release_id)
        .bind(&params.golden_run_id)
        .bind(&params.promotion_request_id)
        .bind(&params.target_stage)
        .bind(&params.promotion_status)
        .bind(&params.build_id)
        .bind(&params.build_git_sha)
        .bind(&params.ci_run_id)
        .bind(&params.ci_status)
        .bind(&params.image_digest)
        .bind(&params.bundle_hash)
        .bind(&params.metadata_json)
        .execute(self.pool())
        .await
        .map_err(db_err("upsert release correlation"))?;

        Ok(())
    }

    /// Update CI attestation fields for a release correlation
    pub async fn update_release_ci_attestation(
        &self,
        params: UpdateCiAttestationParams,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE release_correlations
             SET ci_run_id = ?,
                 ci_status = ?,
                 ci_checked_at = datetime('now'),
                 ci_attestation_signature = ?,
                 ci_attestation_public_key = ?,
                 build_git_sha = COALESCE(?, build_git_sha),
                 image_digest = COALESCE(?, image_digest),
                 build_id = COALESCE(?, build_id),
                 updated_at = datetime('now')
             WHERE release_id = ?",
        )
        .bind(&params.ci_run_id)
        .bind(&params.ci_status)
        .bind(&params.ci_attestation_signature)
        .bind(&params.ci_attestation_public_key)
        .bind(&params.build_git_sha)
        .bind(&params.image_digest)
        .bind(&params.build_id)
        .bind(&params.release_id)
        .execute(self.pool())
        .await
        .map_err(db_err("update release ci attestation"))?;

        Ok(())
    }

    /// Update promotion status for a release correlation
    pub async fn update_release_promotion_status(
        &self,
        params: UpdateReleasePromotionStatusParams,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE release_correlations
             SET promotion_status = ?,
                 approval_signature = COALESCE(?, approval_signature),
                 updated_at = datetime('now')
             WHERE release_id = ?",
        )
        .bind(&params.promotion_status)
        .bind(&params.approval_signature)
        .bind(&params.release_id)
        .execute(self.pool())
        .await
        .map_err(db_err("update release promotion status"))?;

        Ok(())
    }

    // ----- Golden Run Stages -----

    /// Get golden run stage information
    pub async fn get_golden_run_stage(&self, stage_name: &str) -> Result<Option<GoldenRunStage>> {
        let row = sqlx::query(
            "SELECT stage_name, active_golden_run_id, previous_golden_run_id, promoted_at, promoted_by
             FROM golden_run_stages
             WHERE stage_name = ?",
        )
        .bind(stage_name)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err("fetch golden run stage"))?;

        if let Some(row) = row {
            Ok(Some(GoldenRunStage {
                stage_name: row.try_get("stage_name")?,
                active_golden_run_id: row.try_get("active_golden_run_id")?,
                previous_golden_run_id: row.try_get("previous_golden_run_id").ok(),
                promoted_at: row.try_get("promoted_at")?,
                promoted_by: row.try_get("promoted_by")?,
            }))
        } else {
            Ok(None)
        }
    }

    /// Get active golden run ID for a stage
    pub async fn get_stage_active_golden_run(&self, stage_name: &str) -> Result<String> {
        let row =
            sqlx::query("SELECT active_golden_run_id FROM golden_run_stages WHERE stage_name = ?")
                .bind(stage_name)
                .fetch_one(self.pool())
                .await
                .map_err(db_err("fetch stage active golden run"))?;

        let active_id: String = row.try_get("active_golden_run_id")?;
        Ok(active_id)
    }

    /// Update golden run stage (for promotion)
    pub async fn update_golden_run_stage(
        &self,
        stage_name: &str,
        active_golden_run_id: &str,
        previous_golden_run_id: &str,
        promoted_by: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE golden_run_stages
             SET active_golden_run_id = ?,
                 previous_golden_run_id = ?,
                 promoted_at = datetime('now'),
                 promoted_by = ?
             WHERE stage_name = ?",
        )
        .bind(active_golden_run_id)
        .bind(previous_golden_run_id)
        .bind(promoted_by)
        .bind(stage_name)
        .execute(self.pool())
        .await
        .map_err(db_err("update golden run stage"))?;

        Ok(())
    }

    /// Rollback golden run stage (sets active to previous, clears previous)
    pub async fn rollback_golden_run_stage(
        &self,
        stage_name: &str,
        promoted_by: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE golden_run_stages
             SET active_golden_run_id = previous_golden_run_id,
                 previous_golden_run_id = NULL,
                 promoted_at = datetime('now'),
                 promoted_by = ?
             WHERE stage_name = ?",
        )
        .bind(promoted_by)
        .bind(stage_name)
        .execute(self.pool())
        .await
        .map_err(db_err("rollback golden run stage"))?;

        Ok(())
    }

    // ----- Promotion History -----

    /// Record promotion in history
    pub async fn record_promotion_history(
        &self,
        request_id: &str,
        golden_run_id: &str,
        action: &str,
        target_stage: &str,
        previous_golden_run_id: &str,
        promoted_by: &str,
        approval_signature: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO golden_run_promotion_history
             (request_id, golden_run_id, action, target_stage, previous_golden_run_id, promoted_by, approval_signature, promoted_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'))",
        )
        .bind(request_id)
        .bind(golden_run_id)
        .bind(action)
        .bind(target_stage)
        .bind(previous_golden_run_id)
        .bind(promoted_by)
        .bind(approval_signature)
        .execute(self.pool())
        .await
        .map_err(db_err("record promotion history"))?;

        Ok(())
    }

    /// Record rollback in history with metadata
    pub async fn record_rollback_history(
        &self,
        request_id: &str,
        golden_run_id: &str,
        target_stage: &str,
        previous_golden_run_id: &str,
        promoted_by: &str,
        approval_signature: &str,
        metadata: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO golden_run_promotion_history
             (request_id, golden_run_id, action, target_stage, previous_golden_run_id, promoted_by, approval_signature, metadata, promoted_at)
             VALUES (?, ?, 'rolled_back', ?, ?, ?, ?, ?, datetime('now'))",
        )
        .bind(request_id)
        .bind(golden_run_id)
        .bind(target_stage)
        .bind(previous_golden_run_id)
        .bind(promoted_by)
        .bind(approval_signature)
        .bind(metadata)
        .execute(self.pool())
        .await
        .map_err(db_err("record rollback history"))?;

        Ok(())
    }

    /// List recent promotion history entries for a stage
    pub async fn list_promotion_history_for_stage(
        &self,
        target_stage: &str,
        limit: i64,
    ) -> Result<Vec<(String, String, String, String, String)>> {
        let rows = sqlx::query(
            "SELECT golden_run_id, action, previous_golden_run_id, promoted_by, promoted_at
             FROM golden_run_promotion_history
             WHERE target_stage = ?
             ORDER BY promoted_at DESC
             LIMIT ?",
        )
        .bind(target_stage)
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(db_err("fetch promotion history"))?;

        let entries = rows
            .iter()
            .map(|row| {
                Ok((
                    row.try_get("golden_run_id")?,
                    row.try_get("action")?,
                    row.try_get::<Option<String>, _>("previous_golden_run_id")?
                        .unwrap_or_default(),
                    row.try_get("promoted_by")?,
                    row.try_get("promoted_at")?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(entries)
    }
}
