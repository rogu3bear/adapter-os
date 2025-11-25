//! Inference evidence tracking for deterministic provenance.
//!
//! Records the document chunks and context used for each inference operation,
//! enabling full audit trails and reproducibility.

use crate::{Db, Result};
use adapteros_core::AosError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Inference evidence record
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct InferenceEvidence {
    pub id: String,
    pub inference_id: String,
    pub session_id: Option<String>,
    pub message_id: Option<String>,
    pub document_id: String,
    pub chunk_id: String,
    pub page_number: Option<i32>,
    pub document_hash: String,
    pub chunk_hash: String,
    pub relevance_score: f64,
    pub rank: i32,
    pub context_hash: String,
    pub created_at: String,
}

/// Parameters for creating inference evidence
#[derive(Debug, Clone)]
pub struct CreateEvidenceParams {
    pub inference_id: String,
    pub session_id: Option<String>,
    pub message_id: Option<String>,
    pub document_id: String,
    pub chunk_id: String,
    pub page_number: Option<i32>,
    pub document_hash: String,
    pub chunk_hash: String,
    pub relevance_score: f64,
    pub rank: i32,
    pub context_hash: String,
}

impl Db {
    /// Create inference evidence record
    ///
    /// Records a document chunk that contributed to an inference operation.
    /// This creates an immutable audit trail linking inferences to their sources.
    ///
    /// # Arguments
    /// * `params` - Evidence parameters including document/chunk IDs and hashes
    ///
    /// # Returns
    /// The unique ID of the created evidence record
    pub async fn create_inference_evidence(&self, params: CreateEvidenceParams) -> Result<String> {
        let id = Uuid::new_v4().to_string();

        sqlx::query(
            r#"
            INSERT INTO inference_evidence (
                id, inference_id, session_id, message_id, document_id, chunk_id,
                page_number, document_hash, chunk_hash, relevance_score, rank,
                context_hash, created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
            "#,
        )
        .bind(&id)
        .bind(&params.inference_id)
        .bind(&params.session_id)
        .bind(&params.message_id)
        .bind(&params.document_id)
        .bind(&params.chunk_id)
        .bind(&params.page_number)
        .bind(&params.document_hash)
        .bind(&params.chunk_hash)
        .bind(params.relevance_score)
        .bind(params.rank)
        .bind(&params.context_hash)
        .execute(&*self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to create inference evidence: {}", e))
        })?;

        Ok(id)
    }

    /// Get evidence records for an inference operation
    ///
    /// Retrieves all document chunks that contributed to a specific inference,
    /// sorted by rank (most relevant first).
    pub async fn get_evidence_by_inference(
        &self,
        inference_id: &str,
    ) -> Result<Vec<InferenceEvidence>> {
        let records = sqlx::query_as::<_, InferenceEvidenceRow>(
            r#"
            SELECT id, inference_id, session_id, message_id, document_id, chunk_id,
                   page_number, document_hash, chunk_hash, relevance_score, rank,
                   context_hash, created_at
            FROM inference_evidence
            WHERE inference_id = ?
            ORDER BY rank ASC
            "#,
        )
        .bind(inference_id)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch inference evidence: {}", e)))?;

        Ok(records.into_iter().map(Into::into).collect())
    }

    /// Get evidence records for a chat message
    ///
    /// Retrieves all document chunks that contributed to a specific message
    /// in a chat session, sorted by rank.
    pub async fn get_evidence_by_message(
        &self,
        message_id: &str,
    ) -> Result<Vec<InferenceEvidence>> {
        let records = sqlx::query_as::<_, InferenceEvidenceRow>(
            r#"
            SELECT id, inference_id, session_id, message_id, document_id, chunk_id,
                   page_number, document_hash, chunk_hash, relevance_score, rank,
                   context_hash, created_at
            FROM inference_evidence
            WHERE message_id = ?
            ORDER BY rank ASC
            "#,
        )
        .bind(message_id)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch message evidence: {}", e)))?;

        Ok(records.into_iter().map(Into::into).collect())
    }

