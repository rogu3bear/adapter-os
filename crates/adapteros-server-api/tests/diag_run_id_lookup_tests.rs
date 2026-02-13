//! Focused integration tests for diagnostic run-id fallback lookups.
//!
//! Verifies that diagnostics handlers accept a diagnostic run_id where callers
//! historically send `trace_id`, and still resolve to the correct run.

use adapteros_api_types::diagnostics::{DiagBundleExportRequest, DiagExportRequest};
use adapteros_db::sqlx;
use adapteros_db::Db;
use adapteros_server_api::handlers::diag_bundle::create_bundle_export;
use adapteros_server_api::handlers::diagnostics::export_diag_run;
use axum::extract::{Extension, State};
use axum::Json;
use chrono::Utc;
use uuid::Uuid;

mod common;
use common::{setup_state, test_admin_claims, TestkitEnvGuard};

async fn create_test_diag_run(
    db: &impl std::ops::Deref<Target = Db>,
    run_id: &str,
    trace_id: &str,
    tenant_id: &str,
) -> anyhow::Result<()> {
    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind(tenant_id)
        .bind(format!("Test Tenant {}", tenant_id))
        .execute(db.pool())
        .await?;

    let now = Utc::now();
    sqlx::query(
        r#"
        INSERT INTO diag_runs (id, tenant_id, trace_id, status, request_hash, started_at_unix_ms, created_at)
        VALUES (?, ?, ?, 'completed', 'hash123', ?, ?)
        "#,
    )
    .bind(run_id)
    .bind(tenant_id)
    .bind(trace_id)
    .bind(now.timestamp_millis())
    .bind(now.to_rfc3339())
    .execute(db.pool())
    .await?;

    sqlx::query(
        r#"
        INSERT INTO diag_events (run_id, tenant_id, seq, mono_us, event_type, severity, payload_json)
        VALUES (?, ?, ?, ?, 'stage_enter', 'info', '{}')
        "#,
    )
    .bind(run_id)
    .bind(tenant_id)
    .bind(0_i64)
    .bind(1000_i64)
    .execute(db.pool())
    .await?;

    Ok(())
}

#[tokio::test]
async fn export_diag_run_accepts_run_id_lookup() -> anyhow::Result<()> {
    let _env = TestkitEnvGuard::disabled().await;
    let state = setup_state(None).await?;
    let claims = test_admin_claims();

    let tenant_id = claims.tenant_id.clone();
    let run_id = format!("run-{}", Uuid::now_v7());
    let trace_id = format!("trc-{}", Uuid::now_v7());
    create_test_diag_run(&state.db, &run_id, &trace_id, &tenant_id).await?;

    // Intentionally pass run_id in the trace_id field to verify fallback.
    let request = DiagExportRequest {
        trace_id: run_id.clone(),
        format: "json".to_string(),
        include_events: true,
        include_timing: true,
        include_metadata: true,
        max_events: Some(10),
    };

    let Json(response) = export_diag_run(State(state), Extension(claims), Json(request))
        .await
        .expect("export_diag_run should resolve run_id fallback");

    assert_eq!(response.run.id, run_id);
    assert_eq!(response.run.trace_id, trace_id);
    assert_eq!(response.run.status, "completed");

    Ok(())
}

#[tokio::test]
async fn create_bundle_export_accepts_run_id_lookup() -> anyhow::Result<()> {
    let _env = TestkitEnvGuard::disabled().await;
    let state = setup_state(None).await?;
    let claims = test_admin_claims();

    let tenant_id = claims.tenant_id.clone();
    let run_id = format!("run-{}", Uuid::now_v7());
    let trace_id = format!("trc-{}", Uuid::now_v7());
    create_test_diag_run(&state.db, &run_id, &trace_id, &tenant_id).await?;

    // Intentionally pass run_id in the trace_id field to verify fallback.
    let request = DiagBundleExportRequest {
        trace_id: run_id.clone(),
        format: "tar.zst".to_string(),
        include_evidence: false,
        evidence_auth_token: None,
    };

    let Json(response) =
        create_bundle_export(State(state.clone()), Extension(claims), Json(request))
            .await
            .expect("create_bundle_export should resolve run_id fallback");

    assert_eq!(response.manifest.run_id, run_id);
    assert!(!response.export_id.is_empty());

    // Test hygiene: remove generated bundle file in ./var/exports.
    if let Some((file_path,)) =
        sqlx::query_as::<_, (String,)>("SELECT file_path FROM diag_bundle_exports WHERE id = ?")
            .bind(&response.export_id)
            .fetch_optional(state.db.pool())
            .await?
    {
        let _ = tokio::fs::remove_file(file_path).await;
    }

    // Ensure original trace_id is still the run's canonical trace_id in storage.
    let stored: (String,) =
        sqlx::query_as("SELECT trace_id FROM diag_runs WHERE id = ? AND tenant_id = ?")
            .bind(&run_id)
            .bind(&tenant_id)
            .fetch_one(state.db.pool())
            .await?;
    assert_eq!(stored.0, trace_id);

    Ok(())
}
