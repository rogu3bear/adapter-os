//! Diagnostic bundle export handler.
//!
//! Endpoints:
//! - POST /v1/diag/bundle - Create a signed bundle export
//! - GET /v1/diag/bundle/{export_id} - Get bundle export info
//! - GET /v1/diag/bundle/{export_id}/download - Download bundle file

use crate::auth::Claims;
use crate::middleware::require_any_role;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_api_types::diagnostics::{
    BackendConfigSnapshot, BundleFileEntry, BundleIdentity, BundleManifest, ConfigSnapshot,
    DiagBundleExportRequest, DiagBundleExportResponse, RouterConfigSnapshot,
};
use adapteros_api_types::API_SCHEMA_VERSION;
use adapteros_core::B3Hash;
use adapteros_crypto::bundle_sign;
use adapteros_db::diagnostics::{
    get_all_diag_events_for_run, get_bundle_export_by_id, get_diag_run_by_trace_id,
    insert_bundle_export, CreateBundleExportParams, DiagEventRecord,
};
use adapteros_db::users::Role;
use axum::body::Body;
use axum::extract::{Extension, Path, State};
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use chrono::Utc;
use std::io::Write;
use subtle::ConstantTimeEq;
use tokio_util::io::ReaderStream;
use tracing::{debug, info, warn};

