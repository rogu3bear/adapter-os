//! Dataset safety and trust handlers.
//!
//! This module provides:
//! - Safety status validation for datasets
//! - Trust state management and overrides
//! - Dataset preview capabilities
//! - Training safety gate checks
//!
//! # Safety Status Values
//!
//! Individual safety signals use the following status values:
//! - `clean`: No issues detected
//! - `warn`: Potential issues detected, review recommended
//! - `block`: Critical issues detected, dataset should not be used
//! - `unknown`: Safety status has not been evaluated
//!
//! # Trust States
//!
//! Aggregate trust states for training gates:
//! - `allowed`: Dataset is safe for training
//! - `allowed_with_warning`: Dataset can be used but has warnings
//! - `blocked`: Dataset must not be used for training
//! - `needs_approval`: Dataset requires manual review before training
//! - `unknown`: Trust state has not been determined

use super::helpers::stream_preview_file;
use super::types::{
    DatasetTrustOverrideRequest, TrustOverrideRequest, TrustOverrideResponse,
    UpdateDatasetSafetyRequest, UpdateDatasetSafetyResponse,
};
use crate::auth::Claims;
use crate::error_helpers::{bad_request, db_error, forbidden, not_found};
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::ErrorResponse;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{info, warn};
use utoipa::ToSchema;

// ============================================================================
// Safety Status Types and Constants
// ============================================================================

/// Valid safety status values for individual signals (PII, toxicity, leak, anomaly).
pub const VALID_SAFETY_STATUSES: &[&str] = &["clean", "warn", "block", "unknown"];

/// Valid trust state values for aggregate dataset trust.
pub const VALID_TRUST_STATES: &[&str] = &[
    "allowed",
    "allowed_with_warning",
    "blocked",
    "needs_approval",
    "unknown",
];

/// Trust states that permit training to proceed.
pub const SAFE_TRUST_STATES: &[&str] = &["allowed", "allowed_with_warning"];

/// Trust states that block training.
pub const BLOCKED_TRUST_STATES: &[&str] = &["blocked", "needs_approval", "unknown"];

/// Result of validating dataset safety status.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SafetyStatusValidationResult {
    /// Whether the safety status is valid.
    pub is_valid: bool,
    /// Validation errors, if any.
    pub errors: Vec<String>,
    /// The validated status value (normalized to lowercase).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normalized_status: Option<String>,
}

/// Result of checking if a dataset is safe for training.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DatasetSafetyCheckResult {
    /// Whether the dataset is safe for training.
    pub is_safe: bool,
    /// The effective trust state.
    pub trust_state: String,
    /// Individual safety signals.
    pub safety_signals: SafetySignals,
    /// Reasons why the dataset is not safe (if applicable).
    pub blocking_reasons: Vec<String>,
    /// Warnings that don't block training but should be noted.
    pub warnings: Vec<String>,
}

/// Individual safety signal statuses.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default)]
pub struct SafetySignals {
    /// PII (Personally Identifiable Information) detection status.
    pub pii_status: String,
    /// Toxicity detection status.
    pub toxicity_status: String,
    /// Data leak detection status.
    pub leak_status: String,
    /// Anomaly detection status.
    pub anomaly_status: String,
    /// Overall aggregated safety status.
    pub overall_safety: String,
}

// ============================================================================
// Safety Status Validation Functions
// ============================================================================

/// Validate a single safety status value.
///
/// Returns `Ok(normalized_value)` if valid, `Err(error_message)` if invalid.
pub fn validate_safety_status(status: &str) -> Result<String, String> {
    let normalized = status.to_ascii_lowercase();
    if VALID_SAFETY_STATUSES.contains(&normalized.as_str()) {
        Ok(normalized)
    } else {
        Err(format!(
            "Invalid safety status '{}'. Valid values: {}",
            status,
            VALID_SAFETY_STATUSES.join(", ")
        ))
    }
}

/// Validate a trust state value.
///
/// Returns `Ok(normalized_value)` if valid, `Err(error_message)` if invalid.
pub fn validate_trust_state(state: &str) -> Result<String, String> {
    let normalized = state.to_ascii_lowercase();
    if VALID_TRUST_STATES.contains(&normalized.as_str()) {
        Ok(normalized)
    } else {
        Err(format!(
            "Invalid trust state '{}'. Valid values: {}",
            state,
            VALID_TRUST_STATES.join(", ")
        ))
    }
}

