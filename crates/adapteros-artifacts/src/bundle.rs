//! Bundle creation and extraction (tar+zstd)

use adapteros_core::{AosError, Result};
use std::fs::File;
use std::path::Path;
use tar::{Archive, Builder};

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
    archive
        .unpack(output_dir.as_ref())
        .map_err(|e| AosError::Artifact(format!("Failed to extract: {}", e)))?;

    Ok(())
}

/// Compute bundle size
pub fn bundle_size<P: AsRef<Path>>(bundle: P) -> Result<u64> {
    let metadata = std::fs::metadata(bundle.as_ref())
        .map_err(|e| AosError::Artifact(format!("Failed to get metadata: {}", e)))?;
    Ok(metadata.len())
}
