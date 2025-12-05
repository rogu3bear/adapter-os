use adapteros_db::kv_backend::{IndexManager, KvBackend as DbKvBackend, KvDb};
use adapteros_db::{adapters::AdapterRegistrationBuilder, AtomicDualWriteConfig, Db, StorageMode};
use adapteros_storage::{AdapterRepository as StorageAdapterRepo, StorageError};
use async_trait::async_trait;
use std::sync::Arc;

struct FailingBackend;

#[async_trait]
impl DbKvBackend for FailingBackend {
    async fn get(&self, _key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        Ok(None)
    }

    async fn set(&self, _key: &str, _value: Vec<u8>) -> Result<(), StorageError> {
        Err(StorageError::BackendError("injected failure".to_string()))
    }

    async fn delete(&self, _key: &str) -> Result<bool, StorageError> {
        Err(StorageError::BackendError("injected failure".to_string()))
    }

    async fn exists(&self, _key: &str) -> Result<bool, StorageError> {
        Err(StorageError::BackendError("injected failure".to_string()))
    }

    async fn scan_prefix(&self, _prefix: &str) -> Result<Vec<String>, StorageError> {
        Ok(vec![])
    }

    async fn batch_get(&self, keys: &[String]) -> Result<Vec<Option<Vec<u8>>>, StorageError> {
        Ok(vec![None; keys.len()])
    }

    async fn batch_set(&self, _pairs: Vec<(String, Vec<u8>)>) -> Result<(), StorageError> {
        Err(StorageError::BackendError("injected failure".to_string()))
    }

    async fn batch_delete(&self, _keys: &[String]) -> Result<usize, StorageError> {
        Err(StorageError::BackendError("injected failure".to_string()))
    }

    async fn set_add(&self, _key: &str, _member: &str) -> Result<(), StorageError> {
        Err(StorageError::BackendError("injected failure".to_string()))
    }

    async fn set_remove(&self, _key: &str, _member: &str) -> Result<(), StorageError> {
        Err(StorageError::BackendError("injected failure".to_string()))
    }

    async fn set_members(&self, _key: &str) -> Result<Vec<String>, StorageError> {
        Ok(vec![])
    }

    async fn set_is_member(&self, _key: &str, _member: &str) -> Result<bool, StorageError> {
        Ok(false)
    }
}

fn failing_kvdb() -> KvDb {
    let backend: Arc<dyn DbKvBackend> = Arc::new(FailingBackend);
    let index_manager = Arc::new(IndexManager::new(backend.clone()));
    KvDb::new(backend, index_manager)
}

async fn insert_default_tenant(db: &Db) {
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('default-tenant', 'Default Test Tenant')")
        .execute(db.pool())
        .await
        .unwrap();
}

#[tokio::test]
async fn strict_mode_registration_rolls_back_on_kv_failure() {
    let mut db = Db::new_in_memory().await.unwrap();
    insert_default_tenant(&db).await;

    db.attach_kv_backend(failing_kvdb());
    db.set_storage_mode(StorageMode::DualWrite);
    db.set_atomic_dual_write_config(AtomicDualWriteConfig::strict_atomic());

    let params = AdapterRegistrationBuilder::new()
        .adapter_id("strict-fail")
        .name("Strict Fail")
        .hash_b3("b3:strict")
        .rank(4)
        .tier("warm")
        .category("code")
        .scope("global")
        .build()
        .unwrap();

    let result = db.register_adapter(params).await;
    assert!(result.is_err(), "strict mode should propagate KV failure");
    let adapter = db.get_adapter("strict-fail").await.unwrap();
    assert!(adapter.is_none(), "SQL insert should be rolled back");
}

#[tokio::test]
async fn best_effort_registration_succeeds_on_kv_failure() {
    let mut db = Db::new_in_memory().await.unwrap();
    insert_default_tenant(&db).await;

    db.attach_kv_backend(failing_kvdb());
    db.set_storage_mode(StorageMode::DualWrite);
    db.set_atomic_dual_write_config(AtomicDualWriteConfig::best_effort());

    let params = AdapterRegistrationBuilder::new()
        .adapter_id("best-effort")
        .name("Best Effort")
        .hash_b3("b3:best")
        .rank(4)
        .tier("warm")
        .category("code")
        .scope("global")
        .build()
        .unwrap();

    let id = db.register_adapter(params).await.unwrap();
    assert!(!id.is_empty());
    let adapter = db.get_adapter("best-effort").await.unwrap();
    assert!(
        adapter.is_some(),
        "SQL insert should remain despite KV failure"
    );
}

