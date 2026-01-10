//! Metrics handlers
//!
//! Handlers for quality metrics, adapter metrics, system metrics, and Prometheus endpoint.

use crate::auth::Claims;
use crate::state::AppState;
use crate::types::*;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Extension, Json};
use chrono::{Duration, Utc};
use serde_json::Value;
use sqlx::Row;

fn is_missing_table_error(error: &sqlx::Error) -> bool {
    error.to_string().contains("no such table")
}

struct PatchStats {
    total: i64,
    completed: i64,
    failed: i64,
    rolled_back: i64,
    validation_total: i64,
    compile_success: i64,
    tests_passed: i64,
    evidence_present: i64,
    follow_up_fixes: i64,
}

impl PatchStats {
    fn empty() -> Self {
        Self {
            total: 0,
            completed: 0,
            failed: 0,
            rolled_back: 0,
            validation_total: 0,
            compile_success: 0,
            tests_passed: 0,
            evidence_present: 0,
            follow_up_fixes: 0,
        }
    }
}

fn parse_validation_summary(value: &Value) -> (Option<bool>, Option<bool>, bool) {
    let compile_success = value
        .get("compile_success")
        .and_then(|v| v.as_bool())
        .or_else(|| value.get("build_success").and_then(|v| v.as_bool()))
        .or_else(|| value.get("compiled").and_then(|v| v.as_bool()));

    let tests_passed = value
        .get("tests_passed")
        .and_then(|v| v.as_bool())
        .or_else(|| value.get("tests_ok").and_then(|v| v.as_bool()))
        .or_else(|| value.get("passed").and_then(|v| v.as_bool()))
        .or_else(|| {
            let total = value.get("tests_total").and_then(|v| v.as_i64());
            let failed = value.get("tests_failed").and_then(|v| v.as_i64());
            match (total, failed) {
                (Some(total), Some(failed)) if total > 0 => Some(failed == 0),
                _ => None,
            }
        });

    let evidence_present = value
        .get("evidence_spans")
        .and_then(|v| v.as_array())
        .map(|v| !v.is_empty())
        .or_else(|| {
            value
                .get("citations")
                .and_then(|v| v.as_array())
                .map(|v| !v.is_empty())
        })
        .or_else(|| {
            value
                .get("evidence")
                .and_then(|v| v.as_array())
                .map(|v| !v.is_empty())
        })
        .unwrap_or(false);

    (compile_success, tests_passed, evidence_present)
}

async fn fetch_patch_stats(
    pool: &sqlx::SqlitePool,
    tenant_id: &str,
    start_time: &str,
    end_time: &str,
) -> Result<PatchStats, (StatusCode, Json<ErrorResponse>)> {
    let rows = match sqlx::query(
        "SELECT status, COUNT(*) as count \
         FROM patch_applications \
         WHERE tenant_id = ? AND applied_at >= ? AND applied_at < ? \
         GROUP BY status",
    )
    .bind(tenant_id)
    .bind(start_time)
    .bind(end_time)
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            if is_missing_table_error(&e) {
                return Ok(PatchStats::empty());
            }
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ));
        }
    };

    let mut stats = PatchStats::empty();
    for row in rows {
        let status: String = row.get("status");
        let count: i64 = row.get("count");
        stats.total += count;
        match status.as_str() {
            "completed" => stats.completed += count,
            "failed" => stats.failed += count,
            "rolled_back" => stats.rolled_back += count,
            _ => {}
        }
    }

    let validation_rows = match sqlx::query(
        "SELECT validation_results, rollback_id \
         FROM patch_applications \
         WHERE tenant_id = ? AND applied_at >= ? AND applied_at < ?",
    )
    .bind(tenant_id)
    .bind(start_time)
    .bind(end_time)
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            if is_missing_table_error(&e) {
                return Ok(stats);
            }
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ));
        }
    };

    for row in validation_rows {
        if row.get::<Option<String>, _>("rollback_id").is_some() {
            stats.follow_up_fixes += 1;
        }

        if let Some(raw) = row.get::<Option<String>, _>("validation_results") {
            if let Ok(value) = serde_json::from_str::<Value>(&raw) {
                stats.validation_total += 1;
                let (compile_success, tests_passed, evidence_present) =
                    parse_validation_summary(&value);
                if compile_success.unwrap_or(false) {
                    stats.compile_success += 1;
                }
                if tests_passed.unwrap_or(false) {
                    stats.tests_passed += 1;
                }
                if evidence_present {
                    stats.evidence_present += 1;
                }
            }
        }
    }

    Ok(stats)
}

