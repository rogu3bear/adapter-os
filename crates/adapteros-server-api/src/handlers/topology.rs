use crate::api_error::{ApiError, ApiResult};
use crate::auth::Claims;
use crate::state::AppState;
use adapteros_api_types::{topology::PredictedPathNode, ErrorResponse, TopologyGraph};
use adapteros_lora_router::{AdapterInfo, Router, RouterWeights};
use adapteros_model_hub::manifest::{ManifestV3, RouterCfg};
use axum::{
    extract::{Query, State},
    Extension, Json,
};
use serde::Deserialize;
use std::collections::HashMap;
use tracing::{debug, error, warn};

#[derive(Debug, Deserialize)]
pub struct TopologyQuery {
    pub preview_text: Option<String>,
}

/// Return the full semantic topology graph for UI rendering.
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/topology",
    params(
        ("preview_text" = Option<String>, Query, description = "Optional text to preview the deterministic router path")
    ),
    responses(
        (status = 200, description = "Semantic topology graph", body = TopologyGraph),
        (status = 500, description = "Topology unavailable", body = ErrorResponse)
    )
)]
pub async fn get_topology(
    State(state): State<AppState>,
    maybe_claims: Option<Extension<Claims>>,
    Query(query): Query<TopologyQuery>,
) -> ApiResult<TopologyGraph> {
    let graph = state
        .db
        .get_topology_graph()
        .await
        .map_err(ApiError::db_error)?;

    // Map DB types to API types (structures are isomorphic but live in different crates).
    let adjacency = graph
        .adjacency
        .into_iter()
        .map(|(from, edges)| {
            let mapped = edges
                .into_iter()
                .map(|edge| adapteros_api_types::topology::AdjacencyEdge {
                    to_cluster_id: edge.to_cluster_id,
                    probability: edge.probability,
                })
                .collect();
            (from, mapped)
        })
        .collect();

    let adapters: Vec<adapteros_api_types::topology::AdapterTopology> = graph
        .adapters
        .into_iter()
        .map(|a| adapteros_api_types::topology::AdapterTopology {
            adapter_id: a.adapter_id,
            name: a.name,
            cluster_ids: a.cluster_ids,
            transition_probabilities: a.transition_probabilities,
        })
        .collect();

    let cluster_lookup: HashMap<String, String> = adapters
        .iter()
        .filter_map(|adapter| {
            adapter
                .cluster_ids
                .first()
                .cloned()
                .map(|cluster| (adapter.adapter_id.clone(), cluster))
        })
        .collect();

    let mut response = TopologyGraph {
        clusters_version: graph.clusters_version,
        clusters: graph
            .clusters
            .into_iter()
            .map(|c| adapteros_api_types::topology::ClusterDefinition {
                id: c.id,
                description: c.description,
                default_adapter_id: c.default_adapter_id,
                version: c.version,
            })
            .collect(),
        adapters,
        adjacency,
        predicted_path: None,
    };

    if let Some(raw_text) = query.preview_text.as_deref() {
        let preview_text = raw_text.trim();
        if !preview_text.is_empty() {
            if let Some(Extension(claims)) = maybe_claims {
                match project_predicted_path(
                    &state,
                    &claims.tenant_id,
                    preview_text,
                    &cluster_lookup,
                )
                .await
                {
                    Ok(predicted) if !predicted.is_empty() => {
                        response.predicted_path = Some(predicted);
                    }
                    Ok(_) => {
                        debug!(
                            tenant_id = %claims.tenant_id,
                            "Router dry-run returned no adapters for preview_text"
                        );
                    }
                    Err(err) => {
                        warn!(
                            tenant_id = %claims.tenant_id,
                            error = ?err,
                            "Failed to project router path for topology request"
                        );
                    }
                }
            } else {
                debug!("Skipping predicted_path projection: no auth context provided");
            }
        }
    }

    Ok(Json(response))
}

fn should_skip_adapter(adapter: &adapteros_db::adapters::Adapter) -> bool {
    adapter.active == 0 || adapter.archived_at.is_some() || adapter.purged_at.is_some()
}

fn parse_languages(raw: Option<&str>) -> Vec<usize> {
    raw.and_then(|json| {
        serde_json::from_str::<Vec<usize>>(json)
            .map_err(|e| {
                error!(
                    target: "api.topology",
                    error = %e,
                    json = %json,
                    "Failed to parse languages JSON"
                );
                e
            })
            .ok()
    })
    .unwrap_or_default()
}

fn parse_reasoning_specialties(raw: Option<&str>) -> Vec<String> {
    raw.and_then(|json| {
        serde_json::from_str::<serde_json::Value>(json)
            .map_err(|e| {
                error!(
                    target: "api.topology",
                    error = %e,
                    "Failed to parse metadata JSON for reasoning specialties"
                );
                e
            })
            .ok()
    })
    .and_then(|val| val.get("reasoning_specialties").cloned())
    .and_then(|value| {
        serde_json::from_value::<Vec<String>>(value.clone())
            .map_err(|e| {
                error!(
                    target: "api.topology",
                    error = %e,
                    "Failed to parse reasoning_specialties array"
                );
                e
            })
            .ok()
    })
    .unwrap_or_default()
}

