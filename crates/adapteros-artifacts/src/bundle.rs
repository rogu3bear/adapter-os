//! Bundle creation and extraction (tar+zstd)

use adapteros_core::{AosError, Result};
use adapteros_storage::secure_fs::path_policy::{
    canonicalize_strict, canonicalize_strict_in_allowed_roots,
};
use adapteros_storage::secure_fs::traversal::check_path_traversal;
use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::Path;
use tar::{Archive, Builder};
use tracing::error;

/// Create a tar+zstd bundle from a directory
pub fn create_bundle<P: AsRef<Path>, Q: AsRef<Path>>(source_dir: P, output: Q) -> Result<()> {
    let file = File::create(output.as_ref())
        .map_err(|e| AosError::Artifact(format!("Failed to create bundle file: {}", e)))?;

    let encoder = zstd::Encoder::new(file, 3)
        .map_err(|e| AosError::Artifact(format!("Failed to create encoder: {}", e)))?;

    let mut builder = Builder::new(encoder);
    builder
        .append_dir_all(".", source_dir.as_ref())
        .map_err(|e| AosError::Artifact(format!("Failed to append directory: {}", e)))?;

    let encoder = builder
        .into_inner()
        .map_err(|e| AosError::Artifact(format!("Failed to finish tar: {}", e)))?;

    encoder
        .finish()
        .map_err(|e| AosError::Artifact(format!("Failed to finish compression: {}", e)))?;

    Ok(())
}

/// Extract a tar+zstd bundle to a directory
pub fn extract_bundle<P: AsRef<Path>, Q: AsRef<Path>>(bundle: P, output_dir: Q) -> Result<()> {
    let file = File::open(bundle.as_ref())
        .map_err(|e| AosError::Artifact(format!("Failed to open bundle: {}", e)))?;

    let decoder = zstd::Decoder::new(file)
        .map_err(|e| AosError::Artifact(format!("Failed to create decoder: {}", e)))?;

    let mut archive = Archive::new(decoder);
    extract_tar_entries(&mut archive, output_dir.as_ref())?;

    Ok(())
}

/// Compute bundle size
pub fn bundle_size<P: AsRef<Path>>(bundle: P) -> Result<u64> {
    let metadata = std::fs::metadata(bundle.as_ref())
        .map_err(|e| AosError::Artifact(format!("Failed to get metadata: {}", e)))?;
    Ok(metadata.len())
}

fn extract_tar_entries<R: Read>(archive: &mut Archive<R>, output_dir: &Path) -> Result<()> {
    fs::create_dir_all(output_dir).map_err(|e| {
        AosError::Artifact(format!(
            "Failed to create output directory {}: {}",
            output_dir.display(),
            e
        ))
    })?;
    let canonical_output_dir = canonicalize_strict(output_dir)
        .map_err(|e| AosError::Artifact(format!("Failed to canonicalize output dir: {}", e)))?;
    let allowed_roots = [canonical_output_dir.clone()];

    for entry in archive
        .entries()
        .map_err(|e| AosError::Artifact(format!("Failed to read bundle entries: {}", e)))?
    {
        let mut entry = entry.map_err(|e| AosError::Artifact(format!("Entry error: {}", e)))?;
        let entry_path = entry
            .path()
            .map_err(|e| AosError::Artifact(format!("Entry path error: {}", e)))?;
        validate_archive_entry_path(&entry_path, &entry_path.to_string_lossy())?;

        let entry_type = entry.header().entry_type();
        if entry_type.is_symlink() || entry_type.is_hard_link() {
            return Err(AosError::Artifact(format!(
                "Bundle entry is a link and was rejected: {}",
                entry_path.display()
            )));
        }

        let output_path = canonical_output_dir.join(&entry_path);
        if entry_type.is_dir() {
            fs::create_dir_all(&output_path).map_err(|e| {
                AosError::Artifact(format!(
                    "Failed to create directory {}: {}",
                    output_path.display(),
                    e
                ))
            })?;
            canonicalize_strict_in_allowed_roots(&output_path, &allowed_roots)
                .map_err(|e| AosError::Artifact(format!("Bundle path rejected: {}", e)))?;
            continue;
        }

        if !entry_type.is_file() {
            return Err(AosError::Artifact(format!(
                "Unsupported bundle entry type for {}",
                entry_path.display()
            )));
        }

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                AosError::Artifact(format!(
                    "Failed to create directory {}: {}",
                    parent.display(),
                    e
                ))
            })?;
            canonicalize_strict_in_allowed_roots(parent, &allowed_roots)
                .map_err(|e| AosError::Artifact(format!("Bundle path rejected: {}", e)))?;
        }

        if output_path.exists() {
            let metadata = fs::symlink_metadata(&output_path).map_err(|e| {
                AosError::Artifact(format!(
                    "Failed to read metadata for {}: {}",
                    output_path.display(),
                    e
                ))
            })?;
            if metadata.file_type().is_symlink() {
                return Err(AosError::Artifact(format!(
                    "Bundle entry path is a symlink and was rejected: {}",
                    output_path.display()
                )));
            }
        }

        let mut output_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output_path)
            .map_err(|e| {
                AosError::Artifact(format!(
                    "Failed to create output file {}: {}",
                    output_path.display(),
                    e
                ))
            })?;

        std::io::copy(&mut entry, &mut output_file).map_err(|e| {
            AosError::Artifact(format!(
                "Failed to extract bundle entry {}: {}",
                output_path.display(),
                e
            ))
        })?;

        canonicalize_strict_in_allowed_roots(&output_path, &allowed_roots)
            .map_err(|e| AosError::Artifact(format!("Bundle path rejected: {}", e)))?;
    }

    Ok(())
}

fn validate_archive_entry_path(entry_path: &Path, entry_name: &str) -> Result<()> {
    if entry_path.is_absolute() {
        error!(
            original = %entry_name,
            canonical = "<unavailable>",
            "Bundle entry path rejected (absolute)"
        );
        return Err(AosError::Artifact(format!(
            "Bundle entry path is absolute: {}",
            entry_name
        )));
    }

    check_path_traversal(entry_path).map_err(|e| {
        error!(
            original = %entry_name,
            canonical = "<unavailable>",
            error = %e,
            "Bundle entry path rejected (traversal)"
        );
        AosError::Artifact(format!("Bundle entry path rejected: {}", entry_name))
    })?;

    Ok(())
}
