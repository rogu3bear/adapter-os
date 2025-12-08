use crate::auth::Claims;
use crate::error_helpers::{db_error, not_found};
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_api_types::{
    AdapterPackage, AdapterStrength, CreatePackageRequest, PackageListResponse, PackageResponse,
    UpdatePackageRequest, API_SCHEMA_VERSION,
};
use adapteros_core::StackName;
use adapteros_db::{
    AdapterStrengthOverride, CreatePackageRequest as DbCreatePackageRequest, PackageRecord,
    UpdatePackageRequest as DbUpdatePackageRequest,
};
use adapteros_types::adapters::metadata::RoutingDeterminismMode;
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use std::str::FromStr;
use tracing::info;
use uuid::Uuid;

fn to_api_strengths(raw: Option<String>) -> Vec<AdapterStrength> {
    raw.and_then(|s| serde_json::from_str::<Vec<AdapterStrengthOverride>>(&s).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|s| AdapterStrength {
            adapter_id: s.adapter_id,
            strength: s.strength,
        })
        .collect()
}

fn to_api_package(
    record: PackageRecord,
    stack: Option<adapteros_db::StackRecord>,
    installed_at: Option<String>,
) -> AdapterPackage {
    let tags: Vec<String> = record
        .tags_json
        .as_ref()
        .and_then(|t| serde_json::from_str(t).ok())
        .unwrap_or_default();

    let stack_adapter_ids = stack
        .as_ref()
        .and_then(|s| serde_json::from_str::<Vec<String>>(&s.adapter_ids_json).ok())
        .unwrap_or_default();

    let stack_det = stack.as_ref().and_then(|s| s.determinism_mode.clone());
    let record_route_det = record
        .routing_determinism_mode
        .as_ref()
        .and_then(|s| RoutingDeterminismMode::from_str(s).ok());
    let stack_route_det = stack
        .as_ref()
        .and_then(|s| s.routing_determinism_mode.as_ref())
        .and_then(|s| RoutingDeterminismMode::from_str(s).ok());

    let scope_path = record.scope_path.clone();
    let routing_determinism_mode = record_route_det.or(stack_route_det).map(|m| m.to_string());

    AdapterPackage {
        schema_version: API_SCHEMA_VERSION.to_string(),
        id: record.id,
        tenant_id: record.tenant_id,
        name: record.name,
        description: record.description,
        stack_id: record.stack_id,
        tags,
        adapter_strengths: to_api_strengths(record.adapter_strengths_json),
        adapter_ids: stack_adapter_ids,
        determinism_mode: record.determinism_mode.or(stack_det),
        routing_determinism_mode,
        domain: record.domain,
        scope_path: scope_path.clone(),
        scope_path_prefix: scope_path,
        installed: installed_at.is_some(),
        installed_at,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

#[derive(Debug, Default, serde::Deserialize)]
pub struct PackageQuery {
    pub domain: Option<String>,
}

async fn list_packages_for_tenant_internal(
    state: &AppState,
    tenant_id: &str,
    domain: Option<String>,
) -> Result<Vec<AdapterPackage>, (StatusCode, Json<ErrorResponse>)> {
    let rows = state
        .db
        .list_packages_with_install_status(tenant_id, domain.as_deref())
        .await
        .map_err(db_error)?;

    let mut packages = Vec::new();
    for (record, installed_at) in rows {
        let stack = state
            .db
            .get_stack(tenant_id, &record.stack_id)
            .await
            .map_err(db_error)?;
        packages.push(to_api_package(record, stack, installed_at));
    }

    Ok(packages)
}

fn sanitize_stack_name(base: &str) -> String {
    let slug: String = base
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    let candidate = format!("stack.pkg.{}", if slug.is_empty() { "pkg" } else { &slug });

    if StackName::parse(&candidate).is_ok() {
        candidate
    } else {
        format!("stack.pkg.{}", Uuid::now_v7().as_simple())
    }
}

async fn ensure_stack_for_package(
    state: &AppState,
    tenant_id: &str,
    req: &CreatePackageRequest,
) -> Result<String, (StatusCode, Json<ErrorResponse>)> {
    if let Some(stack_id) = &req.stack_id {
        let stack = state
            .db
            .get_stack(tenant_id, stack_id)
            .await
            .map_err(db_error)?
            .ok_or_else(|| not_found("Stack"))?;
        if stack.tenant_id != tenant_id {
            return Err((
                StatusCode::FORBIDDEN,
                Json(
                    ErrorResponse::new("Stack belongs to a different tenant")
                        .with_code("ISOLATION_VIOLATION"),
                ),
            ));
        }
        return Ok(stack_id.clone());
    }

    let adapters = if req.adapters.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("adapters are required when stack_id is not provided")
                    .with_code("VALIDATION_ERROR"),
            ),
        ));
    } else {
        req.adapters.clone()
    };

    for adapter in &adapters {
        let exists = state
            .db
            .get_adapter_by_id(tenant_id, &adapter.adapter_id)
            .await
            .map_err(db_error)?;
        if exists.is_none() {
            return Err(not_found(&format!("Adapter {}", adapter.adapter_id)));
        }
    }

    let stack_name = sanitize_stack_name(&req.name);
    let db_req = adapteros_db::CreateStackRequest {
        tenant_id: tenant_id.to_string(),
        name: stack_name,
        description: req.description.clone(),
        adapter_ids: adapters.iter().map(|a| a.adapter_id.clone()).collect(),
        workflow_type: Some("Parallel".to_string()),
        determinism_mode: req.determinism_mode.clone(),
        routing_determinism_mode: req.routing_determinism_mode.clone().map(|m| m.to_string()),
    };

    let stack_id = state.db.insert_stack(&db_req).await.map_err(db_error)?;
    Ok(stack_id)
}