/// Validate the safety status request, checking all provided status values.
pub fn validate_safety_request(
    request: &UpdateDatasetSafetyRequest,
) -> SafetyStatusValidationResult {
    let mut errors = Vec::new();
    let mut all_valid = true;

    // Validate PII status
    if let Some(ref status) = request.pii_status {
        if let Err(e) = validate_safety_status(status) {
            errors.push(format!("pii_status: {}", e));
            all_valid = false;
        }
    }

    // Validate toxicity status
    if let Some(ref status) = request.toxicity_status {
        if let Err(e) = validate_safety_status(status) {
            errors.push(format!("toxicity_status: {}", e));
            all_valid = false;
        }
    }

    // Validate leak status
    if let Some(ref status) = request.leak_status {
        if let Err(e) = validate_safety_status(status) {
            errors.push(format!("leak_status: {}", e));
            all_valid = false;
        }
    }

    // Validate anomaly status
    if let Some(ref status) = request.anomaly_status {
        if let Err(e) = validate_safety_status(status) {
            errors.push(format!("anomaly_status: {}", e));
            all_valid = false;
        }
    }

    SafetyStatusValidationResult {
        is_valid: all_valid,
        errors,
        normalized_status: None,
    }
}

/// Check if a trust state indicates the dataset is safe for training.
pub fn is_trust_state_safe(trust_state: &str) -> bool {
    let normalized = trust_state.to_ascii_lowercase();
    SAFE_TRUST_STATES.contains(&normalized.as_str())
}

/// Check if a trust state blocks training.
pub fn is_trust_state_blocked(trust_state: &str) -> bool {
    let normalized = trust_state.to_ascii_lowercase();
    BLOCKED_TRUST_STATES.contains(&normalized.as_str())
}

/// Derive the overall safety status from individual signals.
///
/// Priority: block > warn > unknown > clean
pub fn derive_overall_safety(
    pii_status: &str,
    toxicity_status: &str,
    leak_status: &str,
    anomaly_status: &str,
) -> String {
    let statuses = [pii_status, toxicity_status, leak_status, anomaly_status];

    // If any signal is "block", overall is "block"
    if statuses
        .iter()
        .any(|s| s.eq_ignore_ascii_case("block") || s.eq_ignore_ascii_case("unsafe"))
    {
        return "block".to_string();
    }

    // If any signal is "warn", overall is "warn"
    if statuses.iter().any(|s| s.eq_ignore_ascii_case("warn")) {
        return "warn".to_string();
    }

    // If all signals are "unknown", overall is "unknown"
    if statuses.iter().all(|s| s.eq_ignore_ascii_case("unknown")) {
        return "unknown".to_string();
    }

    // Otherwise, all signals are clean
    "clean".to_string()
}

/// Evaluate dataset safety for training.
///
/// This function checks whether a dataset is safe to use for training based on
/// its trust state and individual safety signals.
pub fn evaluate_dataset_safety(
    trust_state: &str,
    pii_status: &str,
    toxicity_status: &str,
    leak_status: &str,
    anomaly_status: &str,
) -> DatasetSafetyCheckResult {
    let mut blocking_reasons = Vec::new();
    let mut warnings = Vec::new();

    let trust_lower = trust_state.to_ascii_lowercase();
    let is_safe = is_trust_state_safe(&trust_lower);

    // Check trust state
    match trust_lower.as_str() {
        "blocked" => blocking_reasons.push("Dataset is explicitly blocked".to_string()),
        "needs_approval" => {
            blocking_reasons.push("Dataset requires approval before training".to_string())
        }
        "unknown" => blocking_reasons.push("Dataset trust state is unknown".to_string()),
        "allowed_with_warning" => warnings.push("Dataset has warnings, review recommended".to_string()),
        _ => {}
    }

    // Check individual safety signals
    let check_signal = |status: &str, signal_name: &str| -> Option<(bool, String)> {
        match status.to_ascii_lowercase().as_str() {
            "block" | "unsafe" => Some((
                true,
                format!("{} detected blocking issues", signal_name),
            )),
            "warn" => Some((
                false,
                format!("{} detected potential issues", signal_name),
            )),
            "unknown" => Some((
                false,
                format!("{} status is unknown", signal_name),
            )),
            _ => None,
        }
    };

    if let Some((is_block, msg)) = check_signal(pii_status, "PII detection") {
        if is_block {
            blocking_reasons.push(msg);
        } else {
            warnings.push(msg);
        }
    }

    if let Some((is_block, msg)) = check_signal(toxicity_status, "Toxicity detection") {
        if is_block {
            blocking_reasons.push(msg);
        } else {
            warnings.push(msg);
        }
    }

    if let Some((is_block, msg)) = check_signal(leak_status, "Data leak detection") {
        if is_block {
            blocking_reasons.push(msg);
        } else {
            warnings.push(msg);
        }
    }

    if let Some((is_block, msg)) = check_signal(anomaly_status, "Anomaly detection") {
        if is_block {
            blocking_reasons.push(msg);
        } else {
            warnings.push(msg);
        }
    }

    let overall_safety = derive_overall_safety(pii_status, toxicity_status, leak_status, anomaly_status);

    DatasetSafetyCheckResult {
        is_safe,
        trust_state: trust_state.to_string(),
        safety_signals: SafetySignals {
            pii_status: pii_status.to_string(),
            toxicity_status: toxicity_status.to_string(),
            leak_status: leak_status.to_string(),
            anomaly_status: anomaly_status.to_string(),
            overall_safety,
        },
        blocking_reasons,
        warnings,
    }
}

