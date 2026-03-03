//! Adapter hash repair command
//!
//! Repairs missing `content_hash_b3` and `manifest_hash` fields for adapters
//! that were registered before these fields became mandatory for preflight.
//!
//! # Background
//!
//! As of preflight hardening, adapters require both `content_hash_b3` and
//! `manifest_hash` to pass alias swap preflight checks. Older adapters may
//! be missing one or both fields, blocking them from activation.
//!
//! # Usage
//!
//! ```bash
//! # Repair a single adapter
//! aosctl adapter repair-hashes --adapter-id my-adapter
//!
//! # Preview changes without updating (dry-run)
//! aosctl adapter repair-hashes --adapter-id my-adapter --dry-run
//!
//! # Repair all adapters for a tenant
//! aosctl adapter repair-hashes --tenant-id tenant-123
//! ```
//!
//! # Hash Computation
//!
//! - `content_hash_b3`: BLAKE3(manifest_bytes || canonical_segment_payload)
//! - `manifest_hash`: BLAKE3(manifest_bytes)

use crate::output::OutputWriter;
use adapteros_aos::{compute_scope_hash, open_aos, BackendTag};
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_db::{Adapter, Db};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{info, warn};

/// Result of a hash repair operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashRepairResult {
    pub adapter_id: String,
    pub tenant_id: String,
    pub aos_file_path: String,
    pub content_hash_b3_before: Option<String>,
    pub content_hash_b3_after: Option<String>,
    pub manifest_hash_before: Option<String>,
    pub manifest_hash_after: Option<String>,
    pub repaired: bool,
    pub error: Option<String>,
}

/// Compute hashes from .aos file
///
/// Returns (content_hash_b3, manifest_hash) if successful.
fn compute_hashes_from_aos(aos_path: &Path) -> Result<(String, String)> {
    let bytes = std::fs::read(aos_path).map_err(|e| {
        AosError::Io(format!(
            "Failed to read .aos file {}: {}",
            aos_path.display(),
            e
        ))
    })?;

    let view = open_aos(&bytes).map_err(|e| {
        AosError::Validation(format!(
            "Failed to parse .aos file {}: {}",
            aos_path.display(),
            e
        ))
    })?;

    // Compute manifest hash
    let manifest_hash = B3Hash::hash(view.manifest_bytes).to_hex();

    // Find canonical segment for content hash
    let scope_path = serde_json::from_slice::<serde_json::Value>(view.manifest_bytes)
        .ok()
        .and_then(|manifest| manifest.get("metadata").cloned())
        .and_then(|meta| meta.get("scope_path").cloned())
        .and_then(|val| val.as_str().map(|s| s.to_string()));

    let canonical_segment = scope_path
        .as_deref()
        .map(compute_scope_hash)
        .and_then(|scope_hash| {
            view.segments.iter().find(|seg| {
                seg.backend_tag == BackendTag::Canonical && seg.scope_hash == scope_hash
            })
        })
        .or_else(|| {
            view.segments
                .iter()
                .find(|seg| seg.backend_tag == BackendTag::Canonical)
        })
        .or_else(|| view.segments.first());

    let content_hash_b3 = match canonical_segment {
        Some(segment) => B3Hash::hash_multi(&[view.manifest_bytes, segment.payload]).to_hex(),
        None => {
            return Err(AosError::Validation(format!(
                "No segments found in .aos file {}",
                aos_path.display()
            )));
        }
    };

    Ok((content_hash_b3, manifest_hash))
}

/// Check if adapter needs hash repair
fn needs_repair(adapter: &Adapter) -> (bool, bool) {
    let needs_content_hash = adapter
        .content_hash_b3
        .as_ref()
        .map(|h| h.trim().is_empty())
        .unwrap_or(true);

    let needs_manifest_hash = adapter
        .manifest_hash
        .as_ref()
        .map(|h| h.trim().is_empty())
        .unwrap_or(true);

    (needs_content_hash, needs_manifest_hash)
}

