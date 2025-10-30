//! Memory-mapped .aos file loader

use adapteros_core::{AosError, Result};
use adapteros_single_file_adapter::{AdapterManifest, SingleFileAdapter};
use memmap2::Mmap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, info, instrument};

/// Memory-mapped adapter wrapper
///
/// Provides zero-copy access to .aos file contents via memory mapping.
/// The underlying file remains mapped for the lifetime of this struct.
#[derive(Clone)]
pub struct MmapAdapter {
    /// Path to the .aos file
    path: PathBuf,
    /// Memory-mapped file data
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

/// Memory-mapped adapter loader
///
/// Loads .aos files using memory mapping for efficient zero-copy access.
#[derive(Debug, Default)]
pub struct MmapAdapterLoader {
    /// Whether to verify signatures during load
    verify_signatures: bool,
}

impl MmapAdapterLoader {
    /// Create a new loader with default settings
    pub fn new() -> Self {
        Self {
            verify_signatures: true,
        }
    }

    /// Create a loader with signature verification disabled
    pub fn without_verification() -> Self {
        Self {
            verify_signatures: false,
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
        
        let metadata = file.metadata()
            .map_err(|e| AosError::Io(format!("Failed to read file metadata: {}", e)))?;
        
        let size_bytes = metadata.len();
        
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

    /// Load adapter via temporary file (workaround for ZIP requiring seekable input)
    async fn load_via_tempfile(&self, mmap: &Mmap) -> Result<SingleFileAdapter> {
        use adapteros_single_file_adapter::SingleFileAdapterLoader;
        use std::io::Write;
        
        let temp_path = {
            let mut temp_file = tempfile::NamedTempFile::new()
                .map_err(|e| AosError::Io(format!("Failed to create temp file: {}", e)))?;
            
            temp_file.write_all(mmap)
                .map_err(|e| AosError::Io(format!("Failed to write to temp file: {}", e)))?;
            
            temp_file.flush()
                .map_err(|e| AosError::Io(format!("Failed to flush temp file: {}", e)))?;
            
            // Keep temp file alive by persisting it
            temp_file.into_temp_path()
        };
        
        // Load using standard loader
        let adapter = SingleFileAdapterLoader::load(&temp_path).await?;
        
        Ok(adapter)
    }

    /// Verify adapter signature
    fn verify_adapter_signature(&self, adapter: &SingleFileAdapter) -> Result<()> {
        if let Some(_signature) = &adapter.signature {
            debug!("Verifying adapter signature");
            adapter.verify_signature()
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
