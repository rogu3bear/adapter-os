//! Adapter health check and diagnostic handlers
//!
//! Handlers for verifying adapter health, GPU integrity, and tracking activations.

use crate::auth::Claims;
use crate::handlers::utils::aos_error_to_response;
use crate::middleware::{require_any_role, require_role};
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::*;
use adapteros_core::AosError;
use adapteros_db::users::Role;
use adapteros_lora_lifecycle::GpuIntegrityReport;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use serde_json::json;
use sqlx::Row;
use std::collections::HashMap;

// Import helper functions from adapter_utils
use super::adapter_utils::{rollup_health_flag, select_primary_subcode};

const METRIC_ADAPTER_HEALTH_CORRUPT: &str = "adapter_versions_health_corrupt";
const METRIC_ADAPTER_HEALTH_UNSAFE: &str = "adapter_versions_health_unsafe";

#[utoipa::path(
    get,
    path = "/v1/adapters/verify-gpu",
    params(
        ("adapter_id" = Option<String>, Query, description = "Optional adapter ID filter")
    ),
    responses(
        (status = 200, description = "GPU integrity report", body = adapteros_lora_lifecycle::GpuIntegrityReport),
        (status = 500, description = "Verification failed", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn verify_gpu_integrity(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<adapteros_lora_lifecycle::GpuIntegrityReport>, (StatusCode, Json<ErrorResponse>)> {
    // Require operator or admin role
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    let adapter_id = params.get("adapter_id").map(|s| s.as_str());

    tracing::info!(
        adapter_id = ?adapter_id,
        "GPU integrity verification requested"
    );

    // Check if Worker is available
    if let Some(worker) = &state.worker {
        let report = worker
            .lock()
            .await
            .verify_gpu_integrity()
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("GPU verification failed")
                            .with_code("VERIFICATION_FAILED")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;

        tracing::info!(
            total_checked = report.total_checked,
            verified = report.verified.len(),
            failed = report.failed.len(),
            skipped = report.skipped.len(),
            "GPU integrity verification completed"
        );

        Ok(Json(report))
    } else {
        // Worker not available - return empty report with informative message
        tracing::warn!("GPU verification endpoint called but Worker not available in AppState");

        let report = adapteros_lora_lifecycle::GpuIntegrityReport {
            verified: vec![],
            failed: vec![],
            skipped: vec![],
            total_checked: 0,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| {
                    aos_error_to_response(AosError::Internal(format!("System time error: {}", e)))
                })?
                .as_secs(),
        };

        Ok(Json(report))
    }
}

#[utoipa::path(
    get,
    path = "/v1/adapters/{adapter_id}/activations",
    tag = "adapters",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID"),
        ("limit" = Option<i32>, Query, description = "Maximum activations to return")
    ),
    responses(
        (status = 200, description = "Adapter activations", body = Vec<AdapterActivationResponse>),
        (status = 404, description = "Adapter not found", body = ErrorResponse)
    )
)]
pub async fn get_adapter_activations(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Vec<AdapterActivationResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterView)?;

    let adapter = state
        .db
        .get_adapter_for_tenant(&claims.tenant_id, &adapter_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to fetch adapter")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("adapter not found").with_code("NOT_FOUND")),
            )
        })?;

    validate_tenant_isolation(&claims, &adapter.tenant_id)?;

    let limit = query
        .get("limit")
        .and_then(|l| l.parse().ok())
        .unwrap_or(100);

    let activations = state
        .db
        .get_adapter_activations(&adapter_id, limit)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to get activations")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let responses: Vec<AdapterActivationResponse> = activations
        .into_iter()
        .map(|a| AdapterActivationResponse {
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
            id: a.id,
            adapter_id: a.adapter_id,
            request_id: a.request_id,
            gate_value: a.gate_value,
            selected: a.selected == 1,
            created_at: a.created_at,
        })
        .collect();

    Ok(Json(responses))
}

