//! Dataset readiness invariant tests (multipart + chunked JSONL).

use adapteros_server_api::create_app;
use adapteros_server_api::types::{
    CompleteChunkedUploadResponse, InitiateChunkedUploadResponse, UploadDatasetResponse,
};
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::json;
use tower::ServiceExt;
use uuid::Uuid;

mod common;

fn multipart_body(boundary: &str, dataset_name: &str, file_name: &str, file_body: &str) -> String {
    format!(
        "--{boundary}\r\n\
Content-Disposition: form-data; name=\"name\"\r\n\r\n\
{dataset_name}\r\n\
--{boundary}\r\n\
Content-Disposition: form-data; name=\"format\"\r\n\r\n\
jsonl\r\n\
--{boundary}\r\n\
Content-Disposition: form-data; name=\"file\"; filename=\"{file_name}\"\r\n\
Content-Type: application/json\r\n\r\n\
{file_body}\r\n\
--{boundary}--\r\n"
    )
}

async fn dataset_by_id(
    state: &adapteros_server_api::state::AppState,
    dataset_id: &str,
) -> adapteros_db::training_datasets::TrainingDataset {
    state
        .db
        .get_training_dataset(dataset_id)
        .await
        .expect("get dataset")
        .expect("dataset exists")
}

async fn single_dataset_for_tenant(
    state: &adapteros_server_api::state::AppState,
    tenant_id: &str,
) -> adapteros_db::training_datasets::TrainingDataset {
    let mut datasets = state
        .db
        .list_training_datasets_for_tenant(tenant_id, 100)
        .await
        .expect("list datasets");
    assert_eq!(
        datasets.len(),
        1,
        "expected exactly one dataset for tenant, found {}",
        datasets.len()
    );
    datasets.pop().expect("dataset exists")
}

#[tokio::test]
async fn multipart_jsonl_empty_marks_failed() {
    let _env = common::TestkitEnvGuard::enabled(true).await;
    let state = common::setup_state(None).await.expect("state");
    let app = create_app(state.clone());

    let dataset_name = format!("empty-multipart-{}", Uuid::new_v4());
    let boundary = "BOUNDARY-EMPTY";
    let file_body = "{\"foo\":\"bar\"}\n{\"prompt\":\"\"}\n";
    let body = multipart_body(boundary, &dataset_name, "empty.jsonl", file_body);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/datasets")
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={}", boundary),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .expect("multipart response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("response body");
    let error: serde_json::Value = serde_json::from_slice(&body).expect("error response");
    assert_eq!(error["code"].as_str(), Some("BAD_REQUEST"));

    let dataset = single_dataset_for_tenant(&state, "default").await;
    assert_eq!(dataset.status, "failed");
    assert!(dataset.name.contains("empty.jsonl"));
    assert_ne!(dataset.name, dataset_name);
}

#[tokio::test]
async fn multipart_jsonl_valid_marks_ready_with_rows() {
    let _env = common::TestkitEnvGuard::enabled(true).await;
    let state = common::setup_state(None).await.expect("state");
    let app = create_app(state.clone());

    let dataset_name = format!("valid-multipart-{}", Uuid::new_v4());
    let boundary = "BOUNDARY-VALID";
    let file_body =
        "{\"prompt\":\"hi\",\"completion\":\"there\"}\n{\"prompt\":\"two\",\"completion\":\"ok\"}\n";
    let body = multipart_body(boundary, &dataset_name, "valid.jsonl", file_body);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/datasets")
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={}", boundary),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .expect("multipart response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("response body");
    let payload: UploadDatasetResponse = serde_json::from_slice(&body).expect("upload response");

    assert!(payload.name.contains("valid.jsonl"));
    assert_ne!(payload.name, dataset_name);

    let dataset = dataset_by_id(&state, &payload.dataset_id).await;
    assert_eq!(dataset.status, "ready");
    assert_eq!(dataset.name, payload.name);

    let row_count = state
        .db
        .count_training_dataset_rows(&payload.dataset_id, payload.dataset_version_id.as_deref())
        .await
        .expect("row count");
    assert_eq!(row_count, 2);
}

