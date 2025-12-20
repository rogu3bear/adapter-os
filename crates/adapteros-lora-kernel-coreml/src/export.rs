//! CoreML export pipeline for converting `.aos` adapters into fused CoreML artifacts.
//!
//! This module provides a reusable helper that:
//! - Loads a base CoreML package (directory or Manifest.json path)
//! - Loads adapter weights from a `.aos` bundle
//! - Runs the CoreML kernel stub fusion path to ensure the LoRA branch executes
//! - Writes a fused copy of the CoreML package and emits a verification metadata file
//!
//! The fused artifact is intentionally produced via a copy to preserve base bytes and
//! determinism; the metadata contains hashes for the base manifest, fused manifest,
//! and adapter payload so callers can verify the combination later.

use crate::{ComputeUnits, CoreMLBackend};
use adapteros_aos::{open_aos, BackendTag};
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Request for exporting a CoreML fused package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreMLExportRequest {
    /// Path to the base CoreML package directory or its `Manifest.json`.
    pub base_package: PathBuf,
    /// Path to the adapter `.aos` bundle containing weights.
    pub adapter_aos: PathBuf,
    /// Destination path for the fused package (directory or manifest path).
    pub output_package: PathBuf,
    /// Compute units hint used when exercising the CoreML kernel path.
    #[serde(default)]
    pub compute_units: ComputeUnits,
}

/// Result of a CoreML export.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreMLExportOutcome {
    /// Path to the fused CoreML package (directory or manifest path).
    pub fused_package: PathBuf,
    /// Path to the verification metadata JSON written next to the fused package.
    pub metadata_path: PathBuf,
    pub base_manifest_hash: B3Hash,
    pub fused_manifest_hash: B3Hash,
    pub adapter_hash: B3Hash,
}

/// Verification metadata for a fused CoreML package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreMLFusionMetadata {
    pub base_manifest_hash: B3Hash,
    pub fused_manifest_hash: B3Hash,
    pub adapter_hash: B3Hash,
    pub base_package: PathBuf,
    pub fused_package: PathBuf,
    pub adapter_path: PathBuf,
}

impl CoreMLFusionMetadata {
    /// Verify that the stored hashes match the current on-disk artifacts.
    pub fn verify(&self) -> Result<()> {
        let base_manifest = manifest_path_for(&self.base_package)?;
        let fused_manifest = manifest_path_for(&self.fused_package)?;

        let actual_base = hash_manifest(&base_manifest)?;
        let actual_fused = hash_manifest(&fused_manifest)?;
        let actual_adapter = B3Hash::hash(&fs::read(&self.adapter_path).map_err(|e| {
            AosError::Io(format!(
                "Failed to read adapter for verification ({}): {}",
                self.adapter_path.display(),
                e
            ))
        })?);

        if actual_base != self.base_manifest_hash {
            return Err(AosError::Validation(
                "Base manifest hash mismatch during CoreML fusion verification".to_string(),
            ));
        }
        if actual_fused != self.fused_manifest_hash {
            return Err(AosError::Validation(
                "Fused manifest hash mismatch during CoreML fusion verification".to_string(),
            ));
        }
        if actual_adapter != self.adapter_hash {
            return Err(AosError::Validation(
                "Adapter hash mismatch during CoreML fusion verification".to_string(),
            ));
        }

        Ok(())
    }
}

/// Export an adapter into a CoreML package and emit verification metadata.
///
/// The function:
/// 1. Copies the base package to the output location.
/// 2. Loads the adapter weights from the `.aos` bundle.
/// 3. Exercises the CoreML kernel stub fusion path (when available) to ensure the LoRA
///    branch is covered without mutating base bytes.
/// 4. Writes a metadata JSON alongside the fused package containing hashes for later checks.
pub fn export_coreml_adapter(req: &CoreMLExportRequest) -> Result<CoreMLExportOutcome> {
    let manifest_path = manifest_path_for(&req.base_package)?;
    if !manifest_path.exists() {
        return Err(AosError::NotFound(format!(
            "CoreML manifest not found at {}",
            manifest_path.display()
        )));
    }

    let base_manifest_hash = hash_manifest(&manifest_path)?;

    let adapter_bytes = fs::read(&req.adapter_aos).map_err(|e| {
        AosError::Io(format!(
            "Failed to read adapter bundle {}: {}",
            req.adapter_aos.display(),
            e
        ))
    })?;
    let adapter_hash = B3Hash::hash(&adapter_bytes);
    let adapter_payload = adapter_payload_from_aos(&adapter_bytes)?;

    // Copy the base package to the output location (non-destructive).
    let fused_manifest_path = copy_package(&req.base_package, &req.output_package)?;

    // Exercise the fusion path in stub mode when available.
    #[cfg(any(test, debug_assertions, feature = "coreml-stub"))]
    {
        apply_stub_fusion(req.compute_units, &adapter_payload)?;
    }
    #[cfg(not(any(test, debug_assertions, feature = "coreml-stub")))]
    {
        warn!(
            "CoreML export ran without `coreml-stub`; fusion step skipped (metadata still emitted)"
        );
    }

    let fused_manifest_hash = hash_manifest(&fused_manifest_path)?;

    let metadata = CoreMLFusionMetadata {
        base_manifest_hash,
        fused_manifest_hash,
        adapter_hash,
        base_package: req.base_package.clone(),
        fused_package: req.output_package.clone(),
        adapter_path: req.adapter_aos.clone(),
    };

    let metadata_path = metadata_target_path(&req.output_package);
    if let Some(parent) = metadata_path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            AosError::Io(format!(
                "Failed to create metadata directory {}: {}",
                parent.display(),
                e
            ))
        })?;
    }
    fs::write(
        &metadata_path,
        serde_json::to_vec_pretty(&metadata).map_err(AosError::Serialization)?,
    )
    .map_err(|e| {
        AosError::Io(format!(
            "Failed to write fusion metadata at {}: {}",
            metadata_path.display(),
            e
        ))
    })?;

    info!(
        base = %req.base_package.display(),
        fused = %req.output_package.display(),
        metadata = %metadata_path.display(),
        "CoreML export completed"
    );

    Ok(CoreMLExportOutcome {
        fused_package: req.output_package.clone(),
        metadata_path,
        base_manifest_hash,
        fused_manifest_hash,
        adapter_hash,
    })
}

