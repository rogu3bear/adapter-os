//! Diagnostics API handlers for PRD G2
//!
//! Endpoints:
//! - GET /v1/diagnostics/determinism-status - Get determinism check status
//! - GET /v1/diagnostics/quarantine-status - Get quarantine status

use crate::auth::Claims;
use crate::middleware::require_any_role;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_db::users::Role;
use axum::extract::{Extension, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;
use sqlx::Row;
use tracing::{debug, warn};
use utoipa::ToSchema;

/// Determinism check status response
#[derive(Debug, Serialize, ToSchema)]
pub struct DeterminismStatusResponse {
    pub last_run: Option<String>,
    pub result: Option<String>, // "pass" | "fail" | null
    pub runs: Option<usize>,
    pub divergences: Option<usize>,
}

/// GET /v1/diagnostics/determinism-status - Get determinism check status
#[utoipa::path(
    get,
    path = "/v1/diagnostics/determinism-status",
    responses(
        (status = 200, description = "Determinism check status", body = DeterminismStatusResponse)
    ),
    tag = "diagnostics",
    security(("bearer_token" = []))
)]
pub async fn get_determinism_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<DeterminismStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(
        &claims,
        &[Role::Admin, Role::Operator, Role::Viewer],
    )?;

    debug!("Querying determinism check status");

    // Query for last determinism check run
    // For MVP, we'll store this in a simple table or use a file-based approach
    // In production, this would be stored in a diagnostics_runs table
    let result = sqlx::query(
        "SELECT last_run, result, runs, divergences 
         FROM determinism_checks 
         ORDER BY last_run DESC 
         LIMIT 1",
    )
    .fetch_optional(state.db.pool())
    .await;

    match result {
        Ok(Some(row)) => {
            let last_run: Option<String> = row.try_get("last_run").ok();
            let result: Option<String> = row.try_get("result").ok();
            let runs: Option<i64> = row.try_get("runs").ok();
            let divergences: Option<i64> = row.try_get("divergences").ok();

            Ok(Json(DeterminismStatusResponse {
                last_run,
                result,
                runs: runs.map(|r| r as usize),
                divergences: divergences.map(|d| d as usize),
            }))
        }
        Ok(None) => {
            // No previous check
            Ok(Json(DeterminismStatusResponse {
                last_run: None,
                result: None,
                runs: None,
                divergences: None,
            }))
        }
        Err(e) => {
            // Table might not exist yet - return empty status
            debug!("Determinism checks table not found or error: {}", e);
            Ok(Json(DeterminismStatusResponse {
                last_run: None,
                result: None,
                runs: None,
                divergences: None,
            }))
        }
    }
}

/// Quarantine status response
#[derive(Debug, Serialize, ToSchema)]
pub struct QuarantineStatusResponse {
    pub is_quarantined: bool,
    pub reason: Option<String>,
    pub quarantined_adapters: Vec<String>,
    pub active_quarantined_adapters: Vec<(String, String)>, // (adapter_id, stack_id)
    pub last_checked: Option<String>,
}

/// Quarantined adapter info
#[derive(Debug, Serialize, ToSchema)]
pub struct QuarantinedAdapter {
    pub id: String,
    pub reason: String,
    pub created_at: String,
}

/// GET /v1/diagnostics/quarantine-status - Get quarantine status
#[utoipa::path(
    get,
    path = "/v1/diagnostics/quarantine-status",
    responses(
        (status = 200, description = "Quarantine status", body = QuarantineStatusResponse)
    ),
    tag = "diagnostics",
    security(("bearer_token" = []))
)]
pub async fn get_quarantine_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<QuarantineStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(
        &claims,
        &[Role::Admin, Role::Operator, Role::Viewer],
    )?;

    debug!("Querying quarantine status");

    // Query active quarantines from database (PRD G2)
    let quarantines = sqlx::query(
        "SELECT id, reason, created_at, violation_type, cpid, metadata 
         FROM active_quarantine 
         ORDER BY created_at DESC",
    )
    .fetch_all(state.db.pool())
    .await
    .map_err(|e| {
        warn!(error = %e, "Failed to query quarantines");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to query quarantine status")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Extract adapter IDs from quarantine records (improved logic from diag.rs)
    let mut quarantined_adapter_ids = Vec::new();
    let mut system_quarantined = false;
    let mut quarantine_reason: Option<String> = None;

    for row in &quarantines {
        let id: String = row.try_get("id").unwrap_or_default();
        let reason: String = row.try_get("reason").unwrap_or_default();
        let metadata: Option<String> = row.try_get("metadata").ok();

        // Check if this is a system-wide quarantine (not adapter-specific)
        if reason.contains("policy hash") || reason.contains("system") || id == "system" {
            system_quarantined = true;
            if quarantine_reason.is_none() {
                quarantine_reason = Some(reason.clone());
            }
        }

        // Try to extract adapter ID from metadata JSON
        let mut adapter_id: Option<String> = None;
        if let Some(ref meta_str) = metadata {
            if let Ok(meta_json) = serde_json::from_str::<serde_json::Value>(meta_str) {
                if let Some(serde_json::Value::String(adapter_id_str)) = meta_json.get("adapter_id")
                {
                    adapter_id = Some(adapter_id_str.clone());
                } else if let Some(serde_json::Value::String(adapter_id_str)) =
                    meta_json.get("adapter")
                {
                    adapter_id = Some(adapter_id_str.clone());
                }
            }
        }

        // Fall back to extracting from reason
        if adapter_id.is_none() {
            for part in reason.split_whitespace() {
                if part.starts_with("adapter:") || part.starts_with("Adapter:") {
                    if let Some(id_part) = part.split(':').nth(1) {
                        adapter_id = Some(id_part.trim().to_string());
                        break;
                    }
                }
            }
            if adapter_id.is_none() && !reason.contains(' ') && reason.len() > 5 {
                adapter_id = Some(reason.clone());
            }
        }

        if let Some(adapter_id_str) = adapter_id {
            if !quarantined_adapter_ids.contains(&adapter_id_str) {
                quarantined_adapter_ids.push(adapter_id_str);
            }
        }
    }

    // Check for quarantined adapters in active stacks
    let active_stacks = sqlx::query(
        "SELECT id, name, adapter_ids_json 
         FROM adapter_stacks 
         WHERE active = 1",
    )
    .fetch_all(state.db.pool())
    .await
    .map_err(|e| {
        warn!(error = %e, "Failed to list active stacks for quarantine check");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Failed to list active stacks").with_code("DATABASE_ERROR")),
        )
    })?;

    let mut active_quarantined_adapters = Vec::new();
    for stack_row in &active_stacks {
        let stack_id: String = stack_row.try_get("id").unwrap_or_default();
        let adapter_ids_json: Option<String> = stack_row.try_get("adapter_ids_json").ok();

        if let Some(ref json_str) = adapter_ids_json {
            if let Ok(adapter_ids) = serde_json::from_str::<Vec<String>>(json_str) {
                for adapter_id in &adapter_ids {
                    if quarantined_adapter_ids.contains(adapter_id) {
                        active_quarantined_adapters.push((adapter_id.clone(), stack_id.clone()));
                    }
                }
            }
        }
    }

    Ok(Json(QuarantineStatusResponse {
        is_quarantined: system_quarantined || !quarantined_adapter_ids.is_empty(),
        reason: quarantine_reason,
        quarantined_adapters: quarantined_adapter_ids,
        active_quarantined_adapters,
        last_checked: Some(chrono::Utc::now().to_rfc3339()),
    }))
}
