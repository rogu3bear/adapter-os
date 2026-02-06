//! Orchestration configuration handlers
//!
//! Provides tenant-scoped orchestration configuration with persistence.
//! Includes prompt analysis with rule matching and stack scoring,
//! plus routing decision metrics aggregation.

use crate::handlers::{AppState, Claims, ErrorResponse};
use crate::permissions::{require_any_role, require_permission, Permission};
use adapteros_api_types::orchestration::OrchestrationConfig;
use adapteros_db::sqlx;
use adapteros_db::sqlx::Row;
use adapteros_db::users::Role;
use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{info, warn};
use utoipa::ToSchema;

const ORCHESTRATION_CONFIG_KEY_PREFIX: &str = "orchestration.config";

fn tenant_config_key(tenant_id: &str) -> String {
    format!("{}.{}", ORCHESTRATION_CONFIG_KEY_PREFIX, tenant_id)
}

fn global_config_key() -> String {
    ORCHESTRATION_CONFIG_KEY_PREFIX.to_string()
}

fn validate_config(config: &OrchestrationConfig) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if let Some(entropy) = config.entropy_threshold {
        if !(0.0..=1.0).contains(&entropy) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("entropy_threshold must be between 0.0 and 1.0")
                        .with_code("BAD_REQUEST"),
                ),
            ));
        }
    }

    if let Some(confidence) = config.confidence_threshold {
        if !(0.0..=1.0).contains(&confidence) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("confidence_threshold must be between 0.0 and 1.0")
                        .with_code("BAD_REQUEST"),
                ),
            ));
        }
    }

    if config.max_adapters_per_request == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("max_adapters_per_request must be greater than zero")
                    .with_code("BAD_REQUEST"),
            ),
        ));
    }

    if config.timeout_ms == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("timeout_ms must be greater than zero").with_code("BAD_REQUEST"),
            ),
        ));
    }

    Ok(())
}

fn score_stack(
    prompt: &str,
    stack_name: &str,
    description: Option<&str>,
    adapter_ids: &[String],
) -> i32 {
    let prompt_lower = prompt.to_ascii_lowercase();
    let mut score = 0i32;

    if prompt_lower.contains(&stack_name.to_ascii_lowercase()) {
        score += 2;
    }

    if let Some(desc) = description {
        if prompt_lower.contains(&desc.to_ascii_lowercase()) {
            score += 1;
        }
    }

    for adapter_id in adapter_ids {
        if prompt_lower.contains(&adapter_id.to_ascii_lowercase()) {
            score += 1;
        }
    }

    score
}

/// Request body for prompt analysis (stubbed)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct PromptAnalysisRequest {
    pub prompt: String,
}

/// Session summary for orchestration UI
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct OrchestrationSessionSummary {
    pub id: String,
    pub status: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapters: Option<Vec<String>>,
}

/// Response wrapper for orchestration sessions
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrchestrationSessionsResponse {
    pub sessions: Vec<OrchestrationSessionSummary>,
}

/// Get orchestration configuration (single-node stub)
#[utoipa::path(
    get,
    path = "/v1/orchestration/config",
    responses(
        (status = 200, description = "Orchestration configuration", body = adapteros_api_types::orchestration::OrchestrationConfig),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "orchestration"
)]
pub async fn get_orchestration_config(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<OrchestrationConfig>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator, Role::Viewer])?;

    let tenant_key = tenant_config_key(&claims.tenant_id);
    let global_key = global_config_key();

    let stored = state
        .db
        .get_system_setting(&tenant_key)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to load orchestration config")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let stored = match stored {
        Some(value) => Some(value),
        None => state
            .db
            .get_system_setting(&global_key)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to load orchestration config")
                            .with_code("DATABASE_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?,
    };

    let config = stored
        .as_deref()
        .and_then(|raw| serde_json::from_str::<OrchestrationConfig>(raw).ok())
        .unwrap_or_else(|| {
            if stored.is_some() {
                warn!("Failed to parse stored orchestration config, using defaults");
            }
            OrchestrationConfig::default()
        });

    Ok(Json(config))
}

