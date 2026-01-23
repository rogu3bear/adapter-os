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

use super::helpers::{ensure_dataset_file_within_root, stream_preview_file};
use super::types::{
    DatasetTrustOverrideRequest, TrustOverrideRequest, TrustOverrideResponse,
    UpdateDatasetSafetyRequest, UpdateDatasetSafetyResponse,
};
use crate::api_error::ApiError;
use crate::audit_helper;
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{info, warn};
use utoipa::{IntoParams, ToSchema};

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

/// Default maximum synthetic data ratio (50%).
pub const DEFAULT_SYNTHETIC_RATIO_CAP: f64 = 0.5;

/// Warning threshold for synthetic data ratio (approaching cap).
pub const SYNTHETIC_RATIO_WARNING_THRESHOLD: f64 = 0.4;

// ============================================================================
// Synthetic Ratio Validation Types
// ============================================================================

/// Configuration for synthetic data ratio validation.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SyntheticRatioConfig {
    /// Maximum allowed ratio of synthetic data (0.0-1.0).
    pub ratio_cap: f64,
    /// Whether the guardrail is enabled.
    pub enabled: bool,
    /// Whether to allow override (with explicit acknowledgment).
    pub allow_override: bool,
}

impl Default for SyntheticRatioConfig {
    fn default() -> Self {
        Self {
            ratio_cap: DEFAULT_SYNTHETIC_RATIO_CAP,
            enabled: true,
            allow_override: true,
        }
    }
}

/// Result of synthetic ratio validation.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SyntheticRatioValidationResult {
    /// The computed synthetic ratio (0.0-1.0).
    pub ratio: f64,
    /// The configured cap.
    pub cap: f64,
    /// Safety status: clean, warn, or block.
    pub status: String,
    /// Whether an override was requested.
    pub override_requested: bool,
    /// Whether the override was accepted (allows exceeding cap with warning).
    pub override_accepted: bool,
    /// Human-readable message.
    pub message: String,
}

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
    /// Synthetic data ratio status (clean/warn/block/unknown).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synthetic_ratio_status: Option<String>,
    /// Actual synthetic data ratio (0.0-1.0) if computed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synthetic_ratio: Option<f64>,
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

#[derive(Debug, Clone)]
struct NormalizedSafetyStatuses {
    pii_status: Option<String>,
    toxicity_status: Option<String>,
    leak_status: Option<String>,
    anomaly_status: Option<String>,
}

fn normalize_optional_safety_status(
    status: Option<&str>,
    field: &str,
    errors: &mut Vec<String>,
) -> Option<String> {
    match status {
        Some(raw) => match validate_safety_status(raw) {
            Ok(normalized) => Some(normalized),
            Err(e) => {
                errors.push(format!("{}: {}", field, e));
                None
            }
        },
        None => None,
    }
}

fn normalize_safety_statuses(
    request: &UpdateDatasetSafetyRequest,
) -> Result<NormalizedSafetyStatuses, Vec<String>> {
    let mut errors = Vec::new();
    let pii_status =
        normalize_optional_safety_status(request.pii_status.as_deref(), "pii_status", &mut errors);
    let toxicity_status = normalize_optional_safety_status(
        request.toxicity_status.as_deref(),
        "toxicity_status",
        &mut errors,
    );
    let leak_status = normalize_optional_safety_status(
        request.leak_status.as_deref(),
        "leak_status",
        &mut errors,
    );
    let anomaly_status = normalize_optional_safety_status(
        request.anomaly_status.as_deref(),
        "anomaly_status",
        &mut errors,
    );

    if errors.is_empty() {
        Ok(NormalizedSafetyStatuses {
            pii_status,
            toxicity_status,
            leak_status,
            anomaly_status,
        })
    } else {
        Err(errors)
    }
}

fn map_safety_status_to_validation(status: &str) -> &'static str {
    match status {
        "clean" => "valid",
        "warn" => "warn",
        "block" => "block",
        "unknown" => "pending",
        _ => "pending",
    }
}

async fn record_safety_status_updates(
    state: &AppState,
    dataset_version_id: &str,
    actor: &str,
    statuses: &NormalizedSafetyStatuses,
) {
    let signals = [
        ("pii", statuses.pii_status.as_deref()),
        ("toxicity", statuses.toxicity_status.as_deref()),
        ("leak", statuses.leak_status.as_deref()),
        ("anomaly", statuses.anomaly_status.as_deref()),
    ];

    for (signal, status) in signals {
        let Some(status) = status else {
            continue;
        };
        let validation_status = map_safety_status_to_validation(status);
        let _ = state
            .db
            .record_dataset_version_validation_run(
                dataset_version_id,
                "tier2_safety",
                validation_status,
                Some(signal),
                None,
                None,
                Some(actor),
                None,
                None,
                None,
            )
            .await;
    }
}

