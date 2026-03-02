//! Dataset version handlers.

use super::types::{CreateDatasetVersionRequest, CreateDatasetVersionResponse, DatasetRowEditRequest};
use crate::api_error::ApiError;
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::services::dataset_domain::DatasetDomain;
use crate::services::{CanonicalRow, DatasetDomainService, RawDialect, RawFileDescriptor, RawIngestRequest, SamplingConfig};
use crate::state::AppState;
use crate::types::{DatasetVersionSummary, DatasetVersionsResponse};
use adapteros_db::training_datasets::{CreateTrainingDatasetRowParams, EvidenceFilter, SampleRole};
use adapteros_orchestrator::code_ingestion::normalize_repo_id;
use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Extension, Json,
};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::fs;
use tokio::io::AsyncWriteExt;

/// List all versions for a dataset (ordered latest-first) with effective trust_state.
#[utoipa::path(
    get,
    path = "/v1/datasets/{dataset_id}/versions",
    params(
        ("dataset_id" = String, Path, description = "Dataset ID")
    ),
    responses(
        (status = 200, description = "Dataset versions", body = DatasetVersionsResponse),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Dataset not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn list_dataset_versions(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dataset_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    require_permission(&claims, Permission::DatasetView)?;

    let dataset_id = crate::id_resolver::resolve_any_id(&state.db, &dataset_id).await?;

    // Ensure dataset exists and enforce tenant isolation
    let dataset = state
        .db
        .get_training_dataset_routed(&claims.tenant_id, &dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to load dataset: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Dataset"))?;

    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        return Err(ApiError::forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    let tenant_key = dataset.tenant_id.as_deref().unwrap_or("default");
    let versions = state
        .db
        .list_dataset_versions_routed(tenant_key, &dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to list dataset versions: {}", e)))?;

    // Include repo_slug from parent dataset in version summaries
    let repo_slug = repo_slug_from_dataset(&dataset);

    let mut summaries = Vec::with_capacity(versions.len());
    for version in versions {
        let trust_state = resolve_trust_state(&state.db, &version).await?;
        summaries.push(DatasetVersionSummary {
            dataset_version_id: version.id,
            version_number: version.version_number,
            version_label: version.version_label,
            hash_b3: Some(version.hash_b3),
            storage_path: Some(version.storage_path),
            trust_state: Some(trust_state),
            repo_slug: repo_slug.clone(),
            created_at: version.created_at,
        });
    }

    Ok(Json(DatasetVersionsResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        dataset_id,
        versions: summaries,
    }))
}

/// Create a dataset version explicitly (e.g., to pin a manifest before training).
#[utoipa::path(
    post,
    path = "/v1/datasets/{dataset_id}/versions",
    params(
        ("dataset_id" = String, Path, description = "Dataset ID")
    ),
    request_body = CreateDatasetVersionRequest,
    responses(
        (status = 200, description = "Dataset version created", body = CreateDatasetVersionResponse),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Dataset not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn create_dataset_version(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dataset_id): Path<String>,
    Json(body): Json<CreateDatasetVersionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    require_permission(&claims, Permission::DatasetValidate)?;

    let dataset_id = crate::id_resolver::resolve_any_id(&state.db, &dataset_id).await?;

    let dataset = state
        .db
        .get_training_dataset_routed(&claims.tenant_id, &dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to load dataset: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Dataset"))?;

    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        return Err(ApiError::forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    let tenant_key = dataset.tenant_id.as_deref().unwrap_or("default");

    let version_id = if let Some(row_edits) = body.row_edits.as_ref() {
        if row_edits.is_empty() {
            return Err(ApiError::bad_request(
                "row_edits must include at least one edited row",
            ));
        }

        let base_revision = body
            .base_dataset_version_id
            .as_deref()
            .map(str::to_string)
            .unwrap_or_else(|| "latest".to_string());
        let base_version =
            resolve_version_by_revision(&state, tenant_key, &dataset_id, &base_revision).await?;

        let domain = DatasetDomainService::new(Arc::new(state.clone()));
        let tenant = dataset
            .tenant_id
            .as_deref()
            .unwrap_or(claims.tenant_id.as_str())
            .to_string();

        let base_rows = domain
            .stream_rows(&base_version.id, &tenant, SamplingConfig::default())
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;
        if base_rows.is_empty() {
            return Err(ApiError::bad_request(
                "Base dataset version has no canonical rows to edit",
            ));
        }

        let edited_rows = apply_row_edits(&base_rows, row_edits)?;
        let temp_path = write_rows_to_temp_file(&state, &dataset_id, &edited_rows).await?;

        let ingest_result = domain
            .ingest_raw_dataset(RawIngestRequest {
                tenant_id: tenant.clone(),
                dataset_id: dataset_id.clone(),
                version_label: body.version_label.clone(),
                created_by: Some(claims.sub.clone()),
                files: vec![RawFileDescriptor {
                    path: temp_path.clone(),
                    format: RawDialect::CanonicalJsonl,
                    split: None,
                }],
            })
            .await
            .map_err(|e| ApiError::internal(e.to_string()));

        let _ = fs::remove_file(&temp_path).await;
        let descriptor = ingest_result?;

        let final_rows = domain
            .stream_rows(
                &descriptor.dataset_version_id,
                &tenant,
                SamplingConfig::default(),
            )
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;

        persist_training_rows_for_version(
            &state,
            &dataset_id,
            &descriptor.dataset_version_id,
            dataset.tenant_id.as_deref(),
            Some(&claims.sub),
            &final_rows,
        )
        .await?;

        let _ = persist_dataset_evaluation(
            &state,
            &dataset_id,
            &descriptor.dataset_version_id,
            &final_rows,
            Some(&claims.sub),
        )
        .await;

        descriptor.dataset_version_id
    } else {
        let manifest_json = if let Some(v) = body.manifest_json {
            Some(
                serde_json::to_string(&v)
                    .map_err(|e| ApiError::bad_request(format!("invalid manifest_json: {}", e)))?,
            )
        } else {
            None
        };

        let created_version_id = state
            .db
            .create_training_dataset_version(
                &dataset_id,
                dataset.tenant_id.as_deref(),
                body.version_label.as_deref(),
                &dataset.storage_path,
                &dataset.hash_b3,
                body.manifest_path.as_deref(),
                manifest_json.as_deref(),
                Some(&claims.sub),
            )
            .await
            .map_err(|e| ApiError::db_error(format!("Failed to create dataset version: {}", e)))?;
        created_version_id
    };

    let version = state
        .db
        .get_training_dataset_version_routed(tenant_key, &version_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to fetch created dataset version: {}", e)))?
        .ok_or_else(|| ApiError::internal("Dataset version was created but not found"))?;

    Ok(Json(CreateDatasetVersionResponse {
        dataset_id,
        dataset_version_id: version_id,
        version_number: version.version_number,
        trust_state: version.trust_state,
        created_at: version.created_at,
    }))
}

use axum::extract::Query;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// Query parameters for listing dataset versions
#[derive(Debug, Deserialize, ToSchema, IntoParams)]
pub struct ListVersionsQuery {
    /// Maximum number of versions to return (default: 50, max: 100)
    pub limit: Option<i64>,
    /// Number of versions to skip for pagination
    pub offset: Option<i64>,
    /// Filter by trust state (e.g., "allowed", "blocked", "needs_approval")
    pub trust_state: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema, IntoParams, Default)]
pub struct DatasetVersionDetailQuery {
    /// Optional comparison target version (version id, version number, or "latest")
    pub compare_to: Option<String>,
    /// Regenerate and persist dataset evaluation before returning detail.
    pub regenerate_evaluation: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct DatasetSourceSpan {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_start: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_end: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub char_start: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub char_end: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct DatasetEvaluationCoverageStats {
    pub total_rows: usize,
    pub rows_with_response: usize,
    pub rows_with_source_span: usize,
    pub rows_with_provenance_metadata: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct DatasetEvaluationDuplicationStats {
    pub duplicate_prompt_response_pairs: usize,
    pub duplicate_prompt_only: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct DatasetEvaluationLeakageRisk {
    pub risky_row_count: usize,
    pub risk_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct DatasetSchemaAnomaly {
    pub issue: String,
    pub row_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_span: Option<DatasetSourceSpan>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct DatasetEvaluationExampleRow {
    pub row_id: String,
    pub prompt: String,
    pub response: String,
    pub split: String,
    pub weight: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_span: Option<DatasetSourceSpan>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct DatasetEvaluationCitation {
    pub issue: String,
    pub row_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_span: Option<DatasetSourceSpan>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct DatasetEvaluationArtifact {
    pub artifact_id: String,
    pub dataset_id: String,
    pub dataset_version_id: String,
    pub generated_at: String,
    pub generator_version: String,
    pub coverage_stats: DatasetEvaluationCoverageStats,
    pub duplication_stats: DatasetEvaluationDuplicationStats,
    pub leakage_risk: DatasetEvaluationLeakageRisk,
    pub schema_anomalies: Vec<DatasetSchemaAnomaly>,
    pub example_rows: Vec<DatasetEvaluationExampleRow>,
    pub citations: Vec<DatasetEvaluationCitation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct DatasetVersionRowChange {
    pub row_id: String,
    pub change_type: String,
    pub changed_fields: Vec<String>,
    pub provenance_impact: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct DatasetVersionCompareSummary {
    pub base_dataset_version_id: String,
    pub compare_dataset_version_id: String,
    pub total_changed_rows: usize,
    pub changed_rows: Vec<DatasetVersionRowChange>,
}

/// Response for a single dataset version with full details
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct DatasetVersionDetailResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub dataset_id: String,
    pub dataset_version_id: String,
    pub version_number: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_label: Option<String>,
    pub hash_b3: String,
    pub storage_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_path: Option<String>,
    pub validation_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_errors: Option<Vec<String>>,
    pub pii_status: String,
    pub toxicity_status: String,
    pub leak_status: String,
    pub anomaly_status: String,
    pub overall_safety_status: String,
    pub trust_state: String,
    pub overall_trust_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sensitivity: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locked_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evaluation: Option<DatasetEvaluationArtifact>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compare: Option<DatasetVersionCompareSummary>,
}

fn schema_version() -> String {
    adapteros_api_types::API_SCHEMA_VERSION.to_string()
}

const DATASET_EVALUATION_EVIDENCE_TYPE: &str = "dataset_evaluation_v1";
const DATASET_EVALUATION_GENERATOR_VERSION: &str = "dataset-evaluator-v1";
const DEFAULT_VERSION_DETAIL_ROW_LIMIT: usize = 5000;
const MAX_CHANGED_ROWS_RETURNED: usize = 200;

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn metadata_string(metadata: &Map<String, Value>, key: &str) -> Option<String> {
    metadata
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

fn metadata_i32(metadata: &Map<String, Value>, key: &str) -> Option<i32> {
    let raw = metadata.get(key)?;
    match raw {
        Value::Number(num) => num.as_i64().and_then(|n| i32::try_from(n).ok()),
        Value::String(text) => text.trim().parse::<i32>().ok(),
        _ => None,
    }
}

fn extract_source_span(metadata: &Map<String, Value>) -> Option<DatasetSourceSpan> {
    let source_file = metadata_string(metadata, "source_file")
        .or_else(|| metadata_string(metadata, "source_document_name"))
        .or_else(|| metadata_string(metadata, "file_path"));
    let page_start =
        metadata_i32(metadata, "source_page_number").or_else(|| metadata_i32(metadata, "page_start"));
    let page_end = metadata_i32(metadata, "source_page_end").or(page_start);
    let char_start =
        metadata_i32(metadata, "source_start_offset").or_else(|| metadata_i32(metadata, "char_start"));
    let char_end =
        metadata_i32(metadata, "source_end_offset").or_else(|| metadata_i32(metadata, "char_end"));

    if source_file.is_none() && page_start.is_none() && char_start.is_none() {
        return None;
    }

    Some(DatasetSourceSpan {
        source_file,
        page_start,
        page_end,
        char_start,
        char_end,
    })
}

fn row_has_response(row: &CanonicalRow) -> bool {
    !row.response.trim().is_empty()
}

fn row_has_provenance(metadata: &Map<String, Value>) -> bool {
    metadata.contains_key("source_file")
        || metadata.contains_key("source_document_name")
        || metadata.contains_key("source_page_number")
        || metadata.contains_key("source_start_offset")
        || metadata.contains_key("source_end_offset")
}

fn row_has_risk(row: &CanonicalRow) -> bool {
    let haystack = format!(
        "{} {}",
        row.prompt.to_ascii_lowercase(),
        row.response.to_ascii_lowercase()
    );
    haystack.contains("api_key")
        || haystack.contains("secret")
        || haystack.contains("password")
        || haystack.contains("-----begin")
        || haystack.contains("@")
}

fn push_unique_citation(
    citations: &mut Vec<DatasetEvaluationCitation>,
    issue: &str,
    row: &CanonicalRow,
) {
    if citations.iter().any(|c| c.issue == issue && c.row_id == row.row_id) {
        return;
    }
    citations.push(DatasetEvaluationCitation {
        issue: issue.to_string(),
        row_id: row.row_id.clone(),
        source_span: extract_source_span(&row.metadata),
    });
}

fn evaluate_rows(
    artifact_id: String,
    dataset_id: &str,
    dataset_version_id: &str,
    rows: &[CanonicalRow],
) -> DatasetEvaluationArtifact {
    let mut prompt_response_seen: HashSet<(String, String)> = HashSet::new();
    let mut prompt_seen: HashSet<String> = HashSet::new();
    let mut duplicate_prompt_response_pairs = 0usize;
    let mut duplicate_prompt_only = 0usize;
    let mut risky_row_count = 0usize;
    let mut rows_with_response = 0usize;
    let mut rows_with_source_span = 0usize;
    let mut rows_with_provenance_metadata = 0usize;
    let mut anomalies = Vec::new();
    let mut citations = Vec::new();
    let mut examples = Vec::new();

    for row in rows {
        if row_has_response(row) {
            rows_with_response += 1;
        }

        if row_has_provenance(&row.metadata) {
            rows_with_provenance_metadata += 1;
        }

        if extract_source_span(&row.metadata).is_some() {
            rows_with_source_span += 1;
        }

        let pair_key = (
            row.prompt.trim().to_string(),
            row.response.trim().to_string(),
        );
        if !prompt_response_seen.insert(pair_key) {
            duplicate_prompt_response_pairs += 1;
            push_unique_citation(&mut citations, "duplicate_prompt_response_pair", row);
        }

        let prompt_key = row.prompt.trim().to_string();
        if !prompt_seen.insert(prompt_key) {
            duplicate_prompt_only += 1;
            push_unique_citation(&mut citations, "duplicate_prompt", row);
        }

        if row.prompt.trim().is_empty() {
            anomalies.push(DatasetSchemaAnomaly {
                issue: "empty_prompt".to_string(),
                row_id: row.row_id.clone(),
                source_span: extract_source_span(&row.metadata),
            });
            push_unique_citation(&mut citations, "empty_prompt", row);
        }

        if row.split.trim().is_empty() {
            anomalies.push(DatasetSchemaAnomaly {
                issue: "empty_split".to_string(),
                row_id: row.row_id.clone(),
                source_span: extract_source_span(&row.metadata),
            });
            push_unique_citation(&mut citations, "empty_split", row);
        }

        if row.weight.is_sign_negative() {
            anomalies.push(DatasetSchemaAnomaly {
                issue: "negative_weight".to_string(),
                row_id: row.row_id.clone(),
                source_span: extract_source_span(&row.metadata),
            });
            push_unique_citation(&mut citations, "negative_weight", row);
        }

        if row_has_risk(row) {
            risky_row_count += 1;
            push_unique_citation(&mut citations, "leakage_risk", row);
        }

        if examples.len() < 5 {
            examples.push(DatasetEvaluationExampleRow {
                row_id: row.row_id.clone(),
                prompt: row.prompt.clone(),
                response: row.response.clone(),
                split: row.split.clone(),
                weight: row.weight,
                source_span: extract_source_span(&row.metadata),
            });
        }
    }

    let risk_score = if rows.is_empty() {
        0.0
    } else {
        (risky_row_count as f32) / (rows.len() as f32)
    };

    DatasetEvaluationArtifact {
        artifact_id,
        dataset_id: dataset_id.to_string(),
        dataset_version_id: dataset_version_id.to_string(),
        generated_at: now_rfc3339(),
        generator_version: DATASET_EVALUATION_GENERATOR_VERSION.to_string(),
        coverage_stats: DatasetEvaluationCoverageStats {
            total_rows: rows.len(),
            rows_with_response,
            rows_with_source_span,
            rows_with_provenance_metadata,
        },
        duplication_stats: DatasetEvaluationDuplicationStats {
            duplicate_prompt_response_pairs,
            duplicate_prompt_only,
        },
        leakage_risk: DatasetEvaluationLeakageRisk {
            risky_row_count,
            risk_score,
        },
        schema_anomalies: anomalies,
        example_rows: examples,
        citations,
    }
}

async fn persist_dataset_evaluation(
    state: &AppState,
    dataset_id: &str,
    dataset_version_id: &str,
    rows: &[CanonicalRow],
    created_by: Option<&str>,
) -> Result<DatasetEvaluationArtifact, ApiError> {
    let artifact_id = crate::id_generator::readable_id(adapteros_id::IdPrefix::Dst, "eval");
    let evaluation = evaluate_rows(artifact_id, dataset_id, dataset_version_id, rows);
    let payload = serde_json::to_string(&evaluation).map_err(|e| {
        ApiError::internal(format!("Failed to serialize dataset evaluation artifact: {}", e))
    })?;
    let summary = format!(
        "coverage={} duplicates={} leakage={:.3}",
        evaluation.coverage_stats.total_rows,
        evaluation.duplication_stats.duplicate_prompt_response_pairs,
        evaluation.leakage_risk.risk_score
    );

    state
        .db
        .create_evidence_entry(
            Some(dataset_id),
            None,
            DATASET_EVALUATION_EVIDENCE_TYPE,
            dataset_version_id,
            Some(summary.as_str()),
            "high",
            created_by,
            Some(payload.as_str()),
        )
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to persist dataset evaluation: {}", e)))?;

    Ok(evaluation)
}

async fn load_latest_dataset_evaluation(
    state: &AppState,
    dataset_id: &str,
    dataset_version_id: &str,
) -> Result<Option<DatasetEvaluationArtifact>, ApiError> {
    let filter = EvidenceFilter {
        dataset_id: Some(dataset_id.to_string()),
        adapter_id: None,
        evidence_type: Some(DATASET_EVALUATION_EVIDENCE_TYPE.to_string()),
        confidence: None,
        limit: Some(100),
    };
    let entries = state
        .db
        .list_evidence_entries(&filter)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to load dataset evaluation: {}", e)))?;

    for entry in entries {
        if entry.reference != dataset_version_id {
            continue;
        }
        if let Some(raw) = entry.metadata_json {
            if let Ok(parsed) = serde_json::from_str::<DatasetEvaluationArtifact>(&raw) {
                return Ok(Some(parsed));
            }
        }
    }

    Ok(None)
}

fn split_normalized(value: Option<&str>, fallback: &str) -> String {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(fallback)
        .to_ascii_lowercase()
}

fn apply_row_edits(
    rows: &[CanonicalRow],
    edits: &[DatasetRowEditRequest],
) -> Result<Vec<CanonicalRow>, ApiError> {
    let mut edits_by_row: HashMap<&str, &DatasetRowEditRequest> = HashMap::new();
    for edit in edits {
        let key = edit.row_id.trim();
        if key.is_empty() {
            return Err(ApiError::bad_request("row_edits[].row_id is required"));
        }
        if edits_by_row.insert(key, edit).is_some() {
            return Err(ApiError::bad_request(
                "Duplicate row_edits[].row_id values are not allowed",
            ));
        }
    }

    let mut edited_rows = Vec::with_capacity(rows.len());
    let mut applied_edits = 0usize;
    for row in rows {
        let mut next = row.clone();
        if let Some(edit) = edits_by_row.get(row.row_id.as_str()) {
            applied_edits += 1;
            let original_prompt = next.prompt.clone();
            let original_response = next.response.clone();
            let original_weight = next.weight;
            let original_split = next.split.clone();

            if let Some(prompt) = edit.prompt.as_ref() {
                next.prompt = prompt.clone();
            }
            if let Some(response) = edit.response.as_ref() {
                next.response = response.clone();
            }
            if let Some(weight) = edit.weight {
                next.weight = weight;
            }
            if let Some(split) = edit.split.as_ref() {
                next.split = split_normalized(Some(split), &next.split);
            }

            let changed = next.prompt != original_prompt
                || next.response != original_response
                || (next.weight - original_weight).abs() > f32::EPSILON
                || next.split != original_split;
            if changed {
                next.metadata.insert(
                    "provenance_invalidated".to_string(),
                    Value::Bool(true),
                );
                next.metadata.insert(
                    "provenance_invalidated_reason".to_string(),
                    Value::String("row_edited".to_string()),
                );
                next.metadata.insert(
                    "provenance_invalidated_from_row_id".to_string(),
                    Value::String(row.row_id.clone()),
                );
                next.metadata.insert(
                    "provenance_invalidated_at".to_string(),
                    Value::String(now_rfc3339()),
                );
            }
        }
        edited_rows.push(next);
    }

    if applied_edits != edits_by_row.len() {
        return Err(ApiError::bad_request(
            "One or more row_edits.row_id values do not exist in the base version",
        ));
    }

    Ok(edited_rows)
}

async fn write_rows_to_temp_file(
    state: &AppState,
    dataset_id: &str,
    rows: &[CanonicalRow],
) -> Result<PathBuf, ApiError> {
    let paths = crate::handlers::datasets::DatasetPaths::from_state(state)
        .map_err(|e| ApiError::internal(e.to_string()))?;
    super::ensure_dirs([paths.temp.as_path()])
        .await
        .map_err(|(_, payload)| ApiError::internal(payload.0.message.clone()))?;

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| ApiError::internal(format!("Clock drift while creating temp file: {}", e)))?
        .as_nanos();
    let path = paths
        .temp
        .join(format!("dataset-row-edit-{}-{}.jsonl", dataset_id, timestamp));

    let mut file = fs::File::create(&path)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create temp dataset rows file: {}", e)))?;

    for row in rows {
        let line = serde_json::to_string(row).map_err(|e| {
            ApiError::internal(format!("Failed to serialize canonical row for temp file: {}", e))
        })?;
        file.write_all(line.as_bytes())
            .await
            .map_err(|e| ApiError::internal(format!("Failed to write temp dataset rows file: {}", e)))?;
        file.write_all(b"\n")
            .await
            .map_err(|e| ApiError::internal(format!("Failed to write temp dataset rows file: {}", e)))?;
    }

    file.flush()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to flush temp dataset rows file: {}", e)))?;
    Ok(path)
}

fn metadata_to_json(meta: &Map<String, Value>) -> Option<String> {
    if meta.is_empty() {
        None
    } else {
        serde_json::to_string(meta).ok()
    }
}

fn sample_role_from_weight(weight: f32) -> SampleRole {
    if weight.is_sign_negative() {
        SampleRole::Negative
    } else {
        SampleRole::Positive
    }
}

async fn persist_training_rows_for_version(
    state: &AppState,
    dataset_id: &str,
    dataset_version_id: &str,
    tenant_id: Option<&str>,
    created_by: Option<&str>,
    rows: &[CanonicalRow],
) -> Result<(), ApiError> {
    let mut inserts = Vec::with_capacity(rows.len());
    for row in rows {
        let mut row_metadata = row.metadata.clone();
        row_metadata
            .entry("canonical_row_id".to_string())
            .or_insert_with(|| Value::String(row.row_id.clone()));

        let mut builder = CreateTrainingDatasetRowParams::builder(
            dataset_id,
            row.prompt.clone(),
            row.response.clone(),
        )
        .dataset_version_id(dataset_version_id)
        .weight(row.weight as f64)
        .split(row.split.clone())
        .sample_role(sample_role_from_weight(row.weight));

        if let Some(value) = metadata_string(&row_metadata, "source_type") {
            builder = builder.source_type(value);
        }
        if let Some(value) = metadata_string(&row_metadata, "source_file")
            .or_else(|| metadata_string(&row_metadata, "source_document_name"))
        {
            builder = builder.source_file(value);
        }
        if let Some(value) = metadata_i32(&row_metadata, "source_line") {
            builder = builder.source_line(value);
        }
        if let Some(value) = tenant_id {
            builder = builder.tenant_id(value);
        }
        if let Some(value) = created_by {
            builder = builder.created_by(value);
        }
        if let Some(metadata_json) = metadata_to_json(&row_metadata) {
            builder = builder.metadata_json(metadata_json);
        }

        inserts.push(builder.build());
    }

    state
        .db
        .bulk_insert_training_dataset_rows(&inserts)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to persist dataset rows: {}", e)))?;
    Ok(())
}

async fn resolve_version_by_revision(
    state: &AppState,
    tenant_key: &str,
    dataset_id: &str,
    revision: &str,
) -> Result<adapteros_db::training_datasets::TrainingDatasetVersion, ApiError> {
    if revision.eq_ignore_ascii_case("latest") {
        let versions = state
            .db
            .list_dataset_versions_routed(tenant_key, dataset_id)
            .await
            .map_err(|e| ApiError::db_error(format!("Failed to load latest version: {}", e)))?;
        return versions
            .into_iter()
            .next()
            .ok_or_else(|| ApiError::not_found("Dataset version"));
    }

    if let Ok(version_number) = revision.parse::<i64>() {
        let versions = state
            .db
            .list_dataset_versions_routed(tenant_key, dataset_id)
            .await
            .map_err(|e| ApiError::db_error(format!("Failed to list versions: {}", e)))?;
        return versions
            .into_iter()
            .find(|v| v.version_number == version_number)
            .ok_or_else(|| ApiError::not_found("Dataset version"));
    }

    let version = state
        .db
        .get_training_dataset_version_routed(tenant_key, revision)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to load version: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Dataset version"))?;
    if version.dataset_id != dataset_id {
        return Err(ApiError::not_found("Dataset version"));
    }
    Ok(version)
}

fn provenance_impact(previous: Option<&CanonicalRow>, current: Option<&CanonicalRow>) -> String {
    match (previous, current) {
        (None, Some(row)) => {
            if row
                .metadata
                .get("provenance_invalidated")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                "invalidated".to_string()
            } else {
                "new".to_string()
            }
        }
        (Some(_), None) => "removed".to_string(),
        (Some(old), Some(new)) => {
            let old_invalidated = old
                .metadata
                .get("provenance_invalidated")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let new_invalidated = new
                .metadata
                .get("provenance_invalidated")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if old_invalidated != new_invalidated {
                if new_invalidated {
                    "invalidated".to_string()
                } else {
                    "restored".to_string()
                }
            } else if extract_source_span(&old.metadata) != extract_source_span(&new.metadata) {
                "source_span_changed".to_string()
            } else if new_invalidated {
                "invalidated".to_string()
            } else {
                "preserved".to_string()
            }
        }
        (None, None) => "preserved".to_string(),
    }
}

fn compare_rows(
    base_dataset_version_id: &str,
    compare_dataset_version_id: &str,
    base_rows: &[CanonicalRow],
    compare_rows: &[CanonicalRow],
) -> DatasetVersionCompareSummary {
    let base_map: HashMap<&str, &CanonicalRow> =
        base_rows.iter().map(|row| (row.row_id.as_str(), row)).collect();
    let compare_map: HashMap<&str, &CanonicalRow> =
        compare_rows.iter().map(|row| (row.row_id.as_str(), row)).collect();
    let mut row_ids: Vec<&str> = base_map
        .keys()
        .chain(compare_map.keys())
        .copied()
        .collect::<HashSet<&str>>()
        .into_iter()
        .collect();
    row_ids.sort_unstable();

    let mut changed = Vec::new();
    for row_id in row_ids {
        let before = base_map.get(row_id).copied();
        let after = compare_map.get(row_id).copied();

        let (change_type, changed_fields) = match (before, after) {
            (None, Some(_)) => ("added".to_string(), Vec::new()),
            (Some(_), None) => ("removed".to_string(), Vec::new()),
            (Some(old), Some(new)) => {
                let mut fields = Vec::new();
                if old.prompt != new.prompt {
                    fields.push("prompt".to_string());
                }
                if old.response != new.response {
                    fields.push("response".to_string());
                }
                if (old.weight - new.weight).abs() > f32::EPSILON {
                    fields.push("weight".to_string());
                }
                if old.split != new.split {
                    fields.push("split".to_string());
                }
                if fields.is_empty() {
                    continue;
                }
                ("changed".to_string(), fields)
            }
            (None, None) => continue,
        };

        changed.push(DatasetVersionRowChange {
            row_id: row_id.to_string(),
            change_type,
            changed_fields,
            provenance_impact: provenance_impact(before, after),
        });
    }

    let total_changed_rows = changed.len();
    if changed.len() > MAX_CHANGED_ROWS_RETURNED {
        changed.truncate(MAX_CHANGED_ROWS_RETURNED);
    }

    DatasetVersionCompareSummary {
        base_dataset_version_id: base_dataset_version_id.to_string(),
        compare_dataset_version_id: compare_dataset_version_id.to_string(),
        total_changed_rows,
        changed_rows: changed,
    }
}

fn repo_slug_from_dataset(
    dataset: &adapteros_db::training_datasets::TrainingDataset,
) -> Option<String> {
    dataset.repo_slug.clone().or_else(|| {
        dataset
            .metadata_json
            .as_deref()
            .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
            .and_then(|val| {
                val.get("repo_slug")
                    .and_then(|v| v.as_str())
                    .map(|slug| slug.to_string())
            })
    })
}

async fn resolve_trust_state(
    db: &adapteros_db::Db,
    version: &adapteros_db::training_datasets::TrainingDatasetVersion,
) -> Result<String, ApiError> {
    if db.storage_mode().read_from_sql() {
        match db.get_effective_trust_state(&version.id).await {
            Ok(Some(state)) => Ok(state),
            Ok(None) => Ok(version.trust_state.clone()),
            Err(e) => {
                tracing::warn!(
                    version_id = %version.id,
                    error = %e,
                    "Failed to resolve effective trust state; using stored trust_state"
                );
                Ok(version.trust_state.clone())
            }
        }
    } else {
        Ok(version.trust_state.clone())
    }
}

/// Response for listing versions by codebase
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct CodebaseVersionsResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    /// The canonical codebase identifier (normalized repo identifier or source location)
    pub codebase_id: String,
    /// Dataset ID if a codebase dataset exists
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_id: Option<String>,
    /// Repository slug for identifying the source repository (e.g., "org/repo-name")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_slug: Option<String>,
    /// List of versions for this codebase
    pub versions: Vec<DatasetVersionSummary>,
    /// Total count of versions (for pagination)
    pub total_count: i64,
}

/// Get a specific dataset version by ID or revision number.
///
/// The `revision` parameter can be:
/// - A version ID (UUID string)
/// - A version number (integer, e.g., "1", "2", "latest")
/// - "latest" to get the most recent version
#[utoipa::path(
    get,
    path = "/v1/datasets/{dataset_id}/versions/{revision}",
    params(
        ("dataset_id" = String, Path, description = "Dataset ID"),
        ("revision" = String, Path, description = "Version ID, version number, or 'latest'"),
        DatasetVersionDetailQuery
    ),
    responses(
        (status = 200, description = "Dataset version details", body = DatasetVersionDetailResponse),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Dataset or version not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn get_dataset_version(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((dataset_id, revision)): Path<(String, String)>,
    Query(query): Query<DatasetVersionDetailQuery>,
) -> Result<impl IntoResponse, ApiError> {
    require_permission(&claims, Permission::DatasetView)?;
    let dataset_id = crate::id_resolver::resolve_any_id(&state.db, &dataset_id).await?;
    let revision = crate::id_resolver::resolve_any_id(&state.db, &revision).await?;

    // Ensure dataset exists and enforce tenant isolation
    let dataset = state
        .db
        .get_training_dataset_routed(&claims.tenant_id, &dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to load dataset: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Dataset"))?;

    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        return Err(ApiError::forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    let tenant_key = dataset.tenant_id.as_deref().unwrap_or("default");
    let version = resolve_version_by_revision(&state, tenant_key, &dataset_id, &revision).await?;
    let tenant = dataset
        .tenant_id
        .as_deref()
        .unwrap_or(claims.tenant_id.as_str())
        .to_string();
    let domain = DatasetDomainService::new(Arc::new(state.clone()));

    let mut rows = domain
        .stream_rows(
            &version.id,
            &tenant,
            SamplingConfig {
                split: None,
                shuffle_seed: None,
            },
        )
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    if rows.len() > DEFAULT_VERSION_DETAIL_ROW_LIMIT {
        rows.truncate(DEFAULT_VERSION_DETAIL_ROW_LIMIT);
    }
    let row_count = rows.len();

    // Parse validation errors if present
    let validation_errors = version
        .validation_errors_json
        .as_ref()
        .and_then(|json| serde_json::from_str::<Vec<String>>(json).ok());

    let trust_state = resolve_trust_state(&state.db, &version).await?;

    let regenerate = query.regenerate_evaluation.unwrap_or(false);
    let mut evaluation = if regenerate {
        None
    } else {
        load_latest_dataset_evaluation(&state, &dataset_id, &version.id).await?
    };

    if evaluation.is_none() {
        evaluation = Some(
            persist_dataset_evaluation(
                &state,
                &dataset_id,
                &version.id,
                &rows,
                Some(&claims.sub),
            )
            .await?,
        );
    }

    let compare = if let Some(compare_to_raw) = query.compare_to.as_deref() {
        let compare_revision = crate::id_resolver::resolve_any_id(&state.db, compare_to_raw).await?;
        let compare_version =
            resolve_version_by_revision(&state, tenant_key, &dataset_id, &compare_revision).await?;
        let mut target_rows = domain
            .stream_rows(
                &compare_version.id,
                &tenant,
                SamplingConfig {
                    split: None,
                    shuffle_seed: None,
                },
            )
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;
        if target_rows.len() > DEFAULT_VERSION_DETAIL_ROW_LIMIT {
            target_rows.truncate(DEFAULT_VERSION_DETAIL_ROW_LIMIT);
        }
        Some(compare_rows(
            &version.id,
            &compare_version.id,
            &rows,
            &target_rows,
        ))
    } else {
        None
    };

    Ok(Json(DatasetVersionDetailResponse {
        schema_version: schema_version(),
        dataset_id: version.dataset_id,
        dataset_version_id: version.id,
        version_number: version.version_number,
        version_label: version.version_label,
        hash_b3: version.hash_b3,
        storage_path: version.storage_path,
        manifest_path: version.manifest_path,
        validation_status: version.validation_status,
        validation_errors,
        pii_status: version.pii_status,
        toxicity_status: version.toxicity_status,
        leak_status: version.leak_status,
        anomaly_status: version.anomaly_status,
        overall_safety_status: version.overall_safety_status,
        trust_state: trust_state.clone(),
        overall_trust_status: trust_state,
        sensitivity: version.sensitivity,
        created_at: version.created_at,
        created_by: version.created_by,
        locked_at: version.locked_at,
        row_count: Some(row_count),
        evaluation,
        compare,
    }))
}

/// List dataset versions by codebase source location.
///
/// This endpoint finds the dataset associated with a codebase (by source_location)
/// and returns all its versions. Useful for codebase adapter workflows.
#[utoipa::path(
    get,
    path = "/v1/datasets/by-codebase/{codebase_id}/versions",
    params(
        ("codebase_id" = String, Path, description = "Codebase identifier (URL-encoded repo identifier or source location, e.g., repo path)"),
        ListVersionsQuery
    ),
    responses(
        (status = 200, description = "Codebase dataset versions", body = CodebaseVersionsResponse),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "No dataset found for codebase"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn list_versions_by_codebase(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(codebase_id): Path<String>,
    Query(params): Query<ListVersionsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    require_permission(&claims, Permission::DatasetView)?;

    let limit = params.limit.unwrap_or(50).min(100);
    let offset = params.offset.unwrap_or(0);

    // URL-decode the codebase_id (it may contain slashes, etc.)
    let source_location = urlencoding::decode(&codebase_id)
        .map_err(|e| ApiError::bad_request(format!("Invalid codebase_id encoding: {}", e)))?
        .into_owned();
    let normalized_codebase_id = normalize_repo_id(&source_location);
    let tenant_scopes: Vec<Option<&str>> = if claims.role == "admin" {
        vec![None]
    } else {
        let mut scopes = Vec::with_capacity(1 + claims.admin_tenants.len());
        scopes.push(Some(claims.tenant_id.as_str()));
        for tenant in &claims.admin_tenants {
            scopes.push(Some(tenant.as_str()));
        }
        scopes
    };
    let mut datasets = Vec::new();
    let mut seen_ids = HashSet::new();
    let mut push_dataset = |dataset: adapteros_db::training_datasets::TrainingDataset| {
        if seen_ids.insert(dataset.id.clone()) {
            datasets.push(dataset);
        }
    };

    for tenant_scope in &tenant_scopes {
        for dataset in state
            .db
            .list_codebase_datasets_by_repo(&source_location, *tenant_scope)
            .await
            .map_err(|e| ApiError::db_error(format!("Failed to list codebase datasets: {}", e)))?
        {
            push_dataset(dataset);
        }
        if normalized_codebase_id != source_location {
            for dataset in state
                .db
                .list_codebase_datasets_by_repo(&normalized_codebase_id, *tenant_scope)
                .await
                .map_err(|e| {
                    ApiError::db_error(format!("Failed to list codebase datasets: {}", e))
                })?
            {
                push_dataset(dataset);
            }
        }
        for dataset in state
            .db
            .list_codebase_datasets_by_repo_identifier(&normalized_codebase_id, *tenant_scope)
            .await
            .map_err(|e| ApiError::db_error(format!("Failed to list codebase datasets: {}", e)))?
        {
            push_dataset(dataset);
        }
    }

    let mut accessible = Vec::new();
    for dataset in datasets {
        if let Some(ref dataset_tenant_id) = dataset.tenant_id {
            if validate_tenant_isolation(&claims, dataset_tenant_id).is_ok() {
                accessible.push(dataset);
            }
        } else if claims.role == "admin" {
            accessible.push(dataset);
        }
    }

    if accessible.is_empty() {
        return Ok(Json(CodebaseVersionsResponse {
            schema_version: schema_version(),
            codebase_id: normalized_codebase_id,
            dataset_id: None,
            repo_slug: None,
            versions: Vec::new(),
            total_count: 0,
        }));
    }

    accessible.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| a.id.cmp(&b.id))
    });

    let dataset_id = accessible.first().map(|dataset| dataset.id.clone());
    let repo_slug = accessible.iter().filter_map(repo_slug_from_dataset).next();

    let mut summaries = Vec::new();
    for dataset in &accessible {
        let tenant_key = dataset.tenant_id.as_deref().unwrap_or("default");
        let versions = state
            .db
            .list_dataset_versions_routed(tenant_key, &dataset.id)
            .await
            .map_err(|e| ApiError::db_error(format!("Failed to list dataset versions: {}", e)))?;
        let dataset_repo_slug = repo_slug_from_dataset(dataset);

        for version in versions {
            let trust_state = resolve_trust_state(&state.db, &version).await?;
            if let Some(ref filter) = params.trust_state {
                if trust_state.to_lowercase() != filter.to_lowercase() {
                    continue;
                }
            }
            summaries.push(DatasetVersionSummary {
                dataset_version_id: version.id,
                version_number: version.version_number,
                version_label: version.version_label,
                hash_b3: Some(version.hash_b3),
                storage_path: Some(version.storage_path),
                trust_state: Some(trust_state),
                repo_slug: dataset_repo_slug.clone(),
                created_at: version.created_at,
            });
        }
    }

    summaries.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| b.dataset_version_id.cmp(&a.dataset_version_id))
    });

    let total_count = summaries.len() as i64;

    // Apply pagination
    let paginated: Vec<DatasetVersionSummary> = summaries
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .collect();

    Ok(Json(CodebaseVersionsResponse {
        schema_version: schema_version(),
        codebase_id: normalized_codebase_id,
        dataset_id,
        repo_slug,
        versions: paginated,
        total_count,
    }))
}
