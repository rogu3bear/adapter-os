use crate::state::AppState;
use crate::types::{
    CreateDomainAdapterRequest, DomainAdapterExecutionResponse, DomainAdapterManifestResponse,
    DomainAdapterResponse, EpsilonStatsResponse, ErrorResponse, LoadDomainAdapterRequest,
    TestDomainAdapterRequest, TestDomainAdapterResponse,
};
use adapteros_core::{AosError, B3Hash};
use adapteros_deterministic_exec::{DeterministicExecutor, ExecutorConfig};
use adapteros_domain::{
    error::DomainAdapterError, text::text_to_tensor, text::TextAdapter, DomainAdapter, TensorData,
};
use adapteros_trace::{Event, TraceBundle};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use chrono::Utc;
use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing::error;
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

fn manifest_path_for(adapter_id: &str) -> PathBuf {
    env::var("ADAPTEROS_DOMAIN_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("manifests/domain_adapters"))
        .join(format!("{adapter_id}.toml"))
}

fn map_domain_error(context: &str, error: DomainAdapterError) -> AosError {
    match error {
        DomainAdapterError::InvalidManifest { reason } => {
            AosError::InvalidManifest(format!("{context}: {reason}"))
        }
        DomainAdapterError::ManifestLoadError { path, source } => AosError::InvalidManifest(
            format!("{context}: failed to load manifest {}: {}", path, source),
        ),
        DomainAdapterError::UnsupportedInputFormat { format }
        | DomainAdapterError::UnsupportedOutputFormat { format } => {
            AosError::Validation(format!("{context}: unsupported format '{format}'"))
        }
        DomainAdapterError::TensorShapeMismatch { expected, actual } => {
            AosError::Validation(format!(
                "{context}: tensor shape mismatch, expected {:?}, got {:?}",
                expected, actual
            ))
        }
        DomainAdapterError::AdapterNotInitialized { adapter_name } => AosError::Internal(format!(
            "{context}: adapter '{adapter_name}' not initialized"
        )),
        DomainAdapterError::DeterminismViolation { details } => {
            AosError::DeterminismViolation(format!("{context}: {details}"))
        }
        DomainAdapterError::NumericalErrorThreshold { error, threshold } => {
            AosError::DeterminismViolation(format!(
                "{context}: numerical error {:.6} exceeded threshold {:.6}",
                error, threshold
            ))
        }
        DomainAdapterError::ModelFileNotFound { path } => {
            AosError::NotFound(format!("{context}: model file not found {path}"))
        }
        DomainAdapterError::HashVerificationFailed { expected, actual } => {
            AosError::Validation(format!(
                "{context}: hash verification failed (expected {}, actual {})",
                expected, actual
            ))
        }
        DomainAdapterError::TokenizationError { details }
        | DomainAdapterError::ImageProcessingError { details }
        | DomainAdapterError::TelemetryError { details } => {
            AosError::Internal(format!("{context}: {details}"))
        }
        DomainAdapterError::ExecutorError(e) => {
            AosError::DeterministicExecutor(format!("{context}: {e}"))
        }
        DomainAdapterError::NumericsError(e) => AosError::Internal(format!("{context}: {e}")),
        DomainAdapterError::IoError(e) => AosError::Io(format!("{context}: {e}")),
        DomainAdapterError::SerializationError(e) => AosError::Serialization(e),
        DomainAdapterError::TomlError(e) => AosError::InvalidManifest(format!("{context}: {e}")),
    }
}

fn http_error_from_aos(message: &str, error: AosError) -> (StatusCode, Json<ErrorResponse>) {
    let status = match error {
        AosError::Validation(_) => StatusCode::BAD_REQUEST,
        AosError::NotFound(_) => StatusCode::NOT_FOUND,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };

    let code = match status {
        StatusCode::BAD_REQUEST => "BAD_REQUEST",
        StatusCode::NOT_FOUND => "NOT_FOUND",
        StatusCode::UNAUTHORIZED => "UNAUTHORIZED",
        StatusCode::FORBIDDEN => "FORBIDDEN",
        StatusCode::CONFLICT => "CONFLICT",
        _ => "INTERNAL_ERROR",
    };

    let details = error.to_string();
    error!(
        target: "adapteros_server_api::domain_adapters",
        %details,
        "{message}"
    );

    (
        status,
        Json(
            ErrorResponse::new(message)
                .with_code(code)
                .with_string_details(details),
        ),
    )
}

fn summarize_event(event: &Event) -> String {
    format!(
        "{}:{}:{}",
        event.tick_id,
        event.op_id,
        event.blake3_hash.to_hex()
    )
}

fn ensure_tensor_hash(stage: &str, tensor: &TensorData) -> std::result::Result<(), AosError> {
    if tensor.verify_hash() {
        Ok(())
    } else {
        Err(AosError::Validation(format!(
            "{stage} tensor hash verification failed"
        )))
    }
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
    Json(input_data): Json<serde_json::Value>,
) -> Result<Json<DomainAdapterExecutionResponse>, (StatusCode, Json<ErrorResponse>)> {
    let input_object = match input_data.as_object() {
        Some(obj) => obj,
        None => {
            let err = AosError::Validation(
                "Input payload must be a JSON object with an 'input' or 'input_text' field"
                    .to_string(),
            );
            return Err(http_error_from_aos("Invalid domain adapter input", err));
        }
    };

    let input_text = match input_object
        .get("input")
        .or_else(|| input_object.get("input_text"))
        .and_then(Value::as_str)
    {
        Some(text) if !text.is_empty() => text.to_string(),
        _ => {
            let err = AosError::Validation(
                "Input must include non-empty string field 'input' or 'input_text'".to_string(),
            );
            return Err(http_error_from_aos("Invalid domain adapter input", err));
        }
    };

    let manifest_path = manifest_path_for(&adapter_id);
    if !manifest_path.exists() {
        let err = AosError::NotFound(format!(
            "Adapter manifest not found: {}",
            manifest_path.display()
        ));
        return Err(http_error_from_aos("Domain adapter not found", err));
    }

    let mut adapter = match TextAdapter::load(&manifest_path) {
        Ok(adapter) => adapter,
        Err(err) => {
            let aos_err = map_domain_error("Failed to load domain adapter", err);
            return Err(http_error_from_aos(
                "Failed to load domain adapter",
                aos_err,
            ));
        }
    };

    let input_tensor = match text_to_tensor(&adapter, &input_text) {
        Ok(tensor) => tensor,
        Err(err) => {
            let aos_err = map_domain_error("Failed to preprocess adapter input", err);
            return Err(http_error_from_aos(
                "Failed to preprocess adapter input",
                aos_err,
            ));
        }
    };

    if let Err(err) = ensure_tensor_hash("input", &input_tensor) {
        return Err(http_error_from_aos("Invalid adapter input", err));
    }

    let adapter_seed = B3Hash::hash(adapter_id.as_bytes());
    let mut executor_config = ExecutorConfig::default();
    executor_config.global_seed = adapter_seed.to_bytes();
    executor_config.enable_event_logging = true;

    let mut executor = DeterministicExecutor::new(executor_config);
    executor.clear_event_log();

    if let Err(err) = adapter.prepare(&mut executor) {
        let aos_err = map_domain_error("Failed to prepare adapter", err);
        return Err(http_error_from_aos(
            "Failed to prepare domain adapter",
            aos_err,
        ));
    }

    let session_id = Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string();
    let mut trace_bundle = TraceBundle::new(
        adapter_seed,
        format!("domain_adapter::{}", adapter_id),
        adapter_id.clone(),
        "default".to_string(),
        session_id.clone(),
    );
    let mut trace_events: Vec<String> = Vec::new();

    let mut prepare_inputs = HashMap::new();
    prepare_inputs.insert("stage".to_string(), Value::String("prepare".to_string()));
    prepare_inputs.insert(
        "adapter".to_string(),
        Value::String(adapter.name().to_string()),
    );
    let mut prepare_outputs = HashMap::new();
    prepare_outputs.insert(
        "status".to_string(),
        Value::String("initialized".to_string()),
    );
    let prepare_event = adapter.create_trace_event(
        executor.current_tick(),
        format!("{}::prepare", adapter.name()),
        &prepare_inputs,
        &prepare_outputs,
    );
    trace_events.push(summarize_event(&prepare_event));
    trace_bundle.add_event(prepare_event);

    let execution_timer = Instant::now();

    let forward_output = match adapter.forward(&input_tensor) {
        Ok(output) => output,
        Err(err) => {
            let aos_err = map_domain_error("Adapter forward pass failed", err);
            return Err(http_error_from_aos("Adapter forward pass failed", aos_err));
        }
    };

    if let Err(err) = ensure_tensor_hash("forward_output", &forward_output) {
        return Err(http_error_from_aos(
            "Forward tensor verification failed",
            err,
        ));
    }

    let input_hash_hex = input_tensor.metadata.hash.to_hex();
    let forward_hash_hex = forward_output.metadata.hash.to_hex();

    let mut forward_inputs = HashMap::new();
    forward_inputs.insert(
        "input_hash".to_string(),
        Value::String(input_hash_hex.clone()),
    );
    let mut forward_outputs = HashMap::new();
    forward_outputs.insert(
        "output_hash".to_string(),
        Value::String(forward_hash_hex.clone()),
    );
    let forward_event = adapter.create_trace_event(
        executor.current_tick(),
        format!("{}::forward", adapter.name()),
        &forward_inputs,
        &forward_outputs,
    );
    trace_events.push(summarize_event(&forward_event));
    trace_bundle.add_event(forward_event);

    let postprocessed_output = match adapter.postprocess(&forward_output) {
        Ok(output) => output,
        Err(err) => {
            let aos_err = map_domain_error("Adapter postprocess failed", err);
            return Err(http_error_from_aos("Adapter postprocess failed", aos_err));
        }
    };

    if let Err(err) = ensure_tensor_hash("postprocess_output", &postprocessed_output) {
        return Err(http_error_from_aos(
            "Postprocess tensor verification failed",
            err,
        ));
    }

    let output_hash_hex = postprocessed_output.metadata.hash.to_hex();
    let mut postprocess_inputs = HashMap::new();
    postprocess_inputs.insert(
        "forward_hash".to_string(),
        Value::String(forward_hash_hex.clone()),
    );
    let mut postprocess_outputs = HashMap::new();
    postprocess_outputs.insert(
        "output_hash".to_string(),
        Value::String(output_hash_hex.clone()),
    );
    postprocess_outputs.insert(
        "output_format".to_string(),
        Value::String(adapter.metadata().output_format.clone()),
    );
    let postprocess_event = adapter.create_trace_event(
        executor.current_tick(),
        format!("{}::postprocess", adapter.name()),
        &postprocess_inputs,
        &postprocess_outputs,
    );
    trace_events.push(summarize_event(&postprocess_event));
    trace_bundle.add_event(postprocess_event);

    let execution_duration = execution_timer.elapsed();
    let execution_time_ms = execution_duration.as_millis().min(u128::from(u64::MAX)) as u64;

    let epsilon = adapter
        .epsilon_stats()
        .map(|stats| stats.mean_error)
        .unwrap_or(0.0);

    if epsilon > adapter.metadata().epsilon_threshold {
        let err = AosError::DeterminismViolation(format!(
            "Observed epsilon {:.6} exceeds threshold {:.6}",
            epsilon,
            adapter.metadata().epsilon_threshold
        ));
        return Err(http_error_from_aos(
            "Adapter epsilon threshold exceeded",
            err,
        ));
    }

    for executor_event in executor.get_event_log() {
        trace_events.push(format!("executor::{executor_event:?}"));
    }

    if !trace_bundle.verify_hash() {
        error!(
            target: "adapteros_server_api::domain_adapters",
            adapter = %adapter_id,
            "Trace bundle hash verification failed"
        );
        trace_events.push("trace_bundle_hash_mismatch".to_string());
    }

    trace_events.push(format!(
        "trace_bundle_hash:{}",
        trace_bundle.bundle_hash.to_hex()
    ));

    adapter.reset();

    let execution = DomainAdapterExecutionResponse {
        execution_id: Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string(),
        adapter_id: adapter_id.clone(),
        input_hash: input_hash_hex,
        output_hash: output_hash_hex,
        epsilon,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{ApiConfig, MetricsConfig};
    use adapteros_db::Db;
    use adapteros_metrics_exporter::MetricsExporter;
    use axum::extract::{Path, State as AxumState};
    use serde_json::json;
    use std::fs;
    use std::sync::{Arc, RwLock};
    use tempfile::TempDir;

    async fn create_test_state() -> AppState {
        let db = Db::connect(":memory:")
            .await
            .expect("in-memory database should connect");
        let config = Arc::new(RwLock::new(ApiConfig {
            metrics: MetricsConfig {
                enabled: false,
                bearer_token: "test-token".to_string(),
            },
        }));
        let metrics_exporter = Arc::new(
            MetricsExporter::new(vec![0.1, 0.5, 1.0]).expect("metrics exporter should initialize"),
        );

        AppState::new(db, b"jwt-secret".to_vec(), config, metrics_exporter)
    }

    fn write_text_manifest(dir: &TempDir, adapter_name: &str) {
        let manifest_path = dir.path().join(format!("{adapter_name}.toml"));
        let manifest = format!(
            r#"[adapter]
name = "{adapter_name}"
version = "1.0.0"
model = "test-model"
hash = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
input_format = "UTF8 canonical"
output_format = "BPE deterministic"
epsilon_threshold = 0.1
deterministic = true

[adapter.parameters]
vocab_size = 128
max_sequence_length = 64
"#
        );

        fs::write(&manifest_path, manifest).expect("manifest should be written");
    }

    #[tokio::test]
    async fn execute_domain_adapter_runs_text_pipeline() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        write_text_manifest(&temp_dir, "test_adapter");
        std::env::set_var("ADAPTEROS_DOMAIN_MANIFEST_DIR", temp_dir.path());

        let state = create_test_state().await;

        let result = execute_domain_adapter(
            AxumState(state.clone()),
            Path("test_adapter".to_string()),
            Json(json!({ "input_text": "hello world" })),
        )
        .await;

        std::env::remove_var("ADAPTEROS_DOMAIN_MANIFEST_DIR");

        assert!(result.is_ok(), "execution should succeed: {result:?}");
        let Json(response) = result.unwrap();
        assert_eq!(response.adapter_id, "test_adapter");
        assert!(!response.input_hash.is_empty());
        assert!(!response.output_hash.is_empty());
        assert!(response.trace_events.len() >= 3);
        assert!(response.execution_time_ms <= u64::MAX);
        assert!(response.epsilon >= 0.0);
    }

    #[tokio::test]
    async fn execute_domain_adapter_rejects_missing_input() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        write_text_manifest(&temp_dir, "test_adapter");
        std::env::set_var("ADAPTEROS_DOMAIN_MANIFEST_DIR", temp_dir.path());

        let state = create_test_state().await;

        let result = execute_domain_adapter(
            AxumState(state.clone()),
            Path("test_adapter".to_string()),
            Json(json!({})),
        )
        .await;

        std::env::remove_var("ADAPTEROS_DOMAIN_MANIFEST_DIR");

        assert!(result.is_err());
        let (status, Json(err)) = result.err().unwrap();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(err.code, "BAD_REQUEST");
    }

    #[tokio::test]
    async fn execute_domain_adapter_reports_missing_manifest() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        std::env::set_var("ADAPTEROS_DOMAIN_MANIFEST_DIR", temp_dir.path());

        let state = create_test_state().await;

        let result = execute_domain_adapter(
            AxumState(state.clone()),
            Path("missing_adapter".to_string()),
            Json(json!({ "input": "hello" })),
        )
        .await;

        std::env::remove_var("ADAPTEROS_DOMAIN_MANIFEST_DIR");

        assert!(result.is_err());
        let (status, Json(err)) = result.err().unwrap();
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(err.code, "NOT_FOUND");
    }
}
