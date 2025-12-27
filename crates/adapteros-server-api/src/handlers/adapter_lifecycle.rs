//! Adapter lifecycle management handlers
//!
//! Handlers for loading, unloading, and promoting adapters.

use crate::auth::Claims;
use crate::middleware::{require_any_role, require_role};
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::*;
use adapteros_core::{AosError, B3Hash};
use adapteros_db::users::Role;
use adapteros_types::training::LoraTier;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use serde_json::json;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

// Import helper functions from other modules
use super::adapter_utils::{
    guard_in_flight_requests, lora_scope_from_provenance, lora_tier_from_provenance, parse_hash_b3,
};

#[utoipa::path(
    post,
    path = "/v1/adapters/{adapter_id}/load",
    tag = "adapters",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Adapter loaded", body = AdapterResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse),
        (status = 500, description = "Failed to load adapter", body = ErrorResponse)
    )
)]
pub async fn load_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<AdapterResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: Operator, SRE, and Admin can load adapters
    crate::permissions::require_permission(&claims, crate::permissions::Permission::AdapterLoad)?;

    // Get adapter from database with tenant-scoped query
    let adapter = match state
        .db
        .get_adapter_for_tenant(&claims.tenant_id, &adapter_id)
        .await
    {
        Ok(Some(adapter)) => adapter,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("adapter not found").with_code("NOT_FOUND")),
            ));
        }
        Err(e) => {
            crate::audit_helper::log_failure_or_warn(
                &state.db,
                &claims,
                crate::audit_helper::actions::ADAPTER_LOAD,
                crate::audit_helper::resources::ADAPTER,
                Some(&adapter_id),
                &format!("Error: {}", e),
            )
            .await;
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to get adapter")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ));
        }
    };

    // Use lifecycle manager if available to update state to 'loading'
    if let Some(ref lifecycle) = state.lifecycle_manager {
        let manager = lifecycle.lock().await;
        if let Some(adapter_idx) = manager.get_adapter_idx(&adapter_id) {
            use adapteros_lora_lifecycle::AdapterState;
            // Update state to loading via lifecycle manager
            if let Err(e) = manager
                .update_adapter_state(adapter_idx, AdapterState::Cold, "user_request")
                .await
            {
                tracing::warn!(adapter_id = %adapter_id, error = %e, "Failed to update adapter state via lifecycle manager, continuing");
            }
        }
    } else {
        // Fallback: direct DB update if no lifecycle manager
        // Use transactional version for safety in handlers
        if let Err(e) = state
            .lifecycle_db()
            .update_adapter_state_tx_for_tenant(
                &adapter.tenant_id,
                &adapter_id,
                "loading",
                "user_request",
            )
            .await
        {
            crate::audit_helper::log_failure_or_warn(
                &state.db,
                &claims,
                crate::audit_helper::actions::ADAPTER_LOAD,
                crate::audit_helper::resources::ADAPTER,
                Some(&adapter_id),
                &format!("Error: {}", e),
            )
            .await;
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to update adapter state")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ));
        }
    }

    let expected_hash = match parse_hash_b3(&adapter.hash_b3) {
        Ok(hash) => hash,
        Err(e) => {
            crate::audit_helper::log_failure_or_warn(
                &state.db,
                &claims,
                crate::audit_helper::actions::ADAPTER_LOAD,
                crate::audit_helper::resources::ADAPTER,
                Some(&adapter_id),
                &format!("Error: {}", e),
            )
            .await;
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("invalid adapter hash")
                        .with_code("INVALID_HASH")
                        .with_string_details(format!("{}: {}", adapter.hash_b3, e)),
                ),
            ));
        }
    };
    let mut expected_hashes = HashMap::new();
    expected_hashes.insert(adapter.hash_b3.clone(), expected_hash);

    tracing::info!("Loading adapter {} ({})", adapter_id, adapter.name);

    // Actually load the adapter using LifecycleManager if available
    if let Some(ref lifecycle) = state.lifecycle_manager {
        let mut manager = lifecycle.lock().await;

        // Load adapter (updates internal state only)
        if let Err(e) = manager.get_or_reload(&adapter_id) {
            tracing::error!(adapter_id = %adapter_id, error = %e, "Failed to load adapter via lifecycle manager");
            // Must drop the lock before awaiting
            drop(manager);
            // Audit log: adapter load failure
            crate::audit_helper::log_failure_or_warn(
                &state.db,
                &claims,
                crate::audit_helper::actions::ADAPTER_LOAD,
                crate::audit_helper::resources::ADAPTER,
                Some(&adapter_id),
                &format!("Failed to load adapter: {}", e),
            )
            .await;
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to load adapter")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ));
        }

        // Update state (handles DB update if db is set)
        if let Some(adapter_idx) = manager.get_adapter_idx(&adapter_id) {
            use adapteros_lora_lifecycle::AdapterState;
            if let Err(e) = manager
                .update_adapter_state(adapter_idx, AdapterState::Cold, "loaded_via_api")
                .await
            {
                tracing::warn!(adapter_id = %adapter_id, error = %e, "Failed to update adapter state via lifecycle manager");
                // Fallback: update DB state directly
                // Note: This is a best-effort fallback, the adapter is already loaded
                let _ = state
                    .lifecycle_db()
                    .update_adapter_state_tx_for_tenant(
                        &adapter.tenant_id,
                        &adapter_id,
                        "cold",
                        "loaded_via_api",
                    )
                    .await;
            }

            // Note: Memory tracking is handled internally by lifecycle manager
        } else {
            return Err((
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("adapter not found in lifecycle manager")
                        .with_code("NOT_FOUND"),
                ),
            ));
        }
    } else {
        // Fallback: direct DB update if no lifecycle manager
        // Use transactional version for safety in handlers
        tracing::info!(adapter_id = %adapter_id, "simulating adapter load (no lifecycle manager)");
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        if let Err(e) = state
            .lifecycle_db()
            .update_adapter_state_tx_for_tenant(
                &adapter.tenant_id,
                &adapter_id,
                "warm",
                "simulated_load",
            )
            .await
        {
            crate::audit_helper::log_failure_or_warn(
                &state.db,
                &claims,
                crate::audit_helper::actions::ADAPTER_LOAD,
                crate::audit_helper::resources::ADAPTER,
                Some(&adapter_id),
                &format!("Error: {}", e),
            )
            .await;
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to update adapter state")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ));
        }

        tracing::info!(
            event = "adapter.load",
            adapter_id = %adapter_id,
            adapter_name = %adapter.name,
            "Adapter loaded successfully (simulated)"
        );
    }

    // Audit log: adapter load
    crate::audit_helper::log_success_or_warn(
        &state.db,
        &claims,
        crate::audit_helper::actions::ADAPTER_LOAD,
        crate::audit_helper::resources::ADAPTER,
        Some(&adapter_id),
    )
    .await;

    // Return the adapter with updated stats
    let (total, selected, avg_gate) = state
        .db
        .get_adapter_stats(&claims.tenant_id, &adapter_id)
        .await
        .unwrap_or((0, 0, 0.0));

    let selection_rate = if total > 0 {
        (selected as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    let lora_tier = lora_tier_from_provenance(&adapter.provenance_json);
    let lora_scope =
        lora_scope_from_provenance(&adapter.provenance_json, Some(adapter.scope.clone()));

    let adapter_id_val = adapter
        .adapter_id
        .clone()
        .unwrap_or_else(|| adapter.id.clone());
    Ok(Json(AdapterResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        id: adapter.id,
        adapter_id: adapter_id_val,
        name: adapter.name,
        hash_b3: adapter.hash_b3,
        rank: adapter.rank,
        tier: adapter.tier,
        assurance_tier: None,
        version: adapter.version.clone(),
        lifecycle_state: adapter.lifecycle_state.clone(),
        languages: serde_json::from_str(adapter.languages_json.as_deref().unwrap_or("[]"))
            .unwrap_or_default(),
        framework: adapter.framework,
        category: Some(adapter.category),
        scope: Some(adapter.scope),
        lora_tier,
        lora_strength: adapter.lora_strength,
        lora_scope,
        framework_id: adapter.framework_id,
        framework_version: adapter.framework_version,
        repo_id: adapter.repo_id,
        commit_sha: adapter.commit_sha,
        intent: adapter.intent,
        created_at: adapter.created_at,
        updated_at: Some(adapter.updated_at),
        stats: Some(AdapterStats {
            total_activations: total,
            selected_count: selected,
            avg_gate_value: avg_gate,
            selection_rate,
        }),
        runtime_state: Some(adapter.current_state),
        pinned: None,
        memory_bytes: None,
        deduplicated: None,
        drift_reference_backend: None,
        drift_baseline_backend: None,
        drift_test_backend: None,
        drift_tier: None,
        drift_metric: None,
        drift_loss_metric: None,
        drift_slice_size: None,
        drift_slice_offset: None,
    }))
}

