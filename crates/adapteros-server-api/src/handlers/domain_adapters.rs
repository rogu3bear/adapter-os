use crate::state::AppState;
use crate::types::{
    CreateDomainAdapterRequest, DomainAdapterExecutionResponse, DomainAdapterManifestResponse,
    DomainAdapterResponse, EpsilonStatsResponse, ErrorResponse, LoadDomainAdapterRequest,
    TestDomainAdapterRequest, TestDomainAdapterResponse,
};
use adapteros_core::{AosError, Result as AosResult};
use adapteros_deterministic_exec::{DeterministicExecutor, ExecutorConfig};
use adapteros_domain::error::DomainAdapterError;
use adapteros_domain::manifest::{load_manifest, AdapterManifest};
use adapteros_domain::{DomainAdapter, TelemetryAdapter, TensorData, TextAdapter, VisionAdapter};
use adapteros_trace::Event;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use chrono::Utc;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;
use uuid::Uuid;

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
    // TODO: Implement domain adapter database methods
    // let adapters = state.db.list_domain_adapters().await
    //     .map_err(|e| {
    //         tracing::error!("Failed to list domain adapters: {}", e);
    //         (
    //             StatusCode::INTERNAL_SERVER_ERROR,
    //             Json(ErrorResponse::new("Failed to list domain adapters").with_code("INTERNAL_ERROR").with_string_details(e.to_string())),
    //         )
    //     })?;

    // Mock response for now
    let adapters = vec![];

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
    // Citation: crates/adapteros-server-api/src/handlers/git_repository.rs L280-L304
    // TODO: Implement domain adapter database methods
    // let adapter = state.db.get_domain_adapter(&adapter_id).await
    //     .map_err(|e| {
    //         tracing::error!("Failed to get domain adapter: {}", e);
    //         (
    //             StatusCode::INTERNAL_SERVER_ERROR,
    //             Json(ErrorResponse::new("Failed to get domain adapter").with_code("INTERNAL_ERROR").with_string_details(e.to_string())),
    //         )
    //     })?;

    // Mock response for now
    let adapter = None;

    let adapter = adapter.ok_or_else(|| {
        tracing::warn!("Domain adapter not found: {}", adapter_id);
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
    State(_state): State<AppState>,
    Json(req): Json<CreateDomainAdapterRequest>,
) -> Result<(StatusCode, Json<DomainAdapterResponse>), (StatusCode, Json<ErrorResponse>)> {
    // Validate inputs
    if req.name.is_empty() || req.domain_type.is_empty() || req.model.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("name, domain_type, and model are required")
                    .with_code("INTERNAL_ERROR"),
            ),
        ));
    }

    // Domain adapter creation - placeholder implementation
    // Future implementation would involve:
    // 1. Creating the adapter manifest
    // 2. Registering with the deterministic executor
    // 3. Storing in database

    let adapter = DomainAdapterResponse {
        id: Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string(),
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
    State(_state): State<AppState>,
    Path(adapter_id): Path<String>,
    Json(_req): Json<LoadDomainAdapterRequest>,
) -> Result<Json<DomainAdapterResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Adapter loading - placeholder implementation
    // This would involve:
    // 1. Loading the adapter manifest
    // 2. Registering with the deterministic executor
    // 3. Updating the adapter status

    let adapter = DomainAdapterResponse {
        id: adapter_id.clone(),
        name: format!("{}-v1", adapter_id),
        version: "0.1.0".to_string(),
        description: "Mock domain adapter".to_string(),
        domain_type: "text".to_string(),
        model: "mock_model".to_string(),
        hash: "mock_hash".to_string(),
        input_format: "mock_input".to_string(),
        output_format: "mock_output".to_string(),
        config: HashMap::new(),
        status: "loaded".to_string(),
        epsilon_stats: Some(EpsilonStatsResponse {
            mean_error: 0.001,
            max_error: 0.005,
            error_count: 0,
            last_updated: Utc::now().to_rfc3339(),
        }),
        last_execution: None,
        execution_count: 0,
        created_at: Utc::now().to_rfc3339(),
        updated_at: Utc::now().to_rfc3339(),
    };

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
    State(_state): State<AppState>,
    Path(adapter_id): Path<String>,
) -> Result<Json<DomainAdapterResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Adapter unloading - placeholder implementation
    // This would involve:
    // 1. Unregistering from the deterministic executor
    // 2. Updating the adapter status

    let adapter = DomainAdapterResponse {
        id: adapter_id.clone(),
        name: format!("{}-v1", adapter_id),
        version: "0.1.0".to_string(),
        description: "Mock domain adapter".to_string(),
        domain_type: "text".to_string(),
        model: "mock_model".to_string(),
        hash: "mock_hash".to_string(),
        input_format: "mock_input".to_string(),
        output_format: "mock_output".to_string(),
        config: HashMap::new(),
        status: "unloaded".to_string(),
        epsilon_stats: None,
        last_execution: None,
        execution_count: 0,
        created_at: Utc::now().to_rfc3339(),
        updated_at: Utc::now().to_rfc3339(),
    };

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
    State(_state): State<AppState>,
    Path(adapter_id): Path<String>,
    Json(req): Json<TestDomainAdapterRequest>,
) -> Result<Json<TestDomainAdapterResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Determinism testing - placeholder implementation
    // This would involve:
    // 1. Running the adapter multiple times with the same input
    // 2. Comparing outputs for byte-identical results
    // 3. Calculating epsilon (numerical drift)
    // 4. Generating trace events

    let test_result = TestDomainAdapterResponse {
        test_id: Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string(),
        adapter_id: adapter_id.clone(),
        input_data: req.input_data,
        actual_output: "mock_output".to_string(),
        expected_output: req.expected_output,
        epsilon: Some(0.001),
        passed: true,
        iterations: req.iterations.unwrap_or(100),
        execution_time_ms: 150,
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
    State(_state): State<AppState>,
    Path(adapter_id): Path<String>,
) -> Result<Json<DomainAdapterManifestResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Manifest retrieval - placeholder implementation
    // This would involve loading the TOML manifest file

    let manifest = DomainAdapterManifestResponse {
        adapter_id: adapter_id.clone(),
        name: format!("{}-v1", adapter_id),
        version: "0.1.0".to_string(),
        description: "Mock domain adapter manifest".to_string(),
        domain_type: "text".to_string(),
        model: "mock_model".to_string(),
        hash: "mock_hash".to_string(),
        input_format: "mock_input".to_string(),
        output_format: "mock_output".to_string(),
        config: HashMap::new(),
        created_at: Utc::now().to_rfc3339(),
        updated_at: Utc::now().to_rfc3339(),
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
    State(_state): State<AppState>,
    Path(adapter_id): Path<String>,
    Json(input_data): Json<Value>,
) -> Result<Json<DomainAdapterExecutionResponse>, (StatusCode, Json<ErrorResponse>)> {
    match execute_domain_adapter_internal(&adapter_id, &input_data) {
        Ok(response) => Ok(Json(response)),
        Err(err) => {
            tracing::error!(adapter_id, error = %err, "Domain adapter execution failed");
            let (status, error_body) = map_error_to_response(err);
            Err((status, Json(error_body)))
        }
    }
}

