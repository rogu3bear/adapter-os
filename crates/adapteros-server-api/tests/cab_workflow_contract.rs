use adapteros_core::B3Hash;
use adapteros_crypto::Keypair;
use adapteros_server_api::cab_workflow::CABWorkflow;
use sqlx::{Row, SqlitePool};

#[tokio::test]
async fn cab_workflow_uses_schema_valid_columns_and_sqlite_sql() {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite pool should initialize");

    sqlx::query(
        "CREATE TABLE plans (
            cpid TEXT PRIMARY KEY,
            plan_id_b3 TEXT NOT NULL,
            metallib_hash_b3 TEXT,
            kernel_hashes_json TEXT NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .expect("plans table should be created");

    sqlx::query(
        "CREATE TABLE bundle_signatures (
            id TEXT PRIMARY KEY,
            cpid TEXT NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .expect("bundle_signatures table should be created");

    sqlx::query(
        "CREATE TABLE replay_test_bundles (
            test_bundle_id TEXT PRIMARY KEY,
            cpid TEXT NOT NULL,
            test_name TEXT NOT NULL,
            expected_output TEXT NOT NULL,
            expected_hash TEXT NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .expect("replay_test_bundles table should be created");

    sqlx::query(
        "CREATE TABLE cab_approvals (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            cpid TEXT NOT NULL,
            approver TEXT NOT NULL,
            approval_message TEXT NOT NULL,
            signature TEXT NOT NULL,
            public_key TEXT NOT NULL,
            approved_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
    .execute(&pool)
    .await
    .expect("cab_approvals table should be created");

    sqlx::query(
        "CREATE TABLE cp_pointers (
            name TEXT PRIMARY KEY,
            active_cpid TEXT,
            before_cpid TEXT,
            approval_signature TEXT,
            promoted_at TEXT
        )",
    )
    .execute(&pool)
    .await
    .expect("cp_pointers table should be created");

    sqlx::query(
        "CREATE TABLE promotion_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            cpid TEXT NOT NULL,
            status TEXT NOT NULL,
            approval_signature TEXT NOT NULL,
            before_cpid TEXT,
            promoted_at TEXT NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .expect("promotion_history table should be created");

    sqlx::query("INSERT INTO cp_pointers (name, active_cpid, before_cpid) VALUES (?, ?, ?)")
        .bind("production")
        .bind("cpid-old")
        .bind("cpid-older")
        .execute(&pool)
        .await
        .expect("cp pointer should be seeded");

    sqlx::query(
        "INSERT INTO plans (cpid, plan_id_b3, metallib_hash_b3, kernel_hashes_json) VALUES (?, ?, ?, ?)",
    )
    .bind("cpid-new")
    .bind("plan-b3-new")
    .bind("metallib-hash-new")
    .bind(r#"["kernel-hash-1"]"#)
    .execute(&pool)
    .await
    .expect("plan should be seeded");

    sqlx::query("INSERT INTO bundle_signatures (id, cpid) VALUES (?, ?)")
        .bind("sig-1")
        .bind("cpid-new")
        .execute(&pool)
        .await
        .expect("bundle signature should be seeded");

    let expected_output = "deterministic replay output";
    let expected_hash = B3Hash::hash(expected_output.as_bytes()).to_hex();
    sqlx::query(
        "INSERT INTO replay_test_bundles (test_bundle_id, cpid, test_name, expected_output, expected_hash) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("bundle-1")
    .bind("cpid-new")
    .bind("replay-1")
    .bind(expected_output)
    .bind(expected_hash)
    .execute(&pool)
    .await
    .expect("replay bundle should be seeded");

    let workflow = CABWorkflow::new(pool.clone(), Keypair::generate());
    let result = workflow
        .promote_cpid("cpid-new", "approver@example.com")
        .await
        .expect("workflow should succeed with schema-valid fixtures");

    assert!(result.hash_validation.valid, "hash validation should pass");
    assert!(result.replay_result.passed, "replay validation should pass");
    assert_eq!(result.promotion_record.cpid, "cpid-new");

    let pointer_row = sqlx::query(
        "SELECT active_cpid, before_cpid, approval_signature, promoted_at FROM cp_pointers WHERE name = ?",
    )
    .bind("production")
    .fetch_one(&pool)
    .await
    .expect("updated cp pointer should exist");

    let active_cpid: Option<String> = pointer_row.try_get("active_cpid").ok();
    let approval_signature: Option<String> = pointer_row.try_get("approval_signature").ok();
    let promoted_at: Option<String> = pointer_row.try_get("promoted_at").ok();

    assert_eq!(active_cpid.as_deref(), Some("cpid-new"));
    assert!(
        approval_signature.as_ref().is_some_and(|s| !s.is_empty()),
        "approval signature should be recorded"
    );
    assert!(
        promoted_at.as_ref().is_some_and(|s| !s.is_empty()),
        "promotion timestamp should be set via sqlite datetime()"
    );
}