#[utoipa::path(
    post,
    path = "/v1/adapters/{adapter_id}/unload",
    tag = "adapters",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Adapter unloaded"),
        (status = 404, description = "Adapter not found", body = ErrorResponse),
        (status = 500, description = "Failed to unload adapter", body = ErrorResponse)
    )
)]
pub async fn unload_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Role check: Operator, SRE, and Admin can unload adapters
    crate::permissions::require_permission(&claims, crate::permissions::Permission::AdapterUnload)?;

    // Hot-swap guard: prevent unloading while other requests are active
    guard_in_flight_requests(&state.in_flight_requests)?;

    // Get adapter from database with tenant-scoped query
    let adapter = match state
        .db
        .get_adapter_for_tenant(&claims.tenant_id, &adapter_id)
        .await
    {
        Ok(Some(adapter)) => adapter,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("adapter not found").with_code("NOT_FOUND")),
            ));
        }
        Err(e) => {
            crate::audit_helper::log_failure_or_warn(
                &state.db,
                &claims,
                crate::audit_helper::actions::ADAPTER_UNLOAD,
                crate::audit_helper::resources::ADAPTER,
                Some(&adapter_id),
                &format!("Error: {}", e),
            )
            .await;
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to get adapter")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ));
        }
    };

    // Use lifecycle manager if available to update state to 'unloading'
    if let Some(ref lifecycle) = state.lifecycle_manager {
        let manager = lifecycle.lock().await;
        if let Some(adapter_idx) = manager.get_adapter_idx(&adapter_id) {
            use adapteros_lora_lifecycle::AdapterState;
            // Update state to unloading via lifecycle manager
            if let Err(e) = manager
                .update_adapter_state(adapter_idx, AdapterState::Unloaded, "user_request")
                .await
            {
                tracing::warn!(adapter_id = %adapter_id, error = %e, "Failed to update adapter state via lifecycle manager, continuing");
            }
        }
    } else {
        // Fallback: direct DB update if no lifecycle manager
        // Use transactional version for safety in handlers
        if let Err(e) = state
            .lifecycle_db()
            .update_adapter_state_tx_for_tenant(
                &adapter.tenant_id,
                &adapter_id,
                "unloading",
                "user_request",
            )
            .await
        {
            crate::audit_helper::log_failure_or_warn(
                &state.db,
                &claims,
                crate::audit_helper::actions::ADAPTER_UNLOAD,
                crate::audit_helper::resources::ADAPTER,
                Some(&adapter_id),
                &format!("Error: {}", e),
            )
            .await;
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to update adapter state")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ));
        }
    }

    tracing::info!("Unloading adapter {}", adapter_id);

    // Actually unload the adapter using LifecycleManager if available
    if let Some(ref lifecycle) = state.lifecycle_manager {
        let manager = lifecycle.lock().await;

        // Get adapter index
        if let Some(adapter_idx) = manager.get_adapter_idx(&adapter_id) {
            // Use evict_adapter which handles both unloading and DB update
            if let Err(e) = manager.evict_adapter(adapter_idx).await {
                tracing::warn!(adapter_id = %adapter_id, error = %e, "Failed to evict adapter via lifecycle manager");
                // Fallback: update DB state directly
                // Note: This is a best-effort fallback, adapter may still be loaded in memory
                let _ = state
                    .lifecycle_db()
                    .update_adapter_state_tx_for_tenant(
                        &adapter.tenant_id,
                        &adapter_id,
                        "unloaded",
                        "eviction_fallback",
                    )
                    .await;
            }

            tracing::info!(
                event = "adapter.unload",
                adapter_id = %adapter_id,
                "Adapter unloaded successfully via lifecycle manager"
            );
        } else {
            // Adapter not found in lifecycle manager, update DB state directly
            if let Err(e) = state
                .lifecycle_db()
                .update_adapter_state_tx_for_tenant(
                    &adapter.tenant_id,
                    &adapter_id,
                    "unloaded",
                    "not_found_in_lifecycle_manager",
                )
                .await
            {
                crate::audit_helper::log_failure_or_warn(
                    &state.db,
                    &claims,
                    crate::audit_helper::actions::ADAPTER_UNLOAD,
                    crate::audit_helper::resources::ADAPTER,
                    Some(&adapter_id),
                    &format!("Error: {}", e),
                )
                .await;
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to update adapter state")
                            .with_code("INTERNAL_SERVER_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                ));
            }

            tracing::info!(
                event = "adapter.unload",
                adapter_id = %adapter_id,
                "Adapter state updated (not found in lifecycle manager)"
            );
        }
    } else {
        // Fallback: direct DB update if no lifecycle manager
        if let Err(e) = state
            .lifecycle_db()
            .update_adapter_state_tx_for_tenant(
                &adapter.tenant_id,
                &adapter_id,
                "unloaded",
                "unloaded_via_api",
            )
            .await
        {
            crate::audit_helper::log_failure_or_warn(
                &state.db,
                &claims,
                crate::audit_helper::actions::ADAPTER_UNLOAD,
                crate::audit_helper::resources::ADAPTER,
                Some(&adapter_id),
                &format!("Error: {}", e),
            )
            .await;
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to update adapter state")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ));
        }

        state
            .db
            .update_adapter_memory_for_tenant(&adapter.tenant_id, &adapter_id, 0)
            .await
            .ok();

        tracing::info!(
            event = "adapter.unload",
            adapter_id = %adapter_id,
            "Adapter unloaded successfully (no lifecycle manager)"
        );
    }

    // Audit log: adapter unload
    crate::audit_helper::log_success_or_warn(
        &state.db,
        &claims,
        crate::audit_helper::actions::ADAPTER_UNLOAD,
        crate::audit_helper::resources::ADAPTER,
        Some(&adapter_id),
    )
    .await;

    Ok(StatusCode::OK)
}

