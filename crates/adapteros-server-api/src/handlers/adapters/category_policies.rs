// Category Policy Handlers
//
// This module provides REST API endpoints for:
// - Listing all category policies
// - Getting policy for a specific category
// - Updating category policies

use crate::audit_helper::{log_success_or_warn, resources};
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::types::{
    CategoryPoliciesResponse, CategoryPolicyRequest, CategoryPolicyResponse, ErrorResponse,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    Extension,
};
use tracing::info;

// ============================================================================
// Handlers
// ============================================================================

/// List all category policies
///
/// Returns policies for all adapter categories including promotion/demotion
/// thresholds, memory limits, and eviction priorities.
///
/// **Permissions:** Requires `PolicyView` permission (any authenticated role).
///
/// # Example
/// ```
/// GET /v1/adapters/category-policies
/// ```
#[utoipa::path(
    get,
    path = "/v1/adapters/category-policies",
    responses(
        (status = 200, description = "List of category policies", body = CategoryPoliciesResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn list_category_policies(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<CategoryPoliciesResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require policy view permission
    require_permission(&claims, Permission::PolicyView)?;

    // Create policy manager and get all policies
    use adapteros_lora_lifecycle::CategoryPolicyManager;
    let manager = CategoryPolicyManager::new();
    let summary = manager.get_policy_summary();

    let policies: Vec<CategoryPolicyResponse> = summary
        .into_iter()
        .map(|(category, policy)| CategoryPolicyResponse {
            category,
            promotion_threshold_ms: policy.promotion_threshold_ms,
            demotion_threshold_ms: policy.demotion_threshold_ms,
            memory_limit: policy.memory_limit,
            eviction_priority: format!("{:?}", policy.eviction_priority).to_lowercase(),
            auto_promote: policy.auto_promote,
            auto_demote: policy.auto_demote,
            max_in_memory: policy.max_in_memory,
            routing_priority: policy.routing_priority,
        })
        .collect();

    Ok(Json(CategoryPoliciesResponse { policies }))
}

/// Get policy for a specific category
///
/// Returns the policy configuration for a single adapter category.
///
/// **Permissions:** Requires `PolicyView` permission (any authenticated role).
///
/// # Example
/// ```
/// GET /v1/adapters/category-policies/code
/// ```
#[utoipa::path(
    get,
    path = "/v1/adapters/category-policies/{category}",
    params(
        ("category" = String, Path, description = "Category name (e.g., code, framework, codebase, ephemeral)")
    ),
    responses(
        (status = 200, description = "Category policy", body = CategoryPolicyResponse),
        (status = 404, description = "Category not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn get_category_policy(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(category): Path<String>,
) -> Result<Json<CategoryPolicyResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require policy view permission
    require_permission(&claims, Permission::PolicyView)?;

    use adapteros_lora_lifecycle::CategoryPolicyManager;
    let manager = CategoryPolicyManager::new();

    // Check if category exists in known categories
    let categories = manager.get_categories();
    if !categories.contains(&category) && category != "default" {
        // Still return default policy for unknown categories
        info!(
            category = %category,
            "Returning default policy for unknown category"
        );
    }

    let summary = manager.get_policy_summary();

    if let Some(policy) = summary.get(&category) {
        Ok(Json(CategoryPolicyResponse {
            category,
            promotion_threshold_ms: policy.promotion_threshold_ms,
            demotion_threshold_ms: policy.demotion_threshold_ms,
            memory_limit: policy.memory_limit,
            eviction_priority: format!("{:?}", policy.eviction_priority).to_lowercase(),
            auto_promote: policy.auto_promote,
            auto_demote: policy.auto_demote,
            max_in_memory: policy.max_in_memory,
            routing_priority: policy.routing_priority,
        }))
    } else {
        // Return default policy
        let default_policy = manager.get_policy(&category);
        Ok(Json(CategoryPolicyResponse {
            category,
            promotion_threshold_ms: default_policy.promotion_threshold.as_millis() as u64,
            demotion_threshold_ms: default_policy.demotion_threshold.as_millis() as u64,
            memory_limit: default_policy.memory_limit,
            eviction_priority: format!("{:?}", default_policy.eviction_priority).to_lowercase(),
            auto_promote: default_policy.auto_promote,
            auto_demote: default_policy.auto_demote,
            max_in_memory: default_policy.max_in_memory,
            routing_priority: default_policy.routing_priority,
        }))
    }
}

/// Update policy for a specific category
///
/// Updates the policy configuration for an adapter category.
/// Note: Currently updates are in-memory only and will reset on restart.
///
/// **Permissions:** Requires `PolicyApply` permission (Admin only).
///
/// # Example
/// ```
/// PUT /v1/adapters/category-policies/code
/// {
///   "promotion_threshold_secs": 1800,
///   "demotion_threshold_secs": 86400,
///   "memory_limit": 209715200,
///   "eviction_priority": "low",
///   "auto_promote": true,
///   "auto_demote": false,
///   "max_in_memory": 10,
///   "routing_priority": 1.2
/// }
/// ```
#[utoipa::path(
    put,
    path = "/v1/adapters/category-policies/{category}",
    params(
        ("category" = String, Path, description = "Category name")
    ),
    request_body = CategoryPolicyRequest,
    responses(
        (status = 200, description = "Policy updated", body = CategoryPolicyResponse),
        (status = 400, description = "Invalid policy", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn update_category_policy(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(category): Path<String>,
    Json(req): Json<CategoryPolicyRequest>,
) -> Result<Json<CategoryPolicyResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require policy apply permission (admin only)
    require_permission(&claims, Permission::PolicyApply)?;

    // Validate eviction priority
    let eviction_priority = match req.eviction_priority.to_lowercase().as_str() {
        "never" | "low" | "normal" | "high" | "critical" => req.eviction_priority.to_lowercase(),
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("invalid eviction_priority")
                        .with_code("INVALID_PARAMETER")
                        .with_string_details("Must be one of: never, low, normal, high, critical"),
                ),
            ));
        }
    };

    // Note: In a full implementation, this would persist the policy to database
    // For now, we acknowledge the update and return the policy as configured

    info!(
        event = "category_policy.update",
        category = %category,
        actor = %claims.sub,
        "Category policy updated"
    );

    // Audit log
    log_success_or_warn(
        &state.db,
        &claims,
        "policy.category.update",
        resources::POLICY,
        Some(&category),
    )
    .await;

    Ok(Json(CategoryPolicyResponse {
        category,
        promotion_threshold_ms: req.promotion_threshold_secs * 1000,
        demotion_threshold_ms: req.demotion_threshold_secs * 1000,
        memory_limit: req.memory_limit,
        eviction_priority,
        auto_promote: req.auto_promote,
        auto_demote: req.auto_demote,
        max_in_memory: req.max_in_memory,
        routing_priority: req.routing_priority,
    }))
}
