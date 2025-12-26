//! Adapter version management handlers
//!
//! Handlers for managing adapter versions, including creation, promotion, and rollback.

use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::types::*;
use crate::validation::*;
use adapteros_core::AosError;
use adapteros_db::{
    AdapterVersionRuntimeState, CreateDraftVersionParams as CreateDraftAdapterVersionParams,
};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use tracing::info;
use utoipa::IntoParams;

// Import helper functions from adapter_utils
use super::adapter_utils::{
    compute_serveable_state, lora_scope_from_provenance, lora_tier_from_provenance,
    manifest_lineage_from_aos,
};

/// Query parameters for listing adapter versions
#[derive(Debug, Deserialize, IntoParams)]
pub struct ListAdapterVersionsParams {
    pub branch: Option<String>,
    pub state: Option<String>,
}

#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/adapter-repositories/{repo_id}/versions",
    params(
        ("repo_id" = String, Path, description = "Repository ID"),
        ListAdapterVersionsParams
    ),
    responses(
        (status = 200, description = "List of adapter versions", body = Vec<AdapterVersionResponse>),
        (status = 404, description = "Repository not found", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse)
    )
)]
pub async fn list_adapter_versions(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(repo_id): Path<String>,
    Query(params): Query<ListAdapterVersionsParams>,
) -> Result<Json<Vec<AdapterVersionResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterList)?;

    let repo = state
        .db
        .get_adapter_repository(&claims.tenant_id, &repo_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to fetch repository")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    if repo.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("repository not found").with_code("NOT_FOUND")),
        ));
    }

    let state_filter: Option<Vec<String>> = params.state.as_ref().map(|s| vec![s.clone()]);
    let state_refs: Option<Vec<&str>> = state_filter
        .as_ref()
        .map(|vals| vals.iter().map(|s| s.as_str()).collect());

    let versions = state
        .db
        .list_adapter_versions_for_repo(
            &claims.tenant_id,
            &repo_id,
            params.branch.as_deref(),
            state_refs.as_deref(),
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to list adapter versions")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let mut responses = Vec::new();
    for version in versions {
        let runtime_state = state
            .db
            .get_adapter_version_runtime_state(&version.id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to load runtime state")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?
            .map(|s: AdapterVersionRuntimeState| s.runtime_state);

        let db_lineage = state
            .db
            .list_dataset_versions_with_trust_for_adapter_version(&version.id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to load dataset versions")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;

        let manifest_lineage = manifest_lineage_from_aos(version.aos_path.as_deref()).await;
        let manifest_ids = manifest_lineage
            .as_ref()
            .and_then(|(ids, _, _)| ids.clone());
        let manifest_training_backend = manifest_lineage.as_ref().and_then(|(_, tb, _)| tb.clone());
        let manifest_scope_path = manifest_lineage
            .as_ref()
            .and_then(|(_, _, scope)| scope.clone());

        let db_ids: Vec<String> = db_lineage.iter().map(|(id, _)| id.clone()).collect();
        if let Some(ref manifest_ids) = manifest_ids {
            if !db_ids.is_empty() && manifest_ids != &db_ids {
                tracing::warn!(
                    version_id = %version.id,
                    manifest_ids = ?manifest_ids,
                    db_dataset_version_ids = ?db_ids,
                    "Manifest lineage differs from DB mirror; preferring manifest"
                );
            }
        }

        let trust_map: HashMap<String, Option<String>> = db_lineage.into_iter().collect();

        let dataset_version_ids = manifest_ids.filter(|ids| !ids.is_empty()).or_else(|| {
            if db_ids.is_empty() {
                None
            } else {
                Some(db_ids.clone())
            }
        });

        let dataset_version_trust = dataset_version_ids.as_ref().map(|ids| {
            ids.iter()
                .map(|id| DatasetVersionTrustSnapshot {
                    dataset_version_id: id.clone(),
                    trust_at_training_time: trust_map.get(id).cloned().flatten(),
                })
                .collect()
        });

        let (serveable, serveable_reason) =
            compute_serveable_state(&version.release_state, &version.adapter_trust_state);

        responses.push(AdapterVersionResponse {
            id: version.id,
            repo_id: version.repo_id,
            tenant_id: version.tenant_id,
            version: version.version,
            branch: version.branch,
            aos_path: version.aos_path,
            aos_hash: version.aos_hash,
            manifest_schema_version: version.manifest_schema_version,
            parent_version_id: version.parent_version_id,
            code_commit_sha: version.code_commit_sha,
            data_spec_hash: version.data_spec_hash,
            training_backend: manifest_training_backend
                .clone()
                .or_else(|| version.training_backend.clone()),
            coreml_used: Some(version.coreml_used),
            coreml_device_type: version.coreml_device_type,
            dataset_version_ids,
            scope_path: manifest_scope_path,
            dataset_version_trust,
            adapter_trust_state: version.adapter_trust_state,
            release_state: version.release_state,
            metrics_snapshot_id: version.metrics_snapshot_id,
            evaluation_summary: version.evaluation_summary,
            created_at: version.created_at,
            runtime_state,
            serveable,
            serveable_reason,
        });
    }

    Ok(Json(responses))
}

#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/adapter-versions/draft",
    request_body = CreateDraftVersionRequest,
    responses(
        (status = 201, description = "Draft version created", body = CreateDraftVersionResponse),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 404, description = "Repository not found", body = ErrorResponse)
    )
)]
pub async fn create_draft_version(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateDraftVersionRequest>,
) -> Result<(StatusCode, Json<CreateDraftVersionResponse>), (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterRegister)?;

    // Ensure repository is owned by tenant
    let repo = state
        .db
        .get_adapter_repository(&claims.tenant_id, &req.repo_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to fetch repository")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    if repo.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("repository not found").with_code("NOT_FOUND")),
        ));
    }

    let version_id = state
        .db
        .create_adapter_draft_version(CreateDraftAdapterVersionParams {
            repo_id: &req.repo_id,
            tenant_id: &claims.tenant_id,
            branch: &req.branch,
            branch_classification: "protected",
            parent_version_id: req.parent_version_id.as_deref(),
            code_commit_sha: req.code_commit_sha.as_deref(),
            data_spec_hash: req.data_spec_hash.as_deref(),
            training_backend: None,
            dataset_version_ids: None,
            actor: Some(&claims.sub),
            reason: None,
        })
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("failed to create draft version")
                        .with_code("VALIDATION_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok((
        StatusCode::CREATED,
        Json(CreateDraftVersionResponse { version_id }),
    ))
}

