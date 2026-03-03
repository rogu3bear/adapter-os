use adapteros_core::Result;
use adapteros_db::adapters::AdapterRegistrationBuilder;
use adapteros_db::Db;
use sqlx::Row;
use tempfile::tempdir;

async fn setup_db_with_tenant() -> Db {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('default-tenant', 'Default')")
        .execute(db.pool_result().unwrap())
        .await
        .unwrap();
    db
}

async fn check_active_aos_invariants(db: &Db) -> Result<()> {
    let rows = sqlx::query(
        "SELECT adapter_id, aos_file_path, aos_file_hash FROM adapters WHERE active = 1",
    )
    .fetch_all(db.pool_result().unwrap())
    .await
    .map_err(|e| adapteros_core::AosError::Database(e.to_string()))?;

    for row in rows {
        let adapter_id: Option<String> = row.try_get("adapter_id").unwrap();
        let aos_path: Option<String> = row.try_get("aos_file_path").unwrap();
        let aos_hash: Option<String> = row.try_get("aos_file_hash").unwrap();

        let adapter_id = adapter_id.unwrap_or_else(|| "<unknown>".to_string());
        let path = aos_path.ok_or_else(|| {
            adapteros_core::AosError::Validation(format!(
                "Adapter {} missing aos_file_path",
                adapter_id
            ))
        })?;
        let expected_hash = aos_hash.ok_or_else(|| {
            adapteros_core::AosError::Validation(format!(
                "Adapter {} missing aos_file_hash",
                adapter_id
            ))
        })?;

        let data = std::fs::read(&path).map_err(|e| {
            adapteros_core::AosError::Io(format!(
                "Adapter {} missing bundle at {}: {}",
                adapter_id, path, e
            ))
        })?;
        let actual_hash = blake3::hash(&data).to_hex().to_string();
        if expected_hash != actual_hash {
            return Err(adapteros_core::AosError::Validation(format!(
                "Adapter {} aos_file_hash mismatch (stored {}, computed {})",
                adapter_id, expected_hash, actual_hash
            )));
        }
    }

    Ok(())
}

#[tokio::test]
async fn active_adapters_require_aos_path_and_matching_hash() {
    let db = setup_db_with_tenant().await;
    let tmp = tempdir().unwrap();
    let aos_path = tmp.path().join("docs_adapter.aos");
    std::fs::write(&aos_path, b"dummy-aos-bytes").unwrap();
    let aos_hash = blake3::hash(b"dummy-aos-bytes").to_hex().to_string();

    let params = AdapterRegistrationBuilder::new()
        .adapter_id("docs-adapter")
        .name("Docs Adapter")
        .hash_b3("b3:dummy-weights")
        .rank(8)
        .tier("warm")
        .aos_file_path(Some(aos_path.to_string_lossy()))
        .aos_file_hash(Some(&aos_hash))
        .build()
        .unwrap();

    db.register_adapter_extended(params).await.unwrap();

    check_active_aos_invariants(&db).await.unwrap();
}

#[tokio::test]
async fn invariant_detects_hash_mismatch() {
    let db = setup_db_with_tenant().await;
    let tmp = tempdir().unwrap();
    let aos_path = tmp.path().join("docs_adapter_mismatch.aos");
    std::fs::write(&aos_path, b"dummy-aos-bytes").unwrap();
    let aos_hash = blake3::hash(b"dummy-aos-bytes").to_hex().to_string();

    let params = AdapterRegistrationBuilder::new()
        .adapter_id("docs-adapter-mismatch")
        .name("Docs Adapter Mismatch")
        .hash_b3("b3:dummy-weights")
        .rank(8)
        .tier("warm")
        .aos_file_path(Some(aos_path.to_string_lossy()))
        .aos_file_hash(Some(&aos_hash))
        .build()
        .unwrap();

    db.register_adapter_extended(params).await.unwrap();

    // Tamper stored hash
    sqlx::query(
        "UPDATE adapters SET aos_file_hash = 'deadbeef' WHERE adapter_id = 'docs-adapter-mismatch'",
    )
    .execute(db.pool_result().unwrap())
    .await
    .unwrap();

    let err = check_active_aos_invariants(&db).await.err().unwrap();
    let msg = err.to_string();
    assert!(
        msg.contains("aos_file_hash mismatch"),
        "expected hash mismatch error, got {}",
        msg
    );
}
