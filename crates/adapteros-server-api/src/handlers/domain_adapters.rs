use crate::adapter_helpers::fetch_adapter_for_tenant;
use crate::api_error::{ApiError, ApiResult};
use crate::auth::Claims;
use crate::inference_core::InferenceCore;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::{
    CreateDomainAdapterRequest, DomainAdapterExecutionResponse, DomainAdapterManifestResponse,
    DomainAdapterResponse, ErrorResponse, InferenceRequestInternal, LoadDomainAdapterRequest,
    TestDomainAdapterRequest, TestDomainAdapterResponse,
};
use adapteros_db::adapters::{Adapter, AdapterRegistrationBuilder};
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::Json,
};
use chrono::Utc;
use std::collections::HashMap;
use std::time::Instant;
use tracing::{error, info, warn};
use uuid::Uuid;

/// Convert database Adapter to DomainAdapterResponse
fn adapter_to_domain_response(adapter: Adapter) -> DomainAdapterResponse {
    // Parse config from adapter's acl_json or targets_json as config placeholder
    let config: HashMap<String, serde_json::Value> = adapter
        .acl_json
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    DomainAdapterResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        id: adapter.id.clone(),
        name: adapter.name.clone(),
        version: adapter.version.clone(),
        description: adapter.intent.clone().unwrap_or_default(),
        domain_type: adapter
            .domain
            .clone()
            .unwrap_or_else(|| "general".to_string()),
        model: adapter
            .framework
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
        hash: adapter.hash_b3.clone(),
        input_format: "text".to_string(),
        output_format: "text".to_string(),
        config,
        status: adapter.current_state.clone(),
        epsilon_stats: None,
        last_execution: adapter.last_activated.clone(),
        execution_count: adapter.activation_count as u64,
        created_at: adapter.created_at.clone(),
        updated_at: adapter.updated_at.clone(),
    }
}

/// List all domain adapters
#[utoipa::path(
    get,
    path = "/v1/domain-adapters",
    responses(
        (status = 200, description = "List of domain adapters", body = Vec<DomainAdapterResponse>),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
pub async fn list_domain_adapters(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<DomainAdapterResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterView)?;

    // Filter by tenant to ensure tenant isolation - list_adapters_by_category doesn't filter by tenant,
    // so we use list_adapters_for_tenant and filter by category
    let all_adapters = state
        .db
        .list_adapters_for_tenant(&claims.tenant_id)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to list domain adapters");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to list domain adapters")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Filter to only domain adapters
    let responses: Vec<DomainAdapterResponse> = all_adapters
        .into_iter()
        .filter(|a| a.category == "domain")
        .map(adapter_to_domain_response)
        .collect();

    info!(count = responses.len(), tenant_id = %claims.tenant_id, "Listed domain adapters");
    Ok(Json(responses))
}

/// Get a specific domain adapter
#[utoipa::path(
    get,
    path = "/v1/domain-adapters/{adapter_id}",
    params(
        ("adapter_id" = String, Path, description = "Domain adapter ID")
    ),
    responses(
        (status = 200, description = "Domain adapter details", body = DomainAdapterResponse),
        (status = 404, description = "Domain adapter not found", body = ErrorResponse)
    )
)]
pub async fn get_domain_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> ApiResult<DomainAdapterResponse> {
    require_permission(&claims, Permission::AdapterView)?;

    let adapter = fetch_adapter_for_tenant(&state.db, &claims, &adapter_id).await?;

    // Verify it's a domain adapter
    if adapter.category != "domain" {
        warn!(adapter_id = %adapter_id, "Adapter is not a domain adapter");
        return Err(ApiError::not_found("Domain adapter").with_details(format!(
            "Adapter {} has category '{}'",
            adapter_id, adapter.category
        )));
    }

    Ok(Json(adapter_to_domain_response(adapter)))
}