/// Update semantic/safety statuses for a dataset version (Tier 2).
#[utoipa::path(
    post,
    path = "/v1/datasets/{dataset_id}/safety",
    params(("dataset_id" = String, Path, description = "Dataset ID")),
    request_body = UpdateDatasetSafetyRequest,
    responses(
        (status = 200, description = "Safety statuses updated", body = UpdateDatasetSafetyResponse),
        (status = 404, description = "Dataset not found"),
        (status = 403, description = "Tenant isolation violation"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn update_dataset_safety(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dataset_id): Path<String>,
    Json(body): Json<UpdateDatasetSafetyRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetValidate)?;

    // Validate the safety status request before processing
    let validation_result = validate_safety_request(&body);
    if !validation_result.is_valid {
        return Err(bad_request(format!(
            "Invalid safety status values: {}",
            validation_result.errors.join("; ")
        )));
    }

    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| not_found("Dataset"))?;

    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    }

    let version_id = state
        .db
        .ensure_dataset_version_exists(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to ensure dataset version: {}", e)))?;

    // Compute overall safety using the centralized function
    let overall_safety = derive_overall_safety(
        body.pii_status.as_deref().unwrap_or("unknown"),
        body.toxicity_status.as_deref().unwrap_or("unknown"),
        body.leak_status.as_deref().unwrap_or("unknown"),
        body.anomaly_status.as_deref().unwrap_or("unknown"),
    );

    let trust_state = state
        .db
        .update_dataset_version_safety_status(
            &version_id,
            body.pii_status.as_deref(),
            body.toxicity_status.as_deref(),
            body.leak_status.as_deref(),
            body.anomaly_status.as_deref(),
        )
        .await
        .map_err(|e| db_error(format!("Failed to update safety status: {}", e)))?;

    let _ = state
        .db
        .record_dataset_version_validation_run(
            &version_id,
            "tier2_safety",
            &overall_safety,
            None,
            None,
            None,
            Some(claims.sub.as_str()),
        )
        .await;

    info!(
        dataset_id = %dataset_id,
        version_id = %version_id,
        trust_state = %trust_state,
        overall_safety = %overall_safety,
        actor = %claims.sub,
        "Updated dataset safety status"
    );

    Ok(Json(UpdateDatasetSafetyResponse {
        dataset_id,
        dataset_version_id: version_id,
        trust_state,
        overall_safety_status: overall_safety,
    }))
}

/// Admin override for dataset trust_state.
#[utoipa::path(
    post,
    path = "/v1/datasets/{dataset_id}/trust_override",
    params(("dataset_id" = String, Path, description = "Dataset ID")),
    request_body = TrustOverrideRequest,
    responses(
        (status = 200, description = "Trust override applied", body = TrustOverrideResponse),
        (status = 404, description = "Dataset not found"),
        (status = 403, description = "Tenant isolation violation"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn override_dataset_trust(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dataset_id): Path<String>,
    Json(body): Json<TrustOverrideRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetValidate)?;

    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| not_found("Dataset"))?;

    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    }

    let version_id = state
        .db
        .ensure_dataset_version_exists(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to ensure dataset version: {}", e)))?;

    state
        .db
        .create_dataset_version_override(
            &version_id,
            &body.trust_state,
            body.reason.as_deref(),
            &claims.sub,
        )
        .await
        .map_err(|e| db_error(format!("Failed to create trust override: {}", e)))?;

    Ok(Json(TrustOverrideResponse {
        dataset_id,
        dataset_version_id: version_id,
        trust_state: body.trust_state,
    }))
}

