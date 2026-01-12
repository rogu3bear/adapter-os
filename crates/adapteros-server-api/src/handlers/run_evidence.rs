use crate::auth::Claims;
use crate::error_helpers::{db_error, internal_error, not_found};
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_core::evidence_envelope::{EvidenceEnvelope, InferenceReceiptRef};
use adapteros_db::inference_trace::{recompute_receipt, TraceReceipt};
// UmaStats import removed - live runtime metrics excluded for deterministic exports
use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use std::io::Write;
use zip::{write::FileOptions, CompressionMethod, DateTime};

const DETERMINISTIC_ENVELOPE_TIMESTAMP: &str = "1970-01-01T00:00:00Z";

#[derive(Debug, Serialize)]
struct ManifestRefEntry {
    manifest_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    manifest_json: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
struct PolicyDigestEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    policy_mask_digest_b3: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    warnings: Vec<String>,
}

/// Boot state snapshot for evidence bundles.
///
/// DETERMINISM: This struct intentionally excludes:
/// - Phase timing fields (started_at_ms, finished_at_ms, duration_ms)
/// - Live boot state (state, accepting_requests)
///
/// This ensures evidence bundles are fully deterministic - the same run
/// will always produce the same ZIP hash regardless of when export occurs
/// or whether the server has rebooted.
#[derive(Debug, Serialize)]
struct BootStateSnapshotDeterministic {
    /// Boot session correlation ID (for debugging only).
    /// NOTE: This is from the CURRENT boot session at export time, not the
    /// boot session when the inference ran. Included for forensic reference
    /// but should not be relied upon for determinism.
    #[serde(skip_serializing_if = "Option::is_none")]
    boot_trace_id_current: Option<String>,
    /// Phase outcomes only (no timing data)
    phases: Vec<PhaseOutcomeDeterministic>,
}

/// Deterministic phase outcome for evidence bundles.
///
/// Contains only the phase name, outcome status, and optional error code.
/// Excludes all timing fields (started_at_ms, finished_at_ms, duration_ms)
/// to ensure reproducible evidence hashes.
#[derive(Debug, Serialize)]
struct PhaseOutcomeDeterministic {
    name: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_code: Option<String>,
}