/// Perform constant-time comparison of two strings to prevent timing attacks.
/// Returns false if lengths differ (which leaks length info, but that's acceptable
/// for tokens where length is not secret).
fn constant_time_compare(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

/// Evidence authorization token for bundle export with evidence.
const EVIDENCE_AUTH_SECRET: &str = "AOS_EVIDENCE_AUTH_SECRET";

/// POST /v1/diag/bundle - Create a signed bundle export
#[utoipa::path(
    post,
    path = "/v1/diag/bundle",
    request_body = DiagBundleExportRequest,
    responses(
        (status = 200, description = "Bundle created", body = DiagBundleExportResponse),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - evidence not authorized"),
        (status = 404, description = "Run not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "diagnostics",
    security(("bearer_token" = []))
)]
pub async fn create_bundle_export(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<DiagBundleExportRequest>,
) -> Result<Json<DiagBundleExportResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role gating: Admin or Operator only
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    let tenant_id = &claims.tenant_id;

    debug!(
        tenant_id = %tenant_id,
        trace_id = %request.trace_id,
        format = %request.format,
        include_evidence = request.include_evidence,
        "Creating diagnostic bundle export"
    );

    // Validate evidence authorization if requested
    if request.include_evidence {
        let expected_token = std::env::var(EVIDENCE_AUTH_SECRET).ok();
        match (expected_token, &request.evidence_auth_token) {
            (Some(expected), Some(provided)) if constant_time_compare(&expected, provided) => {
                debug!("Evidence authorization validated");
            }
            (Some(_), Some(_)) => {
                warn!("Evidence authorization failed: token mismatch");
                return Err((
                    StatusCode::FORBIDDEN,
                    Json(
                        ErrorResponse::new("Invalid evidence authorization token")
                            .with_code("FORBIDDEN"),
                    ),
                ));
            }
            (Some(_), None) => {
                warn!("Evidence requested but no authorization token provided");
                return Err((
                    StatusCode::FORBIDDEN,
                    Json(
                        ErrorResponse::new("Evidence authorization token required")
                            .with_code("FORBIDDEN"),
                    ),
                ));
            }
            (None, _) if request.include_evidence => {
                warn!("Evidence requested but AOS_EVIDENCE_AUTH_SECRET not configured");
                return Err((
                    StatusCode::FORBIDDEN,
                    Json(
                        ErrorResponse::new("Evidence export not configured on this server")
                            .with_code("FORBIDDEN"),
                    ),
                ));
            }
            _ => {}
        }
    }

    // Get the diagnostic run
    let run = get_diag_run_by_trace_id(state.db.pool(), tenant_id, &request.trace_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get diagnostic run")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("Diagnostic run not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(format!("trace_id: {}", request.trace_id)),
                ),
            )
        })?;

    // Get all events for the run
    let events = get_all_diag_events_for_run(state.db.pool(), tenant_id, &run.id, 50000)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get events")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let events_truncated = events.len() >= 50000;

    // Build bundle contents
    let export_id = crate::id_generator::readable_id(adapteros_core::ids::IdKind::Export, "diag");
    let created_at = Utc::now().to_rfc3339();

    // Create bundle files in memory
    let mut files: Vec<BundleFileEntry> = Vec::new();
    let mut total_uncompressed = 0u64;

    // 1. Events NDJSON (canonical encoding)
    let events_data = build_events_ndjson(&events);
    let events_hash = B3Hash::hash(&events_data);
    files.push(BundleFileEntry {
        path: "events.ndjson".to_string(),
        size_bytes: events_data.len() as u64,
        hash: events_hash.to_hex(),
        content_type: "application/x-ndjson".to_string(),
    });
    total_uncompressed += events_data.len() as u64;

    // 2. Compute events merkle root
    let merkle_root = compute_events_merkle_root(&events).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to compute events merkle root")
                    .with_code("MERKLE_ERROR")
                    .with_string_details(e),
            ),
        )
    })?;

    // 3. Config snapshot
    // Get active policy packs from policy manager
    let active_policy_packs: Vec<String> = state
        .policy_manager
        .get_all_configs()
        .iter()
        .filter(|(_, cfg)| cfg.enabled)
        .map(|(id, _)| id.name().to_string())
        .collect();

    // Get k_sparse from router policy pack config
    let k_sparse_value = state
        .policy_manager
        .get_pack_config(&adapteros_policy::PolicyPackId::Router)
        .and_then(|cfg| cfg.config.get("k_sparse"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);

    let config_snapshot = ConfigSnapshot {
        server_version: env!("CARGO_PKG_VERSION").to_string(),
        api_schema_version: API_SCHEMA_VERSION.to_string(),
        active_policy_packs,
        router_config: RouterConfigSnapshot {
            k_sparse_value,
            determinism_mode: "strict".to_string(),
            tie_break_policy: "score_desc_index_asc".to_string(),
        },
        backend_config: BackendConfigSnapshot {
            backend_type: "mlx".to_string(),
            metal_enabled: cfg!(target_os = "macos"),
            coreml_enabled: cfg!(target_os = "macos"),
            ane_enabled: cfg!(target_os = "macos"),
        },
    };
    let config_data = serde_json::to_vec_pretty(&config_snapshot).unwrap_or_default();
    let config_hash = B3Hash::hash(&config_data);
    files.push(BundleFileEntry {
        path: "config_snapshot.json".to_string(),
        size_bytes: config_data.len() as u64,
        hash: config_hash.to_hex(),
        content_type: "application/json".to_string(),
    });
    total_uncompressed += config_data.len() as u64;

    // 4. Identity info
    let adapter_stack_ids: Vec<String> = run
        .adapter_stack_ids
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    let code_identity = get_code_identity();

    let identity = BundleIdentity {
        request_hash: run.request_hash.clone(),
        decision_chain_hash: run.decision_chain_hash.clone(),
        backend_identity_hash: run.backend_identity_hash.clone(),
        model_identity_hash: run.model_identity_hash.clone(),
        adapter_stack_ids: adapter_stack_ids.clone(),
        code_identity: code_identity.clone(),
    };

    let identity_data = serde_json::to_vec_pretty(&identity).unwrap_or_default();
    let identity_hash = B3Hash::hash(&identity_data);
    files.push(BundleFileEntry {
        path: "identity.json".to_string(),
        size_bytes: identity_data.len() as u64,
        hash: identity_hash.to_hex(),
        content_type: "application/json".to_string(),
    });
    total_uncompressed += identity_data.len() as u64;

    // 5. Build manifest
    let manifest = BundleManifest {
        schema_version: "1.0.0".to_string(),
        format: request.format.clone(),
        created_at: created_at.clone(),
        trace_id: request.trace_id.clone(),
        run_id: run.id.clone(),
        tenant_id: tenant_id.clone(),
        run_status: run.status.clone(),
        files: files.clone(),
        total_uncompressed_bytes: total_uncompressed,
        events_merkle_root: merkle_root.to_hex(),
        events_count: events.len() as u64,
        events_truncated,
        evidence_included: request.include_evidence,
        identity,
    };

    let manifest_data = serde_json::to_vec_pretty(&manifest).unwrap_or_default();
    let manifest_hash = B3Hash::hash(&manifest_data);

    // Create bundle file
    let exports_dir = std::path::Path::new("var/exports");
    std::fs::create_dir_all(exports_dir).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to create exports directory")
                    .with_code("IO_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let bundle_filename = format!("diag_bundle_{}_{}.tar.zst", run.trace_id, export_id);
    let bundle_path = exports_dir.join(&bundle_filename);

    // Write bundle (tar.zst)
    let bundle_data =
        create_tar_zst_bundle(&manifest_data, &events_data, &config_data, &identity_data).map_err(
            |e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("Failed to create bundle")
                            .with_code("IO_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            },
        )?;

    let bundle_hash = B3Hash::hash(&bundle_data);
    let bundle_size = bundle_data.len() as u64;

    std::fs::write(&bundle_path, &bundle_data).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to write bundle")
                    .with_code("IO_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Set restrictive file permissions (owner read/write only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&bundle_path, perms).map_err(|e| {
            let _ = std::fs::remove_file(&bundle_path); // cleanup
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to secure bundle file")
                        .with_code("IO_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;
    }

    // Sign the bundle
    let keypair = &state.ed25519_keypair;
    let signature = bundle_sign::sign_bundle(&bundle_hash, &merkle_root, keypair).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to sign bundle")
                    .with_code("CRYPTO_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let public_key_hex = hex::encode(keypair.public_key().to_bytes());
    let signature_hex = hex::encode(signature.signature.to_bytes());
    let key_id = signature.key_id.clone();

    // Store export metadata in database
    let db_params = CreateBundleExportParams {
        id: export_id.clone(),
        tenant_id: tenant_id.clone(),
        run_id: run.id.clone(),
        trace_id: request.trace_id.clone(),
        format: request.format.clone(),
        file_path: bundle_path.to_string_lossy().to_string(),
        size_bytes: bundle_size as i64,
        bundle_hash: bundle_hash.to_hex(),
        merkle_root: merkle_root.to_hex(),
        signature: signature_hex.clone(),
        public_key: public_key_hex.clone(),
        key_id: key_id.clone(),
        manifest_json: serde_json::to_string(&manifest).unwrap_or_default(),
        evidence_included: request.include_evidence,
        request_hash: Some(run.request_hash.clone()),
        decision_chain_hash: run.decision_chain_hash.clone(),
        backend_identity_hash: run.backend_identity_hash.clone(),
        model_identity_hash: run.model_identity_hash.clone(),
        adapter_stack_ids: run.adapter_stack_ids.clone(),
        code_identity,
        created_by: Some(claims.sub.clone()),
        expires_at: Some(
            (Utc::now() + chrono::Duration::days(30))
                .to_rfc3339()
                .to_string(),
        ),
    };

    insert_bundle_export(state.db.pool(), &db_params)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to store export metadata")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    info!(
        export_id = %export_id,
        trace_id = %request.trace_id,
        bundle_hash = %bundle_hash.to_hex(),
        size_bytes = bundle_size,
        "Created diagnostic bundle export"
    );

    Ok(Json(DiagBundleExportResponse {
        schema_version: API_SCHEMA_VERSION.to_string(),
        export_id: export_id.clone(),
        format: request.format,
        size_bytes: bundle_size,
        bundle_hash: bundle_hash.to_hex(),
        merkle_root: merkle_root.to_hex(),
        signature: signature_hex,
        public_key: public_key_hex,
        key_id,
        download_url: format!("/v1/diag/bundle/{}/download", export_id),
        created_at,
        manifest,
    }))
}

/// GET /v1/diag/bundle/{export_id} - Get bundle export info
#[utoipa::path(
    get,
    path = "/v1/diag/bundle/{export_id}",
    params(
        ("export_id" = String, Path, description = "Export ID")
    ),
    responses(
        (status = 200, description = "Bundle info", body = DiagBundleExportResponse),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Export not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "diagnostics",
    security(("bearer_token" = []))
)]
pub async fn get_bundle_export(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(export_id): Path<String>,
) -> Result<Json<DiagBundleExportResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator, Role::Viewer])?;

    let tenant_id = &claims.tenant_id;

    let export = get_bundle_export_by_id(state.db.pool(), tenant_id, &export_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get export")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("Export not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(format!("export_id: {}", export_id)),
                ),
            )
        })?;

    let manifest: BundleManifest = serde_json::from_str(&export.manifest_json).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to parse manifest")
                    .with_code("SERIALIZATION_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    Ok(Json(DiagBundleExportResponse {
        schema_version: API_SCHEMA_VERSION.to_string(),
        export_id: export.id,
        format: export.format,
        size_bytes: export.size_bytes as u64,
        bundle_hash: export.bundle_hash,
        merkle_root: export.merkle_root,
        signature: export.signature,
        public_key: export.public_key,
        key_id: export.key_id,
        download_url: format!("/v1/diag/bundle/{}/download", export_id),
        created_at: export.created_at,
        manifest,
    }))
}

