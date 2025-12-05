//! RAG storage abstraction over SQL + KV with deterministic retrieval.

use crate::{Db, StorageMode};
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_storage::{RagDocumentKv, RagRepository};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, warn};

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
            .execute(self.pool())
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
            .execute(self.pool())
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
            .execute(self.pool())
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
            .fetch_all(self.pool())
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
            .fetch_all(self.pool())
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
        let rows: Vec<(String, String, String, String, String, Option<String>, String)> =
            sqlx::query_as(
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
            .fetch_all(self.pool())
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

        deterministic_sort_and_take(scored_docs, top_k)
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

        deterministic_sort_and_take(scored_docs, top_k)
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if mag_a == 0.0 || mag_b == 0.0 {
        return 0.0;
    }
    dot / (mag_a * mag_b)
}

fn deterministic_sort_and_take(
    mut docs: Vec<(String, String, f32)>,
    top_k: usize,
) -> Result<Vec<RagRetrievedDocument>> {
    docs.sort_by(|(id_a, _, score_a), (id_b, _, score_b)| {
        score_b
            .partial_cmp(score_a)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| id_a.cmp(id_b))
    });

    Ok(docs
        .into_iter()
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
