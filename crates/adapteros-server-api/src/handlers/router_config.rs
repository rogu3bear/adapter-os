use crate::api_error::ApiError;
use crate::auth::Claims;
use crate::middleware::require_any_role;
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_api_types::RoutingPolicy;
use adapteros_db::{adapters::Adapter, users::Role};
use adapteros_model_hub::manifest::{Adapter as ManifestAdapter, ManifestV3, RouterCfg};
use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use tracing::debug;
use utoipa::ToSchema;

/// Router parameters as used by the worker manifest.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RouterParameters {
    pub k_sparse: usize,
    pub tau: f32,
    pub entropy_floor: f32,
    pub gate_quant: String,
    pub sample_tokens_full: usize,
    pub algorithm: String,
    pub warmup: bool,
}

/// Summary of an adapter the router may consider.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RouterAdapterSummary {
    pub adapter_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alpha: Option<f64>,
    /// True when this adapter is part of the tenant's default stack (effective routing set).
    pub in_default_stack: bool,
}

/// Summary of the effective stack used for routing (default stack, if configured).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RouterStackSummary {
    pub stack_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lifecycle_state: Option<String>,
    pub adapter_ids: Vec<String>,
}

/// Router configuration view aligned with the manifest and routing policy used in inference.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RouterConfigView {
    pub tenant_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_hash: Option<String>,
    pub router: RouterParameters,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routing_policy: Option<RoutingPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack: Option<RouterStackSummary>,
    pub adapters: Vec<RouterAdapterSummary>,
}

/// GET /v1/tenants/{tenant_id}/router/config
///
/// Returns the router parameters (k-sparse, entropy floor, quantization) sourced from
/// the active manifest plus the tenant routing policy and effective adapter set
/// (default stack) so the UI mirrors what inference uses.
#[utoipa::path(
    get,
    path = "/v1/tenants/{tenant_id}/router/config",
    params(
        ("tenant_id" = String, Path, description = "Tenant ID")
    ),
    responses(
        (status = 200, description = "Router configuration for the tenant", body = RouterConfigView),
        (status = 403, description = "Forbidden", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "routing",
    security(("bearer_token" = []))
)]
pub async fn get_router_config(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
) -> Result<Json<RouterConfigView>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator, Role::Viewer])?;
    let tenant_id = crate::id_resolver::resolve_any_id(&state.db, &tenant_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;
    validate_tenant_isolation(&claims, &tenant_id)?;

    // Execution policy (includes optional routing policy)
    let execution_policy = state
        .db
        .get_execution_policy_or_default(&tenant_id)
        .await
        .map_err(ApiError::db_error)?;

    // Active manifest (aligns with worker/router configuration)
    let manifest_hash = state.manifest_hash.clone();
    let manifest = if let Some(ref hash) = manifest_hash {
        match state.db.get_manifest_by_hash(hash).await {
            Ok(Some(record)) => serde_json::from_str::<ManifestV3>(&record.body_json).ok(),
            Ok(None) => None,
            Err(e) => {
                debug!(error = %e, manifest_hash = %hash, "Failed to load manifest for router config view");
                None
            }
        }
    } else {
        None
    };

    let router_cfg = manifest
        .as_ref()
        .map(|m| m.router.clone())
        .unwrap_or_else(default_router_cfg);

    // Resolve effective adapters via tenant default stack (same path used by inference fallback)
    let default_stack_id = state
        .db
        .get_default_stack(&tenant_id)
        .await
        .map_err(ApiError::db_error)?;

    let (stack_summary, effective_adapter_ids) = if let Some(stack_id) = default_stack_id.as_ref() {
        match state.db.get_stack(&tenant_id, stack_id).await {
            Ok(Some(stack)) => {
                let adapter_ids: Vec<String> =
                    serde_json::from_str(&stack.adapter_ids_json).unwrap_or_default();
                let summary = RouterStackSummary {
                    stack_id: stack.id.clone(),
                    version: Some(stack.version_number()),
                    lifecycle_state: Some(stack.lifecycle_state.clone()),
                    adapter_ids: adapter_ids.clone(),
                };
                (Some(summary), adapter_ids)
            }
            Ok(None) => (None, Vec::new()),
            Err(e) => {
                debug!(error = %e, "Failed to load default stack for router config view");
                (None, Vec::new())
            }
        }
    } else {
        (None, Vec::new())
    };

    // Fetch adapter metadata for tenant and build summaries for the effective set (or manifest adapters if no stack)
    let adapters = state
        .db
        .list_adapters_for_tenant(&tenant_id)
        .await
        .map_err(ApiError::db_error)?;
    let adapter_map: HashMap<String, Adapter> = adapters
        .into_iter()
        .map(|a| {
            let id = a.adapter_id.clone().unwrap_or_else(|| a.id.clone());
            (id, a)
        })
        .collect();

    let manifest_adapters: HashMap<String, ManifestAdapter> = manifest
        .as_ref()
        .map(|m| {
            m.adapters
                .iter()
                .map(|a| (a.id.clone(), a.clone()))
                .collect()
        })
        .unwrap_or_default();
    let effective_set: Vec<String> = if !effective_adapter_ids.is_empty() {
        effective_adapter_ids
    } else if !manifest_adapters.is_empty() {
        manifest_adapters.keys().cloned().collect()
    } else {
        // Fall back to all tenant adapters when no stack or manifest adapters are available
        adapter_map.keys().cloned().collect()
    };
    let effective_set_lookup: HashSet<String> = effective_set.iter().cloned().collect();

    let adapters_summary: Vec<RouterAdapterSummary> = effective_set
        .iter()
        .map(|adapter_id| {
            let db_adapter = adapter_map.get(adapter_id);
            let manifest_adapter = manifest_adapters.get(adapter_id);
            build_adapter_summary(
                adapter_id,
                db_adapter,
                manifest_adapter,
                &effective_set_lookup,
            )
        })
        .collect();

    let response = RouterConfigView {
        tenant_id,
        manifest_hash,
        router: RouterParameters {
            k_sparse: router_cfg.k_sparse,
            tau: router_cfg.tau,
            entropy_floor: router_cfg.entropy_floor,
            gate_quant: router_cfg.gate_quant.clone(),
            sample_tokens_full: router_cfg.sample_tokens_full,
            algorithm: router_cfg.algorithm.clone(),
            warmup: router_cfg.warmup,
        },
        routing_policy: execution_policy.routing,
        stack: stack_summary,
        adapters: adapters_summary,
    };

    Ok(Json(response))
}