/// Create a new domain adapter
#[utoipa::path(
    post,
    path = "/v1/domain-adapters",
    request_body = CreateDomainAdapterRequest,
    responses(
        (status = 201, description = "Domain adapter created", body = DomainAdapterResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse)
    )
)]
pub async fn create_domain_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateDomainAdapterRequest>,
) -> Result<Json<DomainAdapterResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterRegister)?;

    // Validate inputs
    if req.name.is_empty() || req.domain_type.is_empty() || req.model.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("name, domain_type, and model are required")
                    .with_code("VALIDATION_ERROR"),
            ),
        ));
    }

    // Generate adapter ID
    let adapter_id = Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string();

    // Build registration parameters
    let params = AdapterRegistrationBuilder::new()
        .adapter_id(&adapter_id)
        .name(&req.name)
        .hash_b3(&req.hash)
        .rank(8) // Default rank for domain adapters
        .tier("warm")
        .category("domain")
        .scope("tenant")
        .domain(Some(&req.domain_type))
        .framework(Some(&req.model))
        .intent(Some(&req.description))
        .acl_json(Some(serde_json::to_string(&req.config).unwrap_or_default()))
        .build()
        .map_err(|e| {
            error!(error = %e, "Failed to build adapter registration params");
            (
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("Invalid adapter configuration")
                        .with_code("VALIDATION_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Register adapter in database
    let id = state.db.register_adapter(params).await.map_err(|e| {
        error!(error = %e, "Failed to register domain adapter");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to create domain adapter")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    info!(adapter_id = %id, name = %req.name, "Created domain adapter");

    // Emit plugin event for adapter registration (if event bus configured)
    if let Some(ref event_bus) = state.event_bus {
        use adapteros_core::plugin_events::{AdapterEvent, PluginEvent};
        use chrono::Utc;

        let adapter_event = AdapterEvent {
            adapter_id: id.clone(),
            action: "registered".to_string(),
            hash: Some(req.hash.clone()),
            tier: Some("warm".to_string()),
            rank: Some(8),
            tenant_id: Some(claims.tenant_id.clone()),
            lifecycle_state: Some("unloaded".to_string()),
            timestamp: Utc::now().to_rfc3339(),
            metadata: std::collections::HashMap::new(),
        };

        let event = PluginEvent::AdapterRegistered(adapter_event);
        let event_bus_clone = event_bus.clone();
        tokio::spawn(async move {
            if let Err(failures) = event_bus_clone.emit(event).await {
                warn!(
                    failed_plugins = ?failures,
                    "Some plugins failed to handle AdapterRegistered event"
                );
            }
        });
    }

    // Audit log: domain adapter created
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::DOMAIN_ADAPTER_CREATE,
        crate::audit_helper::resources::DOMAIN_ADAPTER,
        Some(&id),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    let response = DomainAdapterResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        id,
        name: req.name,
        version: req.version,
        description: req.description,
        domain_type: req.domain_type,
        model: req.model,
        hash: req.hash,
        input_format: req.input_format,
        output_format: req.output_format,
        config: req.config,
        status: "unloaded".to_string(),
        epsilon_stats: None,
        last_execution: None,
        execution_count: 0,
        created_at: Utc::now().to_rfc3339(),
        updated_at: Utc::now().to_rfc3339(),
    };

    // Return 200 OK (consistent with create_tenant pattern)
    // Status code can be set via response builder if 201 is required
    Ok(Json(response))
}

/// Load a domain adapter into the deterministic executor
#[utoipa::path(
    post,
    path = "/v1/domain-adapters/{adapter_id}/load",
    params(
        ("adapter_id" = String, Path, description = "Domain adapter ID")
    ),
    request_body = LoadDomainAdapterRequest,
    responses(
        (status = 200, description = "Domain adapter loaded", body = DomainAdapterResponse),
        (status = 404, description = "Domain adapter not found", body = ErrorResponse)
    )
)]
pub async fn load_domain_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
    Json(_req): Json<LoadDomainAdapterRequest>,
) -> ApiResult<DomainAdapterResponse> {
    require_permission(&claims, Permission::AdapterLoad)?;

    // Get adapter from database with tenant isolation validation
    let adapter = fetch_adapter_for_tenant(&state.db, &claims, &adapter_id).await?;

    // Verify it's a domain adapter
    if adapter.category != "domain" {
        return Err(
            ApiError::bad_request("Adapter is not a domain adapter").with_details(format!(
                "Adapter {} has category '{}'",
                adapter_id, adapter.category
            )),
        );
    }

    // Use lifecycle manager if available
    if let Some(ref lifecycle) = state.lifecycle_manager {
        let mut manager = lifecycle.lock().await;

        // Load adapter (updates internal state only)
        manager.get_or_reload(&adapter_id).map_err(|e| {
            error!(error = %e, adapter_id = %adapter_id, "Failed to load adapter via lifecycle manager");
            ApiError::internal("Failed to load adapter").with_details(e.to_string())
        })?;

        // Update state (handles DB update if db is set)
        if let Some(adapter_idx) = manager.get_adapter_idx(&adapter_id) {
            use adapteros_lora_lifecycle::AdapterHeatState;
            if let Err(e) = manager
                .update_adapter_state(adapter_idx, AdapterHeatState::Cold, "loaded_via_api")
                .await
            {
                error!(error = %e, adapter_id = %adapter_id, "Failed to update adapter state via lifecycle manager");
                // Fallback: update DB state directly
                state
                    .lifecycle_db()
                    .update_adapter_state_tx_for_tenant(&adapter.tenant_id, &adapter_id, "cold", "loaded_via_api")
                    .await
                    .map_err(|e| {
                        error!(error = %e, adapter_id = %adapter_id, "Failed to update adapter state");
                        ApiError::internal("Failed to update adapter state").with_details(e.to_string())
                    })?;
            }
        } else {
            // Adapter not found in lifecycle manager, update DB state directly
            state
                .lifecycle_db()
                .update_adapter_state_tx_for_tenant(
                    &adapter.tenant_id,
                    &adapter_id,
                    "cold",
                    "loaded_via_api",
                )
                .await
                .map_err(|e| {
                    error!(error = %e, adapter_id = %adapter_id, "Failed to update adapter state");
                    ApiError::internal("Failed to update adapter state").with_details(e.to_string())
                })?;
        }
    } else {
        // Fallback: direct DB update if no lifecycle manager
        state
            .lifecycle_db()
            .update_adapter_state_tx_for_tenant(
                &adapter.tenant_id,
                &adapter_id,
                "cold",
                "loaded_via_api",
            )
            .await
            .map_err(|e| {
                error!(error = %e, adapter_id = %adapter_id, "Failed to update adapter state");
                ApiError::internal("Failed to update adapter state").with_details(e.to_string())
            })?;
    }

    // Fetch updated adapter
    let updated_adapter = fetch_adapter_for_tenant(&state.db, &claims, &adapter_id).await?;

    info!(adapter_id = %adapter_id, "Loaded domain adapter");

    // Audit log: domain adapter loaded
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::DOMAIN_ADAPTER_LOAD,
        crate::audit_helper::resources::DOMAIN_ADAPTER,
        Some(&adapter_id),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    Ok(Json(adapter_to_domain_response(updated_adapter)))
}

/// Unload a domain adapter from the deterministic executor
#[utoipa::path(
    post,
    path = "/v1/domain-adapters/{adapter_id}/unload",
    params(
        ("adapter_id" = String, Path, description = "Domain adapter ID")
    ),
    responses(
        (status = 200, description = "Domain adapter unloaded", body = DomainAdapterResponse),
        (status = 404, description = "Domain adapter not found", body = ErrorResponse)
    )
)]
pub async fn unload_domain_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> ApiResult<DomainAdapterResponse> {
    require_permission(&claims, Permission::AdapterLoad)?;

    // Get adapter from database with tenant isolation validation
    let adapter = fetch_adapter_for_tenant(&state.db, &claims, &adapter_id).await?;

    // Verify it's a domain adapter
    if adapter.category != "domain" {
        return Err(
            ApiError::bad_request("Adapter is not a domain adapter").with_details(format!(
                "Adapter {} has category '{}'",
                adapter_id, adapter.category
            )),
        );
    }

    // Update adapter state to unloaded in database
    state
        .lifecycle_db()
        .update_adapter_state_tx_for_tenant(
            &adapter.tenant_id,
            &adapter_id,
            "unloaded",
            "Unloaded via API",
        )
        .await
        .map_err(|e| {
            error!(error = %e, adapter_id = %adapter_id, "Failed to update adapter state");
            ApiError::internal("Failed to update adapter state").with_details(e.to_string())
        })?;

    // Fetch updated adapter
    let updated_adapter = fetch_adapter_for_tenant(&state.db, &claims, &adapter_id).await?;

    info!(adapter_id = %adapter_id, "Unloaded domain adapter");

    // Audit log: domain adapter unloaded
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::DOMAIN_ADAPTER_UNLOAD,
        crate::audit_helper::resources::DOMAIN_ADAPTER,
        Some(&adapter_id),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    Ok(Json(adapter_to_domain_response(updated_adapter)))
}