async fn fetch_secret_violations(
    pool: &sqlx::SqlitePool,
    tenant_id: &str,
    start_time: &str,
    end_time: &str,
) -> Result<usize, (StatusCode, Json<ErrorResponse>)> {
    let count: i64 = match sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) \
         FROM policy_violations pv \
         JOIN policy_packs pp ON pv.policy_pack_id = pp.id \
         WHERE pv.tenant_id = ? AND pv.detected_at >= ? AND pv.detected_at < ? \
           AND pp.policy_type = 'secrets'",
    )
    .bind(tenant_id)
    .bind(start_time)
    .bind(end_time)
    .fetch_one(pool)
    .await
    {
        Ok(count) => count,
        Err(e) => {
            if is_missing_table_error(&e) {
                return Ok(0);
            }
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ));
        }
    };

    Ok(count as usize)
}

async fn fetch_router_metrics(
    pool: &sqlx::SqlitePool,
    tenant_id: &str,
    start_time: &str,
    end_time: &str,
    duration: Duration,
) -> Result<(f32, f32, f32), (StatusCode, Json<ErrorResponse>)> {
    let total_decisions: i64 = match sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM routing_decisions \
         WHERE tenant_id = ? AND timestamp >= ? AND timestamp < ?",
    )
    .bind(tenant_id)
    .bind(start_time)
    .bind(end_time)
    .fetch_one(pool)
    .await
    {
        Ok(count) => count,
        Err(e) => {
            if is_missing_table_error(&e) {
                return Ok((0.0, 0.0, 0.0));
            }
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ));
        }
    };

    let avg_overhead: f64 = match sqlx::query_scalar::<_, Option<f64>>(
        "SELECT AVG(overhead_pct) FROM routing_decisions \
         WHERE tenant_id = ? AND timestamp >= ? AND timestamp < ? AND overhead_pct IS NOT NULL",
    )
    .bind(tenant_id)
    .bind(start_time)
    .bind(end_time)
    .fetch_one(pool)
    .await
    {
        Ok(value) => value.unwrap_or(0.0),
        Err(e) => {
            if is_missing_table_error(&e) {
                return Ok((0.0, 0.0, 0.0));
            }
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ));
        }
    };

    let latencies: Vec<i64> = match sqlx::query_scalar::<_, i64>(
        "SELECT total_inference_latency_us FROM routing_decisions \
         WHERE tenant_id = ? AND timestamp >= ? AND timestamp < ? \
           AND total_inference_latency_us IS NOT NULL \
         ORDER BY total_inference_latency_us ASC LIMIT 1000",
    )
    .bind(tenant_id)
    .bind(start_time)
    .bind(end_time)
    .fetch_all(pool)
    .await
    {
        Ok(values) => values,
        Err(e) => {
            if is_missing_table_error(&e) {
                return Ok((0.0, 0.0, 0.0));
            }
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ));
        }
    };

    let latency_p95_ms = if latencies.is_empty() {
        0.0
    } else {
        let idx = ((latencies.len() - 1) as f32 * 0.95).floor() as usize;
        (latencies[idx] as f32) / 1000.0
    };

    let seconds = duration.num_seconds().max(1) as f32;
    let throughput = (total_decisions as f32) / seconds;

    Ok((latency_p95_ms, throughput, avg_overhead as f32))
}

fn parse_time_range(range: &str) -> Result<Duration, (StatusCode, Json<ErrorResponse>)> {
    match range {
        "7d" => Ok(Duration::days(7)),
        "30d" => Ok(Duration::days(30)),
        "90d" => Ok(Duration::days(90)),
        _ => Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid time_range")
                    .with_code("BAD_REQUEST")
                    .with_string_details(range.to_string()),
            ),
        )),
    }
}

