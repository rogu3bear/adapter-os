//! Memory-mapped .aos file loader (DEPRECATED)
//!
//! # Deprecation Notice
//!
//! This module is **deprecated** and non-functional. The canonical AOS loading
//! implementation is in [`crate::implementation`], which provides:
//!
//! - [`AosLoader`](crate::AosLoader) - The working AOS file loader with Metal integration
//! - [`AosManifest`](crate::AosManifest) - Proper manifest structure matching docs/AOS_FORMAT.md
//! - [`LoadedAdapter`](crate::LoadedAdapter) - Loaded adapter with Metal buffers
//!
//! ## Migration
//!
//! Replace usage of this module's types:
//! - `MmapAdapterLoader` -> `AosLoader`
//! - `MmapAdapter` -> `LoadedAdapter`
//! - `AdapterManifest` -> `AosManifest`
//!
//! ## Why This Module Exists
//!
//! This module was created during the migration from the deleted
//! `adapteros-single-file-adapter` crate. The proper implementation now lives in
//! `implementation.rs`. This module is retained only for API compatibility with
//! `manager.rs`, `hot_swap.rs`, and `cache.rs`, which should be migrated to use
//! the canonical implementation.
//!
//! ## Status
//!
//! All methods in this module return errors. Do not use for new code.

use adapteros_core::{AosError, Result};
// Removed: use adapteros_single_file_adapter::{AdapterManifest, SingleFileAdapter};
use memmap2::Mmap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, info, instrument};

// DEPRECATED: These are stub types that do not function.
// Use crate::AosManifest, crate::AosLoader, and crate::LoadedAdapter instead.
// See crate::implementation for the canonical working implementation.

/// Stub manifest for mmap loader (DEPRECATED - use crate::AosManifest instead)
#[deprecated(
    since = "0.1.0",
    note = "Use crate::AosManifest from crate::implementation instead"
)]
#[derive(Debug, Clone, Default)]
pub struct AdapterManifest {
    pub adapter_id: String,
    pub version: String,
}

/// Stub adapter for mmap loader (DEPRECATED - use crate::LoadedAdapter instead)
#[deprecated(
    since = "0.1.0",
    note = "Use crate::LoadedAdapter from crate::implementation instead"
)]
#[derive(Debug, Clone, Default)]
pub struct SingleFileAdapter {
    pub manifest: AdapterManifest,
    pub signature: Option<Vec<u8>>,
}

impl SingleFileAdapter {
    /// Stub verification - always succeeds (DEPRECATED)
    pub fn verify_signature(&self) -> std::result::Result<(), String> {
        // DEPRECATED: This is a stub that always succeeds.
        // Use crate::AosLoader for proper loading with validation.
        Ok(())
    }
}

/// Memory-mapped adapter wrapper (DEPRECATED)
///
/// **DEPRECATED**: Use [`crate::LoadedAdapter`] from [`crate::AosLoader`] instead.
///
/// This type was intended to provide zero-copy access to .aos file contents via
/// memory mapping, but the implementation is incomplete and non-functional.
/// The canonical implementation is in [`crate::implementation`].
#[deprecated(
    since = "0.1.0",
    note = "Use crate::LoadedAdapter from crate::AosLoader instead"
)]
#[derive(Clone)]
pub struct MmapAdapter {
    /// Path to the .aos file
    path: PathBuf,
    /// Memory-mapped file data
    #[allow(dead_code)]
    mmap: Arc<Mmap>,
    /// Parsed adapter (lazily loaded)
    adapter: Arc<SingleFileAdapter>,
    /// File size in bytes
    size_bytes: u64,
}

impl MmapAdapter {
    /// Get the file path
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Get the adapter manifest
    pub fn manifest(&self) -> &AdapterManifest {
        &self.adapter.manifest
    }

    /// Get the full adapter
    pub fn adapter(&self) -> &SingleFileAdapter {
        &self.adapter
    }

    /// Get file size in bytes
    pub fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    /// Get adapter ID
    pub fn adapter_id(&self) -> &str {
        &self.adapter.manifest.adapter_id
    }

    /// Get adapter version
    pub fn version(&self) -> &str {
        &self.adapter.manifest.version
    }
}

impl std::fmt::Debug for MmapAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MmapAdapter")
            .field("path", &self.path)
            .field("adapter_id", &self.adapter.manifest.adapter_id)
            .field("version", &self.adapter.manifest.version)
            .field("size_bytes", &self.size_bytes)
            .finish()
    }
}