#[tokio::test]
async fn chunked_jsonl_empty_marks_failed() {
    let _env = common::TestkitEnvGuard::enabled(true).await;
    let state = common::setup_state(None).await.expect("state");
    let app = create_app(state.clone());

    let dataset_name = format!("empty-chunked-{}", Uuid::new_v4());
    let file_body = b"{\"foo\":\"bar\"}\n{\"prompt\":\"\"}\n";

    let initiate_body = json!({
        "file_name": "empty.jsonl",
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
        "name": dataset_name,
        "format": "jsonl"
    });
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

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("complete body");
    let error: serde_json::Value = serde_json::from_slice(&body).expect("error response");
    assert_eq!(error["code"].as_str(), Some("BAD_REQUEST"));

    let dataset = single_dataset_for_tenant(&state, "default").await;
    assert_eq!(dataset.status, "failed");
    assert!(dataset.name.contains("empty.jsonl"));
    assert_ne!(dataset.name, dataset_name);
}

#[tokio::test]
async fn chunked_jsonl_valid_marks_ready_with_rows() {
    let _env = common::TestkitEnvGuard::enabled(true).await;
    let state = common::setup_state(None).await.expect("state");
    let app = create_app(state.clone());

    let dataset_name = format!("valid-chunked-{}", Uuid::new_v4());
    let file_body =
        b"{\"prompt\":\"hi\",\"completion\":\"there\"}\n{\"prompt\":\"two\",\"completion\":\"ok\"}\n";

    let initiate_body = json!({
        "file_name": "valid.jsonl",
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
        "name": dataset_name,
        "format": "jsonl"
    });
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
    let payload: CompleteChunkedUploadResponse =
        serde_json::from_slice(&body).expect("complete response");

    assert!(payload.name.contains("valid.jsonl"));
    assert_ne!(payload.name, dataset_name);

    let dataset = dataset_by_id(&state, &payload.dataset_id).await;
    assert_eq!(dataset.status, "ready");
    assert_eq!(dataset.name, payload.name);

    let row_count = state
        .db
        .count_training_dataset_rows(&payload.dataset_id, payload.dataset_version_id.as_deref())
        .await
        .expect("row count");
    assert_eq!(row_count, 2);
}

#[tokio::test]
async fn chunked_jsonl_preview_boundary_valid_marks_ready_with_rows() {
    let _env = common::TestkitEnvGuard::enabled(true).await;
    let state = common::setup_state(None).await.expect("state");
    let app = create_app(state.clone());

    let dataset_name = format!("valid-chunked-preview-boundary-{}", Uuid::new_v4());
    let oversized_prompt = "x".repeat(70 * 1024);
    let file_body = format!(
        "{{\"prompt\":\"{}\",\"completion\":\"there\"}}\n{{\"prompt\":\"two\",\"completion\":\"ok\"}}\n",
        oversized_prompt
    );
    let file_body = file_body.into_bytes();

    let initiate_body = json!({
        "file_name": "valid-preview-boundary.jsonl",
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
                .body(Body::from(file_body))
                .unwrap(),
        )
        .await
        .expect("chunk upload response");
    assert_eq!(response.status(), StatusCode::OK);

    let complete_body = json!({
        "name": dataset_name,
        "format": "jsonl"
    });
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
    let payload: CompleteChunkedUploadResponse =
        serde_json::from_slice(&body).expect("complete response");

    assert!(payload.name.contains("valid-preview-boundary.jsonl"));
    assert_ne!(payload.name, dataset_name);

    let dataset = dataset_by_id(&state, &payload.dataset_id).await;
    assert_eq!(dataset.status, "ready");
    assert_eq!(dataset.name, payload.name);

    let row_count = state
        .db
        .count_training_dataset_rows(&payload.dataset_id, payload.dataset_version_id.as_deref())
        .await
        .expect("row count");
    assert_eq!(row_count, 2);
}
