use crate::{auth::Claims, state::AppState, types::ErrorResponse};
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tracing::{info, warn};
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
pub struct JourneyPath {
    /// Type of journey (e.g., "adapter", "training", "deployment")
    pub journey_type: String,
    /// Unique identifier for the journey
    pub id: String,
}

#[derive(Serialize, ToSchema)]
pub struct JourneyResponse {
    pub schema_version: String,
    pub journey_id: String,
    pub steps: Vec<JourneyStep>,
    pub current_step: usize,
    pub completed: bool,
    pub states: Vec<JourneyState>,
    pub id: String,
    pub journey_type: String,
    pub created_at: String,
}

#[derive(Serialize, ToSchema)]
pub struct JourneyStep {
    pub id: String,
    pub name: String,
    pub status: String, // "pending" | "in_progress" | "completed" | "skipped"
    pub metadata: Option<serde_json::Value>,
}

#[derive(Serialize, ToSchema)]
pub struct JourneyState {
    pub state: String,
    pub timestamp: String,
    pub details: serde_json::Value,
}

/// Get journey details by type and ID
#[utoipa::path(
    get,
    path = "/v1/journeys/{journey_type}/{id}",
    params(
        ("journey_type" = String, Path, description = "Type of journey (adapter-lifecycle, promotion-pipeline, monitoring-flow)"),
        ("id" = String, Path, description = "Journey identifier")
    ),
    responses(
        (status = 200, description = "Journey data retrieved", body = JourneyResponse),
        (status = 400, description = "Invalid journey type", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden - ITAR restriction", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "journeys"
)]
pub async fn get_journey(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((journey_type, id)): Path<(String, String)>,
) -> Result<Json<JourneyResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Use the correct admin/operator permission checks for Claims
    let is_admin = claims.role == "admin";
    let is_operator = claims.role == "operator";
    if !is_operator && !is_admin {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("insufficient permissions").with_code("UNAUTHORIZED")),
        ));
    }

    let tenant_id = &claims.tenant_id;

    // ITAR check
    if ["security-compliance", "incident-response"].contains(&journey_type.as_str()) {
        let tenant_row = sqlx::query("SELECT itar_flag FROM tenants WHERE id = ?")
            .bind(tenant_id)
            .fetch_optional(state.db.pool())
            .await
            .map_err(|e| {
                warn!("ITAR check failed for tenant {}: {}", tenant_id, e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("ITAR check failed").with_code("SECURITY_ERROR")),
                )
            })?;

        if let Some(row) = tenant_row {
            let itar_flag: i64 = row.try_get("itar_flag").unwrap_or(0);
            if itar_flag != 0 && !is_admin {
                return Err((
                    StatusCode::FORBIDDEN,
                    Json(
                        ErrorResponse::new("admin required for ITAR-restricted journey")
                            .with_code("FORBIDDEN"),
                    ),
                ));
            }
        }
    }

    info!(
        "Fetching journey data for user {}: type={}, id={}, tenant={}",
        claims.sub, journey_type, id, tenant_id
    );

    let mut states = Vec::new();

    match journey_type.as_str() {
        "adapter-lifecycle" => {
            info!("Querying adapter lifecycle for {}", id);
            let rows = sqlx::query(
                r#"
                SELECT id, current_state, updated_at, memory_bytes, activation_count
                FROM adapters
                WHERE id = ? AND tenant_id = ?
                ORDER BY updated_at ASC
                "#,
            )
            .bind(&id)
            .bind(tenant_id)
            .fetch_all(state.db.pool())
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("database query failed")
                            .with_code("DATABASE_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;

            for row in rows {
                let current_state: String = row.try_get("current_state").unwrap_or_default();
                let updated_at: String = row.try_get("updated_at").unwrap_or_default();
                let memory_bytes: Option<i64> = row.try_get("memory_bytes").unwrap_or(None);
                let activation_count: Option<i64> = row.try_get("activation_count").unwrap_or(None);

                let timestamp: DateTime<Utc> =
                    NaiveDateTime::parse_from_str(&updated_at, "%Y-%m-%dT%H:%M:%S%.fZ")
                        .map_err(|parse_err| {
                            warn!("Timestamp parse failed: {}", parse_err);
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(
                                    ErrorResponse::new("invalid timestamp format")
                                        .with_code("DATA_ERROR"),
                                ),
                            )
                        })?
                        .and_utc();

                states.push(JourneyState {
                    state: current_state,
                    timestamp: timestamp.to_rfc3339(),
                    details: serde_json::json!({
                        "memory_bytes": memory_bytes,
                        "activation_count": activation_count,
                    }),
                });
            }

            info!("Retrieved {} states for adapter lifecycle", states.len());
        }
        "promotion-pipeline" => {
            info!("Querying promotion pipeline for {}", id);
            // Note: promotions table doesn't have status or approver columns in current schema
            // This would need to be updated when the promotions schema is extended
            let promotions = sqlx::query(
                "SELECT cpid, created_at, promoted_by FROM promotions WHERE cpid = ? ORDER BY created_at ASC",
            )
            .bind(&id)
            .fetch_all(state.db.pool())
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("database query failed").with_code("DATABASE_ERROR").with_string_details(e.to_string())),
                )
            })?;

            for promo in promotions {
                let created_at: String = promo.try_get("created_at").unwrap_or_default();
                let timestamp: DateTime<Utc> =
                    NaiveDateTime::parse_from_str(&created_at, "%Y-%m-%dT%H:%M:%S%.fZ")
                        .map_err(|parse_err| {
                            warn!("Timestamp parse failed: {}", parse_err);
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(
                                    ErrorResponse::new("invalid timestamp format")
                                        .with_code("DATA_ERROR"),
                                ),
                            )
                        })?
                        .and_utc();

                states.push(JourneyState {
                    state: "completed".to_string(), // Default state since status column doesn't exist
                    timestamp: timestamp.to_rfc3339(),
                    details: serde_json::json!({
                        "cpid": promo.try_get::<String,_>("cpid").unwrap_or_default(),
                        "promoted_by": promo.try_get::<Option<String>,_>("promoted_by").unwrap_or(None),
                    }),
                });
            }

            info!("Retrieved {} promotions", states.len());
        }
        "monitoring-flow" => {
            info!("Querying monitoring flow for {}", id);
            // Note: system_metrics table doesn't have tenant_id, worker_id, metric_key, or value columns
            // Using available columns and adapting the query structure
            let metrics = sqlx::query(
                "SELECT cpu_usage, memory_usage, timestamp FROM system_metrics ORDER BY timestamp DESC LIMIT 10"
            )
            .fetch_all(state.db.pool())
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("database query failed").with_code("DATABASE_ERROR").with_string_details(e.to_string())),
                )
            })?;

            for metric in metrics {
                let cpu_usage: f64 = metric.try_get("cpu_usage").unwrap_or(0.0);
                let memory_usage: f64 = metric.try_get("memory_usage").unwrap_or(0.0);
                let ts_raw: i64 = metric.try_get("timestamp").unwrap_or(0);
                let timestamp: DateTime<Utc> =
                    DateTime::from_timestamp(ts_raw, 0).unwrap_or_else(Utc::now);

                states.push(JourneyState {
                    state: format!("cpu: {:.2}%, mem: {:.2}%", cpu_usage, memory_usage),
                    timestamp: timestamp.to_rfc3339(),
                    details: serde_json::json!({
                        "cpu_usage": cpu_usage,
                        "memory_usage": memory_usage,
                    }),
                });
            }

            info!("Retrieved {} metrics", states.len());
        }
        _ => {
            warn!("Unsupported journey type requested: {}", journey_type);
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new("unsupported journey type").with_code("INVALID_INPUT")),
            ));
        }
    }

    let created_at = Utc::now();
    info!("Journey response prepared for {}", journey_type);

    // Generate steps from states for frontend compatibility
    let steps: Vec<JourneyStep> = states
        .iter()
        .enumerate()
        .map(|(i, s)| JourneyStep {
            id: format!("step-{}", i),
            name: s.state.clone(),
            status: "completed".to_string(),
            metadata: Some(s.details.clone()),
        })
        .collect();

    let current_step = if steps.is_empty() { 0 } else { steps.len() - 1 };
    let completed = !steps.is_empty();

    Ok(Json(JourneyResponse {
        schema_version: "1.0".to_string(),
        journey_id: id.clone(),
        steps,
        current_step,
        completed,
        states,
        id,
        journey_type,
        created_at: created_at.to_rfc3339(),
    }))
}