/// Repair hashes for a single adapter
pub async fn repair_single_adapter(
    db: &Db,
    adapter_id: &str,
    tenant_id: Option<&str>,
    dry_run: bool,
    output: &OutputWriter,
) -> Result<HashRepairResult> {
    // Fetch adapter
    #[allow(deprecated)]
    let adapter = match tenant_id {
        Some(tid) => db.get_adapter_for_tenant(tid, adapter_id).await?,
        None => db.get_adapter(adapter_id).await?,
    };

    let adapter = match adapter {
        Some(a) => a,
        None => {
            return Ok(HashRepairResult {
                adapter_id: adapter_id.to_string(),
                tenant_id: tenant_id.unwrap_or("unknown").to_string(),
                aos_file_path: String::new(),
                content_hash_b3_before: None,
                content_hash_b3_after: None,
                manifest_hash_before: None,
                manifest_hash_after: None,
                repaired: false,
                error: Some(format!("Adapter '{}' not found", adapter_id)),
            });
        }
    };

    let aos_path = match adapter.aos_file_path.as_ref() {
        Some(path) if !path.trim().is_empty() => path.clone(),
        _ => {
            return Ok(HashRepairResult {
                adapter_id: adapter_id.to_string(),
                tenant_id: adapter.tenant_id.clone(),
                aos_file_path: String::new(),
                content_hash_b3_before: adapter.content_hash_b3.clone(),
                content_hash_b3_after: None,
                manifest_hash_before: adapter.manifest_hash.clone(),
                manifest_hash_after: None,
                repaired: false,
                error: Some("Adapter has no .aos file path - cannot compute hashes".to_string()),
            });
        }
    };

    let (needs_content, needs_manifest) = needs_repair(&adapter);

    if !needs_content && !needs_manifest {
        output.info(format!(
            "Adapter '{}' already has both hashes set - no repair needed",
            adapter_id
        ));
        return Ok(HashRepairResult {
            adapter_id: adapter_id.to_string(),
            tenant_id: adapter.tenant_id.clone(),
            aos_file_path: aos_path,
            content_hash_b3_before: adapter.content_hash_b3.clone(),
            content_hash_b3_after: adapter.content_hash_b3.clone(),
            manifest_hash_before: adapter.manifest_hash.clone(),
            manifest_hash_after: adapter.manifest_hash.clone(),
            repaired: false,
            error: None,
        });
    }

    // Compute hashes from .aos file
    let (content_hash, manifest_hash) = match compute_hashes_from_aos(Path::new(&aos_path)) {
        Ok(hashes) => hashes,
        Err(e) => {
            return Ok(HashRepairResult {
                adapter_id: adapter_id.to_string(),
                tenant_id: adapter.tenant_id.clone(),
                aos_file_path: aos_path,
                content_hash_b3_before: adapter.content_hash_b3.clone(),
                content_hash_b3_after: None,
                manifest_hash_before: adapter.manifest_hash.clone(),
                manifest_hash_after: None,
                repaired: false,
                error: Some(format!("Failed to compute hashes: {}", e)),
            });
        }
    };

    let new_content_hash = if needs_content {
        Some(content_hash.clone())
    } else {
        adapter.content_hash_b3.clone()
    };

    let new_manifest_hash = if needs_manifest {
        Some(manifest_hash.clone())
    } else {
        adapter.manifest_hash.clone()
    };

    if dry_run {
        output.info(format!("[DRY-RUN] Would repair adapter '{}':", adapter_id));
        if needs_content {
            output.kv("  content_hash_b3", &format!("NULL -> {}", content_hash));
        }
        if needs_manifest {
            output.kv("  manifest_hash", &format!("NULL -> {}", manifest_hash));
        }

        return Ok(HashRepairResult {
            adapter_id: adapter_id.to_string(),
            tenant_id: adapter.tenant_id.clone(),
            aos_file_path: aos_path,
            content_hash_b3_before: adapter.content_hash_b3.clone(),
            content_hash_b3_after: new_content_hash,
            manifest_hash_before: adapter.manifest_hash.clone(),
            manifest_hash_after: new_manifest_hash,
            repaired: false, // dry-run doesn't actually repair
            error: None,
        });
    }

    // Update adapter with new hashes
    let update_result = db
        .update_adapter_hashes(
            &adapter.tenant_id,
            &adapter.id,
            new_content_hash.as_deref(),
            new_manifest_hash.as_deref(),
        )
        .await;

    match update_result {
        Ok(_) => {
            info!(
                adapter_id = %adapter_id,
                content_hash = %content_hash,
                manifest_hash = %manifest_hash,
                code = "HASH_REPAIR_SUCCESS",
                "Repaired adapter hashes"
            );

            output.success(format!("Repaired hashes for adapter '{}'", adapter_id));
            if needs_content {
                output.kv("  content_hash_b3", &content_hash);
            }
            if needs_manifest {
                output.kv("  manifest_hash", &manifest_hash);
            }

            Ok(HashRepairResult {
                adapter_id: adapter_id.to_string(),
                tenant_id: adapter.tenant_id.clone(),
                aos_file_path: aos_path,
                content_hash_b3_before: adapter.content_hash_b3.clone(),
                content_hash_b3_after: new_content_hash,
                manifest_hash_before: adapter.manifest_hash.clone(),
                manifest_hash_after: new_manifest_hash,
                repaired: true,
                error: None,
            })
        }
        Err(e) => {
            warn!(
                adapter_id = %adapter_id,
                error = %e,
                code = "HASH_REPAIR_FAILED",
                "Failed to update adapter hashes"
            );

            Ok(HashRepairResult {
                adapter_id: adapter_id.to_string(),
                tenant_id: adapter.tenant_id.clone(),
                aos_file_path: aos_path,
                content_hash_b3_before: adapter.content_hash_b3.clone(),
                content_hash_b3_after: None,
                manifest_hash_before: adapter.manifest_hash.clone(),
                manifest_hash_after: None,
                repaired: false,
                error: Some(format!("Database update failed: {}", e)),
            })
        }
    }
}

