use adapteros_db::{sqlx, Db};
use anyhow::Result;

#[tokio::test]
async fn latest_trusted_version_skips_blocked_head() -> Result<()> {
    let db = Db::new_in_memory().await?;
    let tenant_id = db.create_tenant("Tenant 1", false).await?;

    let dataset_id = db
        .create_training_dataset(
            "ds-trust",
            Some("test dataset"),
            "jsonl",
            "hash-ds",
            "var/ds-trust",
            None,
            None,
            Some("ready"),
            Some("hash-ds"),
            None,
        )
        .await?;

    sqlx::query("UPDATE training_datasets SET tenant_id = ? WHERE id = ?")
        .bind(&tenant_id)
        .bind(&dataset_id)
        .execute(db.pool())
        .await?;

    // v1 is valid and should be trusted
    let v1 = db
        .create_training_dataset_version(
            &dataset_id,
            Some(&tenant_id),
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
    db.update_dataset_version_safety_status(
        &v1,
        Some("none"),
        Some("none"),
        Some("none"),
        Some("none"),
    )
    .await?;

    // v2 is newer but invalid -> trust_state should be blocked
    let v2 = db
        .create_training_dataset_version(
            &dataset_id,
            Some(&tenant_id),
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

    let v1_record = db
        .get_training_dataset_version(&v1)
        .await?
        .expect("v1 should exist");
    let v2_record = db
        .get_training_dataset_version(&v2)
        .await?
        .expect("v2 should exist");

    assert_eq!(v1_record.trust_state, "allowed");
    assert_eq!(v2_record.trust_state, "blocked");

    let latest = db
        .get_latest_trusted_dataset_version_for_dataset(&dataset_id)
        .await?
        .expect("trusted version should exist");

    assert_eq!(latest.0.id, v1, "Should pick last trusted version");
    assert_eq!(latest.1, "allowed");

    Ok(())
}

#[tokio::test]
async fn version_storage_path_prefers_single_file_match() -> Result<()> {
    let db = Db::new_in_memory().await?;

    let dataset_id = db
        .create_training_dataset(
            "ds-single-file",
            None,
            "jsonl",
            "hash-file",
            "var/ds-single-file",
            None,
            None,
            Some("ready"),
            Some("hash-file"),
            None,
        )
        .await?;

    db.add_dataset_file(
        &dataset_id,
        "data.jsonl",
        "var/ds-single-file/data.jsonl",
        123,
        "hash-file",
        Some("application/jsonl"),
    )
    .await?;

    let version_id = db
        .create_training_dataset_version(
            &dataset_id,
            None,
            None,
            "var/ds-single-file",
            "hash-file",
            None,
            None,
            None,
        )
        .await?;

    let version = db
        .get_training_dataset_version(&version_id)
        .await?
        .expect("version should exist");

    assert_eq!(version.storage_path, "var/ds-single-file/data.jsonl");

    Ok(())
}
