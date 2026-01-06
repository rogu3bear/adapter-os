#![allow(clippy::await_holding_lock)]

use adapteros_db::users::Role;
use adapteros_db::{global_kv_metrics, kv_coverage_summary, Db, KvDb, KvErrorType, StorageMode};
use once_cell::sync::Lazy;
use std::sync::{Arc, Mutex};

static KV_METRICS_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[tokio::test]
async fn kv_only_supported_domains_smoke_when_coverage_ready() -> adapteros_core::Result<()> {
    let _guard = KV_METRICS_LOCK.lock().unwrap();

    let coverage = kv_coverage_summary();
    if !coverage.unsupported_domains.is_empty() {
        println!(
            "Skipping kv-only smoke; unsupported domains: {:?}",
            coverage.unsupported_domains
        );
        return Ok(());
    }

    let tmp_root = std::path::PathBuf::from("var").join("tmp");
    std::fs::create_dir_all(&tmp_root)?;
    let tmp = tempfile::tempdir_in(&tmp_root)?;
    let kv_path = tmp.path().join("kv.redb");
    let kv = KvDb::init_redb(kv_path.as_path())?;
    let metrics = global_kv_metrics();
    metrics.reset();

    let mut db = Db::new_kv_only(Some(Arc::new(kv)), StorageMode::KvOnly);
    db.enforce_kv_only_guard()?;

    let tenant_id = db.create_tenant("kv-only", false).await?;
    let user_id = db
        .create_user(
            "kv-only@example.com",
            "KV Only User",
            "pw-hash",
            Role::Admin,
            &tenant_id,
        )
        .await?;

    let fetched_user = db.get_user(&user_id).await?;
    assert!(fetched_user.is_some());
    assert_eq!(fetched_user.unwrap().tenant_id, tenant_id);

    let snapshot = metrics.snapshot();
    assert_eq!(snapshot.fallback_operations_total, 0);
    assert_eq!(snapshot.errors_total, 0);

    Ok(())
}

#[tokio::test]
async fn kv_only_downgrades_on_fallback_metrics() -> adapteros_core::Result<()> {
    let _guard = KV_METRICS_LOCK.lock().unwrap();

    let coverage = kv_coverage_summary();
    if !coverage.unsupported_domains.is_empty() {
        println!(
            "Skipping downgrade test; unsupported domains: {:?}",
            coverage.unsupported_domains
        );
        return Ok(());
    }

    let metrics = global_kv_metrics();
    metrics.reset();
    metrics.record_fallback_write();
    metrics.record_error(KvErrorType::Backend);
    let snapshot = metrics.snapshot();
    assert_eq!(snapshot.fallback_operations_total, 1);
    assert_eq!(snapshot.errors_total, 1);

    let mut db = Db::new_in_memory().await?;
    let kv = KvDb::init_in_memory()?;
    db.attach_kv_backend(kv);

    db.set_storage_mode(StorageMode::KvOnly)?;

    db.enforce_kv_only_guard()?;

    // Fallbacks/errors should force downgrade to kv_primary with a reason.
    assert_eq!(db.storage_mode(), StorageMode::KvPrimary);
    assert!(db.is_degraded());
    let reason = db.degradation_reason().unwrap();
    assert!(reason.contains("fallbacks=1"));
    assert!(reason.contains("errors=1"));

    let snapshot = metrics.snapshot();
    assert_eq!(snapshot.fallback_operations_total, 1);
    assert_eq!(snapshot.errors_total, 1);
    assert_eq!(snapshot.degraded_events_total, 1);

    Ok(())
}