fn execute_domain_adapter_internal(
    adapter_id: &str,
    payload: &Value,
) -> AosResult<DomainAdapterExecutionResponse> {
    let kind = AdapterKind::from_id(adapter_id)
        .ok_or_else(|| AosError::NotFound(format!("Unknown domain adapter: {adapter_id}")))?;

    let manifest_path = manifest_path(kind);
    if !manifest_path.exists() {
        return Err(AosError::NotFound(format!(
            "Domain adapter manifest not found at {}",
            manifest_path.display()
        )));
    }

    let manifest = load_manifest(&manifest_path).map_err(map_domain_error)?;
    let mut adapter = load_adapter(kind, &manifest_path)?;
    let input_tensor = prepare_input(&adapter, kind, &manifest, payload)?;

    let seed_hash = manifest.parse_hash().map_err(map_domain_error)?;
    let mut executor = DeterministicExecutor::new(ExecutorConfig {
        global_seed: seed_hash.to_bytes(),
        enable_event_logging: true,
        ..Default::default()
    });

    let input_hash = input_tensor.metadata.hash.to_hex();
    let mut trace_events = Vec::new();
    let mut final_output: Option<TensorData> = None;
    let mut epsilon = 0.0f64;
    let start = Instant::now();

    {
        let adapter_ref = adapter.as_domain_adapter_mut();
        adapter_ref
            .prepare(&mut executor)
            .map_err(map_domain_error)?;
        push_trace_event(
            adapter_ref,
            1,
            "adapter.prepare",
            HashMap::from([
                ("adapter_id".to_string(), json!(adapter_id)),
                ("input_hash".to_string(), json!(input_hash)),
            ]),
            HashMap::from([("status".to_string(), json!("ready"))]),
            &mut trace_events,
        )?;

        let forward_output = adapter_ref
            .forward(&input_tensor)
            .map_err(map_domain_error)?;
        let forward_hash = forward_output.metadata.hash.to_hex();
        push_trace_event(
            adapter_ref,
            2,
            "adapter.forward",
            HashMap::from([
                ("adapter_id".to_string(), json!(adapter_id)),
                ("input_hash".to_string(), json!(input_hash)),
            ]),
            HashMap::from([("output_hash".to_string(), json!(forward_hash))]),
            &mut trace_events,
        )?;

        let post_output = adapter_ref
            .postprocess(&forward_output)
            .map_err(map_domain_error)?;
        let output_hash = post_output.metadata.hash.to_hex();
        push_trace_event(
            adapter_ref,
            3,
            "adapter.postprocess",
            HashMap::from([
                ("adapter_id".to_string(), json!(adapter_id)),
                ("input_hash".to_string(), json!(input_hash)),
            ]),
            HashMap::from([("output_hash".to_string(), json!(output_hash))]),
            &mut trace_events,
        )?;

        epsilon = adapter_ref
            .epsilon_stats()
            .map(|stats| stats.l2_error)
            .unwrap_or(0.0);

        adapter_ref.reset();
        final_output = Some(post_output);
    }

    let final_output = final_output.ok_or_else(|| {
        AosError::Internal("Adapter execution did not produce output".to_string())
    })?;

    let execution_time_ms = start.elapsed().as_millis() as u64;

    Ok(DomainAdapterExecutionResponse {
        execution_id: Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string(),
        adapter_id: adapter_id.to_string(),
        input_hash,
        output_hash: final_output.metadata.hash.to_hex(),
        epsilon,
        execution_time_ms,
        trace_events,
        executed_at: Utc::now().to_rfc3339(),
    })
}

