// Adapter Hot-Swap Handler
//
// This module provides REST API endpoints for:
// - Hot-swapping adapters (replacing one adapter with another)
//
// All swap operations are gated by preflight checks to ensure:
// - Adapter exists in registry with valid metadata
// - .aos file exists and has valid hashes + manifest hash
// - Lifecycle state allows activation
// - Training evidence snapshot exists
// - No conflicting active adapters for same repo/branch
// - System is not in maintenance mode

use crate::audit_helper::{log_preflight_result, log_success_or_warn, resources};
use crate::auth::Claims;
use crate::handlers::adapters::preflight_adapter::run_api_preflight;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::{AdapterSwapRequest, AdapterSwapResponse, ErrorResponse};
use adapteros_core::preflight::PreflightConfig;
use axum::{extract::State, http::StatusCode, response::Json, Extension};
use tracing::{error, info, warn};

// ============================================================================
// Handlers
// ============================================================================

/// Hot-swap adapters (replace one adapter with another)
///
/// Atomically swaps one adapter for another with minimal downtime.
/// Supports dry-run mode for validation without execution.
///
/// **Permissions:** Requires `AdapterLoad` and `AdapterUnload` permissions (Operator or Admin role).
///
/// **Telemetry:** Emits `adapter.swap` event with metadata:
/// - old_adapter_id, new_adapter_id, vram_delta_mb, duration_ms
///
/// # Example
/// ```
/// POST /v1/adapters/swap
/// {
///   "old_adapter_id": "adapter-old",
///   "new_adapter_id": "adapter-new",
///   "dry_run": false
/// }
/// ```
#[utoipa::path(
    post,
    path = "/v1/adapters/swap",
    request_body = AdapterSwapRequest,
    responses(
        (status = 200, description = "Adapter swap successful", body = AdapterSwapResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse),
        (status = 400, description = "Invalid swap request", body = ErrorResponse),
        (status = 500, description = "Swap failed", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn swap_adapters(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<AdapterSwapRequest>,
) -> Result<Json<AdapterSwapResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require both load and unload permissions
    require_permission(&claims, Permission::AdapterLoad)?;
    require_permission(&claims, Permission::AdapterUnload)?;

    let start_time = std::time::Instant::now();

    // Verify old adapter exists
    let old_adapter = state
        .db
        .get_adapter_for_tenant(&claims.tenant_id, &req.old_adapter_id)
        .await
        .map_err(|e| {
            error!(
                tenant_id = %claims.tenant_id,
                old_adapter_id = %req.old_adapter_id,
                error = %e,
                "Failed to fetch old adapter"
            );
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
            warn!(old_adapter_id = %req.old_adapter_id, "Old adapter not found");
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("old adapter not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(format!("Adapter ID: {}", req.old_adapter_id)),
                ),
            )
        })?;

    // Validate tenant isolation for old adapter
    validate_tenant_isolation(&claims, &old_adapter.tenant_id)?;

    // Verify new adapter exists
    let new_adapter = state
        .db
        .get_adapter_for_tenant(&claims.tenant_id, &req.new_adapter_id)
        .await
        .map_err(|e| {
            error!(
                tenant_id = %claims.tenant_id,
                new_adapter_id = %req.new_adapter_id,
                error = %e,
                "Failed to fetch new adapter"
            );
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
            warn!(new_adapter_id = %req.new_adapter_id, "New adapter not found");
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("new adapter not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(format!("Adapter ID: {}", req.new_adapter_id)),
                ),
            )
        })?;

    // Validate tenant isolation for new adapter
    validate_tenant_isolation(&claims, &new_adapter.tenant_id)?;

    // =========================================================================
    // PREFLIGHT GATING: Validate new adapter is ready for swap
    // =========================================================================
    // Run shared preflight checks on the new adapter before allowing the swap
    let preflight_config = PreflightConfig::with_actor(&claims.tenant_id, &claims.sub);
    let preflight_result = run_api_preflight(&new_adapter, &state.db, &preflight_config).await;

    // Audit log the preflight result including any bypasses used (Gap 4 fix)
    log_preflight_result(&state.db, &claims, &req.new_adapter_id, &preflight_result).await;

    if !preflight_result.passed {
        let primary_code = preflight_result
            .primary_error_code()
            .map(|c| c.as_str())
            .unwrap_or("PREFLIGHT_FAILED");
        let error_details = preflight_result.failure_summary();

        warn!(
            new_adapter_id = %req.new_adapter_id,
            error_code = %primary_code,
            checks_failed = preflight_result.failures.len(),
            "Adapter swap blocked by preflight checks"
        );
        return Err((
            StatusCode::PRECONDITION_FAILED,
            Json(
                ErrorResponse::new("Adapter swap blocked by preflight checks")
                    .with_code(primary_code)
                    .with_string_details(error_details),
            ),
        ));
    }

    // Calculate VRAM delta
    let vram_delta_mb = (new_adapter.memory_bytes - old_adapter.memory_bytes) / (1024 * 1024);

    // If dry run, just validate and return
    if req.dry_run {
        let duration_ms = start_time.elapsed().as_millis() as u64;

        info!(
            event = "adapter.swap.dry_run",
            old_adapter_id = %req.old_adapter_id,
            new_adapter_id = %req.new_adapter_id,
            vram_delta_mb = %vram_delta_mb,
            "Dry run swap validation successful"
        );

        return Ok(Json(AdapterSwapResponse {
            success: true,
            message: "Dry run: swap validated successfully".to_string(),
            old_adapter_id: req.old_adapter_id,
            new_adapter_id: req.new_adapter_id,
            vram_delta_mb: Some(vram_delta_mb),
            duration_ms,
            dry_run: true,
        }));
    }

    // Execute the swap: unload old, load new using lifecycle manager
    // Unload old adapter via lifecycle manager
    if let Some(ref lifecycle) = state.lifecycle_manager {
        let mut manager = lifecycle.lock().await;

        // Unload old adapter
        if let Some(old_adapter_idx) = manager.get_adapter_idx(&req.old_adapter_id) {
            // Use evict_adapter which handles both unloading and DB update
            if let Err(e) = manager.evict_adapter(old_adapter_idx).await {
                tracing::warn!(adapter_id = %req.old_adapter_id, error = %e, "Failed to evict old adapter via lifecycle manager");
                // Fallback: update DB state directly
                state
                    .lifecycle_db()
                    .update_adapter_state_tx_for_tenant(
                        &claims.tenant_id,
                        &req.old_adapter_id,
                        "unloaded",
                        "swap_eviction_fallback",
                    )
                    .await
                    .map_err(|e| {
                        error!(
                            tenant_id = %claims.tenant_id,
                            adapter_id = %req.old_adapter_id,
                            error = %e,
                            "Failed to update old adapter state"
                        );
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(
                                ErrorResponse::new("failed to update adapter state")
                                    .with_code("INTERNAL_ERROR")
                                    .with_string_details(e.to_string()),
                            ),
                        )
                    })?;
            }
        } else {
            // Adapter not found in lifecycle manager, update DB directly
            state
                .lifecycle_db()
                .update_adapter_state_tx_for_tenant(
                    &claims.tenant_id,
                    &req.old_adapter_id,
                    "unloaded",
                    "swap_not_found_in_lifecycle",
                )
                .await
                .map_err(|e| {
                    error!(
                        tenant_id = %claims.tenant_id,
                        adapter_id = %req.old_adapter_id,
                        error = %e,
                        "Failed to update old adapter state"
                    );
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            ErrorResponse::new("failed to update adapter state")
                                .with_code("INTERNAL_ERROR")
                                .with_string_details(e.to_string()),
                        ),
                    )
                })?;
        }

        // Load new adapter via lifecycle manager
        if let Err(e) = manager.get_or_reload(&req.new_adapter_id) {
            tracing::warn!(adapter_id = %req.new_adapter_id, error = %e, "Failed to load new adapter via lifecycle manager");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to load new adapter")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ));
        }

        // Update new adapter state via lifecycle manager
        if let Some(new_adapter_idx) = manager.get_adapter_idx(&req.new_adapter_id) {
            use adapteros_lora_lifecycle::AdapterState;
            if let Err(e) = manager
                .update_adapter_state(new_adapter_idx, AdapterState::Warm, "swapped_in")
                .await
            {
                tracing::warn!(adapter_id = %req.new_adapter_id, error = %e, "Failed to update new adapter state via lifecycle manager");
                // Fallback: update DB state directly
                state
                    .lifecycle_db()
                    .update_adapter_state_tx_for_tenant(
                        &claims.tenant_id,
                        &req.new_adapter_id,
                        "warm",
                        "swap_load_fallback",
                    )
                    .await
                    .map_err(|e| {
                        error!(
                            tenant_id = %claims.tenant_id,
                            adapter_id = %req.new_adapter_id,
                            error = %e,
                            "Failed to update new adapter state"
                        );
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(
                                ErrorResponse::new("failed to update adapter state")
                                    .with_code("INTERNAL_ERROR")
                                    .with_string_details(e.to_string()),
                            ),
                        )
                    })?;
            }
        } else {
            return Err((
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("new adapter not found in lifecycle manager")
                        .with_code("NOT_FOUND"),
                ),
            ));
        }
    } else {
        // Fallback: direct DB updates if no lifecycle manager
        state
            .lifecycle_db()
            .update_adapter_state_tx_for_tenant(
                &claims.tenant_id,
                &req.old_adapter_id,
                "unloaded",
                "swap_no_lifecycle_manager",
            )
            .await
            .map_err(|e| {
                error!(
                    tenant_id = %claims.tenant_id,
                    adapter_id = %req.old_adapter_id,
                    error = %e,
                    "Failed to update old adapter state"
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to update adapter state")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;

        state
            .lifecycle_db()
            .update_adapter_state_tx_for_tenant(
                &claims.tenant_id,
                &req.new_adapter_id,
                "warm",
                "swap_no_lifecycle_manager",
            )
            .await
            .map_err(|e| {
                error!(
                    tenant_id = %claims.tenant_id,
                    adapter_id = %req.new_adapter_id,
                    error = %e,
                    "Failed to update new adapter state"
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to update adapter state")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;
    }

    let duration_ms = start_time.elapsed().as_millis() as u64;

    // Emit telemetry event
    let telemetry_event = serde_json::json!({
        "event_type": "adapter.swap",
        "component": "adapteros-server-api",
        "severity": "info",
        "message": format!("Adapter swap: {} -> {}", req.old_adapter_id, req.new_adapter_id),
        "metadata": {
            "old_adapter_id": req.old_adapter_id,
            "new_adapter_id": req.new_adapter_id,
            "vram_delta_mb": vram_delta_mb,
            "duration_ms": duration_ms,
            "actor": claims.sub.clone(),
        }
    });

    info!(
        event = %telemetry_event,
        old_adapter_id = %req.old_adapter_id,
        new_adapter_id = %req.new_adapter_id,
        duration_ms = %duration_ms,
        "Adapter swap completed"
    );

    // Audit log
    log_success_or_warn(
        &state.db,
        &claims,
        "adapter.swap",
        resources::ADAPTER,
        Some(&format!("{} -> {}", req.old_adapter_id, req.new_adapter_id)),
    )
    .await;

    Ok(Json(AdapterSwapResponse {
        success: true,
        message: "Adapter swap completed successfully".to_string(),
        old_adapter_id: req.old_adapter_id,
        new_adapter_id: req.new_adapter_id,
        vram_delta_mb: Some(vram_delta_mb),
        duration_ms,
        dry_run: false,
    }))
}
