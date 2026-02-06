use axum::http::StatusCode;
use axum::{
    extract::{Path, State},
    Extension, Json,
};
use std::path::Component;

use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::types::{ErrorResponse, GoldenCompareRequest, GoldenRunSummary};

use adapteros_verify::{
    verify_against_golden, ComparisonConfig, GoldenRunArchive, StrictnessLevel,
};

/// Validate a golden run name to prevent path traversal attacks.
///
/// Checks for:
/// - Parent directory references (..)
/// - URL-encoded traversal patterns
/// - Null bytes
/// - Absolute paths
fn validate_golden_name(name: &str) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    // Check for URL-encoded and other traversal patterns
    let patterns = [
        "..",
        "%2e%2e",
        "%2E%2E",
        "%252e%252e",
        "%c0%ae",
        "%c1%9c",
        "..%2f",
        "..%5c",
        "%00", // Null byte attack
    ];

    let lower = name.to_lowercase();
    for pattern in patterns {
        if lower.contains(&pattern.to_lowercase()) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("invalid golden run name")
                        .with_code("PATH_TRAVERSAL")
                        .with_string_details("Name contains forbidden path traversal pattern"),
                ),
            ));
        }
    }

    // Check path components for parent directory or absolute references
    let path = std::path::Path::new(name);
    for component in path.components() {
        match component {
            Component::ParentDir => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(
                        ErrorResponse::new("invalid golden run name")
                            .with_code("PATH_TRAVERSAL")
                            .with_string_details("Name contains parent directory reference (..)"),
                    ),
                ));
            }
            Component::RootDir => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(
                        ErrorResponse::new("invalid golden run name")
                            .with_code("PATH_TRAVERSAL")
                            .with_string_details("Name cannot be an absolute path"),
                    ),
                ));
            }
            _ => {}
        }
    }

    Ok(())
}

/// GET /v1/golden/runs — list golden baseline names
#[utoipa::path(
    get,
    path = "/v1/golden/runs",
    responses(
        (status = 200, description = "List of golden run names", body = Vec<String>),
        (status = 500, description = "Failed to list golden runs", body = ErrorResponse)
    ),
    tag = "golden"
)]
pub async fn list_golden_runs(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<String>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::PromotionManage)?;

    let base = std::path::Path::new("golden_runs");
    // Ensure directory exists; empty vec if none
    if !base.exists() {
        // Return empty list rather than 404 to simplify UX
        return Ok(Json(Vec::new()));
    }
    match adapteros_verify::list_golden_runs(base) {
        Ok(names) => Ok(Json(names)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to list golden runs")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )),
    }
}

/// GET /v1/golden/runs/:name — return a summary of a specific golden baseline
#[utoipa::path(
    get,
    path = "/v1/golden/runs/{name}",
    params(
        ("name" = String, Path, description = "Golden run name")
    ),
    responses(
        (status = 200, description = "Golden run summary", body = GoldenRunSummary),
        (status = 404, description = "Golden run not found", body = ErrorResponse)
    ),
    tag = "golden"
)]
pub async fn get_golden_run(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(name): Path<String>,
) -> Result<Json<GoldenRunSummary>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::PromotionManage)?;

    // Validate name to prevent path traversal attacks
    validate_golden_name(&name)?;

    let base_dir = std::path::Path::new("golden_runs").join("baselines");
    let dir = base_dir.join(&name);

    // Verify the resolved path is still under baselines directory
    // (defense in depth after validation)
    if let Ok(canonical_base) = base_dir.canonicalize() {
        if let Ok(canonical_dir) = dir.canonicalize() {
            if !canonical_dir.starts_with(&canonical_base) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(
                        ErrorResponse::new("invalid golden run name")
                            .with_code("PATH_TRAVERSAL")
                            .with_string_details("Resolved path escapes baselines directory"),
                    ),
                ));
            }
        }
    }
    match GoldenRunArchive::load(&dir) {
        Ok(archive) => {
            let layer_count = archive.epsilon_stats.layer_stats.len();
            let max_epsilon = archive.epsilon_stats.max_epsilon();
            let mean_epsilon = archive.epsilon_stats.mean_epsilon();
            let summary = GoldenRunSummary {
                name,
                run_id: archive.metadata.run_id,
                cpid: archive.metadata.cpid,
                plan_id: archive.metadata.plan_id,
                bundle_hash: archive.bundle_hash.to_string(),
                layer_count,
                max_epsilon,
                mean_epsilon,
                toolchain_summary: archive.metadata.toolchain.summary(),
                adapters: archive.metadata.adapters,
                created_at: archive.metadata.created_at.to_rfc3339(),
                has_signature: archive.signature.is_some(),
            };
            Ok(Json(summary))
        }
        Err(e) => Err((
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("golden run not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(e.to_string()),
            ),
        )),
    }
}