/// GET /v1/diag/bundle/{export_id}/download - Download bundle file
#[utoipa::path(
    get,
    path = "/v1/diag/bundle/{export_id}/download",
    params(
        ("export_id" = String, Path, description = "Export ID")
    ),
    responses(
        (status = 200, description = "Bundle file", content_type = "application/octet-stream"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Export not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "diagnostics",
    security(("bearer_token" = []))
)]
pub async fn download_bundle(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(export_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    let tenant_id = &claims.tenant_id;

    let export = get_bundle_export_by_id(state.db.pool(), tenant_id, &export_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get export")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("Export not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(format!("export_id: {}", export_id)),
                ),
            )
        })?;

    // Path traversal protection: validate file_path is within var/exports/
    let exports_dir = std::path::Path::new("var/exports");
    let canonical_exports_dir = exports_dir.canonicalize().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to resolve exports directory")
                    .with_code("IO_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let file_path = std::path::Path::new(&export.file_path);
    let canonical_file_path = file_path.canonicalize().map_err(|_| {
        // File doesn't exist or can't be resolved
        (
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("Bundle file not found")
                    .with_code("NOT_FOUND")
                    .with_string_details("Bundle file may have been cleaned up"),
            ),
        )
    })?;

    // Verify the canonical path is within the exports directory
    if !canonical_file_path.starts_with(&canonical_exports_dir) {
        warn!(
            export_id = %export_id,
            attempted_path = %export.file_path,
            "Path traversal attempt detected"
        );
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Access denied")
                    .with_code("PATH_TRAVERSAL")
                    .with_string_details("File path is outside allowed directory"),
            ),
        ));
    }

    // Verify file integrity
    let file_data = tokio::fs::read(file_path).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to read bundle")
                    .with_code("IO_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let computed_hash = B3Hash::hash(&file_data);
    if computed_hash.to_hex() != export.bundle_hash {
        warn!(
            export_id = %export_id,
            expected = %export.bundle_hash,
            computed = %computed_hash.to_hex(),
            "Bundle integrity check failed"
        );
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Bundle integrity check failed")
                    .with_code("VERIFICATION_ERROR")
                    .with_string_details("File hash mismatch - possible tampering"),
            ),
        ));
    }

    let filename = file_path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| format!("bundle_{}.tar.zst", export_id));

    let file = tokio::fs::File::open(file_path).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to open bundle")
                    .with_code("IO_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    // Build response with headers
    let response = axum::response::Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", filename),
        )
        .header(header::CONTENT_LENGTH, export.size_bytes.to_string())
        .header("x-bundle-hash", export.bundle_hash.clone())
        .header("x-bundle-signature", export.signature.clone())
        .body(body)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to build response")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(response)
}

