use adapteros_api_types::adapters::AdapterHealthFlag;
use adapteros_db::adapters::AdapterRegistrationBuilder;
use adapteros_db::sqlx;
use adapteros_db::{CreateRepositoryParams, CreateVersionParams};
use adapteros_server_api::handlers::get_adapter_health;
use axum::{extract::Path, extract::State, http::StatusCode, Extension};
use tokio::time::Duration;

mod common;
use common::{setup_state, test_admin_claims};

const METRIC_ADAPTER_HEALTH_CORRUPT: &str = "adapter_versions_health_corrupt";
const METRIC_ADAPTER_HEALTH_UNSAFE: &str = "adapter_versions_health_unsafe";

fn base_adapter_params(
    tenant_id: &str,
    adapter_id: &str,
) -> adapteros_db::adapters::AdapterRegistrationParams {
    AdapterRegistrationBuilder::new()
        .tenant_id(tenant_id)
        .adapter_id(adapter_id)
        .name(adapter_id)
        .hash_b3("adapter-hash")
        .rank(4)
        .targets_json(r#"["q_proj"]"#)
        .content_hash_b3(Some(format!("content-hash-{}", adapter_id)))
        .build()
        .expect("adapter params")
}

#[tokio::test]
async fn corrupt_storage_emits_corrupt_metric() {
    let state = setup_state(None).await.expect("state");
    let claims = test_admin_claims();
    let adapter_id = "adapter-health-corrupt";
    let params = base_adapter_params(&claims.tenant_id, adapter_id);
    state
        .db
        .register_adapter(params)
        .await
        .expect("register adapter");

    sqlx::query(
        r#"
        INSERT INTO storage_reconciliation_issues (
            id, tenant_id, owner_type, owner_id, version_id,
            issue_type, severity, path, expected_hash, actual_hash, message
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind("issue-corrupt")
    .bind(&claims.tenant_id)
    .bind("adapter")
    .bind(adapter_id)
    .bind(adapter_id)
    .bind("missing_file")
    .bind("error")
    .bind("var/adapters/adapter.aos")
    .bind(Some("expected"))
    .bind(Some("actual"))
    .bind(Some("test corruption"))
    .execute(state.db.pool())
    .await
    .expect("insert issue");

    let response = get_adapter_health(
        State(state.clone()),
        Extension(claims),
        Path(adapter_id.to_string()),
    )
    .await
    .expect("health response")
    .0;

    assert_eq!(response.health, AdapterHealthFlag::Corrupt);

    tokio::time::sleep(Duration::from_millis(10)).await;
    let count = state
        .metrics_registry
        .get_series_async(METRIC_ADAPTER_HEALTH_CORRUPT)
        .await
        .map(|s| s.get_points(None, None).len())
        .unwrap_or(0);
    assert_eq!(count, 1);
}

#[tokio::test]
async fn blocked_trust_emits_unsafe_metric() {
    let state = setup_state(None).await.expect("state");
    let claims = test_admin_claims();
    // Create a blocked dataset version and link it to the adapter version.
    let dataset_id = "ds-unsafe";
    let dataset_version_id = "dsv-unsafe";
    state
        .db
        .create_training_dataset_with_id(
            dataset_id,
            "ds",
            Some("desc"),
            "jsonl",
            "hash-unsafe",
            "var/ds",
            Some(&claims.sub),
            None,
            Some("ready"),
            Some("hash-unsafe"),
            None,
        )
        .await
        .expect("dataset");
    sqlx::query("UPDATE training_datasets SET tenant_id = ? WHERE id = ?")
        .bind(&claims.tenant_id)
        .bind(dataset_id)
        .execute(state.db.pool())
        .await
        .expect("dataset tenant");
    state
        .db
        .create_training_dataset_version_with_id(
            dataset_version_id,
            dataset_id,
            Some(&claims.tenant_id),
            Some("v1"),
            "var/ds/v1",
            "hash-unsafe",
            None,
            None,
            Some(&claims.sub),
        )
        .await
        .expect("dataset version");
    sqlx::query(
        "UPDATE training_dataset_versions SET trust_state = 'blocked', overall_trust_status = 'blocked' WHERE id = ?",
    )
    .bind(dataset_version_id)
    .execute(state.db.pool())
    .await
    .expect("mark blocked");

    let repo_id = state
        .db
        .create_adapter_repository(CreateRepositoryParams {
            tenant_id: &claims.tenant_id,
            name: "adapter-health-unsafe",
            base_model_id: None,
            default_branch: Some("main"),
            created_by: Some(&claims.sub),
            description: None,
        })
        .await
        .expect("create repo");
    let dataset_version_ids = vec![dataset_version_id.to_string()];
    let adapter_version_id = state
        .db
        .create_adapter_version(CreateVersionParams {
            repo_id: &repo_id,
            tenant_id: &claims.tenant_id,
            version: "v1",
            branch: "main",
            branch_classification: "sandbox",
            aos_path: None,
            aos_hash: None,
            manifest_schema_version: None,
            parent_version_id: None,
            code_commit_sha: None,
            data_spec_hash: None,
            training_backend: None,
            coreml_used: None,
            coreml_device_type: None,
            dataset_version_ids: Some(&dataset_version_ids),
            release_state: "draft",
            metrics_snapshot_id: None,
            evaluation_summary: None,
            allow_archived: false,
            actor: Some(&claims.sub),
            reason: Some("adapter health test"),
            train_job_id: None,
        })
        .await
        .expect("create adapter version");

    let adapter_id = adapter_version_id.clone();
    let params = base_adapter_params(&claims.tenant_id, &adapter_id);
    state
        .db
        .register_adapter(params)
        .await
        .expect("register adapter");

    let response = get_adapter_health(
        State(state.clone()),
        Extension(claims),
        Path(adapter_id.to_string()),
    )
    .await
    .expect("health response")
    .0;

    assert_eq!(response.health, AdapterHealthFlag::Unsafe);

    tokio::time::sleep(Duration::from_millis(10)).await;
    let count = state
        .metrics_registry
        .get_series_async(METRIC_ADAPTER_HEALTH_UNSAFE)
        .await
        .map(|s| s.get_points(None, None).len())
        .unwrap_or(0);
    assert_eq!(count, 1);
}

#[tokio::test]
async fn tenant_cannot_read_other_tenant_adapter_health() {
    let state = setup_state(None).await.expect("state");
    let claims_a = test_admin_claims();
    let adapter_id = "adapter-health-isolation";
    let params = base_adapter_params(&claims_a.tenant_id, adapter_id);
    state
        .db
        .register_adapter(params)
        .await
        .expect("register adapter");

    let mut claims_b = claims_a.clone();
    claims_b.tenant_id = "tenant-2".to_string();

    let result = get_adapter_health(
        State(state.clone()),
        Extension(claims_b),
        Path(adapter_id.to_string()),
    )
    .await;

    assert!(
        result.is_err(),
        "Cross-tenant adapter health access must be rejected"
    );
    let (status, _) = result.unwrap_err();
    assert_eq!(status, StatusCode::NOT_FOUND);
}