fn build_adapter_summary(
    adapter_id: &str,
    db_adapter: Option<&Adapter>,
    manifest_adapter: Option<&ManifestAdapter>,
    effective_set: &HashSet<String>,
) -> RouterAdapterSummary {
    let name = db_adapter
        .map(|a| a.name.clone())
        .or_else(|| manifest_adapter.map(|m| m.id.clone()));

    let tier = db_adapter
        .map(|a| a.tier.clone())
        .or_else(|| manifest_adapter.map(|m| format!("{:?}", m.tier).to_lowercase()));

    let category = db_adapter
        .map(|a| a.category.clone())
        .or_else(|| manifest_adapter.map(|m| format!("{:?}", m.category).to_lowercase()));

    let scope = db_adapter
        .map(|a| a.scope.clone())
        .or_else(|| manifest_adapter.map(|m| format!("{:?}", m.scope).to_lowercase()));

    let rank = db_adapter
        .map(|a| a.rank)
        .or_else(|| manifest_adapter.map(|m| m.rank as i32));

    let alpha = db_adapter
        .map(|a| a.alpha)
        .or_else(|| manifest_adapter.map(|m| m.alpha as f64));

    RouterAdapterSummary {
        adapter_id: adapter_id.to_string(),
        name,
        tier,
        category,
        scope,
        rank,
        alpha,
        in_default_stack: effective_set.contains(adapter_id),
    }
}

fn default_router_cfg() -> RouterCfg {
    RouterCfg {
        k_sparse: 4,
        gate_quant: "q15".to_string(),
        entropy_floor: 0.02,
        tau: 1.0,
        sample_tokens_full: 128,
        warmup: false,
        algorithm: "weighted".to_string(),
        safe_mode: false,
        orthogonal_penalty: 0.1,
        shared_downsample: false,
        compression_ratio: 0.8,
        multi_path_enabled: false,
        diversity_threshold: 0.05,
        orthogonal_constraints: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_router_cfg_matches_expected_defaults() {
        let cfg = default_router_cfg();
        assert_eq!(cfg.k_sparse, 4);
        assert_eq!(cfg.gate_quant, "q15");
        assert!((cfg.entropy_floor - 0.02).abs() < f32::EPSILON);
        assert!((cfg.tau - 1.0).abs() < f32::EPSILON);
        assert_eq!(cfg.sample_tokens_full, 128);
        assert_eq!(cfg.algorithm, "weighted");
        assert!(!cfg.warmup);
    }
}
