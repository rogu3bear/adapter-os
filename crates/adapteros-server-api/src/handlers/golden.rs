use axum::http::StatusCode;
use axum::{
    extract::{Path, State},
    Extension, Json,
};

use crate::auth::Claims;
use crate::state::AppState;
use crate::types::{ErrorResponse, GoldenCompareRequest, GoldenRunSummary};

use adapteros_verify::{
    list_golden_runs, verify_against_golden, ComparisonConfig, GoldenRunArchive, StrictnessLevel,
};

/// GET /v1/golden/runs — list golden baseline names
pub async fn list_golden_runs(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Result<Json<Vec<String>>, (StatusCode, Json<ErrorResponse>)> {
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
pub async fn get_golden_run(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(name): Path<String>,
) -> Result<Json<GoldenRunSummary>, (StatusCode, Json<ErrorResponse>)> {
    let dir = std::path::Path::new("golden_runs")
        .join("baselines")
        .join(&name);
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
pub async fn golden_compare(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Json(req): Json<GoldenCompareRequest>,
) -> Result<
    Json<adapteros_verify::verification::VerificationReport>,
    (StatusCode, Json<ErrorResponse>),
> {
    // Resolve golden path and bundle path
    let golden_dir = std::path::Path::new("golden_runs")
        .join("baselines")
        .join(&req.golden);
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
