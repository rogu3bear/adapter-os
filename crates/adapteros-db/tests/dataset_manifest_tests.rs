use adapteros_core::AosError;
use adapteros_db::Db;
use anyhow::Result;

#[tokio::test]
async fn manifest_helpers_round_trip_and_enforce_tenant() -> Result<()> {
    let db = Db::new_in_memory().await?;

    let dataset_id = db
        .create_training_dataset("ds", None, "jsonl", "hash", "/tmp/ds", None)
        .await?;

    let version_id = db
        .create_training_dataset_version_with_id(
            "ver-1",
            &dataset_id,
            Some("tenant-a"),
            Some("v1"),
            "/tmp/ds/canonical.jsonl",
            "hash",
            Some("/tmp/ds/manifest.json"),
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
        .get_training_dataset_version_for_tenant(&version_id, "tenant-a")
        .await?;
    assert!(version_ok.is_some());

    let err = db
        .get_training_dataset_version_for_tenant(&version_id, "tenant-b")
        .await
        .unwrap_err();
    assert!(matches!(err, AosError::Authz(_)));

    Ok(())
}