fn validate_cpid_scoping(
    cpid: &str,
    tenant_id: &str,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if cpid != tenant_id {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("cpid must match tenant for code metrics")
                    .with_code("BAD_REQUEST")
                    .with_string_details(format!(
                        "requested cpid '{}' does not match tenant '{}'",
                        cpid, tenant_id
                    )),
            ),
        ));
    }

    Ok(())
}

async fn build_code_metrics(
    state: &AppState,
    tenant_id: &str,
    cpid: &str,
    time_range: &str,
) -> Result<CodeMetricsResponse, (StatusCode, Json<ErrorResponse>)> {
    build_code_metrics_at(state, tenant_id, cpid, time_range, Utc::now()).await
}

async fn build_code_metrics_at(
    state: &AppState,
    tenant_id: &str,
    cpid: &str,
    time_range: &str,
    end_time: chrono::DateTime<Utc>,
) -> Result<CodeMetricsResponse, (StatusCode, Json<ErrorResponse>)> {
    let duration = parse_time_range(time_range)?;
    let start = end_time - duration;
    let prev_start = start - duration;

    let start_str = start.to_rfc3339();
    let end_str = end_time.to_rfc3339();
    let prev_start_str = prev_start.to_rfc3339();
    let prev_end_str = start.to_rfc3339();

    let stats = fetch_patch_stats(state.db.pool(), tenant_id, &start_str, &end_str).await?;
    let prev_stats =
        fetch_patch_stats(state.db.pool(), tenant_id, &prev_start_str, &prev_end_str).await?;

    let acceptance_rate = if stats.total > 0 {
        stats.completed as f32 / stats.total as f32
    } else {
        0.0
    };
    let prev_acceptance_rate = if prev_stats.total > 0 {
        prev_stats.completed as f32 / prev_stats.total as f32
    } else {
        0.0
    };
    let acceptance_trend = acceptance_rate - prev_acceptance_rate;

    let regression_rate = if stats.total > 0 {
        (stats.failed + stats.rolled_back) as f32 / stats.total as f32
    } else {
        0.0
    };

    let compile_success = if stats.validation_total > 0 {
        stats.compile_success as f32 / stats.validation_total as f32
    } else {
        acceptance_rate
    };

    let test_pass_rate = if stats.validation_total > 0 {
        stats.tests_passed as f32 / stats.validation_total as f32
    } else {
        acceptance_rate
    };

    let evidence_coverage = if stats.validation_total > 0 {
        stats.evidence_present as f32 / stats.validation_total as f32
    } else {
        0.0
    };

    let follow_up_fixes_rate = if stats.total > 0 {
        stats.follow_up_fixes as f32 / stats.total as f32
    } else {
        0.0
    };

    let secret_violations =
        fetch_secret_violations(state.db.pool(), tenant_id, &start_str, &end_str).await?;

    let (latency_p95_ms, throughput_req_per_sec, router_overhead_pct) =
        fetch_router_metrics(state.db.pool(), tenant_id, &start_str, &end_str, duration).await?;

    Ok(CodeMetricsResponse {
        cpid: cpid.to_string(),
        time_range: time_range.to_string(),
        acceptance_rate,
        acceptance_trend,
        compile_success,
        test_pass_rate,
        regression_rate,
        evidence_coverage,
        follow_up_fixes_rate,
        secret_violations,
        latency_p95_ms,
        throughput_req_per_sec,
        router_overhead_pct,
    })
}

// ========== Handlers ==========