#[utoipa::path(
    post,
    path = "/v1/adapters/{adapter_id}/state/promote",
    tag = "adapters",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Adapter state promoted", body = AdapterStateResponse),
        (status = 400, description = "Invalid state transition", body = ErrorResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse)
    )
)]
pub async fn promote_adapter_state(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<AdapterStateResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Get current adapter state with tenant-scoped query
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

    // Determine next tier based on current tier
    // Tiers: "persistent" → "warm" → "ephemeral"
    let new_tier = match adapter.tier.as_str() {
        "persistent" => "warm".to_string(),
        "warm" => "ephemeral".to_string(),
        "ephemeral" => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("adapter already at maximum tier (ephemeral)")
                        .with_code("ALREADY_AT_MAX_TIER"),
                ),
            ));
        }
        other => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new(format!("unknown tier: {}", other))
                        .with_code("UNKNOWN_TIER"),
                ),
            ));
        }
    };

    let old_tier = adapter.tier.clone();

    // Update adapter tier in database (tenant-scoped to prevent TOCTOU)
    state
        .db
        .update_adapter_tier_for_tenant(&claims.tenant_id, &adapter_id, &new_tier)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to update adapter tier")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(AdapterStateResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        adapter_id,
        old_state: old_tier,
        new_state: new_tier,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Download adapter manifest as JSON
pub async fn download_adapter_manifest(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<AdapterManifest>, (StatusCode, Json<ErrorResponse>)> {
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

    let manifest = AdapterManifest {
        adapter_id: adapter
            .adapter_id
            .clone()
            .unwrap_or_else(|| adapter.id.clone()),
        name: adapter.name,
        hash_b3: adapter.hash_b3,
        rank: adapter.rank,
        tier: adapter.tier.clone(),
        lora_strength: adapter.lora_strength,
        framework: adapter.framework,
        languages_json: adapter.languages_json,
        category: Some(adapter.category),
        scope: Some(adapter.scope),
        framework_id: adapter.framework_id,
        framework_version: adapter.framework_version,
        repo_id: adapter.repo_id,
        commit_sha: adapter.commit_sha,
        intent: adapter.intent,
        created_at: adapter.created_at,
        updated_at: adapter.updated_at,
    };

    Ok(Json(manifest))
}