/// Test a domain adapter for determinism
#[utoipa::path(
    post,
    path = "/v1/domain-adapters/{adapter_id}/test",
    params(
        ("adapter_id" = String, Path, description = "Domain adapter ID")
    ),
    request_body = TestDomainAdapterRequest,
    responses(
        (status = 200, description = "Test completed", body = TestDomainAdapterResponse),
        (status = 404, description = "Domain adapter not found", body = ErrorResponse)
    )
)]
pub async fn test_domain_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
    Json(req): Json<TestDomainAdapterRequest>,
) -> ApiResult<TestDomainAdapterResponse> {
    require_permission(&claims, Permission::AdapterView)?;

    let start_time = Instant::now();
    let iterations = req.iterations.unwrap_or(1);

    // Get adapter from database with tenant isolation validation
    let adapter = fetch_adapter_for_tenant(&state.db, &claims, &adapter_id).await?;

    // Verify it's a domain adapter
    if adapter.category != "domain" {
        return Err(
            ApiError::bad_request("Adapter is not a domain adapter").with_details(format!(
                "Adapter {} has category '{}'",
                adapter_id, adapter.category
            )),
        );
    }

    // Run inference test using worker if available
    let mut actual_output = String::new();
    let mut epsilon = None;
    let mut passed = true;

    let core = InferenceCore::new(&state);
    let tenant_id = claims.tenant_id.clone();
    let prompt = req.input_data.clone();

    // Run inference iterations for determinism testing
    let mut outputs = Vec::with_capacity(iterations as usize);

    for i in 0..iterations {
        let mut inference_req = InferenceRequestInternal::new(tenant_id.clone(), prompt.clone());
        inference_req.adapters = Some(vec![adapter_id.clone()]);
        inference_req.max_tokens = 256;
        inference_req.stack_determinism_mode = Some("strict".to_string());

        match core.route_and_infer(inference_req, None, None, None).await {
            Ok(response) => {
                let output_text = response.text;
                outputs.push(output_text.clone());
                if i == 0 {
                    actual_output = output_text;
                }
            }
            Err(e) => {
                error!(error = %e, adapter_id = %adapter_id, iteration = i, "Inference failed during test");
                passed = false;
                break;
            }
        }
    }

    // Check determinism - all outputs should be identical
    if passed && outputs.len() > 1 {
        let first = &outputs[0];
        for (i, output) in outputs.iter().enumerate().skip(1) {
            if output != first {
                passed = false;
                epsilon = Some(1.0); // Non-deterministic
                warn!(
                    adapter_id = %adapter_id,
                    iteration = i,
                    "Non-deterministic output detected"
                );
                break;
            }
        }
        if passed {
            epsilon = Some(0.0); // Fully deterministic
        }
    }

    // Check against expected output if provided
    if let Some(ref expected) = req.expected_output {
        if actual_output != *expected {
            passed = false;
        }
    }

    let execution_time_ms = start_time.elapsed().as_millis() as u64;

    // Update activation count
    let _ = state
        .db
        .increment_adapter_activation(&claims.tenant_id, &adapter_id)
        .await
        .map_err(|e| {
            warn!(error = %e, adapter_id = %adapter_id, "Failed to increment activation count");
        });

    info!(
        adapter_id = %adapter_id,
        passed = passed,
        iterations = iterations,
        execution_time_ms = execution_time_ms,
        "Completed domain adapter test"
    );

    let test_result = TestDomainAdapterResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        test_id: Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string(),
        adapter_id: adapter_id.clone(),
        input_data: req.input_data,
        actual_output,
        expected_output: req.expected_output,
        epsilon,
        passed,
        iterations,
        execution_time_ms,
        executed_at: Utc::now().to_rfc3339(),
    };

    Ok(Json(test_result))
}

