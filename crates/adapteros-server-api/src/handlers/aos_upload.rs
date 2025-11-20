use adapteros_core::identity::IdentityEnvelope;
use adapteros_core::{
    hash::{B3Hash, Hasher},
    AosError,
};
use adapteros_db::{
    adapters::{AdapterRegistrationBuilder, AdapterRegistrationParams},
    Db,
};
use adapteros_secure_fs::traversal::normalize_path;
use adapteros_telemetry::{EventType, LogLevel, TelemetryEventBuilder};
/// PRD-02: .aos Adapter Upload Handler
///
/// This module implements .aos file upload with specific error handling.
use axum::{
    extract::{multipart::Multipart, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{debug, error, info, warn};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    audit_helper::{actions, log_failure, log_success, resources},
    auth::Claims,
    handlers::aos_upload_error::{io_error_to_upload_error, AosUploadError},
    permissions::{require_permission, Permission},
    state::AppState,
};

/// Maximum file size for .aos uploads (1GB)
const MAX_AOS_FILE_SIZE: usize = 1024 * 1024 * 1024;

/// Maximum adapter name length
const MAX_ADAPTER_NAME_LENGTH: usize = 256;

/// Minimum and maximum rank values
const MIN_RANK: i32 = 1;
const MAX_RANK: i32 = 512;

/// Minimum and maximum alpha values
const MIN_ALPHA: f64 = 0.0;
const MAX_ALPHA: f64 = 100.0;

/// Valid tier values
const VALID_TIERS: &[&str] = &["ephemeral", "warm", "persistent"];

/// Valid category values
const VALID_CATEGORIES: &[&str] = &["general", "code", "text", "vision", "audio"];

/// Valid scope values
const VALID_SCOPES: &[&str] = &["general", "public", "private", "tenant"];

/// Response structure for successful .aos upload
#[derive(Debug, Serialize, ToSchema)]
pub struct AosUploadResponse {
    pub adapter_id: String,
    pub tenant_id: String,
    pub hash_b3: String,
    pub file_path: String,
    pub file_size: u64,
    pub lifecycle_state: String,
    pub created_at: String,
}

/// Error response structure with detailed context
#[derive(Debug, Serialize)]
pub struct AosUploadErrorResponse {
    pub error_code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

/// Convert AosUploadError to HTTP response
fn error_to_response(error: AosUploadError) -> (StatusCode, Json<AosUploadErrorResponse>) {
    let code = error.error_code().to_string();
    let message = error.to_string();
    let status = error.status_code();

    let display_message = if error.may_leak_sensitive_info() {
        match error {
            AosUploadError::PermissionDenied => {
                "Upload failed: insufficient permissions to write files".to_string()
            }
            AosUploadError::InvalidPath { .. } => "Upload failed: invalid file path".to_string(),
            AosUploadError::TemporaryFileFailed { .. } => {
                "Upload failed: temporary file operation error".to_string()
            }
            _ => message,
        }
    } else {
        message
    };

    let response = AosUploadErrorResponse {
        error_code: code,
        message: display_message,
        details: None,
    };

    (status, Json(response))
}

#[utoipa::path(
    post,
    path = "/v1/adapters/upload-aos",
    request_body(content = Multipart),
    responses(
        (status = 200, description = "Upload successful", body = AosUploadResponse),
        (status = 400, description = "Invalid request"),
        (status = 403, description = "Insufficient permissions"),
        (status = 409, description = "Conflict (adapter ID exists)"),
        (status = 413, description = "File too large"),
        (status = 507, description = "Insufficient storage"),
        (status = 500, description = "Internal error")
    ),
    tag = "adapters",
    security(("bearer" = []))
)]
pub async fn upload_aos_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, (StatusCode, Json<AosUploadErrorResponse>)> {
    let upload_start = std::time::Instant::now();

    // Check permissions
    if let Err(e) = require_permission(&claims, Permission::AdapterRegister) {
        emit_upload_permission_denied(
            &claims.sub,
            &claims.tenant_id.clone().unwrap_or_default(),
            &e.to_string(),
        );
        log_failure(
            &state.db,
            &claims,
            actions::ADAPTER_UPLOAD,
            resources::ADAPTER,
            None,
            &e.to_string(),
        )
        .await;
        return Err((
            StatusCode::FORBIDDEN,
            Json(AosUploadErrorResponse {
                error_code: "AOS_PERMISSION_DENIED".to_string(),
                message: e.to_string(),
                details: None,
            }),
        ));
    }

    let tenant_id = claims.tenant_id.clone().ok_or((
        StatusCode::BAD_REQUEST,
        Json(AosUploadErrorResponse {
            error_code: "AOS_INVALID_REQUEST".to_string(),
            message: "Missing tenant_id in claims".to_string(),
            details: None,
        }),
    ))?;

    // Process multipart form
    let mut file_data: Option<Vec<u8>> = None;
    let mut file_name: Option<String> = None;
    let mut adapter_name: Option<String> = None;
    let mut description: Option<String> = None;
    let mut tier = "ephemeral".to_string();
    let mut category = "general".to_string();
    let mut scope = "general".to_string();
    let mut rank = 1;
    let mut alpha = 1.0;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        error!("Failed to read multipart field: {}", e);
        let err = AosUploadError::InvalidRequest {
            reason: "Malformed multipart request body".to_string(),
        };
        error_to_response(err)
    })? {
        let field_name = field.name().unwrap_or("").to_string();

        match field_name.as_str() {
            "file" => {
                file_name = field.file_name().map(|s| s.to_string());
                if let Some(ref name) = file_name {
                    if !name.ends_with(".aos") {
                        let ext = name.split('.').last().unwrap_or("unknown").to_string();
                        let err = AosUploadError::InvalidExtension { extension: ext };
                        return Err(error_to_response(err));
                    }
                } else {
                    let err = AosUploadError::InvalidRequest {
                        reason: "File must have a name".to_string(),
                    };
                    return Err(error_to_response(err));
                }

                let data = field.bytes().await.map_err(|e| {
                    error!("Failed to read file data: {}", e);
                    let err = AosUploadError::InvalidRequest {
                        reason: "Unable to read file data".to_string(),
                    };
                    error_to_response(err)
                })?;

                if data.len() > MAX_AOS_FILE_SIZE {
                    let err = AosUploadError::FileTooLarge {
                        max_mb: MAX_AOS_FILE_SIZE as u64 / (1024 * 1024),
                        actual_mb: data.len() as u64 / (1024 * 1024),
                    };
                    return Err(error_to_response(err));
                }

                file_data = Some(data.to_vec());
            }
            "name" => {
                adapter_name = Some(field.text().await.map_err(|e| {
                    error!("Failed to read name field: {}", e);
                    let err = AosUploadError::InvalidRequest {
                        reason: "Invalid name field".to_string(),
                    };
                    error_to_response(err)
                })?);
            }
            "tier" => {
                tier = field.text().await.map_err(|e| {
                    error!("Failed to parse tier field: {}", e);
                    let err = AosUploadError::InvalidRequest {
                        reason: "Invalid tier field".to_string(),
                    };
                    error_to_response(err)
                })?;
                if !VALID_TIERS.contains(&tier.as_str()) {
                    warn!(tier = %tier, "Invalid tier value");
                    let err = AosUploadError::InvalidEnumValue {
                        field: "tier".to_string(),
                        value: tier.clone(),
                        valid_values: VALID_TIERS.join(", "),
                    };
                    return Err(error_to_response(err));
                }
            }
            "category" => {
                category = field.text().await.map_err(|e| {
                    error!("Failed to parse category field: {}", e);
                    let err = AosUploadError::InvalidRequest {
                        reason: "Invalid category field".to_string(),
                    };
                    error_to_response(err)
                })?;
                if !VALID_CATEGORIES.contains(&category.as_str()) {
                    warn!(category = %category, "Invalid category value");
                    let err = AosUploadError::InvalidEnumValue {
                        field: "category".to_string(),
                        value: category.clone(),
                        valid_values: VALID_CATEGORIES.join(", "),
                    };
                    return Err(error_to_response(err));
                }
            }
            "scope" => {
                scope = field.text().await.map_err(|e| {
                    error!("Failed to parse scope field: {}", e);
                    let err = AosUploadError::InvalidRequest {
                        reason: "Invalid scope field".to_string(),
                    };
                    error_to_response(err)
                })?;
                if !VALID_SCOPES.contains(&scope.as_str()) {
                    warn!(scope = %scope, "Invalid scope value");
                    let err = AosUploadError::InvalidEnumValue {
                        field: "scope".to_string(),
                        value: scope.clone(),
                        valid_values: VALID_SCOPES.join(", "),
                    };
                    return Err(error_to_response(err));
                }
            }
            "rank" => {
                rank = field
                    .text()
                    .await
                    .map_err(|e| {
                        error!("Failed to parse rank field: {}", e);
                        let err = AosUploadError::InvalidRequest {
                            reason: "Invalid rank field".to_string(),
                        };
                        error_to_response(err)
                    })?
                    .parse::<i32>()
                    .map_err(|e| {
                        error!("Failed to parse rank as integer: {}", e);
                        let err = AosUploadError::InvalidRequest {
                            reason: "Rank must be integer".to_string(),
                        };
                        error_to_response(err)
                    })?;
                if rank < MIN_RANK || rank > MAX_RANK {
                    warn!(rank = rank, "Rank value out of bounds");
                    let err = AosUploadError::InvalidRank {
                        min: MIN_RANK,
                        max: MAX_RANK,
                        actual: rank,
                    };
                    return Err(error_to_response(err));
                }
            }
            "alpha" => {
                alpha = field
                    .text()
                    .await
                    .map_err(|e| {
                        error!("Failed to parse alpha field: {}", e);
                        let err = AosUploadError::InvalidRequest {
                            reason: "Invalid alpha field".to_string(),
                        };
                        error_to_response(err)
                    })?
                    .parse::<f64>()
                    .map_err(|e| {
                        error!("Failed to parse alpha as float: {}", e);
                        let err = AosUploadError::InvalidRequest {
                            reason: "Alpha must be float".to_string(),
                        };
                        error_to_response(err)
                    })?;
                if alpha < MIN_ALPHA || alpha > MAX_ALPHA {
                    warn!(alpha = alpha, "Alpha value out of bounds");
                    let err = AosUploadError::InvalidAlpha {
                        min: MIN_ALPHA,
                        max: MAX_ALPHA,
                        actual: alpha,
                    };
                    return Err(error_to_response(err));
                }
            }
            _ => {
                warn!("Ignoring unknown field: {}", field_name);
            }
        }
    }

    let file_data = file_data.ok_or_else(|| {
        let err = AosUploadError::InvalidRequest {
            reason: "No file provided".to_string(),
        };
        error_to_response(err)
    })?;
    let adapter_name = adapter_name.unwrap_or_else(|| {
        file_name
            .clone()
            .unwrap_or_else(|| "Unnamed Adapter".to_string())
    });

    if adapter_name.len() > MAX_ADAPTER_NAME_LENGTH {
        error!(
            adapter_name_length = adapter_name.len(),
            "Adapter name exceeds max length"
        );
        let err = AosUploadError::InvalidAdapterName {
            reason: format!(
                "Name exceeds max length ({} chars max)",
                MAX_ADAPTER_NAME_LENGTH
            ),
        };
        return Err(error_to_response(err));
    }

    let adapter_id = {
        const MAX_RETRIES: usize = 3;
        for attempt in 1..=MAX_RETRIES {
            let candidate_id = format!("adapter_{}", Uuid::now_v7());
            match state.db.get_adapter(&candidate_id).await {
                Ok(_) => {
                    warn!(adapter_id = %candidate_id, attempt = attempt, "UUID collision");
                    continue;
                }
                Err(_) => {
                    info!(adapter_id = %candidate_id, attempt = attempt, "Generated unique adapter ID");
                    break candidate_id;
                }
            }
        }
        let err = AosUploadError::UniqueIdGenerationFailed {
            attempts: MAX_RETRIES,
        };
        return Err(error_to_response(err));
    };

    let mut hasher = Hasher::new();
    hasher.update(&file_data);
    let hash_b3 = hasher.finalize().to_hex().to_string();

    let adapters_dir = Path::new("./adapters");
    fs::create_dir_all(adapters_dir).await.map_err(|e| {
        error!("Failed to create adapters directory: {}", e);
        let upload_err = io_error_to_upload_error(&e, "create adapters directory");
        error_to_response(upload_err)
    })?;

    let file_path = format!("./adapters/{}.aos", adapter_id);
    let normalized_path = normalize_path(&file_path).map_err(|e| {
        error!("Invalid file path: {}", e);
        let err = AosUploadError::InvalidPath {
            reason: "Cannot normalize file path".to_string(),
        };
        error_to_response(err)
    })?;

    let temp_path = format!("./adapters/.{}.tmp", Uuid::now_v7());
    let normalized_temp_path = normalize_path(&temp_path).map_err(|e| {
        error!("Invalid temp file path: {}", e);
        let err = AosUploadError::InvalidPath {
            reason: "Cannot normalize temp path".to_string(),
        };
        error_to_response(err)
    })?;

    let mut file = fs::File::create(&normalized_temp_path).await.map_err(|e| {
        error!("Failed to create file: {}", e);
        let upload_err = io_error_to_upload_error(&e, "create temp file");
        error_to_response(upload_err)
    })?;

    file.write_all(&file_data).await.map_err(|e| {
        error!("Failed to write file data: {}", e);
        let upload_err = io_error_to_upload_error(&e, "write file data");
        error_to_response(upload_err)
    })?;

    file.flush().await.map_err(|e| {
        error!("Failed to flush file: {}", e);
        let upload_err = io_error_to_upload_error(&e, "flush file");
        error_to_response(upload_err)
    })?;

    file.sync_all().await.map_err(|e| {
        error!("Failed to sync file: {}", e);
        let upload_err = io_error_to_upload_error(&e, "sync file");
        error_to_response(upload_err)
    })?;

    drop(file);

    fs::rename(&normalized_temp_path, &normalized_path)
        .await
        .map_err(|e| {
            error!("Failed to finalize file: {}", e);
            let temp_clone = normalized_temp_path.clone();
            tokio::spawn(async move {
                let _ = fs::remove_file(&temp_clone).await;
            });
            let upload_err = io_error_to_upload_error(&e, "finalize file rename");
            error_to_response(upload_err)
        })?;

    let written_data = fs::read(&normalized_path).await.map_err(|e| {
        error!("Failed to read back written file: {}", e);
        let path_clone = normalized_path.clone();
        tokio::spawn(async move {
            if let Err(err) = fs::remove_file(&path_clone).await {
                warn!("Failed to clean up file: {}", err);
            }
        });
        let upload_err = io_error_to_upload_error(&e, "read back written file");
        error_to_response(upload_err)
    })?;

    let mut verify_hasher = Hasher::new();
    verify_hasher.update(&written_data);
    let written_hash = verify_hasher.finalize().to_hex().to_string();

    if written_hash != hash_b3 {
        error!(original_hash = %hash_b3, written_hash = %written_hash, "File corruption detected");
        let path_clone = normalized_path.clone();
        tokio::spawn(async move {
            let _ = fs::remove_file(&path_clone).await;
        });
        let err = AosUploadError::HashMismatch {
            expected: hash_b3.clone(),
            actual: written_hash,
        };
        return Err(error_to_response(err));
    }

    info!(adapter_id = %adapter_id, hash = %hash_b3, "File integrity verified");

    let file_size = file_data.len() as u64;

    let params = AdapterRegistrationBuilder::new()
        .tenant_id(&tenant_id)
        .adapter_id(&adapter_id)
        .name(&adapter_name)
        .hash_b3(&hash_b3)
        .rank(rank)
        .alpha(alpha)
        .tier(&tier)
        .category(&category)
        .scope(&scope)
        .targets_json("[]")
        .aos_file_path(Some(file_path.clone()))
        .aos_file_hash(Some(hash_b3.clone()))
        .build()
        .map_err(|e| {
            error!("Failed to build registration params: {}", e);
            let err = AosUploadError::Other {
                message: format!("Unable to prepare registration: {}", e),
            };
            error_to_response(err)
        })?;

    let id = state
        .db
        .register_adapter_with_aos(params)
        .await
        .map_err(|e| {
            error!("Database error during registration: {}", e);
            let path_clone = normalized_path.clone();
            tokio::spawn(async move {
                let _ = fs::remove_file(&path_clone).await;
            });

            let upload_err = if e.to_string().contains("UNIQUE constraint failed") {
                AosUploadError::DatabaseConstraintViolation {
                    reason: format!("Adapter ID '{}' already exists", adapter_id),
                }
            } else if e.to_string().contains("connection") || e.to_string().contains("timeout") {
                AosUploadError::DatabaseConnection {
                    reason: format!("Database connection error: {}", e),
                }
            } else {
                AosUploadError::DatabaseOperation {
                    reason: format!("Failed to register adapter: {}", e),
                }
            };
            error_to_response(upload_err)
        })?;

    log_success(
        &state.db,
        &claims,
        actions::ADAPTER_UPLOAD,
        resources::ADAPTER,
        Some(&adapter_id),
    )
    .await;

    let duration_ms = upload_start.elapsed().as_millis() as u64;
    emit_upload_success(
        &claims.sub,
        &tenant_id,
        &adapter_id,
        &hash_b3,
        file_size,
        duration_ms,
    );

    info!(adapter_id = %adapter_id, tenant_id = %tenant_id, file_size = file_size, duration_ms = duration_ms, "Successfully uploaded .aos adapter");

    Ok(Json(AosUploadResponse {
        adapter_id,
        tenant_id,
        hash_b3,
        file_path,
        file_size,
        lifecycle_state: "draft".to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    }))
}

