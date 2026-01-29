//! AdapterOS receipt retrieval and replay verification endpoints.

use crate::api_error::{ApiError, ApiResult};
use crate::auth::Claims;
use crate::handlers::replay::{
    build_receipt_verification_result, verify_bundle_bytes, ReceiptVerificationResult,
};
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_api_types::inference::{RunReceipt, StopReasonCode};
use adapteros_core::{AosError, B3Hash};
use axum::{
    extract::{Path, State},
    Extension, Json,
};
use base64::Engine as _;
use serde::Deserialize;
use std::str::FromStr;
use utoipa::ToSchema;

#[derive(Debug, sqlx::FromRow)]
struct ReceiptRow {
    trace_id: String,
    tenant_id: String,
    run_head_hash: Vec<u8>,
    output_digest: Vec<u8>,
    receipt_digest: Vec<u8>,
    signature: Option<Vec<u8>>,
    attestation: Option<Vec<u8>>,
    logical_prompt_tokens: i64,
    prefix_cached_token_count: i64,
    billed_input_tokens: i64,
    logical_output_tokens: i64,
    billed_output_tokens: i64,
    stop_reason_code: Option<String>,
    stop_reason_token_index: Option<i64>,
    stop_policy_digest_b3: Option<Vec<u8>>,
    tenant_kv_quota_bytes: i64,
    tenant_kv_bytes_used: i64,
    kv_evictions: i64,
    kv_residency_policy_id: Option<String>,
    kv_quota_enforced: i64,
    prefix_kv_key_b3: Option<String>,
    prefix_cache_hit: i64,
    prefix_kv_bytes: i64,
    model_cache_identity_v2_digest_b3: Option<Vec<u8>>,
    previous_receipt_digest: Option<Vec<u8>>,
    session_sequence: Option<i64>,
}

