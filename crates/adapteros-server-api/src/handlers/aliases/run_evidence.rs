use crate::auth::Claims;
use crate::handlers::aliases::add_alias_headers;
use crate::handlers::run_evidence;
use crate::state::AppState;
use crate::types::ErrorResponse;
use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
    Extension,
};

#[deprecated(note = "Use /v1/runs/{run_id}/evidence instead.")]
#[utoipa::path(
    get,
    path = "/v1/evidence/runs/{run_id}/export",
    params(
        ("run_id" = String, Path, description = "Inference run identifier (request_id)")
    ),
    responses(
        (status = 200, description = "Evidence bundle zip for the run"),
        (status = 403, description = "Forbidden", body = ErrorResponse),
        (status = 404, description = "Run not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "replay"
)]
pub async fn download_run_evidence_alias(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(run_id): Path<String>,
) -> Response {
    use crate::handlers::run_evidence::EvidenceExportParams;
    use axum::extract::Query;

    let canonical = format!("/v1/runs/{}/evidence", run_id);
    let response = match run_evidence::download_run_evidence(
        State(state),
        Extension(claims),
        Path(run_id),
        Query(EvidenceExportParams::default()),
    )
    .await
    {
        Ok(response) => response,
        Err(err) => err.into_response(),
    };
    add_alias_headers(response, &canonical)
}
