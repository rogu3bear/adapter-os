//! Chunked upload session lifecycle tests.

use adapteros_db::sqlx::Row;
use adapteros_server_api::handlers::datasets::cleanup_expired_sessions;
use adapteros_server_api::ip_extraction::ClientIp;
use adapteros_server_api::types::{CompleteChunkedUploadResponse, InitiateChunkedUploadResponse};
use adapteros_server_api::{create_app, state::AppState};
use axum::{
    body::{to_bytes, Body},
    extract::State,
    http::{Request, StatusCode},
    Extension,
};
use serde_json::json;
use tower::ServiceExt;
use uuid::Uuid;

mod common;

async fn fetch_session_status(state: &AppState, session_id: &str) -> (String, Option<String>) {
    let row = adapteros_db::sqlx::query(
        "SELECT status, error_message FROM dataset_upload_sessions WHERE session_id = ?",
    )
    .bind(session_id)
    .fetch_one(state.db.pool())
    .await
    .expect("session row");
    let status: String = row.get("status");
    let error_message: Option<String> = row.get("error_message");
    (status, error_message)
}

#[tokio::test]
async fn cleanup_marks_stale_session_failed() {
    std::env::remove_var("AOS_KEEP_CHUNKED_UPLOAD_PARTS");
    let state = common::setup_state(None).await.expect("state");
    let claims = common::test_admin_claims();

    let session_id = Uuid::new_v4().to_string();
    let temp_dir_obj =
        tempfile::TempDir::with_prefix("aos-test-session-cleanup-").expect("temp dir");
    let temp_dir = temp_dir_obj.path().to_path_buf();

    adapteros_db::sqlx::query(
        "INSERT INTO dataset_upload_sessions (
            session_id, session_key, tenant_id, workspace_id, dataset_id, file_name,
            normalized_file_name, total_size_bytes, chunk_size_bytes, content_type,
            received_chunks_json, received_chunks_count, status, temp_dir, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now','-7200 seconds'), datetime('now','-7200 seconds'))",
    )
    .bind(&session_id)
    .bind(format!("key-{}", session_id))
    .bind(&claims.tenant_id)
    .bind(&claims.tenant_id)
    .bind(format!("dataset-{}", session_id))
    .bind("file.jsonl")
    .bind("file.jsonl")
    .bind(10_i64)
    .bind(10_i64)
    .bind("application/json")
    .bind("{}")
    .bind(0_i64)
    .bind("uploading")
    .bind(temp_dir.to_string_lossy().to_string())
    .execute(state.db.pool())
    .await
    .expect("insert session");

    let _ = cleanup_expired_sessions(
        State(state.clone()),
        Extension(claims),
        Extension(ClientIp("127.0.0.1".to_string())),
    )
    .await
    .expect("cleanup");

    let (status, error_message) = fetch_session_status(&state, &session_id).await;
    assert_eq!(status, "failed");
    assert_eq!(error_message.as_deref(), Some("Upload session expired"));
    assert!(!temp_dir.exists(), "temp dir should be removed");
}

#[tokio::test]
async fn chunked_complete_only_once() {
    let _env = common::TestkitEnvGuard::enabled(true).await;
    let state = common::setup_state(None).await.expect("state");
    let app = create_app(state.clone());

    let file_body = b"{\"prompt\":\"hi\",\"completion\":\"there\"}\n";
    let initiate_body = json!({
        "file_name": "once.jsonl",
        "total_size": file_body.len(),
        "content_type": "application/json"
    });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/datasets/chunked-upload/initiate")
                .header("content-type", "application/json")
                .body(Body::from(initiate_body.to_string()))
                .unwrap(),
        )
        .await
        .expect("initiate response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("initiate body");
    let initiate: InitiateChunkedUploadResponse =
        serde_json::from_slice(&body).expect("initiate response");

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/v1/datasets/chunked-upload/{}/chunk?chunk_index=0",
                    initiate.session_id
                ))
                .header("content-type", "application/octet-stream")
                .body(Body::from(file_body.to_vec()))
                .unwrap(),
        )
        .await
        .expect("chunk upload response");
    assert_eq!(response.status(), StatusCode::OK);

    let complete_body = json!({
        "name": "once-dataset",
        "format": "jsonl"
    });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/v1/datasets/chunked-upload/{}/complete",
                    initiate.session_id
                ))
                .header("content-type", "application/json")
                .body(Body::from(complete_body.to_string()))
                .unwrap(),
        )
        .await
        .expect("complete response");
    let status = response.status();
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("complete body");
    if status != StatusCode::OK {
        eprintln!(
            "Complete response error: status={}, body={}",
            status,
            String::from_utf8_lossy(&body)
        );
    }
    assert_eq!(status, StatusCode::OK);
    let payload: CompleteChunkedUploadResponse =
        serde_json::from_slice(&body).expect("complete response");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/v1/datasets/chunked-upload/{}/complete",
                    initiate.session_id
                ))
                .header("content-type", "application/json")
                .body(Body::from(complete_body.to_string()))
                .unwrap(),
        )
        .await
        .expect("complete response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("complete body");
    let retry: CompleteChunkedUploadResponse =
        serde_json::from_slice(&body).expect("complete response");
    assert_eq!(retry.dataset_id, payload.dataset_id);
}