/// Validate a fused package against its metadata JSON.
pub fn validate_coreml_fusion(metadata_path: &Path) -> Result<CoreMLFusionMetadata> {
    let bytes = fs::read(metadata_path).map_err(|e| {
        AosError::Io(format!(
            "Failed to read fusion metadata {}: {}",
            metadata_path.display(),
            e
        ))
    })?;
    let metadata: CoreMLFusionMetadata =
        serde_json::from_slice(&bytes).map_err(AosError::Serialization)?;
    metadata.verify()?;
    Ok(metadata)
}

#[cfg(any(test, debug_assertions, feature = "coreml-stub"))]
fn apply_stub_fusion(compute_units: ComputeUnits, adapter_payload: &[u8]) -> Result<()> {
    let mut backend = CoreMLBackend::new_stub(compute_units)?;
    FusedKernels::load_adapter(&mut backend, 0, adapter_payload)?;

    let mut ring = RouterRing::new(1);
    ring.set(&[0u16], &[1200i16]);

    let mut io = IoBuffers::new(8);
    io.input_ids = vec![1, 2, 3];
    backend.run_step(&ring, &mut io)?;
    Ok(())
}

fn adapter_payload_from_aos(bytes: &[u8]) -> Result<Vec<u8>> {
    let file_view = open_aos(bytes)?;
    let segment = file_view
        .segments
        .iter()
        .find(|seg| seg.backend_tag == BackendTag::Coreml)
        .or_else(|| {
            file_view
                .segments
                .iter()
                .find(|seg| seg.backend_tag == BackendTag::Canonical)
        })
        .ok_or_else(|| {
            AosError::Validation(
                "Adapter bundle does not contain a CoreML or canonical segment".into(),
            )
        })?;

    Ok(segment.payload.to_vec())
}

fn manifest_path_for(path: &Path) -> Result<PathBuf> {
    if path.is_dir() {
        Ok(path.join("Manifest.json"))
    } else {
        Ok(path.to_path_buf())
    }
}

fn metadata_target_path(output_package: &Path) -> PathBuf {
    if output_package.is_dir() {
        output_package.join("adapteros_coreml_fusion.json")
    } else {
        output_package.with_extension("fusion.json")
    }
}

fn copy_package(base: &Path, dest: &Path) -> Result<PathBuf> {
    if base.is_dir() {
        copy_dir_recursive(base, dest)?;
        return Ok(dest.join("Manifest.json"));
    }

    let target =
        if dest.is_dir() || dest.extension().is_none() {
            dest.join(base.file_name().ok_or_else(|| {
                AosError::Validation("Missing filename for base package".to_string())
            })?)
        } else {
            dest.to_path_buf()
        };

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            AosError::Io(format!(
                "Failed to create output directory {}: {}",
                parent.display(),
                e
            ))
        })?;
    }

    fs::copy(base, &target).map_err(|e| {
        AosError::Io(format!(
            "Failed to copy base manifest {} to {}: {}",
            base.display(),
            target.display(),
            e
        ))
    })?;

    Ok(target)
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).map_err(|e| {
        AosError::Io(format!(
            "Failed to create destination directory {}: {}",
            dst.display(),
            e
        ))
    })?;

    for entry in fs::read_dir(src).map_err(|e| {
        AosError::Io(format!(
            "Failed to read source directory {}: {}",
            src.display(),
            e
        ))
    })? {
        let entry = entry.map_err(|e| {
            AosError::Io(format!(
                "Failed to read directory entry in {}: {}",
                src.display(),
                e
            ))
        })?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path).map_err(|e| {
                AosError::Io(format!(
                    "Failed to copy file {} to {}: {}",
                    src_path.display(),
                    dst_path.display(),
                    e
                ))
            })?;
        }
    }

    Ok(())
}

fn hash_manifest(manifest_path: &Path) -> Result<B3Hash> {
    let bytes = fs::read(manifest_path).map_err(|e| {
        AosError::Io(format!(
            "Failed to read CoreML manifest {}: {}",
            manifest_path.display(),
            e
        ))
    })?;
    Ok(B3Hash::hash(&bytes))
}
