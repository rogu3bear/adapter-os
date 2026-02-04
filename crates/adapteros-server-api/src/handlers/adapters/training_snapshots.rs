// Adapter Training Snapshot Handlers
//
// This module provides REST API endpoints for:
// - Getting training snapshots (provenance) for adapters
// - Exporting complete training provenance

use crate::adapter_helpers::fetch_adapter_for_tenant;
use crate::api_error::{ApiError, ApiResult};
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::types::*;
use adapteros_db::AdapterTrainingSnapshot;
use axum::{
    extract::{Path, State},
    response::Json,
    Extension,
};
use tracing::info;

// ============================================================================
// Handlers
// ============================================================================

/// Get training snapshot (provenance) for an adapter
///
/// Retrieves the training snapshot showing exactly which documents and
/// chunking configuration were used to train the adapter.
///
/// GET /v1/adapters/:adapter_id/training-snapshot
#[utoipa::path(
    get,
    path = "/v1/adapters/{adapter_id}/training-snapshot",
    tag = "adapters",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Training snapshot retrieved", body = AdapterTrainingSnapshot),
        (status = 404, description = "Snapshot not found", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    )
)]
pub async fn get_adapter_training_snapshot(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> ApiResult<AdapterTrainingSnapshot> {
    // Permission check
    require_permission(&claims, Permission::AdapterView)?;
    let adapter_id = crate::id_resolver::resolve_any_id(&state.db, &adapter_id).await?;

    // CRITICAL: Fetch adapter with tenant isolation validation to prevent cross-tenant access
    let _adapter = fetch_adapter_for_tenant(&state.db, &claims, &adapter_id).await?;

    // Get training snapshot from database
    let snapshot = state
        .db
        .get_adapter_training_snapshot(&adapter_id)
        .await
        .map_err(ApiError::db_error)?
        .ok_or_else(|| ApiError::not_found_msg("Training snapshot not found for this adapter"))?;

    info!(
        adapter_id = %adapter_id,
        training_job_id = %snapshot.training_job_id,
        actor = %claims.sub,
        "Retrieved training snapshot"
    );

    Ok(Json(snapshot))
}

/// Export complete training provenance for an adapter
///
/// Returns full provenance data including:
/// - Adapter metadata (id, name, version, base_model)
/// - Training jobs that produced this adapter
/// - Datasets used for training
/// - Documents with their content hashes
/// - Configuration versions (chunking, training)
/// - Export timestamp and integrity hash
///
/// GET /v1/adapters/:adapter_id/training-export
#[utoipa::path(
    get,
    path = "/v1/adapters/{adapter_id}/training-export",
    tag = "adapters",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Training provenance export", body = TrainingProvenanceExportResponse),
        (status = 404, description = "Adapter or snapshot not found", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    )
)]
pub async fn export_training_provenance(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> ApiResult<TrainingProvenanceExportResponse> {
    use blake3::Hasher;

    // Permission check
    require_permission(&claims, Permission::AdapterView)?;
    let adapter_id = crate::id_resolver::resolve_any_id(&state.db, &adapter_id).await?;

    // Get adapter details with tenant isolation validation
    let adapter = fetch_adapter_for_tenant(&state.db, &claims, &adapter_id).await?;

    // Get training snapshot
    let snapshot = state
        .db
        .get_adapter_training_snapshot(&adapter_id)
        .await
        .map_err(ApiError::db_error)?;

    // Build export data
    let mut training_jobs = Vec::new();
    let mut datasets = Vec::new();
    let mut documents = Vec::new();
    let mut chunking_config: Option<serde_json::Value> = None;
    let mut training_config: Option<serde_json::Value> = None;

    // If we have a training snapshot, extract documents and job info
    if let Some(ref snapshot) = snapshot {
        // Get training job details
        if let Ok(Some(job)) = state.db.get_training_job(&snapshot.training_job_id).await {
            // SECURITY: Validate training job belongs to adapter's tenant
            // Skip export if tenant mismatch to prevent cross-tenant data leakage
            if job.tenant_id.as_ref() == Some(&adapter.tenant_id) {
                // Parse training config JSON
                let config_value: serde_json::Value =
                    serde_json::from_str(&job.training_config_json)
                        .unwrap_or(serde_json::json!({}));
                training_config = Some(config_value.clone());

                training_jobs.push(TrainingExportJob {
                    id: job.id.clone(),
                    config_hash: job.config_hash_b3.clone(),
                    training_config: config_value,
                    started_at: job.started_at.clone(),
                    completed_at: job.completed_at.clone(),
                    status: job.status.clone(),
                });

                // Get dataset if linked
                if let Some(ref dataset_id) = job.dataset_id {
                    if let Ok(Some(dataset)) = state.db.get_training_dataset(dataset_id).await {
                        // SECURITY: Validate dataset belongs to adapter's tenant
                        if dataset.tenant_id.as_ref() == Some(&adapter.tenant_id) {
                            datasets.push(TrainingExportDataset {
                                id: dataset.id,
                                name: dataset.name,
                                hash: dataset.hash_b3,
                                source_location: dataset.source_location,
                            });
                        }
                    }
                }
            }
        }

        // Parse documents from snapshot
        if let Ok(doc_refs) =
            serde_json::from_str::<Vec<serde_json::Value>>(&snapshot.documents_json)
        {
            for doc_ref in doc_refs {
                if let Some(doc_id) = doc_ref.get("doc_id").and_then(|v| v.as_str()) {
                    // Fetch full document info
                    if let Ok(Some(doc)) = state.db.get_document(&claims.tenant_id, doc_id).await {
                        documents.push(TrainingExportDocument {
                            id: doc.id,
                            name: doc.name,
                            hash: doc.content_hash,
                            page_count: doc.page_count,
                            created_at: doc.created_at,
                        });
                    }
                }
            }
        }

        // Parse chunking config from snapshot
        if let Ok(chunking) =
            serde_json::from_str::<serde_json::Value>(&snapshot.chunking_config_json)
        {
            chunking_config = Some(chunking);
        }
    }

    // Build adapter export data
    let adapter_export = TrainingExportAdapter {
        id: adapter.id.clone(),
        name: adapter.name.clone(),
        version: adapter.version.clone(),
        base_model: adapter.parent_id.clone(),
        rank: adapter.rank,
        alpha: adapter.alpha,
        created_at: adapter.created_at.clone(),
    };

    // Build config versions
    let config_versions = TrainingExportConfigVersions {
        chunking_config,
        training_config,
    };

    // Build pre-hash response for computing export hash
    let export_timestamp = chrono::Utc::now().to_rfc3339();
    let pre_hash_response = serde_json::json!({
        "schema_version": "v1",
        "adapter": adapter_export,
        "training_jobs": training_jobs,
        "datasets": datasets,
        "documents": documents,
        "config_versions": config_versions,
        "export_timestamp": export_timestamp,
    });

    // Compute BLAKE3 hash of the export
    let mut hasher = Hasher::new();
    hasher.update(pre_hash_response.to_string().as_bytes());
    let export_hash = hasher.finalize().to_hex().to_string();

    let response = TrainingProvenanceExportResponse {
        schema_version: "v1".to_string(),
        adapter: adapter_export,
        training_jobs,
        datasets,
        documents,
        config_versions,
        export_timestamp,
        export_hash,
    };

    info!(
        adapter_id = %adapter_id,
        documents_count = response.documents.len(),
        jobs_count = response.training_jobs.len(),
        actor = %claims.sub,
        "Exported training provenance"
    );

    Ok(Json(response))
}
