//! Dev no-auth login + dataset creation integration test.

#[cfg(test)]
mod tests {
    use super::super::super::common::test_harness::ApiTestHarness;
    use adapteros_api_types::{DatasetListResponse, UploadDatasetResponse, UserInfoResponse};
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use sqlx::Row;
    use tower::ServiceExt;

    fn build_multipart_body(boundary: &str, jsonl_bytes: &[u8]) -> Vec<u8> {
        let mut body = Vec::new();

        // name field
        body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"name\"\r\n\r\n");
        body.extend_from_slice(b"dev-login-dataset\r\n");

        // format field
        body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"format\"\r\n\r\n");
        body.extend_from_slice(b"jsonl\r\n");

        // file field
        body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
        body.extend_from_slice(
            b"Content-Disposition: form-data; name=\"file\"; filename=\"dataset.jsonl\"\r\n",
        );
        body.extend_from_slice(b"Content-Type: application/jsonl\r\n\r\n");
        body.extend_from_slice(jsonl_bytes);
        body.extend_from_slice(b"\r\n");

        // closing boundary
        body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());

        body
    }

    #[tokio::test]
    async fn test_dev_no_auth_login_and_dataset_creation() {
        std::env::set_var("AOS_DEV_NO_AUTH", "1");

        let harness = ApiTestHarness::new()
            .await
            .expect("Failed to initialize harness");

        // Step 1: Dev no-auth login verification
        let me_request = Request::builder()
            .method("GET")
            .uri("/v1/auth/me")
            .body(Body::empty())
            .expect("Failed to build /v1/auth/me request");

        let me_response = harness
            .app
            .clone()
            .oneshot(me_request)
            .await
            .expect("Failed to execute /v1/auth/me request");

        assert_eq!(me_response.status(), StatusCode::OK);

        let me_bytes = axum::body::to_bytes(me_response.into_body(), usize::MAX)
            .await
            .expect("Failed to read /v1/auth/me response body");
        let me_body: UserInfoResponse =
            serde_json::from_slice(&me_bytes).expect("Failed to parse /v1/auth/me response");

        assert_eq!(me_body.email, "dev-no-auth@adapteros.local");
        assert_eq!(me_body.role, "admin");
        assert_eq!(me_body.tenant_id, "default");

        // Step 2: Upload dataset via multipart
        let jsonl_payload = b"{\"prompt\":\"Hello\",\"response\":\"World\"}\n{\"prompt\":\"Ping\",\"response\":\"Pong\"}\n";
        let boundary = "XBOUNDARY_DEV_LOGIN_DATASET";
        let body = build_multipart_body(boundary, jsonl_payload);

        let upload_request = Request::builder()
            .method("POST")
            .uri("/v1/datasets")
            .header(
                "content-type",
                format!("multipart/form-data; boundary={}", boundary),
            )
            .body(Body::from(body))
            .expect("Failed to build /v1/datasets request");

        let upload_response = harness
            .app
            .clone()
            .oneshot(upload_request)
            .await
            .expect("Failed to execute /v1/datasets request");

        assert_eq!(upload_response.status(), StatusCode::OK);

        let upload_bytes = axum::body::to_bytes(upload_response.into_body(), usize::MAX)
            .await
            .expect("Failed to read /v1/datasets response body");
        let upload_body: UploadDatasetResponse = serde_json::from_slice(&upload_bytes)
            .expect("Failed to parse /v1/datasets response");

        assert_eq!(upload_body.name, "dev-login-dataset");
        assert_eq!(upload_body.file_count, 1);
        assert_eq!(upload_body.format, "jsonl");
        assert!(!upload_body.dataset_id.is_empty());

        // Step 3: Verify dataset appears in list
        let list_request = Request::builder()
            .method("GET")
            .uri("/v1/datasets")
            .body(Body::empty())
            .expect("Failed to build /v1/datasets list request");

        let list_response = harness
            .app
            .clone()
            .oneshot(list_request)
            .await
            .expect("Failed to execute /v1/datasets list request");

        assert_eq!(list_response.status(), StatusCode::OK);

        let list_bytes = axum::body::to_bytes(list_response.into_body(), usize::MAX)
            .await
            .expect("Failed to read /v1/datasets list response body");
        let list_body: DatasetListResponse =
            serde_json::from_slice(&list_bytes).expect("Failed to parse dataset list");

        let contains_dataset = list_body
            .datasets
            .iter()
            .any(|dataset| dataset.dataset_id == upload_body.dataset_id);
        assert!(
            contains_dataset,
            "Expected uploaded dataset to appear in list"
        );

        // Cleanup: remove dataset files if present
        let storage_path: Option<String> = sqlx::query("SELECT storage_path FROM training_datasets WHERE id = ?")
            .bind(&upload_body.dataset_id)
            .fetch_optional(harness.db().pool())
            .await
            .ok()
            .flatten()
            .map(|row| row.get(0));

        if let Some(path) = storage_path {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}