// ============================================================================
// Helper functions
// ============================================================================

/// Build NDJSON from events using canonical encoding.
fn build_events_ndjson(events: &[DiagEventRecord]) -> Vec<u8> {
    let mut output = Vec::new();
    for event in events {
        // Parse payload and re-serialize with JCS for determinism
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(&event.payload_json) {
            let entry = serde_json::json!({
                "seq": event.seq,
                "mono_us": event.mono_us,
                "event_type": event.event_type,
                "severity": event.severity,
                "payload": payload,
            });
            if let Ok(line) = serde_jcs::to_vec(&entry) {
                output.extend_from_slice(&line);
                output.push(b'\n');
            }
        }
    }
    output
}

/// Compute Merkle root of events.
///
/// Returns an error if any event fails to parse, preventing silent event dropping
/// which could allow attackers to corrupt events and still pass integrity checks.
fn compute_events_merkle_root(events: &[DiagEventRecord]) -> Result<B3Hash, String> {
    if events.is_empty() {
        return Ok(B3Hash::hash(b"empty"));
    }

    // Hash each event - fail if any event cannot be parsed
    let hashes: Vec<B3Hash> = events
        .iter()
        .enumerate()
        .map(|(idx, e)| {
            let payload = serde_json::from_str::<serde_json::Value>(&e.payload_json)
                .map_err(|err| format!("Failed to parse event {} (seq={}): {}", idx, e.seq, err))?;
            let entry = serde_json::json!({
                "seq": e.seq,
                "mono_us": e.mono_us,
                "event_type": e.event_type,
                "severity": e.severity,
                "payload": payload,
            });
            let canonical = serde_jcs::to_vec(&entry).map_err(|err| {
                format!(
                    "Failed to canonicalize event {} (seq={}): {}",
                    idx, e.seq, err
                )
            })?;
            Ok(B3Hash::hash(&canonical))
        })
        .collect::<Result<Vec<_>, String>>()?;

    // Prepend event count hash to prevent count manipulation attacks
    let count_hash = B3Hash::hash(format!("event_count:{}", events.len()).as_bytes());
    let mut hashes_with_count = vec![count_hash];
    hashes_with_count.extend(hashes);
    let mut hashes = hashes_with_count;

    // Build Merkle tree
    while hashes.len() > 1 {
        let mut next_level = Vec::new();
        for chunk in hashes.chunks(2) {
            if chunk.len() == 2 {
                let mut combined = Vec::new();
                combined.extend_from_slice(chunk[0].as_bytes());
                combined.extend_from_slice(chunk[1].as_bytes());
                next_level.push(B3Hash::hash(&combined));
            } else {
                next_level.push(chunk[0]);
            }
        }
        hashes = next_level;
    }

    Ok(hashes.pop().unwrap_or_else(|| B3Hash::hash(b"empty")))
}

