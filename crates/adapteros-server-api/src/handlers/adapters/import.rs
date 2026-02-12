// Adapter Import Handler
//
// This module provides REST API endpoints for:
// - Importing adapters from .aos files

use super::fs_utils::write_temp_bundle;
use super::hashing::hash_multi_bytes;
use super::paths::resolve_adapter_roots;
use super::progress::emit_adapter_progress;
use super::repo::{map_repo_error, AdapterRepo, DefaultAdapterRepo, StoreBundleRequest};
use crate::audit_helper::{actions, log_success_or_warn, resources};
use crate::auth::Claims;
use crate::ip_extraction::ClientIp;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::types::{AdapterResponse, ErrorResponse, LifecycleState};
use crate::validation::validate_adapter_id;
use adapteros_db::AdapterRegistrationBuilder;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    Extension,
};
use std::collections::HashMap;
use tokio::io::AsyncWriteExt;
use tracing::{error, info, warn};

/// Maximum adapter file size (500 MB)
const MAX_ADAPTER_SIZE: u64 = 500 * 1024 * 1024;

/// Known manifest fields - reject import if unknown fields are present
/// This list must be explicitly maintained to catch schema drift
const KNOWN_MANIFEST_FIELDS: &[&str] = &[
    // Core identity
    "adapter_id",
    "name",
    "version",
    "schema_version",
    // Classification
    "scope",
    "category",
    "tier",
    // Technical
    "rank",
    "alpha",
    "targets",
    "backend_family",
    "base_model",
    "base_model_hash",
    // Hashes
    "weights_hash",
    "content_hash",
    "manifest_hash",
    "integrity_hash",
    // Metadata container (nested fields allowed)
    "metadata",
    "description",
    "intent",
    // Source tracking
    "framework",
    "framework_version",
    "repo_id",
    "commit_sha",
    // Security
    "signature",
    "signed_by",
    // Training and determinism metadata
    "created_at",
    "training_config",
    "training_backend",
    "determinism",
    "per_layer_hashes",
    "quantization",
    "gate_q15_denominator",
    "kernel_version",
    "synthetic_mode",
    "data_lineage_mode",
    "recommended_for_moe",
    "coreml_placement",
    // Optional extension fields (allowed but not required)
    "tags",
    "labels",
    "annotations",
    "custom",
];

// ============================================================================
// Handlers
// ============================================================================

