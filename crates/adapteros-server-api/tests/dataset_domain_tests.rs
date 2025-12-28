use std::sync::Arc;

use adapteros_core::AosError;
use adapteros_server_api::services::dataset_domain::{
    DatasetDomain, DatasetDomainService, RawDialect, RawFileDescriptor, RawIngestRequest,
};
use adapteros_server_api::services::SamplingConfig;
use adapteros_server_api::state::AppState;
use adapteros_server_api::types::{CanonicalRow, DatasetManifest};
mod common;
use common::{create_test_tenant, test_admin_claims};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::OnceCell;
use uuid::Uuid;

static STATE: OnceCell<AppState> = OnceCell::const_new();

async fn state() -> AppState {
    STATE
        .get_or_init(|| async {
            common::setup_state(None)
                .await
                .expect("state setup should succeed")
        })
        .await
        .clone()
}

async fn create_dataset(state: &AppState) -> String {
    let dataset_id = format!("ds-{}", Uuid::now_v7());
    state
        .db
        .create_training_dataset_with_id(
            &dataset_id,
            "test dataset",
            None,
            "jsonl",
            "hash",
            "var/raw",
            None,
            None,
            Some("ready"),
            Some("hash"),
        )
        .await
        .expect("dataset created");
    dataset_id
}

async fn write_jsonl(lines: &[&str]) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::TempDir::new_in(".").expect("tempdir");
    let path = dir.path().join("data.jsonl");
    let mut file = fs::File::create(&path).await.expect("file create");
    for line in lines {
        file.write_all(line.as_bytes()).await.expect("write line");
        file.write_all(b"\n").await.expect("newline");
    }
    file.flush().await.expect("flush");
    (dir, path)
}

#[tokio::test]
#[ignore = "requires dataset storage path setup"]
async fn manifest_and_streaming_are_deterministic_and_tenant_safe() {
    let state = state().await;
    let dataset_id = create_dataset(&state).await;
    let claims = test_admin_claims();
    create_test_tenant(&state, &claims.tenant_id, "tenant-1")
        .await
        .expect("tenant created");

    let (_jsonl_dir, jsonl_path) = write_jsonl(&[
        r#"{"prompt":"p1","response":"r1","split":"train"}"#,
        r#"{"prompt":"p2","response":"r2","split":"eval"}"#,
    ])
    .await;

    let service = DatasetDomainService::new(Arc::new(state.clone()));
    let descriptor = service
        .ingest_raw_dataset(RawIngestRequest {
            tenant_id: claims.tenant_id.clone(),
            dataset_id: dataset_id.clone(),
            version_label: Some("v1".into()),
            created_by: Some(claims.sub.clone()),
            files: vec![RawFileDescriptor {
                path: jsonl_path,
                format: RawDialect::CanonicalJsonl,
                split: None,
            }],
        })
        .await
        .expect("ingest succeeds");

    let manifest: DatasetManifest = service
        .get_manifest(&descriptor.dataset_version_id, &claims.tenant_id)
        .await
        .expect("manifest read")
        .expect("manifest exists");
    assert_eq!(manifest.total_rows, 2);
    assert!(manifest.splits.contains_key("train"));
    assert!(manifest.splits.contains_key("eval"));

    let rows_seed_a: Vec<CanonicalRow> = service
        .stream_rows(
            &descriptor.dataset_version_id,
            &claims.tenant_id,
            SamplingConfig {
                split: Some("train".into()),
                shuffle_seed: Some("seed-a".into()),
            },
        )
        .await
        .expect("stream rows");
    assert_eq!(rows_seed_a.len(), 1);
    assert_eq!(rows_seed_a[0].prompt, "p1");

    let rows_seed_a_again = service
        .stream_rows(
            &descriptor.dataset_version_id,
            &claims.tenant_id,
            SamplingConfig {
                split: Some("train".into()),
                shuffle_seed: Some("seed-a".into()),
            },
        )
        .await
        .expect("stream rows again");
    assert_eq!(rows_seed_a, rows_seed_a_again);

    let err = service
        .stream_rows(
            &descriptor.dataset_version_id,
            "other-tenant",
            SamplingConfig {
                split: None,
                shuffle_seed: None,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, AosError::Authz(_)));
}
