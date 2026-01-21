//! SQLite-backed RAG vector search
//!
//! Provides deterministic retrieval with tie-breaking (score desc, doc_id asc).
//!
//! **SQLite Backend:**
//! - Uses JSON arrays for embeddings, in-memory cosine similarity
//!
//! **Policy Compliance:**
//! - RAG Index Ruleset (#7): Per-tenant isolation, deterministic ordering
//! - Determinism Ruleset (#2): Score DESC, doc_id ASC tie-breaking
//! - Performance Ruleset (#11): In-memory similarity calculation

use adapteros_core::{cosine_similarity, AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqlitePool;

/// Document metadata for vector storage
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PgVectorDocument {
    pub doc_id: String,
    pub tenant_id: String,
    pub text: String,
    pub embedding: Vec<f32>,
    pub rev: String,
    pub effectivity: String,
    pub source_type: String,
    pub superseded_by: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// SQLite-backed RAG index
pub struct PgVectorIndex {
    sqlite_pool: SqlitePool,
    embedding_model_hash: B3Hash,
    embedding_dimension: usize,
}

impl PgVectorIndex {
    /// Create a new index with SQLite backend
    pub fn new_sqlite(pool: SqlitePool, embedding_model_hash: B3Hash, dimension: usize) -> Self {
        Self {
            sqlite_pool: pool,
            embedding_model_hash,
            embedding_dimension: dimension,
        }
    }

    /// Add a document to the index
    ///
    /// Stores document text, embedding, and metadata.
    /// SQLite: Stores embedding as JSON array
    #[allow(clippy::too_many_arguments)]
    pub async fn add_document(
        &self,
        tenant_id: &str,
        doc_id: String,
        text: String,
        embedding: Vec<f32>,
        rev: String,
        effectivity: String,
        source_type: String,
        superseded_by: Option<String>,
    ) -> Result<()> {
        // Validate embedding dimension
        if embedding.len() != self.embedding_dimension {
            return Err(AosError::Rag(format!(
                "Embedding dimension mismatch: expected {}, got {}",
                self.embedding_dimension,
                embedding.len()
            )));
        }

        // Convert embedding to JSON array
        let embedding_json = serde_json::to_string(&embedding)
            .map_err(|e| AosError::Rag(format!("Failed to serialize embedding: {}", e)))?;

        sqlx::query(
            "INSERT INTO rag_documents (doc_id, tenant_id, text, embedding_json, rev, effectivity, source_type, superseded_by, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
             ON CONFLICT (doc_id, tenant_id) 
             DO UPDATE SET 
                text = excluded.text,
                embedding_json = excluded.embedding_json,
                rev = excluded.rev,
                effectivity = excluded.effectivity,
                source_type = excluded.source_type,
                superseded_by = excluded.superseded_by,
                updated_at = CURRENT_TIMESTAMP"
        )
        .bind(&doc_id)
        .bind(tenant_id)
        .bind(&text)
        .bind(&embedding_json)
        .bind(&rev)
        .bind(&effectivity)
        .bind(&source_type)
        .bind(&superseded_by)
        .execute(&self.sqlite_pool)
        .await
        .map_err(|e| AosError::Rag(format!("Failed to add document: {}", e)))?;

        tracing::debug!("Added document {} to tenant {}", doc_id, tenant_id);
        Ok(())
    }

    /// Retrieve top-K documents using cosine similarity
    ///
    /// Uses in-memory cosine similarity calculation with deterministic tie-breaking.
    ///
    /// Implements deterministic tie-breaking: (score desc, doc_id asc).
    ///
    /// # Determinism Guarantee
    /// - Sorting by cosine similarity DESC, then doc_id ASC
    /// - Ensures identical results across queries
    pub async fn retrieve(
        &self,
        tenant_id: &str,
        query_embedding: &[f32],
        top_k: usize,
    ) -> Result<Vec<RetrievedDocument>> {
        // Fetch all documents for tenant (SQLite doesn't have native vector ops)
        #[derive(sqlx::FromRow)]
        struct SqliteDocRow {
            doc_id: String,
            text: String,
            rev: String,
            effectivity: String,
            source_type: String,
            superseded_by: Option<String>,
            embedding_json: String,
        }

        let rows: Vec<SqliteDocRow> = sqlx::query_as(
            "SELECT doc_id, text, rev, effectivity, source_type, superseded_by, embedding_json
             FROM rag_documents
             WHERE tenant_id = ?1",
        )
        .bind(tenant_id)
        .fetch_all(&self.sqlite_pool)
        .await
        .map_err(|e| AosError::Rag(format!("Failed to retrieve documents: {}", e)))?;

        // Calculate cosine similarity in-memory and sort
        let mut scored_docs: Vec<(SqliteDocRow, f32)> = rows
            .into_iter()
            .filter_map(|row| {
                let embedding: Vec<f32> = serde_json::from_str(&row.embedding_json).ok()?;
                let score = cosine_similarity(query_embedding, &embedding);
                Some((row, score))
            })
            .collect();

        // Deterministic sorting: score DESC, doc_id ASC
        scored_docs.sort_by(|(row_a, score_a), (row_b, score_b)| {
            score_b
                .partial_cmp(score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| row_a.doc_id.cmp(&row_b.doc_id))
        });

        // Take top-K and convert to RetrievedDocument
        let documents: Vec<RetrievedDocument> = scored_docs
            .into_iter()
            .take(top_k)
            .map(|(row, score)| {
                let span_hash = compute_span_hash(&row.doc_id, &row.text, &row.rev);
                RetrievedDocument {
                    doc_id: row.doc_id,
                    text: row.text,
                    rev: row.rev,
                    effectivity: row.effectivity,
                    source_type: row.source_type,
                    score,
                    span_hash,
                    superseded: row.superseded_by,
                }
            })
            .collect();

        tracing::debug!(
            "Retrieved {} documents for tenant {} (top_k={})",
            documents.len(),
            tenant_id,
            top_k
        );

        Ok(documents)
    }

    /// Get document count for a tenant
    pub async fn document_count(&self, tenant_id: &str) -> Result<i64> {
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM rag_documents WHERE tenant_id = ?1")
                .bind(tenant_id)
                .fetch_one(&self.sqlite_pool)
                .await
                .map_err(|e| AosError::Rag(format!("Failed to count documents: {}", e)))?;

        Ok(count.0)
    }

    /// Delete all documents for a tenant
    pub async fn clear_tenant_documents(&self, tenant_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM rag_documents WHERE tenant_id = ?1")
            .bind(tenant_id)
            .execute(&self.sqlite_pool)
            .await
            .map_err(|e| AosError::Rag(format!("Failed to clear documents: {}", e)))?;

        tracing::info!("Cleared all documents for tenant {}", tenant_id);
        Ok(())
    }

    /// Validate embedding model hash
    pub fn validate_embedding_hash(&self, hash: &B3Hash) -> Result<()> {
        if *hash != self.embedding_model_hash {
            return Err(AosError::Rag(format!(
                "Embedding model hash mismatch: expected {}, got {}",
                self.embedding_model_hash, hash
            )));
        }
        Ok(())
    }
}

/// Retrieved document with provenance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievedDocument {
    pub doc_id: String,
    pub text: String,
    pub rev: String,
    pub effectivity: String,
    pub source_type: String,
    pub score: f32,
    pub span_hash: B3Hash,
    pub superseded: Option<String>,
}

impl RetrievedDocument {
    /// Check if this document is from a superseded revision
    pub fn is_superseded(&self) -> bool {
        self.superseded.is_some()
    }

    /// Generate warning if superseded
    pub fn supersession_warning(&self) -> Option<String> {
        self.superseded.as_ref().map(|new_rev| {
            format!(
                "Document {} revision {} has been superseded by {}",
                self.doc_id, self.rev, new_rev
            )
        })
    }
}

/// Compute span hash for evidence tracking
fn compute_span_hash(doc_id: &str, text: &str, rev: &str) -> B3Hash {
    let combined = format!("{}||{}||{}", doc_id, rev, text);
    B3Hash::hash(combined.as_bytes())
}

// cosine_similarity is imported from adapteros_core::vector_math

#[cfg(test)]
mod tests {
    use super::*;

    // cosine_similarity tests moved to adapteros_core::vector_math
}