#[utoipa::path(
    post,
    path = "/v1/packages",
    request_body = CreatePackageRequest,
    responses(
        (status = 201, description = "Package created", body = PackageResponse),
        (status = 400, description = "Invalid request"),
        (status = 404, description = "Stack or adapter not found"),
        (status = 409, description = "Duplicate package"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn create_package(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreatePackageRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterRegister)?;
    let tenant_id = claims.tenant_id.clone();

    let stack_id = ensure_stack_for_package(&state, &tenant_id, &req).await?;

    let strengths: Option<Vec<AdapterStrengthOverride>> = if req.adapters.is_empty() {
        None
    } else {
        Some(
            req.adapters
                .iter()
                .map(|s| AdapterStrengthOverride {
                    adapter_id: s.adapter_id.clone(),
                    strength: s.strength,
                })
                .collect(),
        )
    };

    let db_req = DbCreatePackageRequest {
        tenant_id: tenant_id.clone(),
        name: req.name.clone(),
        description: req.description.clone(),
        stack_id: stack_id.clone(),
        tags: req.tags.clone(),
        domain: req.domain.clone(),
        scope_path: req.scope_path_prefix.clone().or(req.scope_path.clone()),
        adapter_strengths: strengths,
        determinism_mode: req.determinism_mode.clone(),
        routing_determinism_mode: req.routing_determinism_mode.clone().map(|m| m.to_string()),
    };

    let id = match state.db.create_package(&db_req).await {
        Ok(id) => id,
        Err(e) => {
            if e.to_string().contains("UNIQUE constraint failed") {
                return Err((
                    StatusCode::CONFLICT,
                    Json(
                        ErrorResponse::new("Package name already exists for tenant")
                            .with_code("CONFLICT"),
                    ),
                ));
            }
            return Err(db_error(e));
        }
    };

    let package = state
        .db
        .get_package(&tenant_id, &id)
        .await
        .map_err(db_error)?
        .ok_or_else(|| not_found("Package"))?;
    let stack = state
        .db
        .get_stack(&tenant_id, &stack_id)
        .await
        .map_err(db_error)?;

    let response = PackageResponse {
        schema_version: API_SCHEMA_VERSION.to_string(),
        package: to_api_package(package, stack, None),
    };

    Ok((StatusCode::CREATED, Json(response)))
}

#[utoipa::path(
    get,
    path = "/v1/packages",
    params(
        ("domain" = Option<String>, Query, description = "Optional domain filter")
    ),
    responses(
        (status = 200, description = "List of packages", body = PackageListResponse),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn list_packages(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<PackageQuery>,
) -> Result<Json<PackageListResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterView)?;
    let tenant_id = claims.tenant_id.clone();

    let packages = list_packages_for_tenant_internal(&state, &tenant_id, query.domain).await?;

    Ok(Json(PackageListResponse {
        schema_version: API_SCHEMA_VERSION.to_string(),
        packages,
    }))
}

