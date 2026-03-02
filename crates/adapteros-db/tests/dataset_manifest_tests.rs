use adapteros_core::AosError;
use adapteros_db::{sqlx, Db};
use anyhow::Result;

#[tokio::test]
async fn manifest_helpers_round_trip_and_enforce_tenant() -> Result<()> {
    let db = Db::new_in_memory().await?;

    let tenant_a = db.create_tenant("Tenant A", false).await?;
    let tenant_b = db.create_tenant("Tenant B", false).await?;

    let dataset_id = db
        .create_training_dataset(
            "ds",
            None,
            "jsonl",
            "hash",
            "var/ds",
            None,
            None,
            Some("ready"),
            Some("hash"),
            None,
        )
        .await?;

    // Attach dataset to tenant for FK guards used by downstream writes
    sqlx::query("UPDATE training_datasets SET tenant_id = ? WHERE id = ?")
        .bind(&tenant_a)
        .bind(&dataset_id)
        .execute(db.pool_result()?)
        .await?;

    let version_id = db
        .create_training_dataset_version_with_id(
            "ver-1",
            &dataset_id,
            Some(&tenant_a),
            Some("v1"),
            "var/ds/canonical.jsonl",
            "hash",
            Some("var/ds/manifest.json"),
            Some(r#"{"total_rows":1}"#),
            Some("tester"),
        )
        .await?;

    let manifest = db
        .get_dataset_version_manifest(&version_id)
        .await?
        .expect("manifest should exist");
    assert_eq!(manifest, r#"{"total_rows":1}"#);

    let version_ok = db
        .get_training_dataset_version_for_tenant(&version_id, &tenant_a)
        .await?;
    assert!(version_ok.is_some());

    let err = db
        .get_training_dataset_version_for_tenant(&version_id, &tenant_b)
        .await
        .unwrap_err();
    assert!(matches!(err, AosError::Authz(_)));

    Ok(())
}
