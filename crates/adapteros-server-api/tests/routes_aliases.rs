use adapteros_server_api::handlers::{aliases, run_evidence, workspaces};
use axum::{
    body::{to_bytes, Body},
    http::{header, HeaderMap, Request, StatusCode},
    routing::get,
    Extension, Router,
};
use tower::ServiceExt;

mod common;
use common::{setup_state, test_admin_claims, TestkitEnvGuard};

async fn get_response(
    app: &Router,
    path: &str,
) -> (StatusCode, HeaderMap, bytes::Bytes) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(path)
                .body(Body::empty())
                .expect("request build"),
        )
        .await
        .expect("router response");
    let status = response.status();
    let headers = response.headers().clone();
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    (status, headers, body)
}

#[tokio::test]
async fn routes_evidence_alias_matches_canonical() -> anyhow::Result<()> {
    let _env = TestkitEnvGuard::disabled().await;
    let state = setup_state(None).await?;
    let claims = test_admin_claims();

    let app = Router::new()
        .route(
            "/v1/runs/{run_id}/evidence",
            get(run_evidence::download_run_evidence),
        )
        .route(
            "/v1/evidence/runs/{run_id}/export",
            get(aliases::run_evidence::download_run_evidence_alias),
        )
        .layer(Extension(claims))
        .with_state(state);

    let (canon_status, canon_headers, canon_body) =
        get_response(&app, "/v1/runs/missing-run/evidence").await;
    let (alias_status, alias_headers, alias_body) =
        get_response(&app, "/v1/evidence/runs/missing-run/export").await;

    assert_eq!(canon_status, alias_status);
    assert_eq!(canon_body, alias_body);
    assert!(canon_headers.get("Deprecation").is_none());
    assert_eq!(
        alias_headers
            .get("Deprecation")
            .and_then(|value| value.to_str().ok()),
        Some("true")
    );
    assert_eq!(
        alias_headers
            .get(header::LINK)
            .and_then(|value| value.to_str().ok()),
        Some("</v1/runs/missing-run/evidence>; rel=\"canonical\"")
    );

    Ok(())
}

#[tokio::test]
async fn routes_workspace_active_state_alias_matches_canonical() -> anyhow::Result<()> {
    let _env = TestkitEnvGuard::disabled().await;
    let state = setup_state(None).await?;
    let claims = test_admin_claims();

    let app = Router::new()
        .route(
            "/v1/workspaces/{workspace_id}/active",
            get(workspaces::get_workspace_active_state),
        )
        .route(
            "/v1/workspaces/{workspace_id}/active-state",
            get(aliases::workspaces::get_workspace_active_state_alias),
        )
        .layer(Extension(claims))
        .with_state(state);

    let workspace_id = "tenant-1";
    let (canon_status, _canon_headers, canon_body) = get_response(
        &app,
        &format!("/v1/workspaces/{}/active", workspace_id),
    )
    .await;
    let (alias_status, alias_headers, alias_body) = get_response(
        &app,
        &format!("/v1/workspaces/{}/active-state", workspace_id),
    )
    .await;

    assert_eq!(canon_status, alias_status);
    assert_eq!(canon_body, alias_body);
    assert_eq!(
        alias_headers
            .get("Deprecation")
            .and_then(|value| value.to_str().ok()),
        Some("true")
    );
    assert_eq!(
        alias_headers
            .get(header::LINK)
            .and_then(|value| value.to_str().ok()),
        Some("</v1/workspaces/tenant-1/active>; rel=\"canonical\"")
    );

    Ok(())
}