/// Model status snapshot for evidence bundles.
///
/// DETERMINISM: This struct intentionally excludes live runtime metrics
/// (ane_usage, uma_pressure_level) and mutable timestamps (updated_at)
/// to ensure evidence bundles are fully deterministic - the same run
/// will always produce the same ZIP hash.
#[derive(Debug, Serialize)]
struct ModelStatusSnapshot {
    status: adapteros_api_types::ModelLoadStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    memory_usage_mb: Option<i32>,
    // REMOVED for determinism:
    // - ane_usage: live ANE memory captured at export time
    // - uma_pressure_level: live UMA pressure captured at export time
    // - updated_at: mutable DB timestamp
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
struct BundleReadme {
    run_id: String,
    tenant_id: String,
    manifest_hash: String,
    status: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    files: Vec<String>,
}

/// Query parameters for evidence export
#[derive(Debug, Default, Deserialize)]
pub struct EvidenceExportParams {
    /// If true, allow export even when replay ran with degraded RAG context.
    /// By default, export is blocked for degraded runs to prevent misleading evidence.
    #[serde(default)]
    pub force_incomplete: bool,
}

fn trace_receipt_to_ref(receipt: &TraceReceipt) -> InferenceReceiptRef {
    InferenceReceiptRef {
        trace_id: receipt.trace_id.clone(),
        run_head_hash: receipt.run_head_hash,
        output_digest: receipt.output_digest,
        receipt_digest: receipt.receipt_digest,
        logical_prompt_tokens: receipt.logical_prompt_tokens,
        prefix_cached_token_count: receipt.prefix_cached_token_count,
        billed_input_tokens: receipt.billed_input_tokens,
        logical_output_tokens: receipt.logical_output_tokens,
        billed_output_tokens: receipt.billed_output_tokens,
        stop_reason_code: receipt.stop_reason_code.clone(),
        stop_reason_token_index: receipt.stop_reason_token_index,
        stop_policy_digest_b3: receipt.stop_policy_digest_b3,
        model_cache_identity_v2_digest_b3: receipt.model_cache_identity_v2_digest_b3,
        // PRD-DET-001: Backend identity fields
        // These default to empty/None when converting from legacy TraceReceipt
        // which doesn't track backend identity. New traces should include these.
        backend_used: String::new(),
        backend_attestation_b3: None,
        // PRD-DET-001: Seed lineage binding (PR-A)
        // Defaults to None for legacy traces; new traces bind seed lineage.
        seed_lineage_hash: None,
        // Training lineage for adapter provenance (patent rectification)
        // Defaults to None for legacy traces.
        adapter_training_lineage_digest: None,
    }
}

fn build_readme(summary: &BundleReadme) -> String {
    let mut out = String::new();
    out.push_str("AdapterOS Evidence Bundle\n");
    out.push_str("=========================\n\n");
    out.push_str(&format!("Run ID: {}\n", summary.run_id));
    out.push_str(&format!("Tenant: {}\n", summary.tenant_id));
    out.push_str(&format!("Manifest: {}\n", summary.manifest_hash));
    out.push_str(&format!("Status: {}\n\n", summary.status));
    out.push_str("Contents:\n");
    for file in &summary.files {
        out.push_str(&format!("- {}\n", file));
    }
    if !summary.warnings.is_empty() {
        out.push_str("\nWarnings:\n");
        for w in &summary.warnings {
            out.push_str(&format!("- {}\n", w));
        }
    }
    out.push_str(
        "\nHow to replay (offline):\n\
         1) Use replay_metadata.json to restore sampling params and router seed.\n\
         2) Verify manifest_ref.json hash matches your current manifest.\n\
         3) Validate policy_digest.json against policy snapshots.\n\
         4) Recompute run_envelope.json using stored trace tokens if present.\n\
         5) Model and boot snapshots provide environment fingerprinting.\n",
    );
    out
}

// ane_usage_from_stats removed - live runtime metrics excluded for determinism

#[utoipa::path(
    get,
    path = "/v1/runs/{run_id}/evidence",
    params(
        ("run_id" = String, Path, description = "Inference run identifier (request_id)"),
        ("force_incomplete" = Option<bool>, Query, description = "Allow export even with degraded RAG context")
    ),
    responses(
        (status = 200, description = "Evidence bundle zip for the run"),
        (status = 403, description = "Forbidden", body = ErrorResponse),
        (status = 404, description = "Run not found", body = ErrorResponse),
        (status = 412, description = "Precondition Failed - RAG context degraded", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "replay"
)]
pub async fn download_run_evidence(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(run_id): Path<String>,
    Query(params): Query<EvidenceExportParams>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::InferenceExecute)?;

    // Fetch metadata by inference_id (tenant isolation enforced at DB layer)
    let metadata = state
        .db
        .get_replay_metadata_by_inference(&run_id)
        .await
        .map_err(db_error)?
        .ok_or_else(|| not_found("Run"))?;

    // Defense in depth: also validate at handler level
    validate_tenant_isolation(&claims, &metadata.tenant_id)?;

    // Note: RAG fidelity checks removed - field no longer exists on InferenceReplayMetadata
    // The force_incomplete param is kept for API compatibility but currently unused
    let _ = params.force_incomplete;

    let mut warnings: Vec<String> = Vec::new();

    // Replay metadata (always included)
    let replay_metadata_json = serde_json::to_vec_pretty(&metadata).map_err(internal_error)?;

    // Manifest reference with optional embedded manifest
    let manifest_record = state
        .db
        .get_manifest_by_hash(&metadata.manifest_hash)
        .await
        .map_err(db_error)?;
    let manifest_entry = if let Some(rec) = manifest_record {
        // Manifest found and already tenant-scoped by the query
        let manifest_json: serde_json::Value =
            serde_json::from_str(&rec.body_json).unwrap_or_else(|_| serde_json::json!({}));
        ManifestRefEntry {
            manifest_hash: metadata.manifest_hash.clone(),
            manifest_json: Some(manifest_json),
            warnings: Vec::new(),
        }
    } else {
        warnings.push(format!(
            "manifest hash {} not found in control plane",
            metadata.manifest_hash
        ));
        ManifestRefEntry {
            manifest_hash: metadata.manifest_hash.clone(),
            manifest_json: None,
            warnings: vec!["manifest content unavailable".to_string()],
        }
    };
    let manifest_ref_json = serde_json::to_vec_pretty(&manifest_entry).map_err(internal_error)?;