/// Get code identity (git SHA or build timestamp).
fn get_code_identity() -> Option<String> {
    // Try git SHA first
    if let Ok(output) = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
    {
        if output.status.success() {
            let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !sha.is_empty() {
                return Some(sha);
            }
        }
    }

    // Fall back to build timestamp
    Some(format!("build-{}", env!("CARGO_PKG_VERSION")))
}

/// Create tar.zst bundle.
fn create_tar_zst_bundle(
    manifest_data: &[u8],
    events_data: &[u8],
    config_data: &[u8],
    identity_data: &[u8],
) -> std::io::Result<Vec<u8>> {
    let mut output = Vec::new();

    // Create zstd encoder
    let encoder = zstd::stream::Encoder::new(&mut output, 3)?;
    let mut tar_builder = tar::Builder::new(encoder);

    // Add manifest.json
    add_file_to_tar(&mut tar_builder, "manifest.json", manifest_data)?;

    // Add events.ndjson
    add_file_to_tar(&mut tar_builder, "events.ndjson", events_data)?;

    // Add config_snapshot.json
    add_file_to_tar(&mut tar_builder, "config_snapshot.json", config_data)?;

    // Add identity.json
    add_file_to_tar(&mut tar_builder, "identity.json", identity_data)?;

    // Finish tar and zstd
    let encoder = tar_builder.into_inner()?;
    encoder.finish()?;

    Ok(output)
}

/// Add a file to tar archive.
fn add_file_to_tar<W: Write>(
    tar: &mut tar::Builder<W>,
    name: &str,
    data: &[u8],
) -> std::io::Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_path(name)?;
    header.set_size(data.len() as u64);
    header.set_mode(0o644);
    header.set_mtime(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    );
    header.set_cksum();

    tar.append(&header, data)?;
    Ok(())
}
