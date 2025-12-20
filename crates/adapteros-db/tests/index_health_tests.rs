mod common;

use common::db_helpers::create_test_db;

#[tokio::test]
async fn sqlite_page_stats_collects() -> adapteros_core::Result<()> {
    let db = create_test_db().await?;
    let stats = db
        .collect_sqlite_page_stats()
        .await?
        .expect("expected SQL pool attached");

    assert!(stats.page_size_bytes > 0);
    assert!(stats.page_count > 0);
    assert!(stats.db_size_estimate_bytes > 0);
    Ok(())
}

#[tokio::test]
async fn tenant_index_coverage_detects_leading_tenant_id() -> adapteros_core::Result<()> {
    let db = create_test_db().await?;

    sqlx::query(
        r#"
        CREATE TABLE tenant_index_ok (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            created_at TEXT NOT NULL
        )
        "#,
    )
    .execute(db.pool())
    .await?;
    sqlx::query(
        "CREATE INDEX idx_tenant_index_ok_tenant ON tenant_index_ok(tenant_id, created_at)",
    )
    .execute(db.pool())
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE tenant_index_bad (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            created_at TEXT NOT NULL
        )
        "#,
    )
    .execute(db.pool())
    .await?;
    sqlx::query(
        "CREATE INDEX idx_tenant_index_bad_nonleading ON tenant_index_bad(created_at, tenant_id)",
    )
    .execute(db.pool())
    .await?;

    let coverage = db
        .collect_tenant_index_coverage(&["tenant_index_ok", "tenant_index_bad"])
        .await?;

    let ok = coverage
        .iter()
        .find(|c| c.table == "tenant_index_ok")
        .expect("expected tenant_index_ok coverage row");
    assert!(ok.table_exists);
    assert!(ok.has_tenant_id_column);
    assert!(ok.has_leading_tenant_id_index);

    let bad = coverage
        .iter()
        .find(|c| c.table == "tenant_index_bad")
        .expect("expected tenant_index_bad coverage row");
    assert!(bad.table_exists);
    assert!(bad.has_tenant_id_column);
    assert!(!bad.has_leading_tenant_id_index);

    Ok(())
}

#[tokio::test]
async fn dbstat_index_summary_is_optional() -> adapteros_core::Result<()> {
    let db = create_test_db().await?;
    let summary = db.collect_dbstat_index_summary(5).await?;
    let _ = summary; // May be None if SQLite dbstat vtab is not enabled.
    Ok(())
}

#[tokio::test]
async fn sqlite_index_maintenance_helpers_run() -> adapteros_core::Result<()> {
    let db = create_test_db().await?;

    sqlx::query(
        r#"
        CREATE TABLE tenant_index_maint (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            created_at TEXT NOT NULL
        )
        "#,
    )
    .execute(db.pool())
    .await?;
    sqlx::query("CREATE INDEX idx_tenant_index_maint ON tenant_index_maint(tenant_id, created_at)")
        .execute(db.pool())
        .await?;

    db.sqlite_optimize().await?;
    db.sqlite_analyze_tables(&["tenant_index_maint"]).await?;
    db.sqlite_reindex_tables(&["tenant_index_maint"]).await?;

    Ok(())
}
