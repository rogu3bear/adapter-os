use crate::{auth::Claims, state::AppState, types::ErrorResponse};
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

#[derive(Deserialize)]
pub struct JourneyPath {
    #[allow(dead_code)]
    journey_type: String,
    #[allow(dead_code)]
    id: String,
}

#[derive(Serialize)]
pub struct JourneyResponse {
    journey_type: String,
    id: String,
    data: serde_json::Value,
    states: Vec<JourneyState>,
    created_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct JourneyState {
    state: String,
    timestamp: DateTime<Utc>,
    details: serde_json::Value,
}

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
        let tenant_row = sqlx::query!("SELECT itar_flag FROM tenants WHERE id = ?", tenant_id)
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
            if row.itar_flag != 0 && !is_admin {
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
    #[allow(unused_assignments)]
    let mut data = serde_json::json!({});

    match journey_type.as_str() {
        "adapter-lifecycle" => {
            info!("Querying adapter lifecycle for {}", id);
            let rows = sqlx::query!(
                r#"
                SELECT id, current_state, updated_at, memory_bytes, activation_count
                FROM adapters
                WHERE id = ? AND tenant_id = ?
                ORDER BY updated_at ASC
                "#,
                id,
                tenant_id
            )
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
                let timestamp: DateTime<Utc> =
                    NaiveDateTime::parse_from_str(&row.updated_at, "%Y-%m-%dT%H:%M:%S%.fZ")
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
                    state: row.current_state,
                    timestamp,
                    details: serde_json::json!({
                        "memory_bytes": row.memory_bytes,
                        "activation_count": row.activation_count,
                    }),
                });
            }

            data = serde_json::json!({
                "adapter_id": id,
                "total_states": states.len(),
            });
            info!("Retrieved {} states for adapter lifecycle", states.len());
        }
        "promotion-pipeline" => {
            info!("Querying promotion pipeline for {}", id);
            // Note: promotions table doesn't have status or approver columns in current schema
            // This would need to be updated when the promotions schema is extended
            let promotions = sqlx::query!(
                "SELECT cpid, created_at, promoted_by FROM promotions WHERE cpid = ? ORDER BY created_at ASC",
                id
            )
            .fetch_all(state.db.pool())
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("database query failed").with_code("DATABASE_ERROR").with_string_details(e.to_string())),
                )
            })?;

            for promo in promotions {
                let timestamp: DateTime<Utc> =
                    NaiveDateTime::parse_from_str(&promo.created_at, "%Y-%m-%dT%H:%M:%S%.fZ")
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
                    timestamp,
                    details: serde_json::json!({
                        "cpid": promo.cpid,
                        "promoted_by": promo.promoted_by,
                    }),
                });
            }

            data = serde_json::json!({
                "plan_id": id,
                "total_promotions": states.len(),
            });
            info!("Retrieved {} promotions", states.len());
        }
        "monitoring-flow" => {
            info!("Querying monitoring flow for {}", id);
            // Note: system_metrics table doesn't have tenant_id, worker_id, metric_key, or value columns
            // Using available columns and adapting the query structure
            let metrics = sqlx::query!(
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
                let timestamp: DateTime<Utc> =
                    DateTime::from_timestamp(metric.timestamp, 0).unwrap_or_else(Utc::now);

                states.push(JourneyState {
                    state: format!(
                        "cpu: {:.2}%, mem: {:.2}%",
                        metric.cpu_usage, metric.memory_usage
                    ),
                    timestamp,
                    details: serde_json::json!({
                        "cpu_usage": metric.cpu_usage,
                        "memory_usage": metric.memory_usage,
                    }),
                });
            }

            data = serde_json::json!({
                "worker_id": id,
                "recent_metrics": states.len(),
            });
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

    Ok(Json(JourneyResponse {
        journey_type,
        id,
        data,
        states,
        created_at,
    }))
}