/// Get quality metrics
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/metrics/quality",
    responses(
        (status = 200, description = "Quality metrics", body = QualityMetricsResponse)
    )
)]
pub async fn get_quality_metrics(
    Extension(claims): Extension<Claims>,
) -> Result<Json<QualityMetricsResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: All roles can view metrics
    crate::permissions::require_permission(&claims, crate::permissions::Permission::MetricsView)?;

    // Stub implementation - would compute from telemetry
    Ok(Json(QualityMetricsResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        arr: 0.95,
        ecs5: 0.82,
        hlr: 0.02,
        cr: 0.01,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Get adapter performance metrics
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/metrics/adapters",
    responses(
        (status = 200, description = "Adapter metrics", body = AdapterMetricsResponse)
    )
)]
pub async fn get_adapter_metrics(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<AdapterMetricsResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: All roles can view metrics
    crate::permissions::require_permission(&claims, crate::permissions::Permission::MetricsView)?;

    let adapters = state
        .db
        .list_adapters_for_tenant(&claims.tenant_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to list adapters")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let mut performances = Vec::new();
    for adapter in adapters {
        let (total, selected, avg_gate) = state
            .db
            .get_adapter_stats(
                &claims.tenant_id,
                adapter.adapter_id.as_ref().unwrap_or(&adapter.id),
            )
            .await
            .unwrap_or((0, 0, 0.0));

        let activation_rate = if total > 0 {
            (selected as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        performances.push(AdapterPerformance {
            adapter_id: adapter
                .adapter_id
                .clone()
                .unwrap_or_else(|| adapter.id.clone()),
            name: adapter.name,
            activation_rate,
            avg_gate_value: avg_gate,
            total_requests: total,
        });
    }

    Ok(Json(AdapterMetricsResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        adapters: performances,
    }))
}

/// Get system metrics
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/metrics/system",
    responses(
        (status = 200, description = "System metrics", body = SystemMetricsResponse)
    )
)]
pub async fn get_system_metrics(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<SystemMetricsResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: All roles can view metrics
    crate::permissions::require_permission(&claims, crate::permissions::Permission::MetricsView)?;

    use adapteros_system_metrics::SystemMetricsCollector;
    use std::time::{SystemTime, UNIX_EPOCH};

    // Collect system metrics
    let mut collector = SystemMetricsCollector::new();
    let metrics = collector.collect_metrics();
    let load_avg = collector.load_average();

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System time before UNIX epoch")
        .as_secs();

    // Collect additional metrics for frontend compatibility
    let active_workers =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM workers WHERE status = 'active'")
            .fetch_one(state.db.pool())
            .await
            .unwrap_or(0) as i32;

    let requests_per_second = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM request_log WHERE timestamp > datetime('now', '-1 minute')",
    )
    .fetch_one(state.db.pool())
    .await
    .map(|count| count as f32 / 60.0)
    .unwrap_or(0.0);

    let avg_latency_ms = sqlx::query_scalar::<_, Option<f64>>(
        "SELECT AVG(latency_ms) FROM request_log WHERE timestamp > datetime('now', '-5 minutes')",
    )
    .fetch_one(state.db.pool())
    .await
    .unwrap_or(None)
    .unwrap_or(0.0) as f32;

    // Calculate active sessions count
    let active_sessions = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM chat_sessions WHERE updated_at > datetime('now', '-30 minutes')",
    )
    .fetch_one(state.db.pool())
    .await
    .unwrap_or(0) as i32;

    // Calculate error rate from recent requests
    let error_rate = {
        let total = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM request_log WHERE timestamp > datetime('now', '-5 minutes')",
        )
        .fetch_one(state.db.pool())
        .await
        .unwrap_or(0);

        if total > 0 {
            let errors = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM request_log WHERE timestamp > datetime('now', '-5 minutes') AND status_code >= 500",
            )
            .fetch_one(state.db.pool())
            .await
            .unwrap_or(0);
            Some((errors as f32) / (total as f32))
        } else {
            Some(0.0)
        }
    };

    // Tokens per second would come from inference telemetry - use 0.0 as default
    let tokens_per_second: f32 = 0.0;

    // Calculate p95 latency
    let latency_p95_ms = sqlx::query_scalar::<_, Option<f64>>(
        "SELECT latency_ms FROM request_log WHERE timestamp > datetime('now', '-5 minutes') ORDER BY latency_ms DESC LIMIT 1 OFFSET (SELECT COUNT(*) * 5 / 100 FROM request_log WHERE timestamp > datetime('now', '-5 minutes'))",
    )
    .fetch_one(state.db.pool())
    .await
    .unwrap_or(None)
    .map(|v| v as f32);

    Ok(Json(SystemMetricsResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        cpu_usage: metrics.cpu_usage as f32,
        memory_usage: metrics.memory_usage as f32,
        active_workers,
        requests_per_second,
        avg_latency_ms,
        disk_usage: metrics.disk_io.usage_percent,
        network_bandwidth: metrics.network_io.bandwidth_mbps,
        gpu_utilization: metrics.gpu_metrics.utilization.unwrap_or(0.0) as f32,
        uptime_seconds: collector.uptime_seconds(),
        process_count: collector.process_count(),
        load_average: LoadAverageResponse {
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
            load_1min: load_avg.0,
            load_5min: load_avg.1,
            load_15min: load_avg.2,
        },
        timestamp,
        // Additional fields for frontend compatibility
        cpu_usage_percent: Some(metrics.cpu_usage as f32),
        memory_usage_percent: Some(metrics.memory_usage as f32),
        tokens_per_second: Some(tokens_per_second),
        error_rate,
        active_sessions: Some(active_sessions),
        latency_p95_ms,
    }))
}

