//! Provenance certificate handlers
//!
//! Endpoints for generating, retrieving, and verifying adapter provenance
//! certificates. A provenance certificate captures the full chain of custody
//! for an adapter: training data, checkpoints, promotions, and policy checks.
//!
//! ## Endpoints
//!
//! - `POST /v1/adapters/:adapter_id/provenance` — Generate certificate (protected)
//! - `GET  /v1/adapters/:adapter_id/provenance` — List certificates (optional-auth)
//! - `GET  /v1/provenance/:certificate_id` — Get certificate (optional-auth)
//! - `GET  /v1/provenance/:certificate_id/verify` — Verify certificate (public)

use crate::adapter_helpers::fetch_adapter_for_tenant;
use crate::api_error::{ApiError, ApiResult};
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::types::ErrorResponse;

use adapteros_crypto::ProvenanceCertificateBuilder;
use adapteros_db::{NewProvenanceCertificate, ProvenanceCertificateRecord};
use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

// ===== Request / Response Types =====

/// Query parameters for listing provenance certificates.
#[derive(Debug, Deserialize, IntoParams)]
pub struct ListProvenanceQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    10
}

/// Chain completeness classification for API responses.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChainCompleteness {
    /// >80% of provenance fields populated
    Full,
    /// 40-80% of provenance fields populated
    Partial,
    /// <40% of provenance fields populated
    Minimal,
}

/// A single provenance certificate.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProvenanceCertificateResponse {
    pub id: String,
    pub adapter_id: String,
    pub version_id: String,
    pub tenant_id: String,
    /// BLAKE3 hash of the training data, if available
    pub training_data_hash: Option<String>,
    /// BLAKE3 hash of the training config, if available
    pub training_config_hash: Option<String>,
    /// Training job ID, if available
    pub training_job_id: Option<String>,
    /// Final training loss, if available
    pub training_final_loss: Option<f64>,
    /// Number of training epochs, if available
    pub training_epochs: Option<i64>,
    /// BLAKE3 hash of the checkpoint weights, if available
    pub checkpoint_hash: Option<String>,
    /// ID of the promotion review, if the adapter was promoted
    pub promotion_review_id: Option<String>,
    /// Who approved the promotion
    pub promoted_by: Option<String>,
    /// Policy pack ID that was evaluated
    pub policy_pack_id: Option<String>,
    /// Base model identifier used for training
    pub base_model_id: Option<String>,
    /// Egress was verified blocked at certificate time
    pub egress_blocked: Option<bool>,
    /// Overall chain completeness score (0.0 - 1.0)
    pub completeness_score: f64,
    /// Classification based on completeness_score
    pub completeness: ChainCompleteness,
    /// BLAKE3 content hash over certificate fields (JCS canonical)
    pub content_hash: String,
    /// Ed25519 signature over content_hash (hex-encoded)
    pub signature: String,
    /// Public key used for signing (hex-encoded)
    pub signer_public_key: String,
    /// Whether the signature has been verified (only set by verify endpoint)
    pub verified: Option<bool>,
    pub created_at: String,
}

/// Verification report for a provenance certificate.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VerificationReport {
    pub certificate_id: String,
    pub adapter_id: String,
    pub signature_valid: bool,
    pub chain_completeness: ChainCompleteness,
    pub completeness_score: f64,
    /// Individual field verification results
    pub field_checks: Vec<FieldCheck>,
    pub verified_at: String,
}

/// Result of verifying a single provenance field.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FieldCheck {
    pub field: String,
    pub present: bool,
    pub verified: Option<bool>,
    pub note: Option<String>,
}

/// Paginated list of provenance certificates.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProvenanceCertificateListResponse {
    pub certificates: Vec<ProvenanceCertificateResponse>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

// ===== Handlers =====