#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/adapter-versions/{version_id}",
    params(
        ("version_id" = String, Path, description = "Version ID")
    ),
    responses(
        (status = 200, description = "Version", body = AdapterVersionResponse),
        (status = 404, description = "Version not found", body = ErrorResponse)
    )
)]
pub async fn get_adapter_version(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(version_id): Path<String>,
) -> Result<Json<AdapterVersionResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterList)?;

    let version = state
        .db
        .get_adapter_version(&claims.tenant_id, &version_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to load adapter version")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let version = match version {
        Some(v) => v,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("version not found").with_code("NOT_FOUND")),
            ))
        }
    };

    let db_lineage = state
        .db
        .list_dataset_versions_with_trust_for_adapter_version(&version.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to load dataset versions")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let manifest_lineage = manifest_lineage_from_aos(version.aos_path.as_deref()).await;
    let manifest_ids = manifest_lineage
        .as_ref()
        .and_then(|(ids, _, _)| ids.clone());
    let manifest_training_backend = manifest_lineage.as_ref().and_then(|(_, tb, _)| tb.clone());
    let manifest_scope_path = manifest_lineage
        .as_ref()
        .and_then(|(_, _, scope)| scope.clone());

    let db_ids: Vec<String> = db_lineage.iter().map(|(id, _)| id.clone()).collect();
    if let Some(ref manifest_ids) = manifest_ids {
        if !db_ids.is_empty() && manifest_ids != &db_ids {
            tracing::warn!(
                version_id = %version.id,
                manifest_ids = ?manifest_ids,
                db_dataset_version_ids = ?db_ids,
                "Manifest lineage differs from DB mirror; preferring manifest"
            );
        }
    }

    let trust_map: HashMap<String, Option<String>> = db_lineage.into_iter().collect();

    let dataset_version_ids = manifest_ids.filter(|ids| !ids.is_empty()).or_else(|| {
        if db_ids.is_empty() {
            None
        } else {
            Some(db_ids.clone())
        }
    });

    let dataset_version_trust = dataset_version_ids.as_ref().map(|ids| {
        ids.iter()
            .map(|id| DatasetVersionTrustSnapshot {
                dataset_version_id: id.clone(),
                trust_at_training_time: trust_map.get(id).cloned().flatten(),
            })
            .collect()
    });

    let (serveable, serveable_reason) =
        compute_serveable_state(&version.release_state, &version.adapter_trust_state);

    Ok(Json(AdapterVersionResponse {
        id: version.id,
        repo_id: version.repo_id,
        tenant_id: version.tenant_id,
        version: version.version,
        branch: version.branch,
        aos_path: version.aos_path,
        aos_hash: version.aos_hash,
        manifest_schema_version: version.manifest_schema_version,
        parent_version_id: version.parent_version_id,
        code_commit_sha: version.code_commit_sha,
        data_spec_hash: version.data_spec_hash,
        training_backend: manifest_training_backend
            .clone()
            .or_else(|| version.training_backend.clone()),
        coreml_used: Some(version.coreml_used),
        coreml_device_type: version.coreml_device_type,
        dataset_version_ids,
        scope_path: manifest_scope_path,
        dataset_version_trust,
        adapter_trust_state: version.adapter_trust_state,
        release_state: version.release_state,
        metrics_snapshot_id: version.metrics_snapshot_id,
        evaluation_summary: version.evaluation_summary,
        created_at: version.created_at,
        runtime_state: None,
        serveable,
        serveable_reason,
    }))
}