#[utoipa::path(
    get,
    path = "/v1/packages/{id}",
    params(
        ("id" = String, Path, description = "Package ID")
    ),
    responses(
        (status = 200, description = "Package details", body = PackageResponse),
        (status = 404, description = "Package not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_package(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<Json<PackageResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterView)?;
    let tenant_id = claims.tenant_id.clone();

    let record = state
        .db
        .get_package(&tenant_id, &id)
        .await
        .map_err(db_error)?
        .ok_or_else(|| not_found("Package"))?;

    validate_tenant_isolation(&claims, &record.tenant_id)?;

    let stack = state
        .db
        .get_stack(&tenant_id, &record.stack_id)
        .await
        .map_err(db_error)?;
    let installed_at = state
        .db
        .package_install_timestamp(&tenant_id, &id)
        .await
        .map_err(db_error)?;

    Ok(Json(PackageResponse {
        schema_version: API_SCHEMA_VERSION.to_string(),
        package: to_api_package(record, stack, installed_at),
    }))
}

#[utoipa::path(
    patch,
    path = "/v1/packages/{id}",
    request_body = UpdatePackageRequest,
    params(
        ("id" = String, Path, description = "Package ID")
    ),
    responses(
        (status = 200, description = "Package updated", body = PackageResponse),
        (status = 404, description = "Package not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn update_package(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Json(req): Json<UpdatePackageRequest>,
) -> Result<Json<PackageResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterRegister)?;
    let tenant_id = claims.tenant_id.clone();

    let existing = state
        .db
        .get_package(&tenant_id, &id)
        .await
        .map_err(db_error)?
        .ok_or_else(|| not_found("Package"))?;

    validate_tenant_isolation(&claims, &existing.tenant_id)?;

    let mut stack_id = existing.stack_id.clone();
    let mut adapter_strengths = to_api_strengths(existing.adapter_strengths_json.clone());
    let mut determinism_mode = req
        .determinism_mode
        .clone()
        .or(existing.determinism_mode.clone());
    let mut routing_determinism_mode = req
        .routing_determinism_mode
        .clone()
        .map(|m| m.to_string())
        .or(existing.routing_determinism_mode.clone());

    if let Some(new_stack_id) = &req.stack_id {
        let stack = state
            .db
            .get_stack(&tenant_id, new_stack_id)
            .await
            .map_err(db_error)?
            .ok_or_else(|| not_found("Stack"))?;
        validate_tenant_isolation(&claims, &stack.tenant_id)?;
        stack_id = new_stack_id.clone();
    }

    if let Some(adapters) = &req.adapters {
        if !adapters.is_empty() {
            adapter_strengths = adapters
                .iter()
                .map(|a| AdapterStrength {
                    adapter_id: a.adapter_id.clone(),
                    strength: a.strength,
                })
                .collect();

            let stack_name = sanitize_stack_name(
                req.name
                    .as_deref()
                    .unwrap_or_else(|| existing.name.as_str()),
            );
            let db_req = adapteros_db::CreateStackRequest {
                tenant_id: tenant_id.clone(),
                name: stack_name,
                description: req.description.clone().or(existing.description.clone()),
                adapter_ids: adapters.iter().map(|a| a.adapter_id.clone()).collect(),
                workflow_type: Some("Parallel".to_string()),
                determinism_mode: req.determinism_mode.clone().or(determinism_mode.clone()),
                routing_determinism_mode: routing_determinism_mode.clone(),
            };

            stack_id = state.db.insert_stack(&db_req).await.map_err(db_error)?;
            determinism_mode = db_req.determinism_mode.clone();
            routing_determinism_mode = db_req.routing_determinism_mode.clone();
        }
    }

    let db_req = DbUpdatePackageRequest {
        tenant_id: tenant_id.clone(),
        name: req.name.clone().unwrap_or(existing.name),
        description: req.description.clone().or(existing.description),
        stack_id: stack_id.clone(),
        tags: req.tags.clone().or_else(|| {
            existing
                .tags_json
                .as_ref()
                .and_then(|t| serde_json::from_str(t).ok())
        }),
        domain: req.domain.clone().or(existing.domain),
        scope_path: req
            .scope_path_prefix
            .clone()
            .or(req.scope_path.clone())
            .or(existing.scope_path),
        adapter_strengths: if adapter_strengths.is_empty() {
            None
        } else {
            Some(
                adapter_strengths
                    .iter()
                    .map(|s| AdapterStrengthOverride {
                        adapter_id: s.adapter_id.clone(),
                        strength: s.strength,
                    })
                    .collect(),
            )
        },
        determinism_mode,
        routing_determinism_mode,
    };

    let updated = state.db.update_package(&id, &db_req).await.map_err(|e| {
        if e.to_string().contains("UNIQUE constraint failed") {
            (
                StatusCode::CONFLICT,
                Json(
                    ErrorResponse::new("Package name already exists for tenant")
                        .with_code("CONFLICT"),
                ),
            )
        } else {
            db_error(e)
        }
    })?;

    if !updated {
        return Err(not_found("Package"));
    }

    let package = state
        .db
        .get_package(&tenant_id, &id)
        .await
        .map_err(db_error)?
        .ok_or_else(|| not_found("Package"))?;
    let stack = state
        .db
        .get_stack(&tenant_id, &stack_id)
        .await
        .map_err(db_error)?;
    let installed_at = state
        .db
        .package_install_timestamp(&tenant_id, &id)
        .await
        .map_err(db_error)?;

    Ok(Json(PackageResponse {
        schema_version: API_SCHEMA_VERSION.to_string(),
        package: to_api_package(package, stack, installed_at),
    }))
}

