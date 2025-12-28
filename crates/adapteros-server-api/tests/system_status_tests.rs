//! Tests for the aggregated `/v1/system/status` endpoint.

mod common;

use adapteros_api_types::system_status::StatusIndicator;
use adapteros_core::B3Hash;
use adapteros_server_api::boot_state::BootStateManager;
use adapteros_server_api::handlers::system_status::get_system_status;
use adapteros_server_api::state::AppState;
use axum::{extract::State, Extension};
use uuid::Uuid;

async fn boot_to_ready() -> BootStateManager {
    let manager = BootStateManager::new();
    manager.start().await;
    manager.db_connecting().await;
    manager.migrating().await;
    manager.seeding().await;
    manager.load_policies().await;
    manager.start_backend().await;
    manager.load_base_models().await;
    manager.load_adapters().await;
    manager.worker_discovery().await;
    manager.ready().await;
    manager
}

async fn seed_worker_and_model(state: &AppState) {
    use adapteros_db::sqlx;

    let manifest_hash = B3Hash::hash(b"plan-manifest").to_hex();
    let plan_id = state
        .db
        .create_plan(
            "plan",
            "tenant-1",
            &B3Hash::hash(b"plan-id").to_hex(),
            &manifest_hash,
            "[]",
        )
        .await
        .expect("plan creation");

    let model_hash = B3Hash::hash(b"model-1").to_hex();
    sqlx::query(
        "INSERT OR IGNORE INTO models (id, name, hash_b3, config_hash_b3, tokenizer_hash_b3, tokenizer_cfg_hash_b3, model_type, status, backend, created_at)
         VALUES (?, ?, ?, ?, ?, ?, 'base_model', 'available', 'metal', datetime('now'))",
    )
    .bind("model-1")
    .bind("Model 1")
    .bind(&model_hash)
    .bind(format!("config-{model_hash}"))
    .bind(format!("tokenizer-{model_hash}"))
    .bind(format!("tokenizer-cfg-{model_hash}"))
    .execute(state.db.pool())
    .await
    .expect("insert model");

    sqlx::query(
        "INSERT INTO workers (id, tenant_id, node_id, plan_id, uds_path, pid, status, memory_headroom_pct, k_current, adapters_loaded_json, started_at, last_seen_at)
         VALUES (?, ?, ?, ?, '/var/run/aos/test.sock', NULL, ?, NULL, NULL, '[]', datetime('now'), datetime('now'))",
    )
    .bind(format!("worker-{}", Uuid::new_v4()))
    .bind("tenant-1")
    .bind("node-1")
    .bind(&plan_id)
    .bind("registered")
    .execute(state.db.pool())
    .await
    .expect("insert worker");
}

#[tokio::test]
async fn system_status_handles_db_down() {
    let mut state = common::setup_state(None).await.expect("setup state");
    state.boot_state = Some(boot_to_ready().await);
    let claims = common::test_admin_claims();

    state.db.pool().close().await;

    let response = get_system_status(State(state), Extension(claims))
        .await
        .expect("status ok")
        .0;

    assert_eq!(
        response.readiness.checks.db.status,
        StatusIndicator::Unknown
    );
    assert_eq!(response.readiness.overall, StatusIndicator::Unknown);
    assert!(response.boot.is_some(), "boot status should be present");
}

#[tokio::test]
async fn system_status_tracks_workers() {
    let mut empty_state = common::setup_state(None).await.expect("setup state");
    empty_state.boot_state = Some(boot_to_ready().await);
    let claims = common::test_admin_claims();

    // No workers yet -> not ready
    let initial = get_system_status(State(empty_state), Extension(claims.clone()))
        .await
        .expect("status ok")
        .0;
    assert_eq!(
        initial.readiness.checks.workers.status,
        StatusIndicator::NotReady
    );

    let mut seeded_state = common::setup_state(None).await.expect("setup state");
    seeded_state.boot_state = Some(boot_to_ready().await);
    seed_worker_and_model(&seeded_state).await;

    let hydrated = get_system_status(State(seeded_state), Extension(claims))
        .await
        .expect("status ok")
        .0;

    assert_eq!(
        hydrated.readiness.checks.workers.status,
        StatusIndicator::Ready
    );
    assert_eq!(hydrated.readiness.overall, StatusIndicator::Ready);
}

#[tokio::test]
async fn system_status_surfaces_degraded_boot() {
    let mut state = common::setup_state(None).await.expect("setup state");
    let boot_state = boot_to_ready().await;
    seed_worker_and_model(&state).await;
    boot_state
        .degrade_component("router", "quorum degraded")
        .await;
    state.boot_state = Some(boot_state);
    let claims = common::test_admin_claims();

    let response = get_system_status(State(state), Extension(claims))
        .await
        .expect("status ok")
        .0;

    assert_eq!(
        response.readiness.overall,
        StatusIndicator::NotReady,
        "degraded boot should mark readiness not-ready"
    );
    let degraded = response.boot.and_then(|b| b.degraded.into_iter().next());
    assert!(
        degraded
            .map(|d| d.reason.contains("quorum degraded"))
            .unwrap_or(false),
        "degraded reasons should be surfaced"
    );
}
