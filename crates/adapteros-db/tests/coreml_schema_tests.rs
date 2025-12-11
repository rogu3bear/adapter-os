//! CoreML-specific schema validation for migrations 0173 and 0174.
//! Verifies the CoreML fusion registry table and replay metadata columns
//! are present with expected types after running all migrations.
use adapteros_db::Db;
use anyhow::Result;
use sqlx::Row;
use std::collections::HashMap;

async fn create_test_db() -> Result<Db> {
    Db::new_in_memory().await.map_err(Into::into)
}

async fn column_types(db: &Db, table: &str) -> Result<HashMap<String, String>> {
    let rows = sqlx::query(&format!("PRAGMA table_info({})", table))
        .fetch_all(db.pool())
        .await?;

    let mut types = HashMap::new();
    for row in rows {
        let name: String = row.get(1);
        let ty: String = row.get::<String, _>(2).to_uppercase();
        types.insert(name, ty);
    }
    Ok(types)
}

#[tokio::test]
async fn coreml_migrations_and_schema_are_applied() -> Result<()> {
    let db = create_test_db().await?;

    // Ensure migrations 0173 and 0174 were applied in order.
    let versions: Vec<i64> = sqlx::query_scalar(
        "SELECT version FROM _sqlx_migrations WHERE version IN (173, 174) ORDER BY version",
    )
    .fetch_all(db.pool())
    .await?;
    assert_eq!(
        versions,
        vec![173, 174],
        "CoreML migrations missing or out of order"
    );

    // coreml_fusion_pairs table schema
    let fusion_table_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='coreml_fusion_pairs')",
    )
    .fetch_one(db.pool())
    .await?;
    assert!(
        fusion_table_exists,
        "coreml_fusion_pairs table should exist"
    );

    let fusion_types = column_types(&db, "coreml_fusion_pairs").await?;
    let expected_fusion_columns = vec![
        ("id", "TEXT"),
        ("tenant_id", "TEXT"),
        ("base_model_id", "TEXT"),
        ("adapter_id", "TEXT"),
        ("fused_manifest_hash", "TEXT"),
        ("coreml_package_hash", "TEXT"),
        ("adapter_hash_b3", "TEXT"),
        ("base_model_hash_b3", "TEXT"),
        ("metadata_path", "TEXT"),
        ("created_at", "TEXT"),
    ];
    for (col, ty) in expected_fusion_columns {
        let actual = fusion_types
            .get(col)
            .unwrap_or_else(|| panic!("Missing column {} in coreml_fusion_pairs", col));
        assert_eq!(
            actual, ty,
            "Column {} in coreml_fusion_pairs should be type {}",
            col, ty
        );
    }

    // inference_replay_metadata CoreML verification columns
    let replay_types = column_types(&db, "inference_replay_metadata").await?;
    let expected_replay_columns = vec![
        ("coreml_package_hash", "TEXT"),
        ("coreml_expected_package_hash", "TEXT"),
        ("coreml_hash_mismatch", "INTEGER"),
    ];
    for (col, ty) in expected_replay_columns {
        let actual = replay_types
            .get(col)
            .unwrap_or_else(|| panic!("Missing column {} in inference_replay_metadata", col));
        assert_eq!(
            actual, ty,
            "Column {} in inference_replay_metadata should be type {}",
            col, ty
        );
    }

    Ok(())
}