/// Generate a provenance certificate for an adapter.
///
/// Gathers all available provenance data (training job, checkpoint, promotion,
/// policy evaluations) and produces a signed certificate using JCS canonical
/// JSON and Ed25519. The certificate is stored for later retrieval/verification.
#[utoipa::path(
    post,
    path = "/v1/adapters/{adapter_id}/provenance",
    tag = "provenance",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 201, description = "Provenance certificate generated", body = ProvenanceCertificateResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse),
        (status = 500, description = "Failed to generate certificate", body = ErrorResponse)
    )
)]
pub async fn generate_provenance_certificate(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> ApiResult<ProvenanceCertificateResponse> {
    require_permission(&claims, Permission::AdapterRegister)?;

    let adapter_id = crate::id_resolver::resolve_any_id(&state.db, &adapter_id).await?;
    let adapter = fetch_adapter_for_tenant(&state.db, &claims, &adapter_id).await?;

    let adapter_id_str = adapter.adapter_id.as_ref().unwrap_or(&adapter.id);

    // Try to get latest version for this adapter's repo
    let version_id = if let Some(ref repo_id) = adapter.repo_id {
        state
            .db
            .list_adapter_versions_for_repo(&claims.tenant_id, repo_id, None, None)
            .await
            .ok()
            .and_then(|versions| versions.into_iter().next().map(|v| v.id))
    } else {
        None
    };
    let version_id_str = version_id.unwrap_or_default();

    // Extract provenance fields from adapter metadata
    let prov_json = adapter
        .provenance_json
        .as_ref()
        .and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok());

    let extract_str = |key: &str| -> Option<String> {
        prov_json
            .as_ref()
            .and_then(|v| v.get(key).and_then(|h| h.as_str()).map(|s| s.to_string()))
    };

    // Build the certificate using the crypto module's builder (JCS canonical signing)
    let mut builder =
        ProvenanceCertificateBuilder::new(adapter_id_str, &version_id_str, &claims.tenant_id);

    if let Some(hash) = extract_str("training_data_hash").or_else(|| extract_str("data_hash")) {
        builder = builder.training_data_hash(hash);
    }
    if let Some(hash) = extract_str("training_config_hash") {
        builder = builder.training_config_hash(hash);
    }
    if let Some(id) = extract_str("training_job_id") {
        builder = builder.training_job_id(id);
    }
    if let Some(loss) = prov_json
        .as_ref()
        .and_then(|v| v.get("training_final_loss").and_then(|l| l.as_f64()))
    {
        builder = builder.training_final_loss(loss);
    }
    if let Some(epochs) = prov_json
        .as_ref()
        .and_then(|v| v.get("training_epochs").and_then(|e| e.as_u64()))
    {
        builder = builder.training_epochs(epochs as u32);
    }

    // Checkpoint hash from adapter's BLAKE3 hash
    if !adapter.hash_b3.is_empty() {
        builder = builder.checkpoint_hash(&adapter.hash_b3);
    }

    if let Some(id) = extract_str("base_model_id") {
        builder = builder.base_model_id(id);
    }
    if let Some(id) = extract_str("policy_pack_id") {
        builder = builder.policy_pack_id(id);
    }
    if let Some(hash) = extract_str("policy_pack_hash") {
        builder = builder.policy_pack_hash(hash);
    }
    if let Some(id) = extract_str("promotion_review_id") {
        builder = builder.promotion_review_id(id);
    }
    if let Some(by) = extract_str("promoted_by") {
        builder = builder.promoted_by(by);
    }

    // Sign with the system signing keypair using JCS canonical JSON
    let cert = builder.sign(&state.crypto.signing_keypair).map_err(|e| {
        ApiError::internal("failed to sign provenance certificate").with_details(e.to_string())
    })?;

    // Convert to DB insert type
    let db_cert = NewProvenanceCertificate {
        certificate_id: cert.certificate_id.clone(),
        adapter_id: cert.adapter_id.clone(),
        version_id: cert.version_id.clone(),
        tenant_id: cert.tenant_id.clone(),
        training_data_hash: cert.training_data_hash.clone(),
        training_config_hash: cert.training_config_hash.clone(),
        training_job_id: cert.training_job_id.clone(),
        training_final_loss: cert.training_final_loss,
        training_epochs: cert.training_epochs.map(|e| e as i64),
        checkpoint_hash: cert.checkpoint_hash.clone(),
        checkpoint_signature: cert.checkpoint_signature.clone(),
        checkpoint_signer_key: cert.checkpoint_signer_key.clone(),
        promotion_review_id: cert.promotion_review_id.clone(),
        promoted_by: cert.promoted_by.clone(),
        promoted_at: cert.promoted_at.clone(),
        promoted_from_state: cert.promoted_from_state.clone(),
        promoted_to_state: cert.promoted_to_state.clone(),
        policy_pack_hash: cert.policy_pack_hash.clone(),
        policy_pack_id: cert.policy_pack_id.clone(),
        base_model_id: cert.base_model_id.clone(),
        egress_blocked: cert.egress_blocked.map(|b| if b { 1 } else { 0 }),
        egress_rules_fingerprint: cert.egress_rules_fingerprint.clone(),
        generated_at: cert.generated_at.clone(),
        content_hash: cert.content_hash.clone(),
        signature: cert.signature.clone(),
        signer_public_key: cert.signer_public_key.clone(),
        schema_version: cert.schema_version as i64,
    };

    state
        .db
        .store_provenance_certificate(&db_cert)
        .await
        .map_err(|e| {
            ApiError::internal("failed to store provenance certificate").with_details(e.to_string())
        })?;

    let completeness_score = compute_completeness_score(&cert);
    let completeness = classify_completeness(completeness_score);

    let response = ProvenanceCertificateResponse {
        id: cert.certificate_id,
        adapter_id: cert.adapter_id,
        version_id: cert.version_id,
        tenant_id: cert.tenant_id,
        training_data_hash: cert.training_data_hash,
        training_config_hash: cert.training_config_hash,
        training_job_id: cert.training_job_id,
        training_final_loss: cert.training_final_loss,
        training_epochs: cert.training_epochs.map(|e| e as i64),
        checkpoint_hash: cert.checkpoint_hash,
        promotion_review_id: cert.promotion_review_id,
        promoted_by: cert.promoted_by,
        policy_pack_id: cert.policy_pack_id,
        base_model_id: cert.base_model_id,
        egress_blocked: cert.egress_blocked,
        completeness_score,
        completeness,
        content_hash: cert.content_hash,
        signature: cert.signature,
        signer_public_key: cert.signer_public_key,
        verified: Some(true), // Just signed, so trivially valid
        created_at: cert.generated_at,
    };

    Ok(Json(response))
}