/// Import an adapter from an uploaded .aos file
///
/// # Request
/// - Multipart form with a file field named "file"
/// - Optional query param `load=true` to auto-load after import
///
/// # Response
/// Returns the registered adapter details
///
/// # Features
/// - **Streaming upload**: Writes to temp file during upload, avoiding memory pressure
/// - **Deduplication**: Returns existing adapter if hash matches (with `deduplicated: true`)
/// - **Transactional safety**: Temp file + atomic rename, rollback on failure
/// - **Auto-load**: Registers with lifecycle manager when `load=true`
///
/// # Example
/// ```
/// POST /v1/adapters/import?load=true
/// Content-Type: multipart/form-data
///
/// file: <.aos file binary>
/// ```
#[utoipa::path(
    post,
    path = "/v1/adapters/import",
    params(
        ("load" = Option<bool>, Query, description = "Auto-load adapter after import")
    ),
    responses(
        (status = 200, description = "Adapter imported successfully", body = AdapterResponse),
        (status = 400, description = "Invalid file or format", body = ErrorResponse),
        (status = 413, description = "Payload too large", body = ErrorResponse),
        (status = 500, description = "Import failed", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn import_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Extension(client_ip): Extension<ClientIp>,
    Query(params): Query<HashMap<String, String>>,
    mut multipart: axum::extract::Multipart,
) -> Result<Json<AdapterResponse>, (StatusCode, Json<ErrorResponse>)> {
    use adapteros_core::B3Hash;
    use blake3::Hasher;

    // Require adapter register permission
    require_permission(&claims, Permission::AdapterRegister)?;

    let auto_load = params.get("load").map(|v| v == "true").unwrap_or(false);

    // Resolve adapter repo/cache roots (ENV > config > defaults) and ensure temp directory
    let adapters_paths = resolve_adapter_roots(&state);

    // === STREAMING UPLOAD (Issue 6) ===
    // Stream to temp file while computing whole-file hash
    let (temp_path, mut temp_file) = write_temp_bundle(&adapters_paths).await?;

    let mut hasher = Hasher::new();
    let mut total_bytes: u64 = 0;
    let mut filename: Option<String> = None;
    let mut file_found = false;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        error!(
            tenant_id = %claims.tenant_id,
            actor = %claims.sub,
            temp_path = %temp_path.display(),
            bytes = total_bytes,
            error = %e,
            "Failed to read multipart field"
        );
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("failed to read multipart")
                    .with_code("BAD_REQUEST")
                    .with_string_details(e.to_string()),
            ),
        )
    })? {
        if field.name() == Some("file") {
            file_found = true;
            filename = field.file_name().map(|s| s.to_string());

            // Stream chunks to temp file
            let mut field = field;
            while let Some(chunk) = field.chunk().await.map_err(|e| {
                error!(
                    tenant_id = %claims.tenant_id,
                    actor = %claims.sub,
                    filename = %filename.as_deref().unwrap_or("unknown"),
                    temp_path = %temp_path.display(),
                    bytes = total_bytes,
                    error = %e,
                    "Failed to read chunk"
                );
                let _ = std::fs::remove_file(&temp_path);
                (
                    StatusCode::BAD_REQUEST,
                    Json(
                        ErrorResponse::new("failed to read file chunk")
                            .with_code("BAD_REQUEST")
                            .with_string_details(e.to_string()),
                    ),
                )
            })? {
                total_bytes += chunk.len() as u64;

                // Check size limit
                if total_bytes > MAX_ADAPTER_SIZE {
                    let _ = tokio::fs::remove_file(&temp_path).await;
                    return Err((
                        StatusCode::PAYLOAD_TOO_LARGE,
                        Json(
                            ErrorResponse::new(format!(
                                "adapter file too large (max {} MB)",
                                MAX_ADAPTER_SIZE / (1024 * 1024)
                            ))
                            .with_code("PAYLOAD_TOO_LARGE"),
                        ),
                    ));
                }

                // Update hash (Issue 5: whole-file hash)
                hasher.update(&chunk);

                // Write to temp file
                temp_file.write_all(&chunk).await.map_err(|e| {
                    error!(
                        tenant_id = %claims.tenant_id,
                        actor = %claims.sub,
                        filename = %filename.as_deref().unwrap_or("unknown"),
                        temp_path = %temp_path.display(),
                        bytes = total_bytes,
                        error = %e,
                        "Failed to write chunk to temp file"
                    );
                    let _ = std::fs::remove_file(&temp_path);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            ErrorResponse::new("failed to write to temp file")
                                .with_code("INTERNAL_ERROR")
                                .with_string_details(e.to_string()),
                        ),
                    )
                })?;
            }
        }
    }

    // Ensure we got a file
    if !file_found {
        let _ = tokio::fs::remove_file(&temp_path).await;
        warn!(
            tenant_id = %claims.tenant_id,
            actor = %claims.sub,
            temp_path = %temp_path.display(),
            "No file provided in import request"
        );
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("no file provided").with_code("BAD_REQUEST")),
        ));
    }

    // Flush and close temp file
    temp_file.flush().await.map_err(|e| {
        let _ = std::fs::remove_file(&temp_path);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to flush temp file")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;
    drop(temp_file);

    // Compute whole-file hash (Issue 5)
    let file_hash = hasher.finalize().to_hex().to_string();

    // NOTE: We intentionally do NOT deduplicate based on this whole-file hash.
    // The canonical adapter identity is `weights_hash` (hash_b3) computed from the
    // canonical segment payload; dedup is performed after AOS parsing.

    // === VALIDATE AOS FORMAT ===
    // Read the file for validation (already streamed to disk)
    let data = tokio::fs::read(&temp_path).await.map_err(|e| {
        let _ = std::fs::remove_file(&temp_path);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to read temp file")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let filename_for_default = filename.clone();
    let _name = filename_for_default.unwrap_or_else(|| "imported.aos".to_string());

    // Validate minimum size
    if data.len() < 64 {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid AOS file: too small (< 64 bytes)")
                    .with_code("INVALID_FORMAT"),
            ),
        ));
    }

    let file_view = match adapteros_aos::open_aos(&data) {
        Ok(view) => view,
        Err(e) => {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new(format!("invalid AOS file: {}", e))
                        .with_code("INVALID_FORMAT"),
                ),
            ));
        }
    };

    // Extract and parse manifest JSON
    let manifest_bytes = file_view.manifest_bytes;
    let manifest_str = std::str::from_utf8(manifest_bytes).map_err(|_| {
        let _ = std::fs::remove_file(&temp_path);
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid AOS file: manifest is not valid UTF-8")
                    .with_code("INVALID_FORMAT"),
            ),
        )
    })?;

    let mut manifest: serde_json::Value = serde_json::from_str(manifest_str).map_err(|e| {
        let _ = std::fs::remove_file(&temp_path);
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new(format!(
                    "invalid AOS file: manifest JSON parse error: {}",
                    e
                ))
                .with_code("INVALID_FORMAT"),
            ),
        )
    })?;

    // === ISSUE 4: Reject if unknown fields are present in manifest ===
    if let Some(obj) = manifest.as_object() {
        let unknown_fields: Vec<&str> = obj
            .keys()
            .filter(|k| !KNOWN_MANIFEST_FIELDS.contains(&k.as_str()))
            .map(|k| k.as_str())
            .collect();

        if !unknown_fields.is_empty() {
            let _ = tokio::fs::remove_file(&temp_path).await;
            warn!(
                tenant_id = %claims.tenant_id,
                actor = %claims.sub,
                unknown_fields = ?unknown_fields,
                "Rejected import: unknown manifest fields"
            );
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new(format!(
                        "Adapter import contains unknown required fields: [{}]",
                        unknown_fields.join(", ")
                    ))
                    .with_code("UNKNOWN_MANIFEST_FIELDS"),
                ),
            ));
        }
    }

    let (scope_path, domain, group, _operation) = {
        let metadata_obj = manifest.get("metadata").and_then(|m| m.as_object());
        let scope_path = match metadata_obj
            .and_then(|m| m.get("scope_path"))
            .and_then(|v| v.as_str())
        {
            Some(path) => path.to_string(),
            None => {
                let _ = tokio::fs::remove_file(&temp_path).await;
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(
                        ErrorResponse::new("invalid AOS file: missing scope_path in metadata")
                            .with_code("INVALID_FORMAT"),
                    ),
                ));
            }
        };
        let domain = metadata_obj
            .and_then(|m| m.get("domain").and_then(|v| v.as_str()))
            .unwrap_or("unspecified")
            .to_string();
        let group = metadata_obj
            .and_then(|m| m.get("group").and_then(|v| v.as_str()))
            .unwrap_or("unspecified")
            .to_string();
        let operation = metadata_obj
            .and_then(|m| m.get("operation").and_then(|v| v.as_str()))
            .map(|s| s.to_string());
        (scope_path, domain, group, operation)
    };
    let scope_hash = adapteros_aos::compute_scope_hash(&scope_path);
    let scope_value = manifest
        .get("scope")
        .and_then(|v| v.as_str())
        .unwrap_or("project")
        .to_string();
    let canonical_segment = match file_view
        .segments
        .iter()
        .find(|seg| seg.scope_hash == scope_hash)
        .or_else(|| file_view.segments.first())
    {
        Some(seg) => seg,
        None => {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("invalid AOS file: missing canonical segment")
                        .with_code("INVALID_FORMAT"),
                ),
            ));
        }
    };
    let weights_data = canonical_segment.payload;

    // === PRD-ART-01: ARTIFACT HARDENING VALIDATIONS ===

    // A. Schema Version Validation
    // Current manifest schema version (keep in sync with format.rs MANIFEST_SCHEMA_VERSION)
    const MANIFEST_SCHEMA_VERSION: &str = "1.0.0";

    let schema_version = manifest
        .get("schema_version")
        .and_then(|v| v.as_str())
        .unwrap_or("1.0.0")
        .to_string();

    // Simple major version check: extract first number and compare
    let file_major: u32 = schema_version
        .split('.')
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let current_major: u32 = MANIFEST_SCHEMA_VERSION
        .split('.')
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

    if file_major > current_major {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new(format!(
                    "Schema version {} is newer than supported {}. Update adapterOS.",
                    schema_version, MANIFEST_SCHEMA_VERSION
                ))
                .with_code("INCOMPATIBLE_SCHEMA_VERSION"),
            ),
        ));
    }

    // B. Base Model Compatibility Check
    let base_model = manifest.get("base_model").and_then(|v| v.as_str());
    let resolved_base_model_id: Option<String> = if let Some(base_model_name) = base_model {
        match state
            .db
            .get_model_by_name_for_tenant(&claims.tenant_id, base_model_name)
            .await
        {
            Ok(Some(model)) => Some(model.id),
            Ok(None) => {
                warn!(
                    base_model = %base_model_name,
                    "Imported adapter references base model not available on this system"
                );
                // Don't fail - allow import but log warning (model might be acquired later)
                None
            }
            Err(e) => {
                warn!(
                    base_model = %base_model_name,
                    error = %e,
                    "Failed to check base model availability"
                );
                None
            }
        }
    } else {
        None
    };

    // C. Backend Family Validation
    if let Some(backend) = manifest.get("backend_family").and_then(|v| v.as_str()) {
        if !matches!(backend, "metal" | "coreml" | "mlx" | "auto") {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new(format!("Unsupported backend family: {}", backend))
                        .with_code("UNSUPPORTED_BACKEND"),
                ),
            ));
        }
    }

    // D. Hash Integrity Cross-Check (weights hash from manifest vs computed)
    let weights_data = canonical_segment.payload;
    let computed_weights_hash = B3Hash::hash(weights_data).to_hex().to_string();
    if let Some(manifest_weights_hash) = manifest.get("weights_hash").and_then(|v| v.as_str()) {
        if manifest_weights_hash != computed_weights_hash {
            let _ = tokio::fs::remove_file(&temp_path).await;
            error!(
                tenant_id = %claims.tenant_id,
                manifest_hash = %manifest_weights_hash,
                computed_hash = %computed_weights_hash,
                "Weights hash mismatch - file may be corrupted"
            );
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new(format!(
                        "Weights hash mismatch: manifest says {}, computed {}",
                        manifest_weights_hash, computed_weights_hash
                    ))
                    .with_code("HASH_INTEGRITY_FAILURE"),
                ),
            ));
        }
    }
    let weights_hash = computed_weights_hash;

    // === DEDUPLICATION CHECK (Issue 4) ===
    // Deduplicate on canonical weights hash (adapters.hash_b3) within the tenant.
    if let Ok(Some(existing)) = state
        .db
        .find_adapter_by_hash(&weights_hash, Some(&claims.tenant_id))
        .await
    {
        // Cleanup temp file - we don't need it
        let _ = tokio::fs::remove_file(&temp_path).await;

        info!(
            existing_id = %existing.adapter_id.as_ref().unwrap_or(&existing.id),
            hash_b3 = %weights_hash,
            actor = %claims.sub,
            "Deduplicated adapter import - returning existing adapter"
        );

        let now = chrono::Utc::now().to_rfc3339();
        let display_name = adapteros_id::display_name_for(&existing.id);
        return Ok(Json(AdapterResponse {
            schema_version: "v1".to_string(),
            id: existing.id.clone(),
            adapter_id: existing.adapter_id.clone().unwrap_or(existing.id),
            name: existing.name,
            hash_b3: existing.hash_b3,
            rank: existing.rank,
            tier: existing.tier,
            assurance_tier: None,
            languages: vec![],
            framework: existing.framework,
            category: Some(existing.category),
            scope: Some(existing.scope),
            lora_tier: None,
            lora_strength: existing.lora_strength,
            lora_scope: None,
            framework_id: existing.framework_id,
            framework_version: existing.framework_version,
            repo_id: existing.repo_id,
            commit_sha: existing.commit_sha,
            intent: existing.intent,
            created_at: existing.created_at,
            updated_at: Some(now),
            stats: None,
            version: existing.version,
            lifecycle_state: existing.lifecycle_state.into(),
            runtime_state: Some(existing.current_state),
            pinned: Some(existing.pinned != 0),
            memory_bytes: Some(existing.memory_bytes),
            deduplicated: Some(true),
            drift_reference_backend: None,
            drift_baseline_backend: None,
            drift_test_backend: None,
            drift_tier: None,
            drift_metric: None,
            drift_slice_size: None,
            drift_slice_offset: None,
            drift_loss_metric: None,
            // Codebase adapter fields
            adapter_type: None,
            base_adapter_id: None,
            stream_session_id: None,
            versioning_threshold: None,
            coreml_package_hash: None,
            display_name,
        }));
    }

    // E. Signature Policy Check
    let policy = state
        .db
        .get_execution_policy_or_default(&claims.tenant_id)
        .await
        .map_err(|e| {
            error!(
                tenant_id = %claims.tenant_id,
                error = %e,
                "Failed to get tenant execution policy"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to check tenant policy")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    if policy.require_signed_adapters {
        // Extract signature from manifest
        let sig_hex = manifest
            .get("signature")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                let _ = std::fs::remove_file(&temp_path);
                warn!(
                    tenant_id = %claims.tenant_id,
                    "Rejected unsigned adapter import due to tenant policy"
                );
                (
                    StatusCode::FORBIDDEN,
                    Json(
                        ErrorResponse::new("Tenant policy requires signed adapters")
                            .with_code("SIGNATURE_REQUIRED"),
                    ),
                )
            })?;

        // Get trusted public key for this tenant
        let pubkey = state
            .db
            .get_trusted_adapter_key(&claims.tenant_id)
            .await
            .map_err(|e| {
                error!(
                    tenant_id = %claims.tenant_id,
                    error = %e,
                    "Failed to get trusted adapter key"
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to retrieve trusted signing key")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?
            .ok_or_else(|| {
                let _ = std::fs::remove_file(&temp_path);
                warn!(
                    tenant_id = %claims.tenant_id,
                    "No trusted signing key configured for tenant"
                );
                (
                    StatusCode::FORBIDDEN,
                    Json(
                        ErrorResponse::new("No trusted signing key configured for tenant")
                            .with_code("NO_TRUSTED_KEY"),
                    ),
                )
            })?;

        // Decode signature from hex
        let sig_bytes = hex::decode(sig_hex).map_err(|e| {
            let _ = std::fs::remove_file(&temp_path);
            (
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("Invalid signature format: not valid hex")
                        .with_code("INVALID_SIGNATURE")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

        if sig_bytes.len() != 64 {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("Invalid signature format: expected 64 bytes")
                        .with_code("INVALID_SIGNATURE"),
                ),
            ));
        }

        let mut sig_arr = [0u8; 64];
        sig_arr.copy_from_slice(&sig_bytes);
        let signature = adapteros_crypto::Signature::from_bytes(&sig_arr).map_err(|e| {
            let _ = std::fs::remove_file(&temp_path);
            (
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("Invalid Ed25519 signature")
                        .with_code("INVALID_SIGNATURE")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

        // Compute BLAKE3 hash of manifest bytes for verification
        let manifest_hash = B3Hash::hash(manifest_bytes);

        // Verify signature
        adapteros_crypto::verify_signature(&pubkey, manifest_hash.as_bytes(), &signature).map_err(
            |e| {
                let _ = std::fs::remove_file(&temp_path);
                warn!(
                    tenant_id = %claims.tenant_id,
                    error = %e,
                    "Adapter signature verification failed"
                );
                (
                    StatusCode::FORBIDDEN,
                    Json(
                        ErrorResponse::new("Adapter signature verification failed")
                            .with_code("SIGNATURE_INVALID")
                            .with_string_details(e.to_string()),
                    ),
                )
            },
        )?;

        info!(
            tenant_id = %claims.tenant_id,
            "Adapter signature verified successfully"
        );
    }

    // === END PRD-ART-01 VALIDATIONS ===

    // Extract adapter fields from manifest
    // Adapter identity for runtime loads must be stable and must match the on-disk bundle manifest.
    let user_adapter_id = manifest
        .get("adapter_id")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    if let Some(id) = user_adapter_id {
        validate_adapter_id(id)?;
    }

    let adapter_id = user_adapter_id.map(|s| s.to_string()).unwrap_or_else(|| {
        // Deterministic fallback for legacy bundles that omit adapter_id:
        // tie identity to the canonical weights hash so repeated imports are stable.
        let prefix_len = 32.min(weights_hash.len());
        format!("adp_{}", &weights_hash[..prefix_len])
    });

    let adapter_name = manifest
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| adapter_id.clone());

    // Normalize legacy manifests by injecting adapter_id so worker-side integrity checks can run.
    let mut did_rewrite_manifest = false;
    let manifest_adapter_id = manifest
        .get("adapter_id")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .unwrap_or("");
    if manifest_adapter_id != adapter_id {
        if let Some(obj) = manifest.as_object_mut() {
            obj.insert(
                "adapter_id".to_string(),
                serde_json::Value::String(adapter_id.clone()),
            );
            did_rewrite_manifest = true;
        }
    }

    if did_rewrite_manifest {
        // Re-package to a canonical AOS archive so the stored bundle's manifest matches
        // the control-plane adapter_id and can be verified in the worker.
        let mut writer = adapteros_aos::AosWriter::new();
        let manifest_scope_hash = adapteros_aos::compute_scope_hash(&scope_path);
        let scope_path_string = scope_path.clone();

        for seg in &file_view.segments {
            // Preserve whether a segment is scoped vs unscoped. For legacy bundles we only
            // know the manifest scope_path, so scoped segments all use that hash.
            let scope_arg = (seg.scope_hash != [0u8; 16]).then_some(scope_path_string.clone());
            if seg.backend_tag == adapteros_aos::BackendTag::Canonical {
                // Ensure canonical segment is always scoped to the manifest scope_path.
                writer
                    .add_segment(
                        seg.backend_tag,
                        Some(scope_path_string.clone()),
                        seg.payload,
                    )
                    .map_err(|e| {
                        let _ = std::fs::remove_file(&temp_path);
                        (
                            StatusCode::BAD_REQUEST,
                            Json(
                                ErrorResponse::new(format!(
                                    "invalid AOS file: failed to normalize segments: {}",
                                    e
                                ))
                                .with_code("INVALID_FORMAT"),
                            ),
                        )
                    })?;
            } else if seg.scope_hash == [0u8; 16] || seg.scope_hash == manifest_scope_hash {
                writer
                    .add_segment(seg.backend_tag, scope_arg, seg.payload)
                    .map_err(|e| {
                        let _ = std::fs::remove_file(&temp_path);
                        (
                            StatusCode::BAD_REQUEST,
                            Json(
                                ErrorResponse::new(format!(
                                    "invalid AOS file: failed to normalize segments: {}",
                                    e
                                ))
                                .with_code("INVALID_FORMAT"),
                            ),
                        )
                    })?;
            } else {
                warn!(
                    adapter_id = %adapter_id,
                    backend_tag = ?seg.backend_tag,
                    "Skipping non-canonical segment with unknown scope hash during import normalization"
                );
            }
        }

        writer.write_archive(&temp_path, &manifest).map_err(|e| {
            let _ = std::fs::remove_file(&temp_path);
            (
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new(format!(
                        "invalid AOS file: failed to normalize manifest: {}",
                        e
                    ))
                    .with_code("INVALID_FORMAT"),
                ),
            )
        })?;
    }

    // Content hash identity (BLAKE3(manifest_json + weights_bytes)).
    // If we normalized, use the bytes we will store (writer uses pretty JSON too).
    let manifest_bytes_for_hash: Vec<u8> = if did_rewrite_manifest {
        serde_json::to_vec_pretty(&manifest).map_err(|e| {
            let _ = std::fs::remove_file(&temp_path);
            (
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new(format!(
                        "invalid AOS file: failed to serialize normalized manifest: {}",
                        e
                    ))
                    .with_code("INVALID_FORMAT"),
                ),
            )
        })?
    } else {
        manifest_bytes.to_vec()
    };
    let content_hash_b3 = hash_multi_bytes(&[&manifest_bytes_for_hash, weights_data]);

    // Validate user-provided rank if present (must be positive and reasonable)
    if let Some(user_rank) = manifest.get("rank").and_then(|v| v.as_i64()) {
        if user_rank <= 0 {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new("rank must be positive").with_code("VALIDATION_ERROR")),
            ));
        }
        if user_rank > 256 {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new("rank too large (max 256)").with_code("VALIDATION_ERROR")),
            ));
        }
    }
    let rank = manifest
        .get("rank")
        .and_then(|v| v.as_i64())
        .map(|r| r as i32)
        .unwrap_or(16);

    // === ISSUE 1: Reject if version string is missing from metadata ===
    if manifest.get("version").and_then(|v| v.as_str()).is_none() {
        let _ = tokio::fs::remove_file(&temp_path).await;
        warn!(
            tenant_id = %claims.tenant_id,
            actor = %claims.sub,
            "Rejected import: missing version string in metadata"
        );
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Adapter version string is missing from metadata")
                    .with_code("MISSING_VERSION"),
            ),
        ));
    }

    // Validate user-provided version (must be semver-like)
    if let Some(user_version) = manifest.get("version").and_then(|v| v.as_str()) {
        if user_version.is_empty() {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new("version cannot be empty").with_code("VALIDATION_ERROR")),
            ));
        }
        if user_version.len() > 64 {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("version too long (max 64 chars)")
                        .with_code("VALIDATION_ERROR"),
                ),
            ));
        }
        // Basic semver format check (allow alphanumeric, dots, hyphens, underscores)
        if !user_version
            .chars()
            .all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '_')
        {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("version contains invalid characters (use alphanumeric, dots, hyphens, underscores)")
                        .with_code("VALIDATION_ERROR"),
                ),
            ));
        }
    }
    // Version is guaranteed to exist after ISSUE 1 validation above
    let version = manifest
        .get("version")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| {
            // This should never happen due to validation above, but handle gracefully
            let temp_path_for_cleanup = temp_path.clone();
            // Fire-and-forget cleanup - drop handle explicitly to satisfy clippy
            drop(tokio::spawn(async move {
                let _ = tokio::fs::remove_file(&temp_path_for_cleanup).await;
            }));
            (
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("version field missing from manifest")
                        .with_code("VALIDATION_ERROR"),
                ),
            )
        })?;
    let uploaded_file_name = filename.clone().unwrap_or_else(|| _name.clone());

    emit_adapter_progress(
        &adapter_id,
        "validated",
        Some(uploaded_file_name.as_str()),
        50.0,
        "Validated adapter bundle",
    );

    // Note: weights_hash was computed in PRD-ART-01 validation section above

    // === TRANSACTIONAL SAFETY (Issue 1) ===
    let repo = DefaultAdapterRepo::new(&state);
    let stored = repo
        .store_bundle(StoreBundleRequest {
            tenant_id: claims.tenant_id.clone(),
            // Store bundles under adapter_id so worker-side runtime resolution is stable.
            adapter_name: adapter_id.clone(),
            version: version.clone(),
            temp_path: temp_path.clone(),
            precomputed_hash: if did_rewrite_manifest {
                None
            } else {
                Some(file_hash.clone())
            },
        })
        .await
        .map_err(map_repo_error)?;
    let file_path = stored.final_path.clone();
    let file_path_str = file_path.to_string_lossy().to_string();
    let file_hash = stored.manifest_hash.clone();

    // Step 2: Register in database (rollback file on failure)
    let tier = if auto_load { "warm" } else { "ephemeral" };
    let registration_params = AdapterRegistrationBuilder::new()
        .adapter_id(&adapter_id)
        .tenant_id(&claims.tenant_id)
        .name(&adapter_name)
        .hash_b3(&weights_hash)
        .rank(rank)
        .tier(tier)
        .scope(&scope_value)
        .domain(Some(domain))
        .purpose(Some(group))
        .aos_file_path(Some(&file_path_str))
        .aos_file_hash(Some(&file_hash)) // Store whole-file hash separately from weights hash
        // PRD-ART-01: Artifact hardening fields
        .manifest_schema_version(Some(schema_version))
        .content_hash_b3(Some(&content_hash_b3))
        .base_model_id(resolved_base_model_id)
        .build()
        .map_err(|e| {
            // Rollback: remove the file we just created
            let _ = std::fs::remove_file(&file_path);
            error!(
                adapter_id = %adapter_id,
                tenant_id = %claims.tenant_id,
                file_path = %file_path.display(),
                error = %e,
                "Failed to build adapter registration params"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to build registration params")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let registered_id = repo
        .register_bundle(&adapter_id, &claims.tenant_id, registration_params)
        .await
        .map_err(map_repo_error)?;

    // === AUTO-LOAD (Issue 2) ===
    // Register with lifecycle manager and optionally load
    if let Some(ref lifecycle) = state.lifecycle_manager {
        let mut manager = lifecycle.lock().await;
        let hash = B3Hash::from_hex(&weights_hash).unwrap_or_else(|_| B3Hash::hash(weights_data));

        match manager.register_adapter(
            adapter_id.clone(),
            hash,
            Some("code".to_string()),
            auto_load,
        ) {
            Ok(adapter_idx) => {
                info!(
                    adapter_id = %adapter_id,
                    adapter_idx = adapter_idx,
                    auto_load = auto_load,
                    "Registered adapter with lifecycle manager"
                );
            }
            Err(e) => {
                // Don't fail the import, just warn
                warn!(
                    adapter_id = %adapter_id,
                    error = %e,
                    "Failed to register adapter with lifecycle manager (import still succeeded)"
                );
            }
        }
    }

    // Emit telemetry event
    info!(
        event = "adapter.imported",
        adapter_id = %adapter_id,
        registered_id = %registered_id,
        auto_load = %auto_load,
        file_size = %total_bytes,
        file_path = %file_path_str,
        rank = %rank,
        weights_hash = %weights_hash,
        file_hash = %file_hash,
        actor = %claims.sub,
        "Adapter imported from AOS file with full transactional safety"
    );

    // Audit log
    log_success_or_warn(
        &state.db,
        &claims,
        actions::ADAPTER_REGISTER,
        resources::ADAPTER,
        Some(&adapter_id),
        Some(client_ip.0.as_str()),
    )
    .await;

    emit_adapter_progress(
        &adapter_id,
        "registered",
        Some(uploaded_file_name.as_str()),
        100.0,
        "Adapter import complete",
    );

    // Return adapter response with manifest data
    let now = chrono::Utc::now().to_rfc3339();
    Ok(Json(AdapterResponse {
        schema_version: "v1".to_string(),
        id: adapter_id.clone(),
        adapter_id: adapter_id.clone(),
        name: adapter_name,
        hash_b3: weights_hash,
        rank,
        tier: tier.to_string(),
        assurance_tier: None,
        languages: vec![],
        framework: None,
        category: None,
        scope: None,
        lora_tier: None,
        lora_strength: Some(1.0),
        lora_scope: None,
        framework_id: None,
        framework_version: None,
        repo_id: None,
        commit_sha: None,
        intent: None,
        created_at: now,
        updated_at: None,
        stats: None,
        version,
        lifecycle_state: LifecycleState::Draft,
        runtime_state: Some(if auto_load {
            "warm".to_string()
        } else {
            "cold".to_string()
        }),
        pinned: None,
        memory_bytes: None,
        deduplicated: Some(false),
        drift_reference_backend: None,
        drift_baseline_backend: None,
        drift_test_backend: None,
        drift_tier: None,
        drift_metric: None,
        drift_slice_size: None,
        drift_slice_offset: None,
        drift_loss_metric: None,
        // Codebase adapter fields
        adapter_type: None,
        base_adapter_id: None,
        stream_session_id: None,
        versioning_threshold: None,
        coreml_package_hash: None,
        display_name: adapteros_id::display_name_for(&adapter_id),
    }))
}