/// Get a preview of dataset contents
#[utoipa::path(
    get,
    path = "/v1/datasets/{dataset_id}/preview",
    params(
        ("dataset_id" = String, Path, description = "Dataset ID"),
        ("limit" = Option<i32>, Query, description = "Number of examples to preview")
    ),
    responses(
        (status = 200, description = "Dataset preview", body = serde_json::Value),
        (status = 404, description = "Dataset not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn preview_dataset(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dataset_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetView)?;

    let limit = params
        .get("limit")
        .and_then(|l| l.parse::<usize>().ok())
        .unwrap_or(10)
        .min(100);

    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| not_found("Dataset"))?;

    // CRITICAL: Validate tenant isolation - non-admin users can only preview their own tenant's datasets
    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        // Datasets without tenant_id can only be previewed by admins
        return Err(forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    let files = state
        .db
        .get_dataset_files(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to get dataset files: {}", e)))?;

    let mut examples = Vec::new();
    let mut count = 0;

    // Stream read files for memory efficiency
    for file in files {
        if count >= limit {
            break;
        }

        match stream_preview_file(
            std::path::Path::new(&file.file_path),
            &dataset.format,
            limit - count,
        )
        .await
        {
            Ok(mut file_examples) => {
                count += file_examples.len();
                examples.append(&mut file_examples);
            }
            Err(e) => {
                warn!("Failed to preview file {}: {}", file.file_name, e);
                continue;
            }
        }
    }

    Ok(Json(serde_json::json!({
        "dataset_id": dataset_id,
        "format": dataset.format,
        "total_examples": examples.len(),
        "examples": examples
    })))
}

/// Apply a trust override to the latest dataset version
#[utoipa::path(
    post,
    path = "/v1/datasets/{dataset_id}/trust_override",
    request_body = DatasetTrustOverrideRequest,
    responses(
        (status = 200, description = "Trust override applied"),
        (status = 400, description = "Invalid override"),
        (status = 404, description = "Dataset not found"),
    ),
    tag = "datasets"
)]
pub async fn apply_dataset_trust_override(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dataset_id): Path<String>,
    Json(payload): Json<DatasetTrustOverrideRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetValidate)?;

    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to load dataset: {}", e)))?
        .ok_or_else(|| not_found("Dataset"))?;

    // Tenant isolation
    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    }

    let allowed_states = [
        "allowed",
        "allowed_with_warning",
        "blocked",
        "needs_approval",
    ];
    if !allowed_states
        .iter()
        .any(|s| s.eq_ignore_ascii_case(payload.override_state.as_str()))
    {
        return Err(bad_request("Invalid override_state"));
    }

    let version_id = state
        .db
        .ensure_dataset_version_exists(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to ensure dataset version: {}", e)))?;

    state
        .db
        .create_dataset_version_override(
            &version_id,
            payload.override_state.as_str(),
            payload.reason.as_deref(),
            &claims.sub,
        )
        .await
        .map_err(|e| db_error(format!("Failed to create override: {}", e)))?;

    let effective = state
        .db
        .get_effective_trust_state(&version_id)
        .await
        .map_err(|e| db_error(format!("Failed to read effective trust_state: {}", e)))?
        .unwrap_or_else(|| "unknown".to_string());

    Ok(Json(serde_json::json!({
        "dataset_id": dataset_id,
        "dataset_version_id": version_id,
        "effective_trust_state": effective,
    })))
}

