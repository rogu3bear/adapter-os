//! pgvector backend for RAG vector search
//!
//! Replaces in-memory HNSW with PostgreSQL + pgvector for production deployments.
//! Provides deterministic retrieval with tie-breaking (score desc, doc_id asc).
//!
//! **Dual Backend Support:**
//! - **SQLite (Development)**: Uses JSON arrays for embeddings, in-memory cosine similarity
//! - **PostgreSQL (Production)**: Uses pgvector extension with native vector operations
//!
//! **Policy Compliance:**
//! - RAG Index Ruleset (#7): Per-tenant isolation, deterministic ordering
//! - Determinism Ruleset (#2): Score DESC, doc_id ASC tie-breaking
//! - Performance Ruleset (#11): IVFFlat/HNSW indices for sub-24ms p95 latency

use adapteros_core::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPool, sqlite::SqlitePool};
use tokio::task;

/// Document metadata for pgvector storage
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

/// Database backend type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseBackend {
    Sqlite,
    Postgres,
}

/// Builder for creating documents with complex parameter sets
#[derive(Debug)]
pub struct DocumentBuilder {
    tenant_id: String,
    doc_id: String,
    text: String,
    embedding: Vec<f32>,
    rev: String,
    effectivity: String,
    source_type: String,
    superseded_by: Option<String>,
}

/// Parameters for document creation
#[derive(Debug)]
pub struct DocumentParams {
    pub tenant_id: String,
    pub doc_id: String,
    pub text: String,
    pub embedding: Vec<f32>,
    pub rev: String,
    pub effectivity: String,
    pub source_type: String,
    pub superseded_by: Option<String>,
}

impl DocumentBuilder {
    /// Create a new document builder
    pub fn new() -> Self {
        Self {
            tenant_id: String::new(),
            doc_id: String::new(),
            text: String::new(),
            embedding: Vec::new(),
            rev: String::new(),
            effectivity: String::new(),
            source_type: String::new(),
            superseded_by: None,
        }
    }

    /// Set the tenant ID
    pub fn tenant_id(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = tenant_id.into();
        self
    }

    /// Set the document ID
    pub fn doc_id(mut self, doc_id: impl Into<String>) -> Self {
        self.doc_id = doc_id.into();
        self
    }

    /// Set the document text
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();
        self
    }

    /// Set the embedding vector
    pub fn embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = embedding;
        self
    }

    /// Set the revision
    pub fn rev(mut self, rev: impl Into<String>) -> Self {
        self.rev = rev.into();
        self
    }

    /// Set the effectivity period
    pub fn effectivity(mut self, effectivity: impl Into<String>) -> Self {
        self.effectivity = effectivity.into();
        self
    }

    /// Set the source type
    pub fn source_type(mut self, source_type: impl Into<String>) -> Self {
        self.source_type = source_type.into();
        self
    }

    /// Set the superseded revision (optional)
    pub fn superseded_by(mut self, superseded_by: Option<impl Into<String>>) -> Self {
        self.superseded_by = superseded_by.map(|s| s.into());
        self
    }

    /// Build the document parameters
    pub fn build(self) -> DocumentParams {
        DocumentParams {
            tenant_id: self.tenant_id,
            doc_id: self.doc_id,
            text: self.text,
            embedding: self.embedding,
            rev: self.rev,
            effectivity: self.effectivity,
            source_type: self.source_type,
            superseded_by: self.superseded_by,
        }
    }
}

/// pgvector-backed RAG index with dual backend support
pub struct PgVectorIndex {
    backend: DatabaseBackend,
    pg_pool: Option<PgPool>,
    sqlite_pool: Option<SqlitePool>,
    embedding_model_hash: B3Hash,
    embedding_dimension: usize,
}

impl PgVectorIndex {
    /// Create a new pgvector index with PostgreSQL backend
    ///
    /// Requires pgvector extension to be installed:
    /// ```sql
    /// CREATE EXTENSION IF NOT EXISTS vector;
    /// ```
    pub fn new_postgres(pool: PgPool, embedding_model_hash: B3Hash, dimension: usize) -> Self {
        Self {
            backend: DatabaseBackend::Postgres,
            pg_pool: Some(pool),
            sqlite_pool: None,
            embedding_model_hash,
            embedding_dimension: dimension,
        }
    }