/// List orchestration sessions (empty for single-node/dev)
#[utoipa::path(
    get,
    path = "/v1/orchestration/sessions",
    responses(
        (status = 200, description = "Orchestration sessions", body = OrchestrationSessionsResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden")
    ),
    tag = "orchestration"
)]
pub async fn list_orchestration_sessions(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<OrchestrationSessionsResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator, Role::Viewer])?;

    let tracker = match state.inference_state_tracker.as_ref() {
        Some(tracker) => tracker.clone(),
        None => {
            return Ok(Json(OrchestrationSessionsResponse {
                sessions: Vec::new(),
            }))
        }
    };

    let sessions = tracker
        .list_active()
        .into_iter()
        .filter(|entry| entry.tenant_id == claims.tenant_id)
        .map(|entry| OrchestrationSessionSummary {
            id: entry.inference_id,
            status: entry.state.name().to_string(),
            created_at: entry.created_at.to_rfc3339(),
            adapters: if entry.adapter_ids.is_empty() {
                None
            } else {
                Some(entry.adapter_ids)
            },
        })
        .collect();

    Ok(Json(OrchestrationSessionsResponse { sessions }))
}

/// Update orchestration configuration
#[utoipa::path(
    put,
    path = "/v1/orchestration/config",
    request_body = adapteros_api_types::orchestration::OrchestrationConfig,
    responses(
        (status = 200, description = "Updated orchestration configuration", body = adapteros_api_types::orchestration::OrchestrationConfig),
        (status = 400, description = "Invalid payload"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "orchestration"
)]
pub async fn update_orchestration_config(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<OrchestrationConfig>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterStackManage)?;

    validate_config(&payload)?;

    let key = tenant_config_key(&claims.tenant_id);
    let serialized = serde_json::to_string(&payload).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("failed to serialize config")
                    .with_code("BAD_REQUEST")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    state
        .db
        .set_system_setting(&key, &serialized)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to persist orchestration config")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    info!(
        user = %claims.sub,
        routing_strategy = %payload.routing_strategy,
        "Persisted orchestration config update"
    );

    Ok((StatusCode::OK, Json(payload)))
}

