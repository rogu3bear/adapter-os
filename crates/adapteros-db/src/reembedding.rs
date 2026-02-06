//! Re-embedding manager for handling embedding model migrations.
//!
//! This module provides:
//! - Job creation for model transitions
//! - Batch processing with progress tracking
//! - Resume capability for interrupted jobs
//! - Per-document failure tolerance

use crate::new_id;
use crate::{AosError, Db, Result};
use adapteros_id::IdPrefix;
use sqlx::Row;
use tracing::{info, warn};

/// Status of a re-embedding job
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl JobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobStatus::Pending => "pending",
            JobStatus::Running => "running",
            JobStatus::Completed => "completed",
            JobStatus::Failed => "failed",
            JobStatus::Cancelled => "cancelled",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "running" => JobStatus::Running,
            "completed" => JobStatus::Completed,
            "failed" => JobStatus::Failed,
            "cancelled" => JobStatus::Cancelled,
            _ => JobStatus::Pending,
        }
    }
}

/// Re-embedding job information
#[derive(Debug, Clone)]
pub struct ReembeddingJob {
    pub id: String,
    pub tenant_id: String,
    pub source_model_hash: String,
    pub target_model_hash: String,
    pub status: JobStatus,
    pub total_docs: i64,
    pub processed_docs: i64,
    pub failed_docs: i64,
    pub skipped_docs: i64,
    pub error_message: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub last_processed_doc_id: Option<String>,
}

impl ReembeddingJob {
    pub fn progress_percentage(&self) -> f32 {
        if self.total_docs == 0 {
            0.0
        } else {
            (self.processed_docs + self.failed_docs + self.skipped_docs) as f32
                / self.total_docs as f32
                * 100.0
        }
    }

    pub fn is_complete(&self) -> bool {
        matches!(
            self.status,
            JobStatus::Completed | JobStatus::Failed | JobStatus::Cancelled
        )
    }
}

/// Result of processing a batch of documents
#[derive(Debug)]
pub struct BatchResult {
    pub processed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub last_doc_id: Option<String>,
}

/// Manager for re-embedding operations
pub struct ReembeddingManager<'a> {
    db: &'a Db,
}

impl<'a> ReembeddingManager<'a> {
    pub fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Create a new re-embedding job for model migration.
    pub async fn create_job(
        &self,
        tenant_id: &str,
        source_model_hash: &str,
        target_model_hash: &str,
    ) -> Result<String> {
        // Count documents that need re-embedding
        let doc_count = self
            .count_documents_for_model(tenant_id, source_model_hash)
            .await?;

        if doc_count == 0 {
            return Err(AosError::Validation(
                "No documents found for source model".to_string(),
            ));
        }

        let job_id = new_id(IdPrefix::Job);

        sqlx::query(
            r#"
            INSERT INTO rag_reembedding_jobs (
                id, tenant_id, source_model_hash, target_model_hash,
                status, total_docs, created_at
            ) VALUES (?, ?, ?, ?, 'pending', ?, datetime('now'))
            "#,
        )
        .bind(&job_id)
        .bind(tenant_id)
        .bind(source_model_hash)
        .bind(target_model_hash)
        .bind(doc_count)
        .execute(self.db.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to create re-embedding job: {}", e)))?;

        info!(
            job_id = %job_id,
            tenant_id = %tenant_id,
            source_model = %source_model_hash,
            target_model = %target_model_hash,
            doc_count = doc_count,
            "Created re-embedding job"
        );

        Ok(job_id)
    }

    /// Get job by ID
    pub async fn get_job(&self, job_id: &str) -> Result<Option<ReembeddingJob>> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, source_model_hash, target_model_hash,
                   status, total_docs, processed_docs, failed_docs, skipped_docs,
                   error_message, created_at, started_at, completed_at, last_processed_doc_id
            FROM rag_reembedding_jobs
            WHERE id = ?
            "#,
        )
        .bind(job_id)
        .fetch_optional(self.db.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get job: {}", e)))?;

        Ok(row.map(|r| ReembeddingJob {
            id: r.get("id"),
            tenant_id: r.get("tenant_id"),
            source_model_hash: r.get("source_model_hash"),
            target_model_hash: r.get("target_model_hash"),
            status: JobStatus::from_str(r.get("status")),
            total_docs: r.get("total_docs"),
            processed_docs: r.get("processed_docs"),
            failed_docs: r.get("failed_docs"),
            skipped_docs: r.get("skipped_docs"),
            error_message: r.get("error_message"),
            created_at: r.get("created_at"),
            started_at: r.get("started_at"),
            completed_at: r.get("completed_at"),
            last_processed_doc_id: r.get("last_processed_doc_id"),
        }))
    }

