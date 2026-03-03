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
use adapteros_api_types::inference::{Citation, DocumentLink};
use adapteros_core::{B3Hash, Result};
use serde_json::Value;
use sqlx::Row;
use std::collections::{BTreeMap, HashSet};
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

fn deterministic_preview(prompt: Option<&str>, response: Option<&str>) -> String {
    const MAX_PREVIEW_CHARS: usize = 160;

    let text = prompt
        .filter(|value| !value.trim().is_empty())
        .or_else(|| response.filter(|value| !value.trim().is_empty()))
        .unwrap_or("[content preview not available]");

    text.chars().take(MAX_PREVIEW_CHARS).collect()
}

#[derive(Debug, Clone)]
struct DocumentLinkCandidate {
    adapter_id: String,
    dataset_version_id: Option<String>,
    source_file: Option<String>,
}

fn extract_trimmed_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

fn extract_document_ref_from_object(
    object: &serde_json::Map<String, Value>,
) -> Option<(String, Option<String>)> {
    let document_id = extract_trimmed_string(object.get("source_document_id"))
        .or_else(|| extract_trimmed_string(object.get("document_id")))
        .or_else(|| extract_trimmed_string(object.get("doc_id")))
        .or_else(|| extract_trimmed_string(object.get("source_doc_id")));

    let document_name = extract_trimmed_string(object.get("source_document_name"))
        .or_else(|| extract_trimmed_string(object.get("document_name")))
        .or_else(|| extract_trimmed_string(object.get("doc_name")));

    document_id.map(|id| (id, document_name))
}

fn extract_document_ref_from_metadata(
    metadata_json: Option<&str>,
) -> Option<(String, Option<String>)> {
    let raw = metadata_json?.trim();
    if raw.is_empty() {
        return None;
    }

    let value: Value = serde_json::from_str(raw).ok()?;
    let object = value.as_object()?;

    if let Some(found) = extract_document_ref_from_object(object) {
        return Some(found);
    }

    if let Some(source_obj) = object.get("source").and_then(Value::as_object) {
        if let Some(found) = extract_document_ref_from_object(source_obj) {
            return Some(found);
        }
    }

    if let Some(provenance_obj) = object.get("provenance").and_then(Value::as_object) {
        if let Some(found) = extract_document_ref_from_object(provenance_obj) {
            return Some(found);
        }
    }

    if let Some(provenance_raw) = object.get("provenance").and_then(Value::as_str) {
        if let Ok(parsed) = serde_json::from_str::<Value>(provenance_raw) {
            if let Some(provenance_obj) = parsed.as_object() {
                return extract_document_ref_from_object(provenance_obj);
            }
        }
    }

    None
}