/// Repair hashes for all adapters in a tenant
pub async fn repair_tenant_adapters(
    db: &Db,
    tenant_id: &str,
    dry_run: bool,
    batch_size: i64,
    output: &OutputWriter,
) -> Result<Vec<HashRepairResult>> {
    let adapters = db
        .find_adapters_with_missing_hashes(Some(tenant_id), batch_size)
        .await?;

    if adapters.is_empty() {
        output.info(format!(
            "No adapters with missing hashes found for tenant '{}'",
            tenant_id
        ));
        return Ok(Vec::new());
    }

    output.info(format!(
        "Found {} adapters with missing hashes for tenant '{}'",
        adapters.len(),
        tenant_id
    ));

    let mut results = Vec::new();
    let mut repaired_count = 0;
    let mut skipped_count = 0;
    let mut error_count = 0;

    for adapter in &adapters {
        let adapter_id = match adapter.adapter_id.as_deref() {
            Some(id) => id,
            None => {
                skipped_count += 1;
                results.push(HashRepairResult {
                    adapter_id: adapter.id.clone(),
                    tenant_id: tenant_id.to_string(),
                    aos_file_path: adapter.aos_file_path.clone().unwrap_or_default(),
                    content_hash_b3_before: adapter.content_hash_b3.clone(),
                    content_hash_b3_after: adapter.content_hash_b3.clone(),
                    manifest_hash_before: adapter.manifest_hash.clone(),
                    manifest_hash_after: adapter.manifest_hash.clone(),
                    repaired: false,
                    error: Some("Adapter missing adapter_id; skipping repair".to_string()),
                });
                continue;
            }
        };

        let result =
            repair_single_adapter(db, adapter_id, Some(tenant_id), dry_run, output).await?;

        if result.repaired {
            repaired_count += 1;
        } else if result.error.is_some() {
            error_count += 1;
        } else {
            skipped_count += 1;
        }

        results.push(result);
    }

    output.blank();
    output.section("Repair Summary");
    output.kv("Total", &adapters.len().to_string());
    output.kv("Repaired", &repaired_count.to_string());
    output.kv("Skipped", &skipped_count.to_string());
    output.kv("Errors", &error_count.to_string());

    if dry_run {
        output.info("(Dry-run mode - no changes were made)");
    }

    Ok(results)
}