fn map_error_to_response(err: AosError) -> (StatusCode, ErrorResponse) {
    match err {
        AosError::NotFound(msg) => (
            StatusCode::NOT_FOUND,
            ErrorResponse::new(msg).with_code("NOT_FOUND"),
        ),
        AosError::Validation(msg) => (
            StatusCode::BAD_REQUEST,
            ErrorResponse::new(msg).with_code("BAD_REQUEST"),
        ),
        AosError::DeterministicExecutor(msg) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            ErrorResponse::new(msg).with_code("DETERMINISTIC_EXECUTOR_ERROR"),
        ),
        AosError::DeterminismViolation(msg) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            ErrorResponse::new(msg).with_code("DETERMINISM_VIOLATION"),
        ),
        AosError::Telemetry(msg) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            ErrorResponse::new(msg).with_code("TELEMETRY_ERROR"),
        ),
        other => (
            StatusCode::INTERNAL_SERVER_ERROR,
            ErrorResponse::new(other.to_string()).with_code("INTERNAL_ERROR"),
        ),
    }
}

fn map_domain_error(err: DomainAdapterError) -> AosError {
    match err {
        DomainAdapterError::ManifestLoadError { path, source } => {
            AosError::InvalidManifest(format!("Failed to load manifest from {}: {}", path, source))
        }
        DomainAdapterError::InvalidManifest { reason } => AosError::InvalidManifest(reason),
        DomainAdapterError::TensorShapeMismatch { expected, actual } => {
            AosError::Validation(format!(
                "Tensor shape mismatch: expected {:?}, got {:?}",
                expected, actual
            ))
        }
        DomainAdapterError::UnsupportedInputFormat { format } => {
            AosError::Validation(format!("Unsupported input format: {}", format))
        }
        DomainAdapterError::UnsupportedOutputFormat { format } => {
            AosError::Validation(format!("Unsupported output format: {}", format))
        }
        DomainAdapterError::AdapterNotInitialized { adapter_name } => {
            AosError::Internal(format!("Adapter not initialized: {}", adapter_name))
        }
        DomainAdapterError::DeterminismViolation { details } => {
            AosError::DeterminismViolation(details)
        }
        DomainAdapterError::NumericalErrorThreshold { error, threshold } => {
            AosError::DeterminismViolation(format!(
                "Numerical error {error} exceeded threshold {threshold}"
            ))
        }
        DomainAdapterError::ModelFileNotFound { path } => {
            AosError::NotFound(format!("Model file not found: {}", path))
        }
        DomainAdapterError::HashVerificationFailed { expected, actual } => {
            AosError::Validation(format!(
                "Hash verification failed: expected {}, got {}",
                expected, actual
            ))
        }
        DomainAdapterError::TokenizationError { details } => {
            AosError::Validation(format!("Tokenization error: {}", details))
        }
        DomainAdapterError::ImageProcessingError { details } => {
            AosError::Validation(format!("Image processing error: {}", details))
        }
        DomainAdapterError::TelemetryError { details } => AosError::Telemetry(details),
        DomainAdapterError::ExecutorError(e) => AosError::DeterministicExecutor(e.to_string()),
        DomainAdapterError::NumericsError(e) => AosError::Quantization(e.to_string()),
        DomainAdapterError::IoError(e) => AosError::Io(e.to_string()),
        DomainAdapterError::SerializationError(e) => AosError::Serialization(e),
        DomainAdapterError::TomlError(e) => AosError::Parse(e.to_string()),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AdapterKind {
    Text,
    Vision,
    Telemetry,
}

impl AdapterKind {
    fn from_id(adapter_id: &str) -> Option<Self> {
        match adapter_id {
            "text_adapter_v1" | "text" => Some(Self::Text),
            "vision_adapter_v1" | "vision" => Some(Self::Vision),
            "telemetry_adapter_v1" | "telemetry" => Some(Self::Telemetry),
            _ => None,
        }
    }

    fn manifest_file(self) -> &'static str {
        match self {
            Self::Text => "text_example.toml",
            Self::Vision => "vision_example.toml",
            Self::Telemetry => "telemetry_example.toml",
        }
    }
}

fn manifest_path(kind: AdapterKind) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../adapteros-domain/manifests")
        .join(kind.manifest_file())
}