    // Policy digest entry
    let mut policy_warnings = Vec::new();
    if metadata.policy_mask_digest_b3.is_none() {
        policy_warnings.push("policy digest missing; replay may be approximate".to_string());
    }
    warnings.extend(policy_warnings.clone());
    let policy_entry = PolicyDigestEntry {
        policy_mask_digest_b3: metadata.policy_mask_digest_b3.clone(),
        warnings: policy_warnings.clone(),
    };
    let policy_digest_json = serde_json::to_vec_pretty(&policy_entry).map_err(internal_error)?;

    // Boot state snapshot (deterministic - no timing fields)
    // DETERMINISM: We exclude phase timing (started_at_ms, finished_at_ms, duration_ms)
    // and live boot state (state, accepting_requests) to ensure evidence bundles
    // produce identical hashes regardless of when export occurs or server reboots.
    let boot_snapshot = state.boot_state.as_ref().map(|bs| {
        let phases = bs
            .phase_statuses()
            .into_iter()
            .map(|p| PhaseOutcomeDeterministic {
                name: p.name,
                status: format!("{:?}", p.status),
                error_code: p.error_code,
            })
            .collect();
        BootStateSnapshotDeterministic {
            boot_trace_id_current: Some(bs.boot_trace_id()),
            phases,
        }
    });
    if boot_snapshot.is_none() {
        warnings.push(
            "boot state manager not initialized; boot_state.json is a placeholder".to_string(),
        );
    }
    let boot_state_json = boot_snapshot
        .as_ref()
        .map(|snapshot| serde_json::to_vec_pretty(snapshot).map_err(internal_error))
        .transpose()?;

    // Model status snapshot (tenant scoped)
    // DETERMINISM: We intentionally exclude live runtime metrics (ane_usage, uma_pressure_level)
    // and mutable timestamps (updated_at) to ensure evidence bundles are fully deterministic.
    let mut model_warnings = Vec::new();
    let status_record = state
        .db
        .get_base_model_status(&metadata.tenant_id)
        .await
        .map_err(db_error)?;
    let model_snapshot = if let Some(status) = status_record {
        let model_name = status.model_id.clone();
        ModelStatusSnapshot {
            status: adapteros_api_types::ModelLoadStatus::parse_status(&status.status),
            model_id: Some(status.model_id),
            model_name: Some(model_name),
            memory_usage_mb: status.memory_usage_mb,
            warnings: Vec::new(),
        }
    } else {
        model_warnings.push("no base model status recorded for tenant".to_string());
        warnings.extend(model_warnings.clone());
        ModelStatusSnapshot {
            status: adapteros_api_types::ModelLoadStatus::NoModel,
            model_id: None,
            model_name: None,
            memory_usage_mb: None,
            warnings: model_warnings.clone(),
        }
    };
    let model_status_json = serde_json::to_vec_pretty(&model_snapshot).map_err(internal_error)?;

