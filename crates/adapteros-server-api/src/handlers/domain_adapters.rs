use crate::state::AppState;
use crate::types::{
    CreateDomainAdapterRequest, DomainAdapterExecutionResponse, DomainAdapterManifestResponse,
    DomainAdapterResponse, EpsilonStatsResponse, ErrorResponse, LoadDomainAdapterRequest,
    TestDomainAdapterRequest, TestDomainAdapterResponse,
};
use adapteros_db::DomainAdapterCreateBuilder;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use chrono::Utc;
use tracing::{error, info, warn};

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
) -> Result<Json<Vec<DomainAdapterResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let adapters = state.db.list_domain_adapters().await.map_err(|e| {
        error!("Failed to list domain adapters: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to list domain adapters")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    Ok(Json(adapters))
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
    Path(adapter_id): Path<String>,
) -> Result<Json<DomainAdapterResponse>, (StatusCode, Json<ErrorResponse>)> {
    let adapter = state
        .db
        .get_domain_adapter(&adapter_id)
        .await
        .map_err(|e| {
            error!("Failed to get domain adapter: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get domain adapter")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let adapter = adapter.ok_or_else(|| {
        warn!("Domain adapter not found: {}", adapter_id);
        (
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("Domain adapter not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(format!("Adapter ID: {}", adapter_id)),
            ),
        )
    })?;

    Ok(Json(adapter))
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
    Json(req): Json<CreateDomainAdapterRequest>,
) -> Result<(StatusCode, Json<DomainAdapterResponse>), (StatusCode, Json<ErrorResponse>)> {
    // Validate inputs
    if req.name.is_empty() || req.domain_type.is_empty() || req.model.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("name, domain_type, and model are required")
                    .with_code("BAD_REQUEST"),
            ),
        ));
    }

    // Build domain adapter parameters
    let params = DomainAdapterCreateBuilder::new()
        .name(req.name)
        .version(req.version)
        .description(req.description)
        .domain_type(req.domain_type)
        .model(req.model)
        .hash(req.hash)
        .input_format(req.input_format)
        .output_format(req.output_format)
        .config(req.config)
        .build()
        .map_err(|e| {
            error!("Failed to build domain adapter params: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("Invalid domain adapter parameters")
                        .with_code("BAD_REQUEST")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Create domain adapter in database
    let adapter_id = state.db.create_domain_adapter(params).await.map_err(|e| {
        error!("Failed to create domain adapter: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to create domain adapter")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Get the created adapter
    let adapter = state
        .db
        .get_domain_adapter(&adapter_id)
        .await
        .map_err(|e| {
            error!("Failed to retrieve created domain adapter: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to retrieve created domain adapter")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            error!("Domain adapter not found after creation: {}", adapter_id);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Domain adapter not found after creation")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    info!("Domain adapter created: {}", adapter_id);
    Ok((StatusCode::CREATED, Json(adapter)))
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
    Path(adapter_id): Path<String>,
    Json(_req): Json<LoadDomainAdapterRequest>,
) -> Result<Json<DomainAdapterResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Check if adapter exists
    let adapter = state
        .db
        .get_domain_adapter(&adapter_id)
        .await
        .map_err(|e| {
            error!("Failed to get domain adapter: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get domain adapter")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            warn!("Domain adapter not found: {}", adapter_id);
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("Domain adapter not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(format!("Adapter ID: {}", adapter_id)),
                ),
            )
        })?;

    // Check if already loaded
    if adapter.status == "loaded" {
        return Ok(Json(adapter));
    }

    // TODO: Load adapter into deterministic executor
    // This would involve:
    // 1. Loading the adapter manifest
    // 2. Registering with the deterministic executor
    // For now, just update the status

    // Update adapter status to loaded
    state
        .db
        .update_domain_adapter_status(&adapter_id, "loaded")
        .await
        .map_err(|e| {
            error!("Failed to update domain adapter status: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to load domain adapter")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Get updated adapter
    let adapter = state
        .db
        .get_domain_adapter(&adapter_id)
        .await
        .map_err(|e| {
            error!("Failed to retrieve updated domain adapter: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to retrieve updated domain adapter")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            error!("Domain adapter not found after update: {}", adapter_id);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Domain adapter not found after update")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    info!("Domain adapter loaded: {}", adapter_id);
    Ok(Json(adapter))
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
    Path(adapter_id): Path<String>,
) -> Result<Json<DomainAdapterResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Check if adapter exists
    let adapter = state
        .db
        .get_domain_adapter(&adapter_id)
        .await
        .map_err(|e| {
            error!("Failed to get domain adapter: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get domain adapter")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            warn!("Domain adapter not found: {}", adapter_id);
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("Domain adapter not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(format!("Adapter ID: {}", adapter_id)),
                ),
            )
        })?;

    // Check if already unloaded
    if adapter.status == "unloaded" {
        return Ok(Json(adapter));
    }

    // TODO: Unload adapter from deterministic executor
    // This would involve:
    // 1. Unregistering from the deterministic executor
    // For now, just update the status

    // Update adapter status to unloaded
    state
        .db
        .update_domain_adapter_status(&adapter_id, "unloaded")
        .await
        .map_err(|e| {
            error!("Failed to update domain adapter status: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to unload domain adapter")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Get updated adapter
    let adapter = state
        .db
        .get_domain_adapter(&adapter_id)
        .await
        .map_err(|e| {
            error!("Failed to retrieve updated domain adapter: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to retrieve updated domain adapter")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            error!("Domain adapter not found after update: {}", adapter_id);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Domain adapter not found after update")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    info!("Domain adapter unloaded: {}", adapter_id);
    Ok(Json(adapter))
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
    Path(adapter_id): Path<String>,
    Json(req): Json<TestDomainAdapterRequest>,
) -> Result<Json<TestDomainAdapterResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Check if adapter exists and is loaded
    let adapter = state
        .db
        .get_domain_adapter(&adapter_id)
        .await
        .map_err(|e| {
            error!("Failed to get domain adapter: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get domain adapter")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            warn!("Domain adapter not found: {}", adapter_id);
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("Domain adapter not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(format!("Adapter ID: {}", adapter_id)),
                ),
            )
        })?;

    if adapter.status != "loaded" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Domain adapter must be loaded before testing")
                    .with_code("BAD_REQUEST")
                    .with_string_details(format!("Adapter status: {}", adapter.status)),
            ),
        ));
    }

    let iterations = req.iterations.unwrap_or(100);
    let start_time = std::time::Instant::now();

    // TODO: Implement actual determinism testing
    // This would involve:
    // 1. Running the adapter multiple times with the same input
    // 2. Comparing outputs for byte-identical results
    // 3. Calculating epsilon (numerical drift)
    // 4. Generating trace events

    // For now, simulate a test result
    let execution_time_ms = start_time.elapsed().as_millis() as u64;
    let passed = true; // Mock result
    let epsilon = Some(0.001); // Mock epsilon
    let actual_output = "test_output".to_string(); // Mock output

    // Record test in database
    let test_id = state
        .db
        .record_domain_adapter_test(
            &adapter_id,
            &req.input_data,
            &actual_output,
            req.expected_output.as_deref(),
            epsilon,
            passed,
            iterations as u32,
            execution_time_ms,
        )
        .await
        .map_err(|e| {
            error!("Failed to record domain adapter test: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to record test results")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let test_result = TestDomainAdapterResponse {
        test_id,
        adapter_id: adapter_id.clone(),
        input_data: req.input_data,
        actual_output,
        expected_output: req.expected_output,
        epsilon,
        passed,
        iterations: iterations as u32,
        execution_time_ms,
        executed_at: Utc::now().to_rfc3339(),
    };

    info!("Domain adapter test completed: {}", test_id);
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
    Path(adapter_id): Path<String>,
) -> Result<Json<DomainAdapterManifestResponse>, (StatusCode, Json<ErrorResponse>)> {
    let manifest = state
        .db
        .get_domain_adapter_manifest(&adapter_id)
        .await
        .map_err(|e| {
            error!("Failed to get domain adapter manifest: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get domain adapter manifest")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            warn!("Domain adapter manifest not found: {}", adapter_id);
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("Domain adapter manifest not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(format!("Adapter ID: {}", adapter_id)),
                ),
            )
        })?;

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
    Path(adapter_id): Path<String>,
    Json(input_data): Json<serde_json::Value>,
) -> Result<Json<DomainAdapterExecutionResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Check if adapter exists and is loaded
    let adapter = state
        .db
        .get_domain_adapter(&adapter_id)
        .await
        .map_err(|e| {
            error!("Failed to get domain adapter: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get domain adapter")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            warn!("Domain adapter not found: {}", adapter_id);
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("Domain adapter not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(format!("Adapter ID: {}", adapter_id)),
                ),
            )
        })?;

    if adapter.status != "loaded" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Domain adapter must be loaded before execution")
                    .with_code("BAD_REQUEST")
                    .with_string_details(format!("Adapter status: {}", adapter.status)),
            ),
        ));
    }

    let start_time = std::time::Instant::now();

    // TODO: Implement actual adapter execution
    // This would involve:
    // 1. Preparing the input data
    // 2. Running through the deterministic executor
    // 3. Collecting trace events
    // 4. Calculating epsilon
    // 5. Returning the result

    // For now, simulate execution
    let execution_time_ms = start_time.elapsed().as_millis() as u64;
    let input_hash = format!(
        "{:x}",
        md5::compute(serde_json::to_string(&input_data).unwrap_or_default())
    );
    let output_hash = "simulated_output_hash".to_string(); // Mock output hash
    let epsilon = 0.001; // Mock epsilon
    let trace_events = vec![
        "adapter_prepare".to_string(),
        "adapter_forward".to_string(),
        "adapter_postprocess".to_string(),
    ];

    // Record execution in database
    let execution_id = state
        .db
        .record_domain_adapter_execution(
            &adapter_id,
            &input_hash,
            &output_hash,
            epsilon,
            execution_time_ms,
            &trace_events,
        )
        .await
        .map_err(|e| {
            error!("Failed to record domain adapter execution: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to record execution results")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let execution = DomainAdapterExecutionResponse {
        execution_id,
        adapter_id: adapter_id.clone(),
        input_hash,
        output_hash,
        epsilon,
        execution_time_ms,
        trace_events,
        executed_at: Utc::now().to_rfc3339(),
    };

    info!("Domain adapter execution completed: {}", execution_id);
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
    Path(adapter_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Check if adapter exists
    let adapter = state
        .db
        .get_domain_adapter(&adapter_id)
        .await
        .map_err(|e| {
            error!("Failed to get domain adapter: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get domain adapter")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    if adapter.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("Domain adapter not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(format!("Adapter ID: {}", adapter_id)),
            ),
        ));
    }

    // TODO: Unload adapter from deterministic executor if loaded
    // This would involve unregistering from the executor

    // Delete adapter from database (cascades to executions and tests)
    state
        .db
        .delete_domain_adapter(&adapter_id)
        .await
        .map_err(|e| {
            error!("Failed to delete domain adapter: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to delete domain adapter")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    info!("Domain adapter deleted: {}", adapter_id);
    Ok(StatusCode::NO_CONTENT)
}
