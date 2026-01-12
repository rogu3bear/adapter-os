#[cfg(test)]
mod tests {
    use super::super::super::common::test_harness::ApiTestHarness;
    use adapteros_server_api::handlers::chunked_upload::MIN_CHUNK_SIZE;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use serde_json::Value;
    use tower::ServiceExt;

    fn build_jsonl_payload(min_size: usize) -> Vec<u8> {
        let line = b"{\"input\":\"a\",\"output\":\"b\"}\n";
        let lines = (min_size + line.len() - 1) / line.len();
        let total_size = lines * line.len();
        line.repeat(lines).into_iter().take(total_size).collect()
    }

    async fn send_json(
        app: &axum::Router,
        method: &str,
        uri: &str,
        token: &str,
        body: Value,
    ) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {}", token))
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload = if bytes.is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };

        (status, payload)
    }

    async fn send_bytes(
        app: &axum::Router,
        method: &str,
        uri: &str,
        token: &str,
        body: Vec<u8>,
    ) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/octet-stream")
            .header("authorization", format!("Bearer {}", token))
            .body(Body::from(body))
            .unwrap();

        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload = if bytes.is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };

        (status, payload)
    }

    #[tokio::test]
    async fn test_chunked_upload_idempotency_and_dedupe() {
        let mut harness = ApiTestHarness::new().await.expect("Failed to init harness");
        let token = harness
            .authenticate()
            .await
            .expect("Failed to authenticate");

        let payload = build_jsonl_payload(MIN_CHUNK_SIZE + 128);
        let total_size = payload.len() as u64;
        let expected_hash = blake3::hash(&payload).to_hex().to_string();
        let idempotency_key = "chunked-idempotency-key-1";

        let (status, init_payload) = send_json(
            &harness.app,
            "POST",
            "/v1/datasets/chunked-upload/initiate",
            &token,
            serde_json::json!({
                "file_name": "training.jsonl",
                "total_size": total_size,
                "chunk_size": MIN_CHUNK_SIZE,
                "content_type": "application/jsonl",
                "idempotency_key": idempotency_key,
                "expected_file_hash_b3": expected_hash,
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let session_id = init_payload["session_id"].as_str().unwrap().to_string();
        let chunk_size = init_payload["chunk_size"].as_u64().unwrap() as usize;
        assert_eq!(chunk_size, MIN_CHUNK_SIZE);

        let first_chunk = payload[..chunk_size].to_vec();
        let second_chunk = payload[chunk_size..].to_vec();

        let (status, _) = send_bytes(
            &harness.app,
            "POST",
            &format!(
                "/v1/datasets/chunked-upload/{}/chunk?chunk_index=0",
                session_id
            ),
            &token,
            first_chunk,
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, _) = send_bytes(
            &harness.app,
            "POST",
            &format!(
                "/v1/datasets/chunked-upload/{}/chunk?chunk_index=1",
                session_id
            ),
            &token,
            second_chunk,
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, complete_payload) = send_json(
            &harness.app,
            "POST",
            &format!(
                "/v1/datasets/chunked-upload/{}/complete",
                session_id
            ),
            &token,
            serde_json::json!({
                "name": "idempotent-dataset",
                "description": "test dataset",
                "format": "jsonl"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let dataset_id = complete_payload["dataset_id"].as_str().unwrap().to_string();
        assert!(!dataset_id.is_empty());

        let (status, retry_init_payload) = send_json(
            &harness.app,
            "POST",
            "/v1/datasets/chunked-upload/initiate",
            &token,
            serde_json::json!({
                "file_name": "training.jsonl",
                "total_size": total_size,
                "chunk_size": MIN_CHUNK_SIZE,
                "content_type": "application/jsonl",
                "idempotency_key": idempotency_key,
                "expected_file_hash_b3": expected_hash,
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            retry_init_payload["session_id"].as_str().unwrap(),
            session_id
        );

        let (status, retry_complete_payload) = send_json(
            &harness.app,
            "POST",
            &format!(
                "/v1/datasets/chunked-upload/{}/complete",
                session_id
            ),
            &token,
            serde_json::json!({
                "format": "jsonl"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            retry_complete_payload["dataset_id"].as_str().unwrap(),
            dataset_id
        );

        let (status, conflict_payload) = send_json(
            &harness.app,
            "POST",
            "/v1/datasets/chunked-upload/initiate",
            &token,
            serde_json::json!({
                "file_name": "training.jsonl",
                "total_size": total_size,
                "chunk_size": MIN_CHUNK_SIZE,
                "content_type": "application/jsonl",
                "idempotency_key": idempotency_key,
                "expected_file_hash_b3": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(conflict_payload["code"], "IDEMPOTENCY_CONFLICT");

        let (status, init_payload) = send_json(
            &harness.app,
            "POST",
            "/v1/datasets/chunked-upload/initiate",
            &token,
            serde_json::json!({
                "file_name": "training.jsonl",
                "total_size": total_size,
                "chunk_size": MIN_CHUNK_SIZE,
                "content_type": "application/jsonl",
                "idempotency_key": "chunked-idempotency-key-2",
                "expected_file_hash_b3": expected_hash,
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let new_session_id = init_payload["session_id"].as_str().unwrap();
        let (status, _) = send_bytes(
            &harness.app,
            "POST",
            &format!(
                "/v1/datasets/chunked-upload/{}/chunk?chunk_index=0",
                new_session_id
            ),
            &token,
            payload[..chunk_size].to_vec(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, incomplete_payload) = send_json(
            &harness.app,
            "POST",
            &format!(
                "/v1/datasets/chunked-upload/{}/complete",
                new_session_id
            ),
            &token,
            serde_json::json!({
                "format": "jsonl"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(incomplete_payload["code"], "UPLOAD_INCOMPLETE");
    }
}
