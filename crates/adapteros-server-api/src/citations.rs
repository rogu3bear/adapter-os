//! Citation collection for the AUDIT phase of the AARA lifecycle.
//!
//! This module provides citation functionality that traces adapter knowledge
//! back to source documents via the training lineage chain:
//!
//! adapter → training_lineage → dataset_version → training_dataset_rows → source_file
//!
//! Citations enable users to verify where model knowledge comes from and
//! supports the Anchor-Audit-Rectify-Act lifecycle.

use crate::state::AppState;
use adapteros_api_types::inference::Citation;
use adapteros_core::{B3Hash, Result};
use sqlx::Row;
use std::collections::HashSet;
use tracing::{debug, warn};

/// Build a citation index for a dataset (no-op until full indexing implemented).
pub async fn build_dataset_index(
    _state: &AppState,
    _dataset_id: &str,
    _tenant_id: &str,
) -> Result<()> {
    Ok(())
}

/// Load or build a citation index for a dataset.
pub async fn load_or_build_index(
    _state: &AppState,
    _dataset_id: &str,
    _tenant_id: &str,
) -> Result<Vec<Citation>> {
    Ok(Vec::new())
}

/// Gather citations for the given adapters by tracing their training lineage.
///
/// This function:
/// 1. Looks up training lineage for each adapter
/// 2. Gets dataset versions from the lineage
/// 3. Extracts source file information from training dataset rows
/// 4. Returns citations with file paths and content hashes
///
/// # Arguments
/// * `state` - Application state with database connection
/// * `tenant_id` - Tenant ID for isolation
/// * `adapters` - List of adapter IDs used in inference
/// * `query` - The query text (reserved for future relevance ranking)
/// * `top_k` - Maximum citations to return
///
/// # Returns
/// List of citations traced from the adapters' training data
pub async fn collect_citations_for_adapters(
    state: &AppState,
    tenant_id: &str,
    adapters: &[String],
    _query: &str,
    top_k: usize,
) -> Vec<Citation> {
    if adapters.is_empty() {
        return Vec::new();
    }

    let mut citations = Vec::new();
    let mut seen_files: HashSet<String> = HashSet::new();

    for adapter_id in adapters {
        // Look up training lineage for this adapter
        let lineage_result = sqlx::query(
            r#"
            SELECT dataset_version_id, training_job_id
            FROM adapter_training_lineage
            WHERE adapter_id = ?
            LIMIT 1
            "#,
        )
        .bind(adapter_id)
        .fetch_optional(state.db.pool())
        .await;

        let lineage = match lineage_result {
            Ok(Some(row)) => row,
            Ok(None) => {
                debug!(adapter_id = %adapter_id, "No training lineage found for adapter");
                continue;
            }
            Err(e) => {
                warn!(adapter_id = %adapter_id, error = %e, "Failed to query training lineage");
                continue;
            }
        };

        let dataset_version_id: Option<String> =
            lineage.try_get("dataset_version_id").ok().flatten();
        let Some(dsv_id) = dataset_version_id else {
            continue;
        };

        // Get source files from training dataset rows
        let rows_result = sqlx::query(
            r#"
            SELECT DISTINCT
                source_file,
                content_hash_b3,
                chunk_index,
                line_start,
                line_end,
                content_preview
            FROM training_dataset_rows
            WHERE dataset_version_id = ? AND tenant_id = ?
            ORDER BY chunk_index ASC
            LIMIT ?
            "#,
        )
        .bind(&dsv_id)
        .bind(tenant_id)
        .bind(top_k as i64)
        .fetch_all(state.db.pool())
        .await;

        let rows = match rows_result {
            Ok(r) => r,
            Err(e) => {
                warn!(dataset_version_id = %dsv_id, error = %e, "Failed to query dataset rows");
                continue;
            }
        };

        for row in rows {
            let source_file: Option<String> = row.try_get("source_file").ok().flatten();
            let content_hash: Option<String> = row.try_get("content_hash_b3").ok().flatten();
            let chunk_index: Option<i64> = row.try_get("chunk_index").ok().flatten();
            let line_start: Option<i64> = row.try_get("line_start").ok().flatten();
            let line_end: Option<i64> = row.try_get("line_end").ok().flatten();
            let preview: Option<String> = row.try_get("content_preview").ok().flatten();

            let Some(file_path) = source_file else {
                continue;
            };

            // Deduplicate by file path
            if seen_files.contains(&file_path) {
                continue;
            }
            seen_files.insert(file_path.clone());

            // Generate deterministic citation ID from content hash
            let citation_id = content_hash.as_ref().map(|h| {
                let hash_input = format!("{}:{}:{}", adapter_id, file_path, h);
                B3Hash::hash(hash_input.as_bytes()).to_hex()[..16].to_string()
            });

            citations.push(Citation {
                adapter_id: adapter_id.clone(),
                file_path,
                chunk_id: chunk_index
                    .map(|i| format!("chunk_{}", i))
                    .unwrap_or_default(),
                offset_start: line_start.unwrap_or(0) as u64,
                offset_end: line_end.unwrap_or(0) as u64,
                preview: preview.unwrap_or_else(|| "[content preview not available]".to_string()),
                citation_id,
                page_number: None,
                char_range: None,
                bbox: None,
                relevance_score: None,
                rank: None,
            });

            if citations.len() >= top_k {
                return citations;
            }
        }
    }

    debug!(
        adapter_count = adapters.len(),
        citation_count = citations.len(),
        "Collected citations for inference"
    );

    citations
}