/// Get code metrics for dashboards
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/metrics/code",
    request_body = CodeMetricsRequest,
    responses(
        (status = 200, description = "Code metrics", body = CodeMetricsResponse)
    )
)]
pub async fn get_code_metrics(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CodeMetricsRequest>,
) -> Result<Json<CodeMetricsResponse>, (StatusCode, Json<ErrorResponse>)> {
    crate::permissions::require_permission(&claims, crate::permissions::Permission::MetricsView)?;

    if req.cpid.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("cpid is required").with_code("BAD_REQUEST")),
        ));
    }

    validate_cpid_scoping(&req.cpid, &claims.tenant_id)?;

    let metrics = build_code_metrics(&state, &claims.tenant_id, &req.cpid, &req.time_range).await?;

    Ok(Json(metrics))
}

/// Compare code metrics between two CPIDs
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/metrics/compare",
    request_body = CompareMetricsRequest,
    responses(
        (status = 200, description = "Metrics comparison", body = CompareMetricsResponse)
    )
)]
pub async fn compare_metrics(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CompareMetricsRequest>,
) -> Result<Json<CompareMetricsResponse>, (StatusCode, Json<ErrorResponse>)> {
    crate::permissions::require_permission(&claims, crate::permissions::Permission::MetricsView)?;

    if req.old_cpid.is_empty() || req.new_cpid.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("old_cpid and new_cpid are required").with_code("BAD_REQUEST")),
        ));
    }

    if req.old_cpid != req.new_cpid {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("code metrics compare uses time windows within one tenant")
                    .with_code("BAD_REQUEST")
                    .with_string_details(
                        "code metrics are tenant-scoped; use the active tenant cpid".to_string(),
                    ),
            ),
        ));
    }

    validate_cpid_scoping(&req.old_cpid, &claims.tenant_id)?;

    let time_range = "30d";
    let duration = parse_time_range(time_range)?;
    let now = Utc::now();
    let previous_end = now - duration;

    let metrics_old = build_code_metrics_at(
        &state,
        &claims.tenant_id,
        &req.old_cpid,
        time_range,
        previous_end,
    )
    .await?;
    let metrics_new =
        build_code_metrics_at(&state, &claims.tenant_id, &req.new_cpid, time_range, now).await?;

    let mut improvements = Vec::new();
    let mut regressions = Vec::new();

    let comparisons: [(&str, f32, f32, f32, bool); 11] = [
        (
            "acceptance_rate",
            metrics_old.acceptance_rate,
            metrics_new.acceptance_rate,
            0.01,
            true,
        ),
        (
            "compile_success",
            metrics_old.compile_success,
            metrics_new.compile_success,
            0.01,
            true,
        ),
        (
            "test_pass_rate",
            metrics_old.test_pass_rate,
            metrics_new.test_pass_rate,
            0.01,
            true,
        ),
        (
            "evidence_coverage",
            metrics_old.evidence_coverage,
            metrics_new.evidence_coverage,
            0.01,
            true,
        ),
        (
            "throughput_req_per_sec",
            metrics_old.throughput_req_per_sec,
            metrics_new.throughput_req_per_sec,
            0.1,
            true,
        ),
        (
            "acceptance_trend",
            metrics_old.acceptance_trend,
            metrics_new.acceptance_trend,
            0.01,
            true,
        ),
        (
            "regression_rate",
            metrics_old.regression_rate,
            metrics_new.regression_rate,
            0.01,
            false,
        ),
        (
            "latency_p95_ms",
            metrics_old.latency_p95_ms,
            metrics_new.latency_p95_ms,
            1.0,
            false,
        ),
        (
            "router_overhead_pct",
            metrics_old.router_overhead_pct,
            metrics_new.router_overhead_pct,
            0.5,
            false,
        ),
        (
            "follow_up_fixes_rate",
            metrics_old.follow_up_fixes_rate,
            metrics_new.follow_up_fixes_rate,
            0.01,
            false,
        ),
        (
            "secret_violations",
            metrics_old.secret_violations as f32,
            metrics_new.secret_violations as f32,
            1.0,
            false,
        ),
    ];

    for (name, old_value, new_value, threshold, higher_is_better) in comparisons {
        let diff = new_value - old_value;
        if diff.abs() < threshold {
            continue;
        }
        if higher_is_better {
            if diff > 0.0 {
                improvements.push(format!("{} improved by {:.3}", name, diff));
            } else {
                regressions.push(format!("{} declined by {:.3}", name, diff));
            }
        } else if diff < 0.0 {
            improvements.push(format!("{} improved by {:.3}", name, diff.abs()));
        } else {
            regressions.push(format!("{} worsened by {:.3}", name, diff));
        }
    }

    Ok(Json(CompareMetricsResponse {
        old_cpid: req.old_cpid,
        new_cpid: req.new_cpid,
        metrics_old,
        metrics_new,
        improvements,
        regressions,
    }))
}