    /// List jobs for a tenant
    pub async fn list_jobs(
        &self,
        tenant_id: &str,
        include_completed: bool,
    ) -> Result<Vec<ReembeddingJob>> {
        let query = if include_completed {
            r#"
            SELECT id, tenant_id, source_model_hash, target_model_hash,
                   status, total_docs, processed_docs, failed_docs, skipped_docs,
                   error_message, created_at, started_at, completed_at, last_processed_doc_id
            FROM rag_reembedding_jobs
            WHERE tenant_id = ?
            ORDER BY created_at DESC
            "#
        } else {
            r#"
            SELECT id, tenant_id, source_model_hash, target_model_hash,
                   status, total_docs, processed_docs, failed_docs, skipped_docs,
                   error_message, created_at, started_at, completed_at, last_processed_doc_id
            FROM rag_reembedding_jobs
            WHERE tenant_id = ? AND status NOT IN ('completed', 'failed', 'cancelled')
            ORDER BY created_at DESC
            "#
        };

        let rows = sqlx::query(query)
            .bind(tenant_id)
            .fetch_all(self.db.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to list jobs: {}", e)))?;

        Ok(rows
            .into_iter()
            .map(|r| ReembeddingJob {
                id: r.get("id"),
                tenant_id: r.get("tenant_id"),
                source_model_hash: r.get("source_model_hash"),
                target_model_hash: r.get("target_model_hash"),
                status: JobStatus::from_str(r.get("status")),
                total_docs: r.get("total_docs"),
                processed_docs: r.get("processed_docs"),
                failed_docs: r.get("failed_docs"),
                skipped_docs: r.get("skipped_docs"),
                error_message: r.get("error_message"),
                created_at: r.get("created_at"),
                started_at: r.get("started_at"),
                completed_at: r.get("completed_at"),
                last_processed_doc_id: r.get("last_processed_doc_id"),
            })
            .collect())
    }

    /// Start a pending job (transition to running)
    pub async fn start_job(&self, job_id: &str) -> Result<bool> {
        let result = sqlx::query(
            r#"
            UPDATE rag_reembedding_jobs
            SET status = 'running', started_at = datetime('now')
            WHERE id = ? AND status = 'pending'
            "#,
        )
        .bind(job_id)
        .execute(self.db.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to start job: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    /// Get next batch of documents to process.
    /// Returns up to `batch_size` documents that haven't been processed yet.
    pub async fn get_next_batch(
        &self,
        job: &ReembeddingJob,
        batch_size: usize,
    ) -> Result<Vec<(String, String)>> {
        if batch_size == 0 {
            return Err(AosError::Validation(
                "batch_size must be greater than 0".into(),
            ));
        }

        // Get documents that haven't been processed yet
        let rows = sqlx::query(
            r#"
            SELECT rd.doc_id, rd.text
            FROM rag_documents rd
            JOIN rag_document_embeddings rde ON rd.doc_id = rde.doc_id AND rd.tenant_id = rde.tenant_id
            LEFT JOIN rag_reembedding_progress rp ON rd.doc_id = rp.doc_id AND rp.job_id = ?1
            WHERE rd.tenant_id = ?2
              AND rde.model_hash = ?3
              AND rp.doc_id IS NULL
            ORDER BY rd.doc_id
            LIMIT ?4
            "#
        )
        .bind(&job.id)
        .bind(&job.tenant_id)
        .bind(&job.source_model_hash)
        .bind(batch_size as i64)
        .fetch_all(self.db.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get next batch: {}", e)))?;

        Ok(rows
            .into_iter()
            .map(|r| (r.get::<String, _>("doc_id"), r.get::<String, _>("text")))
            .collect())
    }

    /// Record progress for a document
    pub async fn record_document_progress(
        &self,
        job_id: &str,
        doc_id: &str,
        status: &str,
        error_message: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO rag_reembedding_progress (job_id, doc_id, status, error_message, processed_at)
            VALUES (?, ?, ?, ?, datetime('now'))
            ON CONFLICT (job_id, doc_id) DO UPDATE SET
                status = excluded.status,
                error_message = excluded.error_message,
                processed_at = excluded.processed_at
            "#
        )
        .bind(job_id)
        .bind(doc_id)
        .bind(status)
        .bind(error_message)
        .execute(self.db.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to record progress: {}", e)))?;

        Ok(())
    }

    /// Update job progress counters
    pub async fn update_job_progress(
        &self,
        job_id: &str,
        batch_result: &BatchResult,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE rag_reembedding_jobs
            SET processed_docs = processed_docs + ?,
                failed_docs = failed_docs + ?,
                skipped_docs = skipped_docs + ?,
                last_processed_doc_id = COALESCE(?, last_processed_doc_id)
            WHERE id = ?
            "#,
        )
        .bind(batch_result.processed as i64)
        .bind(batch_result.failed as i64)
        .bind(batch_result.skipped as i64)
        .bind(&batch_result.last_doc_id)
        .bind(job_id)
        .execute(self.db.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to update progress: {}", e)))?;

        Ok(())
    }

    /// Complete a job (mark as completed or failed).
    /// Only transitions from 'pending' or 'running' states.
    /// Returns Ok(true) if job was completed, Ok(false) if job was not in valid state.
    pub async fn complete_job(
        &self,
        job_id: &str,
        success: bool,
        error_message: Option<&str>,
    ) -> Result<bool> {
        let status = if success { "completed" } else { "failed" };

        let result = sqlx::query(
            r#"
            UPDATE rag_reembedding_jobs
            SET status = ?, error_message = ?, completed_at = datetime('now')
            WHERE id = ? AND status IN ('pending', 'running')
            "#,
        )
        .bind(status)
        .bind(error_message)
        .bind(job_id)
        .execute(self.db.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to complete job: {}", e)))?;

        if result.rows_affected() > 0 {
            info!(job_id = %job_id, status = %status, "Re-embedding job completed");
            Ok(true)
        } else {
            warn!(job_id = %job_id, "Re-embedding job not in valid state for completion");
            Ok(false)
        }
    }

    /// Cancel a running or pending job
    pub async fn cancel_job(&self, job_id: &str) -> Result<bool> {
        let result = sqlx::query(
            r#"
            UPDATE rag_reembedding_jobs
            SET status = 'cancelled', completed_at = datetime('now')
            WHERE id = ? AND status IN ('pending', 'running')
            "#,
        )
        .bind(job_id)
        .execute(self.db.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to cancel job: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    /// Count documents for a specific model
    async fn count_documents_for_model(&self, tenant_id: &str, model_hash: &str) -> Result<i64> {
        let count: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(DISTINCT rd.doc_id)
            FROM rag_documents rd
            JOIN rag_document_embeddings rde ON rd.doc_id = rde.doc_id AND rd.tenant_id = rde.tenant_id
            WHERE rd.tenant_id = ? AND rde.model_hash = ?
            "#
        )
        .bind(tenant_id)
        .bind(model_hash)
        .fetch_one(self.db.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to count documents: {}", e)))?;

        Ok(count.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_status_conversion() {
        assert_eq!(JobStatus::from_str("pending"), JobStatus::Pending);
        assert_eq!(JobStatus::from_str("running"), JobStatus::Running);
        assert_eq!(JobStatus::from_str("completed"), JobStatus::Completed);
        assert_eq!(JobStatus::from_str("failed"), JobStatus::Failed);
        assert_eq!(JobStatus::from_str("unknown"), JobStatus::Pending);
    }

    #[test]
    fn test_job_progress() {
        let job = ReembeddingJob {
            id: "test".to_string(),
            tenant_id: "t1".to_string(),
            source_model_hash: "old".to_string(),
            target_model_hash: "new".to_string(),
            status: JobStatus::Running,
            total_docs: 100,
            processed_docs: 50,
            failed_docs: 5,
            skipped_docs: 0,
            error_message: None,
            created_at: "".to_string(),
            started_at: None,
            completed_at: None,
            last_processed_doc_id: None,
        };

        assert_eq!(job.progress_percentage(), 55.0);
        assert!(!job.is_complete());
    }
}
