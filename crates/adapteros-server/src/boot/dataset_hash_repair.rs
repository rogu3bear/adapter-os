//! Dataset hash repair for boot-time recomputation.
//!
//! This module fixes the dataset_hash_b3 values that were incorrectly backfilled
//! by migration 0230 (which used single-file hash_b3 instead of proper manifest hash).
//!
//! The repair runs once during boot to recompute hashes using the correct algorithm:
//! BLAKE3(sort(["{normalized_file_name}:{size_bytes}:{file_hash_b3}", ...]))

use adapteros_db::Db;
use adapteros_server_api::handlers::datasets::hashing::{hash_dataset_manifest, DatasetHashInput};
use anyhow::Result;
use tracing::{debug, info, warn};

/// Repair dataset hashes that were incorrectly computed.
///
/// Queries all datasets marked with `hash_needs_recompute = 1` and recomputes
/// their `dataset_hash_b3` from constituent files using the normalized manifest algorithm.
///
/// # Arguments
///
/// * `db` - Database connection
///
/// # Returns
///
/// Number of datasets repaired, or error if repair fails
pub async fn repair_dataset_hashes(db: &Db) -> Result<usize> {
    let datasets = db.get_datasets_needing_hash_recompute().await?;

    if datasets.is_empty() {
        debug!("No datasets need hash recomputation");
        return Ok(0);
    }

    info!(
        count = datasets.len(),
        "Found datasets needing hash recomputation"
    );

    let mut repaired = 0;

    for dataset in datasets {
        let files = db.get_dataset_files(&dataset.id).await?;

        let new_hash = if files.is_empty() {
            // No constituent files recorded - use existing hash_b3 as fallback
            // This handles legacy datasets that predate the dataset_files table
            debug!(
                dataset_id = %dataset.id,
                "No files found, using existing hash_b3 as fallback"
            );
            dataset.hash_b3.clone()
        } else {
            // Compute proper manifest hash from constituent files
            let manifest: Vec<DatasetHashInput> = files
                .iter()
                .map(|f| DatasetHashInput {
                    file_name: f.file_name.clone(),
                    size_bytes: f.size_bytes as u64,
                    file_hash_b3: f.hash_b3.clone(),
                })
                .collect();
            hash_dataset_manifest(&manifest)
        };

        // Update with new hash and mark as v2 algorithm
        if let Err(e) = db.update_dataset_hash(&dataset.id, &new_hash, 2).await {
            warn!(
                dataset_id = %dataset.id,
                error = %e,
                "Failed to update dataset hash, will retry on next boot"
            );
            continue;
        }

        debug!(
            dataset_id = %dataset.id,
            old_hash = %dataset.dataset_hash_b3,
            new_hash = %new_hash,
            file_count = files.len(),
            "Recomputed dataset hash"
        );
        repaired += 1;
    }

    info!(repaired, "Dataset hash repair complete");
    Ok(repaired)
}
