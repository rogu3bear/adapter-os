use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::types::ErrorResponse;
use crate::uds_client::{UdsClient, UdsClientError, WorkerCoremlVerification};
use adapteros_db::models::Worker;
use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::Serialize;
use std::time::Duration;
use tracing::warn;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
pub struct WorkerCoremlStatus {
    pub worker_id: String,
    pub tenant_id: String,
    pub plan_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub mismatch: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CoremlVerificationStatusResponse {
    pub workers: Vec<WorkerCoremlStatus>,
}

fn map_snapshot(worker: &Worker, snapshot: WorkerCoremlVerification) -> WorkerCoremlStatus {
    WorkerCoremlStatus {
        worker_id: worker.id.clone(),
        tenant_id: worker.tenant_id.clone(),
        plan_id: worker.plan_id.clone(),
        status: snapshot.status,
        mode: snapshot.mode,
        expected: snapshot.expected,
        actual: snapshot.actual,
        source: snapshot.source,
        mismatch: snapshot.mismatch,
        error: None,
    }
}

fn unavailable_snapshot(worker: &Worker) -> WorkerCoremlStatus {
    WorkerCoremlStatus {
        worker_id: worker.id.clone(),
        tenant_id: worker.tenant_id.clone(),
        plan_id: worker.plan_id.clone(),
        status: "unavailable".to_string(),
        mode: None,
        expected: None,
        actual: None,
        source: None,
        mismatch: false,
        error: Some("worker_unreachable".to_string()),
    }
}

/// Aggregate CoreML verification status from all workers via UDS.
#[utoipa::path(
    tag = "debug",
    get,
    path = "/v1/debug/coreml_verification_status",
    responses(
        (status = 200, description = "Per-worker CoreML verification status", body = CoremlVerificationStatusResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    ),
    security(("bearer_token" = []))
)]
pub async fn coreml_verification_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<CoremlVerificationStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkerView)?;

    let client = UdsClient::new(Duration::from_secs(2));
    let workers = match state.db.list_all_workers().await {
        Ok(ws) => ws,
        Err(e) => {
            warn!(
                error = %e,
                "Failed to list workers for CoreML verification status"
            );
            return Ok(Json(CoremlVerificationStatusResponse { workers: vec![] }));
        }
    };

    let mut statuses = Vec::new();
    for worker in workers {
        if worker.status == "stopped" {
            continue;
        }

        let uds_path = std::path::Path::new(&worker.uds_path);
        let snapshot = match client.coreml_verification_status(uds_path).await {
            Ok(snapshot) => map_snapshot(&worker, snapshot),
            Err(e) => {
                if !matches!(e, UdsClientError::RoutingBypass(_)) {
                    warn!(
                        worker_id = %worker.id,
                        tenant_id = %worker.tenant_id,
                        error = %e,
                        "CoreML verification status fetch failed"
                    );
                }
                unavailable_snapshot(&worker)
            }
        };
        statuses.push(snapshot);
    }

    Ok(Json(CoremlVerificationStatusResponse { workers: statuses }))
}
