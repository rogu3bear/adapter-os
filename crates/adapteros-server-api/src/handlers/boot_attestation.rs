//! Boot attestation API handlers.
//!
//! Provides endpoints for retrieving and verifying boot attestations,
//! which are cryptographic proofs of the boot sequence.
//!
//! ## Security
//!
//! The verification endpoint includes replay protection:
//! - Timestamp validation: requests must include a timestamp within 5 minutes of current time
//! - Public key validation: unrecognized keys are logged as warnings
//!
//! This prevents attackers from replaying old verification requests.

use crate::api_error::{ApiError, ApiResult};
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use adapteros_boot::BootAttestation;
use axum::extract::{Extension, State};
use axum::Json;
use serde::{Deserialize, Serialize};

/// Maximum age of a verification request in seconds (5 minutes).
/// Requests with timestamps older than this are rejected to prevent replay attacks.
/// This matches the key update max age constant for consistency.
pub const ATTESTATION_VERIFY_MAX_AGE_SECS: i64 = 300;

/// Response for GET /v1/system/boot-attestation
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct BootAttestationResponse {
    /// Schema version
    pub schema_version: u8,

    /// Boot ID (unique identifier for this boot)
    pub boot_id: String,

    /// Merkle root of all phase evidence (hex encoded)
    pub merkle_root: String,

    /// Number of phases that completed
    pub phase_count: u32,

    /// Total boot time in milliseconds
    pub total_boot_time_ms: u64,

    /// Whether boot completed successfully
    pub boot_successful: bool,

    /// Number of invariant checks passed
    pub checks_passed: u32,

    /// Number of invariant checks failed
    pub checks_failed: u32,

    /// Whether the attestation is signed
    pub is_signed: bool,

    /// Key ID used for signing (if signed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_id: Option<String>,

    /// Ed25519 signature (hex encoded, if signed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,

    /// Public key used for signing (hex encoded, if signed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,

    /// Timestamp when attestation was created (microseconds since epoch)
    pub attested_at_us: u64,

    /// Git commit hash (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_commit: Option<String>,

    /// Build timestamp (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_timestamp: Option<u64>,
}

impl From<&BootAttestation> for BootAttestationResponse {
    fn from(attestation: &BootAttestation) -> Self {
        let is_signed = !attestation.signature.is_empty();

        Self {
            schema_version: attestation.schema_version,
            boot_id: attestation.boot_id.clone(),
            merkle_root: attestation.merkle_root.to_hex(),
            phase_count: attestation.phase_count,
            total_boot_time_ms: attestation.total_boot_time_ms,
            boot_successful: attestation.boot_successful,
            checks_passed: attestation.checks_passed,
            checks_failed: attestation.checks_failed,
            is_signed,
            key_id: if attestation.key_id.is_empty() {
                None
            } else {
                Some(attestation.key_id.clone())
            },
            signature: if attestation.signature.is_empty() {
                None
            } else {
                Some(attestation.signature.clone())
            },
            public_key: if attestation.public_key.is_empty() {
                None
            } else {
                Some(attestation.public_key.clone())
            },
            attested_at_us: attestation.attested_at_us,
            git_commit: attestation.git_commit.clone(),
            build_timestamp: attestation.build_timestamp,
        }
    }
}

/// Request body for POST /v1/system/verify-boot-attestation
#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct VerifyBootAttestationRequest {
    /// Merkle root to verify (hex encoded)
    pub merkle_root: String,

    /// Ed25519 signature (hex encoded)
    pub signature: String,

    /// Public key to verify against (hex encoded)
    pub public_key: String,

    /// Request timestamp in microseconds since Unix epoch (for replay protection).
    /// If not provided, the request will be rejected.
    /// Must be within ATTESTATION_VERIFY_MAX_AGE_SECS (5 minutes) of server time.
    pub timestamp_us: Option<u64>,
}