/// Analyze prompt for orchestration
///
/// Matches custom rules, scores stacks, and recommends adapter selection.
#[utoipa::path(
    post,
    path = "/v1/orchestration/analyze",
    request_body = PromptAnalysisRequest,
    responses(
        (status = 200, description = "Prompt analysis result", body = Value),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 500, description = "Internal server error")
    ),
    tag = "orchestration"
)]
pub async fn analyze_orchestration_prompt(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(body): Json<PromptAnalysisRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator, Role::Viewer])?;

    let tenant_key = tenant_config_key(&claims.tenant_id);
    let config = state
        .db
        .get_system_setting(&tenant_key)
        .await
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str::<OrchestrationConfig>(&raw).ok())
        .unwrap_or_default();

    let stacks = state
        .db
        .list_stacks_for_tenant(&claims.tenant_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to fetch stacks for analysis")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let prompt = body.prompt.trim().to_string();
    let prompt_lower = prompt.to_ascii_lowercase();

    let mut matched_rules = Vec::new();
    let mut selected_stack = config.default_adapter_stack.clone();

    for rule in &config.custom_rules {
        if !rule.enabled {
            continue;
        }

        let condition = rule.condition.to_ascii_lowercase();
        if !condition.is_empty() && prompt_lower.contains(&condition) {
            matched_rules.push(serde_json::json!({
                "rule_id": rule.id,
                "rule_name": rule.name,
                "adapter_stack": rule.adapter_stack,
                "priority": rule.priority,
            }));
        }
    }

    if !matched_rules.is_empty() {
        matched_rules.sort_by(|a, b| {
            let prio_a = a.get("priority").and_then(|v| v.as_i64()).unwrap_or(0);
            let prio_b = b.get("priority").and_then(|v| v.as_i64()).unwrap_or(0);
            prio_b.cmp(&prio_a)
        });
        selected_stack = matched_rules
            .first()
            .and_then(|rule| rule.get("adapter_stack"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or(selected_stack);
    }

    let mut recommendations = Vec::new();
    for stack in stacks {
        let adapter_ids: Vec<String> =
            serde_json::from_str(&stack.adapter_ids_json).unwrap_or_default();
        let score = score_stack(
            &prompt,
            &stack.name,
            stack.description.as_deref(),
            &adapter_ids,
        );
        recommendations.push(serde_json::json!({
            "stack_id": stack.id,
            "stack_name": stack.name,
            "score": score,
            "adapter_count": adapter_ids.len(),
            "routing_mode": stack.routing_determinism_mode,
        }));
    }

    recommendations.sort_by(|a, b| {
        let score_a = a.get("score").and_then(|v| v.as_i64()).unwrap_or(0);
        let score_b = b.get("score").and_then(|v| v.as_i64()).unwrap_or(0);
        let name_a = a
            .get("stack_name")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let name_b = b
            .get("stack_name")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        score_b.cmp(&score_a).then_with(|| name_a.cmp(name_b))
    });

    if selected_stack.is_none() {
        selected_stack = recommendations
            .first()
            .and_then(|r| r.get("stack_id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
    }

    let analysis_id = crate::id_generator::readable_id(adapteros_id::IdPrefix::Job, "orch");

    Ok(Json(serde_json::json!({
        "analysis_id": analysis_id,
        "tenant_id": claims.tenant_id,
        "prompt": prompt,
        "routing_strategy": config.routing_strategy,
        "selected_stack": selected_stack,
        "matched_rules": matched_rules,
        "recommendations": recommendations,
        "stack_count": recommendations.len(),
    })))
}

/// Retrieve orchestration metrics
///
/// Returns routing decision counts, averages, and top stack usage.
#[utoipa::path(
    get,
    path = "/v1/orchestration/metrics",
    responses(
        (status = 200, description = "Orchestration metrics", body = Value),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 500, description = "Internal server error")
    ),
    tag = "orchestration"
)]
pub async fn get_orchestration_metrics(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Value>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator, Role::Viewer])?;

    let tenant_id = claims.tenant_id.clone();

    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM routing_decisions WHERE tenant_id = ?")
            .bind(&tenant_id)
            .fetch_optional(state.db.pool())
            .await
            .unwrap_or(Some(0))
            .unwrap_or(0);

    let last_hour: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM routing_decisions WHERE tenant_id = ? AND timestamp >= datetime('now', '-1 hour')",
    )
    .bind(&tenant_id)
    .fetch_optional(state.db.pool())
    .await
    .unwrap_or(Some(0))
    .unwrap_or(0);

    let last_day: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM routing_decisions WHERE tenant_id = ? AND timestamp >= datetime('now', '-1 day')",
    )
    .bind(&tenant_id)
    .fetch_optional(state.db.pool())
    .await
    .unwrap_or(Some(0))
    .unwrap_or(0);

    let avg_row: Option<(Option<f64>, Option<f64>, Option<f64>)> = sqlx::query_as(
        "SELECT AVG(entropy), AVG(overhead_pct), AVG(total_inference_latency_us) FROM routing_decisions WHERE tenant_id = ?",
    )
    .bind(&tenant_id)
    .fetch_optional(state.db.pool())
    .await
    .unwrap_or(None);

    let (avg_entropy, avg_overhead, avg_latency_us) = avg_row.unwrap_or((None, None, None));

    let top_stacks: Vec<serde_json::Value> = sqlx::query(
        "SELECT stack_id, COUNT(*) as cnt FROM routing_decisions WHERE tenant_id = ? AND stack_id IS NOT NULL GROUP BY stack_id ORDER BY cnt DESC, stack_id ASC LIMIT 5",
    )
    .bind(&tenant_id)
    .fetch_all(state.db.pool())
    .await
    .map(|rows| {
        rows.into_iter()
            .filter_map(|row| {
                let stack_id: Option<String> = row.try_get("stack_id").ok();
                let cnt: i64 = row.try_get("cnt").unwrap_or(0);
                stack_id.map(|id| serde_json::json!({"stack_id": id, "decision_count": cnt}))
            })
            .collect()
    })
    .unwrap_or_default();

    Ok(Json(serde_json::json!({
        "tenant_id": tenant_id,
        "decision_count_total": total,
        "decision_count_last_hour": last_hour,
        "decision_count_last_day": last_day,
        "avg_entropy": avg_entropy,
        "avg_overhead_pct": avg_overhead,
        "avg_inference_latency_ms": avg_latency_us.map(|v| v / 1000.0),
        "top_stacks": top_stacks,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    })))
}