enum LoadedAdapter {
    Text(TextAdapter),
    Vision(VisionAdapter),
    Telemetry(TelemetryAdapter),
}

impl LoadedAdapter {
    fn as_domain_adapter_mut(&mut self) -> &mut dyn DomainAdapter {
        match self {
            Self::Text(adapter) => adapter,
            Self::Vision(adapter) => adapter,
            Self::Telemetry(adapter) => adapter,
        }
    }

    fn as_text(&self) -> Option<&TextAdapter> {
        if let Self::Text(adapter) = self {
            Some(adapter)
        } else {
            None
        }
    }

    fn as_vision(&self) -> Option<&VisionAdapter> {
        if let Self::Vision(adapter) = self {
            Some(adapter)
        } else {
            None
        }
    }

    fn kind(&self) -> AdapterKind {
        match self {
            Self::Text(_) => AdapterKind::Text,
            Self::Vision(_) => AdapterKind::Vision,
            Self::Telemetry(_) => AdapterKind::Telemetry,
        }
    }
}

fn load_adapter(kind: AdapterKind, manifest_path: &Path) -> AosResult<LoadedAdapter> {
    match kind {
        AdapterKind::Text => Ok(LoadedAdapter::Text(
            TextAdapter::load(manifest_path).map_err(map_domain_error)?,
        )),
        AdapterKind::Vision => Ok(LoadedAdapter::Vision(
            VisionAdapter::load(manifest_path).map_err(map_domain_error)?,
        )),
        AdapterKind::Telemetry => Ok(LoadedAdapter::Telemetry(
            TelemetryAdapter::load(manifest_path).map_err(map_domain_error)?,
        )),
    }
}