/// Response for POST /v1/system/verify-boot-attestation
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct VerifyBootAttestationResponse {
    /// Whether the signature is valid
    pub valid: bool,

    /// Error message if verification failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Get the boot attestation for this server instance.
///
/// Returns a cryptographic proof of the boot sequence including phase timings,
/// invariant checks, and a signed Merkle root.
#[utoipa::path(
    get,
    path = "/v1/system/boot-attestation",
    responses(
        (status = 200, description = "Boot attestation", body = BootAttestationResponse),
        (status = 403, description = "Forbidden", body = crate::types::ErrorResponse),
        (status = 404, description = "No attestation available", body = crate::types::ErrorResponse)
    ),
    tag = "system"
)]
pub async fn get_boot_attestation(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> ApiResult<BootAttestationResponse> {
    require_permission(&claims, Permission::MetricsView)?;

    let attestation = state.boot_attestation.as_ref().ok_or_else(|| {
        ApiError::not_found("Boot attestation not available (boot may not have completed normally)")
    })?;

    Ok(Json(BootAttestationResponse::from(attestation.as_ref())))
}

/// Verify a boot attestation signature.
///
/// This endpoint allows verifying that a boot attestation was signed by
/// a specific public key, enabling remote attestation of boot integrity.
///
/// ## Replay Protection
///
/// Requests must include a `timestamp_us` field (microseconds since Unix epoch).
/// The timestamp must be within 5 minutes of the server's current time to prevent
/// replay attacks. Unrecognized public keys are logged as warnings.
#[utoipa::path(
    post,
    path = "/v1/system/verify-boot-attestation",
    request_body = VerifyBootAttestationRequest,
    responses(
        (status = 200, description = "Verification result", body = VerifyBootAttestationResponse),
        (status = 400, description = "Invalid request", body = crate::types::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::types::ErrorResponse)
    ),
    tag = "system"
)]
pub async fn verify_boot_attestation(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<VerifyBootAttestationRequest>,
) -> ApiResult<VerifyBootAttestationResponse> {
    require_permission(&claims, Permission::MetricsView)?;

    // Replay protection: validate timestamp freshness
    let timestamp_us = request.timestamp_us.ok_or_else(|| {
        ApiError::bad_request("Missing timestamp_us field (required for replay protection)")
    })?;

    let now_us = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or_else(|e| {
            tracing::error!("System time before UNIX epoch: {}", e);
            0
        });

    // Convert max age to microseconds
    let max_age_us = (ATTESTATION_VERIFY_MAX_AGE_SECS as u64) * 1_000_000;

    // Check if timestamp is too old (past)
    if timestamp_us < now_us.saturating_sub(max_age_us) {
        tracing::warn!(
            timestamp_us = timestamp_us,
            now_us = now_us,
            max_age_secs = ATTESTATION_VERIFY_MAX_AGE_SECS,
            "Boot attestation verification request timestamp too old (possible replay)"
        );
        return Err(ApiError::bad_request(format!(
            "Request timestamp too old: must be within {} seconds of server time",
            ATTESTATION_VERIFY_MAX_AGE_SECS
        )));
    }

    // Check if timestamp is too far in the future (clock skew protection)
    if timestamp_us > now_us.saturating_add(max_age_us) {
        tracing::warn!(
            timestamp_us = timestamp_us,
            now_us = now_us,
            max_age_secs = ATTESTATION_VERIFY_MAX_AGE_SECS,
            "Boot attestation verification request timestamp too far in future"
        );
        return Err(ApiError::bad_request(format!(
            "Request timestamp too far in future: must be within {} seconds of server time",
            ATTESTATION_VERIFY_MAX_AGE_SECS
        )));
    }

    // Decode hex inputs
    let merkle_root_bytes = hex::decode(&request.merkle_root)
        .map_err(|e| ApiError::bad_request(format!("Invalid merkle_root hex encoding: {}", e)))?;

    let signature_bytes = hex::decode(&request.signature)
        .map_err(|e| ApiError::bad_request(format!("Invalid signature hex encoding: {}", e)))?;

    let public_key_bytes = hex::decode(&request.public_key)
        .map_err(|e| ApiError::bad_request(format!("Invalid public_key hex encoding: {}", e)))?;

    // Verify signature length
    if signature_bytes.len() != 64 {
        return Ok(Json(VerifyBootAttestationResponse {
            valid: false,
            error: Some(format!(
                "Invalid signature length: expected 64 bytes, got {}",
                signature_bytes.len()
            )),
        }));
    }

    // Verify public key length
    if public_key_bytes.len() != 32 {
        return Ok(Json(VerifyBootAttestationResponse {
            valid: false,
            error: Some(format!(
                "Invalid public key length: expected 32 bytes, got {}",
                public_key_bytes.len()
            )),
        }));
    }

    // Parse public key
    let public_key_array: [u8; 32] = public_key_bytes
        .try_into()
        .map_err(|_| ApiError::bad_request("Public key must be exactly 32 bytes"))?;
    let public_key = match ed25519_dalek::VerifyingKey::from_bytes(&public_key_array) {
        Ok(key) => key,
        Err(e) => {
            return Ok(Json(VerifyBootAttestationResponse {
                valid: false,
                error: Some(format!("Invalid public key: {}", e)),
            }));
        }
    };

    // Log warning if public key is unrecognized (not matching server's boot attestation key)
    let is_known_key = state
        .boot_attestation
        .as_ref()
        .is_some_and(|attestation| attestation.public_key == request.public_key);

    if !is_known_key {
        tracing::warn!(
            public_key = %request.public_key,
            "Boot attestation verification with unrecognized public key"
        );
    }

    // Parse signature
    let signature_array: [u8; 64] = signature_bytes
        .try_into()
        .map_err(|_| ApiError::bad_request("Signature must be exactly 64 bytes"))?;
    let signature = ed25519_dalek::Signature::from_bytes(&signature_array);

    // Verify signature over merkle root
    use ed25519_dalek::Verifier;
    match public_key.verify(&merkle_root_bytes, &signature) {
        Ok(()) => {
            tracing::debug!(
                merkle_root = %request.merkle_root,
                is_known_key = is_known_key,
                "Boot attestation signature verified successfully"
            );
            Ok(Json(VerifyBootAttestationResponse {
                valid: true,
                error: None,
            }))
        }
        Err(e) => Ok(Json(VerifyBootAttestationResponse {
            valid: false,
            error: Some(format!("Signature verification failed: {}", e)),
        })),
    }
}