fn parse_digest(label: &str, bytes: Vec<u8>) -> Result<B3Hash, ApiError> {
    if bytes.len() != 32 {
        return Err(ApiError::internal(format!(
            "Invalid {label} length: {}",
            bytes.len()
        )));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(B3Hash::from_bytes(arr))
}

fn parse_digest_opt(label: &str, bytes: Option<Vec<u8>>) -> Result<Option<B3Hash>, ApiError> {
    match bytes {
        Some(bytes) => Ok(Some(parse_digest(label, bytes)?)),
        None => Ok(None),
    }
}

fn parse_digest_hex_opt(label: &str, hex: Option<String>) -> Result<Option<B3Hash>, ApiError> {
    match hex {
        Some(hex) => B3Hash::from_hex(&hex)
            .map(Some)
            .map_err(|e| ApiError::internal(format!("Invalid {label} hex: {e}"))),
        None => Ok(None),
    }
}

#[utoipa::path(
    get,
    path = "/v1/adapteros/receipts/{digest}",
    params(
        ("digest" = String, Path, description = "Receipt digest (hex)")
    ),
    responses(
        (status = 200, description = "Stored receipt", body = RunReceipt),
        (status = 400, description = "Invalid digest", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse),
        (status = 404, description = "Receipt not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "replay"
)]
pub async fn get_receipt_by_digest(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(digest_hex): Path<String>,
) -> ApiResult<RunReceipt> {
    require_permission(&claims, Permission::InferenceExecute)?;

    let digest =
        B3Hash::from_hex(&digest_hex).map_err(|e| ApiError::bad_request(e.to_string()))?;

    let row = sqlx::query_as::<_, ReceiptRow>(
        r#"
        SELECT
            r.trace_id,
            t.tenant_id,
            r.run_head_hash,
            r.output_digest,
            r.receipt_digest,
            r.signature,
            r.attestation,
            r.logical_prompt_tokens,
            r.prefix_cached_token_count,
            r.billed_input_tokens,
            r.logical_output_tokens,
            r.billed_output_tokens,
            r.stop_reason_code,
            r.stop_reason_token_index,
            r.stop_policy_digest_b3,
            r.tenant_kv_quota_bytes,
            r.tenant_kv_bytes_used,
            r.kv_evictions,
            r.kv_residency_policy_id,
            r.kv_quota_enforced,
            r.prefix_kv_key_b3,
            r.prefix_cache_hit,
            r.prefix_kv_bytes,
            r.model_cache_identity_v2_digest_b3,
            r.previous_receipt_digest,
            r.session_sequence
        FROM inference_trace_receipts r
        JOIN inference_traces t ON r.trace_id = t.trace_id
        WHERE r.receipt_digest = ? OR r.crypto_receipt_digest_b3 = ?
        LIMIT 1
        "#,
    )
    .bind(&digest.as_bytes()[..])
    .bind(&digest.as_bytes()[..])
    .fetch_optional(state.db.pool())
    .await
    .map_err(ApiError::db_error)?
    .ok_or_else(|| ApiError::not_found("Receipt"))?;

    validate_tenant_isolation(&claims, &row.tenant_id)?;

    let stop_reason_code = row
        .stop_reason_code
        .as_deref()
        .and_then(|code| StopReasonCode::from_str(code).ok());

    let run_receipt = RunReceipt {
        trace_id: row.trace_id,
        run_head_hash: parse_digest("run_head_hash", row.run_head_hash)?,
        output_digest: parse_digest("output_digest", row.output_digest)?,
        receipt_digest: parse_digest("receipt_digest", row.receipt_digest)?,
        signature: row
            .signature
            .map(|s| base64::engine::general_purpose::STANDARD.encode(s)),
        attestation: row
            .attestation
            .map(|s| base64::engine::general_purpose::STANDARD.encode(s)),
        logical_prompt_tokens: row.logical_prompt_tokens as u32,
        prefix_cached_token_count: row.prefix_cached_token_count as u32,
        billed_input_tokens: row.billed_input_tokens as u32,
        logical_output_tokens: row.logical_output_tokens as u32,
        billed_output_tokens: row.billed_output_tokens as u32,
        stop_reason_code,
        stop_reason_token_index: row.stop_reason_token_index.map(|v| v as u32),
        stop_policy_digest_b3: parse_digest_opt("stop_policy_digest_b3", row.stop_policy_digest_b3)?,
        tenant_kv_quota_bytes: row.tenant_kv_quota_bytes as u64,
        tenant_kv_bytes_used: row.tenant_kv_bytes_used as u64,
        kv_evictions: row.kv_evictions as u32,
        kv_residency_policy_id: row.kv_residency_policy_id,
        kv_quota_enforced: row.kv_quota_enforced != 0,
        prefix_kv_key_b3: parse_digest_hex_opt("prefix_kv_key_b3", row.prefix_kv_key_b3)?,
        prefix_cache_hit: row.prefix_cache_hit != 0,
        prefix_kv_bytes: row.prefix_kv_bytes as u64,
        model_cache_identity_v2_digest_b3: parse_digest_opt(
            "model_cache_identity_v2_digest_b3",
            row.model_cache_identity_v2_digest_b3,
        )?,
        previous_receipt_digest: parse_digest_opt(
            "previous_receipt_digest",
            row.previous_receipt_digest,
        )?,
        session_sequence: row.session_sequence.unwrap_or(0) as u64,
    };

    Ok(Json(run_receipt))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct AdapterosReplayRequest {
    /// Receipt digest to verify against stored trace data.
    #[serde(default, alias = "digest", alias = "receipt_digest_hex")]
    pub receipt_digest: Option<String>,
    /// Optional receipt bundle payload (JSON).
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
}

#[utoipa::path(
    post,
    path = "/v1/adapteros/replay",
    request_body = AdapterosReplayRequest,
    responses(
        (status = 200, description = "Receipt verification result", body = ReceiptVerificationResult),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse),
        (status = 404, description = "Receipt or trace not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "replay"
)]
pub async fn adapteros_replay(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<AdapterosReplayRequest>,
) -> ApiResult<ReceiptVerificationResult> {
    require_permission(&claims, Permission::ReplayManage)?;

    if let Some(payload) = req.payload {
        let bytes = serde_json::to_vec(&payload)
            .map_err(|e| ApiError::bad_request(format!("Invalid payload JSON: {e}")))?;
        let report =
            verify_bundle_bytes(&bytes).map_err(|e| ApiError::bad_request(e.to_string()))?;
        if let Some(tenant) = report.tenant_id.as_ref() {
            validate_tenant_isolation(&claims, tenant)?;
        }
        return Ok(Json(report));
    }

    let digest_hex = req
        .receipt_digest
        .ok_or_else(|| ApiError::bad_request("Must provide receipt_digest or payload"))?;
    let digest =
        B3Hash::from_hex(&digest_hex).map_err(|e| ApiError::bad_request(e.to_string()))?;

    let (trace_id, tenant_id) = adapteros_db::find_trace_by_receipt_digest(&state.db, &digest)
        .await
        .map_err(ApiError::db_error)?
        .ok_or_else(|| ApiError::not_found("Receipt"))?;

    validate_tenant_isolation(&claims, &tenant_id)?;

    let verification = adapteros_db::recompute_receipt(&state.db, &trace_id)
        .await
        .map_err(|e| match e {
            AosError::NotFound(_) => ApiError::not_found("Inference trace"),
            AosError::Database(_) => ApiError::db_error(e),
            _ => ApiError::internal("Failed to verify trace receipt").with_details(e.to_string()),
        })?;

    Ok(Json(build_receipt_verification_result(
        trace_id,
        verification,
        "receipt_digest",
    )))
}