#[tokio::test]
async fn strict_mode_update_returns_error_but_sql_committed() {
    let mut db = Db::new_in_memory().await.unwrap();
    insert_default_tenant(&db).await;

    // Attach working KV for registration
    let kv_working = KvDb::init_in_memory().unwrap();
    db.attach_kv_backend(kv_working);
    db.set_storage_mode(StorageMode::DualWrite);
    db.set_atomic_dual_write_config(AtomicDualWriteConfig::strict_atomic());

    let params = AdapterRegistrationBuilder::new()
        .adapter_id("strict-update")
        .name("Strict Update")
        .hash_b3("b3:update")
        .rank(4)
        .tier("warm")
        .category("code")
        .scope("global")
        .build()
        .unwrap();
    db.register_adapter(params).await.unwrap();

    // Swap in failing KV to force update failure
    db.attach_kv_backend(failing_kvdb());
    db.set_storage_mode(StorageMode::DualWrite);

    let result = db
        .update_adapter_state("strict-update", "hot", "force kv failure")
        .await;
    assert!(
        result.is_err(),
        "strict mode should surface KV failure on update"
    );

    // SQL should still reflect the update
    let adapter = db.get_adapter("strict-update").await.unwrap().unwrap();
    assert_eq!(adapter.current_state, "hot");
}

#[tokio::test]
async fn ensure_consistency_repairs_missing_kv_entry() {
    let mut db = Db::new_in_memory().await.unwrap();
    insert_default_tenant(&db).await;

    let kv = KvDb::init_in_memory().unwrap();
    db.attach_kv_backend(kv.clone());
    db.set_storage_mode(StorageMode::DualWrite);

    let params = AdapterRegistrationBuilder::new()
        .adapter_id("consistency-missing")
        .name("Consistency Missing")
        .hash_b3("b3:consistency_missing")
        .rank(8)
        .tier("warm")
        .category("code")
        .scope("global")
        .build()
        .unwrap();
    db.register_adapter(params).await.unwrap();

    // Delete from KV directly to simulate drift
    let repo = StorageAdapterRepo::new(kv.backend().clone(), kv.index_manager().clone());
    let _ = repo
        .delete("default-tenant", "consistency-missing")
        .await
        .ok();

    let repaired = db.ensure_consistency("consistency-missing").await.unwrap();
    assert!(repaired);

    let adapter_kv = repo
        .get("default-tenant", "consistency-missing")
        .await
        .unwrap();
    assert!(adapter_kv.is_some(), "KV entry should be repaired from SQL");
}

#[tokio::test]
async fn ensure_consistency_returns_false_for_missing_adapter() {
    let db = Db::new_in_memory().await.unwrap();
    let result = db.ensure_consistency("non-existent").await.unwrap();
    assert!(!result);
}

#[tokio::test]
async fn kv_indexes_cover_state_and_tier_queries() {
    let mut db = Db::new_in_memory().await.unwrap();
    insert_default_tenant(&db).await;

    let kv = KvDb::init_in_memory().unwrap();
    db.attach_kv_backend(kv.clone());
    db.set_storage_mode(StorageMode::DualWrite);

    let params = AdapterRegistrationBuilder::new()
        .adapter_id("index-check")
        .name("Index Check")
        .hash_b3("b3:index_check")
        .rank(2)
        .tier("warm")
        .category("code")
        .scope("global")
        .build()
        .unwrap();
    db.register_adapter(params).await.unwrap();

    let repo = StorageAdapterRepo::new(kv.backend().clone(), kv.index_manager().clone());
    let by_state = repo
        .list_by_state("default-tenant", "unloaded")
        .await
        .unwrap();
    assert_eq!(by_state.len(), 1);

    let by_tier = repo.list_by_tier("default-tenant", "warm").await.unwrap();
    assert_eq!(by_tier.len(), 1);
}
