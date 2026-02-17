//! Setup self-service handlers.

use crate::api_error::{ApiError, ApiResult};
use crate::auth::Claims;
use crate::middleware::require_any_role;
use crate::state::AppState;
use adapteros_api_types::{
    schema_version, SetupDiscoverModelsResponse, SetupDiscoveredModel, SetupMigrateResponse,
    SetupSeedModelResult, SetupSeedModelStatus, SetupSeedModelsRequest, SetupSeedModelsResponse,
};
use adapteros_config::resolve_base_model_location;
use adapteros_db::users::Role;
use adapteros_db::{SetupSeedOptions, SetupSeedStatus};
use adapteros_storage::secure_fs::path_policy::canonicalize_strict_in_allowed_roots;
use axum::{
    extract::{Extension, State},
    Json,
};
use std::path::{Path, PathBuf};

async fn model_allowed_roots() -> Result<Vec<PathBuf>, ApiError> {
    let location = resolve_base_model_location(None, None, false)
        .map_err(|e| ApiError::internal(e.to_string()))?;

    if !location.cache_root.exists() {
        tokio::fs::create_dir_all(&location.cache_root)
            .await
            .map_err(|e| {
                ApiError::internal(format!(
                    "Failed to create model cache root {}: {}",
                    location.cache_root.display(),
                    e
                ))
            })?;
    }

    Ok(vec![location.cache_root])
}

fn canonicalize_model_path(path: &Path, allowed_roots: &[PathBuf]) -> Result<PathBuf, ApiError> {
    canonicalize_strict_in_allowed_roots(path, allowed_roots).map_err(|e| match e {
        adapteros_core::AosError::NotFound(_) => {
            ApiError::bad_request("model path does not exist").with_details(e.to_string())
        }
        _ => ApiError::forbidden("model path not permitted").with_details(e.to_string()),
    })
}

#[utoipa::path(
    post,
    path = "/v1/setup/migrate",
    responses(
        (status = 200, description = "Migrations completed", body = SetupMigrateResponse),
        (status = 403, description = "Forbidden - Admin role required"),
        (status = 500, description = "Migration failed")
    ),
    tag = "setup"
)]
pub async fn setup_migrate(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> ApiResult<SetupMigrateResponse> {
    require_any_role(&claims, &[Role::Admin]).map_err(|_| ApiError::forbidden("access denied"))?;

    state
        .db
        .setup_run_migrations()
        .await
        .map_err(ApiError::db_error)?;

    Ok(Json(SetupMigrateResponse {
        schema_version: schema_version(),
        status: "ok".to_string(),
        message: "Migrations completed successfully".to_string(),
    }))
}

#[utoipa::path(
    get,
    path = "/v1/setup/models/discover",
    responses(
        (status = 200, description = "Discovered setup models", body = SetupDiscoverModelsResponse),
        (status = 403, description = "Forbidden - Admin role required"),
        (status = 500, description = "Discovery failed")
    ),
    tag = "setup"
)]
pub async fn setup_discover_models(
    Extension(claims): Extension<Claims>,
) -> ApiResult<SetupDiscoverModelsResponse> {
    require_any_role(&claims, &[Role::Admin]).map_err(|_| ApiError::forbidden("access denied"))?;

    let allowed_roots = model_allowed_roots().await?;
    let root = allowed_roots
        .first()
        .cloned()
        .ok_or_else(|| ApiError::internal("no allowed model roots configured"))?;

    let canonical_root = canonicalize_model_path(&root, &allowed_roots)?;
    let models: Vec<SetupDiscoveredModel> =
        adapteros_db::Db::setup_discover_models(&canonical_root)
            .into_iter()
            .map(|model| SetupDiscoveredModel {
                name: model.name,
                model_path: model.path.to_string_lossy().to_string(),
                format: model.format,
                backend: model.backend,
            })
            .collect();

    Ok(Json(SetupDiscoverModelsResponse {
        schema_version: schema_version(),
        root: canonical_root.to_string_lossy().to_string(),
        total: models.len(),
        models,
    }))
}

#[utoipa::path(
    post,
    path = "/v1/setup/models/seed",
    request_body = SetupSeedModelsRequest,
    responses(
        (status = 200, description = "Seed results", body = SetupSeedModelsResponse),
        (status = 400, description = "Invalid request"),
        (status = 403, description = "Forbidden - Admin role required"),
        (status = 500, description = "Seeding failed")
    ),
    tag = "setup"
)]
pub async fn setup_seed_models(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<SetupSeedModelsRequest>,
) -> ApiResult<SetupSeedModelsResponse> {
    require_any_role(&claims, &[Role::Admin]).map_err(|_| ApiError::forbidden("access denied"))?;

    if req.model_paths.is_empty() {
        return Err(ApiError::bad_request("model_paths must not be empty"));
    }

    let allowed_roots = model_allowed_roots().await?;
    let canonical_paths: Vec<PathBuf> = req
        .model_paths
        .iter()
        .map(|path| canonicalize_model_path(Path::new(path), &allowed_roots))
        .collect::<Result<Vec<_>, _>>()?;

    let summary = state
        .db
        .setup_seed_models(
            &canonical_paths,
            SetupSeedOptions {
                force: req.force,
                tenant_id: "system",
                imported_by: &claims.sub,
            },
        )
        .await
        .map_err(ApiError::db_error)?;

    let results: Vec<SetupSeedModelResult> = summary
        .items
        .into_iter()
        .map(|item| SetupSeedModelResult {
            name: item.name,
            model_path: item.path.to_string_lossy().to_string(),
            status: match item.status {
                SetupSeedStatus::Seeded => SetupSeedModelStatus::Seeded,
                SetupSeedStatus::Skipped => SetupSeedModelStatus::Skipped,
                SetupSeedStatus::Failed => SetupSeedModelStatus::Failed,
            },
            model_id: item.model_id,
            message: item.message,
        })
        .collect();

    Ok(Json(SetupSeedModelsResponse {
        schema_version: schema_version(),
        total: summary.total,
        seeded: summary.seeded,
        skipped: summary.skipped,
        failed: summary.failed,
        results,
    }))
}
