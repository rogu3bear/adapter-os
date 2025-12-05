//! Hot-swap adapter loading and unloading

use adapteros_aos::{AosWriter, AOS_MAGIC, HEADER_SIZE};
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_single_file_adapter::format::AosSignature;
use memmap2::Mmap;
use safetensors::SafeTensors;
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use zeroize::Zeroize;
use zip::ZipArchive;

fn production_mode_enabled() -> bool {
    std::env::var("AOS_SERVER_PRODUCTION_MODE")
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

/// Loaded adapter weights with zeroize-on-drop
struct LoadedWeights {
    /// Raw weight data
    data: Vec<u8>,
    /// Memory-mapped file (kept alive for zero-copy access)
    _mmap: Option<Arc<Mmap>>,
}

impl Drop for LoadedWeights {
    fn drop(&mut self) {
        self.data.zeroize();
    }
}

/// Adapter metadata parsed from SafeTensors
#[derive(Debug, Clone)]
pub struct AdapterMetadata {
    /// Total number of parameters
    pub num_parameters: usize,
    /// LoRA rank (if detectable)
    pub rank: Option<usize>,
    /// Target modules (detected from tensor names)
    pub target_modules: Vec<String>,
}

/// Canonical logical layer identifier used for per-layer hashing
/// Example: "transformer.layer_12.attn.q_proj.lora_A"
fn canonical_layer_id(tensor_name: &str) -> String {
    let mut segments = Vec::new();
    let mut iter = tensor_name.split(|c| c == '.' || c == '/').peekable();

    while let Some(seg) = iter.next() {
        if seg.is_empty() {
            continue;
        }

        let lower = seg.to_lowercase();
        if lower == "weight" {
            continue;
        }

        if lower == "model" || lower == "transformer" {
            if segments.is_empty() {
                segments.push("transformer".to_string());
            }
            continue;
        }

        if lower == "layers" || lower == "layer" {
            if let Some(next) = iter.peek() {
                if let Ok(idx) = next.parse::<usize>() {
                    segments.push(format!("layer_{}", idx));
                    iter.next();
                    continue;
                }
            }
        }

        let normalized = match lower.as_str() {
            "lora_a" => "lora_A".to_string(),
            "lora_b" => "lora_B".to_string(),
            other => other.to_string(),
        };

        segments.push(normalized);
    }

    if segments.is_empty() {
        return tensor_name.to_string();
    }

    if segments[0] != "transformer" {
        let mut prefixed = vec!["transformer".to_string()];
        prefixed.extend(segments);
        segments = prefixed;
    }

    segments.join(".")
}

/// Adapter loader for hot-swap operations
pub struct AdapterLoader {
    /// Base path for adapter files
    base_path: PathBuf,
    /// Currently loaded adapters (adapter_id -> (path, weights))
    loaded: HashMap<u16, (PathBuf, LoadedWeights)>,
    /// Expected hashes from manifest
    expected_hashes: HashMap<String, B3Hash>,
    /// Whether to require signature verification for .aos files
    /// In debug builds, defaults to false (warn only)
    /// In release builds, defaults to true (enforced)
    require_signatures: bool,
}

impl AdapterLoader {
    /// Create a new adapter loader
    ///
    /// In release builds, signature verification is required by default.
    /// In debug builds, signature verification is optional (warns only).
    pub fn new(base_path: PathBuf, expected_hashes: HashMap<String, B3Hash>) -> Self {
        // Default: require signatures in release, warn-only in debug
        #[cfg(not(debug_assertions))]
        let mut require_signatures = true;
        #[cfg(debug_assertions)]
        let mut require_signatures = false;

        let production_mode = production_mode_enabled();
        if production_mode {
            require_signatures = true;
            tracing::warn!(
                production_mode,
                "Enforcing adapter signatures (production mode enabled)"
            );
        }

        Self {
            base_path,
            loaded: HashMap::new(),
            expected_hashes,
            require_signatures,
        }
    }

    /// Create a new adapter loader with explicit signature requirement
    pub fn new_with_options(
        base_path: PathBuf,
        expected_hashes: HashMap<String, B3Hash>,
        require_signatures: bool,
    ) -> Self {
        Self {
            base_path,
            loaded: HashMap::new(),
            expected_hashes,
            require_signatures,
        }
    }

    /// Set whether signatures are required
    pub fn set_require_signatures(&mut self, require: bool) {
        self.require_signatures = require;
    }

    #[cfg(test)]
    pub fn signatures_required(&self) -> bool {
        self.require_signatures
    }

    fn expected_hash(&self, adapter_name: &str) -> Result<B3Hash> {
        self.expected_hashes
            .get(adapter_name)
            .copied()
            .ok_or_else(|| {
                AosError::Lifecycle(format!(
                    "Missing expected hash for adapter {}",
                    adapter_name
                ))
            })
    }

    /// Register expected hash for a new adapter (called during import)
    pub fn register_hash(&mut self, adapter_name: String, hash: B3Hash) {
        self.expected_hashes.insert(adapter_name, hash);
    }

    /// Get the base path for adapter files
    pub fn adapters_base_path(&self) -> &PathBuf {
        &self.base_path
    }

    /// Load an adapter from disk (blocking call, use load_adapter_async for async contexts)
    pub fn load_adapter(&mut self, adapter_id: u16, adapter_name: &str) -> Result<AdapterHandle> {
        // Check for .aos file first, fall back to .safetensors
        let aos_path = self.base_path.join(format!("{}.aos", adapter_name));
        let safetensors_path = self.base_path.join(format!("{}.safetensors", adapter_name));

        let (adapter_path, weights_data, metadata) = if aos_path.exists() {
            tracing::debug!(
                adapter_name = adapter_name,
                path = %aos_path.display(),
                "Loading from .aos file"
            );
            let (data, meta) = self.load_from_aos(&aos_path)?;
            (aos_path, data, meta)
        } else if safetensors_path.exists() {
            tracing::debug!(
                adapter_name = adapter_name,
                path = %safetensors_path.display(),
                "Loading from .safetensors file"
            );
            let (data, meta) = self.load_and_parse_safetensors(&safetensors_path)?;
            (safetensors_path, data, meta)
        } else {
            return Err(AosError::Lifecycle(format!(
                "Adapter file not found: {} (checked .aos and .safetensors)",
                adapter_name
            )));
        };

        let expected_hash = self.expected_hash(adapter_name)?;
        let actual_hash = B3Hash::hash(&weights_data.data);

        if actual_hash != expected_hash {
            tracing::error!(
                "Adapter hash mismatch for {} (expected {}, got {})",
                adapter_name,
                expected_hash,
                actual_hash
            );
            return Err(AosError::AdapterHashMismatch {
                adapter_id: adapter_name.to_string(),
                expected: expected_hash,
                actual: actual_hash,
            });
        }

        let memory_bytes = Self::calculate_memory_bytes(&metadata, weights_data.data.len());
        self.loaded
            .insert(adapter_id, (adapter_path.clone(), weights_data));

        tracing::info!(
            adapter_id = adapter_id,
            adapter_name = adapter_name,
            path = %adapter_path.display(),
            memory_bytes = memory_bytes,
            num_parameters = metadata.num_parameters,
            rank = ?metadata.rank,
            "Loaded adapter"
        );

        Ok(AdapterHandle {
            adapter_id,
            path: adapter_path,
            memory_bytes,
            metadata,
        })
    }

    /// Load an adapter asynchronously using spawn_blocking
    pub async fn load_adapter_async(
        &mut self,
        adapter_id: u16,
        adapter_name: &str,
    ) -> Result<AdapterHandle> {
        let base_path = self.base_path.clone();
        let expected_hash = self.expected_hash(adapter_name)?;
        let adapter_name_owned = adapter_name.to_string();

        let (handle, weights_data) = tokio::task::spawn_blocking(move || {
            // Check for .aos file first, fall back to .safetensors
            let aos_path = base_path.join(format!("{}.aos", &adapter_name_owned));
            let safetensors_path = base_path.join(format!("{}.safetensors", &adapter_name_owned));

            let (adapter_path, weights_data, metadata) = if aos_path.exists() {
                tracing::debug!(
                    adapter_name = adapter_name_owned,
                    path = %aos_path.display(),
                    "Loading from .aos file (async)"
                );
                // Load from .aos file
                let (data, meta) = AdapterLoader::load_from_aos_static(&aos_path)?;
                (aos_path, data, meta)
            } else if safetensors_path.exists() {
                tracing::debug!(
                    adapter_name = adapter_name_owned,
                    path = %safetensors_path.display(),
                    "Loading from .safetensors file (async)"
                );
                // Load from .safetensors file
                let file = File::open(&safetensors_path).map_err(|e| {
                    AosError::Lifecycle(format!("Failed to open adapter file: {}", e))
                })?;

                let mmap = unsafe { Mmap::map(&file) }.map_err(|e| {
                    AosError::Lifecycle(format!("Failed to mmap adapter file: {}", e))
                })?;

                let mmap = Arc::new(mmap);

                // Parse SafeTensors to extract metadata
                let tensors = SafeTensors::deserialize(&mmap).map_err(|e| {
                    AosError::Lifecycle(format!("Failed to parse SafeTensors: {}", e))
                })?;

                let metadata = AdapterLoader::extract_metadata(&tensors);

                // Read data for hashing (mmap gives us zero-copy access)
                let weights_data_vec = mmap.to_vec();

                let loaded_weights = LoadedWeights {
                    data: weights_data_vec,
                    _mmap: Some(mmap),
                };

                (safetensors_path, loaded_weights, metadata)
            } else {
                return Err(AosError::Lifecycle(format!(
                    "Adapter file not found: {} (checked .aos and .safetensors)",
                    adapter_name_owned
                )));
            };

            let actual_hash = B3Hash::hash(&weights_data.data);

            if actual_hash != expected_hash {
                tracing::error!(
                    "Adapter hash mismatch for {} (expected {}, got {})",
                    adapter_name_owned,
                    expected_hash,
                    actual_hash
                );
                return Err(AosError::AdapterHashMismatch {
                    adapter_id: adapter_name_owned.clone(),
                    expected: expected_hash,
                    actual: actual_hash,
                });
            }

            let memory_bytes =
                AdapterLoader::calculate_memory_bytes(&metadata, weights_data.data.len());

            tracing::info!(
                adapter_id = adapter_id,
                adapter_name = adapter_name_owned,
                path = %adapter_path.display(),
                memory_bytes = memory_bytes,
                num_parameters = metadata.num_parameters,
                rank = ?metadata.rank,
                "Loaded adapter async"
            );

            Ok((
                AdapterHandle {
                    adapter_id,
                    path: adapter_path,
                    memory_bytes,
                    metadata,
                },
                weights_data,
            ))
        })
        .await
        .map_err(|e| AosError::Lifecycle(format!("Failed to spawn load task: {}", e)))??;

        // Update internal state
        self.loaded
            .insert(adapter_id, (handle.path.clone(), weights_data));

        Ok(handle)
    }

    /// Unload an adapter from memory
    ///
    /// This removes the adapter from the loaded map and zeroizes the weights
    /// via the LoadedWeights drop implementation.
    pub fn unload_adapter(&mut self, adapter_id: u16) -> Result<()> {
        if let Some((path, _weights)) = self.loaded.remove(&adapter_id) {
            // Weights are automatically zeroized when dropped
            tracing::info!(
                adapter_id = adapter_id,
                path = %path.display(),
                "Unloaded adapter (weights zeroized)"
            );
            Ok(())
        } else {
            Err(AosError::Lifecycle(format!(
                "Adapter {} not loaded",
                adapter_id
            )))
        }
    }

    /// Check if adapter is loaded
    pub fn is_loaded(&self, adapter_id: u16) -> bool {
        self.loaded.contains_key(&adapter_id)
    }

    /// Get number of loaded adapters
    pub fn loaded_count(&self) -> usize {
        self.loaded.len()
    }

    /// Load and parse SafeTensors file, returning weights and metadata
    fn load_and_parse_safetensors(
        &self,
        adapter_path: &PathBuf,
    ) -> Result<(LoadedWeights, AdapterMetadata)> {
        // Open and memory-map the file for efficient reading
        let file = File::open(adapter_path)
            .map_err(|e| AosError::Lifecycle(format!("Failed to open adapter file: {}", e)))?;

        let mmap = unsafe { Mmap::map(&file) }
            .map_err(|e| AosError::Lifecycle(format!("Failed to mmap adapter file: {}", e)))?;

        let mmap = Arc::new(mmap);

        // Parse SafeTensors to extract metadata
        let tensors = SafeTensors::deserialize(&mmap)
            .map_err(|e| AosError::Lifecycle(format!("Failed to parse SafeTensors: {}", e)))?;

        let metadata = Self::extract_metadata(&tensors);

        // Keep data in memory for hashing and potential GPU upload
        let weights_data = mmap.to_vec();

        Ok((
            LoadedWeights {
                data: weights_data,
                _mmap: Some(mmap),
            },
            metadata,
        ))
    }

    /// Load and parse .aos file, extracting SafeTensors weights section
    ///
    /// # Security
    /// This method verifies Ed25519 signatures on .aos files when `require_signatures` is true.
    /// In debug builds, unsigned adapters log a warning; in release builds, they fail.
    fn load_from_aos(&self, aos_path: &PathBuf) -> Result<(LoadedWeights, AdapterMetadata)> {
        // First verify the signature if .aos format supports it
        self.verify_aos_signature(aos_path)?;

        // Then load the weights
        Self::load_from_aos_static(aos_path)
    }

    /// Verify the Ed25519 signature on an .aos file
    ///
    /// .aos files are ZIP archives that may contain a signature.sig file.
    /// This function verifies the signature against the manifest.json hash.
    fn verify_aos_signature(&self, aos_path: &PathBuf) -> Result<()> {
        let file = File::open(aos_path).map_err(|e| {
            AosError::Io(format!(
                "Failed to open .aos file for signature check: {}",
                e
            ))
        })?;

        let mut archive = match ZipArchive::new(file) {
            Ok(arc) => arc,
            Err(e) => {
                if self.require_signatures {
                    return Err(AosError::Io(format!("Failed to open .aos as ZIP: {}", e)));
                } else {
                    tracing::warn!(
                        path = %aos_path.display(),
                        error = %e,
                        "Skipping signature verification for non-ZIP .aos (dev mode)"
                    );
                    return Ok(());
                }
            }
        };

        // Try to read the signature file
        let signature = match archive.by_name("signature.sig") {
            Ok(mut file) => {
                let mut data = Vec::new();
                file.read_to_end(&mut data)
                    .map_err(|e| AosError::Io(format!("Failed to read signature.sig: {}", e)))?;
                let sig: AosSignature = serde_json::from_slice(&data)
                    .map_err(|e| AosError::Parse(format!("Invalid signature format: {}", e)))?;
                Some(sig)
            }
            Err(zip::result::ZipError::FileNotFound) => None,
            Err(e) => {
                return Err(AosError::Io(format!(
                    "Failed to access signature.sig: {}",
                    e
                )));
            }
        };

        match signature {
            Some(sig) => {
                // Read manifest.json to compute its hash
                let manifest_hash = {
                    // Re-open archive since we consumed it
                    let file = File::open(aos_path)?;
                    let mut archive = ZipArchive::new(file)
                        .map_err(|e| AosError::Io(format!("Failed to reopen .aos: {}", e)))?;
                    let mut manifest_file = archive.by_name("manifest.json").map_err(|e| {
                        AosError::Io(format!("Failed to read manifest.json: {}", e))
                    })?;
                    let mut manifest_data = Vec::new();
                    manifest_file
                        .read_to_end(&mut manifest_data)
                        .map_err(|e| AosError::Io(format!("Failed to read manifest: {}", e)))?;
                    B3Hash::hash(&manifest_data)
                };

                // Verify the signature
                sig.public_key
                    .verify(&manifest_hash.to_bytes(), &sig.signature)
                    .map_err(|e| {
                        AosError::Validation(format!("Signature verification failed: {}", e))
                    })?;

                tracing::debug!(
                    path = %aos_path.display(),
                    key_id = %sig.key_id,
                    "Adapter signature verified successfully"
                );
                Ok(())
            }
            None => {
                if self.require_signatures {
                    Err(AosError::Validation(format!(
                        "Adapter {} has no signature (required in production mode)",
                        aos_path.display()
                    )))
                } else {
                    tracing::warn!(
                        path = %aos_path.display(),
                        "Loading unsigned adapter (development mode only)"
                    );
                    Ok(())
                }
            }
        }
    }

    /// Static helper for loading .aos files (used in both sync and async contexts)
    fn load_from_aos_static(aos_path: &PathBuf) -> Result<(LoadedWeights, AdapterMetadata)> {
        // Open and memory-map the .aos file
        let file = File::open(aos_path)
            .map_err(|e| AosError::Lifecycle(format!("Failed to open .aos file: {}", e)))?;

        let mmap = unsafe { Mmap::map(&file) }
            .map_err(|e| AosError::Lifecycle(format!("Failed to mmap .aos file: {}", e)))?;

        // Validate minimum file size for header
        if mmap.len() < HEADER_SIZE {
            return Err(AosError::Validation(format!(
                "AOS file too small: {} bytes (minimum {} bytes for header)",
                mmap.len(),
                HEADER_SIZE
            )));
        }

        // Validate magic bytes (4 bytes at offset 0)
        if &mmap[0..4] != &AOS_MAGIC {
            return Err(AosError::Validation(format!(
                "Invalid AOS magic bytes: expected {:?}, got {:?}",
                AOS_MAGIC,
                &mmap[0..4]
            )));
        }

        // Read header fields to locate weights and manifest sections
        let weights_offset = u64::from_le_bytes(mmap[8..16].try_into().unwrap()) as usize;
        let weights_size = u64::from_le_bytes(mmap[16..24].try_into().unwrap()) as usize;
        let manifest_offset = u64::from_le_bytes(mmap[24..32].try_into().unwrap()) as usize;
        let manifest_size = u64::from_le_bytes(mmap[32..40].try_into().unwrap()) as usize;

        // Validate weights section bounds
        if manifest_offset + manifest_size > mmap.len() {
            return Err(AosError::Validation(format!(
                "Manifest extends beyond file: offset {} + size {} > file size {}",
                manifest_offset,
                manifest_size,
                mmap.len()
            )));
        }
        if weights_offset + weights_size > mmap.len() {
            return Err(AosError::Validation(format!(
                "Weights extend beyond file: offset {} + size {} > file size {}",
                weights_offset,
                weights_size,
                mmap.len()
            )));
        }

        // Extract manifest for integrity checks
        let manifest_bytes = &mmap[manifest_offset..manifest_offset + manifest_size];
        let manifest: ManifestForVerify = serde_json::from_slice(manifest_bytes)
            .map_err(|e| AosError::Parse(format!("Failed to parse adapter manifest: {}", e)))?;

        // Extract the SafeTensors weights section
        let weights_data = &mmap[weights_offset..weights_offset + weights_size];

        // Parse SafeTensors to extract metadata
        let tensors = SafeTensors::deserialize(weights_data).map_err(|e| {
            AosError::Lifecycle(format!("Failed to parse SafeTensors from .aos: {}", e))
        })?;

        let adapter_name = manifest
            .adapter_id
            .clone()
            .unwrap_or_else(|| "unknown-adapter".to_string());

        // Verify per-layer hashes first for precise diagnostics (optional for backward compat)
        if let Some(per_layer) = &manifest.per_layer_hashes {
            Self::verify_per_layer_hashes(&tensors, per_layer, &adapter_name)?;
        }

        // Verify whole-adapter hash if present
        if let Some(expected_hex) = &manifest.weights_hash {
            let expected = B3Hash::from_hex(expected_hex).map_err(|e| {
                AosError::InvalidHash(format!("Invalid manifest weights_hash: {}", e))
            })?;
            let actual = B3Hash::hash(weights_data);
            if actual != expected {
                return Err(AosError::AdapterHashMismatch {
                    adapter_id: adapter_name.clone(),
                    expected,
                    actual,
                });
            }
        }

        let metadata = Self::extract_metadata(&tensors);

        // Copy weights data for hashing and potential GPU upload
        let weights_vec = weights_data.to_vec();

        tracing::debug!(
            path = %aos_path.display(),
            weights_offset = weights_offset,
            weights_size = weights_size,
            num_tensors = tensors.len(),
            "Extracted SafeTensors from .aos file"
        );

        Ok((
            LoadedWeights {
                data: weights_vec,
                _mmap: None, // We don't keep the mmap since we copied the data
            },
            metadata,
        ))
    }

    fn verify_per_layer_hashes(
        tensors: &SafeTensors,
        expected: &HashMap<String, LayerHashEntry>,
        adapter_id: &str,
    ) -> Result<()> {
        // Build lookup keyed by canonical logical layer id (primary identifier).
        let mut actual: HashMap<String, B3Hash> = HashMap::new();
        for (name, tensor) in tensors.tensors() {
            let hash = B3Hash::hash(tensor.data());
            let canonical = canonical_layer_id(&name);
            actual.entry(canonical).or_insert(hash);
        }

        for (layer_id, entry) in expected {
            // Enforce canonical logical path as primary key; tolerate legacy raw names
            // but surface a warning and canonicalize before lookup.
            let canonical_expected = canonical_layer_id(layer_id);
            if canonical_expected != *layer_id {
                tracing::warn!(
                    adapter_id = %adapter_id,
                    expected_layer = %layer_id,
                    canonical_layer = %canonical_expected,
                    "Manifest provided non-canonical layer id; using canonical path for verification"
                );
            }

            let expected_hash = B3Hash::from_hex(entry.hash()).map_err(|e| {
                AosError::InvalidHash(format!("Invalid per-layer hash for '{}': {}", layer_id, e))
            })?;
            let actual_hash = actual.get(&canonical_expected).ok_or_else(|| {
                AosError::Validation(format!(
                    "Per-layer hash provided for missing tensor '{}' in adapter {}",
                    canonical_expected, adapter_id
                ))
            })?;
            if *actual_hash != expected_hash {
                return Err(AosError::AdapterLayerHashMismatch {
                    adapter_id: adapter_id.to_string(),
                    layer_id: canonical_expected.clone(),
                    expected: expected_hash,
                    actual: *actual_hash,
                });
            }
        }
        Ok(())
    }

    /// Extract metadata from parsed SafeTensors
    fn extract_metadata(tensors: &SafeTensors) -> AdapterMetadata {
        let mut num_parameters = 0usize;
        let mut target_modules = Vec::new();
        let mut detected_rank: Option<usize> = None;

        for (name, tensor_view) in tensors.tensors() {
            let shape = tensor_view.shape();
            let tensor_params: usize = shape.iter().product();
            num_parameters += tensor_params;

            // Detect target modules from tensor names (e.g., "lora_A.q_proj")
            if name.contains("lora_A") || name.contains("lora_B") {
                // Extract module name
                let module_name = name
                    .replace("lora_A.", "")
                    .replace("lora_B.", "")
                    .replace(".weight", "");
                if !target_modules.contains(&module_name) {
                    target_modules.push(module_name);
                }

                // Detect LoRA rank from lora_A shape [rank, hidden_dim]
                // or lora_B shape [hidden_dim, rank]
                if name.contains("lora_A") && shape.len() >= 2 {
                    detected_rank = Some(shape[0]);
                } else if name.contains("lora_B") && shape.len() >= 2 {
                    detected_rank = Some(shape[1]);
                }
            }
        }

        AdapterMetadata {
            num_parameters,
            rank: detected_rank,
            target_modules,
        }
    }

    /// Calculate memory usage based on metadata and raw data size
    fn calculate_memory_bytes(metadata: &AdapterMetadata, raw_size: usize) -> usize {
        // Base memory is the raw file size
        let base_memory = raw_size;

        // Add overhead for:
        // - Parsed tensor structures (~10%)
        // - GPU buffer alignment padding
        // - Metadata and indices
        let overhead_factor = 1.15;

        let estimated = (base_memory as f64 * overhead_factor) as usize;

        tracing::debug!(
            raw_size = raw_size,
            estimated = estimated,
            num_parameters = metadata.num_parameters,
            "Calculated adapter memory usage"
        );

        estimated
    }

    /// Get raw weight data for an adapter (for GPU upload)
    pub fn get_weights(&self, adapter_id: u16) -> Option<&[u8]> {
        self.loaded
            .get(&adapter_id)
            .map(|(_, weights)| weights.data.as_slice())
    }
}

/// Handle to a loaded adapter
#[derive(Debug, Clone)]
pub struct AdapterHandle {
    pub adapter_id: u16,
    pub path: PathBuf,
    pub memory_bytes: usize,
    pub metadata: AdapterMetadata,
}

#[derive(Debug, Deserialize)]
struct ManifestForVerify {
    #[serde(default)]
    adapter_id: Option<String>,
    #[serde(default)]
    weights_hash: Option<String>,
    #[serde(default)]
    per_layer_hashes: Option<HashMap<String, LayerHashEntry>>,
}

/// Per-layer hash entry keyed by canonical logical layer id. Accepts either the
/// new `{ \"hash\": \"...\", \"tensor_name\": \"...\" }` form or legacy string
/// hashes via `serde(untagged)`.
#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
enum LayerHashEntry {
    Hash(String),
    Detailed {
        hash: String,
        #[serde(default)]
        tensor_name: Option<String>,
    },
}

impl LayerHashEntry {
    fn hash(&self) -> &str {
        match self {
            LayerHashEntry::Hash(h) => h,
            LayerHashEntry::Detailed { hash, .. } => hash,
        }
    }
}

impl AdapterHandle {
    /// Get memory footprint in bytes
    pub fn memory_bytes(&self) -> usize {
        self.memory_bytes
    }

    /// Get LoRA rank if detected
    pub fn rank(&self) -> Option<usize> {
        self.metadata.rank
    }

    /// Get number of parameters
    pub fn num_parameters(&self) -> usize {
        self.metadata.num_parameters
    }

    /// Get target modules
    pub fn target_modules(&self) -> &[String] {
        &self.metadata.target_modules
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use safetensors::tensor::serialize;
    use serde::Serialize;
    use std::fs;

    /// Create a valid SafeTensors file with test data
    fn create_test_safetensors(path: &std::path::Path) -> Vec<u8> {
        use std::collections::HashMap as StdHashMap;

        // Create simple LoRA-style tensors
        let lora_a_data: Vec<f32> = vec![0.1, 0.2, 0.3, 0.4]; // rank=2, dim=2
        let lora_b_data: Vec<f32> = vec![0.5, 0.6, 0.7, 0.8]; // dim=2, rank=2

        let lora_a_bytes: Vec<u8> = lora_a_data.iter().flat_map(|f| f.to_le_bytes()).collect();
        let lora_b_bytes: Vec<u8> = lora_b_data.iter().flat_map(|f| f.to_le_bytes()).collect();

        let mut tensors = StdHashMap::new();
        tensors.insert(
            "lora_A.q_proj.weight".to_string(),
            safetensors::tensor::TensorView::new(
                safetensors::Dtype::F32,
                vec![2, 2],
                &lora_a_bytes,
            )
            .expect("Test TensorView creation should succeed"),
        );
        tensors.insert(
            "lora_B.q_proj.weight".to_string(),
            safetensors::tensor::TensorView::new(
                safetensors::Dtype::F32,
                vec![2, 2],
                &lora_b_bytes,
            )
            .expect("Test TensorView creation should succeed"),
        );

        let serialized =
            serialize(tensors, &None).expect("Test SafeTensors serialization should succeed");
        fs::write(path, &serialized).expect("Test file write should succeed");
        serialized
    }

    #[test]
    fn test_loader_basic() {
        let temp_dir = std::env::temp_dir().join("mplora_test_loader");
        let _ = fs::remove_dir_all(&temp_dir); // Clean up any previous run
        fs::create_dir_all(&temp_dir).expect("Test temp directory creation should succeed");

        // Create a valid SafeTensors adapter file
        let adapter_path = temp_dir.join("test_adapter.safetensors");
        let serialized = create_test_safetensors(&adapter_path);

        let mut expected_hashes = HashMap::new();
        expected_hashes.insert("test_adapter".to_string(), B3Hash::hash(&serialized));
        let mut loader = AdapterLoader::new(temp_dir.clone(), expected_hashes);

        // Load adapter
        let handle = loader
            .load_adapter(0, "test_adapter")
            .expect("Test adapter load should succeed");
        assert_eq!(handle.adapter_id, 0);
        assert!(loader.is_loaded(0));
        assert_eq!(loader.loaded_count(), 1);

        // Verify metadata was extracted
        assert_eq!(handle.metadata.num_parameters, 8); // 4 + 4 parameters
        assert_eq!(handle.metadata.rank, Some(2));
        assert!(handle
            .metadata
            .target_modules
            .contains(&"q_proj".to_string()));

        // Verify we can get weights
        assert!(loader.get_weights(0).is_some());

        // Unload adapter
        loader
            .unload_adapter(0)
            .expect("Test adapter unload should succeed");
        assert!(!loader.is_loaded(0));
        assert_eq!(loader.loaded_count(), 0);

        // Cleanup
        fs::remove_dir_all(temp_dir).expect("Test cleanup should succeed");
    }

    #[test]
    fn test_loader_hash_mismatch() {
        let temp_dir = std::env::temp_dir().join("mplora_test_loader_mismatch");
        let _ = fs::remove_dir_all(&temp_dir); // Clean up any previous run
        fs::create_dir_all(&temp_dir).expect("Test temp directory creation should succeed");

        let adapter_path = temp_dir.join("test_adapter.safetensors");
        let serialized = create_test_safetensors(&adapter_path);

        let mut expected_hashes = HashMap::new();
        expected_hashes.insert("test_adapter".to_string(), B3Hash::hash(b"different data"));

        let mut loader = AdapterLoader::new(temp_dir.clone(), expected_hashes);

        match loader.load_adapter(0, "test_adapter") {
            Err(AosError::AdapterHashMismatch {
                expected,
                actual,
                adapter_id,
            }) => {
                assert_eq!(adapter_id, "test_adapter");
                assert_eq!(expected, B3Hash::hash(b"different data"));
                assert_eq!(actual, B3Hash::hash(&serialized));
            }
            Err(e) => panic!("Unexpected error: {}", e),
            Ok(_) => panic!("Expected hash mismatch error"),
        }

        assert!(!loader.is_loaded(0));
        assert_eq!(loader.loaded_count(), 0);

        fs::remove_dir_all(temp_dir).expect("Test cleanup should succeed");
    }

    #[test]
    fn test_per_layer_hash_mismatch_fails() {
        // Build a simple safetensors buffer with one tensor
        let lora_a_data: Vec<f32> = vec![0.1, 0.2, 0.3, 0.4];
        let lora_a_bytes: Vec<u8> = lora_a_data.iter().flat_map(|f| f.to_le_bytes()).collect();
        let tensor = safetensors::tensor::TensorView::new(
            safetensors::Dtype::F32,
            vec![2, 2],
            &lora_a_bytes,
        )
        .unwrap();
        let serialized = safetensors::tensor::serialize(
            [("lora_A.q_proj.weight".to_string(), tensor)].into_iter(),
            &None,
        )
        .unwrap();
        let tensors = SafeTensors::deserialize(&serialized).unwrap();

        // Expected map with wrong hash
        let mut expected = HashMap::new();
        let name = "lora_A.q_proj.weight";
        expected.insert(
            canonical_layer_id(name),
            LayerHashEntry::Hash(B3Hash::hash(b"wrong").to_hex()),
        );

        let result = AdapterLoader::verify_per_layer_hashes(&tensors, &expected, "test-adapter");
        assert!(matches!(
            result,
            Err(AosError::AdapterLayerHashMismatch { .. })
        ));
    }

    #[test]
    fn test_aos_per_layer_corruption_reports_layer() {
        use std::collections::HashMap as StdHashMap;

        let temp_dir = std::env::temp_dir().join("mplora_test_per_layer_aos");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).expect("Temp dir create should succeed");

        // Build safetensors with two tensors in the same logical layer
        let lora_a_data: Vec<f32> = vec![0.1, 0.2, 0.3, 0.4];
        let lora_b_data: Vec<f32> = vec![0.5, 0.6, 0.7, 0.8];
        let lora_a_bytes: Vec<u8> = lora_a_data.iter().flat_map(|f| f.to_le_bytes()).collect();
        let lora_b_bytes: Vec<u8> = lora_b_data.iter().flat_map(|f| f.to_le_bytes()).collect();

        let mut tensors = StdHashMap::new();
        tensors.insert(
            "model.layers.0.attn.q_proj.lora_A.weight".to_string(),
            safetensors::tensor::TensorView::new(
                safetensors::Dtype::F32,
                vec![2, 2],
                &lora_a_bytes,
            )
            .unwrap(),
        );
        tensors.insert(
            "model.layers.0.attn.q_proj.lora_B.weight".to_string(),
            safetensors::tensor::TensorView::new(
                safetensors::Dtype::F32,
                vec![2, 2],
                &lora_b_bytes,
            )
            .unwrap(),
        );

        let serialized =
            safetensors::tensor::serialize(tensors, &None).expect("serialize should work");

        // Build per-layer hashes from original data
        let parsed = SafeTensors::deserialize(&serialized).unwrap();
        let mut per_layer_hashes = HashMap::new();
        for (name, tensor) in parsed.tensors() {
            per_layer_hashes.insert(
                canonical_layer_id(&name),
                LayerHashEntry::Detailed {
                    hash: B3Hash::hash(tensor.data()).to_hex(),
                    tensor_name: Some(name.to_string()),
                },
            );
        }

        #[derive(Serialize)]
        struct TestManifest {
            adapter_id: String,
            name: Option<String>,
            version: String,
            rank: u32,
            alpha: f32,
            base_model: String,
            target_modules: Vec<String>,
            category: Option<String>,
            tier: Option<String>,
            created_at: Option<String>,
            weights_hash: Option<String>,
            per_layer_hashes: Option<HashMap<String, LayerHashEntry>>,
            training_config: Option<serde_json::Value>,
        }

        let manifest = TestManifest {
            adapter_id: "test/perlayer".to_string(),
            name: Some("PerLayerTest".to_string()),
            version: "1.0.0".to_string(),
            rank: 2,
            alpha: 4.0,
            base_model: "test-model".to_string(),
            target_modules: vec!["q_proj".to_string()],
            category: Some("code".to_string()),
            tier: Some("persistent".to_string()),
            created_at: Some("2025-01-01T00:00:00Z".to_string()),
            weights_hash: Some(B3Hash::hash(&serialized).to_hex()),
            per_layer_hashes: Some(per_layer_hashes),
            training_config: None,
        };

        let aos_path = temp_dir.join("test_adapter.aos");
        AosWriter::new()
            .write_archive(&aos_path, &manifest, &serialized)
            .expect("write archive");

        // Corrupt a single byte in the first tensor's data (after safetensors header)
        let mut aos_bytes = std::fs::read(&aos_path).expect("read aos");
        let header_len = HEADER_SIZE;
        let header_size = u64::from_le_bytes(serialized[0..8].try_into().unwrap()) as usize;
        let data_offset = 8 + header_size;
        let corrupt_index = header_len + data_offset;
        aos_bytes[corrupt_index] ^= 0xFF;
        std::fs::write(&aos_path, &aos_bytes).expect("write corrupted aos");

        let mut expected_hashes = HashMap::new();
        expected_hashes.insert("test_adapter".to_string(), B3Hash::hash(&serialized));
        let mut loader = AdapterLoader::new(temp_dir.clone(), expected_hashes);

        match loader.load_adapter(0, "test_adapter") {
            Err(AosError::AdapterLayerHashMismatch { layer_id, .. }) => {
                assert!(
                    layer_id.contains("layer_0.attn.q_proj"),
                    "unexpected layer id: {}",
                    layer_id
                );
            }
            Err(e) => panic!("Unexpected error: {}", e),
            Ok(_) => panic!("Expected per-layer hash mismatch"),
        }

        fs::remove_dir_all(temp_dir).expect("cleanup temp dir");
    }

    #[test]
    fn test_loader_file_not_found() {
        let temp_dir = std::env::temp_dir().join("mplora_test_loader_not_found");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).expect("Test temp directory creation should succeed");

        let mut expected_hashes = HashMap::new();
        expected_hashes.insert("missing_adapter".to_string(), B3Hash::hash(b"data"));

        let mut loader = AdapterLoader::new(temp_dir.clone(), expected_hashes);

        match loader.load_adapter(0, "missing_adapter") {
            Err(AosError::Lifecycle(msg)) => {
                assert!(msg.contains("not found"));
            }
            Err(e) => panic!("Unexpected error type: {}", e),
            Ok(_) => panic!("Expected file not found error"),
        }

        fs::remove_dir_all(temp_dir).expect("Test cleanup should succeed");
    }

    #[test]
    fn test_unload_not_loaded() {
        let temp_dir = std::env::temp_dir().join("mplora_test_unload_not_loaded");
        fs::create_dir_all(&temp_dir).expect("Test temp directory creation should succeed");

        let mut loader = AdapterLoader::new(temp_dir.clone(), HashMap::new());

        match loader.unload_adapter(99) {
            Err(AosError::Lifecycle(msg)) => {
                assert!(msg.contains("not loaded"));
            }
            Err(e) => panic!("Unexpected error type: {}", e),
            Ok(_) => panic!("Expected not loaded error"),
        }

        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_production_mode_enforces_signature_requirement_flag() {
        let temp_dir = std::env::temp_dir().join("mplora_prod_flag");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).expect("Temp dir create should succeed");

        let prev = std::env::var("AOS_SERVER_PRODUCTION_MODE").ok();
        std::env::set_var("AOS_SERVER_PRODUCTION_MODE", "true");

        let loader = AdapterLoader::new(temp_dir.clone(), HashMap::new());
        assert!(loader.signatures_required());

        if let Some(v) = prev {
            std::env::set_var("AOS_SERVER_PRODUCTION_MODE", v);
        } else {
            std::env::remove_var("AOS_SERVER_PRODUCTION_MODE");
        }
        let _ = fs::remove_dir_all(temp_dir);
    }
}
