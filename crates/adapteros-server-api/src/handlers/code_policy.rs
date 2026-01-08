use crate::auth::Claims;
use crate::middleware::require_any_role;
use crate::state::AppState;
use crate::types::{CodePolicy, ErrorResponse, GetCodePolicyResponse, UpdateCodePolicyRequest};
use adapteros_db::code_policies::CodePolicy as DbCodePolicy;
use adapteros_db::users::Role;
use axum::{extract::State, http::StatusCode, Extension, Json};
use serde_json::json;

fn parse_policy_from_db(policy: DbCodePolicy) -> CodePolicy {
    let defaults = CodePolicy::default();
    let evidence = serde_json::from_str::<serde_json::Value>(&policy.evidence_config_json)
        .unwrap_or_else(|_| json!({}));
    let auto_apply = serde_json::from_str::<serde_json::Value>(&policy.auto_apply_config_json)
        .unwrap_or_else(|_| json!({}));
    let paths = serde_json::from_str::<serde_json::Value>(&policy.path_permissions_json)
        .unwrap_or_else(|_| json!({}));
    let secrets = serde_json::from_str::<serde_json::Value>(&policy.secret_patterns_json)
        .unwrap_or_else(|_| json!({}));
    let limits = serde_json::from_str::<serde_json::Value>(&policy.patch_limits_json)
        .unwrap_or_else(|_| json!({}));

    CodePolicy {
        min_evidence_spans: evidence
            .get("min_evidence_spans")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(defaults.min_evidence_spans),
        allow_auto_apply: auto_apply
            .get("allow_auto_apply")
            .and_then(|v| v.as_bool())
            .unwrap_or(defaults.allow_auto_apply),
        test_coverage_min: evidence
            .get("test_coverage_min")
            .and_then(|v| v.as_f64())
            .map(|v| v as f32)
            .unwrap_or(defaults.test_coverage_min),
        path_allowlist: paths
            .get("allowlist")
            .and_then(|v| v.as_array())
            .map(|values| {
                values
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or(defaults.path_allowlist),
        path_denylist: paths
            .get("denylist")
            .and_then(|v| v.as_array())
            .map(|values| {
                values
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or(defaults.path_denylist),
        secret_patterns: secrets
            .get("patterns")
            .and_then(|v| v.as_array())
            .map(|values| {
                values
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or(defaults.secret_patterns),
        max_patch_size: limits
            .get("max_patch_size")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(defaults.max_patch_size),
    }
}

fn validate_code_policy(policy: &CodePolicy) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if policy.min_evidence_spans == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("min_evidence_spans must be at least 1")
                    .with_code("BAD_REQUEST"),
            ),
        ));
    }

    if !policy.test_coverage_min.is_finite() || !(0.0..=1.0).contains(&policy.test_coverage_min) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("test_coverage_min must be between 0.0 and 1.0")
                    .with_code("BAD_REQUEST"),
            ),
        ));
    }

    if policy.max_patch_size == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("max_patch_size must be at least 1").with_code("BAD_REQUEST")),
        ));
    }

    if policy.path_allowlist.is_empty() && policy.path_denylist.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("path_allowlist or path_denylist must be provided")
                    .with_code("BAD_REQUEST"),
            ),
        ));
    }

    if policy
        .path_allowlist
        .iter()
        .chain(policy.path_denylist.iter())
        .any(|pattern| pattern.trim().is_empty())
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("path patterns cannot be empty").with_code("BAD_REQUEST")),
        ));
    }

    if policy
        .secret_patterns
        .iter()
        .any(|pattern| pattern.trim().is_empty())
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("secret patterns cannot be empty").with_code("BAD_REQUEST")),
        ));
    }

    Ok(())
}

#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/code-policy",
    responses(
        (status = 200, description = "Code policy", body = GetCodePolicyResponse)
    )
)]
pub async fn get_code_policy(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<GetCodePolicyResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator, Role::Viewer])?;

    let policy = match state.db.get_code_policy(&claims.tenant_id).await {
        Ok(Some(policy)) => parse_policy_from_db(policy),
        Ok(None) => CodePolicy::default(),
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ))
        }
    };

    Ok(Json(GetCodePolicyResponse { policy }))
}

#[utoipa::path(
    tag = "system",
    put,
    path = "/v1/code-policy",
    request_body = UpdateCodePolicyRequest,
    responses(
        (status = 200, description = "Code policy updated")
    )
)]
pub async fn update_code_policy(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<UpdateCodePolicyRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    validate_code_policy(&req.policy)?;

    let evidence_config = json!({
        "min_evidence_spans": req.policy.min_evidence_spans,
        "test_coverage_min": req.policy.test_coverage_min
    });
    let auto_apply_config = json!({
        "allow_auto_apply": req.policy.allow_auto_apply
    });
    let path_permissions = json!({
        "allowlist": req.policy.path_allowlist,
        "denylist": req.policy.path_denylist
    });
    let secret_patterns = json!({
        "patterns": req.policy.secret_patterns
    });
    let patch_limits = json!({
        "max_patch_size": req.policy.max_patch_size
    });

    let evidence_config_json = serde_json::to_string(&evidence_config).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid evidence config")
                    .with_code("BAD_REQUEST")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;
    let auto_apply_config_json = serde_json::to_string(&auto_apply_config).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid auto-apply config")
                    .with_code("BAD_REQUEST")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;
    let path_permissions_json = serde_json::to_string(&path_permissions).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid path permissions config")
                    .with_code("BAD_REQUEST")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;
    let secret_patterns_json = serde_json::to_string(&secret_patterns).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid secret patterns config")
                    .with_code("BAD_REQUEST")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;
    let patch_limits_json = serde_json::to_string(&patch_limits).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid patch limits config")
                    .with_code("BAD_REQUEST")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    state
        .db
        .save_code_policy(
            &claims.tenant_id,
            &evidence_config_json,
            &auto_apply_config_json,
            &path_permissions_json,
            &secret_patterns_json,
            &patch_limits_json,
        )
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
        })?;

    Ok(StatusCode::OK)
}