    /// Create a new index with SQLite backend (for development)
    pub fn new_sqlite(pool: SqlitePool, embedding_model_hash: B3Hash, dimension: usize) -> Self {
        Self {
            backend: DatabaseBackend::Sqlite,
            pg_pool: None,
            sqlite_pool: Some(pool),
            embedding_model_hash,
            embedding_dimension: dimension,
        }
    }

    /// Add a document to the index
    ///
    /// Stores document text, embedding, and metadata.
    /// - PostgreSQL: Uses pgvector's `vector` type for native similarity search
    /// - SQLite: Stores embedding as JSON array for development
    ///
    /// Use [`DocumentBuilder`] to construct complex parameter sets:
    /// ```rust
    /// use adapteros_lora_rag::pgvector::DocumentBuilder;
    ///
    /// let params = DocumentBuilder::new()
    ///     .tenant_id("tenant-123")
    ///     .doc_id("doc-001")
    ///     .text("Document content")
    ///     .embedding(vec![0.1, 0.2, 0.3])
    ///     .rev("v1.0")
    ///     .effectivity("all")
    ///     .source_type("manual")
    ///     .build();
    /// ```
    ///
    /// The resulting [`DocumentParams`] can be handed to [`PgVectorIndex::add_document`].
    pub async fn add_document(&self, params: DocumentParams) -> Result<()> {
        // Validate embedding dimension
        if params.embedding.len() != self.embedding_dimension {
            return Err(AosError::Rag(format!(
                "Embedding dimension mismatch: expected {}, got {}",
                self.embedding_dimension,
                params.embedding.len()
            )));
        }

        match self.backend {
            DatabaseBackend::Postgres => self.add_document_postgres(&params).await,
            DatabaseBackend::Sqlite => self.add_document_sqlite(&params).await,
        }
    }