pub async fn delete_aos_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    adapter_id: String,
) -> Result<impl IntoResponse, (StatusCode, Json<AosUploadErrorResponse>)> {
    if let Err(e) = require_permission(&claims, Permission::AdapterDelete) {
        log_failure(
            &state.db,
            &claims,
            actions::ADAPTER_DELETE,
            resources::ADAPTER,
            Some(&adapter_id),
            &e.to_string(),
        )
        .await;
        return Err((
            StatusCode::FORBIDDEN,
            Json(AosUploadErrorResponse {
                error_code: "AOS_PERMISSION_DENIED".to_string(),
                message: e.to_string(),
                details: None,
            }),
        ));
    }

    let adapter = state.db.get_adapter(&adapter_id).await.map_err(|e| {
        error!("Failed to retrieve adapter {}: {}", adapter_id, e);
        (
            StatusCode::NOT_FOUND,
            Json(AosUploadErrorResponse {
                error_code: "AOS_NOT_FOUND".to_string(),
                message: format!("Adapter '{}' not found", adapter_id),
                details: None,
            }),
        )
    })?;

    state.db.delete_adapter(&adapter_id).await.map_err(|e| {
        error!("Database error during deletion: {}", e);
        let upload_err = if e.to_string().contains("connection") {
            AosUploadError::DatabaseConnection {
                reason: format!("Failed to delete: {}", e),
            }
        } else {
            AosUploadError::DatabaseOperation {
                reason: format!("Failed to delete from database: {}", e),
            }
        };
        error_to_response(upload_err)
    })?;

    if let Some(aos_file_path) = adapter.aos_file_path {
        if let Ok(path) = normalize_path(&aos_file_path) {
            let _ = fs::remove_file(path).await;
        }
    }

    log_success(
        &state.db,
        &claims,
        actions::ADAPTER_DELETE,
        resources::ADAPTER,
        Some(&adapter_id),
    )
    .await;

    emit_delete_success(
        &claims.sub,
        &claims.tenant_id.clone().unwrap_or_default(),
        &adapter_id,
    );

    info!(adapter_id = %adapter_id, "Successfully deleted .aos adapter");

    Ok(StatusCode::NO_CONTENT)
}