    /// Get evidence records for a chat session
    ///
    /// Retrieves all document chunks that contributed to any message in a
    /// chat session, grouped by message and sorted by rank.
    pub async fn get_evidence_by_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<InferenceEvidence>> {
        let records = sqlx::query_as::<_, InferenceEvidenceRow>(
            r#"
            SELECT id, inference_id, session_id, message_id, document_id, chunk_id,
                   page_number, document_hash, chunk_hash, relevance_score, rank,
                   context_hash, created_at
            FROM inference_evidence
            WHERE session_id = ?
            ORDER BY created_at DESC, rank ASC
            "#,
        )
        .bind(session_id)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch session evidence: {}", e)))?;

        Ok(records.into_iter().map(Into::into).collect())
    }
}

/// Internal row type for SQLx query mapping
#[derive(sqlx::FromRow)]
struct InferenceEvidenceRow {
    id: String,
    inference_id: String,
    session_id: Option<String>,
    message_id: Option<String>,
    document_id: String,
    chunk_id: String,
    page_number: Option<i32>,
    document_hash: String,
    chunk_hash: String,
    relevance_score: f64,
    rank: i32,
    context_hash: String,
    created_at: String,
}

impl From<InferenceEvidenceRow> for InferenceEvidence {
    fn from(row: InferenceEvidenceRow) -> Self {
        Self {
            id: row.id,
            inference_id: row.inference_id,
            session_id: row.session_id,
            message_id: row.message_id,
            document_id: row.document_id,
            chunk_id: row.chunk_id,
            page_number: row.page_number,
            document_hash: row.document_hash,
            chunk_hash: row.chunk_hash,
            relevance_score: row.relevance_score,
            rank: row.rank,
            context_hash: row.context_hash,
            created_at: row.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_and_retrieve_evidence() {
        let db = Db::new_in_memory().await.unwrap();

        let inference_id = "inf-001";
        let session_id = Some("session-001".to_string());
        let message_id = Some("msg-001".to_string());

        // Create evidence
        let params = CreateEvidenceParams {
            inference_id: inference_id.to_string(),
            session_id: session_id.clone(),
            message_id: message_id.clone(),
            document_id: "doc-001".to_string(),
            chunk_id: "chunk-001".to_string(),
            page_number: Some(1),
            document_hash: "dochash123".to_string(),
            chunk_hash: "chunkhash456".to_string(),
            relevance_score: 0.95,
            rank: 1,
            context_hash: "contexthash789".to_string(),
        };

        let id = db.create_inference_evidence(params).await.unwrap();
        assert!(!id.is_empty());

        // Retrieve by inference
        let evidence = db.get_evidence_by_inference(inference_id).await.unwrap();
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].inference_id, inference_id);
        assert_eq!(evidence[0].relevance_score, 0.95);

        // Retrieve by message
        let evidence = db
            .get_evidence_by_message(message_id.as_ref().unwrap())
            .await
            .unwrap();
        assert_eq!(evidence.len(), 1);

        // Retrieve by session
        let evidence = db
            .get_evidence_by_session(session_id.as_ref().unwrap())
            .await
            .unwrap();
        assert_eq!(evidence.len(), 1);
    }

    #[tokio::test]
    async fn test_multiple_chunks_ranked() {
        let db = Db::new_in_memory().await.unwrap();

        let inference_id = "inf-002";

        // Create multiple evidence records with different ranks
        for (rank, score) in [(1, 0.95), (2, 0.85), (3, 0.75)] {
            let params = CreateEvidenceParams {
                inference_id: inference_id.to_string(),
                session_id: None,
                message_id: None,
                document_id: format!("doc-{}", rank),
                chunk_id: format!("chunk-{}", rank),
                page_number: Some(rank),
                document_hash: format!("dochash-{}", rank),
                chunk_hash: format!("chunkhash-{}", rank),
                relevance_score: score,
                rank,
                context_hash: "contexthash".to_string(),
            };

            db.create_inference_evidence(params).await.unwrap();
        }

        // Retrieve and verify ordering
        let evidence = db.get_evidence_by_inference(inference_id).await.unwrap();
        assert_eq!(evidence.len(), 3);
        assert_eq!(evidence[0].rank, 1);
        assert_eq!(evidence[1].rank, 2);
        assert_eq!(evidence[2].rank, 3);
        assert_eq!(evidence[0].relevance_score, 0.95);
    }
}