    async fn add_document_postgres(&self, params: &DocumentParams) -> Result<()> {
        let pool = self
            .pg_pool
            .as_ref()
            .ok_or_else(|| AosError::Rag("PostgreSQL pool not initialized".to_string()))?;

        // Convert Vec<f32> to pgvector format
        let embedding_str = format!(
            "[{}]",
            params
                .embedding
                .iter()
                .map(|f| f.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );

        sqlx::query(
            "INSERT INTO rag_documents (doc_id, tenant_id, text, embedding, rev, effectivity, source_type, superseded_by, created_at)
             VALUES ($1, $2, $3, $4::vector, $5, $6, $7, $8, NOW())
             ON CONFLICT (doc_id, tenant_id)
             DO UPDATE SET
                text = EXCLUDED.text,
                embedding = EXCLUDED.embedding,
                rev = EXCLUDED.rev,
                effectivity = EXCLUDED.effectivity,
                source_type = EXCLUDED.source_type,
                superseded_by = EXCLUDED.superseded_by,
                updated_at = NOW()"
        )
        .bind(&params.doc_id)
        .bind(&params.tenant_id)
        .bind(&params.text)
        .bind(&embedding_str)
        .bind(&params.rev)
        .bind(&params.effectivity)
        .bind(&params.source_type)
        .bind(&params.superseded_by)
        .execute(pool)
        .await
        .map_err(|e| AosError::Rag(format!("Failed to add document (postgres): {}", e)))?;

        tracing::debug!(
            "Added document {} to tenant {} (postgres)",
            params.doc_id,
            params.tenant_id
        );
        Ok(())
    }

    async fn add_document_sqlite(&self, params: &DocumentParams) -> Result<()> {
        let pool = self
            .sqlite_pool
            .as_ref()
            .ok_or_else(|| AosError::Rag("SQLite pool not initialized".to_string()))?;

        // Convert embedding to JSON array
        let embedding_json = serde_json::to_string(&params.embedding)
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
        .bind(&params.doc_id)
        .bind(&params.tenant_id)
        .bind(&params.text)
        .bind(&embedding_json)
        .bind(&params.rev)
        .bind(&params.effectivity)
        .bind(&params.source_type)
        .bind(&params.superseded_by)
        .execute(pool)
        .await
        .map_err(|e| AosError::Rag(format!("Failed to add document (sqlite): {}", e)))?;

        tracing::debug!(
            "Added document {} to tenant {} (sqlite)",
            params.doc_id,
            params.tenant_id
        );
        Ok(())
    }

    /// Retrieve top-K documents using cosine similarity
    ///
    /// - PostgreSQL: Uses pgvector's `<=>` operator for cosine distance
    /// - SQLite: Uses in-memory cosine similarity calculation
    ///
    /// Implements deterministic tie-breaking: (score desc, doc_id asc).
    ///
    /// # Determinism Guarantee
    /// - Sorting by (1 - cosine_distance) DESC, then doc_id ASC
    /// - Ensures identical results across queries
    pub async fn retrieve(
        &self,
        tenant_id: &str,
        query_embedding: &[f32],
        top_k: usize,
    ) -> Result<Vec<RetrievedDocument>> {
        match self.backend {
            DatabaseBackend::Postgres => {
                self.retrieve_postgres(tenant_id, query_embedding, top_k)
                    .await
            }
            DatabaseBackend::Sqlite => {
                self.retrieve_sqlite(tenant_id, query_embedding, top_k)
                    .await
            }
        }
    }

    async fn retrieve_postgres(
        &self,
        tenant_id: &str,
        query_embedding: &[f32],
        top_k: usize,
    ) -> Result<Vec<RetrievedDocument>> {
        let pool = self
            .pg_pool
            .as_ref()
            .ok_or_else(|| AosError::Rag("PostgreSQL pool not initialized".to_string()))?;

        // Convert query embedding to pgvector format
        let query_str = format!(
            "[{}]",
            query_embedding
                .iter()
                .map(|f| f.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );

        let results = sqlx::query_as::<_, RetrievedDocumentRow>(
            "SELECT 
                doc_id, 
                text, 
                rev, 
                effectivity,
                source_type,
                superseded_by,
                1 - (embedding <=> $1::vector) AS score
             FROM rag_documents
             WHERE tenant_id = $2
             ORDER BY score DESC, doc_id ASC
             LIMIT $3",
        )
        .bind(&query_str)
        .bind(tenant_id)
        .bind(top_k as i64)
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Rag(format!("Failed to retrieve documents (postgres): {}", e)))?;

        let documents = self.rows_to_documents(results);

        tracing::debug!(
            "Retrieved {} documents for tenant {} (top_k={}, backend=postgres)",
            documents.len(),
            tenant_id,
            top_k
        );

        // Fire-and-forget audit logging (non-blocking)
        if let Some(pool) = self.pg_pool.clone() {
            let tenant = tenant_id.to_string();
            let embedding_model_hash = self.embedding_model_hash.to_hex();
            let doc_ids: Vec<String> = documents.iter().map(|d| d.doc_id.clone()).collect();
            let scores: Vec<f32> = documents.iter().map(|d| d.score).collect();
            let query_hash_hex = compute_query_hash(query_embedding).to_hex();
            let top_k_i64 = top_k as i64;

            task::spawn(async move {
                let doc_ids_json = match serde_json::to_string(&doc_ids) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("RAG audit: failed to serialize doc_ids: {}", e);
                        "[]".to_string()
                    }
                };
                let scores_json = match serde_json::to_string(&scores) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("RAG audit: failed to serialize scores: {}", e);
                        "[]".to_string()
                    }
                };
                if let Err(e) = audit_insert_postgres(
                    &pool,
                    &tenant,
                    &query_hash_hex,
                    &doc_ids_json,
                    &scores_json,
                    top_k_i64,
                    &embedding_model_hash,
                )
                .await
                {
                    let preview_ids = if doc_ids.len() > 5 {
                        format!("{}... (+{})", doc_ids[..5].join(","), doc_ids.len() - 5)
                    } else {
                        doc_ids.join(",")
                    };
                    let preview_scores = if scores.len() > 5 {
                        let s: Vec<String> =
                            scores[..5].iter().map(|v| format!("{:.4}", v)).collect();
                        format!("{}... (+{})", s.join(","), scores.len() - 5)
                    } else {
                        scores
                            .iter()
                            .map(|v| format!("{:.4}", v))
                            .collect::<Vec<_>>()
                            .join(",")
                    };
                    tracing::warn!(
                        "RAG audit insert failed (postgres): tenant={} top_k={} ids=[{}] scores=[{}] err={}",
                        tenant,
                        top_k_i64,
                        preview_ids,
                        preview_scores,
                        e
                    );
                }
            });
        }

        Ok(documents)
    }

    async fn retrieve_sqlite(
        &self,
        tenant_id: &str,
        query_embedding: &[f32],
        top_k: usize,
    ) -> Result<Vec<RetrievedDocument>> {
        let pool = self
            .sqlite_pool
            .as_ref()
            .ok_or_else(|| AosError::Rag("SQLite pool not initialized".to_string()))?;

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
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Rag(format!("Failed to retrieve documents (sqlite): {}", e)))?;

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
            "Retrieved {} documents for tenant {} (top_k={}, backend=sqlite)",
            documents.len(),
            tenant_id,
            top_k
        );

        // Fire-and-forget audit logging (non-blocking)
        if let Some(pool) = self.sqlite_pool.clone() {
            let tenant = tenant_id.to_string();
            let embedding_model_hash = self.embedding_model_hash.to_hex();
            let doc_ids: Vec<String> = documents.iter().map(|d| d.doc_id.clone()).collect();
            let scores: Vec<f32> = documents.iter().map(|d| d.score).collect();
            let query_hash_hex = compute_query_hash(query_embedding).to_hex();
            let top_k_i64 = top_k as i64;

            task::spawn(async move {
                let doc_ids_json = match serde_json::to_string(&doc_ids) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("RAG audit: failed to serialize doc_ids: {}", e);
                        "[]".to_string()
                    }
                };
                let scores_json = match serde_json::to_string(&scores) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("RAG audit: failed to serialize scores: {}", e);
                        "[]".to_string()
                    }
                };
                if let Err(e) = audit_insert_sqlite(
                    &pool,
                    &tenant,
                    &query_hash_hex,
                    &doc_ids_json,
                    &scores_json,
                    top_k_i64,
                    &embedding_model_hash,
                )
                .await
                {
                    let preview_ids = if doc_ids.len() > 5 {
                        format!("{}... (+{})", doc_ids[..5].join(","), doc_ids.len() - 5)
                    } else {
                        doc_ids.join(",")
                    };
                    let preview_scores = if scores.len() > 5 {
                        let s: Vec<String> =
                            scores[..5].iter().map(|v| format!("{:.4}", v)).collect();
                        format!("{}... (+{})", s.join(","), scores.len() - 5)
                    } else {
                        scores
                            .iter()
                            .map(|v| format!("{:.4}", v))
                            .collect::<Vec<_>>()
                            .join(",")
                    };
                    tracing::warn!(
                        "RAG audit insert failed (sqlite): tenant={} top_k={} ids=[{}] scores=[{}] err={}",
                        tenant,
                        top_k_i64,
                        preview_ids,
                        preview_scores,
                        e
                    );
                }
            });
        }

        Ok(documents)
    }

    fn rows_to_documents(&self, results: Vec<RetrievedDocumentRow>) -> Vec<RetrievedDocument> {
        results
            .into_iter()
            .map(|row| {
                let span_hash = compute_span_hash(&row.doc_id, &row.text, &row.rev);
                RetrievedDocument {
                    doc_id: row.doc_id,
                    text: row.text,
                    rev: row.rev,
                    effectivity: row.effectivity,
                    source_type: row.source_type,
                    score: row.score,
                    span_hash,
                    superseded: row.superseded_by,
                }
            })
            .collect()
    }

    /// Get document count for a tenant
    pub async fn document_count(&self, tenant_id: &str) -> Result<i64> {
        match self.backend {
            DatabaseBackend::Postgres => {
                let pool = self
                    .pg_pool
                    .as_ref()
                    .ok_or_else(|| AosError::Rag("PostgreSQL pool not initialized".to_string()))?;

                let count: (i64,) =
                    sqlx::query_as("SELECT COUNT(*) FROM rag_documents WHERE tenant_id = $1")
                        .bind(tenant_id)
                        .fetch_one(pool)
                        .await
                        .map_err(|e| AosError::Rag(format!("Failed to count documents: {}", e)))?;

                Ok(count.0)
            }
            DatabaseBackend::Sqlite => {
                let pool = self
                    .sqlite_pool
                    .as_ref()
                    .ok_or_else(|| AosError::Rag("SQLite pool not initialized".to_string()))?;

                let count: (i64,) =
                    sqlx::query_as("SELECT COUNT(*) FROM rag_documents WHERE tenant_id = ?1")
                        .bind(tenant_id)
                        .fetch_one(pool)
                        .await
                        .map_err(|e| AosError::Rag(format!("Failed to count documents: {}", e)))?;

                Ok(count.0)
            }
        }
    }

    /// Delete all documents for a tenant
    pub async fn clear_tenant_documents(&self, tenant_id: &str) -> Result<()> {
        match self.backend {
            DatabaseBackend::Postgres => {
                let pool = self
                    .pg_pool
                    .as_ref()
                    .ok_or_else(|| AosError::Rag("PostgreSQL pool not initialized".to_string()))?;

                sqlx::query("DELETE FROM rag_documents WHERE tenant_id = $1")
                    .bind(tenant_id)
                    .execute(pool)
                    .await
                    .map_err(|e| AosError::Rag(format!("Failed to clear documents: {}", e)))?;
            }
            DatabaseBackend::Sqlite => {
                let pool = self
                    .sqlite_pool
                    .as_ref()
                    .ok_or_else(|| AosError::Rag("SQLite pool not initialized".to_string()))?;

                sqlx::query("DELETE FROM rag_documents WHERE tenant_id = ?1")
                    .bind(tenant_id)
                    .execute(pool)
                    .await
                    .map_err(|e| AosError::Rag(format!("Failed to clear documents: {}", e)))?;
            }
        }

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

/// Internal row type for SQL queries
#[derive(Debug, sqlx::FromRow)]
struct RetrievedDocumentRow {
    doc_id: String,
    text: String,
    rev: String,
    effectivity: String,
    source_type: String,
    superseded_by: Option<String>,
    score: f32,
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

/// Compute cosine similarity between two embeddings
/// Returns a value between -1.0 and 1.0, where 1.0 means identical vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let magnitude_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let magnitude_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if magnitude_a == 0.0 || magnitude_b == 0.0 {
        return 0.0;
    }

    dot_product / (magnitude_a * magnitude_b)
}