/// Prometheus/OpenMetrics endpoint
/// Note: This endpoint requires bearer token authentication via Authorization header.
/// Authentication is checked in the route layer, not in the handler itself.
pub async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    // Check if metrics are enabled
    let metrics_enabled = {
        let config = match state.config.read() {
            Ok(cfg) => cfg,
            Err(e) => {
                tracing::error!("Failed to acquire config read lock: {}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("internal error")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
                    .into_response();
            }
        };
        config.metrics.enabled
    };

    if !metrics_enabled {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("metrics disabled").with_code("INTERNAL_ERROR")),
        )
            .into_response();
    }

    // Update worker metrics from database
    if let Err(e) = state
        .metrics_exporter
        .update_worker_metrics(&state.db)
        .await
    {
        tracing::warn!("Failed to update worker metrics: {}", e);
    }

    // Update alert metrics from database
    {
        use adapteros_db::process_monitoring::{AlertFilters, ProcessAlert};

        let filters = AlertFilters::default();
        match ProcessAlert::list(state.db.pool(), filters).await {
            Ok(alerts) => {
                let alert_tuples: Vec<(String, String, String, String, String)> = alerts
                    .iter()
                    .map(|a| {
                        (
                            a.title.clone(),
                            format!("{:?}", a.severity).to_lowercase(),
                            a.tenant_id.clone(),
                            a.worker_id.clone(),
                            format!("{:?}", a.status).to_lowercase(),
                        )
                    })
                    .collect();
                state.metrics_exporter.update_alert_metrics(&alert_tuples);
            }
            Err(e) => {
                tracing::warn!("Failed to fetch alerts for metrics: {}", e);
            }
        }
    }

    // Render metrics
    let metrics = match state.metrics_exporter.render() {
        Ok(m) => m,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to render metrics")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
                .into_response();
        }
    };

    (
        StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4",
        )],
        metrics,
    )
        .into_response()
}
