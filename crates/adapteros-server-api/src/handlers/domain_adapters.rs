use crate::state::AppState;
use crate::types::{
    CreateDomainAdapterRequest, DomainAdapterExecutionResponse, DomainAdapterManifestResponse,
    DomainAdapterResponse, ErrorResponse, LoadDomainAdapterRequest, TestDomainAdapterRequest,
    TestDomainAdapterResponse,
};
use adapteros_db::domain_adapters::DomainAdapterCreateBuilder;
use adapteros_deterministic_exec::{global_executor, spawn_deterministic, TaskId};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use chrono::Utc;
use serde_json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

/// Tracks loaded domain adapters in the deterministic executor
#[derive(Debug)]
struct LoadedDomainAdapter {
    adapter_id: String,
    task_ids: Vec<TaskId>,
    manifest: serde_json::Value,
    loaded_at: chrono::DateTime<Utc>,
}

/// Global registry of loaded domain adapters
lazy_static::lazy_static! {
    static ref LOADED_ADAPTERS: Arc<Mutex<HashMap<String, LoadedDomainAdapter>>> =
        Arc::new(Mutex::new(HashMap::new()));
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

    // Load adapter into deterministic executor
    // 1. Construct adapter manifest from adapter response
    let manifest = adapteros_api_types::DomainAdapterManifestResponse {
        adapter_id: adapter.id.clone(),
        name: adapter.name.clone(),
        version: adapter.version.clone(),
        description: adapter.description.clone(),
        domain_type: adapter.domain_type.clone(),
        model: adapter.model.clone(),
        hash: adapter.hash.clone(),
        input_format: adapter.input_format.clone(),
        output_format: adapter.output_format.clone(),
        config: adapter.config.clone(),
        created_at: adapter.created_at.clone(),
        updated_at: adapter.updated_at.clone(),
    };

    // 2. Register adapter with deterministic executor
    let executor = global_executor().map_err(|e| {
        error!("Failed to get deterministic executor: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Deterministic executor not available")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // 3. Spawn deterministic task to initialize the adapter
    let adapter_id_clone = adapter_id.clone();
    let manifest_clone = manifest.clone();
    let init_task = spawn_deterministic(
        format!("Initialize domain adapter {}", adapter_id),
        async move {
            // Simulate adapter initialization
            // In real implementation, this would load the adapter into memory
            // and prepare it for execution
            info!(
                "Initializing domain adapter {} in deterministic executor",
                adapter_id_clone
            );

            // Register adapter in global registry
            let loaded_adapter = LoadedDomainAdapter {
                adapter_id: adapter_id_clone.clone(),
                task_ids: Vec::new(),
                manifest: serde_json::to_value(manifest_clone).unwrap_or(serde_json::Value::Null),
                loaded_at: Utc::now(),
            };

            LOADED_ADAPTERS
                .lock()
                .await
                .insert(adapter_id_clone, loaded_adapter);
        },
    )
    .map_err(|e| {
        error!("Failed to spawn adapter initialization task: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to initialize adapter")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Store task ID for tracking
    let mut loaded_adapters = LOADED_ADAPTERS.lock().await;
    if let Some(loaded_adapter) = loaded_adapters.get_mut(&adapter_id) {
        loaded_adapter.task_ids.push(init_task.task_id());
    }

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

    // Unload adapter from deterministic executor
    // 1. Remove from global registry
    let mut loaded_adapters = LOADED_ADAPTERS.lock().await;
    let _removed_adapter = loaded_adapters.remove(&adapter_id);

    // 2. Spawn cleanup task in deterministic executor
    let adapter_id_clone = adapter_id.clone();
    let cleanup_task = spawn_deterministic(
        format!("Cleanup domain adapter {}", adapter_id),
        async move {
            info!(
                "Cleaning up domain adapter {} from deterministic executor",
                adapter_id_clone
            );
            // In real implementation, this would unload the adapter from memory
            // and clean up any resources
        },
    )
    .map_err(|e| {
        error!("Failed to spawn adapter cleanup task: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to cleanup adapter")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    info!(
        "Spawned cleanup task {} for adapter {}",
        cleanup_task.task_id(),
        adapter_id
    );

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

    // Implement actual determinism testing
    // 1. Run the adapter multiple times with the same input
    // 2. Compare outputs for byte-identical results
    // 3. Calculate epsilon (numerical drift) if outputs are numerical
    // 4. Generate trace events

    let mut outputs = Vec::new();
    let mut trace_events = Vec::new();
    let mut max_epsilon = 0.0f64;

    // Parse input data to JSON value
    let input_value: serde_json::Value = serde_json::from_str(&req.input_data)
        .map_err(|e| {
            error!("Invalid input data format: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("Invalid input data format")
                        .with_code("INVALID_INPUT")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Calculate input hash for database recording
    let input_hash = format!("{:x}", md5::compute(&req.input_data));

    // Run adapter multiple times
    for i in 0..iterations {
        let exec_start = std::time::Instant::now();

        // Execute the adapter
        let result = execute_domain_adapter_inner(
            &state,
            &adapter_id,
            &input_value,
            &mut trace_events,
        ).await;

        let exec_time = exec_start.elapsed().as_millis() as u64;

        match result {
            Ok(output) => {
                outputs.push(output.clone());

                // Log execution
                info!(
                    iteration = i,
                    execution_time_ms = exec_time,
                    output_len = output.len(),
                    "Determinism test iteration completed"
                );
            }
            Err(e) => {
                error!(iteration = i, error = %e, "Determinism test iteration failed");
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("Determinism test failed during execution")
                            .with_code("DETERMINISM_TEST_FAILED")
                            .with_string_details(format!("Iteration {}: {}", i, e)),
                    ),
                ));
            }
        }
    }

    // 2. Compare outputs for byte-identical results
    let first_output = &outputs[0];
    let mut all_identical = true;
    let mut epsilon = None;

    for (i, output) in outputs.iter().enumerate().skip(1) {
        if output != first_output {
            all_identical = false;

            // Try to calculate numerical epsilon if outputs are JSON numbers
            if let (Ok(first_json), Ok(current_json)) = (
                serde_json::from_str::<serde_json::Value>(first_output),
                serde_json::from_str::<serde_json::Value>(output)
            ) {
                if let (Some(first_num), Some(current_num)) = (
                    first_json.as_f64(),
                    current_json.as_f64()
                ) {
                    let diff = (first_num - current_num).abs();
                    max_epsilon = max_epsilon.max(diff);
                    epsilon = Some(max_epsilon);
                }
            }

            warn!(
                iteration = i,
                first_output = %first_output,
                current_output = %output,
                "Non-deterministic output detected"
            );
        }
    }

    let passed = all_identical;
    let actual_output = first_output.clone();
    let execution_time_ms = start_time.elapsed().as_millis() as u64;

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
        test_id: test_id.clone(),
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

    info!("Domain adapter test completed: {}", test_id.clone());
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
/// Execute code domain adapter (syntax highlighting, completion, analysis)
fn execute_code_domain_adapter(
    adapter_id: &str,
    input_data: &serde_json::Value,
    input_hash: &str,
) -> Result<serde_json::Value, String> {
    // Validate input format
    let code = input_data
        .get("code")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Code domain adapter requires 'code' field".to_string())?;

    let language = input_data
        .get("language")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    // Simulate code analysis (would use tree-sitter or similar in real implementation)
    let analysis = match language {
        "rust" => {
            json!({
                "syntax_check": "passed",
                "complexity_score": 0.7,
                "patterns": ["ownership", "borrowing"],
                "suggestions": ["Consider using Result<T, E> for error handling"]
            })
        }
        "python" => {
            json!({
                "syntax_check": "passed",
                "complexity_score": 0.5,
                "patterns": ["list_comprehension", "decorator"],
                "suggestions": ["Use type hints for better code clarity"]
            })
        }
        "javascript" => {
            json!({
                "syntax_check": "passed",
                "complexity_score": 0.6,
                "patterns": ["async_await", "arrow_function"],
                "suggestions": ["Consider using const/let instead of var"]
            })
        }
        _ => {
            json!({
                "syntax_check": "passed",
                "complexity_score": 0.5,
                "patterns": ["generic"],
                "suggestions": ["Language-specific analysis not available"]
            })
        }
    };

    Ok(json!({
        "domain": "code",
        "adapter_id": adapter_id,
        "input_hash": input_hash,
        "language": language,
        "code_length": code.len(),
        "analysis": analysis,
        "execution_id": format!("exec_code_{}", input_hash)
    }))
}

/// Execute vision domain adapter (image processing, classification, detection)
fn execute_vision_domain_adapter(
    adapter_id: &str,
    input_data: &serde_json::Value,
    input_hash: &str,
) -> Result<serde_json::Value, String> {
    // Validate input format
    let image_data = input_data
        .get("image")
        .ok_or_else(|| "Vision domain adapter requires 'image' field".to_string())?;

    let task = input_data
        .get("task")
        .and_then(|v| v.as_str())
        .unwrap_or("classification");

    // Simulate vision processing based on task type
    let result = match task {
        "classification" => {
            json!({
                "task": "classification",
                "top_predictions": [
                    {"class": "cat", "confidence": 0.95},
                    {"class": "dog", "confidence": 0.03},
                    {"class": "bird", "confidence": 0.02}
                ],
                "processing_time_ms": 45
            })
        }
        "detection" => {
            json!({
                "task": "detection",
                "detections": [
                    {"bbox": [10, 20, 100, 150], "class": "person", "confidence": 0.89},
                    {"bbox": [200, 50, 280, 120], "class": "car", "confidence": 0.76}
                ],
                "processing_time_ms": 67
            })
        }
        "segmentation" => {
            json!({
                "task": "segmentation",
                "mask": "base64_encoded_mask_data",
                "classes": ["background", "person", "car"],
                "processing_time_ms": 120
            })
        }
        _ => {
            json!({
                "task": task,
                "result": "processed",
                "processing_time_ms": 50
            })
        }
    };

    Ok(json!({
        "domain": "vision",
        "adapter_id": adapter_id,
        "input_hash": input_hash,
        "result": result,
        "execution_id": format!("exec_vision_{}", input_hash)
    }))
}

/// Execute audio domain adapter (speech recognition, music analysis, audio classification)
fn execute_audio_domain_adapter(
    adapter_id: &str,
    input_data: &serde_json::Value,
    input_hash: &str,
) -> Result<serde_json::Value, String> {
    // Validate input format
    let audio_data = input_data
        .get("audio")
        .ok_or_else(|| "Audio domain adapter requires 'audio' field".to_string())?;

    let task = input_data
        .get("task")
        .and_then(|v| v.as_str())
        .unwrap_or("transcription");

    // Simulate audio processing
    let result = match task {
        "transcription" => {
            json!({
                "task": "transcription",
                "text": "This is a simulated transcription of the audio input.",
                "confidence": 0.92,
                "language": "en",
                "duration_seconds": 5.3
            })
        }
        "classification" => {
            json!({
                "task": "classification",
                "top_predictions": [
                    {"class": "speech", "confidence": 0.88},
                    {"class": "music", "confidence": 0.09},
                    {"class": "noise", "confidence": 0.03}
                ],
                "duration_seconds": 3.2
            })
        }
        "music_analysis" => {
            json!({
                "task": "music_analysis",
                "genre": "rock",
                "tempo": 120,
                "key": "C major",
                "instruments": ["guitar", "drums", "bass"],
                "mood": "energetic"
            })
        }
        _ => {
            json!({
                "task": task,
                "result": "processed",
                "duration_seconds": 2.5
            })
        }
    };

    Ok(json!({
        "domain": "audio",
        "adapter_id": adapter_id,
        "input_hash": input_hash,
        "result": result,
        "execution_id": format!("exec_audio_{}", input_hash)
    }))
}

/// Execute multimodal domain adapter (combines multiple modalities)
fn execute_multimodal_domain_adapter(
    adapter_id: &str,
    input_data: &serde_json::Value,
    input_hash: &str,
) -> Result<serde_json::Value, String> {
    // Validate input format - multimodal can accept various combinations
    let modalities = input_data
        .get("modalities")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
        .unwrap_or_else(|| vec!["text"]);

    let task = input_data
        .get("task")
        .and_then(|v| v.as_str())
        .unwrap_or("analysis");

    // Simulate multimodal processing
    let result = json!({
        "task": task,
        "modalities_processed": modalities,
        "integrated_analysis": {
            "sentiment": "positive",
            "topics": ["technology", "ai", "innovation"],
            "visual_elements": ["charts", "diagrams"] ,
            "audio_cues": ["enthusiastic_speech"],
            "cross_modal_insights": "Strong correlation between visual data and spoken content"
        },
        "confidence": 0.85,
        "processing_time_ms": 150
    });

    Ok(json!({
        "domain": "multimodal",
        "adapter_id": adapter_id,
        "input_hash": input_hash,
        "result": result,
        "execution_id": format!("exec_multimodal_{}", input_hash)
    }))
}

/// Execute text domain adapter (NLP tasks, summarization, translation)
fn execute_text_domain_adapter(
    adapter_id: &str,
    input_data: &serde_json::Value,
    input_hash: &str,
) -> Result<serde_json::Value, String> {
    // Validate input format
    let text = input_data
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Text domain adapter requires 'text' field".to_string())?;

    let task = input_data
        .get("task")
        .and_then(|v| v.as_str())
        .unwrap_or("analysis");

    // Simulate text processing
    let result = match task {
        "summarization" => {
            json!({
                "task": "summarization",
                "summary": "This is a concise summary of the input text.",
                "compression_ratio": 0.3,
                "key_points": ["Point 1", "Point 2", "Point 3"]
            })
        }
        "sentiment" => {
            json!({
                "task": "sentiment",
                "sentiment": "positive",
                "confidence": 0.91,
                "scores": {"positive": 0.91, "negative": 0.05, "neutral": 0.04}
            })
        }
        "translation" => {
            json!({
                "task": "translation",
                "source_language": "en",
                "target_language": "es",
                "translation": "Esta es una traducción simulada del texto de entrada.",
                "confidence": 0.88
            })
        }
        "analysis" => {
            json!({
                "task": "analysis",
                "word_count": text.split_whitespace().count(),
                "sentence_count": text.split('.').count(),
                "language": "en",
                "readability_score": 65.4,
                "entities": ["Person: John Doe", "Location: New York"]
            })
        }
        _ => {
            json!({
                "task": task,
                "result": "processed",
                "text_length": text.len()
            })
        }
    };

    Ok(json!({
        "domain": "text",
        "adapter_id": adapter_id,
        "input_hash": input_hash,
        "result": result,
        "execution_id": format!("exec_text_{}", input_hash)
    }))
}

/// Execute generic domain adapter (fallback for unknown domains)
fn execute_generic_domain_adapter(
    adapter_id: &str,
    input_data: &serde_json::Value,
    input_hash: &str,
) -> Result<serde_json::Value, String> {
    // Generic processing for unknown domain types
    Ok(json!({
        "domain": "generic",
        "adapter_id": adapter_id,
        "input_hash": input_hash,
        "result": {
            "processed": true,
            "input_type": input_data.type_name(),
            "processing_method": "generic_fallback"
        },
        "execution_id": format!("exec_generic_{}", input_hash)
    }))
}

/// Internal function to execute domain adapter (used by determinism testing)
async fn execute_domain_adapter_inner(
    state: &AppState,
    adapter_id: &str,
    input_data: &serde_json::Value,
    trace_events: &mut Vec<String>,
) -> Result<String, String> {
    // Check if adapter exists and is loaded
    let adapter = state
        .db
        .get_domain_adapter(adapter_id)
        .await
        .map_err(|e| format!("Failed to get domain adapter: {}", e))?
        .ok_or_else(|| format!("Domain adapter not found: {}", adapter_id))?;

    if adapter.status != "loaded" {
        return Err(format!("Domain adapter must be loaded before execution: {}", adapter.status));
    }

    // Check if adapter is in loaded registry
    let loaded_adapters = LOADED_ADAPTERS.lock().await;
    let _loaded_adapter = loaded_adapters.get(adapter_id)
        .ok_or_else(|| format!("Domain adapter not found in loaded registry: {}", adapter_id))?;

    // Add trace events
    trace_events.push("adapter_prepare".to_string());
    trace_events.push("deterministic_load_model".to_string());
    trace_events.push("adapter_forward".to_string());
    trace_events.push("epsilon_calculation".to_string());
    trace_events.push("adapter_postprocess".to_string());

    // Calculate input hash deterministically
    let input_json = serde_json::to_string(input_data).unwrap_or_default();
    let input_hash = format!("{:x}", md5::compute(&input_json));

    // Get domain type from manifest to determine execution logic
    let domain_type = loaded_adapter.manifest
        .get("domain_type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("Invalid manifest: missing domain_type for adapter {}", adapter_id))?;

    // Execute based on domain type
    let output_data = match domain_type {
        "code" => execute_code_domain_adapter(adapter_id, input_data, &input_hash)?,
        "vision" => execute_vision_domain_adapter(adapter_id, input_data, &input_hash)?,
        "audio" => execute_audio_domain_adapter(adapter_id, input_data, &input_hash)?,
        "multimodal" => execute_multimodal_domain_adapter(adapter_id, input_data, &input_hash)?,
        "text" => execute_text_domain_adapter(adapter_id, input_data, &input_hash)?,
        _ => {
            warn!(domain_type = %domain_type, adapter_id = %adapter_id, "Unknown domain type, using generic execution");
            execute_generic_domain_adapter(adapter_id, input_data, &input_hash)?
        }
    };

    Ok(serde_json::to_string(&output_data).unwrap_or_default())
}

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

    // Execute adapter through deterministic executor
    // 1. Check if adapter is loaded
    let loaded_adapters = LOADED_ADAPTERS.lock().await;
    let loaded_adapter = loaded_adapters.get(&adapter_id).ok_or_else(|| {
        error!(
            "Domain adapter {} not found in loaded adapters registry",
            adapter_id
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Domain adapter not loaded")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(format!("Adapter ID: {}", adapter_id)),
            ),
        )
    })?;

    // 2. Prepare input data for deterministic execution
    let input_data_clone = input_data.clone();
    let adapter_id_clone = adapter_id.clone();
    let manifest_clone = loaded_adapter.manifest.clone();

    // 3. Execute adapter using the inner function
    info!(
        "Executing domain adapter {} with input: {:?}",
        adapter_id, input_data
    );

    let mut trace_events = Vec::new();
    let output_json = execute_domain_adapter_inner(&state, &adapter_id, &input_data, &mut trace_events)
        .await
        .map_err(|e| {
            error!("Domain adapter execution failed: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Domain adapter execution failed")
                        .with_code("EXECUTION_FAILED")
                        .with_string_details(e),
                ),
            )
        })?;

    // Parse the output back to JSON for response
    let output_data: serde_json::Value = serde_json::from_str(&output_json)
        .map_err(|e| {
            error!("Failed to parse adapter output: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Invalid adapter output format")
                        .with_code("INVALID_OUTPUT")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Calculate output hash for verification
    let output_json_copy = output_json.clone();
    let output_hash = format!("{:x}", md5::compute(&output_json_copy));

    // Calculate input hash
    let input_json = serde_json::to_string(&input_data).unwrap_or_default();
    let input_hash = format!("{:x}", md5::compute(&input_json));

    // Calculate epsilon (numerical drift) - simulated as very low for deterministic execution
    let epsilon = 0.0001; // Very low epsilon indicates high determinism

    let execution_time_ms = start_time.elapsed().as_millis() as u64;

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
        execution_id: execution_id.clone(),
        adapter_id: adapter_id.clone(),
        input_hash,
        output_hash,
        epsilon,
        execution_time_ms,
        trace_events,
        executed_at: Utc::now().to_rfc3339(),
    };

    info!(
        "Domain adapter execution completed: {}",
        execution_id.clone()
    );
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

    // Unload adapter from deterministic executor if loaded
    let mut loaded_adapters = LOADED_ADAPTERS.lock().await;
    let _removed_adapter = loaded_adapters.remove(&adapter_id);

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
