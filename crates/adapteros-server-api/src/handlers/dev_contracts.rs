//! Dev-only contract sample endpoint.
//!
//! Serves fully expanded JSON contract examples for inference, trace, and
//! evidence payloads. The payloads are loaded from `docs/contracts/*.json`,
//! lightly validated, and redacted to strip any prompt or PII fields before
//! returning to the UI.

use crate::state::AppState;
use crate::types::ErrorResponse;
use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use tracing::warn;

const DEFAULT_SAMPLE_DIR: &str = "docs/contracts";

/// Response envelope used by the dev contract viewer.
#[derive(Debug, Serialize)]
pub struct ContractSamplesResponse {
    pub inference: Value,
    pub trace: Value,
    pub evidence: Value,
}

/// Lightweight validation structs that mirror the UI contract types. These keep
/// the samples honest without requiring the full backend schema.
#[derive(Debug, Serialize, Deserialize)]
struct UiRunReceipt {
    trace_id: String,
    run_head_hash: String,
    output_digest: String,
    receipt_digest: String,
    #[serde(default)]
    signature: Option<String>,
    #[serde(default)]
    attestation: Option<String>,
    #[serde(default)]
    logical_prompt_tokens: u32,
    #[serde(default)]
    prefix_cached_token_count: u32,
    #[serde(default)]
    billed_input_tokens: u32,
    #[serde(default)]
    logical_output_tokens: u32,
    #[serde(default)]
    billed_output_tokens: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct UiInferResponse {
    schema_version: String,
    id: String,
    text: String,
    tokens_generated: u64,
    latency_ms: u64,
    adapters_used: Vec<String>,
    finish_reason: String,
    #[serde(default)]
    run_receipt: Option<UiRunReceipt>,
}

#[derive(Debug, Serialize, Deserialize)]
struct UiTraceSpan {
    span_id: String,
    name: String,
    start_time: String,
    end_time: String,
    #[serde(default)]
    attributes: Option<Value>,
    trace_id: String,
    parent_id: String,
    start_ns: u64,
    end_ns: u64,
    status: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct UiTrace {
    trace_id: String,
    root_span_id: String,
    spans: Vec<UiTraceSpan>,
}

#[derive(Debug, Serialize, Deserialize)]
struct UiEvidenceEntry {
    id: String,
    #[serde(default)]
    dataset_id: Option<String>,
    #[serde(default)]
    adapter_id: Option<String>,
    evidence_type: String,
    reference: String,
    #[serde(default)]
    description: Option<String>,
    confidence: String,
    #[serde(default)]
    created_by: Option<String>,
    created_at: String,
    #[serde(default)]
    metadata_json: Option<String>,
}

fn sample_dir() -> PathBuf {
    std::env::var("AOS_CONTRACT_SAMPLE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_SAMPLE_DIR))
}

fn redact_value(value: &mut Value) {
    match value {
        Value::Object(map) => {
            // Remove sensitive fields
            const STRIP_KEYS: &[&str] = &[
                "prompt",
                "prompt_text",
                "raw_prompt",
                "messages",
                "user",
                "email",
                "ip",
                "auth_token",
            ];

            for key in STRIP_KEYS {
                map.remove(*key);
            }

            for (_, v) in map.iter_mut() {
                redact_value(v);
            }
        }
        Value::Array(values) => {
            for v in values {
                redact_value(v);
            }
        }
        _ => {}
    }
}

async fn load_and_validate<T: for<'a> Deserialize<'a> + Serialize>(
    path: &Path,
    label: &str,
) -> Result<Value, (StatusCode, Json<ErrorResponse>)> {
    let contents = tokio::fs::read_to_string(path).await.map_err(|e| {
        warn!(%label, path = %path.display(), error = ?e, "Contract sample missing");
        (
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new(format!("{label} sample not found"))
                    .with_code("NOT_FOUND")
                    .with_string_details(path.display().to_string()),
            ),
        )
    })?;

    let parsed: T = serde_json::from_str(&contents).map_err(|e| {
        warn!(%label, path = %path.display(), error = ?e, "Contract sample failed validation");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new(format!("{label} sample failed validation"))
                    .with_code("INVALID_CONTRACT_SAMPLE")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let mut value = serde_json::to_value(parsed).map_err(|e| {
        warn!(%label, path = %path.display(), error = ?e, "Failed to serialize contract sample");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new(format!("{label} sample serialization failed"))
                    .with_code("SERIALIZATION_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    redact_value(&mut value);
    Ok(value)
}

pub async fn get_contract_samples(
    State(_state): State<AppState>,
) -> Result<Json<ContractSamplesResponse>, (StatusCode, Json<ErrorResponse>)> {
    let dir = sample_dir();

    let inference_path = dir.join("infer-response.json");
    let trace_path = dir.join("trace-response.json");
    let evidence_path = dir.join("evidence-list.json");

    let inference = load_and_validate::<UiInferResponse>(&inference_path, "inference").await?;
    let trace = load_and_validate::<UiTrace>(&trace_path, "trace").await?;
    let evidence_list =
        load_and_validate::<Vec<UiEvidenceEntry>>(&evidence_path, "evidence").await?;

    Ok(Json(ContractSamplesResponse {
        inference,
        trace,
        evidence: evidence_list,
    }))
}
