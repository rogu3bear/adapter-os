//! Integration tests for H4: Heartbeat Mechanism
//!
//! Tests 5-minute stale adapter timeout and auto-recovery.

use adapteros_core::B3Hash;
use adapteros_db::Db;
use adapteros_lora_lifecycle::LifecycleManager;
use adapteros_manifest::Policies;
use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::TempDir;

fn new_test_adapters_dir() -> TempDir {
    let base_dir = PathBuf::from("var").join("tmp");
    let _ = std::fs::create_dir_all(&base_dir);
    tempfile::Builder::new()
        .prefix("lifecycle_test_")
        .tempdir_in(&base_dir)
        .expect("tempdir")
}

#[tokio::test]
async fn test_h4_heartbeat_updates_timestamp() {
    let db = Db::new_in_memory().await.unwrap();
    let tenant_id = db.create_tenant("system", false).await.unwrap();

    let params = adapteros_db::adapters::AdapterRegistrationBuilder::new()
        .tenant_id(&tenant_id)
        .adapter_id("heartbeat-adapter")
        .name("Heartbeat Adapter")
        .hash_b3("hb123")
        .rank(8)
        .tier("persistent")
        .build()
        .unwrap();
    db.register_adapter(params).await.unwrap();

    let adapter_names = vec!["heartbeat-adapter".to_string()];
    let mut hashes = HashMap::new();
    hashes.insert("heartbeat-adapter".to_string(), B3Hash::hash(b"hb"));

    let policies = Policies::default();
    let temp_dir = new_test_adapters_dir();
    let manager = LifecycleManager::new_with_db(
        adapter_names,
        hashes,
        &policies,
        temp_dir.path().to_path_buf(),
        None,
        3,
        db.clone(),
    );

    // Send heartbeat
    manager
        .heartbeat_adapter("heartbeat-adapter")
        .await
        .unwrap();

    // Verify no stale adapters (300 second threshold)
    let stale = manager.check_stale_adapters(300).await.unwrap();
    assert!(stale.is_empty());
}

#[tokio::test]
async fn test_h4_5_minute_stale_detection() {
    let db = Db::new_in_memory().await.unwrap();
    let tenant_id = db.create_tenant("system", false).await.unwrap();

    let params = adapteros_db::adapters::AdapterRegistrationBuilder::new()
        .tenant_id(&tenant_id)
        .adapter_id("stale-adapter")
        .name("Stale Adapter")
        .hash_b3("stale123")
        .rank(8)
        .tier("persistent")
        .build()
        .unwrap();
    db.register_adapter(params).await.unwrap();

    let adapter_names = vec!["stale-adapter".to_string()];
    let mut hashes = HashMap::new();
    hashes.insert("stale-adapter".to_string(), B3Hash::hash(b"stale"));

    let policies = Policies::default();
    let temp_dir = new_test_adapters_dir();
    let manager = LifecycleManager::new_with_db(
        adapter_names,
        hashes,
        &policies,
        temp_dir.path().to_path_buf(),
        None,
        3,
        db.clone(),
    );

    // Set old heartbeat (400 seconds ago > 300 second threshold)
    let old_timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        - 400;

    sqlx::query("UPDATE adapters SET last_heartbeat = ?, load_state = 'cold' WHERE adapter_id = ?")
        .bind(old_timestamp)
        .bind("stale-adapter")
        .execute(db.pool())
        .await
        .unwrap();

    // Check for stale adapters (300 second = 5 minute threshold)
    let stale = manager.check_stale_adapters(300).await.unwrap();
    // Expect the single fixture adapter to be detected as stale
    assert_eq!(stale.len(), 1, "expected one stale adapter");
}