/// Apply trust override to a specific dataset version
#[utoipa::path(
    post,
    path = "/v1/datasets/{dataset_id}/versions/{version_id}/trust-override",
    params(
        ("dataset_id" = String, Path, description = "Dataset ID"),
        ("version_id" = String, Path, description = "Dataset version ID")
    ),
    request_body = DatasetTrustOverrideRequest,
    responses(
        (status = 200, description = "Trust override applied successfully"),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Dataset or version not found"),
        (status = 400, description = "Invalid override state"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn apply_dataset_version_trust_override(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((dataset_id, version_id)): Path<(String, String)>,
    Json(payload): Json<DatasetTrustOverrideRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetValidate)?;

    // Validate dataset exists and enforce tenant isolation
    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to load dataset: {}", e)))?
        .ok_or_else(|| not_found("Dataset"))?;

    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    }

    // Validate version exists and belongs to the dataset
    let version = state
        .db
        .get_training_dataset_version(&version_id)
        .await
        .map_err(|e| db_error(format!("Failed to load dataset version: {}", e)))?
        .ok_or_else(|| not_found("Dataset version"))?;

    if version.dataset_id != dataset_id {
        return Err(bad_request(
            "Version does not belong to the specified dataset",
        ));
    }

    // Enforce tenant isolation on version
    if let Some(ref version_tenant_id) = version.tenant_id {
        validate_tenant_isolation(&claims, version_tenant_id)?;
    }

    // Validate override state
    let allowed_states = [
        "allowed",
        "allowed_with_warning",
        "blocked",
        "needs_approval",
    ];
    if !allowed_states
        .iter()
        .any(|s| s.eq_ignore_ascii_case(payload.override_state.as_str()))
    {
        return Err(bad_request(
            "Invalid override_state. Must be one of: allowed, allowed_with_warning, blocked, needs_approval",
        ));
    }

    // Create the override (this automatically propagates trust changes via DB triggers)
    state
        .db
        .create_dataset_version_override(
            &version_id,
            payload.override_state.as_str(),
            payload.reason.as_deref(),
            &claims.sub,
        )
        .await
        .map_err(|e| db_error(format!("Failed to create override: {}", e)))?;

    // Get the effective trust state after override
    let effective = state
        .db
        .get_effective_trust_state(&version_id)
        .await
        .map_err(|e| db_error(format!("Failed to read effective trust_state: {}", e)))?
        .unwrap_or_else(|| "unknown".to_string());

    info!(
        dataset_id = %dataset_id,
        version_id = %version_id,
        override_state = %payload.override_state,
        effective_state = %effective,
        actor = %claims.sub,
        "Applied dataset version trust override"
    );

    Ok(Json(serde_json::json!({
        "dataset_id": dataset_id,
        "dataset_version_id": version_id,
        "override_state": payload.override_state,
        "effective_trust_state": effective,
        "reason": payload.reason,
    })))
}

/// Update safety signals for a specific dataset version
#[utoipa::path(
    post,
    path = "/v1/datasets/{dataset_id}/versions/{version_id}/safety",
    params(
        ("dataset_id" = String, Path, description = "Dataset ID"),
        ("version_id" = String, Path, description = "Dataset version ID")
    ),
    request_body = UpdateDatasetSafetyRequest,
    responses(
        (status = 200, description = "Safety status updated successfully", body = UpdateDatasetSafetyResponse),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Dataset or version not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn update_dataset_version_safety(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((dataset_id, version_id)): Path<(String, String)>,
    Json(body): Json<UpdateDatasetSafetyRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetValidate)?;

    // Validate the safety status request before processing
    let validation_result = validate_safety_request(&body);
    if !validation_result.is_valid {
        return Err(bad_request(format!(
            "Invalid safety status values: {}",
            validation_result.errors.join("; ")
        )));
    }

    // Validate dataset exists and enforce tenant isolation
    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| not_found("Dataset"))?;

    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    }

    // Validate version exists and belongs to the dataset
    let version = state
        .db
        .get_training_dataset_version(&version_id)
        .await
        .map_err(|e| db_error(format!("Failed to load dataset version: {}", e)))?
        .ok_or_else(|| not_found("Dataset version"))?;

    if version.dataset_id != dataset_id {
        return Err(bad_request(
            "Version does not belong to the specified dataset",
        ));
    }

    // Enforce tenant isolation on version
    if let Some(ref version_tenant_id) = version.tenant_id {
        validate_tenant_isolation(&claims, version_tenant_id)?;
    }

    // Compute overall safety using the centralized function
    let overall_safety = derive_overall_safety(
        body.pii_status.as_deref().unwrap_or("unknown"),
        body.toxicity_status.as_deref().unwrap_or("unknown"),
        body.leak_status.as_deref().unwrap_or("unknown"),
        body.anomaly_status.as_deref().unwrap_or("unknown"),
    );

    // Update safety status (this automatically propagates trust changes via DB layer)
    let trust_state = state
        .db
        .update_dataset_version_safety_status(
            &version_id,
            body.pii_status.as_deref(),
            body.toxicity_status.as_deref(),
            body.leak_status.as_deref(),
            body.anomaly_status.as_deref(),
        )
        .await
        .map_err(|e| db_error(format!("Failed to update safety status: {}", e)))?;

    // Record validation run for audit trail
    let _ = state
        .db
        .record_dataset_version_validation_run(
            &version_id,
            "tier2_safety",
            &overall_safety,
            None,
            None,
            None,
            Some(claims.sub.as_str()),
        )
        .await;

    info!(
        dataset_id = %dataset_id,
        version_id = %version_id,
        trust_state = %trust_state,
        overall_safety = %overall_safety,
        actor = %claims.sub,
        "Updated dataset version safety status"
    );

    Ok(Json(UpdateDatasetSafetyResponse {
        dataset_id,
        dataset_version_id: version_id,
        trust_state,
        overall_safety_status: overall_safety,
    }))
}
