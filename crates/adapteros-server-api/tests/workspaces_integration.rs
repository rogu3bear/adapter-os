//! Workspace integration tests
//!
//! Validates workspace CRUD and membership guardrails using the in-memory AppState.

use adapteros_api_types::PaginationParams;
use adapteros_db::workspaces::WorkspaceRole;
use adapteros_server_api::auth::Claims;
use adapteros_server_api::handlers::workspaces::{
    create_workspace, get_workspace, list_workspaces, CreateWorkspaceRequest, WorkspaceResponse,
};
use adapteros_server_api::ip_extraction::ClientIp;
use adapteros_server_api::state::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};

mod common;
use common::{setup_state, test_admin_claims, test_viewer_claims};

async fn create_workspace_for(
    state: &AppState,
    claims: &Claims,
    name: &str,
    description: Option<&str>,
) -> WorkspaceResponse {
    let Json(created) = create_workspace(
        State(state.clone()),
        Extension(claims.clone()),
        Extension(ClientIp("127.0.0.1".to_string())),
        Json(CreateWorkspaceRequest {
            name: name.to_string(),
            description: description.map(|d| d.to_string()),
        }),
    )
    .await
    .expect("workspace creation");

    created
}

#[tokio::test]
async fn create_and_get_workspace_happy_path() {
    let state = setup_state(None).await.expect("state");
    let admin_claims = test_admin_claims();

    let created = create_workspace_for(&state, &admin_claims, "Workspace A", Some("primary")).await;

    // Creator should be stored as owner membership
    let owner_membership = state
        .db
        .get_workspace_member(
            &created.id,
            &admin_claims.tenant_id,
            Some(&admin_claims.sub),
        )
        .await
        .expect("lookup creator membership");
    assert!(owner_membership.is_some());
    assert_eq!(
        owner_membership.unwrap().role,
        WorkspaceRole::Owner.to_string()
    );

    let Json(fetched) = get_workspace(
        State(state.clone()),
        Extension(admin_claims.clone()),
        Path(created.id.clone()),
    )
    .await
    .expect("get workspace");

    assert_eq!(fetched.id, created.id);
    assert_eq!(fetched.name, "Workspace A");
    assert_eq!(fetched.description.as_deref(), Some("primary"));
}

#[tokio::test]
async fn list_workspaces_respects_membership() {
    let state = setup_state(None).await.expect("state");
    let admin_claims = test_admin_claims();
    let viewer_claims = test_viewer_claims();

    let ws_admin_only =
        create_workspace_for(&state, &admin_claims, "Admin Only", Some("private")).await;
    let ws_shared = create_workspace_for(&state, &admin_claims, "Shared Workspace", None).await;

    // Grant viewer membership only to the shared workspace
    state
        .db
        .add_workspace_member(
            &ws_shared.id,
            &viewer_claims.tenant_id,
            Some(&viewer_claims.sub),
            WorkspaceRole::Viewer,
            None,
            &admin_claims.sub,
        )
        .await
        .expect("add viewer membership");

    let Json(page) = list_workspaces(
        State(state.clone()),
        Extension(viewer_claims.clone()),
        Query(PaginationParams { page: 1, limit: 10 }),
    )
    .await
    .expect("list workspaces");

    let ids: Vec<String> = page.data.into_iter().map(|w| w.id).collect();
    assert!(ids.contains(&ws_shared.id));
    assert!(!ids.contains(&ws_admin_only.id));
    assert_eq!(page.total, 1);
}

#[tokio::test]
async fn get_workspace_denies_non_member() {
    let state = setup_state(None).await.expect("state");
    let admin_claims = test_admin_claims();
    let viewer_claims = test_viewer_claims();

    let ws_private = create_workspace_for(&state, &admin_claims, "Private Workspace", None).await;

    let err = get_workspace(
        State(state.clone()),
        Extension(viewer_claims.clone()),
        Path(ws_private.id.clone()),
    )
    .await
    .expect_err("non-member should be rejected");

    assert_eq!(err.0, StatusCode::FORBIDDEN);
}
