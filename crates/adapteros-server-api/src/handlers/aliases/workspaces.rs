use crate::auth::Claims;
use crate::handlers::aliases::add_alias_headers;
use crate::handlers::workspaces;
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
    Extension,
};

pub async fn get_workspace_active_state_alias(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Response {
    let canonical = format!("/v1/workspaces/{}/active", workspace_id);
    let response = match workspaces::get_workspace_active_state(
        State(state),
        Extension(claims),
        Path(workspace_id),
    )
    .await
    {
        Ok(response) => response.into_response(),
        Err(err) => err.into_response(),
    };
    add_alias_headers(response, &canonical)
}