/// Get domain adapter manifest
#[utoipa::path(
    get,
    path = "/v1/domain-adapters/{adapter_id}/manifest",
    params(
        ("adapter_id" = String, Path, description = "Domain adapter ID")
    ),
    responses(
        (status = 200, description = "Domain adapter manifest", body = DomainAdapterManifestResponse),
        (status = 404, description = "Domain adapter not found", body = ErrorResponse)
    )
)]
pub async fn get_domain_adapter_manifest(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> ApiResult<DomainAdapterManifestResponse> {
    require_permission(&claims, Permission::AdapterView)?;

    // Get adapter from database with tenant isolation validation
    let adapter = fetch_adapter_for_tenant(&state.db, &claims, &adapter_id).await?;

    // Verify it's a domain adapter
    if adapter.category != "domain" {
        return Err(ApiError::not_found("Domain adapter").with_details(format!(
            "Adapter {} has category '{}'",
            adapter_id, adapter.category
        )));
    }

    // Parse config from adapter's acl_json
    let config: HashMap<String, serde_json::Value> = adapter
        .acl_json
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    let manifest = DomainAdapterManifestResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        adapter_id: adapter.id.clone(),
        name: adapter.name.clone(),
        version: adapter.version.clone(),
        description: adapter.intent.clone().unwrap_or_default(),
        domain_type: adapter
            .domain
            .clone()
            .unwrap_or_else(|| "general".to_string()),
        model: adapter
            .framework
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
        hash: adapter.hash_b3.clone(),
        input_format: "text".to_string(),
        output_format: "text".to_string(),
        config,
        created_at: adapter.created_at.clone(),
        updated_at: adapter.updated_at.clone(),
    };

    Ok(Json(manifest))
}

