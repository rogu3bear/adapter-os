use crate::auth::Claims;
use crate::handlers::aliases::add_alias_headers;
use crate::handlers::run_evidence;
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
    Extension,
};

pub async fn download_run_evidence_alias(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(run_id): Path<String>,
) -> Response {
    let canonical = format!("/v1/runs/{}/evidence", run_id);
    let response = match run_evidence::download_run_evidence(
        State(state),
        Extension(claims),
        Path(run_id),
    )
    .await
    {
        Ok(response) => response,
        Err(err) => err.into_response(),
    };
    add_alias_headers(response, &canonical)
}