/// Compute query hash from f32 embedding bytes (little-endian)
fn compute_query_hash(embedding: &[f32]) -> B3Hash {
    let mut bytes = Vec::with_capacity(embedding.len() * 4);
    for f in embedding {
        bytes.extend_from_slice(&f.to_le_bytes());
    }
    B3Hash::hash(&bytes)
}

async fn audit_insert_sqlite(
    pool: &SqlitePool,
    tenant_id: &str,
    query_hash_hex: &str,
    doc_ids_json: &str,
    scores_json: &str,
    top_k: i64,
    embedding_model_hash: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO rag_retrieval_audit \
         (tenant_id, query_hash, retrieved_doc_ids, retrieved_scores, top_k, embedding_model_hash, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'))",
    )
    .bind(tenant_id)
    .bind(query_hash_hex)
    .bind(doc_ids_json)
    .bind(scores_json)
    .bind(top_k)
    .bind(embedding_model_hash)
    .execute(pool)
    .await
    .map_err(|e| AosError::Rag(format!("Failed to insert rag_retrieval_audit (sqlite): {}", e)))?;
    Ok(())
}

async fn audit_insert_postgres(
    pool: &PgPool,
    tenant_id: &str,
    query_hash_hex: &str,
    doc_ids_json: &str,
    scores_json: &str,
    top_k: i64,
    embedding_model_hash: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO rag_retrieval_audit \
         (tenant_id, query_hash, retrieved_doc_ids, retrieved_scores, top_k, embedding_model_hash, created_at) \
         VALUES ($1, $2, $3::jsonb, $4::jsonb, $5, $6, NOW())",
    )
    .bind(tenant_id)
    .bind(query_hash_hex)
    .bind(doc_ids_json)
    .bind(scores_json)
    .bind(top_k)
    .bind(embedding_model_hash)
    .execute(pool)
    .await
    .map_err(|e| AosError::Rag(format!("Failed to insert rag_retrieval_audit (postgres): {}", e)))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_builder_hydrates_all_fields() {
        let params = DocumentBuilder::new()
            .tenant_id("tenant-123")
            .doc_id("doc-001")
            .text("Doc body")
            .embedding(vec![1.0, 2.0, 3.0])
            .rev("v2")
            .effectivity("all")
            .source_type("manual")
            .superseded_by(Some("doc-000"))
            .build();

        assert_eq!(params.tenant_id, "tenant-123");
        assert_eq!(params.doc_id, "doc-001");
        assert_eq!(params.text, "Doc body");
        assert_eq!(params.embedding, vec![1.0, 2.0, 3.0]);
        assert_eq!(params.rev, "v2");
        assert_eq!(params.effectivity, "all");
        assert_eq!(params.source_type, "manual");
        assert_eq!(params.superseded_by.as_deref(), Some("doc-000"));
    }

    #[test]
    fn document_builder_defaults_optional_fields() {
        let params = DocumentBuilder::new()
            .tenant_id("tenant-123")
            .doc_id("doc-001")
            .text("Doc body")
            .embedding(vec![1.0])
            .rev("v1")
            .effectivity("all")
            .source_type("manual")
            .build();

        assert!(params.superseded_by.is_none());
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL with pgvector extension
    async fn test_pgvector_add_and_retrieve() {
        let pool = PgPool::connect("postgresql://aos:aos@localhost/adapteros_test")
            .await
            .expect("Failed to connect to test database");

        let embedding_hash = B3Hash::hash(b"test-model");
        let index = PgVectorIndex::new_postgres(pool.clone(), embedding_hash, 4);

        // Clear test data
        index
            .clear_tenant_documents("test-tenant")
            .await
            .expect("Failed to clear documents");

        // Add test document
        let embedding = vec![0.1, 0.2, 0.3, 0.4];
        let params = DocumentBuilder::new()
            .tenant_id("test-tenant")
            .doc_id("doc-001")
            .text("Test document text")
            .embedding(embedding.clone())
            .rev("v1")
            .effectivity("all")
            .source_type("manual")
            .build();
        index
            .add_document(params)
            .await
            .expect("Failed to add document");

        // Retrieve documents
        let results = index
            .retrieve("test-tenant", &embedding, 5)
            .await
            .expect("Failed to retrieve documents");

        assert!(!results.is_empty());
        assert_eq!(results[0].doc_id, "doc-001");
        assert!(results[0].score > 0.99); // Should be nearly 1.0 for identical embedding

        pool.close().await;
    }

    #[tokio::test]
    #[ignore]
    async fn test_deterministic_retrieval() {
        let pool = PgPool::connect("postgresql://aos:aos@localhost/adapteros_test")
            .await
            .expect("Failed to connect");

        let embedding_hash = B3Hash::hash(b"test-model");
        let index = PgVectorIndex::new_postgres(pool.clone(), embedding_hash, 4);

        index.clear_tenant_documents("test-tenant").await.ok();

        // Add multiple documents with similar scores
        let embedding = vec![0.5, 0.5, 0.5, 0.5];
        for i in 0..5 {
            let params = DocumentBuilder::new()
                .tenant_id("test-tenant")
                .doc_id(format!("doc-{:03}", i))
                .text(format!("Document {}", i))
                .embedding(embedding.clone())
                .rev("v1")
                .effectivity("all")
                .source_type("test")
                .build();
            index
                .add_document(params)
                .await
                .expect("Failed to add document");
        }

        // Retrieve multiple times - order should be identical
        let results1 = index
            .retrieve("test-tenant", &embedding, 5)
            .await
            .expect("Failed");
        let results2 = index
            .retrieve("test-tenant", &embedding, 5)
            .await
            .expect("Failed");

        assert_eq!(results1.len(), results2.len());
        for (r1, r2) in results1.iter().zip(results2.iter()) {
            assert_eq!(r1.doc_id, r2.doc_id);
            assert_eq!(r1.score, r2.score);
        }

        pool.close().await;
    }

    #[test]
    fn test_cosine_similarity() {
        // Identical vectors
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);

        // Orthogonal vectors
        let c = vec![1.0, 0.0];
        let d = vec![0.0, 1.0];
        assert!((cosine_similarity(&c, &d) - 0.0).abs() < 1e-6);

        // Opposite vectors
        let e = vec![1.0, 0.0];
        let f = vec![-1.0, 0.0];
        assert!((cosine_similarity(&e, &f) + 1.0).abs() < 1e-6);

        // Different lengths
        let g = vec![1.0, 2.0];
        let h = vec![1.0];
        assert_eq!(cosine_similarity(&g, &h), 0.0);
    }

    #[test]
    fn test_query_hash_deterministic() {
        let v = vec![0.1f32, -1.25, 3.5, 0.0, 42.42];
        let h1 = compute_query_hash(&v);
        let h2 = compute_query_hash(&v);
        assert_eq!(h1.to_hex(), h2.to_hex());
    }
}