/// Execute domain adapter with input data
#[utoipa::path(
    post,
    path = "/v1/domain-adapters/{adapter_id}/execute",
    params(
        ("adapter_id" = String, Path, description = "Domain adapter ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Execution completed", body = DomainAdapterExecutionResponse),
        (status = 404, description = "Domain adapter not found", body = ErrorResponse)
    )
)]
pub async fn execute_domain_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
    Json(input_data): Json<serde_json::Value>,
) -> ApiResult<DomainAdapterExecutionResponse> {
    require_permission(&claims, Permission::InferenceExecute)?;

    let start_time = Instant::now();

    // Get adapter from database with tenant isolation validation
    let adapter = fetch_adapter_for_tenant(&state.db, &claims, &adapter_id).await?;

    // Verify it's a domain adapter
    if adapter.category != "domain" {
        return Err(
            ApiError::bad_request("Adapter is not a domain adapter").with_details(format!(
                "Adapter {} has category '{}'",
                adapter_id, adapter.category
            )),
        );
    }

    // Compute input hash
    let input_str = serde_json::to_string(&input_data).unwrap_or_default();
    let input_hash = blake3::hash(input_str.as_bytes()).to_hex().to_string();

    // Run inference using worker if available
    let mut output_hash = String::new();
    let mut trace_events = vec![
        "adapter_prepare".to_string(),
        "adapter_forward".to_string(),
        "adapter_postprocess".to_string(),
    ];

    let core = InferenceCore::new(&state);
    let mut inference_req =
        InferenceRequestInternal::new(claims.tenant_id.clone(), input_str.clone());
    inference_req.adapters = Some(vec![adapter_id.clone()]);
    inference_req.max_tokens = 512;
    inference_req.stack_determinism_mode = Some("strict".to_string());

    match core.route_and_infer(inference_req, None, None, None).await {
        Ok(response) => {
            output_hash = blake3::hash(response.text.as_bytes()).to_hex().to_string();
            trace_events.push("inference_complete".to_string());
        }
        Err(e) => {
            error!(error = %e, adapter_id = %adapter_id, "Inference failed during execution");
            trace_events.push(format!("inference_error: {}", e));
        }
    }

    let execution_time_ms = start_time.elapsed().as_millis() as u64;

    // Update activation count
    let _ = state
        .db
        .increment_adapter_activation(&claims.tenant_id, &adapter_id)
        .await
        .map_err(|e| {
            warn!(error = %e, adapter_id = %adapter_id, "Failed to increment activation count");
        });

    info!(
        adapter_id = %adapter_id,
        execution_time_ms = execution_time_ms,
        "Executed domain adapter"
    );

    // Audit log: domain adapter executed
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::DOMAIN_ADAPTER_EXECUTE,
        crate::audit_helper::resources::DOMAIN_ADAPTER,
        Some(&adapter_id),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    let execution = DomainAdapterExecutionResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        execution_id: Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string(),
        adapter_id: adapter_id.clone(),
        input_hash,
        output_hash,
        epsilon: 0.0, // Will be calculated by determinism checks
        execution_time_ms,
        trace_events,
        executed_at: Utc::now().to_rfc3339(),
    };

    Ok(Json(execution))
}

