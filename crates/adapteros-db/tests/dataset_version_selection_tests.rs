use adapteros_db::Db;
use anyhow::Result;

#[tokio::test]
async fn latest_trusted_version_skips_blocked_head() -> Result<()> {
    let db = Db::new_in_memory().await?;

    let dataset_id = db
        .create_training_dataset(
            "ds-trust",
            Some("test dataset"),
            "jsonl",
            "hash-ds",
            "var/ds-trust",
            Some("tester"),
        )
        .await?;

    // v1 is valid and should be trusted
    let v1 = db
        .create_training_dataset_version(
            &dataset_id,
            Some("tenant-1"),
            Some("v1"),
            "var/ds-trust/v1",
            "hash-v1",
            None,
            None,
            Some("tester"),
        )
        .await?;
    db.update_dataset_version_structural_validation(&v1, "valid", None)
        .await?;

    // v2 is newer but invalid -> trust_state should be blocked
    let v2 = db
        .create_training_dataset_version(
            &dataset_id,
            Some("tenant-1"),
            Some("v2"),
            "var/ds-trust/v2",
            "hash-v2",
            None,
            None,
            Some("tester"),
        )
        .await?;
    db.update_dataset_version_structural_validation(&v2, "invalid", Some("bad"))
        .await?;

    let latest = db
        .get_latest_trusted_dataset_version_for_dataset(&dataset_id)
        .await?
        .expect("trusted version should exist");

    assert_eq!(latest.0.id, v1, "Should pick last trusted version");
    assert_eq!(latest.1, "allowed");

    Ok(())
}