fn parse_source_document_ids(raw: Option<&str>) -> Vec<String> {
    let Some(raw) = raw.map(str::trim).filter(|v| !v.is_empty()) else {
        return Vec::new();
    };

    let parsed: Value = match serde_json::from_str(raw) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };

    let mut ids = Vec::new();
    let Some(array) = parsed.as_array() else {
        return ids;
    };

    for value in array {
        if let Some(id) = value.as_str().map(str::trim).filter(|v| !v.is_empty()) {
            ids.push(id.to_string());
        }
    }
    ids
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

    let Some(pool) = state.db.pool_opt() else {
        return vec![];
    };

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
        .fetch_optional(pool)
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
            SELECT
                id,
                source_file,
                source_line,
                content_hash_b3,
                prompt,
                response
            FROM training_dataset_rows
            WHERE dataset_version_id = ? AND tenant_id = ?
            ORDER BY id ASC
            LIMIT ?
            "#,
        )
        .bind(&dsv_id)
        .bind(tenant_id)
        .bind(top_k as i64)
        .fetch_all(pool)
        .await;

        let rows = match rows_result {
            Ok(r) => r,
            Err(e) => {
                warn!(dataset_version_id = %dsv_id, error = %e, "Failed to query dataset rows");
                continue;
            }
        };

        for row in rows {
            let row_id: Option<String> = row.try_get("id").ok().flatten();
            let source_file: Option<String> = row.try_get("source_file").ok().flatten();
            let content_hash: Option<String> = row.try_get("content_hash_b3").ok().flatten();
            let source_line: Option<i64> = row.try_get("source_line").ok().flatten();
            let prompt: Option<String> = row.try_get("prompt").ok().flatten();
            let response: Option<String> = row.try_get("response").ok().flatten();

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
                chunk_id: row_id
                    .map(|id| format!("chunk_{}", id))
                    .unwrap_or_else(|| "chunk_0".to_string()),
                offset_start: source_line.unwrap_or(0).max(0) as u64,
                offset_end: (source_line.unwrap_or(0).max(0) as u64).saturating_add(1),
                preview: deterministic_preview(prompt.as_deref(), response.as_deref()),
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

/// Gather clickable source document links for the given adapters.
///
/// Links are derived from adapter training lineage and dataset row metadata.
/// This is best-effort and always tenant-scoped.
pub async fn collect_document_links_for_adapters(
    state: &AppState,
    tenant_id: &str,
    adapters: &[String],
    top_k: usize,
) -> Vec<DocumentLink> {
    if adapters.is_empty() || top_k == 0 {
        return Vec::new();
    }

    let Some(pool) = state.db.pool_opt() else {
        return Vec::new();
    };

    let mut sorted_adapters: Vec<String> = adapters
        .iter()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect();
    sorted_adapters.sort();
    sorted_adapters.dedup();

    let mut candidates: BTreeMap<String, DocumentLinkCandidate> = BTreeMap::new();

    for adapter_id in sorted_adapters {
        let lineage_rows = match sqlx::query(
            r#"
            SELECT dataset_version_id, training_job_id
            FROM adapter_training_lineage
            WHERE adapter_id = ? AND tenant_id = ?
            ORDER BY ordinal ASC, created_at DESC, id ASC
            "#,
        )
        .bind(&adapter_id)
        .bind(tenant_id)
        .fetch_all(pool)
        .await
        {
            Ok(rows) => rows,
            Err(e) => {
                warn!(
                    adapter_id = %adapter_id,
                    error = %e,
                    "Failed to query lineage for document links"
                );
                continue;
            }
        };

        for lineage in lineage_rows {
            let dataset_version_id: Option<String> =
                lineage.try_get("dataset_version_id").ok().flatten();
            let training_job_id: Option<String> = lineage.try_get("training_job_id").ok().flatten();

            if let Some(dsv_id) = dataset_version_id.clone() {
                let rows = match sqlx::query(
                    r#"
                    SELECT metadata_json, source_file
                    FROM training_dataset_rows
                    WHERE dataset_version_id = ? AND tenant_id = ?
                    ORDER BY id ASC
                    LIMIT 500
                    "#,
                )
                .bind(&dsv_id)
                .bind(tenant_id)
                .fetch_all(pool)
                .await
                {
                    Ok(rows) => rows,
                    Err(e) => {
                        warn!(
                            adapter_id = %adapter_id,
                            dataset_version_id = %dsv_id,
                            error = %e,
                            "Failed to query dataset rows for document links"
                        );
                        Vec::new()
                    }
                };

                for row in rows {
                    let metadata_json: Option<String> = row.try_get("metadata_json").ok().flatten();
                    let source_file: Option<String> = row.try_get("source_file").ok().flatten();

                    let Some((document_id, _document_name)) =
                        extract_document_ref_from_metadata(metadata_json.as_deref())
                    else {
                        continue;
                    };

                    candidates
                        .entry(document_id)
                        .or_insert_with(|| DocumentLinkCandidate {
                            adapter_id: adapter_id.clone(),
                            dataset_version_id: Some(dsv_id.clone()),
                            source_file,
                        });
                }
            }

            if candidates.len() >= top_k {
                break;
            }

            // Legacy fallback: training jobs may store source document IDs directly.
            if let Some(job_id) = training_job_id {
                let source_documents_json: Option<String> = match sqlx::query(
                    r#"
                    SELECT source_documents_json
                    FROM repository_training_jobs
                    WHERE id = ? AND tenant_id = ?
                    LIMIT 1
                    "#,
                )
                .bind(&job_id)
                .bind(tenant_id)
                .fetch_optional(pool)
                .await
                {
                    Ok(Some(row)) => row.try_get("source_documents_json").ok().flatten(),
                    Ok(None) => None,
                    Err(e) => {
                        warn!(
                            adapter_id = %adapter_id,
                            training_job_id = %job_id,
                            error = %e,
                            "Failed to query training job source documents"
                        );
                        None
                    }
                };

                for document_id in parse_source_document_ids(source_documents_json.as_deref()) {
                    candidates
                        .entry(document_id)
                        .or_insert_with(|| DocumentLinkCandidate {
                            adapter_id: adapter_id.clone(),
                            dataset_version_id: dataset_version_id.clone(),
                            source_file: None,
                        });
                }
            }

            if candidates.len() >= top_k {
                break;
            }
        }

        if candidates.len() >= top_k {
            break;
        }
    }

    if candidates.is_empty() {
        return Vec::new();
    }

    let ordered_ids: Vec<String> = candidates.keys().cloned().collect();
    let documents = match state
        .db
        .get_documents_by_ids_ordered(tenant_id, &ordered_ids)
        .await
    {
        Ok(docs) => docs,
        Err(e) => {
            warn!(
                tenant_id = %tenant_id,
                error = %e,
                "Failed to resolve documents for inference links"
            );
            return Vec::new();
        }
    };

    let mut links = Vec::new();
    for (document_id, document) in ordered_ids.into_iter().zip(documents.into_iter()) {
        let Some(document) = document else {
            continue;
        };
        let Some(candidate) = candidates.get(&document_id) else {
            continue;
        };

        links.push(DocumentLink {
            document_id: document.id.clone(),
            document_name: document.name,
            download_url: format!("/v1/documents/{}/download", document.id),
            adapter_id: Some(candidate.adapter_id.clone()),
            dataset_version_id: candidate.dataset_version_id.clone(),
            source_file: candidate.source_file.clone(),
        });

        if links.len() >= top_k {
            break;
        }
    }

    debug!(
        tenant_id = %tenant_id,
        adapter_count = adapters.len(),
        document_link_count = links.len(),
        "Collected document links for inference"
    );

    links
}