/// Delete a domain adapter
#[utoipa::path(
    delete,
    path = "/v1/domain-adapters/{adapter_id}",
    params(
        ("adapter_id" = String, Path, description = "Domain adapter ID")
    ),
    responses(
        (status = 204, description = "Domain adapter deleted"),
        (status = 404, description = "Domain adapter not found", body = ErrorResponse)
    )
)]
pub async fn delete_domain_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    require_permission(&claims, Permission::AdapterRegister)?;

    // Get adapter to verify it exists and is a domain adapter (with tenant isolation)
    let adapter = fetch_adapter_for_tenant(&state.db, &claims, &adapter_id).await?;

    // Verify it's a domain adapter
    if adapter.category != "domain" {
        return Err(ApiError::not_found("Domain adapter").with_details(format!(
            "Adapter {} has category '{}'",
            adapter_id, adapter.category
        )));
    }

    // Check if adapter is pinned
    let is_pinned = state
        .db
        .is_pinned(&adapter.tenant_id, &adapter_id)
        .await
        .unwrap_or(false);

    if is_pinned {
        // Audit log: attempted deletion of pinned adapter (security event)
        if let Err(e) = crate::audit_helper::log_failure(
            &state.db,
            &claims,
            crate::audit_helper::actions::DOMAIN_ADAPTER_DELETE,
            crate::audit_helper::resources::DOMAIN_ADAPTER,
            Some(&adapter_id),
            "Adapter is pinned and cannot be deleted",
        )
        .await
        {
            tracing::warn!(error = %e, "Audit log failed");
        }

        return Err(
            ApiError::conflict("Cannot delete pinned adapter").with_details(format!(
                "Adapter {} is pinned and cannot be deleted",
                adapter_id
            )),
        );
    }

    state
        .db
        .delete_adapter_for_tenant(&claims.tenant_id, &adapter_id)
        .await
        .map_err(|e| {
            error!(error = %e, adapter_id = %adapter_id, "Failed to delete adapter");
            ApiError::internal("Failed to delete adapter").with_details(e.to_string())
        })?;

    info!(adapter_id = %adapter_id, "Deleted domain adapter");

    // Audit log: domain adapter deleted
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::DOMAIN_ADAPTER_DELETE,
        crate::audit_helper::resources::DOMAIN_ADAPTER,
        Some(&adapter_id),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    Ok(StatusCode::NO_CONTENT)
}
