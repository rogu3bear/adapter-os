//! RAG storage abstraction over SQL + KV with deterministic retrieval.

use crate::{Db, StorageMode};
use adapteros_core::{cosine_similarity, AosError, B3Hash, Result};
use adapteros_storage::{RagDocumentKv, RagRepository};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, warn};

type RagDocumentRow = (
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    String,
);

/// Input payload for persisting a RAG embedding document.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RagDocumentWrite {
    pub tenant_id: String,
    pub doc_id: String,
    pub text: String,
    pub embedding: Vec<f32>,
    pub rev: String,
    pub effectivity: String,
    pub source_type: String,
    pub superseded_by: Option<String>,
    pub embedding_model_hash: B3Hash,
    pub embedding_dimension: usize,
}

/// Retrieved document with score used by RAG retrieval.
#[derive(Clone, Debug)]
pub struct RagRetrievedDocument {
    pub doc_id: String,
    pub text: String,
    pub score: f32,
}

/// Merge strategy for multi-model retrieval
#[derive(Debug, Clone, Default)]
pub enum ModelMergeStrategy {
    /// Only use primary model
    #[default]
    PrimaryOnly,
    /// Use fallback if primary returns nothing
    FallbackIfEmpty,
    /// Merge results from both models, deduplicated
    UnionDeduplicated,
}

impl Db {
    fn rag_kv_repo(&self) -> Option<RagRepository> {
        self.kv_backend().map(|kv| {
            let backend: Arc<dyn adapteros_storage::KvBackend> = kv.backend().clone();
            let index_manager = kv.index_manager().clone();
            RagRepository::new(backend, index_manager)
        })
    }