fn prepare_input(
    adapter: &LoadedAdapter,
    kind: AdapterKind,
    manifest: &AdapterManifest,
    payload: &Value,
) -> AosResult<TensorData> {
    match kind {
        AdapterKind::Text => {
            let text = extract_text(payload)?;
            let adapter = adapter
                .as_text()
                .expect("Loaded text adapter should be present");
            adapteros_domain::text::text_to_tensor(adapter, &text).map_err(map_domain_error)
        }
        AdapterKind::Vision => {
            let bytes = extract_image_bytes(payload)?;
            let adapter = adapter
                .as_vision()
                .expect("Loaded vision adapter should be present");
            adapteros_domain::vision::image_to_tensor(adapter, &bytes).map_err(map_domain_error)
        }
        AdapterKind::Telemetry => {
            let num_channels = manifest.get_parameter_i64("num_channels").unwrap_or(16) as usize;
            let window_size = manifest.get_parameter_i64("window_size").unwrap_or(128) as usize;
            let values = extract_timeseries(payload, num_channels * window_size)?;
            adapteros_domain::telemetry::timeseries_to_tensor(num_channels, window_size, &values)
                .map_err(map_domain_error)
        }
    }
}

fn extract_text(payload: &Value) -> AosResult<String> {
    if let Some(obj) = payload.as_object() {
        if let Some(value) = obj.get("text") {
            return value
                .as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| AosError::Validation("Field 'text' must be a string".to_string()));
        }
    }
    Err(AosError::Validation(
        "Text adapter input must be an object with a 'text' field".to_string(),
    ))
}

fn extract_image_bytes(payload: &Value) -> AosResult<Vec<u8>> {
    let obj = payload.as_object().ok_or_else(|| {
        AosError::Validation("Vision adapter input must be a JSON object".to_string())
    })?;

    if let Some(encoded) = obj.get("image_base64") {
        let data = encoded.as_str().ok_or_else(|| {
            AosError::Validation("Field 'image_base64' must be a string".to_string())
        })?;
        return BASE64_STANDARD
            .decode(data)
            .map_err(|e| AosError::Validation(format!("Invalid base64 data: {}", e)));
    }

    if let Some(bytes) = obj.get("image_bytes") {
        let array = bytes.as_array().ok_or_else(|| {
            AosError::Validation("Field 'image_bytes' must be an array".to_string())
        })?;
        let mut data = Vec::with_capacity(array.len());
        for value in array {
            let byte = value.as_u64().ok_or_else(|| {
                AosError::Validation(
                    "All elements of 'image_bytes' must be unsigned integers".to_string(),
                )
            })?;
            if byte > 255 {
                return Err(AosError::Validation(
                    "Values in 'image_bytes' must be between 0 and 255".to_string(),
                ));
            }
            data.push(byte as u8);
        }
        return Ok(data);
    }

    Err(AosError::Validation(
        "Vision adapter input must include 'image_base64' or 'image_bytes'".to_string(),
    ))
}