#[utoipa::path(
    delete,
    path = "/v1/packages/{id}",
    params(
        ("id" = String, Path, description = "Package ID")
    ),
    responses(
        (status = 204, description = "Package deleted"),
        (status = 404, description = "Package not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn delete_package(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterRegister)?;
    let tenant_id = claims.tenant_id.clone();

    let deleted = state
        .db
        .delete_package(&tenant_id, &id)
        .await
        .map_err(db_error)?;

    if !deleted {
        return Err(not_found("Package"));
    }

    info!(package_id = %id, tenant_id = %tenant_id, "Deleted adapter package");
    Ok(StatusCode::NO_CONTENT)
}

/// List packages for a specific tenant (with install state)
#[utoipa::path(
    get,
    path = "/v1/tenants/{tenant_id}/packages",
    params(
        ("tenant_id" = String, Path, description = "Tenant ID"),
        ("domain" = Option<String>, Query, description = "Optional domain filter")
    ),
    responses(
        (status = 200, description = "List of packages", body = PackageListResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
pub async fn list_packages_for_tenant(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
    Query(query): Query<PackageQuery>,
) -> Result<Json<PackageListResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterView)?;
    validate_tenant_isolation(&claims, &tenant_id)?;

    let packages = list_packages_for_tenant_internal(&state, &tenant_id, query.domain).await?;

    Ok(Json(PackageListResponse {
        schema_version: API_SCHEMA_VERSION.to_string(),
        packages,
    }))
}

/// Install (enable) a package for a tenant
#[utoipa::path(
    post,
    path = "/v1/tenants/{tenant_id}/packages/{package_id}/install",
    params(
        ("tenant_id" = String, Path, description = "Tenant ID"),
        ("package_id" = String, Path, description = "Package ID")
    ),
    responses(
        (status = 200, description = "Package installed", body = PackageResponse),
        (status = 404, description = "Package not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
pub async fn install_package_for_tenant(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((tenant_id, package_id)): Path<(String, String)>,
) -> Result<Json<PackageResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterRegister)?;
    validate_tenant_isolation(&claims, &tenant_id)?;

    let package = state
        .db
        .get_package(&tenant_id, &package_id)
        .await
        .map_err(db_error)?
        .ok_or_else(|| not_found("Package"))?;

    state
        .db
        .install_package_for_tenant(&tenant_id, &package_id)
        .await
        .map_err(db_error)?;

    let stack = state
        .db
        .get_stack(&tenant_id, &package.stack_id)
        .await
        .map_err(db_error)?;

    let installed_at = state
        .db
        .package_install_timestamp(&tenant_id, &package_id)
        .await
        .map_err(db_error)?;

    Ok(Json(PackageResponse {
        schema_version: API_SCHEMA_VERSION.to_string(),
        package: to_api_package(package, stack, installed_at),
    }))
}

/// Uninstall (disable) a package for a tenant
#[utoipa::path(
    delete,
    path = "/v1/tenants/{tenant_id}/packages/{package_id}/install",
    params(
        ("tenant_id" = String, Path, description = "Tenant ID"),
        ("package_id" = String, Path, description = "Package ID")
    ),
    responses(
        (status = 200, description = "Package uninstalled", body = PackageResponse),
        (status = 404, description = "Package not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
pub async fn uninstall_package_for_tenant(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((tenant_id, package_id)): Path<(String, String)>,
) -> Result<Json<PackageResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterRegister)?;
    validate_tenant_isolation(&claims, &tenant_id)?;

    let package = state
        .db
        .get_package(&tenant_id, &package_id)
        .await
        .map_err(db_error)?
        .ok_or_else(|| not_found("Package"))?;

    state
        .db
        .uninstall_package_for_tenant(&tenant_id, &package_id)
        .await
        .map_err(db_error)?;

    let stack = state
        .db
        .get_stack(&tenant_id, &package.stack_id)
        .await
        .map_err(db_error)?;

    let installed_at = state
        .db
        .package_install_timestamp(&tenant_id, &package_id)
        .await
        .map_err(db_error)?;

    Ok(Json(PackageResponse {
        schema_version: API_SCHEMA_VERSION.to_string(),
        package: to_api_package(package, stack, installed_at),
    }))
}
