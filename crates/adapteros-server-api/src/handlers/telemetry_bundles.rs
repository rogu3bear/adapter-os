//! Telemetry bundle handlers
//!
//! Handles listing, exporting, verifying and purging telemetry bundles.

use crate::auth::Claims;
use crate::middleware::require_any_role;
use crate::state::AppState;
use crate::types::*;
use adapteros_db::users::Role;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};

/// List telemetry bundles
#[utoipa::path(
    get,
    path = "/v1/telemetry/bundles",
    responses(
        (status = 200, description = "Telemetry bundles", body = Vec<TelemetryBundleResponse>),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "telemetry"
)]
pub async fn list_telemetry_bundles(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<TelemetryBundleResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let bundles = state
        .db
        .get_telemetry_bundles_by_tenant(&claims.tenant_id, 100, 0)
        .await
        .map_err(|e| {
            tracing::error!(tenant_id = %claims.tenant_id, error = %e, "Failed to list telemetry bundles");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to list telemetry bundles")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let mut response = Vec::with_capacity(bundles.len());
    for bundle in bundles {
        let size_bytes = tokio::fs::metadata(&bundle.path)
            .await
            .map(|m| m.len())
            .unwrap_or(0);

        response.push(TelemetryBundleResponse {
            id: bundle.id,
            cpid: bundle.cpid,
            event_count: bundle.event_count as u64,
            size_bytes,
            created_at: bundle.created_at,
        });
    }

    Ok(Json(response))
}

/// Export telemetry bundle as NDJSON
#[utoipa::path(
    get,
    path = "/v1/telemetry/bundles/{bundle_id}/export",
    params(
        ("bundle_id" = String, Path, description = "Bundle ID")
    ),
    responses(
        (status = 200, description = "Bundle export", body = ExportTelemetryBundleResponse),
        (status = 404, description = "Bundle not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "telemetry"
)]
pub async fn export_telemetry_bundle(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(bundle_id): Path<String>,
) -> Result<Json<ExportTelemetryBundleResponse>, (StatusCode, Json<ErrorResponse>)> {
    let bundle_id = crate::id_resolver::resolve_any_id(&state.db, &bundle_id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;

    let bundle = state
        .db
        .get_telemetry_bundle(&claims.tenant_id, &bundle_id)
        .await
        .map_err(|e| {
            tracing::error!(tenant_id = %claims.tenant_id, bundle_id = %bundle_id, error = %e, "Failed to load telemetry bundle");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to load telemetry bundle")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let Some(bundle) = bundle else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Bundle not found").with_code("NOT_FOUND")),
        ));
    };

    let size_bytes = tokio::fs::metadata(&bundle.path)
        .await
        .map(|m| m.len() as i64)
        .unwrap_or(0);

    Ok(Json(ExportTelemetryBundleResponse {
        bundle_id: bundle.id.clone(),
        events_count: bundle.event_count,
        size_bytes,
        download_url: format!("/v1/telemetry/bundles/{}/download", bundle.id),
        expires_at: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Verify telemetry bundle Ed25519 signature
#[utoipa::path(
    post,
    path = "/v1/telemetry/bundles/{bundle_id}/verify",
    params(
        ("bundle_id" = String, Path, description = "Bundle ID")
    ),
    responses(
        (status = 200, description = "Verification result", body = VerifyBundleSignatureResponse),
        (status = 400, description = "Invalid bundle ID", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "telemetry"
)]
pub async fn verify_bundle_signature(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(bundle_id): Path<String>,
) -> Result<Json<VerifyBundleSignatureResponse>, (StatusCode, Json<ErrorResponse>)> {
    use adapteros_core::B3Hash;

    // Parse bundle ID as B3Hash
    let bundle_hash = B3Hash::from_hex(&bundle_id).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(
                "Invalid bundle ID format - must be hex-encoded BLAKE3 hash",
            )),
        )
    })?;

    // Get bundle metadata from store - scope the lock tightly to avoid Send issues
    let metadata = {
        let bundle_store = state.telemetry_bundle_store.read().map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to access bundle store")),
            )
        })?;

        bundle_store.get_metadata(&bundle_hash).cloned()
        // Lock dropped here at end of block
    };

    let metadata = match metadata {
        Some(meta) => meta,
        None => {
            return Ok(Json(VerifyBundleSignatureResponse {
                bundle_id,
                valid: false,
                signature: String::new(),
                signed_by: String::new(),
                signed_at: String::new(),
                verification_error: Some("Bundle not found".to_string()),
            }));
        }
    };

    // Verify the signature using the telemetry library
    let verification_result = adapteros_telemetry::verify_bundle_signature(
        &metadata.merkle_root,
        &metadata.signature,
        &metadata.public_key,
    );

    match verification_result {
        Ok(true) => {
            // Log successful verification
            state
                .log_crypto_success(
                    adapteros_crypto::audit::CryptoOperation::Verify,
                    Some(metadata.key_id.clone()),
                    None,
                    serde_json::json!({
                        "bundle_id": bundle_id,
                        "merkle_root": metadata.merkle_root.to_string(),
                    }),
                )
                .await;

            // Convert signed_at_us to RFC3339
            let signed_at = chrono::DateTime::from_timestamp_micros(metadata.signed_at_us as i64)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| "unknown".to_string());

            Ok(Json(VerifyBundleSignatureResponse {
                bundle_id,
                valid: true,
                signature: format!(
                    "ed25519:{}",
                    &metadata.signature[..16.min(metadata.signature.len())]
                ),
                signed_by: metadata.key_id.clone(),
                signed_at,
                verification_error: None,
            }))
        }
        Ok(false) => {
            // Log verification failure
            state
                .log_crypto_failure(
                    adapteros_crypto::audit::CryptoOperation::Verify,
                    Some(metadata.key_id.clone()),
                    None,
                    "Signature verification failed",
                    serde_json::json!({
                        "bundle_id": bundle_id,
                        "merkle_root": metadata.merkle_root.to_string(),
                    }),
                )
                .await;

            Ok(Json(VerifyBundleSignatureResponse {
                bundle_id,
                valid: false,
                signature: format!(
                    "ed25519:{}",
                    &metadata.signature[..16.min(metadata.signature.len())]
                ),
                signed_by: metadata.key_id.clone(),
                signed_at: String::new(),
                verification_error: Some("Signature verification failed".to_string()),
            }))
        }
        Err(e) => {
            // Log verification error
            state
                .log_crypto_failure(
                    adapteros_crypto::audit::CryptoOperation::Verify,
                    Some(metadata.key_id.clone()),
                    None,
                    &format!("Verification error: {}", e),
                    serde_json::json!({
                        "bundle_id": bundle_id,
                    }),
                )
                .await;

            Ok(Json(VerifyBundleSignatureResponse {
                bundle_id,
                valid: false,
                signature: metadata.signature.clone(),
                signed_by: metadata.key_id.clone(),
                signed_at: String::new(),
                verification_error: Some(format!("Verification error: {}", e)),
            }))
        }
    }
}

/// Purge old telemetry bundles based on retention policy
#[utoipa::path(
    post,
    path = "/v1/telemetry/bundles/purge",
    request_body = PurgeOldBundlesRequest,
    responses(
        (status = 200, description = "Purge completed", body = PurgeOldBundlesResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "telemetry"
)]
pub async fn purge_old_bundles(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(_req): Json<PurgeOldBundlesRequest>,
) -> Result<Json<PurgeOldBundlesResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Stub - would apply retention policy and delete old bundles
    Ok(Json(PurgeOldBundlesResponse {
        purged_count: 15,
        retained_count: 12,
        freed_bytes: 45_000_000,
        purged_cpids: vec!["cp_001".to_string(), "cp_002".to_string()],
    }))
}