fn adapter_to_router_info(adapter: &adapteros_db::adapters::Adapter) -> AdapterInfo {
    let scope_path = if adapter.scope.trim().is_empty() {
        None
    } else {
        Some(adapter.scope.clone())
    };

    AdapterInfo {
        id: adapter
            .adapter_id
            .as_deref()
            .unwrap_or(&adapter.id)
            .to_string(),
        stable_id: adapter.stable_id.unwrap_or(0) as u64,
        framework: adapter.framework.clone(),
        languages: parse_languages(adapter.languages_json.as_deref()),
        tier: adapter.tier.clone(),
        scope_path,
        lora_tier: None,
        base_model: adapter.base_model_id.clone(),
        recommended_for_moe: adapter.recommended_for_moe.unwrap_or(true),
        // Demo note: tag adapter metadata with `reasoning_specialties` and enable
        // reasoning_mode=true (chat UI sets backend=coreml) for conversation-driven hot-swaps.
        reasoning_specialties: parse_reasoning_specialties(adapter.metadata_json.as_deref()),
        adapter_type: adapter.adapter_type.clone(),
        stream_session_id: adapter.stream_session_id.clone(),
        base_adapter_id: adapter.base_adapter_id.clone(),
    }
}

async fn project_predicted_path(
    state: &AppState,
    tenant_id: &str,
    preview_text: &str,
    cluster_lookup: &HashMap<String, String>,
) -> Result<Vec<PredictedPathNode>, ApiError> {
    let router_cfg = load_router_cfg(state).await;
    let mut router = Router::new_with_weights(
        RouterWeights::default(),
        router_cfg.k_sparse.max(1),
        router_cfg.tau,
        router_cfg.entropy_floor,
    );
    router.set_full_log_tokens(router_cfg.sample_tokens_full);
    router.set_orthogonal_constraints(
        router_cfg.orthogonal_constraints,
        router_cfg.diversity_threshold,
        router_cfg.orthogonal_penalty,
        router_cfg.sample_tokens_full,
    );
    router.set_compression_ratio(router_cfg.compression_ratio);
    router.set_shared_downsample(router_cfg.shared_downsample);

    if let Some((stack_id, adapter_ids)) = load_default_stack(state, tenant_id).await {
        router.set_active_stack(Some(stack_id), Some(adapter_ids), None);
    }

    let adapters = state
        .db
        .list_adapters_for_tenant(tenant_id)
        .await
        .map_err(ApiError::db_error)?;
    let adapter_infos: Vec<AdapterInfo> = adapters
        .iter()
        .filter(|adapter| !should_skip_adapter(adapter))
        .map(adapter_to_router_info)
        .collect();

    if adapter_infos.is_empty() {
        return Ok(Vec::new());
    }

    let decision = router
        .dry_run(preview_text, &adapter_infos)
        .map_err(|e| ApiError::internal(e.to_string()))?;

    let gates = decision.gates_f32();
    let predicted_path = decision
        .indices
        .iter()
        .enumerate()
        .filter_map(|(pos, adapter_idx)| {
            adapter_infos.get(*adapter_idx as usize).map(|info| {
                let id = info.id.clone();
                PredictedPathNode {
                    id: id.clone(),
                    adapter_id: Some(id.clone()),
                    cluster_id: cluster_lookup.get(&id).cloned(),
                    confidence: gates.get(pos).copied(),
                    kind: Some("adapter".to_string()),
                }
            })
        })
        .collect();

    Ok(predicted_path)
}

async fn load_router_cfg(state: &AppState) -> RouterCfg {
    if let Some(hash) = state.manifest_hash.clone() {
        if let Ok(Some(record)) = state.db.get_manifest_by_hash(&hash).await {
            if let Ok(manifest) = serde_json::from_str::<ManifestV3>(&record.body_json) {
                return manifest.router;
            }
        }
    }
    default_router_cfg()
}

async fn load_default_stack(state: &AppState, tenant_id: &str) -> Option<(String, Vec<String>)> {
    let stack_id = state
        .db
        .get_default_stack(tenant_id)
        .await
        .map_err(|e| {
            error!(
                target: "api.topology",
                error = %e,
                tenant_id = %tenant_id,
                "Failed to get default stack"
            );
            e
        })
        .ok()
        .flatten()?;
    let stack = state
        .db
        .get_stack(tenant_id, &stack_id)
        .await
        .map_err(|e| {
            error!(
                target: "api.topology",
                error = %e,
                tenant_id = %tenant_id,
                stack_id = %stack_id,
                "Failed to get stack"
            );
            e
        })
        .ok()??;
    let adapter_ids = serde_json::from_str(&stack.adapter_ids_json)
        .map_err(|e| {
            error!(
                target: "api.topology",
                error = %e,
                stack_id = %stack.id,
                "Failed to parse adapter_ids_json"
            );
            e
        })
        .unwrap_or_default();
    Some((stack.id, adapter_ids))
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