/// Memory-mapped adapter loader (DEPRECATED)
///
/// **DEPRECATED**: Use [`crate::AosLoader`] from [`crate::implementation`] instead.
///
/// This loader is non-functional. All load operations return an error.
/// The canonical working implementation is [`crate::AosLoader`].
#[deprecated(
    since = "0.1.0",
    note = "Use crate::AosLoader from crate::implementation instead"
)]
#[derive(Debug)]
pub struct MmapAdapterLoader {
    /// Whether to verify signatures during load
    verify_signatures: bool,
    /// Maximum file size in bytes (default: 500MB)
    max_file_size_bytes: u64,
}

impl Default for MmapAdapterLoader {
    fn default() -> Self {
        Self {
            verify_signatures: true,
            max_file_size_bytes: 500 * 1024 * 1024, // 500MB default
        }
    }
}

impl MmapAdapterLoader {
    /// Create a new loader with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a loader with signature verification disabled
    pub fn without_verification() -> Self {
        Self {
            verify_signatures: false,
            max_file_size_bytes: 500 * 1024 * 1024, // 500MB default
        }
    }

    /// Create a loader with custom maximum file size
    pub fn with_max_file_size(max_bytes: u64) -> Self {
        Self {
            verify_signatures: true,
            max_file_size_bytes: max_bytes,
        }
    }

    /// Load an adapter from the given path
    #[instrument(skip(self), fields(path = %path.as_ref().display()))]
    pub async fn load<P: AsRef<Path>>(&self, path: P) -> Result<MmapAdapter> {
        let path = path.as_ref();

        debug!("Opening .aos file");

        // Open file
        let file = File::open(path)
            .map_err(|e| AosError::Io(format!("Failed to open .aos file: {}", e)))?;

        let metadata = file
            .metadata()
            .map_err(|e| AosError::Io(format!("Failed to read file metadata: {}", e)))?;

        let size_bytes = metadata.len();

        // Check file size to prevent OOM attacks
        if size_bytes > self.max_file_size_bytes {
            return Err(AosError::PolicyViolation(format!(
                "Adapter file size {} bytes exceeds maximum {} bytes",
                size_bytes, self.max_file_size_bytes
            )));
        }

        // Create memory map
        let mmap = unsafe {
            Mmap::map(&file)
                .map_err(|e| AosError::Io(format!("Failed to memory-map file: {}", e)))?
        };

        debug!(size_bytes, "Memory-mapped .aos file");

        // Load adapter by writing mmap to temporary file
        // (ZIP crate requires seekable Read, which Mmap doesn't provide directly)
        let adapter = self.load_adapter_from_mmap(&mmap).await?;

        // Verify signature if enabled
        if self.verify_signatures {
            self.verify_adapter_signature(&adapter)?;
        }

        info!(
            adapter_id = %adapter.manifest.adapter_id,
            version = %adapter.manifest.version,
            size_bytes,
            "Loaded .aos adapter"
        );

        Ok(MmapAdapter {
            path: path.to_path_buf(),
            mmap: Arc::new(mmap),
            adapter: Arc::new(adapter),
            size_bytes,
        })
    }

    /// Load adapter from memory-mapped data
    async fn load_adapter_from_mmap(&self, mmap: &Mmap) -> Result<SingleFileAdapter> {
        self.load_via_tempfile(mmap).await
    }

    /// Load adapter via temporary file (DEPRECATED - always fails)
    ///
    /// This method is non-functional. Use [`crate::AosLoader::load_from_path`] instead.
    async fn load_via_tempfile(&self, _mmap: &Mmap) -> Result<SingleFileAdapter> {
        // DEPRECATED: This module is non-functional.
        // Use crate::AosLoader from crate::implementation for proper AOS loading.
        // See the module-level documentation for migration instructions.

        Err(AosError::Internal(
            "MmapAdapterLoader is deprecated. Use crate::AosLoader instead (see crate::implementation)".to_string(),
        ))
    }

    /// Verify adapter signature
    fn verify_adapter_signature(&self, adapter: &SingleFileAdapter) -> Result<()> {
        if let Some(_signature) = &adapter.signature {
            debug!("Verifying adapter signature");
            adapter
                .verify_signature()
                .map_err(|e| AosError::Crypto(format!("Signature verification failed: {}", e)))?;
            debug!("Signature verified successfully");
        } else {
            debug!("No signature present, skipping verification");
        }
        Ok(())
    }

    /// Load adapter synchronously (blocking)
    pub fn load_sync<P: AsRef<Path>>(&self, path: P) -> Result<MmapAdapter> {
        tokio::runtime::Handle::current().block_on(self.load(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_loader_creation() {
        let loader = MmapAdapterLoader::new();
        assert!(loader.verify_signatures);

        let loader = MmapAdapterLoader::without_verification();
        assert!(!loader.verify_signatures);
    }
}