fn extract_timeseries(payload: &Value, expected_len: usize) -> AosResult<Vec<f32>> {
    let obj = payload.as_object().ok_or_else(|| {
        AosError::Validation("Telemetry adapter input must be a JSON object".to_string())
    })?;

    let values = obj
        .get("values")
        .ok_or_else(|| AosError::Validation("Missing 'values' field".to_string()))?
        .as_array()
        .ok_or_else(|| AosError::Validation("Field 'values' must be an array".to_string()))?;

    if values.len() != expected_len {
        return Err(AosError::Validation(format!(
            "Telemetry input length mismatch: expected {}, got {}",
            expected_len,
            values.len()
        )));
    }

    let mut output = Vec::with_capacity(values.len());
    for value in values {
        let number = value.as_f64().ok_or_else(|| {
            AosError::Validation("All telemetry values must be numeric".to_string())
        })?;
        output.push(number as f32);
    }

    Ok(output)
}

fn push_trace_event(
    adapter: &dyn DomainAdapter,
    tick_id: u64,
    op_id: &str,
    inputs: HashMap<String, Value>,
    outputs: HashMap<String, Value>,
    trace_events: &mut Vec<String>,
) -> AosResult<()> {
    let event: Event = adapter.create_trace_event(tick_id, op_id.to_string(), &inputs, &outputs);
    let serialized = serde_json::to_string(&event)?;
    trace_events.push(serialized);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn text_adapter_execution_produces_trace() {
        let payload = json!({ "text": "Deterministic execution test" });
        let response = execute_domain_adapter_internal("text_adapter_v1", &payload)
            .expect("text adapter should execute successfully");

        assert_eq!(response.adapter_id, "text_adapter_v1");
        assert_eq!(response.trace_events.len(), 3);
        assert!(!response.input_hash.is_empty());
        assert!(!response.output_hash.is_empty());
    }

    #[test]
    fn vision_adapter_accepts_base64_payload() {
        let image_bytes = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
        let payload = json!({
            "image_base64": BASE64_STANDARD.encode(image_bytes),
        });

        let response = execute_domain_adapter_internal("vision_adapter_v1", &payload)
            .expect("vision adapter should execute successfully");

        assert_eq!(response.adapter_id, "vision_adapter_v1");
        assert_eq!(response.trace_events.len(), 3);
    }

    #[test]
    fn telemetry_adapter_requires_correct_length() {
        let manifest = load_manifest(&manifest_path(AdapterKind::Telemetry))
            .expect("manifest should load for tests");
        let num_channels = manifest.get_parameter_i64("num_channels").unwrap_or(16) as usize;
        let window_size = manifest.get_parameter_i64("window_size").unwrap_or(128) as usize;
        let expected = num_channels * window_size;

        let values: Vec<f32> = (0..expected).map(|v| v as f32).collect();
        let payload = json!({ "values": values });

        let response = execute_domain_adapter_internal("telemetry_adapter_v1", &payload)
            .expect("telemetry adapter should execute successfully");

        assert_eq!(response.adapter_id, "telemetry_adapter_v1");
        assert_eq!(response.trace_events.len(), 3);

        let invalid_values: Vec<f32> = (0..(expected - 1)).map(|v| v as f32).collect();
        let invalid_payload = json!({ "values": invalid_values });

        let error = execute_domain_adapter_internal("telemetry_adapter_v1", &invalid_payload)
            .expect_err("mismatched telemetry input should fail");
        assert!(matches!(error, AosError::Validation(_)));
    }

    #[test]
    fn unknown_adapter_returns_not_found() {
        let payload = json!({});
        let error = execute_domain_adapter_internal("unknown_adapter", &payload)
            .expect_err("unknown adapter should error");
        assert!(matches!(error, AosError::NotFound(_)));
    }
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
    State(_state): State<AppState>,
    Path(adapter_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Adapter deletion - placeholder implementation
    // This would involve:
    // 1. Unloading the adapter if loaded
    // 2. Removing from database
    // 3. Cleaning up manifest files

    tracing::info!("Domain adapter {} deleted", adapter_id);
    Ok(StatusCode::NO_CONTENT)
}
