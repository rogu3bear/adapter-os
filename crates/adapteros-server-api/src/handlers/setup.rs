//! Setup self-service handlers.

use crate::api_error::{ApiError, ApiResult};
use crate::auth::Claims;
use crate::middleware::require_any_role;
use crate::model_roots::resolve_model_allowed_roots;
use crate::state::AppState;
use adapteros_api_types::{
    schema_version, SetupDiscoverModelsResponse, SetupDiscoveredModel, SetupMigrateResponse,
    SetupSeedModelResult, SetupSeedModelStatus, SetupSeedModelsRequest, SetupSeedModelsResponse,
};
use adapteros_db::users::Role;
use adapteros_db::{SetupSeedOptions, SetupSeedStatus};
use adapteros_storage::secure_fs::path_policy::canonicalize_strict_in_allowed_roots;
use axum::{
    extract::{Extension, State},
    Json,
};
use chrono::Utc;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

async fn model_allowed_roots(state: &AppState) -> Result<Vec<PathBuf>, ApiError> {
    resolve_model_allowed_roots(Some(&state.db))
        .await
        .map_err(ApiError::internal)
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
        completed_at: Utc::now().to_rfc3339(),
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
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> ApiResult<SetupDiscoverModelsResponse> {
    require_any_role(&claims, &[Role::Admin]).map_err(|_| ApiError::forbidden("access denied"))?;

    let allowed_roots = model_allowed_roots(&state).await?;
    let canonical_roots: Vec<PathBuf> = allowed_roots
        .iter()
        .map(|root| canonicalize_model_path(root, &allowed_roots))
        .collect::<Result<Vec<_>, _>>()?;

    let registered_paths: HashSet<String> = if state.db.pool_opt().is_some() {
        let db_paths: Vec<String> =
            sqlx::query_scalar::<_, String>("SELECT model_path FROM models")
                .fetch_all(state.db.pool_result()?)
                .await
                .map_err(ApiError::db_error)?;
        db_paths
            .into_iter()
            .flat_map(|path| {
                let canonical = canonicalize_model_path(Path::new(&path), &allowed_roots)
                    .ok()
                    .map(|p| p.to_string_lossy().to_string());
                std::iter::once(path).chain(canonical)
            })
            .collect()
    } else {
        HashSet::new()
    };

    let mut models: Vec<SetupDiscoveredModel> = canonical_roots
        .iter()
        .flat_map(|root| adapteros_db::Db::setup_discover_models(root))
        .map(|model| {
            let path = model.path.to_string_lossy().to_string();
            SetupDiscoveredModel {
                name: model.name,
                path: path.clone(),
                format: model.format,
                backend: model.backend,
                already_registered: registered_paths.contains(&path),
            }
        })
        .collect();
    models.sort_by(|a, b| a.name.cmp(&b.name).then(a.path.cmp(&b.path)));
    models.dedup_by(|a, b| a.path == b.path);

    Ok(Json(SetupDiscoverModelsResponse {
        schema_version: schema_version(),
        root: canonical_roots
            .first()
            .map(|root| root.to_string_lossy().to_string())
            .unwrap_or_default(),
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

    let allowed_roots = model_allowed_roots(&state).await?;
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

    let seeded = results
        .iter()
        .filter(|item| item.status == SetupSeedModelStatus::Seeded)
        .cloned()
        .collect::<Vec<_>>();
    let skipped = results
        .iter()
        .filter(|item| item.status == SetupSeedModelStatus::Skipped)
        .cloned()
        .collect::<Vec<_>>();
    let failed = results
        .iter()
        .filter(|item| item.status == SetupSeedModelStatus::Failed)
        .cloned()
        .collect::<Vec<_>>();

    Ok(Json(SetupSeedModelsResponse {
        schema_version: schema_version(),
        total: summary.total,
        seeded_count: summary.seeded,
        skipped_count: summary.skipped,
        failed_count: summary.failed,
        seeded,
        skipped,
        failed,
        results,
    }))
}