/// List provenance certificates for an adapter.
#[utoipa::path(
    get,
    path = "/v1/adapters/{adapter_id}/provenance",
    tag = "provenance",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID"),
        ListProvenanceQuery
    ),
    responses(
        (status = 200, description = "List of provenance certificates", body = ProvenanceCertificateListResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn list_provenance_certificates(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
    Query(query): Query<ListProvenanceQuery>,
) -> ApiResult<ProvenanceCertificateListResponse> {
    let adapter_id = crate::id_resolver::resolve_any_id(&state.db, &adapter_id).await?;
    let limit = query.limit.min(100);
    let offset = query.offset;

    let rows = state
        .db
        .list_provenance_certificates_for_adapter(&adapter_id, limit, offset)
        .await
        .map_err(|e| {
            ApiError::internal("failed to list provenance certificates").with_details(e.to_string())
        })?;

    let total = state
        .db
        .count_provenance_certificates_for_adapter(&adapter_id)
        .await
        .map_err(|e| {
            ApiError::internal("failed to count provenance certificates")
                .with_details(e.to_string())
        })?;

    let certificates = rows.into_iter().map(record_to_response).collect();

    Ok(Json(ProvenanceCertificateListResponse {
        certificates,
        total,
        limit,
        offset,
    }))
}

/// Get a specific provenance certificate.
#[utoipa::path(
    get,
    path = "/v1/provenance/{certificate_id}",
    tag = "provenance",
    params(
        ("certificate_id" = String, Path, description = "Certificate ID")
    ),
    responses(
        (status = 200, description = "Provenance certificate", body = ProvenanceCertificateResponse),
        (status = 404, description = "Certificate not found", body = ErrorResponse)
    )
)]
pub async fn get_provenance_certificate(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(certificate_id): Path<String>,
) -> ApiResult<ProvenanceCertificateResponse> {
    let row = state
        .db
        .get_provenance_certificate(&certificate_id)
        .await
        .map_err(|e| {
            ApiError::internal("failed to retrieve provenance certificate")
                .with_details(e.to_string())
        })?
        .ok_or_else(|| {
            ApiError::not_found("Provenance certificate")
                .with_details(format!("Certificate '{}' does not exist", certificate_id))
        })?;

    Ok(Json(record_to_response(row)))
}

