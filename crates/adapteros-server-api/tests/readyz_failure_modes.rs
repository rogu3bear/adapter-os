use adapteros_server_api::boot_state::{failure_codes, BootStateManager, FailureReason};
use adapteros_server_api::handlers::health::{ready, ReadyzResponse};
use axum::body::to_bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use std::time::Duration;

mod common;

use common::setup_state;

async fn call_readyz(state: adapteros_server_api::state::AppState) -> (StatusCode, ReadyzResponse) {
    let response = ready(State(state)).await.into_response();
    let status = response.status();
    let body_bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("readyz body bytes");
    let readyz: ReadyzResponse =
        serde_json::from_slice(&body_bytes).expect("parse readyz response");
    (status, readyz)
}

async fn advance_boot_to_ready(boot_state: &BootStateManager) {
    boot_state.start().await;
    boot_state.db_connecting().await;
    boot_state.migrating().await;
    boot_state.seeding().await;
    boot_state.load_policies().await;
    boot_state.start_backend().await;
    boot_state.load_base_models().await;
    boot_state.load_adapters().await;
    boot_state.ready().await;
}

#[tokio::test]
async fn readyz_reports_db_failure_code() {
    let mut state = setup_state(None).await.unwrap();
    let boot_state = BootStateManager::new();
    boot_state
        .fail(FailureReason::new(
            failure_codes::DB_CONN_FAILED,
            "db unreachable",
        ))
        .await;
    state.boot_state = Some(boot_state);

    let (status, readyz) = call_readyz(state).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        readyz.last_error_code.as_deref(),
        Some(failure_codes::DB_CONN_FAILED)
    );
    assert!(
        readyz
            .checks
            .worker
            .hint
            .as_deref()
            .unwrap_or_default()
            .contains(failure_codes::DB_CONN_FAILED),
        "worker hint should surface DB failure code"
    );
    assert!(!readyz.ready, "ready should be false when DB is down");
}

#[tokio::test]
async fn readyz_reports_worker_missing() {
    let mut state = setup_state(None).await.unwrap();
    let boot_state = BootStateManager::new();
    advance_boot_to_ready(&boot_state).await;
    state.boot_state = Some(boot_state);

    let (status, readyz) = call_readyz(state).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        readyz.checks.worker.hint.as_deref(),
        Some("no workers registered")
    );
    assert!(
        readyz.last_error_code.is_none(),
        "missing worker should not set a boot error code"
    );
}

#[tokio::test]
async fn readyz_reports_otel_unreachable_code() {
    let mut state = setup_state(None).await.unwrap();
    let boot_state = BootStateManager::new();
    boot_state
        .fail(FailureReason::new(
            failure_codes::OTEL_INIT_FAILED,
            "otel export endpoint unreachable",
        ))
        .await;
    state.boot_state = Some(boot_state);

    let (status, readyz) = call_readyz(state).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        readyz.last_error_code.as_deref(),
        Some(failure_codes::OTEL_INIT_FAILED)
    );
    assert!(
        readyz
            .checks
            .worker
            .hint
            .as_deref()
            .unwrap_or_default()
            .contains("otel export endpoint unreachable"),
        "hint should include OTEL failure message"
    );
}

#[tokio::test]
async fn readyz_reports_migration_blocked_code() {
    let mut state = setup_state(None).await.unwrap();
    let boot_state = BootStateManager::new();
    boot_state
        .fail(FailureReason::new(
            failure_codes::MIGRATION_FAILED,
            "migration blocked waiting for signature",
        ))
        .await;
    state.boot_state = Some(boot_state);

    let (status, readyz) = call_readyz(state).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        readyz.last_error_code.as_deref(),
        Some(failure_codes::MIGRATION_FAILED)
    );
    assert!(
        readyz
            .checks
            .worker
            .hint
            .as_deref()
            .unwrap_or_default()
            .contains("migration blocked"),
        "hint should include migration failure context"
    );
}

#[tokio::test]
async fn readyz_reports_port_conflict_code() {
    let mut state = setup_state(None).await.unwrap();
    let boot_state = BootStateManager::new();
    boot_state
        .fail(FailureReason::new(
            failure_codes::SOCKET_BIND_FAILED,
            "port already in use",
        ))
        .await;
    state.boot_state = Some(boot_state);

    let (status, readyz) = call_readyz(state).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        readyz.last_error_code.as_deref(),
        Some(failure_codes::SOCKET_BIND_FAILED)
    );
    assert!(
        readyz
            .checks
            .worker
            .hint
            .as_deref()
            .unwrap_or_default()
            .contains("port already in use"),
        "hint should include bind failure details"
    );
}

#[tokio::test]
async fn readyz_uses_latest_failed_phase_code() {
    let boot_state = BootStateManager::new();
    boot_state.start_phase("db_connect");
    boot_state.finish_phase_err("db_connect", failure_codes::DB_CONN_FAILED, None);
    tokio::time::sleep(Duration::from_millis(2)).await;
    boot_state.start_phase("router_build");
    boot_state.finish_phase_err("router_build", failure_codes::ROUTER_BUILD_FAILED, None);

    assert_eq!(
        boot_state.last_error_code().as_deref(),
        Some(failure_codes::ROUTER_BUILD_FAILED)
    );
}
