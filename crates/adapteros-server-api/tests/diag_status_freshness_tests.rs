//! Focused freshness coverage for GET /v1/diagnostics/determinism-status.

use adapteros_api_types::diagnostics::{DeterminismFreshnessReason, DeterminismFreshnessStatus};
use adapteros_db::sqlx;
use adapteros_server_api::handlers::diagnostics::get_determinism_status;
use axum::extract::{Extension, State};
use axum::Json;
use chrono::{Duration, Utc};

mod common;
use common::{setup_state, test_admin_claims, TestkitEnvGuard};

async fn insert_determinism_check(
    db: &adapteros_db::Db,
    last_run: &str,
    result: &str,
    runs: i64,
    divergences: i64,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO determinism_checks (last_run, result, runs, divergences, stack_id, seed)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(last_run)
    .bind(result)
    .bind(runs)
    .bind(divergences)
    .bind("stack-test")
    .bind("seed-test")
    .execute(db.pool_result()?)
    .await?;
    Ok(())
}

#[tokio::test]
async fn determinism_status_reports_fresh_when_recent_run_exists() -> anyhow::Result<()> {
    let _env = TestkitEnvGuard::disabled().await;
    let state = setup_state(None).await?;
    let claims = test_admin_claims();

    let recent = (Utc::now() - Duration::seconds(5))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    insert_determinism_check(&state.db, &recent, "pass", 3, 0).await?;

    let Json(response) = get_determinism_status(State(state), Extension(claims))
        .await
        .expect("determinism status should succeed");

    assert_eq!(response.result.as_deref(), Some("pass"));
    assert_eq!(response.runs, Some(3));
    assert_eq!(response.divergences, Some(0));
    assert_eq!(response.freshness_status, DeterminismFreshnessStatus::Fresh);
    assert_eq!(
        response.freshness_reason,
        DeterminismFreshnessReason::RecentRun
    );

    let age_seconds = response
        .freshness_age_seconds
        .expect("fresh result should include age seconds");
    assert!(
        age_seconds <= response.stale_after_seconds,
        "fresh age {} should be <= threshold {}",
        age_seconds,
        response.stale_after_seconds
    );

    Ok(())
}

#[tokio::test]
async fn determinism_status_reports_stale_when_latest_run_is_old() -> anyhow::Result<()> {
    let _env = TestkitEnvGuard::disabled().await;
    let state = setup_state(None).await?;
    let claims = test_admin_claims();

    let old = (Utc::now() - Duration::days(2))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    insert_determinism_check(&state.db, &old, "fail", 3, 2).await?;

    let Json(response) = get_determinism_status(State(state), Extension(claims))
        .await
        .expect("determinism status should succeed");

    assert_eq!(response.freshness_status, DeterminismFreshnessStatus::Stale);
    assert_eq!(
        response.freshness_reason,
        DeterminismFreshnessReason::StaleLastRun
    );

    let age_seconds = response
        .freshness_age_seconds
        .expect("stale result should include age seconds");
    assert!(
        age_seconds > response.stale_after_seconds,
        "stale age {} should be > threshold {}",
        age_seconds,
        response.stale_after_seconds
    );

    Ok(())
}

#[tokio::test]
async fn determinism_status_missing_state_returns_unknown_reason() -> anyhow::Result<()> {
    let _env = TestkitEnvGuard::disabled().await;
    let state = setup_state(None).await?;
    let claims = test_admin_claims();

    let Json(response) = get_determinism_status(State(state), Extension(claims))
        .await
        .expect("determinism status should succeed");

    assert_eq!(response.last_run, None);
    assert_eq!(response.result, None);
    assert_eq!(response.runs, None);
    assert_eq!(response.divergences, None);
    assert_eq!(
        response.freshness_status,
        DeterminismFreshnessStatus::Unknown
    );
    assert_eq!(
        response.freshness_reason,
        DeterminismFreshnessReason::NoDeterminismChecks
    );
    assert_eq!(response.freshness_age_seconds, None);

    Ok(())
}