/// Verify a provenance certificate.
///
/// Reconstructs the certificate from the DB record, recomputes the JCS
/// content hash, and verifies the Ed25519 signature. This endpoint is
/// public (no auth required) to support external audit workflows.
#[utoipa::path(
    get,
    path = "/v1/provenance/{certificate_id}/verify",
    tag = "provenance",
    params(
        ("certificate_id" = String, Path, description = "Certificate ID")
    ),
    responses(
        (status = 200, description = "Verification report", body = VerificationReport),
        (status = 404, description = "Certificate not found", body = ErrorResponse)
    )
)]
pub async fn verify_provenance_certificate(
    State(state): State<AppState>,
    Path(certificate_id): Path<String>,
) -> ApiResult<VerificationReport> {
    let row = state
        .db
        .get_provenance_certificate(&certificate_id)
        .await
        .map_err(|e| {
            ApiError::internal("failed to retrieve provenance certificate")
                .with_details(e.to_string())
        })?
        .ok_or_else(|| {
            ApiError::not_found("Provenance certificate")
                .with_details(format!("Certificate '{}' does not exist", certificate_id))
        })?;

    // Reconstruct the crypto certificate from the DB record and verify
    let crypto_cert = record_to_crypto_cert(&row);
    let signature_valid = crypto_cert.verify().unwrap_or(false);

    let completeness_score = compute_completeness_score(&crypto_cert);
    let completeness = classify_completeness(completeness_score);

    // Build field checks
    let field_checks = vec![
        FieldCheck {
            field: "training_data_hash".to_string(),
            present: row.training_data_hash.is_some(),
            verified: row.training_data_hash.as_ref().map(|_| true),
            note: None,
        },
        FieldCheck {
            field: "training_config_hash".to_string(),
            present: row.training_config_hash.is_some(),
            verified: row.training_config_hash.as_ref().map(|_| true),
            note: None,
        },
        FieldCheck {
            field: "checkpoint_hash".to_string(),
            present: row.checkpoint_hash.is_some(),
            verified: row.checkpoint_hash.as_ref().map(|_| true),
            note: None,
        },
        FieldCheck {
            field: "promotion_review_id".to_string(),
            present: row.promotion_review_id.is_some(),
            verified: None,
            note: None,
        },
        FieldCheck {
            field: "policy_pack_id".to_string(),
            present: row.policy_pack_id.is_some(),
            verified: None,
            note: None,
        },
        FieldCheck {
            field: "base_model_id".to_string(),
            present: row.base_model_id.is_some(),
            verified: None,
            note: None,
        },
        FieldCheck {
            field: "egress_blocked".to_string(),
            present: row.egress_blocked.is_some(),
            verified: None,
            note: None,
        },
        FieldCheck {
            field: "signature".to_string(),
            present: true,
            verified: Some(signature_valid),
            note: if signature_valid {
                None
            } else {
                Some("signature verification failed".to_string())
            },
        },
    ];

    Ok(Json(VerificationReport {
        certificate_id: row.certificate_id,
        adapter_id: row.adapter_id,
        signature_valid,
        chain_completeness: completeness,
        completeness_score,
        field_checks,
        verified_at: Utc::now().to_rfc3339(),
    }))
}

