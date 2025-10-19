use crate::{state::AppState, types::{Claims, ErrorResponse}};
use axum::{extract::{Extension, Path, State}, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use chrono::{DateTime, Utc};
use uuid::Uuid;
use tracing::{info, warn}; // Add tracing

#[derive(Deserialize)]
pub struct JourneyPath {
    journey_type: String,
    id: String,
}

#[derive(Serialize)]
pub struct JourneyResponse {
    journey_type: String,
    id: String,
    data: serde_json::Value, // Flexible for different journeys
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
    // Enhanced RBAC: Base operator, but admin for sensitive
    let is_admin = claims.roles.contains(&"admin".to_string());
    let is_operator = claims.roles.contains(&"operator".to_string());
    if !is_operator && !is_admin {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("insufficient permissions").with_code("UNAUTHORIZED")),
        ));
    }

    let tenant_id = &claims.tenant_id;

    // ITAR check for sensitive journeys
    if ["security-compliance", "incident-response"].contains(&journey_type.as_str()) {
        let tenant = sqlx::query!(
            "SELECT itar_flag FROM tenants WHERE id = ?",
            tenant_id
        )
        .fetch_optional(state.db.pool())
        .await
        .map_err(|e| {
            warn!("ITAR check failed for tenant {}: {}", tenant_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("ITAR check failed").with_code("SECURITY_ERROR")),
            )
        })?;

        if let Some(row) = tenant {
            if row.itar_flag && !is_admin {
                return Err((
                    StatusCode::FORBIDDEN,
                    Json(ErrorResponse::new("admin required for ITAR-restricted journey").with_code("FORBIDDEN")),
                ));
            }
        }
    }

    info!("Fetching journey data for user {}: type={}, id={}, tenant={}", claims.sub, journey_type, id, tenant_id); // Audit log

    let mut states = Vec::new();
    let mut data = serde_json::json!({});

    match journey_type.as_str() {
        "adapter-lifecycle" => {
            info!("Querying adapter lifecycle for {}", id);
            // Query adapters table for state history (assuming audit log or updated_at tracking)
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
                    Json(ErrorResponse::new("database query failed").with_code("DATABASE_ERROR").with_string_details(e.to_string())),
                )
            })?;

            for row in rows {
                states.push(JourneyState {
                    state: row.current_state,
                    timestamp: row.updated_at,
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
            info!("Querying promotion pipeline for plan {}", id);
            // Query promotions and cp_pointers
            let promotions = sqlx::query!(
                "SELECT * FROM promotions WHERE plan_id = ? AND tenant_id = ? ORDER BY created_at ASC",
                id,
                tenant_id
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
                states.push(JourneyState {
                    state: promo.status.unwrap_or_default(),
                    timestamp: promo.created_at,
                    details: serde_json::json!({
                        "cpid": promo.cpid,
                        "approver": promo.approver,
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
            info!("Querying monitoring flow for worker {}", id);
            // Query system_metrics for recent entries
            let metrics = sqlx::query!(
                "SELECT metric_name, value, timestamp FROM system_metrics WHERE tenant_id = ? AND worker_id = ? ORDER BY timestamp DESC LIMIT 10",
                tenant_id,
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

            for metric in metrics {
                states.push(JourneyState {
                    state: metric.metric_name.unwrap_or_default(),
                    timestamp: metric.timestamp,
                    details: serde_json::json!({
                        "value": metric.value,
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