#[tokio::test]
async fn test_h4_auto_recovery_to_unloaded() {
    let db = Db::new_in_memory().await.unwrap();
    let tenant_id = db.create_tenant("system", false).await.unwrap();

    let params = adapteros_db::adapters::AdapterRegistrationBuilder::new()
        .tenant_id(&tenant_id)
        .adapter_id("recover-adapter")
        .name("Recover Adapter")
        .hash_b3("recover123")
        .rank(8)
        .tier("persistent")
        .build()
        .unwrap();
    db.register_adapter(params).await.unwrap();

    let adapter_names = vec!["recover-adapter".to_string()];
    let mut hashes = HashMap::new();
    hashes.insert("recover-adapter".to_string(), B3Hash::hash(b"recover"));

    let policies = Policies::default();
    let temp_dir = new_test_adapters_dir();
    let manager = LifecycleManager::new_with_db(
        adapter_names,
        hashes,
        &policies,
        temp_dir.path().to_path_buf(),
        None,
        3,
        db.clone(),
    );

    // Set old heartbeat and loaded state
    let old_timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        - 400;

    sqlx::query("UPDATE adapters SET last_heartbeat = ?, load_state = 'warm' WHERE adapter_id = ?")
        .bind(old_timestamp)
        .bind("recover-adapter")
        .execute(db.pool())
        .await
        .unwrap();

    // Recover stale adapters
    let recovered = manager.recover_stale_adapters(300).await.unwrap();
    assert_eq!(recovered.len(), 1, "expected one recovered adapter");

    // Verify state was reset to unloaded
    let row: (String, Option<i64>) =
        sqlx::query_as("SELECT load_state, last_heartbeat FROM adapters WHERE adapter_id = ?")
            .bind("recover-adapter")
            .fetch_one(db.pool())
            .await
            .unwrap();

    assert_eq!(row.0, "unloading");
    assert!(row.1.is_none()); // heartbeat cleared
}

#[tokio::test]
async fn test_h4_threshold_edge_cases() {
    let db = Db::new_in_memory().await.unwrap();
    let tenant_id = db.create_tenant("system", false).await.unwrap();

    let params = adapteros_db::adapters::AdapterRegistrationBuilder::new()
        .tenant_id(&tenant_id)
        .adapter_id("edge-adapter")
        .name("Edge Adapter")
        .hash_b3("edge123")
        .rank(8)
        .tier("persistent")
        .build()
        .unwrap();
    db.register_adapter(params).await.unwrap();

    let adapter_names = vec!["edge-adapter".to_string()];
    let mut hashes = HashMap::new();
    hashes.insert("edge-adapter".to_string(), B3Hash::hash(b"edge"));

    let policies = Policies::default();
    let temp_dir = new_test_adapters_dir();
    let manager = LifecycleManager::new_with_db(
        adapter_names,
        hashes,
        &policies,
        temp_dir.path().to_path_buf(),
        None,
        3,
        db.clone(),
    );

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    // Test 1: Just under threshold (299 seconds) - NOT stale
    sqlx::query("UPDATE adapters SET last_heartbeat = ?, load_state = 'cold' WHERE adapter_id = ?")
        .bind(now - 299)
        .bind("edge-adapter")
        .execute(db.pool())
        .await
        .unwrap();

    let stale = manager.check_stale_adapters(300).await.unwrap();
    assert!(stale.is_empty());

    // Test 2: Just over threshold (301 seconds) - IS stale
    sqlx::query("UPDATE adapters SET last_heartbeat = ? WHERE adapter_id = ?")
        .bind(now - 301)
        .bind("edge-adapter")
        .execute(db.pool())
        .await
        .unwrap();

    let stale = manager.check_stale_adapters(300).await.unwrap();
    assert!(!stale.is_empty());
}

#[tokio::test]
async fn test_h4_unloaded_adapters_not_checked() {
    let db = Db::new_in_memory().await.unwrap();
    let tenant_id = db.create_tenant("system", false).await.unwrap();

    let params = adapteros_db::adapters::AdapterRegistrationBuilder::new()
        .tenant_id(&tenant_id)
        .adapter_id("unloaded-adapter")
        .name("Unloaded Adapter")
        .hash_b3("unloaded123")
        .rank(8)
        .tier("persistent")
        .build()
        .unwrap();
    db.register_adapter(params).await.unwrap();

    let adapter_names = vec!["unloaded-adapter".to_string()];
    let mut hashes = HashMap::new();
    hashes.insert("unloaded-adapter".to_string(), B3Hash::hash(b"unloaded"));

    let policies = Policies::default();
    let temp_dir = new_test_adapters_dir();
    let manager = LifecycleManager::new_with_db(
        adapter_names,
        hashes,
        &policies,
        temp_dir.path().to_path_buf(),
        None,
        3,
        db.clone(),
    );

    // Set old heartbeat but keep state as unloaded
    let old_timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        - 400;

    sqlx::query(
        "UPDATE adapters SET last_heartbeat = ?, load_state = 'unloading' WHERE adapter_id = ?",
    )
    .bind(old_timestamp)
    .bind("unloaded-adapter")
    .execute(db.pool())
    .await
    .unwrap();

    // Check stale adapters
    let stale = manager.check_stale_adapters(300).await.unwrap();

    // Unloaded adapters should not be reported as stale
    assert!(stale.is_empty());
}
