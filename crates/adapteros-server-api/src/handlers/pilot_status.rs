//! Pilot Status Handler
//!
//! Minimal readiness-style endpoint for pilot environments.
//! Provides a single JSON payload that the UI can render without chaining multiple API calls.

use crate::permissions::{require_permission, Permission};
use crate::{AppState, Claims, ErrorResponse};
use adapteros_api_types::API_SCHEMA_VERSION;
use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct PilotTrainingJobSummary {
    pub id: String,
    pub status: String,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter_name: Option<String>,
    pub repo_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct PilotStatusResponse {
    pub schema_version: String,
    pub tenant_id: String,
    pub api_ready: bool,
    pub db_ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_error: Option<String>,
    pub worker_registered: bool,
    pub workers_total: i64,
    #[serde(default)]
    pub worker_status_counts: HashMap<String, i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workers_error: Option<String>,
    pub models_seeded: bool,
    pub models_total: i64,
    #[serde(default)]
    pub model_names: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub models_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_training_job: Option<PilotTrainingJobSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub training_error: Option<String>,
    pub timestamp: u64,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/system/pilot-status",
    responses(
        (status = 200, description = "Pilot status checks", body = PilotStatusResponse)
    )
)]
pub async fn get_pilot_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<PilotStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::MetricsView)?;

    let tenant_id = claims.tenant_id.clone();
    let timestamp = now_secs();

    let mut db_ready = false;
    let mut db_error: Option<String> = None;

    // Checks that depend on DB are best-effort: they should not crash the endpoint.
    let mut workers_total: i64 = 0;
    let mut worker_status_counts: HashMap<String, i64> = HashMap::new();
    let mut workers_error: Option<String> = None;

    let mut models_total: i64 = 0;
    let mut model_names: Vec<String> = Vec::new();
    let mut models_error: Option<String> = None;

    let mut last_training_job: Option<PilotTrainingJobSummary> = None;
    let mut training_error: Option<String> = None;

    match state.db.pool().acquire().await {
        Ok(mut conn) => {
            let fk_enabled: Result<i64, _> = sqlx::query_scalar("PRAGMA foreign_keys")
                .fetch_one(&mut *conn)
                .await;
            if let Err(e) = fk_enabled {
                db_error = Some(format!("PRAGMA foreign_keys failed: {}", e));
            } else {
                // Basic DB liveness check.
                if let Err(e) = sqlx::query("SELECT 1").execute(&mut *conn).await {
                    db_error = Some(format!("SELECT 1 failed: {}", e));
                } else {
                    db_ready = true;
                }
            }

            // Workers: count by status for current tenant.
            let worker_rows = sqlx::query(
                "SELECT status, COUNT(*) as cnt FROM workers WHERE tenant_id = ? GROUP BY status",
            )
            .bind(&tenant_id)
            .fetch_all(&mut *conn)
            .await;

            match worker_rows {
                Ok(rows) => {
                    for row in rows {
                        let status: String = row
                            .try_get("status")
                            .unwrap_or_else(|_| "unknown".to_string());
                        let cnt: i64 = row.try_get("cnt").unwrap_or(0);
                        workers_total += cnt;
                        worker_status_counts.insert(status, cnt);
                    }
                }
                Err(e) => {
                    workers_error = Some(e.to_string());
                }
            }

            // Models: global base model seeding.
            let models_count: Result<i64, _> =
                sqlx::query_scalar("SELECT COUNT(*) FROM base_models")
                    .fetch_one(&mut *conn)
                    .await;

            match models_count {
                Ok(count) => {
                    models_total = count;
                    let names: Result<Vec<String>, _> = sqlx::query_scalar(
                        "SELECT name FROM base_models ORDER BY created_at DESC LIMIT 10",
                    )
                    .fetch_all(&mut *conn)
                    .await;
                    if let Ok(names) = names {
                        model_names = names;
                    }
                }
                Err(e) => {
                    models_error = Some(e.to_string());
                }
            }

            // Training: last job status for tenant.
            let job_row = sqlx::query(
                "SELECT id, status, started_at, completed_at, adapter_name, repo_id \
                 FROM repository_training_jobs \
                 WHERE tenant_id = ? OR created_by LIKE ? \
                 ORDER BY started_at DESC \
                 LIMIT 1",
            )
            .bind(&tenant_id)
            .bind(format!("%{}%", tenant_id))
            .fetch_optional(&mut *conn)
            .await;

            match job_row {
                Ok(Some(row)) => {
                    let id: String = row.try_get("id").unwrap_or_default();
                    let status: String = row
                        .try_get("status")
                        .unwrap_or_else(|_| "unknown".to_string());
                    let started_at: String = row.try_get("started_at").unwrap_or_default();
                    let completed_at: Option<String> = row.try_get("completed_at").ok();
                    let adapter_name: Option<String> = row.try_get("adapter_name").ok();
                    let repo_id: String = row.try_get("repo_id").unwrap_or_default();
                    last_training_job = Some(PilotTrainingJobSummary {
                        id,
                        status,
                        started_at,
                        completed_at,
                        adapter_name,
                        repo_id,
                    });
                }
                Ok(None) => {}
                Err(e) => {
                    training_error = Some(e.to_string());
                }
            }
        }
        Err(e) => {
            db_error = Some(e.to_string());
        }
    }

    Ok(Json(PilotStatusResponse {
        schema_version: API_SCHEMA_VERSION.to_string(),
        tenant_id,
        api_ready: true,
        db_ready,
        db_error,
        worker_registered: workers_total > 0,
        workers_total,
        worker_status_counts,
        workers_error,
        models_seeded: models_total > 0,
        models_total,
        model_names,
        models_error,
        last_training_job,
        training_error,
        timestamp,
    }))
}