/// Run the adapter hash repair command
pub async fn run(
    adapter_id: Option<&str>,
    tenant_id: Option<&str>,
    dry_run: bool,
    batch_size: i64,
    output: &OutputWriter,
) -> Result<()> {
    output.section("Adapter Hash Repair");

    let db = Db::connect_env()
        .await
        .map_err(|e| AosError::Database(format!("Failed to connect to database: {}", e)))?;

    if let Some(aid) = adapter_id {
        // Single adapter repair
        output.kv("Mode", "Single adapter");
        output.kv("Adapter ID", aid);
        if dry_run {
            output.kv("Dry-run", "true");
        }
        output.blank();

        let result = repair_single_adapter(&db, aid, tenant_id, dry_run, output).await?;

        if output.is_json() {
            output.json(&result)?;
        }
    } else if let Some(tid) = tenant_id {
        // Tenant-wide repair
        output.kv("Mode", "Tenant batch");
        output.kv("Tenant ID", tid);
        output.kv("Batch size", &batch_size.to_string());
        if dry_run {
            output.kv("Dry-run", "true");
        }
        output.blank();

        let results = repair_tenant_adapters(&db, tid, dry_run, batch_size, output).await?;

        if output.is_json() {
            output.json(&results)?;
        }
    } else {
        return Err(AosError::Validation(
            "Either --adapter-id or --tenant-id is required".to_string(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stub_adapter() -> Adapter {
        Adapter {
            id: "adapter-id".to_string(),
            tenant_id: "tenant-1".to_string(),
            name: "adapter-name".to_string(),
            tier: "warm".to_string(),
            hash_b3: "hash".to_string(),
            rank: 1,
            alpha: 2.0,
            lora_strength: Some(1.0),
            targets_json: "[]".to_string(),
            acl_json: None,
            adapter_id: Some("adapter-id".to_string()),
            languages_json: None,
            framework: None,
            active: 1,
            category: "code".to_string(),
            scope: "global".to_string(),
            framework_id: None,
            framework_version: None,
            repo_id: None,
            commit_sha: None,
            intent: None,
            current_state: "cold".to_string(),
            pinned: 0,
            memory_bytes: 0,
            last_activated: None,
            activation_count: 0,
            expires_at: None,
            load_state: "unloaded".to_string(),
            last_loaded_at: None,
            aos_file_path: None,
            aos_file_hash: None,
            adapter_name: None,
            tenant_namespace: None,
            domain: None,
            purpose: None,
            revision: None,
            parent_id: None,
            fork_type: None,
            fork_reason: None,
            version: "1".to_string(),
            lifecycle_state: "draft".to_string(),
            archived_at: None,
            archived_by: None,
            archive_reason: None,
            purged_at: None,
            base_model_id: None,
            recommended_for_moe: None,
            manifest_schema_version: None,
            content_hash_b3: None,
            metadata_json: None,
            provenance_json: None,
            repo_path: None,
            drift_tier: None,
            drift_metric: None,
            drift_loss_metric: None,
            drift_reference_backend: None,
            drift_baseline_backend: None,
            drift_test_backend: None,
            codebase_scope: None,
            dataset_version_id: None,
            registration_timestamp: None,
            manifest_hash: None,
            adapter_type: None,
            base_adapter_id: None,
            coreml_package_hash: None,
            stream_session_id: None,
            versioning_threshold: None,
            training_dataset_hash_b3: None,
            adapter_version_id: None,
            effective_version_weight: None,
            stable_id: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }
    }

    #[test]
    fn test_needs_repair_both_missing() {
        let adapter = Adapter {
            content_hash_b3: None,
            manifest_hash: None,
            ..stub_adapter()
        };
        let (needs_content, needs_manifest) = needs_repair(&adapter);
        assert!(needs_content);
        assert!(needs_manifest);
    }

    #[test]
    fn test_needs_repair_content_empty() {
        let adapter = Adapter {
            content_hash_b3: Some("".to_string()),
            manifest_hash: Some("abc123".to_string()),
            ..stub_adapter()
        };
        let (needs_content, needs_manifest) = needs_repair(&adapter);
        assert!(needs_content);
        assert!(!needs_manifest);
    }

    #[test]
    fn test_needs_repair_none_needed() {
        let adapter = Adapter {
            content_hash_b3: Some("hash1".to_string()),
            manifest_hash: Some("hash2".to_string()),
            ..stub_adapter()
        };
        let (needs_content, needs_manifest) = needs_repair(&adapter);
        assert!(!needs_content);
        assert!(!needs_manifest);
    }
}
