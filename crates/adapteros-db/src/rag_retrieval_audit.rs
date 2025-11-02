//! Provides non-blocking audit logging and query helpers for operator/administrator
//! observability. Works directly with sqlx pools to avoid plumbing higher-level state.

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

/// Audit record returned for recent retrievals
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagRetrievalAuditRecord {
    pub tenant_id: String,
    pub query_hash: String,
    pub doc_ids: Vec<String>,
    pub scores: Vec<f32>,
    pub top_k: i64,
    pub embedding_model_hash: String,
    pub created_at: String,
}

/// Count record for RAG retrieval statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagRetrievalCount {
    pub tenant_id: String,
    pub count: i64,
}

// ========== INSERTS ==========

pub async fn log_rag_retrieval_sqlite(
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
         VALUES (?, ?, ?, ?, ?, ?, datetime('now'))",
    )
    .bind(tenant_id)
    .bind(query_hash_hex)
    .bind(doc_ids_json)
    .bind(scores_json)
    .bind(top_k)
    .bind(embedding_model_hash)
    .execute(pool)
    .await
    .map_err(|e| AosError::Database(format!("RAG audit insert (sqlite) failed: {}", e)))?;
    Ok(())
}

// Postgres functions disabled - SQLite only for now

// ========== QUERIES ==========

pub async fn list_recent_rag_retrievals_sqlite(
    pool: &SqlitePool,
    limit: i64,
    tenant_opt: Option<&str>,
) -> Result<Vec<RagRetrievalAuditRecord>> {
    let rows = if let Some(t) = tenant_opt {
        sqlx::query(
            "SELECT tenant_id, query_hash, retrieved_doc_ids, retrieved_scores, top_k, embedding_model_hash, created_at \
             FROM rag_retrieval_audit \
             WHERE tenant_id = ? \
             ORDER BY created_at DESC \
             LIMIT ?",
        )
        .bind(t)
        .bind(limit)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query(
            "SELECT tenant_id, query_hash, retrieved_doc_ids, retrieved_scores, top_k, embedding_model_hash, created_at \
             FROM rag_retrieval_audit \
             ORDER BY created_at DESC \
             LIMIT ?",
        )
        .bind(limit)
        .fetch_all(pool)
        .await
    }
    .map_err(|e| AosError::Database(format!("RAG audit list (sqlite) failed: {}", e)))?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let tenant_id: String = row.get("tenant_id");
        let query_hash: String = row.get("query_hash");
        let doc_ids_json: String = row.get("retrieved_doc_ids");
        let scores_json: String = row.get("retrieved_scores");
        let top_k: i64 = row.get("top_k");
        let embedding_model_hash: String = row.get("embedding_model_hash");
        let created_at: String = row.get("created_at");

        let doc_ids: Vec<String> = serde_json::from_str(&doc_ids_json).unwrap_or_default();
        let scores: Vec<f32> = serde_json::from_str(&scores_json).unwrap_or_default();
        out.push(RagRetrievalAuditRecord {
            tenant_id,
            query_hash,
            doc_ids,
            scores,
            top_k,
            embedding_model_hash,
            created_at,
        });
    }
    Ok(out)
}

pub async fn rag_retrieval_counts_by_tenant_sqlite(
    pool: &SqlitePool,
    window_days: Option<i64>,
) -> Result<Vec<RagRetrievalCount>> {
    let rows = if let Some(days) = window_days {
        sqlx::query(
            "SELECT tenant_id, COUNT(*) as count \
             FROM rag_retrieval_audit \
             WHERE created_at >= datetime('now', '-' || ? || ' days') \
             GROUP BY tenant_id \
             ORDER BY count DESC",
        )
        .bind(days)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query(
            "SELECT tenant_id, COUNT(*) as count \
             FROM rag_retrieval_audit \
             GROUP BY tenant_id \
             ORDER BY count DESC",
        )
        .fetch_all(pool)
        .await
    }
    .map_err(|e| AosError::Database(format!("RAG count query (sqlite) failed: {}", e)))?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let tenant_id: String = row.get("tenant_id");
        let count: i64 = row.get("count");
        out.push(RagRetrievalCount { tenant_id, count });
    }
    Ok(out)
}