/// Get adapter health (activation logs, memory usage, policy violations)
pub async fn get_adapter_health(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<AdapterHealthResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Fetch adapter with tenant-scoped query
    let adapter = state
        .db
        .get_adapter_for_tenant(&claims.tenant_id, &adapter_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("adapter not found").with_code("NOT_FOUND")),
            )
        })?;

    // Health thresholds (drift/per-tier)
    let (drift_hard_threshold, high_tier_block_threshold) = adapteros_config::effective_config()
        .map(|cfg| {
            (
                cfg.health.adapter.drift_hard_threshold,
                cfg.health.adapter.high_tier_block_threshold,
            )
        })
        .unwrap_or((0.15, 0.10));

    // Attempt to resolve adapter version data
    let version_id = adapter
        .adapter_id
        .clone()
        .unwrap_or_else(|| adapter.id.clone());
    let adapter_version = state
        .db
        .get_adapter_version(&adapter.tenant_id, &version_id)
        .await
        .ok()
        .flatten();

    // Dataset linkage + trust signals
    let dataset_version_ids = state
        .db
        .list_dataset_versions_for_adapter_version(&version_id)
        .await
        .unwrap_or_default();
    let mut datasets = Vec::new();
    let mut trust_blocked = false;
    for ds_version_id in dataset_version_ids {
        if let Ok(Some(ds)) = state
            .db
            .get_training_dataset_version_for_tenant(&ds_version_id, &adapter.tenant_id)
            .await
        {
            let blocked = ds.trust_state.eq_ignore_ascii_case("blocked")
                || ds.trust_state.eq_ignore_ascii_case("blocked_regressed")
                || ["regressed", "blocked", "unsafe", "blocked_regressed"]
                    .iter()
                    .any(|s| ds.overall_trust_status.eq_ignore_ascii_case(s));
            if blocked {
                trust_blocked = true;
            }
            datasets.push(adapteros_api_types::adapters::AdapterDatasetHealth {
                dataset_version_id: ds.id.clone(),
                trust_state: ds.trust_state.clone(),
                overall_trust_status: Some(ds.overall_trust_status.clone()),
            });
        }
    }

    // Storage / reconciler signals
    let storage_rows = adapteros_db::sqlx::query(
        r#"
        SELECT issue_type, severity, path, expected_hash, actual_hash, detected_at
        FROM storage_reconciliation_issues
        WHERE version_id = ?
        ORDER BY detected_at DESC
        LIMIT 5
        "#,
    )
    .bind(&version_id)
    .fetch_all(state.db.pool())
    .await
    .unwrap_or_default();

    let mut storage_subcodes = Vec::new();
    let mut has_corrupt = false;
    for row in storage_rows {
        let issue_type: String = row.try_get("issue_type").unwrap_or_default();
        let severity: Option<String> = row.try_get("severity").ok();
        let path: Option<String> = row.try_get("path").ok();
        let expected_hash: Option<String> = row.try_get("expected_hash").ok();
        let actual_hash: Option<String> = row.try_get("actual_hash").ok();
        let detected_at: Option<String> = row.try_get("detected_at").ok();

        let is_corrupt_issue = matches!(
            issue_type.as_str(),
            "missing_bytes" | "missing_file" | "hash_mismatch"
        );
        if is_corrupt_issue {
            has_corrupt = true;
        }
        storage_subcodes.push(adapteros_api_types::adapters::AdapterHealthSubcode {
            domain: adapteros_api_types::adapters::AdapterHealthDomain::Storage,
            code: issue_type.clone(),
            message: Some(format!(
                "{} at {}",
                issue_type,
                path.clone().unwrap_or_default()
            )),
            data: Some(serde_json::json!({
                "severity": severity,
                "expected_hash": expected_hash,
                "actual_hash": actual_hash,
                "detected_at": detected_at
            })),
        });
    }

    // Drift summary placeholder (extend when drift metrics are available)
    let drift_summary: Option<adapteros_api_types::adapters::AdapterDriftSummary> = None;
    let mut drift_triggered = false;

    // Backend/CoreML info surfaced for the UI
    let backend =
        adapter_version
            .as_ref()
            .map(|av| adapteros_api_types::adapters::AdapterBackendHealth {
                backend: av.training_backend.clone(),
                coreml_device_type: av.coreml_device_type.clone(),
                coreml_used: Some(av.coreml_used),
            });

    let mut subcodes: Vec<adapteros_api_types::adapters::AdapterHealthSubcode> = Vec::new();

    if let Some(ref summary) = drift_summary {
        if summary.current >= drift_hard_threshold {
            drift_triggered = true;
            subcodes.push(adapteros_api_types::adapters::AdapterHealthSubcode {
                domain: adapteros_api_types::adapters::AdapterHealthDomain::Drift,
                code: "drift_high".to_string(),
                message: Some(format!(
                    "Drift {:.4} exceeds hard threshold {:.4}",
                    summary.current, drift_hard_threshold
                )),
                data: Some(serde_json::json!({
                    "current": summary.current,
                    "hard_threshold": drift_hard_threshold,
                    "tier_block_threshold": high_tier_block_threshold
                })),
            });
        }
    }

    if trust_blocked {
        subcodes.push(adapteros_api_types::adapters::AdapterHealthSubcode {
            domain: adapteros_api_types::adapters::AdapterHealthDomain::Trust,
            code: "trust_blocked".to_string(),
            message: Some("Dataset trust is blocked/regressed".to_string()),
            data: None,
        });
    }

    subcodes.extend(storage_subcodes.clone());

    let overall = rollup_health_flag(has_corrupt, trust_blocked, drift_triggered);
    let primary_subcode = select_primary_subcode(overall, &subcodes);

    if matches!(
        overall,
        adapteros_api_types::adapters::AdapterHealthFlag::Corrupt
    ) {
        state
            .metrics_registry
            .record_metric(METRIC_ADAPTER_HEALTH_CORRUPT.to_string(), 1.0)
            .await;
    } else if matches!(
        overall,
        adapteros_api_types::adapters::AdapterHealthFlag::Unsafe
    ) {
        state
            .metrics_registry
            .record_metric(METRIC_ADAPTER_HEALTH_UNSAFE.to_string(), 1.0)
            .await;
    }

    // Get adapter activations (last 100)
    let activations = state
        .db
        .get_adapter_activations(&adapter_id, 100)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to get activations")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Get adapter stats
    let (total, selected, avg_gate) = state
        .db
        .get_adapter_stats(&adapter_id)
        .await
        .unwrap_or((0, 0, 0.0));

    // Calculate memory usage trend (simplified - would need time-series data in production)
    let memory_usage_mb = activations.len() as f64 * 2.5; // Rough estimate

    let adapter_id_clone = adapter_id.clone();
    let adapter_id_clone2 = adapter_id.clone();
    let adapter_id_clone3 = adapter_id.clone();

    Ok(Json(AdapterHealthResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        adapter_id: adapter_id_clone,
        health: overall,
        primary_subcode,
        subcodes,
        drift_summary,
        datasets,
        storage: Some(adapteros_api_types::adapters::AdapterStorageHealth {
            reconciler_status: if has_corrupt {
                "corrupt".to_string()
            } else {
                "ok".to_string()
            },
            last_checked_at: storage_subcodes
                .first()
                .and_then(|s| s.data.as_ref())
                .and_then(|d| d.get("detected_at"))
                .and_then(|v| v.as_str().map(String::from)),
            issues: if storage_subcodes.is_empty() {
                None
            } else {
                Some(storage_subcodes)
            },
        }),
        backend,
        recent_activations: activations
            .into_iter()
            .take(10)
            .map(|a| AdapterActivationResponse {
                schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
                id: a.id,
                adapter_id: a.adapter_id,
                request_id: a.request_id,
                gate_value: a.gate_value,
                selected: a.selected == 1,
                created_at: a.created_at,
            })
            .collect(),
        total_activations: total as i32,
        selected_count: selected as i32,
        avg_gate_value: avg_gate,
        memory_usage_mb,
        policy_violations: {
            // Query policy violations from telemetry/audit logs
            sqlx::query_as::<_, (String, String)>(
                "SELECT violation_type, message FROM policy_violations 
                 WHERE adapter_id = ? AND timestamp > datetime('now', '-1 hour')
                 ORDER BY timestamp DESC LIMIT 5",
            )
            .bind(&adapter_id_clone2)
            .fetch_all(state.db.pool())
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(
                    "Failed to fetch policy violations for {}: {}",
                    adapter_id_clone3,
                    e
                );
                vec![]
            })
            .into_iter()
            .map(|(vtype, msg)| format!("{}: {}", vtype, msg))
            .collect()
        },
    }))
}