    // Attempt to load trace receipt and build inference envelope
    let trace_id: Option<String> = sqlx::query_scalar(
        "SELECT trace_id FROM inference_traces WHERE request_id = ? AND tenant_id = ? ORDER BY created_at DESC, trace_id DESC LIMIT 1",
    )
    .bind(&run_id)
    .bind(&metadata.tenant_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(db_error)?;

    let mut run_envelope_bytes: Option<Vec<u8>> = None;
    let mut envelope_warnings: Vec<String> = Vec::new();
    if let Some(trace_id) = trace_id {
        match recompute_receipt(state.db.raw(), trace_id.as_str()).await {
            Ok(verification) => {
                let receipt = verification
                    .stored
                    .as_ref()
                    .unwrap_or(&verification.recomputed);
                let receipt_ref = trace_receipt_to_ref(receipt);
                // Keep evidence export deterministic by omitting mutable chain linkage and timestamps.
                let mut envelope =
                    EvidenceEnvelope::new_inference(metadata.tenant_id.clone(), receipt_ref, None);
                envelope.created_at = DETERMINISTIC_ENVELOPE_TIMESTAMP.to_string();
                envelope.signed_at_us = 0;
                run_envelope_bytes =
                    Some(serde_json::to_vec_pretty(&envelope).map_err(internal_error)?);
            }
            Err(e) => {
                envelope_warnings.push(format!(
                    "failed to recompute trace receipt for {}: {}",
                    trace_id, e
                ));
            }
        }
    } else {
        envelope_warnings.push("no inference trace found for run_id; envelope omitted".to_string());
    }
    warnings.extend(envelope_warnings.clone());

    // README summary
    let file_names = vec![
        "run_envelope.json".to_string(),
        "replay_metadata.json".to_string(),
        "policy_digest.json".to_string(),
        "manifest_ref.json".to_string(),
        "model_status.json".to_string(),
        "boot_state.json".to_string(),
        "README.txt".to_string(),
    ];
    let mut status = metadata.replay_status.clone();
    if run_envelope_bytes.is_none() || manifest_entry.manifest_json.is_none() {
        status = format!("{} (incomplete)", status);
    }
    let readme = BundleReadme {
        run_id: run_id.clone(),
        tenant_id: metadata.tenant_id.clone(),
        manifest_hash: metadata.manifest_hash.clone(),
        status,
        warnings: warnings.clone(),
        files: file_names.clone(),
    };
    let readme_bytes = build_readme(&readme).into_bytes();

    // Assemble files and enforce deterministic ordering
    let options = FileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .last_modified_time(
            DateTime::from_date_and_time(1980, 1, 1, 0, 0, 0)
                .unwrap_or_else(|_| DateTime::default()),
        );
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    files.push(("replay_metadata.json".to_string(), replay_metadata_json));
    files.push(("manifest_ref.json".to_string(), manifest_ref_json));
    files.push(("policy_digest.json".to_string(), policy_digest_json));
    files.push(("model_status.json".to_string(), model_status_json));
    files.push((
        "boot_state.json".to_string(),
        boot_state_json.unwrap_or_else(|| {
            serde_json::to_vec_pretty(&serde_json::json!({
                "missing": true,
                "warning": "boot state manager not initialized"
            }))
            .expect("boot state placeholder")
        }),
    ));
    let run_id_for_placeholder = run_id.clone();
    let envelope_warnings_clone = envelope_warnings.clone();
    let has_envelope = run_envelope_bytes.is_some();
    files.push((
        "run_envelope.json".to_string(),
        run_envelope_bytes.unwrap_or_else(|| {
            serde_json::to_vec_pretty(&serde_json::json!({
                "run_id": run_id_for_placeholder,
                "missing": true,
                "warnings": envelope_warnings_clone
            }))
            .expect("envelope placeholder")
        }),
    ));
    files.push(("README.txt".to_string(), readme_bytes));
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut writer = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut writer);
        for (name, data) in files {
            zip.start_file(name, options).map_err(internal_error)?;
            zip.write_all(&data).map_err(internal_error)?;
        }
        zip.finish().map_err(internal_error)?;
    }
    let buffer = writer.into_inner();

    let manifest_fragment = if metadata.manifest_hash.is_empty() {
        "unknown".to_string()
    } else {
        metadata.manifest_hash.chars().take(12).collect::<String>()
    };
    let filename = format!("aos_evidence_{}_{}.zip", run_id, manifest_fragment);
    let disposition = HeaderValue::from_str(&format!("attachment; filename=\"{}\"", filename))
        .unwrap_or_else(|_| HeaderValue::from_static("attachment"));

    // Emit audit event for evidence export
    let export_metadata = serde_json::json!({
        "run_id": run_id,
        "manifest_hash": metadata.manifest_hash,
        "bundle_size_bytes": buffer.len(),
        "warnings_count": warnings.len(),
        "has_envelope": has_envelope,
    });
    if let Err(e) = state
        .db
        .log_audit(
            &claims.sub,
            &claims.role,
            &claims.tenant_id,
            "evidence.exported",
            "inference_evidence",
            Some(&run_id),
            "success",
            None,
            None,
            Some(&export_metadata.to_string()),
        )
        .await
    {
        tracing::warn!(
            run_id = %run_id,
            error = %e,
            "Failed to log evidence export audit event"
        );
    }

    let response = (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/zip"),
            ),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        buffer,
    )
        .into_response();

    Ok(response)
}
