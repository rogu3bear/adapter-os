//! Checkpoint signature verification handlers
//!
//! Endpoints for verifying training checkpoint integrity using BLAKE3
//! hashing and Ed25519 signatures. Checkpoints are verified against their
//! `.sig` sidecar files.
//!
//! ## Endpoints
//!
//! - `POST /v1/training/checkpoints/verify` — Verify checkpoint from uploaded content (protected)
//! - `GET  /v1/training/jobs/{job_id}/checkpoints/{epoch}/verify` — Verify on-disk checkpoint (protected)

use crate::api_error::{ApiError, ApiResult};
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::types::ErrorResponse;

use adapteros_crypto::checkpoint_verify::{verify_checkpoint_bytes, verify_checkpoint_file};
use axum::{
    extract::{Path, State},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// ===== Request / Response Types =====

/// Request body for inline checkpoint verification.
#[derive(Debug, Deserialize, ToSchema)]
pub struct VerifyCheckpointRequest {
    /// Base64-encoded checkpoint content
    pub checkpoint_b64: String,
    /// Base64-encoded signature sidecar JSON
    pub signature_b64: String,
}

/// Checkpoint verification report returned by both endpoints.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CheckpointVerifyResponse {
    /// BLAKE3 hash of the checkpoint content (hex)
    pub blake3_hash: String,
    /// Key ID of the signer (kid-{blake3(pubkey)[..32]})
    pub signer_key_id: String,
    /// ISO 8601 timestamp when the checkpoint was signed
    pub signed_at: String,
    /// Schema version of the sidecar
    pub schema_version: u8,
    /// Whether verification passed
    pub verified: bool,
}

/// Path parameters for job checkpoint verification.
#[derive(Debug, Deserialize)]
pub struct JobCheckpointPath {
    pub job_id: String,
    pub epoch: u32,
}

// ===== Handlers =====

/// Verify a checkpoint from uploaded content and signature.
///
/// Accepts base64-encoded checkpoint content and its signature sidecar,
/// recomputes the BLAKE3 hash, and verifies the Ed25519 signature.
#[utoipa::path(
    post,
    path = "/v1/training/checkpoints/verify",
    tag = "training",
    request_body = VerifyCheckpointRequest,
    responses(
        (status = 200, description = "Checkpoint verification report", body = CheckpointVerifyResponse),
        (status = 400, description = "Invalid input or verification failed", body = ErrorResponse)
    )
)]
pub async fn verify_checkpoint(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<VerifyCheckpointRequest>,
) -> ApiResult<CheckpointVerifyResponse> {
    require_permission(&claims, Permission::TrainingView)?;

    use base64::Engine;
    let engine = base64::engine::general_purpose::STANDARD;

    let content = engine.decode(&req.checkpoint_b64).map_err(|e| {
        ApiError::bad_request("invalid base64 in checkpoint_b64").with_details(e.to_string())
    })?;

    let sig_json = engine.decode(&req.signature_b64).map_err(|e| {
        ApiError::bad_request("invalid base64 in signature_b64").with_details(e.to_string())
    })?;

    let report = verify_checkpoint_bytes(&content, &sig_json).map_err(|e| {
        ApiError::bad_request("checkpoint verification failed").with_details(e.to_string())
    })?;

    Ok(Json(CheckpointVerifyResponse {
        blake3_hash: report.blake3_hash.to_hex(),
        signer_key_id: report.signer_key_id,
        signed_at: report.signed_at,
        schema_version: report.schema_version,
        verified: true,
    }))
}

/// Verify an on-disk checkpoint for a training job.
///
/// Reads the checkpoint file at `{output_dir}/{job_id}_epoch_{epoch:04}.ckpt`
/// and its `.sig` sidecar, then verifies BLAKE3 + Ed25519 integrity.
#[utoipa::path(
    get,
    path = "/v1/training/jobs/{job_id}/checkpoints/{epoch}/verify",
    tag = "training",
    params(
        ("job_id" = String, Path, description = "Training job ID"),
        ("epoch" = u32, Path, description = "Epoch number")
    ),
    responses(
        (status = 200, description = "Checkpoint verification report", body = CheckpointVerifyResponse),
        (status = 404, description = "Checkpoint not found", body = ErrorResponse),
        (status = 400, description = "Verification failed", body = ErrorResponse)
    )
)]
pub async fn verify_job_checkpoint(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(path): Path<JobCheckpointPath>,
) -> ApiResult<CheckpointVerifyResponse> {
    require_permission(&claims, Permission::TrainingView)?;

    let var_dir = std::env::var("AOS_VAR_DIR").unwrap_or_else(|_| "var".to_string());
    let adapters_dir = std::path::PathBuf::from(&var_dir).join("adapters");

    let ckpt_filename = format!("{}_epoch_{:04}.ckpt", path.job_id, path.epoch);
    let ckpt_path = adapters_dir.join(&ckpt_filename);

    if !ckpt_path.exists() {
        return Err(ApiError::not_found("Checkpoint").with_details(format!(
            "Checkpoint file not found: {}",
            ckpt_path.display()
        )));
    }

    let report = verify_checkpoint_file(&ckpt_path).map_err(|e| {
        ApiError::bad_request("checkpoint verification failed").with_details(e.to_string())
    })?;

    Ok(Json(CheckpointVerifyResponse {
        blake3_hash: report.blake3_hash.to_hex(),
        signer_key_id: report.signer_key_id,
        signed_at: report.signed_at,
        schema_version: report.schema_version,
        verified: true,
    }))
}
