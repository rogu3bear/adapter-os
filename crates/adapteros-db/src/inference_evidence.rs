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
    /// JSON array of document IDs in retrieval order (aggregate RAG trace)
    pub rag_doc_ids: Option<String>,
    /// JSON array of relevance scores parallel to rag_doc_ids
    pub rag_scores: Option<String>,
    /// Collection ID used for scoped RAG retrieval
    pub rag_collection_id: Option<String>,
    /// Base model ID used for inference (model context tracking)
    pub base_model_id: Option<String>,
    /// JSON array of adapter IDs used for inference
    pub adapter_ids: Option<String>,
    /// Manifest hash for deterministic provenance
    pub manifest_hash: Option<String>,
}

/// Parameters for creating inference evidence
#[derive(Debug, Clone)]
pub struct CreateEvidenceParams {
    pub tenant_id: String,
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
    /// JSON-serializable list of document IDs in retrieval order (aggregate RAG trace)
    pub rag_doc_ids: Option<Vec<String>>,
    /// JSON-serializable list of relevance scores parallel to rag_doc_ids
    pub rag_scores: Option<Vec<f64>>,
    /// Collection ID used for scoped RAG retrieval
    pub rag_collection_id: Option<String>,
    /// Base model ID at time of inference (snapshot for audit)
    pub base_model_id: Option<String>,
    /// Adapter IDs at time of inference (snapshot for audit)
    pub adapter_ids: Option<Vec<String>>,
    /// Manifest hash at time of inference (determinism binding)
    pub manifest_hash: Option<String>,
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

        // Serialize RAG fields to JSON
        let rag_doc_ids_json = params
            .rag_doc_ids
            .as_ref()
            .map(|ids| serde_json::to_string(ids).unwrap_or_default());
        let rag_scores_json = params
            .rag_scores
            .as_ref()
            .map(|scores| serde_json::to_string(scores).unwrap_or_default());
        // Serialize adapter_ids to JSON
        let adapter_ids_json = params
            .adapter_ids
            .as_ref()
            .map(|ids| serde_json::to_string(ids).unwrap_or_default());

        // Use tenant_id from params (required field)
        let tenant_id = &params.tenant_id;

