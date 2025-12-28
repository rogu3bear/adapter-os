//! Dataset validation handlers.

use super::helpers::{
    map_validation_status, spawn_tier2_safety_validation, validate_file_hash_streaming,
    STREAM_BUFFER_SIZE,
};
use super::progress::emit_progress;
use crate::auth::Claims;
use crate::error_helpers::{db_error, forbidden, not_found};
use crate::handlers::chunked_upload::FileValidator;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::{ErrorResponse, ValidateDatasetRequest, ValidateDatasetResponse};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};

/// Validate a dataset
#[utoipa::path(
    post,
    path = "/v1/datasets/{dataset_id}/validate",
    params(
        ("dataset_id" = String, Path, description = "Dataset ID")
    ),
    request_body = ValidateDatasetRequest,
    responses(
        (status = 200, description = "Validation result", body = ValidateDatasetResponse),
        (status = 404, description = "Dataset not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn validate_dataset(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dataset_id): Path<String>,
    Json(request): Json<ValidateDatasetRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetValidate)?;

    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| not_found("Dataset"))?;

    // CRITICAL: Validate tenant isolation - non-admin users can only validate their own tenant's datasets
    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        // Datasets without tenant_id can only be validated by admins
        return Err(forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    // Set status to 'validating' at start
    state
        .db
        .update_dataset_validation(&dataset_id, "validating", None)
        .await
        .map_err(|e| db_error(format!("Failed to update validation status: {}", e)))?;

    // Send initial validation event
    emit_progress(
        state.dataset_progress_tx.as_ref(),
        &dataset_id,
        "validation",
        None,
        0.0,
        "Starting dataset validation...".to_string(),
        Some(dataset.file_count),
        Some(0),
    );

    // Get dataset files
    let files = state
        .db
        .get_dataset_files(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to get dataset files: {}", e)))?;

    let mut validation_errors = Vec::new();
    let mut is_valid = true;
    let total_files = files.len() as f32;
    let mut processed_files = 0;

    // Validate each file
    for file in &files {
        // Check file exists
        if !tokio::fs::try_exists(&file.file_path)
            .await
            .unwrap_or(false)
        {
            validation_errors.push(format!(
                "File {} does not exist at path {}",
                file.file_name, file.file_path
            ));
            is_valid = false;
            processed_files += 1;
            emit_progress(
                state.dataset_progress_tx.as_ref(),
                &dataset_id,
                "validation",
                Some(file.file_name.clone()),
                if total_files > 0.0 {
                    (processed_files as f32 / total_files) * 100.0
                } else {
                    0.0
                },
                format!("Validating {}", file.file_name),
                Some(files.len() as i32),
                Some(processed_files),
            );
            continue;
        }

        // Verify file hash with streaming to avoid loading entire file
        match validate_file_hash_streaming(std::path::Path::new(&file.file_path), &file.hash_b3)
            .await
        {
            Ok(matches) => {
                if !matches {
                    validation_errors.push(format!("File {} hash mismatch", file.file_name));
                    is_valid = false;
                }
            }
            Err(e) => {
                validation_errors
                    .push(format!("Failed to validate file {}: {}", file.file_name, e));
                is_valid = false;
                continue;
            }
        }

        // Format-specific validation with quick checks
        if request.check_format.unwrap_or(true) {
            if let Err(e) = FileValidator::quick_validate(
                std::path::Path::new(&file.file_path),
                &dataset.format,
                STREAM_BUFFER_SIZE,
            )
            .await
            {
                validation_errors.push(format!(
                    "File {} format validation failed: {}",
                    file.file_name, e
                ));
                is_valid = false;
            }
        }

        processed_files += 1;

        // Send progress event for this file
        emit_progress(
            state.dataset_progress_tx.as_ref(),
            &dataset_id,
            "validation",
            Some(file.file_name.clone()),
            if total_files > 0.0 {
                (processed_files as f32 / total_files) * 100.0
            } else {
                0.0
            },
            format!("Validated {}", file.file_name),
            Some(files.len() as i32),
            Some(processed_files),
        );
    }

    // Update validation status in database - set to "invalid" if validation failed
    let validation_status = if is_valid { "valid" } else { "invalid" };
    let validation_errors_str = if validation_errors.is_empty() {
        None
    } else {
        Some(validation_errors.join("; "))
    };

    state
        .db
        .update_dataset_validation(
            &dataset_id,
            validation_status,
            validation_errors_str.as_deref(),
        )
        .await
        .map_err(|e| {
            // On database error, try to reset status to 'invalid' to prevent stuck 'validating' state
            let db_clone = state.db.clone();
            let dataset_id_clone = dataset_id.clone();
            tokio::spawn(async move {
                let _ = db_clone
                    .update_dataset_validation(
                        &dataset_id_clone,
                        "invalid",
                        Some("Validation failed due to internal error"),
                    )
                    .await;
            });
            crate::error_helpers::internal_error(format!(
                "Failed to update validation status: {}",
                e
            ))
        })?;

    // Mirror structural validation into dataset version trust pipeline
    if let Ok(version_id) = state.db.ensure_dataset_version_exists(&dataset_id).await {
        let _ = state
            .db
            .update_dataset_version_structural_validation(
                &version_id,
                validation_status,
                validation_errors_str.as_deref(),
            )
            .await;
        // Kick off tier2 safety validation asynchronously (stub pipeline)
        spawn_tier2_safety_validation(state.clone(), version_id.clone(), claims.sub.clone());
        let _ = state
            .db
            .record_dataset_version_validation_run(
                &version_id,
                "tier1_structural",
                if is_valid { "valid" } else { "invalid" },
                Some("structural"),
                validation_errors_str.as_deref(),
                None,
                Some(claims.sub.as_str()),
            )
            .await;
    }

    Ok(Json(ValidateDatasetResponse {
        schema_version: "1.0".to_string(),
        dataset_id,
        is_valid,
        validation_status: map_validation_status(validation_status),
        errors: if validation_errors.is_empty() {
            None
        } else {
            Some(validation_errors)
        },
        validated_at: chrono::Utc::now().to_rfc3339(),
    }))
}