/// Emit telemetry event for successful upload
fn emit_upload_success(
    user_id: &str,
    tenant_id: &str,
    adapter_id: &str,
    hash: &str,
    size_bytes: u64,
    duration_ms: u64,
) {
    let identity = IdentityEnvelope::user(user_id.to_string());
    let event = TelemetryEventBuilder::new(
        EventType::Custom("adapter.upload.success".to_string()),
        LogLevel::Info,
        format!("AOS adapter uploaded: {}", adapter_id),
        identity,
    )
    .component("adapteros-server-api".to_string())
    .user_id(user_id.to_string())
    .metadata(serde_json::json!({
        "tenant_id": tenant_id,
        "adapter_id": adapter_id,
        "hash_b3": hash,
        "size_bytes": size_bytes,
        "duration_ms": duration_ms,
        "operation": "upload",
    }))
    .build();

    if let Err(e) = event.emit_sync() {
        warn!("Failed to emit upload success telemetry: {}", e);
    }
}

/// Emit telemetry event for permission denied
fn emit_upload_permission_denied(user_id: &str, tenant_id: &str, reason: &str) {
    let identity = IdentityEnvelope::user(user_id.to_string());
    let event = TelemetryEventBuilder::new(
        EventType::Custom("adapter.upload.permission_denied".to_string()),
        LogLevel::Warn,
        format!("Upload permission denied: {}", reason),
        identity,
    )
    .component("adapteros-server-api".to_string())
    .user_id(user_id.to_string())
    .metadata(serde_json::json!({
        "tenant_id": tenant_id,
        "reason": reason,
        "operation": "upload",
    }))
    .build();

    if let Err(e) = event.emit_sync() {
        warn!("Failed to emit permission denied telemetry: {}", e);
    }
}

/// Emit telemetry event for successful deletion
fn emit_delete_success(user_id: &str, tenant_id: &str, adapter_id: &str) {
    let identity = IdentityEnvelope::user(user_id.to_string());
    let event = TelemetryEventBuilder::new(
        EventType::AdapterUnloaded,
        LogLevel::Info,
        format!("AOS adapter deleted: {}", adapter_id),
        identity,
    )
    .component("adapteros-server-api".to_string())
    .user_id(user_id.to_string())
    .metadata(serde_json::json!({
        "tenant_id": tenant_id,
        "adapter_id": adapter_id,
        "operation": "delete",
    }))
    .build();

    if let Err(e) = event.emit_sync() {
        warn!("Failed to emit delete success telemetry: {}", e);
    }
}
