use adapteros_core::Result;
use adapteros_db::{
    adapters::{AdapterRegistrationBuilder, AdapterRegistrationParams, AtomicDualWriteConfig},
    sqlx, Db,
};

fn test_adapter_params(adapter_id: &str) -> Result<AdapterRegistrationParams> {
    AdapterRegistrationBuilder::new()
        .adapter_id(adapter_id)
        .name(format!("Test Adapter {}", adapter_id))
        .hash_b3(format!("b3:{adapter_id}"))
        .rank(8)
        .tier("warm")
        .build()
}

async fn seed_default_tenant(db: &Db) -> Result<()> {
    sqlx::query(
        "INSERT OR IGNORE INTO tenants (id, name) VALUES ('default-tenant', 'Default Tenant')",
    )
    .execute(db.pool())
    .await?;
    Ok(())
}

#[tokio::test]
async fn atomic_dual_write_config_default() {
    let config = AtomicDualWriteConfig::default();
    assert!(!config.require_kv_success);
    assert!(!config.is_strict());
}

#[tokio::test]
async fn atomic_dual_write_config_best_effort() {
    let config = AtomicDualWriteConfig::best_effort();
    assert!(!config.require_kv_success);
    assert!(!config.is_strict());
}

#[tokio::test]
async fn atomic_dual_write_config_strict() {
    let config = AtomicDualWriteConfig::strict_atomic();
    assert!(config.require_kv_success);
    assert!(config.is_strict());
}

#[tokio::test]
async fn atomic_dual_write_config_from_env() {
    // Default (no env var) - CHANGED: now defaults to strict mode
    std::env::remove_var("AOS_ATOMIC_DUAL_WRITE_STRICT");
    assert!(AtomicDualWriteConfig::from_env().is_strict());

    std::env::set_var("AOS_ATOMIC_DUAL_WRITE_STRICT", "true");
    assert!(AtomicDualWriteConfig::from_env().is_strict());

    std::env::set_var("AOS_ATOMIC_DUAL_WRITE_STRICT", "1");
    assert!(AtomicDualWriteConfig::from_env().is_strict());

    // Explicit disable
    std::env::set_var("AOS_ATOMIC_DUAL_WRITE_STRICT", "false");
    assert!(!AtomicDualWriteConfig::from_env().is_strict());

    std::env::set_var("AOS_ATOMIC_DUAL_WRITE_STRICT", "0");
    assert!(!AtomicDualWriteConfig::from_env().is_strict());

    std::env::remove_var("AOS_ATOMIC_DUAL_WRITE_STRICT");
}

#[tokio::test]
async fn best_effort_mode_sql_only() -> Result<()> {
    // Best-effort: SQL commits even without KV
    let db = Db::new_in_memory()
        .await?
        .with_atomic_dual_write_config(AtomicDualWriteConfig::best_effort());

    seed_default_tenant(&db).await?;
    let params = test_adapter_params("test-adapter-1")?;
    let id = db.register_adapter_extended(params).await?;

    let adapter = db.get_adapter("test-adapter-1").await?;
    assert!(adapter.is_some());
    assert_eq!(adapter.unwrap().id, id);
    Ok(())
}

#[tokio::test]
async fn strict_mode_sql_only() -> Result<()> {
    // Strict but no KV attached: behaves like SQL-only
    let db = Db::new_in_memory()
        .await?
        .with_atomic_dual_write_config(AtomicDualWriteConfig::strict_atomic());

    seed_default_tenant(&db).await?;
    let params = test_adapter_params("test-adapter-2")?;
    let id = db.register_adapter_extended(params).await?;

    let adapter = db.get_adapter("test-adapter-2").await?;
    assert!(adapter.is_some());
    assert_eq!(adapter.unwrap().id, id);
    Ok(())
}

#[tokio::test]
async fn db_config_persists_on_clone() -> Result<()> {
    let db = Db::new_in_memory()
        .await?
        .with_atomic_dual_write_config(AtomicDualWriteConfig::strict_atomic());

    assert!(db.atomic_dual_write_config().is_strict());
    let db_clone = db.clone();
    assert!(db_clone.atomic_dual_write_config().is_strict());
    Ok(())
}

#[tokio::test]
async fn ensure_consistency_no_kv_backend() -> Result<()> {
    let db = Db::new_in_memory().await?;

    seed_default_tenant(&db).await?;
    let params = test_adapter_params("test-adapter-3")?;
    db.register_adapter_extended(params).await?;

    // With no KV attached, ensure_consistency is a no-op but returns true
    let result = db.ensure_consistency("test-adapter-3").await?;
    assert!(result);
    Ok(())
}