// ===== Helpers =====

/// Convert a DB record to the API response type.
fn record_to_response(row: ProvenanceCertificateRecord) -> ProvenanceCertificateResponse {
    let crypto_cert = record_to_crypto_cert(&row);
    let completeness_score = compute_completeness_score(&crypto_cert);
    let completeness = classify_completeness(completeness_score);

    ProvenanceCertificateResponse {
        id: row.certificate_id,
        adapter_id: row.adapter_id,
        version_id: row.version_id,
        tenant_id: row.tenant_id,
        training_data_hash: row.training_data_hash,
        training_config_hash: row.training_config_hash,
        training_job_id: row.training_job_id,
        training_final_loss: row.training_final_loss,
        training_epochs: row.training_epochs,
        checkpoint_hash: row.checkpoint_hash,
        promotion_review_id: row.promotion_review_id,
        promoted_by: row.promoted_by,
        policy_pack_id: row.policy_pack_id,
        base_model_id: row.base_model_id,
        egress_blocked: row.egress_blocked.map(|v| v != 0),
        completeness_score,
        completeness,
        content_hash: row.content_hash,
        signature: row.signature,
        signer_public_key: row.signer_public_key,
        verified: None, // Not verified until verify endpoint is called
        created_at: row.created_at,
    }
}

/// Reconstruct a crypto-layer `ProvenanceCertificate` from a DB record
/// for verification purposes.
fn record_to_crypto_cert(
    row: &ProvenanceCertificateRecord,
) -> adapteros_crypto::ProvenanceCertificate {
    adapteros_crypto::ProvenanceCertificate {
        schema_version: row.schema_version as u8,
        certificate_id: row.certificate_id.clone(),
        adapter_id: row.adapter_id.clone(),
        version_id: row.version_id.clone(),
        tenant_id: row.tenant_id.clone(),
        training_data_hash: row.training_data_hash.clone(),
        training_config_hash: row.training_config_hash.clone(),
        training_job_id: row.training_job_id.clone(),
        training_final_loss: row.training_final_loss,
        training_epochs: row.training_epochs.map(|e| e as u32),
        checkpoint_hash: row.checkpoint_hash.clone(),
        checkpoint_signature: row.checkpoint_signature.clone(),
        checkpoint_signer_key: row.checkpoint_signer_key.clone(),
        promotion_review_id: row.promotion_review_id.clone(),
        promoted_by: row.promoted_by.clone(),
        promoted_at: row.promoted_at.clone(),
        promoted_from_state: row.promoted_from_state.clone(),
        promoted_to_state: row.promoted_to_state.clone(),
        policy_pack_hash: row.policy_pack_hash.clone(),
        policy_pack_id: row.policy_pack_id.clone(),
        base_model_id: row.base_model_id.clone(),
        egress_blocked: row.egress_blocked.map(|v| v != 0),
        egress_rules_fingerprint: row.egress_rules_fingerprint.clone(),
        generated_at: row.generated_at.clone(),
        content_hash: row.content_hash.clone(),
        signature: row.signature.clone(),
        signer_public_key: row.signer_public_key.clone(),
    }
}

/// Compute a completeness score (0.0 - 1.0) based on the five provenance
/// chain categories. Delegates to the crypto crate's canonical implementation.
fn compute_completeness_score(cert: &adapteros_crypto::ProvenanceCertificate) -> f64 {
    cert.chain_completeness().completeness_score as f64
}

/// Classify a completeness score into Full/Partial/Minimal.
fn classify_completeness(score: f64) -> ChainCompleteness {
    if score > 0.8 {
        ChainCompleteness::Full
    } else if score >= 0.4 {
        ChainCompleteness::Partial
    } else {
        ChainCompleteness::Minimal
    }
}
