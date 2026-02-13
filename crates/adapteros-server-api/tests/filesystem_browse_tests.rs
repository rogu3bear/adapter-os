//! Filesystem browse handler tests
//!
//! Focus: permission gating + hidden-file filtering + allowed-roots jail.

use adapteros_api_types::filesystem::{FileBrowseRequest, FileBrowseResponse};
use adapteros_server_api::handlers;
use axum::{
    body::to_bytes,
    extract::{Extension, Query, State},
    http::StatusCode,
    response::IntoResponse,
};

mod common;
type EnvGuard = common::TestkitEnvGuard;

fn find_entry(resp: &FileBrowseResponse, name: &str) -> bool {
    resp.entries.iter().any(|e| e.name == name)
}

#[tokio::test]
async fn filesystem_browse_denies_viewer_role() {
    let _env = EnvGuard::disabled().await;

    let state = common::setup_state(None).await.expect("setup state");
    let params = FileBrowseRequest {
        path: String::new(),
        show_hidden: false,
    };

    let res = handlers::filesystem::browse_filesystem(
        State(state),
        Extension(common::test_viewer_claims()),
        Query(params),
    )
    .await;

    let err = match res {
        Ok(_) => panic!("viewer should be forbidden"),
        Err(err) => err,
    };

    assert_eq!(err.0, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn filesystem_browse_lists_entries_and_filters_hidden() {
    let _env = EnvGuard::disabled().await;

    // Avoid OS temp directories; use adapterOS var/tmp.
    let var_root_tmp = adapteros_core::tempdir_in_var("aos-fs-browse-").expect("temp var root");
    let var_root = var_root_tmp.path().to_path_buf();

    let visible_file = var_root.join("visible.txt");
    let hidden_file = var_root.join(".hidden.txt");
    let subdir = var_root.join("subdir");
    std::fs::write(&visible_file, "ok").expect("write visible file");
    std::fs::write(&hidden_file, "hidden").expect("write hidden file");
    std::fs::create_dir_all(&subdir).expect("create subdir");

    let state = common::setup_state(None).await.expect("setup state");

    // Hidden disabled.
    let ok = handlers::filesystem::browse_filesystem(
        State(state),
        Extension(common::test_operator_claims()),
        Query(FileBrowseRequest {
            path: var_root.to_string_lossy().to_string(),
            show_hidden: false,
        }),
    )
    .await
    .expect("operator browse ok");

    let response = ok.into_response();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body");
    let parsed: FileBrowseResponse = serde_json::from_slice(&body).expect("parse response");

    assert!(find_entry(&parsed, "visible.txt"));
    assert!(find_entry(&parsed, "subdir"));
    assert!(!find_entry(&parsed, ".hidden.txt"));

    // Hidden enabled.
    let state2 = common::setup_state(None).await.expect("setup state");
    let ok2 = handlers::filesystem::browse_filesystem(
        State(state2),
        Extension(common::test_operator_claims()),
        Query(FileBrowseRequest {
            path: var_root.to_string_lossy().to_string(),
            show_hidden: true,
        }),
    )
    .await
    .expect("operator browse ok");

    let response2 = ok2.into_response();
    assert_eq!(response2.status(), StatusCode::OK);
    let body2 = to_bytes(response2.into_body(), usize::MAX)
        .await
        .expect("read body");
    let parsed2: FileBrowseResponse = serde_json::from_slice(&body2).expect("parse response");

    assert!(find_entry(&parsed2, ".hidden.txt"));
}