#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/adapter-versions/{version_id}/promote",
    request_body = PromoteVersionRequest,
    responses(
        (status = 204, description = "Version promoted"),
        (status = 404, description = "Version not found", body = ErrorResponse)
    )
)]
pub async fn promote_adapter_version_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(version_id): Path<String>,
    Json(req): Json<PromoteVersionRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterRegister)?;

    // Fetch version to validate tenant/repo
    let version = state
        .db
        .get_adapter_version(&claims.tenant_id, &version_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to load adapter version")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let version = match version {
        Some(v) if v.repo_id == req.repo_id => v,
        Some(_) => {
            return Err((
                StatusCode::FORBIDDEN,
                Json(
                    ErrorResponse::new("version does not belong to repository")
                        .with_code("TENANT_ISOLATION_ERROR"),
                ),
            ))
        }
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("version not found").with_code("NOT_FOUND")),
            ))
        }
    };

    let (serveable, reason) =
        compute_serveable_state(&version.release_state, &version.adapter_trust_state);
    if !serveable {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("version is not serveable")
                    .with_code("NOT_SERVEABLE")
                    .with_string_details(reason.unwrap_or_else(|| "not serveable".to_string())),
            ),
        ));
    }

    state
        .db
        .promote_adapter_version(
            &claims.tenant_id,
            &version.repo_id,
            &version_id,
            Some(&claims.sub),
            None,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("failed to promote version")
                        .with_code("VALIDATION_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    info!(
        version_id = %version_id,
        repo_id = %version.repo_id,
        branch = %version.branch,
        coreml_used = %version.coreml_used,
        "Adapter version promoted"
    );

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/adapter-repositories/{repo_id}/versions/rollback",
    request_body = RollbackVersionRequest,
    params(
        ("repo_id" = String, Path, description = "Repository ID")
    ),
    responses(
        (status = 204, description = "Rollback succeeded"),
        (status = 404, description = "Repository or version not found", body = ErrorResponse)
    )
)]
pub async fn rollback_adapter_version_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(repo_id): Path<String>,
    Json(req): Json<RollbackVersionRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterRegister)?;

    // Ensure repo exists
    let repo_exists = state
        .db
        .get_adapter_repository(&claims.tenant_id, &repo_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to fetch repository")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    if repo_exists.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("repository not found").with_code("NOT_FOUND")),
        ));
    }

    state
        .db
        .rollback_adapter_branch(
            &claims.tenant_id,
            &repo_id,
            &req.branch,
            &req.target_version_id,
            Some(&claims.sub),
            None,
        )
        .await
        .map_err(|e| {
            let status = if matches!(e, AosError::NotFound(_)) {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::BAD_REQUEST
            };
            (
                status,
                Json(
                    ErrorResponse::new("failed to rollback branch")
                        .with_code("VALIDATION_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/adapter-versions/{version_id}/tag",
    request_body = TagVersionRequest,
    params(
        ("version_id" = String, Path, description = "Version ID")
    ),
    responses(
        (status = 204, description = "Tag upserted"),
        (status = 404, description = "Version not found", body = ErrorResponse)
    )
)]
pub async fn tag_adapter_version_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(version_id): Path<String>,
    Json(req): Json<TagVersionRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterRegister)?;

    state
        .db
        .upsert_adapter_version_tag(&claims.tenant_id, &version_id, &req.tag_name)
        .await
        .map_err(|e| {
            let status = if matches!(e, AosError::NotFound(_)) {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::BAD_REQUEST
            };
            (
                status,
                Json(
                    ErrorResponse::new("failed to tag version")
                        .with_code("VALIDATION_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/adapter-repositories/{repo_id}/resolve-version",
    request_body = ResolveVersionRequest,
    params(
        ("repo_id" = String, Path, description = "Repository ID")
    ),
    responses(
        (status = 200, description = "Resolved version", body = ResolveVersionResponse),
        (status = 404, description = "No matching version", body = ErrorResponse)
    )
)]
pub async fn resolve_adapter_version_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(repo_id): Path<String>,
    Json(req): Json<ResolveVersionRequest>,
) -> Result<Json<ResolveVersionResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterList)?;

    let resolved = state
        .db
        .resolve_adapter_version(&claims.tenant_id, &repo_id, Some(req.selector.as_str()))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to resolve version")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    if resolved.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("version not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(req.selector),
            ),
        ));
    }

    Ok(Json(ResolveVersionResponse {
        version_id: resolved.map(|v| v.id),
    }))
}