fn normalize_override_state(field: &str, raw: &str) -> Result<String, String> {
    let normalized = validate_trust_state(raw)?;
    if normalized == "unknown" {
        return Err(format!(
            "{}: Invalid trust state '{}'. Valid values: allowed, allowed_with_warning, blocked, needs_approval",
            field, raw
        ));
    }
    Ok(normalized)
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

// ============================================================================
// Synthetic Ratio Validation Functions
// ============================================================================

/// Validate the synthetic data ratio against configured guardrails.
///
/// # Arguments
/// * `synthetic_count` - Number of synthetic/generated samples
/// * `total_count` - Total number of samples in the dataset
/// * `config` - Synthetic ratio configuration (cap, enabled, allow_override)
/// * `override_requested` - Whether the user explicitly requested to override the cap
///
/// # Returns
/// A `SyntheticRatioValidationResult` indicating whether the ratio is acceptable.
///
/// # Behavior
/// - If guardrails are disabled: returns "clean" status
/// - If ratio <= warning threshold: returns "clean"
/// - If ratio > warning threshold but <= cap: returns "warn" (approaching limit)
/// - If ratio > cap without override: returns "block"
/// - If ratio > cap with override accepted: returns "warn" with acknowledgment
pub fn validate_synthetic_ratio(
    synthetic_count: u64,
    total_count: u64,
    config: &SyntheticRatioConfig,
    override_requested: bool,
) -> SyntheticRatioValidationResult {
    // Handle edge case: empty dataset
    if total_count == 0 {
        return SyntheticRatioValidationResult {
            ratio: 0.0,
            cap: config.ratio_cap,
            status: "clean".to_string(),
            override_requested,
            override_accepted: false,
            message: "Dataset is empty, synthetic ratio check skipped".to_string(),
        };
    }

    let ratio = synthetic_count as f64 / total_count as f64;

    // If guardrails are disabled, always clean
    if !config.enabled {
        return SyntheticRatioValidationResult {
            ratio,
            cap: config.ratio_cap,
            status: "clean".to_string(),
            override_requested,
            override_accepted: false,
            message: "Synthetic ratio guardrails are disabled".to_string(),
        };
    }

    // Check ratio against thresholds
    if ratio <= SYNTHETIC_RATIO_WARNING_THRESHOLD {
        // Below warning threshold - clean
        SyntheticRatioValidationResult {
            ratio,
            cap: config.ratio_cap,
            status: "clean".to_string(),
            override_requested,
            override_accepted: false,
            message: format!(
                "Synthetic ratio {:.1}% is within safe limits (cap: {:.1}%)",
                ratio * 100.0,
                config.ratio_cap * 100.0
            ),
        }
    } else if ratio <= config.ratio_cap {
        // Between warning threshold and cap - warn (approaching limit)
        SyntheticRatioValidationResult {
            ratio,
            cap: config.ratio_cap,
            status: "warn".to_string(),
            override_requested,
            override_accepted: false,
            message: format!(
                "Synthetic ratio {:.1}% is approaching the cap of {:.1}%",
                ratio * 100.0,
                config.ratio_cap * 100.0
            ),
        }
    } else if override_requested && config.allow_override {
        // Exceeds cap but override requested and allowed - warn with acceptance
        warn!(
            synthetic_ratio = ratio,
            cap = config.ratio_cap,
            "Synthetic ratio exceeds cap but override accepted"
        );
        SyntheticRatioValidationResult {
            ratio,
            cap: config.ratio_cap,
            status: "warn".to_string(),
            override_requested,
            override_accepted: true,
            message: format!(
                "OVERRIDE ACCEPTED: Synthetic ratio {:.1}% exceeds cap of {:.1}%. \
                 Training will proceed with elevated synthetic data. \
                 This may affect model quality and generalization.",
                ratio * 100.0,
                config.ratio_cap * 100.0
            ),
        }
    } else {
        // Exceeds cap without override - block
        SyntheticRatioValidationResult {
            ratio,
            cap: config.ratio_cap,
            status: "block".to_string(),
            override_requested,
            override_accepted: false,
            message: format!(
                "Synthetic ratio {:.1}% exceeds maximum allowed cap of {:.1}%. \
                 Request an explicit override to proceed with training.",
                ratio * 100.0,
                config.ratio_cap * 100.0
            ),
        }
    }
}

/// Calculate the synthetic data ratio from dataset metadata.
///
/// # Arguments
/// * `source_type` - Dataset source type (e.g., "generated", "code_repo", "uploaded_files")
/// * `synthetic_mode` - Whether the training is in synthetic mode
/// * `metadata` - Optional dataset metadata JSON that may contain synthetic sample counts
///
/// # Returns
/// A tuple of (synthetic_count, total_count) for ratio calculation.
pub fn extract_synthetic_counts(
    source_type: Option<&str>,
    synthetic_mode: bool,
    metadata: Option<&serde_json::Value>,
) -> (u64, u64) {
    // If in full synthetic mode, the entire dataset is synthetic
    if synthetic_mode {
        // Check metadata for sample count, default to 1 if not specified
        let total = metadata
            .and_then(|m| m.get("sample_count"))
            .and_then(|v| v.as_u64())
            .unwrap_or(1);
        return (total, total);
    }

    // Check source type
    let is_generated = source_type
        .map(|s| s.eq_ignore_ascii_case("generated") || s.eq_ignore_ascii_case("synthetic"))
        .unwrap_or(false);

    if is_generated {
        // Entire dataset is generated/synthetic
        let total = metadata
            .and_then(|m| m.get("sample_count"))
            .and_then(|v| v.as_u64())
            .unwrap_or(1);
        return (total, total);
    }

    // Check metadata for explicit synthetic counts
    if let Some(meta) = metadata {
        let synthetic_count = meta
            .get("synthetic_sample_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let total_count = meta
            .get("sample_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(1);
        return (synthetic_count, total_count);
    }

    // No synthetic data detected
    (0, 1)
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
        "allowed_with_warning" => {
            warnings.push("Dataset has warnings, review recommended".to_string())
        }
        _ => {}
    }

    // Check individual safety signals
    let check_signal = |status: &str, signal_name: &str| -> Option<(bool, String)> {
        match status.to_ascii_lowercase().as_str() {
            "block" | "unsafe" => Some((true, format!("{} detected blocking issues", signal_name))),
            "warn" => Some((false, format!("{} detected potential issues", signal_name))),
            "unknown" => Some((false, format!("{} status is unknown", signal_name))),
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

    let overall_safety =
        derive_overall_safety(pii_status, toxicity_status, leak_status, anomaly_status);

    DatasetSafetyCheckResult {
        is_safe,
        trust_state: trust_state.to_string(),
        safety_signals: SafetySignals {
            pii_status: pii_status.to_string(),
            toxicity_status: toxicity_status.to_string(),
            leak_status: leak_status.to_string(),
            anomaly_status: anomaly_status.to_string(),
            overall_safety,
            synthetic_ratio_status: None,
            synthetic_ratio: None,
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
) -> Result<impl IntoResponse, ApiError> {
    require_permission(&claims, Permission::DatasetValidate)?;

    let normalized = normalize_safety_statuses(&body).map_err(|errors| {
        ApiError::bad_request(format!(
            "Invalid safety status values: {}",
            errors.join("; ")
        ))
    })?;

    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Dataset"))?;

    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    }

    let version_id = state
        .db
        .ensure_dataset_version_exists(&dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to ensure dataset version: {}", e)))?;

    // Compute overall safety using the centralized function
    let overall_safety = derive_overall_safety(
        normalized.pii_status.as_deref().unwrap_or("unknown"),
        normalized.toxicity_status.as_deref().unwrap_or("unknown"),
        normalized.leak_status.as_deref().unwrap_or("unknown"),
        normalized.anomaly_status.as_deref().unwrap_or("unknown"),
    );

    let trust_state = state
        .db
        .update_dataset_version_safety_status(
            &version_id,
            normalized.pii_status.as_deref(),
            normalized.toxicity_status.as_deref(),
            normalized.leak_status.as_deref(),
            normalized.anomaly_status.as_deref(),
        )
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to update safety status: {}", e)))?;

    record_safety_status_updates(&state, &version_id, &claims.sub, &normalized).await;

    // Audit log: dataset safety update
    audit_helper::log_success_or_warn(
        &state.db,
        &claims,
        audit_helper::actions::DATASET_SAFETY_UPDATE,
        audit_helper::resources::DATASET_VERSION,
        Some(&version_id),
    )
    .await;

    info!(
        dataset_id = %dataset_id,
        version_id = %version_id,
        trust_state = %trust_state,
        overall_safety = %overall_safety,
        pii_status = ?normalized.pii_status,
        toxicity_status = ?normalized.toxicity_status,
        leak_status = ?normalized.leak_status,
        anomaly_status = ?normalized.anomaly_status,
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
) -> Result<impl IntoResponse, ApiError> {
    require_permission(&claims, Permission::DatasetValidate)?;

    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Dataset"))?;

    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    }

    let version_id = state
        .db
        .ensure_dataset_version_exists(&dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to ensure dataset version: {}", e)))?;

    let normalized_state = normalize_override_state("trust_state", &body.trust_state)
        .map_err(ApiError::bad_request)?;

    state
        .db
        .create_dataset_version_override(
            &version_id,
            &normalized_state,
            body.reason.as_deref(),
            &claims.sub,
        )
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to create trust override: {}", e)))?;

    // Audit log: trust override
    audit_helper::log_success_or_warn(
        &state.db,
        &claims,
        audit_helper::actions::DATASET_TRUST_OVERRIDE,
        audit_helper::resources::DATASET_VERSION,
        Some(&version_id),
    )
    .await;

    info!(
        dataset_id = %dataset_id,
        version_id = %version_id,
        trust_state = %normalized_state,
        reason = ?body.reason,
        actor = %claims.sub,
        "Applied dataset trust override"
    );

    Ok(Json(TrustOverrideResponse {
        dataset_id,
        dataset_version_id: version_id,
        trust_state: normalized_state,
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
) -> Result<impl IntoResponse, ApiError> {
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
        .map_err(|e| ApiError::db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Dataset"))?;

    // CRITICAL: Validate tenant isolation - non-admin users can only preview their own tenant's datasets
    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        // Datasets without tenant_id can only be previewed by admins
        return Err(ApiError::forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    let files = state
        .db
        .get_dataset_files(&dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to get dataset files: {}", e)))?;

    let mut examples = Vec::new();
    let mut count = 0;

    // Stream read files for memory efficiency
    for file in files {
        if count >= limit {
            break;
        }

        let safe_path =
            match ensure_dataset_file_within_root(&state, std::path::Path::new(&file.file_path))
                .await
            {
                Ok(path) => path,
                Err(err) => {
                    warn!(
                        "Failed to validate dataset file path for {}: {}",
                        file.file_name, err
                    );
                    continue;
                }
            };

        match stream_preview_file(&safe_path, &dataset.format, limit - count).await {
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
) -> Result<impl IntoResponse, ApiError> {
    require_permission(&claims, Permission::DatasetValidate)?;

    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to load dataset: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Dataset"))?;

    // Tenant isolation
    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    }

    let normalized_state =
        normalize_override_state("override_state", payload.override_state.as_str())
            .map_err(ApiError::bad_request)?;

    let version_id = state
        .db
        .ensure_dataset_version_exists(&dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to ensure dataset version: {}", e)))?;

    state
        .db
        .create_dataset_version_override(
            &version_id,
            normalized_state.as_str(),
            payload.reason.as_deref(),
            &claims.sub,
        )
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to create override: {}", e)))?;

    let effective = state
        .db
        .get_effective_trust_state(&version_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to read effective trust_state: {}", e)))?
        .unwrap_or_else(|| "unknown".to_string());

    // Audit log: trust override
    audit_helper::log_success_or_warn(
        &state.db,
        &claims,
        audit_helper::actions::DATASET_TRUST_OVERRIDE,
        audit_helper::resources::DATASET_VERSION,
        Some(&version_id),
    )
    .await;

    info!(
        dataset_id = %dataset_id,
        version_id = %version_id,
        override_state = %normalized_state,
        effective_trust_state = %effective,
        reason = ?payload.reason,
        actor = %claims.sub,
        "Applied dataset trust override"
    );

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
) -> Result<impl IntoResponse, ApiError> {
    require_permission(&claims, Permission::DatasetValidate)?;

    // Validate dataset exists and enforce tenant isolation
    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to load dataset: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Dataset"))?;

    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    }

    // Validate version exists and belongs to the dataset
    let version = state
        .db
        .get_training_dataset_version(&version_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to load dataset version: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Dataset version"))?;

    if version.dataset_id != dataset_id {
        return Err(ApiError::bad_request(
            "Version does not belong to the specified dataset",
        ));
    }

    // Enforce tenant isolation on version
    if let Some(ref version_tenant_id) = version.tenant_id {
        validate_tenant_isolation(&claims, version_tenant_id)?;
    }

    // Validate override state
    let normalized_state =
        normalize_override_state("override_state", payload.override_state.as_str())
            .map_err(ApiError::bad_request)?;

    // Create the override (this automatically propagates trust changes via DB triggers)
    state
        .db
        .create_dataset_version_override(
            &version_id,
            normalized_state.as_str(),
            payload.reason.as_deref(),
            &claims.sub,
        )
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to create override: {}", e)))?;

    // Get the effective trust state after override
    let effective = state
        .db
        .get_effective_trust_state(&version_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to read effective trust_state: {}", e)))?
        .unwrap_or_else(|| "unknown".to_string());

    // Audit log: version-specific trust override
    audit_helper::log_success_or_warn(
        &state.db,
        &claims,
        audit_helper::actions::DATASET_TRUST_OVERRIDE,
        audit_helper::resources::DATASET_VERSION,
        Some(&version_id),
    )
    .await;

    info!(
        dataset_id = %dataset_id,
        version_id = %version_id,
        override_state = %normalized_state,
        effective_state = %effective,
        reason = ?payload.reason,
        actor = %claims.sub,
        "Applied dataset version trust override"
    );

    Ok(Json(serde_json::json!({
        "dataset_id": dataset_id,
        "dataset_version_id": version_id,
        "override_state": normalized_state,
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
) -> Result<impl IntoResponse, ApiError> {
    require_permission(&claims, Permission::DatasetValidate)?;

    let normalized = normalize_safety_statuses(&body).map_err(|errors| {
        ApiError::bad_request(format!(
            "Invalid safety status values: {}",
            errors.join("; ")
        ))
    })?;

    // Validate dataset exists and enforce tenant isolation
    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Dataset"))?;

    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    }

    // Validate version exists and belongs to the dataset
    let version = state
        .db
        .get_training_dataset_version(&version_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to load dataset version: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Dataset version"))?;

    if version.dataset_id != dataset_id {
        return Err(ApiError::bad_request(
            "Version does not belong to the specified dataset",
        ));
    }

    // Enforce tenant isolation on version
    if let Some(ref version_tenant_id) = version.tenant_id {
        validate_tenant_isolation(&claims, version_tenant_id)?;
    }

    // Compute overall safety using the centralized function
    let overall_safety = derive_overall_safety(
        normalized.pii_status.as_deref().unwrap_or("unknown"),
        normalized.toxicity_status.as_deref().unwrap_or("unknown"),
        normalized.leak_status.as_deref().unwrap_or("unknown"),
        normalized.anomaly_status.as_deref().unwrap_or("unknown"),
    );

    // Update safety status (this automatically propagates trust changes via DB layer)
    let trust_state = state
        .db
        .update_dataset_version_safety_status(
            &version_id,
            normalized.pii_status.as_deref(),
            normalized.toxicity_status.as_deref(),
            normalized.leak_status.as_deref(),
            normalized.anomaly_status.as_deref(),
        )
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to update safety status: {}", e)))?;

    record_safety_status_updates(&state, &version_id, &claims.sub, &normalized).await;

    // Audit log: version-specific safety update
    audit_helper::log_success_or_warn(
        &state.db,
        &claims,
        audit_helper::actions::DATASET_SAFETY_UPDATE,
        audit_helper::resources::DATASET_VERSION,
        Some(&version_id),
    )
    .await;

    info!(
        dataset_id = %dataset_id,
        version_id = %version_id,
        trust_state = %trust_state,
        overall_safety = %overall_safety,
        pii_status = ?normalized.pii_status,
        toxicity_status = ?normalized.toxicity_status,
        leak_status = ?normalized.leak_status,
        anomaly_status = ?normalized.anomaly_status,
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

/// Check dataset safety status for training.
///
/// Returns a comprehensive safety check result indicating whether the dataset
/// can be used for training, along with detailed blocking reasons and warnings.
#[utoipa::path(
    get,
    path = "/v1/datasets/{dataset_id}/safety-check",
    params(
        ("dataset_id" = String, Path, description = "Dataset ID")
    ),
    responses(
        (status = 200, description = "Safety check result", body = DatasetSafetyCheckResult),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Dataset not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn check_dataset_safety(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dataset_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    require_permission(&claims, Permission::DatasetView)?;

    // Get the dataset
    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Dataset"))?;

    // Tenant isolation check
    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        return Err(ApiError::forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    // Get the latest version to check its safety status
    let version_id = state
        .db
        .ensure_dataset_version_exists(&dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to get dataset version: {}", e)))?;

    let version = state
        .db
        .get_training_dataset_version(&version_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to load dataset version: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Dataset version"))?;

    // Get effective trust state (considers overrides)
    let effective_trust_state = state
        .db
        .get_effective_trust_state(&version_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to get effective trust state: {}", e)))?
        .unwrap_or_else(|| "unknown".to_string());

    // Evaluate safety
    let result = evaluate_dataset_safety(
        &effective_trust_state,
        &version.pii_status,
        &version.toxicity_status,
        &version.leak_status,
        &version.anomaly_status,
    );

    // Escalate to review if dataset is not safe
    if !result.is_safe {
        if let Some(ref pause_tracker) = state.pause_tracker {
            let pause_id = format!("dataset-safety-{}-{}", dataset_id, uuid::Uuid::new_v4());
            let context = format!(
                "Dataset '{}' (version {}) failed safety check. Trust state: {}. Blocking reasons: {}",
                dataset_id,
                version_id,
                result.trust_state,
                result.blocking_reasons.join("; ")
            );

            let metadata = serde_json::json!({
                "dataset_id": dataset_id,
                "dataset_version_id": version_id,
                "trust_state": result.trust_state,
                "blocking_reasons": result.blocking_reasons,
                "warnings": result.warnings,
                "safety_signals": {
                    "pii_status": result.safety_signals.pii_status,
                    "toxicity_status": result.safety_signals.toxicity_status,
                    "leak_status": result.safety_signals.leak_status,
                    "anomaly_status": result.safety_signals.anomaly_status,
                    "overall_safety": result.safety_signals.overall_safety,
                }
            });

            pause_tracker.register_server_pause(
                pause_id.clone(),
                format!("dataset:{}", dataset_id),
                "policy_approval",
                Some(context),
                Some(metadata),
            );

            warn!(
                dataset_id = %dataset_id,
                version_id = %version_id,
                pause_id = %pause_id,
                trust_state = %result.trust_state,
                "Dataset safety check failed - escalated to review"
            );
        }
    }

    info!(
        dataset_id = %dataset_id,
        version_id = %version_id,
        is_safe = %result.is_safe,
        trust_state = %result.trust_state,
        "Dataset safety check completed"
    );

    Ok(Json(result))
}

/// Check safety status for a specific dataset version.
#[utoipa::path(
    get,
    path = "/v1/datasets/{dataset_id}/versions/{version_id}/safety-check",
    params(
        ("dataset_id" = String, Path, description = "Dataset ID"),
        ("version_id" = String, Path, description = "Dataset version ID")
    ),
    responses(
        (status = 200, description = "Safety check result", body = DatasetSafetyCheckResult),
        (status = 400, description = "Version does not belong to dataset"),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Dataset or version not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn check_dataset_version_safety(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((dataset_id, version_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    require_permission(&claims, Permission::DatasetView)?;

    // Get the dataset
    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Dataset"))?;

    // Tenant isolation check
    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        return Err(ApiError::forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    // Get the version
    let version = state
        .db
        .get_training_dataset_version(&version_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to load dataset version: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Dataset version"))?;

    // Verify version belongs to dataset
    if version.dataset_id != dataset_id {
        return Err(ApiError::bad_request(
            "Version does not belong to the specified dataset",
        ));
    }

    // Tenant isolation on version
    if let Some(ref version_tenant_id) = version.tenant_id {
        validate_tenant_isolation(&claims, version_tenant_id)?;
    }

    // Get effective trust state (considers overrides)
    let effective_trust_state = state
        .db
        .get_effective_trust_state(&version_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to get effective trust state: {}", e)))?
        .unwrap_or_else(|| "unknown".to_string());

    // Evaluate safety
    let result = evaluate_dataset_safety(
        &effective_trust_state,
        &version.pii_status,
        &version.toxicity_status,
        &version.leak_status,
        &version.anomaly_status,
    );

    // Escalate to review if dataset version is not safe
    if !result.is_safe {
        if let Some(ref pause_tracker) = state.pause_tracker {
            let pause_id = format!(
                "dataset-safety-{}-{}-{}",
                dataset_id,
                version_id,
                uuid::Uuid::new_v4()
            );
            let context = format!(
                "Dataset '{}' version '{}' failed safety check. Trust state: {}. Blocking reasons: {}",
                dataset_id,
                version_id,
                result.trust_state,
                result.blocking_reasons.join("; ")
            );

            let metadata = serde_json::json!({
                "dataset_id": dataset_id,
                "dataset_version_id": version_id,
                "trust_state": result.trust_state,
                "blocking_reasons": result.blocking_reasons,
                "warnings": result.warnings,
                "safety_signals": {
                    "pii_status": result.safety_signals.pii_status,
                    "toxicity_status": result.safety_signals.toxicity_status,
                    "leak_status": result.safety_signals.leak_status,
                    "anomaly_status": result.safety_signals.anomaly_status,
                    "overall_safety": result.safety_signals.overall_safety,
                }
            });

            pause_tracker.register_server_pause(
                pause_id.clone(),
                format!("dataset:{}:{}", dataset_id, version_id),
                "policy_approval",
                Some(context),
                Some(metadata),
            );

            warn!(
                dataset_id = %dataset_id,
                version_id = %version_id,
                pause_id = %pause_id,
                trust_state = %result.trust_state,
                "Dataset version safety check failed - escalated to review"
            );
        }
    }

    info!(
        dataset_id = %dataset_id,
        version_id = %version_id,
        is_safe = %result.is_safe,
        trust_state = %result.trust_state,
        "Dataset version safety check completed"
    );

    Ok(Json(result))
}

// ============================================================================
// Safety History Types and Endpoints
// ============================================================================

/// Query parameters for listing safety history.
#[derive(Debug, Clone, Deserialize, IntoParams, ToSchema)]
pub struct SafetyHistoryQuery {
    /// Maximum number of records to return (default: 50, max: 100)
    #[serde(default = "default_history_limit")]
    pub limit: i64,
}

fn default_history_limit() -> i64 {
    50
}

/// A single validation run record.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ValidationRunRecord {
    /// Unique identifier for this validation run
    pub id: String,
    /// Dataset version ID
    pub dataset_version_id: String,
    /// Validation tier (tier1_structural or tier2_safety)
    pub tier: String,
    /// Validation result status
    pub status: String,
    /// Specific signal being validated (pii, toxicity, leak, anomaly, structural)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signal: Option<String>,
    /// Validation errors in JSON format
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_errors_json: Option<String>,
    /// When the validation was run (ISO 8601)
    pub created_at: String,
    /// Who triggered this validation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

/// A single trust override record.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrustOverrideRecord {
    /// Unique identifier for this override
    pub id: String,
    /// Dataset version ID
    pub dataset_version_id: String,
    /// The override state applied
    pub override_state: String,
    /// Reason for the override
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Who applied this override
    pub created_by: String,
    /// When the override was applied (ISO 8601)
    pub created_at: String,
}

/// Combined safety history for a dataset version.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SafetyHistoryResponse {
    /// Dataset ID
    pub dataset_id: String,
    /// Dataset version ID
    pub dataset_version_id: String,
    /// Current effective trust state
    pub effective_trust_state: String,
    /// Current safety signals
    pub current_safety: SafetySignals,
    /// History of validation runs (most recent first)
    pub validation_runs: Vec<ValidationRunRecord>,
    /// History of trust overrides (most recent first)
    pub trust_overrides: Vec<TrustOverrideRecord>,
}

/// Get safety update history for a dataset version.
///
/// Returns the complete audit trail of safety-related changes including:
/// - Validation runs (PII, toxicity, leak, anomaly checks)
/// - Trust state overrides applied by administrators
#[utoipa::path(
    get,
    path = "/v1/datasets/{dataset_id}/versions/{version_id}/safety-history",
    params(
        ("dataset_id" = String, Path, description = "Dataset ID"),
        ("version_id" = String, Path, description = "Dataset version ID"),
        SafetyHistoryQuery
    ),
    responses(
        (status = 200, description = "Safety history retrieved", body = SafetyHistoryResponse),
        (status = 400, description = "Version does not belong to dataset"),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Dataset or version not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
#[allow(dead_code)]
pub async fn get_dataset_version_safety_history(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((dataset_id, version_id)): Path<(String, String)>,
    Query(params): Query<SafetyHistoryQuery>,
) -> Result<impl IntoResponse, ApiError> {
    require_permission(&claims, Permission::DatasetView)?;

    // Get the dataset
    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Dataset"))?;

    // Tenant isolation check
    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        return Err(ApiError::forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    // Get the version
    let version = state
        .db
        .get_training_dataset_version(&version_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to load dataset version: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Dataset version"))?;

    // Verify version belongs to dataset
    if version.dataset_id != dataset_id {
        return Err(ApiError::bad_request(
            "Version does not belong to the specified dataset",
        ));
    }

    // Tenant isolation on version
    if let Some(ref version_tenant_id) = version.tenant_id {
        validate_tenant_isolation(&claims, version_tenant_id)?;
    }

    // Get effective trust state
    let effective_trust_state = state
        .db
        .get_effective_trust_state(&version_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to get effective trust state: {}", e)))?
        .unwrap_or_else(|| "unknown".to_string());

    // Get validation runs
    let limit = params.limit.min(100);
    let validation_runs = state
        .db
        .list_dataset_version_validation_runs(&version_id, limit)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to list validation runs: {}", e)))?;

    // Get trust overrides
    let overrides = state
        .db
        .list_dataset_version_overrides(&version_id, limit)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to list trust overrides: {}", e)))?;

    // Map to response types
    let validation_run_records: Vec<ValidationRunRecord> = validation_runs
        .into_iter()
        .map(|v| ValidationRunRecord {
            id: v.id,
            dataset_version_id: v.dataset_version_id,
            tier: v.tier,
            status: v.status,
            signal: v.signal,
            validation_errors_json: v.validation_errors_json,
            created_at: v.created_at,
            created_by: v.created_by,
        })
        .collect();

    let override_records: Vec<TrustOverrideRecord> = overrides
        .into_iter()
        .map(|o| TrustOverrideRecord {
            id: o.id,
            dataset_version_id: o.dataset_version_id,
            override_state: o.override_state,
            reason: o.reason,
            created_by: o.created_by,
            created_at: o.created_at,
        })
        .collect();

    // Current safety signals
    let overall_safety = derive_overall_safety(
        &version.pii_status,
        &version.toxicity_status,
        &version.leak_status,
        &version.anomaly_status,
    );

    let current_safety = SafetySignals {
        pii_status: version.pii_status,
        toxicity_status: version.toxicity_status,
        leak_status: version.leak_status,
        anomaly_status: version.anomaly_status,
        overall_safety,
        synthetic_ratio_status: None,
        synthetic_ratio: None,
    };

    info!(
        dataset_id = %dataset_id,
        version_id = %version_id,
        validation_run_count = %validation_run_records.len(),
        override_count = %override_records.len(),
        actor = %claims.sub,
        "Retrieved dataset version safety history"
    );

    Ok(Json(SafetyHistoryResponse {
        dataset_id,
        dataset_version_id: version_id,
        effective_trust_state,
        current_safety,
        validation_runs: validation_run_records,
        trust_overrides: override_records,
    }))
}

/// Get safety update history for a dataset (uses latest version).
///
/// Returns the complete audit trail of safety-related changes for the
/// latest version of the dataset.
#[utoipa::path(
    get,
    path = "/v1/datasets/{dataset_id}/safety-history",
    params(
        ("dataset_id" = String, Path, description = "Dataset ID"),
        SafetyHistoryQuery
    ),
    responses(
        (status = 200, description = "Safety history retrieved", body = SafetyHistoryResponse),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Dataset not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
#[allow(dead_code)]
pub async fn get_dataset_safety_history(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dataset_id): Path<String>,
    Query(params): Query<SafetyHistoryQuery>,
) -> Result<impl IntoResponse, ApiError> {
    require_permission(&claims, Permission::DatasetView)?;

    // Get the dataset
    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Dataset"))?;

    // Tenant isolation check
    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        return Err(ApiError::forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    // Get the latest version
    let version_id = state
        .db
        .ensure_dataset_version_exists(&dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to get dataset version: {}", e)))?;

    let version = state
        .db
        .get_training_dataset_version(&version_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to load dataset version: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Dataset version"))?;

    // Get effective trust state
    let effective_trust_state = state
        .db
        .get_effective_trust_state(&version_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to get effective trust state: {}", e)))?
        .unwrap_or_else(|| "unknown".to_string());

    // Get validation runs
    let limit = params.limit.min(100);
    let validation_runs = state
        .db
        .list_dataset_version_validation_runs(&version_id, limit)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to list validation runs: {}", e)))?;

    // Get trust overrides
    let overrides = state
        .db
        .list_dataset_version_overrides(&version_id, limit)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to list trust overrides: {}", e)))?;

    // Map to response types
    let validation_run_records: Vec<ValidationRunRecord> = validation_runs
        .into_iter()
        .map(|v| ValidationRunRecord {
            id: v.id,
            dataset_version_id: v.dataset_version_id,
            tier: v.tier,
            status: v.status,
            signal: v.signal,
            validation_errors_json: v.validation_errors_json,
            created_at: v.created_at,
            created_by: v.created_by,
        })
        .collect();

    let override_records: Vec<TrustOverrideRecord> = overrides
        .into_iter()
        .map(|o| TrustOverrideRecord {
            id: o.id,
            dataset_version_id: o.dataset_version_id,
            override_state: o.override_state,
            reason: o.reason,
            created_by: o.created_by,
            created_at: o.created_at,
        })
        .collect();

    // Current safety signals
    let overall_safety = derive_overall_safety(
        &version.pii_status,
        &version.toxicity_status,
        &version.leak_status,
        &version.anomaly_status,
    );

    let current_safety = SafetySignals {
        pii_status: version.pii_status,
        toxicity_status: version.toxicity_status,
        leak_status: version.leak_status,
        anomaly_status: version.anomaly_status,
        overall_safety,
        synthetic_ratio_status: None,
        synthetic_ratio: None,
    };

    info!(
        dataset_id = %dataset_id,
        version_id = %version_id,
        validation_run_count = %validation_run_records.len(),
        override_count = %override_records.len(),
        actor = %claims.sub,
        "Retrieved dataset safety history"
    );

    Ok(Json(SafetyHistoryResponse {
        dataset_id,
        dataset_version_id: version_id,
        effective_trust_state,
        current_safety,
        validation_runs: validation_run_records,
        trust_overrides: override_records,
    }))
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod safety_validation_tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Safety Status Validation Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_validate_safety_status_valid_values() {
        assert!(validate_safety_status("clean").is_ok());
        assert!(validate_safety_status("warn").is_ok());
        assert!(validate_safety_status("block").is_ok());
        assert!(validate_safety_status("unknown").is_ok());

        // Case insensitive
        assert!(validate_safety_status("CLEAN").is_ok());
        assert!(validate_safety_status("Warn").is_ok());
        assert!(validate_safety_status("BLOCK").is_ok());
    }

    #[test]
    fn test_validate_safety_status_invalid_values() {
        assert!(validate_safety_status("invalid").is_err());
        assert!(validate_safety_status("").is_err());
        assert!(validate_safety_status("safe").is_err());
        assert!(validate_safety_status("danger").is_err());
    }

    #[test]
    fn test_validate_safety_status_normalizes_case() {
        assert_eq!(validate_safety_status("CLEAN").unwrap(), "clean");
        assert_eq!(validate_safety_status("Warn").unwrap(), "warn");
        assert_eq!(validate_safety_status("BLOCK").unwrap(), "block");
    }

    // -------------------------------------------------------------------------
    // Trust State Validation Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_validate_trust_state_valid_values() {
        assert!(validate_trust_state("allowed").is_ok());
        assert!(validate_trust_state("allowed_with_warning").is_ok());
        assert!(validate_trust_state("blocked").is_ok());
        assert!(validate_trust_state("needs_approval").is_ok());
        assert!(validate_trust_state("unknown").is_ok());

        // Case insensitive
        assert!(validate_trust_state("ALLOWED").is_ok());
        assert!(validate_trust_state("Blocked").is_ok());
    }

    #[test]
    fn test_validate_trust_state_invalid_values() {
        assert!(validate_trust_state("invalid").is_err());
        assert!(validate_trust_state("").is_err());
        assert!(validate_trust_state("approved").is_err());
        assert!(validate_trust_state("denied").is_err());
    }

    #[test]
    fn test_validate_trust_state_normalizes_case() {
        assert_eq!(validate_trust_state("ALLOWED").unwrap(), "allowed");
        assert_eq!(validate_trust_state("Blocked").unwrap(), "blocked");
    }

    // -------------------------------------------------------------------------
    // Safety Request Validation Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_validate_safety_request_all_valid() {
        let request = UpdateDatasetSafetyRequest {
            pii_status: Some("clean".to_string()),
            toxicity_status: Some("warn".to_string()),
            leak_status: Some("block".to_string()),
            anomaly_status: Some("unknown".to_string()),
        };

        let result = validate_safety_request(&request);
        assert!(result.is_valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_validate_safety_request_with_invalid_pii() {
        let request = UpdateDatasetSafetyRequest {
            pii_status: Some("invalid_status".to_string()),
            toxicity_status: Some("clean".to_string()),
            leak_status: None,
            anomaly_status: None,
        };

        let result = validate_safety_request(&request);
        assert!(!result.is_valid);
        assert_eq!(result.errors.len(), 1);
        assert!(result.errors[0].contains("pii_status"));
    }

    #[test]
    fn test_validate_safety_request_with_multiple_invalid() {
        let request = UpdateDatasetSafetyRequest {
            pii_status: Some("bad".to_string()),
            toxicity_status: Some("also_bad".to_string()),
            leak_status: Some("wrong".to_string()),
            anomaly_status: Some("nope".to_string()),
        };

        let result = validate_safety_request(&request);
        assert!(!result.is_valid);
        assert_eq!(result.errors.len(), 4);
    }

    #[test]
    fn test_validate_safety_request_empty_request() {
        let request = UpdateDatasetSafetyRequest {
            pii_status: None,
            toxicity_status: None,
            leak_status: None,
            anomaly_status: None,
        };

        let result = validate_safety_request(&request);
        assert!(result.is_valid);
        assert!(result.errors.is_empty());
    }

    // -------------------------------------------------------------------------
    // Trust State Safety Check Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_is_trust_state_safe() {
        assert!(is_trust_state_safe("allowed"));
        assert!(is_trust_state_safe("allowed_with_warning"));
        assert!(is_trust_state_safe("ALLOWED"));

        assert!(!is_trust_state_safe("blocked"));
        assert!(!is_trust_state_safe("needs_approval"));
        assert!(!is_trust_state_safe("unknown"));
    }

    #[test]
    fn test_is_trust_state_blocked() {
        assert!(is_trust_state_blocked("blocked"));
        assert!(is_trust_state_blocked("needs_approval"));
        assert!(is_trust_state_blocked("unknown"));

        assert!(!is_trust_state_blocked("allowed"));
        assert!(!is_trust_state_blocked("allowed_with_warning"));
    }

    // -------------------------------------------------------------------------
    // Overall Safety Derivation Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_derive_overall_safety_all_clean() {
        let result = derive_overall_safety("clean", "clean", "clean", "clean");
        assert_eq!(result, "clean");
    }

    #[test]
    fn test_derive_overall_safety_any_block() {
        // Any block should result in overall block
        assert_eq!(
            derive_overall_safety("block", "clean", "clean", "clean"),
            "block"
        );
        assert_eq!(
            derive_overall_safety("clean", "block", "clean", "clean"),
            "block"
        );
        assert_eq!(
            derive_overall_safety("clean", "clean", "block", "clean"),
            "block"
        );
        assert_eq!(
            derive_overall_safety("clean", "clean", "clean", "block"),
            "block"
        );

        // "unsafe" is also a blocking status
        assert_eq!(
            derive_overall_safety("unsafe", "clean", "clean", "clean"),
            "block"
        );
    }

    #[test]
    fn test_derive_overall_safety_any_warn() {
        // Warn (without block) should result in overall warn
        assert_eq!(
            derive_overall_safety("warn", "clean", "clean", "clean"),
            "warn"
        );
        assert_eq!(
            derive_overall_safety("clean", "warn", "clean", "clean"),
            "warn"
        );
        assert_eq!(
            derive_overall_safety("clean", "clean", "warn", "clean"),
            "warn"
        );
        assert_eq!(
            derive_overall_safety("clean", "clean", "clean", "warn"),
            "warn"
        );
    }

    #[test]
    fn test_derive_overall_safety_block_takes_priority() {
        // Block should take priority over warn
        assert_eq!(
            derive_overall_safety("block", "warn", "warn", "warn"),
            "block"
        );
        assert_eq!(
            derive_overall_safety("warn", "block", "warn", "warn"),
            "block"
        );
    }

    #[test]
    fn test_derive_overall_safety_all_unknown() {
        let result = derive_overall_safety("unknown", "unknown", "unknown", "unknown");
        assert_eq!(result, "unknown");
    }

    #[test]
    fn test_derive_overall_safety_mixed_unknown_and_clean() {
        // If some are clean and some are unknown, result is clean (not all unknown)
        let result = derive_overall_safety("clean", "unknown", "clean", "unknown");
        assert_eq!(result, "clean");
    }

    // -------------------------------------------------------------------------
    // Dataset Safety Evaluation Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_evaluate_dataset_safety_allowed() {
        let result = evaluate_dataset_safety("allowed", "clean", "clean", "clean", "clean");

        assert!(result.is_safe);
        assert_eq!(result.trust_state, "allowed");
        assert!(result.blocking_reasons.is_empty());
        assert!(result.warnings.is_empty());
        assert_eq!(result.safety_signals.overall_safety, "clean");
    }

    #[test]
    fn test_evaluate_dataset_safety_allowed_with_warning() {
        let result =
            evaluate_dataset_safety("allowed_with_warning", "clean", "warn", "clean", "clean");

        assert!(result.is_safe);
        assert_eq!(result.trust_state, "allowed_with_warning");
        assert!(result.blocking_reasons.is_empty());
        assert!(!result.warnings.is_empty());
        assert_eq!(result.safety_signals.overall_safety, "warn");
    }

    #[test]
    fn test_evaluate_dataset_safety_blocked() {
        let result = evaluate_dataset_safety("blocked", "clean", "clean", "clean", "clean");

        assert!(!result.is_safe);
        assert_eq!(result.trust_state, "blocked");
        assert!(!result.blocking_reasons.is_empty());
        assert!(result.blocking_reasons[0].contains("blocked"));
    }

    #[test]
    fn test_evaluate_dataset_safety_needs_approval() {
        let result = evaluate_dataset_safety("needs_approval", "clean", "clean", "clean", "clean");

        assert!(!result.is_safe);
        assert_eq!(result.trust_state, "needs_approval");
        assert!(!result.blocking_reasons.is_empty());
        assert!(result.blocking_reasons[0].contains("approval"));
    }

    #[test]
    fn test_evaluate_dataset_safety_unknown_trust() {
        let result = evaluate_dataset_safety("unknown", "clean", "clean", "clean", "clean");

        assert!(!result.is_safe);
        assert_eq!(result.trust_state, "unknown");
        assert!(!result.blocking_reasons.is_empty());
    }

    #[test]
    fn test_evaluate_dataset_safety_with_blocking_signals() {
        let result = evaluate_dataset_safety("allowed", "block", "clean", "clean", "clean");

        // Trust state says allowed, but PII is blocking - still reports blocking signals
        assert!(result.is_safe); // Trust state determines overall safety
        assert!(!result.blocking_reasons.is_empty()); // But we still report the signal issue
        assert!(result.blocking_reasons.iter().any(|r| r.contains("PII")));
        assert_eq!(result.safety_signals.pii_status, "block");
    }

    #[test]
    fn test_evaluate_dataset_safety_with_warning_signals() {
        let result = evaluate_dataset_safety("allowed", "warn", "clean", "warn", "clean");

        assert!(result.is_safe);
        assert_eq!(result.warnings.len(), 2); // PII and leak warnings
        assert!(result.warnings.iter().any(|w| w.contains("PII")));
        assert!(result.warnings.iter().any(|w| w.contains("leak")));
    }

    #[test]
    fn test_evaluate_dataset_safety_with_unknown_signals() {
        let result = evaluate_dataset_safety("allowed", "unknown", "unknown", "unknown", "unknown");

        assert!(result.is_safe);
        // Unknown signals should generate warnings
        assert!(!result.warnings.is_empty());
        assert_eq!(result.safety_signals.overall_safety, "unknown");
    }

    #[test]
    fn test_safety_signals_struct() {
        let signals = SafetySignals {
            pii_status: "clean".to_string(),
            toxicity_status: "warn".to_string(),
            leak_status: "block".to_string(),
            anomaly_status: "unknown".to_string(),
            overall_safety: "block".to_string(),
            synthetic_ratio_status: None,
            synthetic_ratio: None,
        };

        assert_eq!(signals.pii_status, "clean");
        assert_eq!(signals.toxicity_status, "warn");
        assert_eq!(signals.leak_status, "block");
        assert_eq!(signals.anomaly_status, "unknown");
        assert_eq!(signals.overall_safety, "block");
    }

    #[test]
    fn test_safety_check_result_serialization() {
        let result = DatasetSafetyCheckResult {
            is_safe: true,
            trust_state: "allowed".to_string(),
            safety_signals: SafetySignals::default(),
            blocking_reasons: vec![],
            warnings: vec!["Test warning".to_string()],
        };

        let json = serde_json::to_string(&result).expect("Should serialize");
        assert!(json.contains("is_safe"));
        assert!(json.contains("trust_state"));
        assert!(json.contains("safety_signals"));
    }

    // -------------------------------------------------------------------------
    // Synthetic Ratio Validation Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_validate_synthetic_ratio_clean_below_warning_threshold() {
        let config = SyntheticRatioConfig::default();
        // 30% synthetic (below 40% warning threshold)
        let result = validate_synthetic_ratio(30, 100, &config, false);

        assert_eq!(result.status, "clean");
        assert!(!result.override_accepted);
        assert!((result.ratio - 0.3).abs() < f64::EPSILON);
    }

    #[test]
    fn test_validate_synthetic_ratio_warn_approaching_cap() {
        let config = SyntheticRatioConfig::default();
        // 45% synthetic (above 40% warning threshold, below 50% cap)
        let result = validate_synthetic_ratio(45, 100, &config, false);

        assert_eq!(result.status, "warn");
        assert!(!result.override_accepted);
        assert!(result.message.contains("approaching"));
    }

    #[test]
    fn test_validate_synthetic_ratio_block_exceeds_cap() {
        let config = SyntheticRatioConfig::default();
        // 60% synthetic (above 50% cap)
        let result = validate_synthetic_ratio(60, 100, &config, false);

        assert_eq!(result.status, "block");
        assert!(!result.override_accepted);
        assert!(result.message.contains("exceeds"));
    }

    #[test]
    fn test_validate_synthetic_ratio_override_accepted() {
        let config = SyntheticRatioConfig::default();
        // 60% synthetic (above 50% cap) with override requested
        let result = validate_synthetic_ratio(60, 100, &config, true);

        assert_eq!(result.status, "warn"); // Not block when override accepted
        assert!(result.override_requested);
        assert!(result.override_accepted);
        assert!(result.message.contains("OVERRIDE ACCEPTED"));
    }

    #[test]
    fn test_validate_synthetic_ratio_override_not_allowed() {
        let config = SyntheticRatioConfig {
            ratio_cap: 0.5,
            enabled: true,
            allow_override: false, // Override not allowed
        };
        // 60% synthetic with override requested but not allowed
        let result = validate_synthetic_ratio(60, 100, &config, true);

        assert_eq!(result.status, "block"); // Still blocked
        assert!(result.override_requested);
        assert!(!result.override_accepted);
    }

    #[test]
    fn test_validate_synthetic_ratio_guardrail_disabled() {
        let config = SyntheticRatioConfig {
            ratio_cap: 0.5,
            enabled: false, // Guardrail disabled
            allow_override: true,
        };
        // 90% synthetic but guardrails disabled
        let result = validate_synthetic_ratio(90, 100, &config, false);

        assert_eq!(result.status, "clean");
        assert!(result.message.contains("disabled"));
    }

    #[test]
    fn test_validate_synthetic_ratio_empty_dataset() {
        let config = SyntheticRatioConfig::default();
        let result = validate_synthetic_ratio(0, 0, &config, false);

        assert_eq!(result.status, "clean");
        assert!((result.ratio - 0.0).abs() < f64::EPSILON);
        assert!(result.message.contains("empty"));
    }

    #[test]
    fn test_validate_synthetic_ratio_custom_cap() {
        let config = SyntheticRatioConfig {
            ratio_cap: 0.3, // 30% cap
            enabled: true,
            allow_override: true,
        };
        // 35% synthetic (above 30% cap)
        let result = validate_synthetic_ratio(35, 100, &config, false);

        assert_eq!(result.status, "block");
        assert!((result.cap - 0.3).abs() < f64::EPSILON);
    }

    #[test]
    fn test_validate_synthetic_ratio_100_percent_synthetic() {
        let config = SyntheticRatioConfig::default();
        // 100% synthetic
        let result = validate_synthetic_ratio(100, 100, &config, false);

        assert_eq!(result.status, "block");
        assert!((result.ratio - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_validate_synthetic_ratio_exactly_at_cap() {
        let config = SyntheticRatioConfig::default();
        // Exactly at 50% cap
        let result = validate_synthetic_ratio(50, 100, &config, false);

        // At cap should be warn (approaching limit), not block
        assert_eq!(result.status, "warn");
    }

    #[test]
    fn test_extract_synthetic_counts_synthetic_mode() {
        let (synthetic, total) = extract_synthetic_counts(None, true, None);
        assert_eq!(synthetic, 1);
        assert_eq!(total, 1);

        // With metadata containing sample count
        let metadata = serde_json::json!({ "sample_count": 500 });
        let (synthetic, total) = extract_synthetic_counts(None, true, Some(&metadata));
        assert_eq!(synthetic, 500);
        assert_eq!(total, 500);
    }

    #[test]
    fn test_extract_synthetic_counts_generated_source_type() {
        let (synthetic, total) = extract_synthetic_counts(Some("generated"), false, None);
        assert_eq!(synthetic, 1);
        assert_eq!(total, 1);

        // Also test "synthetic" alias
        let (synthetic, total) = extract_synthetic_counts(Some("synthetic"), false, None);
        assert_eq!(synthetic, 1);
        assert_eq!(total, 1);
    }

    #[test]
    fn test_extract_synthetic_counts_explicit_metadata() {
        let metadata = serde_json::json!({
            "sample_count": 1000,
            "synthetic_sample_count": 300
        });
        let (synthetic, total) =
            extract_synthetic_counts(Some("uploaded_files"), false, Some(&metadata));

        assert_eq!(synthetic, 300);
        assert_eq!(total, 1000);
    }

    #[test]
    fn test_extract_synthetic_counts_no_synthetic_data() {
        let (synthetic, total) = extract_synthetic_counts(Some("code_repo"), false, None);
        assert_eq!(synthetic, 0);
        assert_eq!(total, 1);
    }

    #[test]
    fn test_synthetic_ratio_config_default() {
        let config = SyntheticRatioConfig::default();
        assert!((config.ratio_cap - DEFAULT_SYNTHETIC_RATIO_CAP).abs() < f64::EPSILON);
        assert!(config.enabled);
        assert!(config.allow_override);
    }

    #[test]
    fn test_synthetic_ratio_validation_result_serialization() {
        let result = SyntheticRatioValidationResult {
            ratio: 0.6,
            cap: 0.5,
            status: "block".to_string(),
            override_requested: false,
            override_accepted: false,
            message: "Synthetic ratio 60.0% exceeds maximum allowed cap of 50.0%.".to_string(),
        };

        let json = serde_json::to_string(&result).expect("Should serialize");
        assert!(json.contains("ratio"));
        assert!(json.contains("cap"));
        assert!(json.contains("status"));
        assert!(json.contains("override_requested"));
    }

    #[test]
    fn test_safety_signals_with_synthetic_ratio() {
        let signals = SafetySignals {
            pii_status: "clean".to_string(),
            toxicity_status: "clean".to_string(),
            leak_status: "clean".to_string(),
            anomaly_status: "clean".to_string(),
            overall_safety: "warn".to_string(),
            synthetic_ratio_status: Some("warn".to_string()),
            synthetic_ratio: Some(0.45),
        };

        assert_eq!(signals.synthetic_ratio_status, Some("warn".to_string()));
        assert_eq!(signals.synthetic_ratio, Some(0.45));

        // Test serialization includes new fields
        let json = serde_json::to_string(&signals).expect("Should serialize");
        assert!(json.contains("synthetic_ratio_status"));
        assert!(json.contains("synthetic_ratio"));
    }
}
