use std::sync::atomic::Ordering;
use std::sync::Arc;

use adapteros_server_api::boot_state::{BootState, BootStateManager};
use adapteros_server_api::handlers::adapter_stacks::deactivate_stack;
use adapteros_server_api::handlers::health::{ready, ReadyzQuery, ReadyzResponse};
use adapteros_server_api::handlers::unload_adapter;
use adapteros_server_api::ip_extraction::ClientIp;
use axum::body::to_bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

mod common;

use common::{create_test_adapter_default, setup_state, test_admin_claims};

#[tokio::test]
async fn readyz_does_not_report_stopped_during_startup() {
    let mut state = setup_state(None).await.unwrap();
    let boot_state = BootStateManager::new();
    state.boot_state = Some(boot_state);

    let response = ready(State(state.clone()), Query(ReadyzQuery { deep: false }))
        .await
        .into_response();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

    let body_bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let readyz: ReadyzResponse = serde_json::from_slice(&body_bytes).unwrap();

    let hint = readyz.checks.worker.hint.clone().unwrap_or_default();
    assert!(
        !hint.contains("stopped"),
        "readyz should not advertise stopped during startup: {}",
        serde_json::to_string(&readyz).unwrap()
    );
}

#[tokio::test]
async fn boot_state_does_not_reset_when_db_is_attached_mid_boot() {
    let boot_state = BootStateManager::new();
    boot_state.start().await;
    boot_state.db_connecting().await;

    let elapsed_before = boot_state.elapsed();
    let db = Arc::new(adapteros_db::Db::new_in_memory().await.unwrap());
    let boot_state = boot_state.with_db(db);

    assert!(
        boot_state.elapsed() >= elapsed_before,
        "attaching DB should preserve the boot timeline"
    );
    assert_eq!(
        boot_state.current_state(),
        BootState::DbConnecting,
        "state should not regress when attaching DB"
    );

    boot_state.migrating().await;
    assert_eq!(boot_state.current_state(), BootState::Migrating);

    boot_state.seeding().await;
    assert_eq!(boot_state.current_state(), BootState::Seeding);

    boot_state.load_policies().await;
    assert_eq!(boot_state.current_state(), BootState::LoadingPolicies);

    boot_state.start_backend().await;
    assert_eq!(boot_state.current_state(), BootState::StartingBackend);

    boot_state.load_base_models().await;
    assert_eq!(boot_state.current_state(), BootState::LoadingBaseModels);

    boot_state.load_adapters().await;
    assert_eq!(boot_state.current_state(), BootState::LoadingAdapters);

    boot_state.worker_discovery().await;
    assert_eq!(boot_state.current_state(), BootState::WorkerDiscovery);

    boot_state.ready().await;
    assert!(
        boot_state.is_ready(),
        "boot sequence should reach Ready without dropping to Stopped"
    );
}

#[tokio::test]
async fn unload_adapter_rejects_when_in_flight_requests_exist() {
    let state = setup_state(None).await.unwrap();
    create_test_adapter_default(&state, "adapter-1", "tenant-1")
        .await
        .unwrap();

    // Simulate another in-flight request in addition to this handler
    state.in_flight_requests.store(2, Ordering::SeqCst);

    let result = unload_adapter(
        State(state.clone()),
        axum::Extension(test_admin_claims()),
        axum::Extension(ClientIp("127.0.0.1".to_string())),
        Path("adapter-1".to_string()),
    )
    .await;

    let (status, Json(err)) = result.expect_err("guard should block unload");
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(err.code, "ADAPTER_IN_USE");
}

#[tokio::test]
async fn deactivate_stack_rejects_when_in_flight_requests_exist() {
    let state = setup_state(None).await.unwrap();

    // Seed an active stack for the tenant
    {
        let mut active = state.active_stack.write().unwrap();
        active.insert("tenant-1".to_string(), Some("stack-1".to_string()));
    }

    // Simulate another in-flight request in addition to this handler
    state.in_flight_requests.store(2, Ordering::SeqCst);

    let result = deactivate_stack(
        State(state.clone()),
        axum::Extension(test_admin_claims()),
        axum::Extension(ClientIp("127.0.0.1".to_string())),
    )
    .await;

    let (status, Json(err)) = result.expect_err("guard should block deactivation");
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(err.code, "ADAPTER_IN_USE");

    // Ensure the active stack mapping was not altered
    let active = state.active_stack.read().unwrap();
    assert_eq!(
        active.get("tenant-1").and_then(|v| v.as_ref().cloned()),
        Some("stack-1".to_string())
    );
}
