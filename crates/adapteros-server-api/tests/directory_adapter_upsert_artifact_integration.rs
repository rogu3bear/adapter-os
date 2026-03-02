//! Integration proof for directory adapter upsert artifact correctness.

mod common;

use adapteros_core::{tempdir_in_var, B3Hash};
use adapteros_db::sqlx;
use adapteros_server_api::{create_app, types::DirectoryUpsertResponse};
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn directory_upsert_rewrites_placeholder_and_repairs_stale_hash() {
    let _env = common::TestkitEnvGuard::enabled(true).await;
    let state = common::setup_state(None).await.expect("state");
    let app = create_app(state.clone());

    let source_root = tempdir_in_var("directory-upsert-artifact-integration-").expect("tempdir");
    let repository_root = source_root.path().join("repo");
    let relative_path = "workspace";
    let analysis_path = repository_root.join(relative_path);
    std::fs::create_dir_all(analysis_path.join("src")).expect("create source tree");
    std::fs::write(
        analysis_path.join("src/lib.rs"),
        r#"
pub fn adapter_feature_flag() -> bool {
    true
}
"#,
    )
    .expect("write sample rust source");
    std::fs::write(analysis_path.join("README.md"), "# sample\n").expect("write sample readme");

    let request_payload = json!({
        "tenant_id": "default",
        "root": repository_root.to_string_lossy(),
        "path": relative_path,
        "activate": false
    });

    let first_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/adapters/directory/upsert")
                .header("content-type", "application/json")
                .body(Body::from(request_payload.to_string()))
                .expect("first request"),
        )
        .await
        .expect("first upsert response");
    assert_eq!(first_response.status(), StatusCode::CREATED);
    let first_body = to_bytes(first_response.into_body(), 1024 * 1024)
        .await
        .expect("first response body");
    let first_payload: DirectoryUpsertResponse =
        serde_json::from_slice(&first_body).expect("first response payload");

    let adapters_root = {
        let config = state.config.read().expect("config lock");
        std::path::PathBuf::from(config.paths.adapters_root.clone())
    };
    let artifact_path = adapters_root
        .join("default")
        .join(format!("{}.safetensors", first_payload.adapter_id));
    assert!(
        artifact_path.exists(),
        "tenant-scoped artifact should exist at expected path"
    );
    assert!(
        !adapters_root
            .join("tenant-1")
            .join(format!("{}.safetensors", first_payload.adapter_id))
            .exists(),
        "artifact should not be written to another tenant path"
    );

    let first_artifact_bytes = std::fs::read(&artifact_path).expect("read first artifact");
    let first_artifact_hash = format!("b3:{}", B3Hash::hash(&first_artifact_bytes).to_hex());
    assert_eq!(
        first_payload.hash_b3, first_artifact_hash,
        "response hash must match persisted artifact bytes"
    );

    let stale_hash = "b3:stale-directory-upsert-hash";
    let rows_affected = sqlx::query(
        "UPDATE adapters
         SET hash_b3 = ?, content_hash_b3 = ?, updated_at = datetime('now')
         WHERE tenant_id = ? AND adapter_id = ?",
    )
    .bind(stale_hash)
    .bind(stale_hash)
    .bind("default")
    .bind(&first_payload.adapter_id)
    .execute(state.db.pool_result().expect("db pool"))
    .await
    .expect("seed stale hash")
    .rows_affected();
    assert_eq!(rows_affected, 1, "expected one adapter row to become stale");
    let stale_row = state
        .db
        .get_adapter_for_tenant("default", &first_payload.adapter_id)
        .await
        .expect("fetch stale row")
        .expect("adapter row should exist");
    assert_eq!(stale_row.hash_b3, stale_hash);

    let placeholder_bytes = b"synthetic adapter placeholder";
    std::fs::write(&artifact_path, placeholder_bytes).expect("write placeholder artifact");
    assert_eq!(
        std::fs::read(&artifact_path).expect("read placeholder artifact"),
        placeholder_bytes
    );

    let second_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/adapters/directory/upsert")
                .header("content-type", "application/json")
                .body(Body::from(request_payload.to_string()))
                .expect("second request"),
        )
        .await
        .expect("second upsert response");
    assert_eq!(second_response.status(), StatusCode::CREATED);
    let second_body = to_bytes(second_response.into_body(), 1024 * 1024)
        .await
        .expect("second response body");
    let second_payload: DirectoryUpsertResponse =
        serde_json::from_slice(&second_body).expect("second response payload");
    assert_eq!(
        second_payload.adapter_id, first_payload.adapter_id,
        "directory fingerprint should produce deterministic adapter id"
    );

    let rewritten_artifact_bytes = std::fs::read(&artifact_path).expect("read rewritten artifact");
    assert_ne!(
        rewritten_artifact_bytes, placeholder_bytes,
        "upsert should rewrite placeholder artifact bytes"
    );
    let tensors = safetensors::SafeTensors::deserialize(&rewritten_artifact_bytes)
        .expect("valid safetensors");
    assert!(tensors.tensor("lora_a").is_ok(), "lora_a tensor missing");
    assert!(tensors.tensor("lora_b").is_ok(), "lora_b tensor missing");

    let rewritten_hash = format!("b3:{}", B3Hash::hash(&rewritten_artifact_bytes).to_hex());
    assert_eq!(
        second_payload.hash_b3, rewritten_hash,
        "second upsert response hash must match rewritten artifact bytes"
    );

    let repaired_row = state
        .db
        .get_adapter_for_tenant("default", &second_payload.adapter_id)
        .await
        .expect("fetch repaired row")
        .expect("adapter row should exist");
    assert_eq!(
        repaired_row.hash_b3, rewritten_hash,
        "stale DB hash should be auto-repaired to artifact hash"
    );
    assert_eq!(
        repaired_row.content_hash_b3.as_deref(),
        Some(rewritten_hash.as_str()),
        "content hash should be repaired together with weight hash"
    );
}