        sqlx::query(
            r#"
            INSERT INTO inference_evidence (
                id, tenant_id, inference_id, session_id, message_id, document_id, chunk_id,
                page_number, document_hash, chunk_hash, relevance_score, rank,
                context_hash, created_at, rag_doc_ids, rag_scores, rag_collection_id,
                base_model_id, adapter_ids, manifest_hash
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'), ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&tenant_id)
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
        .bind(&rag_doc_ids_json)
        .bind(&rag_scores_json)
        .bind(&params.rag_collection_id)
        .bind(&params.base_model_id)
        .bind(&adapter_ids_json)
        .bind(&params.manifest_hash)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to create inference evidence: {}", e)))?;

        // Emit audit event for evidence creation
        let metadata = serde_json::json!({
            "inference_id": params.inference_id,
            "document_id": params.document_id,
            "chunk_id": params.chunk_id,
            "session_id": params.session_id,
            "message_id": params.message_id,
            "rag_collection_id": params.rag_collection_id,
        });
        if let Err(e) = self
            .log_audit(
                "system",
                "system",
                &params.tenant_id,
                "evidence.created",
                "inference_evidence",
                Some(&id),
                "success",
                None,
                None,
                Some(&metadata.to_string()),
            )
            .await
        {
            tracing::warn!(
                evidence_id = %id,
                error = %e,
                "Failed to log evidence creation audit event"
            );
        }

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
                   context_hash, created_at, rag_doc_ids, rag_scores, rag_collection_id
            FROM inference_evidence
            WHERE inference_id = ?
            ORDER BY rank ASC
            "#,
        )
        .bind(inference_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch inference evidence: {}", e)))?;

        Ok(records.into_iter().map(Into::into).collect())
    }

    /// Get evidence records for a chat message
    ///
    /// Retrieves all document chunks that contributed to a specific message
    /// in a chat session, sorted by rank. Filters by tenant_id for workspace isolation.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID for workspace isolation
    /// * `message_id` - The message ID to retrieve evidence for
    pub async fn get_evidence_by_message(
        &self,
        tenant_id: &str,
        message_id: &str,
    ) -> Result<Vec<InferenceEvidence>> {
        let records = sqlx::query_as::<_, InferenceEvidenceRow>(
            r#"
            SELECT id, inference_id, session_id, message_id, document_id, chunk_id,
                   page_number, document_hash, chunk_hash, relevance_score, rank,
                   context_hash, created_at, rag_doc_ids, rag_scores, rag_collection_id,
                   base_model_id, adapter_ids, manifest_hash
            FROM inference_evidence
            WHERE tenant_id = ? AND message_id = ?
            ORDER BY rank ASC
            "#,
        )
        .bind(tenant_id)
        .bind(message_id)
        .fetch_all(self.pool())
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
                   context_hash, created_at, rag_doc_ids, rag_scores, rag_collection_id
            FROM inference_evidence
            WHERE session_id = ?
            ORDER BY created_at DESC, rank ASC
            "#,
        )
        .bind(session_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch session evidence: {}", e)))?;

        Ok(records.into_iter().map(Into::into).collect())
    }

    /// Batch create inference evidence records
    ///
    /// Efficiently inserts multiple evidence records in a single transaction.
    /// Use this instead of calling `create_inference_evidence` in a loop.
    ///
    /// # Arguments
    /// * `params_list` - Vector of evidence parameters to insert
    ///
    /// # Returns
    /// Vector of created evidence record IDs
    pub async fn create_inference_evidence_batch(
        &self,
        params_list: Vec<CreateEvidenceParams>,
    ) -> Result<Vec<String>> {
        if params_list.is_empty() {
            return Ok(Vec::new());
        }

        let mut tx = self.begin_write_tx().await?;

        let mut ids = Vec::with_capacity(params_list.len());

        for params in params_list {
            let id = Uuid::new_v4().to_string();

            // Serialize RAG fields to JSON
            let rag_doc_ids_json = params
                .rag_doc_ids
                .as_ref()
                .map(|ids| serde_json::to_string(ids).unwrap_or_default());
            let rag_scores_json = params
                .rag_scores
                .as_ref()
                .map(|scores| serde_json::to_string(scores).unwrap_or_default());

            // Use tenant_id from params (required field)
            let tenant_id = &params.tenant_id;

            sqlx::query(
                r#"
                INSERT INTO inference_evidence (
                    id, tenant_id, inference_id, session_id, message_id, document_id, chunk_id,
                    page_number, document_hash, chunk_hash, relevance_score, rank,
                    context_hash, created_at, rag_doc_ids, rag_scores, rag_collection_id
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'), ?, ?, ?)
                "#,
            )
            .bind(&id)
            .bind(&tenant_id)
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
            .bind(&rag_doc_ids_json)
            .bind(&rag_scores_json)
            .bind(&params.rag_collection_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::Database(format!("Failed to insert evidence record: {}", e)))?;

            ids.push((id, params.tenant_id.clone(), params.inference_id.clone()));
        }

        tx.commit()
            .await
            .map_err(|e| AosError::Database(format!("Failed to commit transaction: {}", e)))?;

        // Emit audit events for batch evidence creation (after commit)
        let result_ids: Vec<String> = ids.iter().map(|(id, _, _)| id.clone()).collect();
        for (id, tenant_id, inference_id) in &ids {
            let metadata = serde_json::json!({
                "inference_id": inference_id,
                "batch_size": result_ids.len(),
            });
            if let Err(e) = self
                .log_audit(
                    "system",
                    "system",
                    tenant_id,
                    "evidence.created",
                    "inference_evidence",
                    Some(id),
                    "success",
                    None,
                    None,
                    Some(&metadata.to_string()),
                )
                .await
            {
                tracing::warn!(
                    evidence_id = %id,
                    error = %e,
                    "Failed to log batch evidence creation audit event"
                );
            }
        }

        Ok(result_ids)
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
    rag_doc_ids: Option<String>,
    rag_scores: Option<String>,
    rag_collection_id: Option<String>,
    base_model_id: Option<String>,
    adapter_ids: Option<String>,
    manifest_hash: Option<String>,
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
            rag_doc_ids: row.rag_doc_ids,
            rag_scores: row.rag_scores,
            rag_collection_id: row.rag_collection_id,
            base_model_id: row.base_model_id,
            adapter_ids: row.adapter_ids,
            manifest_hash: row.manifest_hash,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create parent records for FK constraints
    async fn setup_test_data(db: &Db, doc_id: &str, chunk_id: &str) -> String {
        // Create tenant if it doesn't exist yet
        let tenant_id = match db.create_tenant("Test Tenant", false).await {
            Ok(id) => id,
            Err(_) => {
                // Tenant already exists, just use a simple query to get one
                sqlx::query_scalar::<_, String>("SELECT id FROM tenants LIMIT 1")
                    .fetch_one(db.pool())
                    .await
                    .expect("No tenant found")
            }
        };

        // Create document with unique content_hash per doc_id
        sqlx::query(
            "INSERT INTO documents (id, tenant_id, name, content_hash, file_path, file_size, mime_type, status)
             VALUES (?, ?, 'test.pdf', ?, '/tmp/test.pdf', 1024, 'application/pdf', 'processed')"
        )
        .bind(doc_id)
        .bind(&tenant_id)
        .bind(format!("hash-{}", doc_id))
        .execute(db.pool())
        .await
        .expect("Failed to create document");

        // Create chunk
        sqlx::query(
            "INSERT INTO document_chunks (id, tenant_id, document_id, chunk_index, chunk_hash)
             VALUES (?, ?, ?, 0, 'chunkhash')",
        )
        .bind(chunk_id)
        .bind(&tenant_id)
        .bind(doc_id)
        .execute(db.pool())
        .await
        .expect("Failed to create chunk");

        tenant_id
    }

    #[tokio::test]
    async fn test_create_and_retrieve_evidence() {
        let db = Db::new_in_memory().await.unwrap();
        let _tenant_id = setup_test_data(&db, "doc-001", "chunk-001").await;

        let inference_id = "inf-001";
        let message_id = Some("msg-001".to_string());

        // Create evidence (without session_id to avoid chat_sessions FK)
        let params = CreateEvidenceParams {
            tenant_id: _tenant_id.clone(),
            inference_id: inference_id.to_string(),
            session_id: None,
            message_id: message_id.clone(),
            document_id: "doc-001".to_string(),
            chunk_id: "chunk-001".to_string(),
            page_number: Some(1),
            document_hash: "dochash123".to_string(),
            chunk_hash: "chunkhash456".to_string(),
            relevance_score: 0.95,
            rank: 1,
            context_hash: "contexthash789".to_string(),
            rag_doc_ids: None,
            rag_scores: None,
            rag_collection_id: None,
            base_model_id: None,
            adapter_ids: None,
            manifest_hash: None,
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
            .get_evidence_by_message(&_tenant_id, message_id.as_ref().unwrap())
            .await
            .unwrap();
        assert_eq!(evidence.len(), 1);
    }

    #[tokio::test]
    async fn test_multiple_chunks_ranked() {
        let db = Db::new_in_memory().await.unwrap();

        // Create parent records for all 3 document/chunk pairs
        let mut tenant_id = String::new();
        for rank in 1..=3i32 {
            tenant_id =
                setup_test_data(&db, &format!("doc-{}", rank), &format!("chunk-{}", rank)).await;
        }

        let inference_id = "inf-002";

        // Create multiple evidence records with different ranks
        for (rank, score) in [(1, 0.95), (2, 0.85), (3, 0.75)] {
            let params = CreateEvidenceParams {
                tenant_id: tenant_id.clone(),
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
                rag_doc_ids: None,
                rag_scores: None,
                rag_collection_id: None,
                base_model_id: None,
                adapter_ids: None,
                manifest_hash: None,
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

    #[tokio::test]
    async fn test_evidence_with_rag_fields() {
        let db = Db::new_in_memory().await.unwrap();
        let tenant_id = setup_test_data(&db, "doc-rag-001", "chunk-rag-001").await;

        // Provide the referenced RAG collection
        sqlx::query(
            "INSERT INTO document_collections (id, tenant_id, name, description)
             VALUES (?, ?, 'rag collection', 'rag evidence test collection')",
        )
        .bind("col-001")
        .bind(&tenant_id)
        .execute(db.pool())
        .await
        .expect("Failed to insert test collection");

        let inference_id = "inf-rag-001";

        // Create evidence with RAG fields populated
        let params = CreateEvidenceParams {
            tenant_id: tenant_id.clone(),
            inference_id: inference_id.to_string(),
            session_id: None,
            message_id: None,
            document_id: "doc-rag-001".to_string(),
            chunk_id: "chunk-rag-001".to_string(),
            page_number: Some(1),
            document_hash: "dochash-rag".to_string(),
            chunk_hash: "chunkhash-rag".to_string(),
            relevance_score: 0.92,
            rank: 0,
            context_hash: "contexthash-rag".to_string(),
            rag_doc_ids: Some(vec!["doc-rag-001".to_string(), "doc-rag-002".to_string()]),
            rag_scores: Some(vec![0.92, 0.85]),
            rag_collection_id: Some("col-001".to_string()),
            base_model_id: Some("llama-3-8b".to_string()),
            adapter_ids: Some(vec!["adapter-001".to_string()]),
            manifest_hash: Some("manifest-hash-001".to_string()),
        };

        db.create_inference_evidence(params).await.unwrap();

        // Retrieve and verify RAG fields
        let evidence = db.get_evidence_by_inference(inference_id).await.unwrap();
        assert_eq!(evidence.len(), 1);

        // Verify RAG doc IDs are stored and retrievable as JSON
        let rag_doc_ids_json = evidence[0].rag_doc_ids.as_ref().unwrap();
        let rag_doc_ids: Vec<String> = serde_json::from_str(rag_doc_ids_json).unwrap();
        assert_eq!(rag_doc_ids, vec!["doc-rag-001", "doc-rag-002"]);

        // Verify RAG scores
        let rag_scores_json = evidence[0].rag_scores.as_ref().unwrap();
        let rag_scores: Vec<f64> = serde_json::from_str(rag_scores_json).unwrap();
        assert_eq!(rag_scores, vec![0.92, 0.85]);

        // Verify collection ID
        assert_eq!(evidence[0].rag_collection_id, Some("col-001".to_string()));
    }
}