    /// Upsert a RAG document according to storage mode (SQL, KV, or dual-write).
    pub async fn upsert_rag_document(&self, doc: RagDocumentWrite) -> Result<()> {
        if doc.embedding.len() != doc.embedding_dimension {
            return Err(AosError::Rag(format!(
                "Embedding dimension mismatch: expected {}, got {}",
                doc.embedding_dimension,
                doc.embedding.len()
            )));
        }

        // SQL write path
        if self.storage_mode().write_to_sql() {
            let embedding_json = serde_json::to_string(&doc.embedding)
                .map_err(|e| AosError::Rag(format!("Failed to serialize embedding: {}", e)))?;

            // Ensure model registry row exists
            sqlx::query(
                r#"
                INSERT INTO rag_embedding_models (model_hash, model_name, dimension, is_active, created_at)
                VALUES (?1, ?2, ?3, 1, CURRENT_TIMESTAMP)
                ON CONFLICT(model_hash) DO NOTHING
            "#,
            )
            .bind(doc.embedding_model_hash.to_hex())
            .bind(doc.embedding_model_hash.to_hex())
            .bind(doc.embedding_dimension as i64)
            .execute(self.pool_result()?)
            .await
            .map_err(|e| AosError::Database(format!("Failed to upsert embedding model: {}", e)))?;

            // Upsert rag_documents row
            sqlx::query(
                r#"
                INSERT INTO rag_documents (
                    doc_id, tenant_id, text, embedding_json, rev, effectivity, source_type, superseded_by,
                    created_at, updated_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
                ON CONFLICT (doc_id, tenant_id) DO UPDATE SET
                    text = excluded.text,
                    embedding_json = excluded.embedding_json,
                    rev = excluded.rev,
                    effectivity = excluded.effectivity,
                    source_type = excluded.source_type,
                    superseded_by = excluded.superseded_by,
                    updated_at = CURRENT_TIMESTAMP
            "#,
            )
            .bind(&doc.doc_id)
            .bind(&doc.tenant_id)
            .bind(&doc.text)
            .bind(&embedding_json)
            .bind(&doc.rev)
            .bind(&doc.effectivity)
            .bind(&doc.source_type)
            .bind(&doc.superseded_by)
            .execute(self.pool_result()?)
            .await
            .map_err(|e| AosError::Database(format!("Failed to upsert rag document: {}", e)))?;

            // Track doc->model mapping
            sqlx::query(
                r#"
                INSERT INTO rag_document_embeddings (doc_id, tenant_id, model_hash, created_at)
                VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP)
                ON CONFLICT(doc_id, tenant_id, model_hash) DO NOTHING
            "#,
            )
            .bind(&doc.doc_id)
            .bind(&doc.tenant_id)
            .bind(doc.embedding_model_hash.to_hex())
            .execute(self.pool_result()?)
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to upsert rag doc embedding: {}", e))
            })?;
        }

        // KV write path
        if self.storage_mode().write_to_kv() {
            let repo = self
                .rag_kv_repo()
                .ok_or_else(|| AosError::Database("KV backend not attached".to_string()))?;

            let kv_doc = RagDocumentKv::new_with_now(
                doc.doc_id.clone(),
                doc.tenant_id.clone(),
                doc.text.clone(),
                doc.embedding.clone(),
                doc.rev.clone(),
                doc.effectivity.clone(),
                doc.superseded_by.clone(),
                doc.source_type.clone(),
                doc.embedding_model_hash.to_hex(),
                doc.embedding_dimension as u32,
            );

            repo.upsert(kv_doc)
                .await
                .map_err(|e| AosError::Database(format!("KV RAG upsert failed: {}", e)))?;
        }

        Ok(())
    }

    /// Retrieve RAG documents using the configured storage mode with deterministic ordering.
    pub async fn retrieve_rag_documents(
        &self,
        tenant_id: &str,
        embedding_model_hash: &B3Hash,
        expected_dimension: usize,
        query_embedding: &[f32],
        top_k: usize,
    ) -> Result<Vec<RagRetrievedDocument>> {
        let mode = self.storage_mode();

        if mode.read_from_kv() {
            match self
                .retrieve_from_kv(
                    tenant_id,
                    embedding_model_hash,
                    expected_dimension,
                    query_embedding,
                    top_k,
                )
                .await
            {
                Ok(kv_results) => {
                    // If KV path returns empty but SQL fallback is allowed, use SQL for parity
                    if kv_results.is_empty() && mode.sql_fallback_enabled() {
                        let sql_results = self
                            .retrieve_from_sql(
                                tenant_id,
                                embedding_model_hash,
                                expected_dimension,
                                query_embedding,
                                top_k,
                            )
                            .await?;
                        return Ok(sql_results);
                    }

                    // Drift detection while KV is primary to avoid silent split-brain.
                    if mode.is_dual_write() && mode.write_to_sql() {
                        if let Ok(sql_results) = self
                            .retrieve_from_sql(
                                tenant_id,
                                embedding_model_hash,
                                expected_dimension,
                                query_embedding,
                                top_k,
                            )
                            .await
                        {
                            if !drift_matches(&sql_results, &kv_results) {
                                warn!(
                                    tenant_id = %tenant_id,
                                    "RAG retrieval drift detected between KV and SQL (kv-primary read)"
                                );
                            }
                        }
                    }

                    return Ok(kv_results);
                }
                Err(e) if mode.sql_fallback_enabled() => {
                    self.record_kv_read_fallback("rag.retrieve.fallback");
                    warn!(
                        tenant_id = %tenant_id,
                        error = %e,
                        "KV RAG retrieval failed, falling back to SQL"
                    );
                }
                Err(e) => return Err(e),
            }
        }

        // SQL primary
        let sql_results = self
            .retrieve_from_sql(
                tenant_id,
                embedding_model_hash,
                expected_dimension,
                query_embedding,
                top_k,
            )
            .await?;

        // Diff check in dual-write modes
        if mode.is_dual_write() && self.storage_mode().write_to_kv() {
            if let Some(kv_repo) = self.rag_kv_repo() {
                if let Ok(kv_results) = self
                    .retrieve_from_kv_repo(
                        &kv_repo,
                        tenant_id,
                        embedding_model_hash,
                        expected_dimension,
                        query_embedding,
                        top_k,
                    )
                    .await
                {
                    if !drift_matches(&sql_results, &kv_results) {
                        warn!(
                            tenant_id = %tenant_id,
                            "RAG retrieval drift detected between SQL and KV (dual-write mode)"
                        );
                    }
                }
            }
        }

        Ok(sql_results)
    }

    /// Backfill existing SQL rag_documents into KV for a tenant filter.
    pub async fn backfill_rag_to_kv(
        &self,
        tenant_filter: Option<&str>,
        embedding_model_hash: &B3Hash,
    ) -> Result<usize> {
        let Some(repo) = self.rag_kv_repo() else {
            return Err(AosError::Database(
                "KV backend is required for backfill".to_string(),
            ));
        };

        let mut rows = if let Some(tenant) = tenant_filter {
            sqlx::query_as::<
                _,
                (
                    String,
                    String,
                    String,
                    String,
                    String,
                    String,
                    String,
                    Option<String>,
                ),
            >(
                r#"
                SELECT d.doc_id, d.tenant_id, d.text, d.embedding_json, d.rev, d.effectivity,
                       d.source_type, d.superseded_by
                FROM rag_documents d
                JOIN rag_document_embeddings rde
                    ON rde.doc_id = d.doc_id AND rde.tenant_id = d.tenant_id
                WHERE rde.model_hash = ?1 AND d.tenant_id = ?2
            "#,
            )
            .bind(embedding_model_hash.to_hex())
            .bind(tenant)
            .fetch_all(self.pool_result()?)
            .await
        } else {
            sqlx::query_as::<
                _,
                (
                    String,
                    String,
                    String,
                    String,
                    String,
                    String,
                    String,
                    Option<String>,
                ),
            >(
                r#"
                SELECT d.doc_id, d.tenant_id, d.text, d.embedding_json, d.rev, d.effectivity,
                       d.source_type, d.superseded_by
                FROM rag_documents d
                JOIN rag_document_embeddings rde
                    ON rde.doc_id = d.doc_id AND rde.tenant_id = d.tenant_id
                WHERE rde.model_hash = ?1
            "#,
            )
            .bind(embedding_model_hash.to_hex())
            .fetch_all(self.pool_result()?)
            .await
        }
        .map_err(|e| {
            AosError::Database(format!("Failed to load rag documents for backfill: {}", e))
        })?;

        let mut count = 0usize;
        for (
            doc_id,
            tenant_id,
            text,
            embedding_json,
            rev,
            effectivity,
            source_type,
            superseded_by,
        ) in rows.drain(..)
        {
            let embedding: Vec<f32> = serde_json::from_str(&embedding_json).map_err(|e| {
                AosError::Rag(format!("Failed to decode embedding during backfill: {}", e))
            })?;

            let kv_doc = RagDocumentKv::new_with_now(
                doc_id.clone(),
                tenant_id.clone(),
                text.clone(),
                embedding.clone(),
                rev.clone(),
                effectivity.clone(),
                superseded_by.clone(),
                source_type.clone(),
                embedding_model_hash.to_hex(),
                embedding.len() as u32,
            );

            repo.upsert(kv_doc)
                .await
                .map_err(|e| AosError::Database(format!("KV RAG backfill failed: {}", e)))?;
            count += 1;
        }

        Ok(count)
    }

    async fn retrieve_from_sql(
        &self,
        tenant_id: &str,
        embedding_model_hash: &B3Hash,
        expected_dimension: usize,
        query_embedding: &[f32],
        top_k: usize,
    ) -> Result<Vec<RagRetrievedDocument>> {
        let rows: Vec<RagDocumentRow> = sqlx::query_as(
                r#"
                SELECT d.doc_id, d.text, d.rev, d.effectivity, d.source_type, d.superseded_by, d.embedding_json
                FROM rag_documents d
                JOIN rag_document_embeddings rde
                    ON rde.doc_id = d.doc_id AND rde.tenant_id = d.tenant_id
                WHERE d.tenant_id = ?1 AND rde.model_hash = ?2
            "#,
            )
            .bind(tenant_id)
            .bind(embedding_model_hash.to_hex())
            .fetch_all(self.pool_result()?)
            .await
            .map_err(|e| AosError::Database(format!("Failed to retrieve RAG documents: {}", e)))?;

        let mut scored_docs = Vec::with_capacity(rows.len());
        for (doc_id, text, _rev, _effectivity, _source_type, _superseded_by, embedding_json) in rows
        {
            let embedding: Vec<f32> =
                serde_json::from_str(&embedding_json).map_err(|e| AosError::Rag(e.to_string()))?;

            if embedding.len() != expected_dimension {
                continue;
            }

            let score = cosine_similarity(query_embedding, &embedding);
            scored_docs.push((doc_id, text, score));
        }

        deterministic_sort_and_take(scored_docs, top_k, 0.0)
    }

    async fn retrieve_from_kv(
        &self,
        tenant_id: &str,
        embedding_model_hash: &B3Hash,
        expected_dimension: usize,
        query_embedding: &[f32],
        top_k: usize,
    ) -> Result<Vec<RagRetrievedDocument>> {
        let repo = self
            .rag_kv_repo()
            .ok_or_else(|| AosError::Database("KV backend not attached".to_string()))?;

        self.retrieve_from_kv_repo(
            &repo,
            tenant_id,
            embedding_model_hash,
            expected_dimension,
            query_embedding,
            top_k,
        )
        .await
    }

    async fn retrieve_from_kv_repo(
        &self,
        repo: &RagRepository,
        tenant_id: &str,
        embedding_model_hash: &B3Hash,
        expected_dimension: usize,
        query_embedding: &[f32],
        top_k: usize,
    ) -> Result<Vec<RagRetrievedDocument>> {
        let docs = repo
            .list_by_tenant_and_model(tenant_id, &embedding_model_hash.to_hex())
            .await
            .map_err(|e| AosError::Database(format!("KV RAG read failed: {}", e)))?;

        let mut scored_docs = Vec::with_capacity(docs.len());
        for doc in docs {
            if doc.embedding_dimension as usize != expected_dimension {
                continue;
            }
            let score = cosine_similarity(query_embedding, &doc.embedding);
            scored_docs.push((doc.doc_id, doc.text, score));
        }

        deterministic_sort_and_take(scored_docs, top_k, 0.0)
    }

    /// Retrieve RAG documents using hybrid search (vector + FTS5).
    /// Combines results using Reciprocal Rank Fusion (RRF).
    pub async fn retrieve_rag_documents_hybrid(
        &self,
        tenant_id: &str,
        query_text: &str,
        embedding_model_hash: &B3Hash,
        expected_dimension: usize,
        query_embedding: &[f32],
        top_k: usize,
        min_score: f32,
    ) -> Result<Vec<RagRetrievedDocument>> {
        const RRF_K: f32 = 60.0;

        // 1. Vector search
        let vector_results = self
            .retrieve_rag_documents(
                tenant_id,
                embedding_model_hash,
                expected_dimension,
                query_embedding,
                top_k * 2, // Over-fetch for fusion
            )
            .await?;

        // 2. FTS5 search
        let fts_results = self
            .retrieve_rag_documents_fts(tenant_id, query_text, top_k * 2)
            .await?;

        // 3. RRF fusion
        use std::collections::HashMap;
        let mut rrf_scores: HashMap<String, (f32, Option<RagRetrievedDocument>)> = HashMap::new();

        // Add vector results with rank-based RRF score
        for (rank, doc) in vector_results.into_iter().enumerate() {
            let rrf_score = 1.0 / (RRF_K + rank as f32 + 1.0);
            rrf_scores.insert(doc.doc_id.clone(), (rrf_score, Some(doc)));
        }

        // Add FTS results, combining scores if doc already exists
        for (rank, doc) in fts_results.into_iter().enumerate() {
            let rrf_score = 1.0 / (RRF_K + rank as f32 + 1.0);
            rrf_scores
                .entry(doc.doc_id.clone())
                .and_modify(|(score, _)| *score += rrf_score)
                .or_insert((rrf_score, Some(doc)));
        }

        // 4. Sort by combined RRF score with deterministic tie-breaking
        let mut combined: Vec<_> = rrf_scores
            .into_iter()
            .filter_map(|(_, (score, doc))| {
                doc.map(|mut d| {
                    d.score = score;
                    d
                })
            })
            .collect();

        combined.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.doc_id.cmp(&b.doc_id))
        });

        // 5. Apply minimum score threshold and take top_k
        Ok(combined
            .into_iter()
            .filter(|d| d.score >= min_score)
            .take(top_k)
            .collect())
    }

    /// FTS5 text search for RAG documents.
    async fn retrieve_rag_documents_fts(
        &self,
        tenant_id: &str,
        query_text: &str,
        limit: usize,
    ) -> Result<Vec<RagRetrievedDocument>> {
        // Escape FTS5 special characters
        let escaped_query = Self::escape_fts5_query(query_text);

        if escaped_query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let rows: Vec<(String, String, f64)> = sqlx::query_as(
            r#"
            SELECT d.doc_id, d.text, bm25(rag_documents_fts) as score
            FROM rag_documents_fts fts
            JOIN rag_documents d ON d.rowid = fts.rowid
            WHERE rag_documents_fts MATCH ?1
              AND d.tenant_id = ?2
            ORDER BY bm25(rag_documents_fts)
            LIMIT ?3
            "#,
        )
        .bind(&escaped_query)
        .bind(tenant_id)
        .bind(limit as i64)
        .fetch_all(self.pool_result()?)
        .await
        .unwrap_or_else(|e| {
            // FTS5 table might not exist yet, or query syntax error
            tracing::warn!(
                error = %e,
                query = %escaped_query,
                "FTS5 search failed, returning empty results"
            );
            Vec::new()
        });

        Ok(rows
            .into_iter()
            .map(|(doc_id, text, score)| RagRetrievedDocument {
                doc_id,
                text,
                score: (-score) as f32, // BM25 returns negative scores, lower is better
            })
            .collect())
    }

    fn escape_fts5_query(query: &str) -> String {
        query
            .chars()
            .map(|c| match c {
                '"' | '*' | '(' | ')' | ':' | '^' | '-' => ' ',
                _ => c,
            })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Retrieve with model fallback support for transitions.
    pub async fn retrieve_rag_documents_with_fallback(
        &self,
        tenant_id: &str,
        primary_model_hash: &B3Hash,
        fallback_model_hash: Option<&B3Hash>,
        expected_dimension: usize,
        query_embedding: &[f32],
        top_k: usize,
        strategy: ModelMergeStrategy,
    ) -> Result<Vec<RagRetrievedDocument>> {
        // Primary retrieval
        let primary_results = self
            .retrieve_rag_documents(
                tenant_id,
                primary_model_hash,
                expected_dimension,
                query_embedding,
                top_k,
            )
            .await?;

        match strategy {
            ModelMergeStrategy::PrimaryOnly => Ok(primary_results),
            ModelMergeStrategy::FallbackIfEmpty if primary_results.is_empty() => {
                if let Some(fallback_hash) = fallback_model_hash {
                    self.retrieve_rag_documents(
                        tenant_id,
                        fallback_hash,
                        expected_dimension,
                        query_embedding,
                        top_k,
                    )
                    .await
                } else {
                    Ok(primary_results)
                }
            }
            ModelMergeStrategy::UnionDeduplicated => {
                let mut results = primary_results;
                if let Some(fallback_hash) = fallback_model_hash {
                    let fallback_results = self
                        .retrieve_rag_documents(
                            tenant_id,
                            fallback_hash,
                            expected_dimension,
                            query_embedding,
                            top_k,
                        )
                        .await?;

                    // Merge and deduplicate
                    let existing_ids: std::collections::HashSet<_> =
                        results.iter().map(|d| d.doc_id.clone()).collect();
                    for doc in fallback_results {
                        if !existing_ids.contains(&doc.doc_id) {
                            results.push(doc);
                        }
                    }

                    // Re-sort deterministically
                    results.sort_by(|a, b| {
                        b.score
                            .partial_cmp(&a.score)
                            .unwrap_or(std::cmp::Ordering::Equal)
                            .then_with(|| a.doc_id.cmp(&b.doc_id))
                    });
                    results.truncate(top_k);
                }
                Ok(results)
            }
            _ => Ok(primary_results),
        }
    }
}

// cosine_similarity is imported from adapteros_core::vector_math

fn deterministic_sort_and_take(
    mut docs: Vec<(String, String, f32)>,
    top_k: usize,
    min_score: f32,
) -> Result<Vec<RagRetrievedDocument>> {
    docs.sort_by(|(id_a, _, score_a), (id_b, _, score_b)| {
        score_b
            .partial_cmp(score_a)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| id_a.cmp(id_b))
    });

    Ok(docs
        .into_iter()
        .filter(|(_, _, score)| *score >= min_score)
        .take(top_k)
        .map(|(doc_id, text, score)| RagRetrievedDocument {
            doc_id,
            text,
            score,
        })
        .collect())
}

fn drift_matches(sql: &[RagRetrievedDocument], kv: &[RagRetrievedDocument]) -> bool {
    if sql.len() != kv.len() {
        return false;
    }

    const EPS: f32 = 1e-5;
    for (a, b) in sql.iter().zip(kv.iter()) {
        if a.doc_id != b.doc_id {
            return false;
        }
        if (a.score - b.score).abs() > EPS {
            return false;
        }
    }
    true
}
