//! Embedding benchmark database operations
//!
//! Storage and retrieval for embedding benchmark reports.

use crate::Db;
use adapteros_core::{AosError, Result};
use sqlx::FromRow;

/// Database row for embedding benchmark reports
#[derive(Debug, Clone, FromRow)]
pub struct EmbeddingBenchmarkRow {
    pub id: i64,
    pub report_id: String,
    pub tenant_id: String,
    pub timestamp: String,
    pub model_name: String,
    pub model_hash: String,
    pub is_finetuned: bool,
    pub corpus_version: String,
    pub num_chunks: i64,
    pub recall_at_10: f64,
    pub ndcg_at_10: f64,
    pub mrr_at_10: f64,
    pub determinism_pass: bool,
    pub determinism_runs: i64,
}

impl Db {
    /// List embedding benchmarks for a tenant
    pub async fn list_embedding_benchmarks(
        &self,
        tenant_id: &str,
        model_name_filter: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<EmbeddingBenchmarkRow>> {
        let rows = if let Some(filter) = model_name_filter {
            sqlx::query_as::<_, EmbeddingBenchmarkRow>(
                r#"
                SELECT id, report_id, tenant_id, timestamp, model_name, model_hash,
                       is_finetuned, corpus_version, num_chunks, recall_at_10,
                       ndcg_at_10, mrr_at_10, determinism_pass, determinism_runs
                FROM embedding_benchmarks
                WHERE tenant_id = ? AND model_name LIKE ?
                ORDER BY timestamp DESC
                LIMIT ? OFFSET ?
                "#,
            )
            .bind(tenant_id)
            .bind(format!("%{}%", filter))
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to list embedding benchmarks: {}", e)))?
        } else {
            sqlx::query_as::<_, EmbeddingBenchmarkRow>(
                r#"
                SELECT id, report_id, tenant_id, timestamp, model_name, model_hash,
                       is_finetuned, corpus_version, num_chunks, recall_at_10,
                       ndcg_at_10, mrr_at_10, determinism_pass, determinism_runs
                FROM embedding_benchmarks
                WHERE tenant_id = ?
                ORDER BY timestamp DESC
                LIMIT ? OFFSET ?
                "#,
            )
            .bind(tenant_id)
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to list embedding benchmarks: {}", e)))?
        };

        Ok(rows)
    }

    /// Count embedding benchmarks for a tenant
    pub async fn count_embedding_benchmarks(
        &self,
        tenant_id: &str,
        model_name_filter: Option<&str>,
    ) -> Result<i64> {
        let count: (i64,) = if let Some(filter) = model_name_filter {
            sqlx::query_as(
                "SELECT COUNT(*) FROM embedding_benchmarks WHERE tenant_id = ? AND model_name LIKE ?",
            )
            .bind(tenant_id)
            .bind(format!("%{}%", filter))
            .fetch_one(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to count embedding benchmarks: {}", e)))?
        } else {
            sqlx::query_as("SELECT COUNT(*) FROM embedding_benchmarks WHERE tenant_id = ?")
                .bind(tenant_id)
                .fetch_one(self.pool())
                .await
                .map_err(|e| {
                    AosError::Database(format!("Failed to count embedding benchmarks: {}", e))
                })?
        };

        Ok(count.0)
    }
}