/// POST /v1/golden/compare — compare a bundle against a golden baseline
#[utoipa::path(
    post,
    path = "/v1/golden/compare",
    request_body = GoldenCompareRequest,
    responses(
        (status = 200, description = "Verification report"),
        (status = 404, description = "Golden baseline or bundle not found", body = ErrorResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Verification failed", body = ErrorResponse)
    ),
    tag = "golden"
)]
pub async fn golden_compare(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<GoldenCompareRequest>,
) -> Result<
    Json<adapteros_verify::verification::VerificationReport>,
    (StatusCode, Json<ErrorResponse>),
> {
    require_permission(&claims, Permission::PromotionManage)?;

    // Validate golden name to prevent path traversal attacks
    validate_golden_name(&req.golden)?;

    // Resolve golden path and bundle path
    let base_dir = std::path::Path::new("golden_runs").join("baselines");
    let golden_dir = base_dir.join(&req.golden);

    // Verify the resolved path is still under baselines directory
    // (defense in depth after validation)
    if let Ok(canonical_base) = base_dir.canonicalize() {
        if let Ok(canonical_dir) = golden_dir.canonicalize() {
            if !canonical_dir.starts_with(&canonical_base) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(
                        ErrorResponse::new("invalid golden name")
                            .with_code("PATH_TRAVERSAL")
                            .with_string_details("Resolved path escapes baselines directory"),
                    ),
                ));
            }
        }
    }

    if !golden_dir.exists() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("golden baseline not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(format!("{}", golden_dir.display())),
            ),
        ));
    }

    let bundle_path = std::path::Path::new("var")
        .join("bundles")
        .join(format!("{}.ndjson", req.bundle_id));
    if !bundle_path.exists() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("bundle not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(format!("{}", bundle_path.display())),
            ),
        ));
    }

    // Build comparison config with defaults
    let mut config = ComparisonConfig::default();

    // Strictness mapping
    if let Some(level) = &req.strictness {
        let lv = match level.as_str() {
            "bitwise" => StrictnessLevel::Bitwise,
            "epsilon-tolerant" => StrictnessLevel::EpsilonTolerant,
            "statistical" => StrictnessLevel::Statistical,
            other => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(
                        ErrorResponse::new("invalid strictness")
                            .with_code("BAD_REQUEST")
                            .with_string_details(format!("unknown strictness: {}", other)),
                    ),
                ))
            }
        };
        config.strictness = lv;
    }

    // Toggles with defaults: toolchain/adapters/signature=true; device=false
    if let Some(v) = req.verify_toolchain {
        config.verify_toolchain = v;
    }
    if let Some(v) = req.verify_adapters {
        config.verify_adapters = v;
    }
    if let Some(v) = req.verify_signature {
        config.verify_signature = v;
    }
    if let Some(v) = req.verify_device {
        config.verify_device = v;
    }

    // Run verification
    match verify_against_golden(&golden_dir, &bundle_path, &config).await {
        Ok(report) => Ok(Json(report)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("verification failed")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )),
    }
}
