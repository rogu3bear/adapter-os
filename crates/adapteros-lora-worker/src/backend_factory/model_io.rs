use adapteros_core::{AosError, B3Hash, Result};
use regex::Regex;
use safetensors::{tensor::TensorView, SafeTensors};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{error, info, warn};

/// Structure representing the safetensors index file for sharded models
#[derive(Deserialize)]
struct SafeTensorsIndex {
    weight_map: HashMap<String, String>,
    /// Metadata from index file (unused but part of the format)
    #[serde(default)]
    #[allow(dead_code)]
    metadata: Option<serde_json::Value>,
}

/// Compute BLAKE3 hash of all model files in a directory
#[allow(dead_code)] // Used by verify_model_integrity for backwards compatibility
pub(crate) fn compute_model_directory_hash(model_path: &Path) -> Result<B3Hash> {
    // Check for CoreML mlpackage format first
    let mlpackage_weight_path = model_path.join("Data/com.apple.CoreML/weights/weight.bin");
    if mlpackage_weight_path.exists() {
        let bytes = std::fs::read(&mlpackage_weight_path).map_err(|e| {
            AosError::Io(format!(
                "Failed to read CoreML weight file '{}': {}",
                mlpackage_weight_path.display(),
                e
            ))
        })?;
        return Ok(B3Hash::hash(&bytes));
    }

    let single_model_path = model_path.join("model.safetensors");

    if single_model_path.exists() {
        let bytes = std::fs::read(&single_model_path).map_err(|e| {
            AosError::Io(format!(
                "Failed to read model file '{}': {}",
                single_model_path.display(),
                e
            ))
        })?;
        return Ok(B3Hash::hash(&bytes));
    }

    // Sharded model: collect all shards, sort, hash in order
    let mut shard_paths: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(model_path) {
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            if file_name.starts_with("model-") && file_name.ends_with(".safetensors") {
                shard_paths.push(entry.path());
            }
        }
    }

    if shard_paths.is_empty() {
        return Err(AosError::Config(format!(
            "No model files found in '{}'",
            model_path.display()
        )));
    }

    shard_paths.sort();
    let mut hasher = blake3::Hasher::new();
    for shard_path in &shard_paths {
        let bytes = std::fs::read(shard_path).map_err(|e| {
            AosError::Io(format!(
                "Failed to read shard '{}': {}",
                shard_path.display(),
                e
            ))
        })?;
        hasher.update(&bytes);
    }
    Ok(B3Hash::from_bytes(*hasher.finalize().as_bytes()))
}

/// Estimate model size in bytes without loading the full weights into memory.
pub(crate) fn estimate_model_size_bytes(model_path: &Path) -> Result<u64> {
    if model_path.is_file() {
        let meta = std::fs::metadata(model_path).map_err(|e| {
            AosError::Io(format!(
                "Failed to read model file metadata '{}': {}",
                model_path.display(),
                e
            ))
        })?;
        return Ok(meta.len());
    }

    let single_model_path = model_path.join("model.safetensors");
    if single_model_path.exists() {
        let meta = std::fs::metadata(&single_model_path).map_err(|e| {
            AosError::Io(format!(
                "Failed to read model file metadata '{}': {}",
                single_model_path.display(),
                e
            ))
        })?;
        return Ok(meta.len());
    }

    if let Some(shard_files) = parse_safetensors_index(model_path)? {
        let mut total = 0u64;
        for shard in shard_files {
            let path = model_path.join(&shard);
            if !path.exists() {
                return Err(AosError::Config(format!(
                    "Shard file '{}' referenced in index but not found",
                    path.display()
                )));
            }
            let meta = std::fs::metadata(&path).map_err(|e| {
                AosError::Io(format!(
                    "Failed to read shard metadata '{}': {}",
                    path.display(),
                    e
                ))
            })?;
            total = total.saturating_add(meta.len());
        }
        return Ok(total);
    }

    let mut shard_paths: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(model_path) {
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            if file_name.starts_with("model-") && file_name.ends_with(".safetensors") {
                shard_paths.push(entry.path());
            }
        }
    }

    if shard_paths.is_empty() {
        return Err(AosError::Config(format!(
            "No model files found in '{}'",
            model_path.display()
        )));
    }

    shard_paths.sort();
    let mut total = 0u64;
    for shard_path in shard_paths {
        let meta = std::fs::metadata(&shard_path).map_err(|e| {
            AosError::Io(format!(
                "Failed to read shard metadata '{}': {}",
                shard_path.display(),
                e
            ))
        })?;
        total = total.saturating_add(meta.len());
    }
    Ok(total)
}

#[allow(dead_code)]
/// Estimate CoreML model size in bytes from a compiled package.
pub(crate) fn estimate_coreml_model_size_bytes(model_path: &Path) -> Result<u64> {
    if model_path.is_file() {
        let meta = std::fs::metadata(model_path).map_err(|e| {
            AosError::Io(format!(
                "Failed to read CoreML model file metadata '{}': {}",
                model_path.display(),
                e
            ))
        })?;
        return Ok(meta.len());
    }

    let weight_candidates = [
        model_path.join("Data/com.apple.CoreML/weights/weight.bin"),
        model_path.join("Data/model/weights.bin"),
        model_path.join("Data/model/weight.bin"),
    ];

    for candidate in weight_candidates {
        if let Ok(meta) = std::fs::metadata(&candidate) {
            return Ok(meta.len());
        }
    }

    let mut total: u64 = 0;
    let mut stack = vec![model_path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|e| {
            AosError::Io(format!(
                "Failed to read CoreML directory '{}': {}",
                dir.display(),
                e
            ))
        })?;
        for entry in entries {
            let entry = entry.map_err(|e| {
                AosError::Io(format!(
                    "Failed to read CoreML directory entry in '{}': {}",
                    dir.display(),
                    e
                ))
            })?;
            let path = entry.path();
            let meta = entry.metadata().map_err(|e| {
                AosError::Io(format!(
                    "Failed to stat CoreML path '{}': {}",
                    path.display(),
                    e
                ))
            })?;
            if meta.is_dir() {
                stack.push(path);
            } else {
                total = total.saturating_add(meta.len());
            }
        }
    }

    if total == 0 {
        return Err(AosError::Config(format!(
            "CoreML model package '{}' is empty",
            model_path.display()
        )));
    }

    Ok(total)
}

/// Verify model bytes against expected manifest hash
///
/// # Deprecation Warning
///
/// This function has a TOCTOU (time-of-check-time-of-use) vulnerability because
/// it verifies the model hash but doesn't return the verified bytes. The model
/// could theoretically change between verification and loading.
///
/// **Prefer `load_model_bytes_atomic_verified()` instead**, which computes the hash
/// from the exact bytes returned, eliminating the TOCTOU gap.
///
/// This function remains for backwards compatibility with existing Metal/CoreML
/// backend code that loads models through platform-specific APIs.
#[allow(dead_code)] // Retained for Metal/CoreML backend compatibility
pub(crate) fn verify_model_integrity(
    model_path: &Path,
    manifest_hash: Option<&B3Hash>,
    backend_name: &str,
) -> Result<()> {
    // SECURITY: This bypass is only available in debug builds.
    let skip_verification = {
        #[cfg(debug_assertions)]
        {
            std::env::var("AOS_SKIP_MODEL_HASH_VERIFY").is_ok()
        }
        #[cfg(not(debug_assertions))]
        {
            if std::env::var("AOS_SKIP_MODEL_HASH_VERIFY").is_ok() {
                warn!(backend = %backend_name, "AOS_SKIP_MODEL_HASH_VERIFY is set but IGNORED in release builds for security");
            }
            false
        }
    };
    if skip_verification {
        warn!(backend = %backend_name, "Model hash verification SKIPPED (debug build only)");
        return Ok(());
    }

    let expected = match manifest_hash {
        Some(h) => h,
        None => {
            warn!(backend = %backend_name, "No manifest_hash provided; skipping verification");
            return Ok(());
        }
    };

    let actual = compute_model_directory_hash(model_path)?;
    if actual != *expected {
        error!(
            backend = %backend_name,
            expected = %expected.to_hex(),
            actual = %actual.to_hex(),
            "MODEL INTEGRITY VERIFICATION FAILED"
        );
        return Err(AosError::CacheCorruption {
            path: model_path.display().to_string(),
            expected: expected.to_hex(),
            actual: actual.to_hex(),
        });
    }

    info!(backend = %backend_name, hash = %actual.to_short_hex(), "Model integrity verified");
    Ok(())
}

/// Load model bytes from a model directory with integrity verification
///
/// Supports both single model files (model.safetensors) and sharded models.
/// For sharded models, loads and merges ALL shards into a single byte buffer.
///
/// # Loading Strategy (Priority Order)
///
/// 1. **Single file**: If `model.safetensors` exists, load it directly
/// 2. **Index-based**: If `model.safetensors.index.json` exists, parse it and load all shards
/// 3. **Pattern-based**: Detect shard pattern (model-XXXXX-of-YYYYY.safetensors) and load all
///
/// # Sharded Model Detection
///
/// Detects when sharded models are incomplete (missing shards) and returns an error
/// with details about which shards are missing. Warns when shards are loaded without
/// an index.json file (Priority 3 fallback).
///
/// # Hash Verification
///
/// Computes BLAKE3 hash of loaded bytes and logs for audit. For full verification
/// against expected `weights_hash` from the model registry, use
/// [`load_model_bytes_verified`] with the expected hash from the control plane.
fn load_model_bytes(model_path: &Path) -> Result<Vec<u8>> {
    load_model_bytes_verified(model_path, None)
}

/// Load model bytes with atomic integrity verification
///
/// This function combines loading and hash verification in a single operation,
/// eliminating TOCTOU (time-of-check-time-of-use) vulnerabilities.
///
/// # Returns
///
/// Returns a tuple of `(bytes, computed_hash)` where:
/// - `bytes`: The loaded model bytes
/// - `computed_hash`: BLAKE3 hash of the exact bytes returned
///
/// # Errors
///
/// Returns `AosError::CacheCorruption` if:
/// - The computed hash doesn't match the expected hash (when provided)
/// - This indicates potential corruption or tampering
///
/// # TOCTOU Safety
///
/// Unlike the separate `verify_model_integrity()` + `load_model_bytes()` pattern,
/// this function computes the hash from the EXACT bytes returned, making it
/// impossible for the model to change between verification and use.
///
/// # Example
///
/// ```no_run
/// # use adapteros_core::{B3Hash, Result};
/// # use std::path::Path;
/// # fn example(model_path: &Path, expected: &B3Hash) -> Result<()> {
/// let (bytes, hash) = load_model_bytes_atomic_verified(model_path, Some(expected))?;
/// // bytes are guaranteed to match the hash - no TOCTOU gap
/// # Ok(())
/// # }
/// ```
pub(crate) fn load_model_bytes_atomic_verified(
    model_path: &Path,
    expected_hash: Option<&B3Hash>,
) -> Result<(Vec<u8>, B3Hash)> {
    let bytes = load_model_bytes(model_path)?;
    let computed_hash = B3Hash::hash(&bytes);

    if let Some(expected) = expected_hash {
        if computed_hash != *expected {
            error!(
                model_path = %model_path.display(),
                computed = %computed_hash.to_hex(),
                expected = %expected.to_hex(),
                "MODEL INTEGRITY FAILURE: Hash mismatch"
            );
            return Err(AosError::CacheCorruption {
                path: model_path.display().to_string(),
                expected: expected.to_hex(),
                actual: computed_hash.to_hex(),
            });
        }
        info!(
            model_path = %model_path.display(),
            hash = %computed_hash.to_short_hex(),
            "Model integrity verified"
        );
    }
    Ok((bytes, computed_hash))
}

/// Load model bytes with optional hash verification against expected value
///
/// When `expected_hash` is `Some`, verifies loaded bytes match the expected hash
/// and returns an error if there's a mismatch (indicating corruption or tampering).
///
/// Implements a 3-priority loading strategy:
/// 1. Single model.safetensors (if exists)
/// 2. Sharded model via index.json (if exists) - loads ALL shards
/// 3. Sharded model via pattern detection - loads ALL shards with warning
///
/// # Arguments
///
/// * `model_path` - Path to model directory
/// * `expected_hash` - Optional expected BLAKE3 hash (e.g., from model registry `weights_hash`)
///
/// # Errors
///
/// Returns `AosError::Config` if:
/// - Model file/shards are missing
/// - Hash mismatch when `expected_hash` is provided
/// - Sharded model is incomplete (missing shards)
pub fn load_model_bytes_verified(
    model_path: &Path,
    expected_hash: Option<&B3Hash>,
) -> Result<Vec<u8>> {
    // Try single model file first
    let single_model_path = model_path.join("model.safetensors");
    if single_model_path.exists() {
        info!(path = %single_model_path.display(), "Loading single model file");
        let bytes = std::fs::read(&single_model_path).map_err(|e| {
            AosError::Config(format!(
                "Failed to read model file '{}': {}",
                single_model_path.display(),
                e
            ))
        })?;

        // Compute and log BLAKE3 hash for audit
        let computed_hash = B3Hash::hash(&bytes);
        info!(
            path = %single_model_path.display(),
            hash = %computed_hash,
            size_bytes = bytes.len(),
            "Model file loaded and hashed"
        );

        // Verify against expected hash if provided
        if let Some(expected) = expected_hash {
            if computed_hash != *expected {
                error!(
                    path = %single_model_path.display(),
                    computed = %computed_hash,
                    expected = %expected,
                    "MODEL INTEGRITY FAILURE: Hash mismatch detected!"
                );
                return Err(AosError::Config(format!(
                    "Model integrity verification failed for '{}': computed hash {} != expected {}. \
                     Model file may be corrupted or tampered with.",
                    single_model_path.display(),
                    computed_hash,
                    expected
                )));
            }
            info!(
                path = %single_model_path.display(),
                hash = %computed_hash,
                "Model integrity verified: hash matches expected value"
            );
        }

        return Ok(bytes);
    }

    // Priority 2: Try loading via index.json if present
    if let Some(shard_files) = parse_safetensors_index(model_path)? {
        info!(
            model_path = %model_path.display(),
            num_shards = shard_files.len(),
            "Loading sharded model via index.json"
        );

        let bytes = load_and_merge_shards(model_path, &shard_files)?;

        // Compute and log BLAKE3 hash for audit
        let computed_hash = B3Hash::hash(&bytes);
        info!(
            model_path = %model_path.display(),
            hash = %computed_hash,
            size_bytes = bytes.len(),
            num_shards = shard_files.len(),
            "Sharded model loaded and hashed via index.json"
        );

        // Verify against expected hash if provided
        if let Some(expected) = expected_hash {
            if computed_hash != *expected {
                error!(
                    model_path = %model_path.display(),
                    computed = %computed_hash,
                    expected = %expected,
                    "SHARDED MODEL INTEGRITY FAILURE: Hash mismatch detected!"
                );
                return Err(AosError::Config(format!(
                    "Model integrity verification failed for sharded model at '{}': computed hash {} != expected {}. \
                     Model may be corrupted or tampered with.",
                    model_path.display(),
                    computed_hash,
                    expected
                )));
            }
            info!(
                model_path = %model_path.display(),
                hash = %computed_hash,
                "Sharded model integrity verified: hash matches expected value"
            );
        }

        return Ok(bytes);
    }

    // Priority 3: Detect shard pattern and load all shards (warn about missing index)
    let sharded_model = detect_sharded_model(model_path)?;
    if let Some((_first_shard_path, total_shards, found_shards)) = sharded_model {
        warn!(
            model_path = %model_path.display(),
            total_shards = total_shards,
            "Sharded model detected but no index.json found - loading shards by pattern"
        );

        // Check for missing shards
        if found_shards.len() < total_shards {
            let missing: Vec<usize> = (1..=total_shards)
                .filter(|i| !found_shards.contains(i))
                .collect();
            warn!(
                model_path = %model_path.display(),
                total_shards = total_shards,
                found_shards = found_shards.len(),
                missing_shards = ?missing,
                "Sharded model is incomplete - some shards are missing"
            );
            return Err(AosError::Config(format!(
                "Sharded model at '{}' is incomplete: expected {} shards, found {}. Missing shards: {:?}",
                model_path.display(),
                total_shards,
                found_shards.len(),
                missing
            )));
        }

        // Build shard file list from pattern
        let shard_files: Vec<String> = (1..=total_shards)
            .map(|i| format!("model-{:05}-of-{:05}.safetensors", i, total_shards))
            .collect();

        info!(
            model_path = %model_path.display(),
            total_shards = total_shards,
            "Loading all shards by pattern"
        );

        let bytes = load_and_merge_shards(model_path, &shard_files)?;

        // Compute and log BLAKE3 hash for audit
        let computed_hash = B3Hash::hash(&bytes);
        info!(
            model_path = %model_path.display(),
            hash = %computed_hash,
            size_bytes = bytes.len(),
            num_shards = shard_files.len(),
            "All shards loaded and hashed (pattern-based)"
        );

        // Verify against expected hash if provided
        if let Some(expected) = expected_hash {
            if computed_hash != *expected {
                error!(
                    model_path = %model_path.display(),
                    computed = %computed_hash,
                    expected = %expected,
                    "SHARDED MODEL INTEGRITY FAILURE: Hash mismatch detected!"
                );
                return Err(AosError::Config(format!(
                    "Model integrity verification failed for sharded model at '{}': computed hash {} != expected {}. \
                     Model may be corrupted or tampered with.",
                    model_path.display(),
                    computed_hash,
                    expected
                )));
            }
            info!(
                model_path = %model_path.display(),
                hash = %computed_hash,
                "Sharded model integrity verified: hash matches expected value"
            );
        }

        return Ok(bytes);
    }

    Err(AosError::Config(format!(
        "No model file found in '{}'. Expected 'model.safetensors' or sharded model files (model-00001-of-NNNNN.safetensors).",
        model_path.display()
    )))
}

/// Parse the safetensors index file and extract unique shard filenames
pub(crate) fn parse_safetensors_index(model_path: &Path) -> Result<Option<Vec<String>>> {
    let index_path = model_path.join("model.safetensors.index.json");
    if !index_path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&index_path).map_err(|e| {
        AosError::Config(format!(
            "Failed to read index file '{}': {}",
            index_path.display(),
            e
        ))
    })?;

    let index: SafeTensorsIndex = serde_json::from_str(&content).map_err(|e| {
        AosError::Config(format!(
            "Failed to parse index JSON '{}': {}",
            index_path.display(),
            e
        ))
    })?;

    let mut shards: Vec<String> = index.weight_map.values().cloned().collect();
    shards.sort();
    shards.dedup();

    if shards.is_empty() {
        return Err(AosError::Config(format!(
            "Index file '{}' contains no shard references",
            index_path.display()
        )));
    }

    info!(index_path = %index_path.display(), num_shards = shards.len(), "Parsed safetensors index");
    Ok(Some(shards))
}

/// Load all shards and merge into a single valid SafeTensors buffer
///
/// Each shard file is a complete SafeTensors file with its own header.
/// This function parses each shard, extracts all tensors, and re-serializes
/// them into a single unified SafeTensors buffer that can be deserialized.
fn load_and_merge_shards(model_path: &Path, shard_files: &[String]) -> Result<Vec<u8>> {
    // Collect all tensor data from all shards
    // We need to keep the raw bytes alive while we build TensorViews
    let mut shard_bytes: Vec<Vec<u8>> = Vec::with_capacity(shard_files.len());

    for (idx, shard_file) in shard_files.iter().enumerate() {
        let shard_path = model_path.join(shard_file);
        if !shard_path.exists() {
            return Err(AosError::Config(format!(
                "Shard file '{}' referenced in index but not found",
                shard_path.display()
            )));
        }

        info!(
            shard = idx + 1,
            total = shard_files.len(),
            path = %shard_path.display(),
            "Loading shard"
        );

        let bytes = std::fs::read(&shard_path).map_err(|e| {
            AosError::Config(format!(
                "Failed to read shard '{}': {}",
                shard_path.display(),
                e
            ))
        })?;
        shard_bytes.push(bytes);
    }

    // Parse all shards and collect tensor views
    let mut all_tensors: Vec<(String, TensorView<'_>)> = Vec::new();
    let mut parsed_shards: Vec<SafeTensors<'_>> = Vec::with_capacity(shard_bytes.len());

    // First pass: parse all shards (we need to keep SafeTensors alive for borrowing)
    for (idx, bytes) in shard_bytes.iter().enumerate() {
        let tensors = SafeTensors::deserialize(bytes).map_err(|e| {
            AosError::Config(format!(
                "Failed to parse shard {} as SafeTensors: {}",
                shard_files[idx], e
            ))
        })?;
        parsed_shards.push(tensors);
    }

    // Second pass: collect all tensor names and views
    for (shard_idx, shard) in parsed_shards.iter().enumerate() {
        for (name, _) in shard.tensors() {
            // Get the tensor view from this shard
            let view = shard.tensor(&name).map_err(|e| {
                AosError::Config(format!(
                    "Failed to get tensor '{}' from shard {}: {}",
                    name, shard_files[shard_idx], e
                ))
            })?;

            // Create a proper TensorView for serialization
            let tensor_view = TensorView::new(view.dtype(), view.shape().to_vec(), view.data())
                .map_err(|e| {
                    AosError::Config(format!(
                        "Failed to create tensor view for '{}': {}",
                        name, e
                    ))
                })?;

            all_tensors.push((name, tensor_view));
        }
    }

    info!(
        total_shards = shard_files.len(),
        total_tensors = all_tensors.len(),
        "Collected tensors from all shards, serializing unified buffer"
    );

    // Serialize all tensors into a single SafeTensors buffer
    let merged_bytes = safetensors::serialize(all_tensors, &None)
        .map_err(|e| AosError::Config(format!("Failed to serialize merged tensors: {}", e)))?;

    info!(
        total_shards = shard_files.len(),
        total_bytes = merged_bytes.len(),
        "Merged all shards into unified SafeTensors buffer"
    );

    Ok(merged_bytes)
}

/// Detect sharded model pattern and return shard information
///
/// Returns `Some((first_shard_path, total_shards, found_shard_indices))` if sharded model found,
/// `None` if no sharded model pattern detected.
fn detect_sharded_model(model_path: &Path) -> Result<Option<(PathBuf, usize, Vec<usize>)>> {
    let entries = match std::fs::read_dir(model_path) {
        Ok(entries) => entries,
        Err(e) => {
            warn!(
                path = %model_path.display(),
                error = %e,
                "Failed to read model directory"
            );
            return Ok(None);
        }
    };

    // Pattern: model-XXXXX-of-YYYYY.safetensors
    let shard_pattern = Regex::new(r"^model-(\d+)-of-(\d+)\.safetensors$").map_err(|e| {
        error!(
            error = %e,
            pattern = r"^model-(\d+)-of-(\d+)\.safetensors$",
            path = %model_path.display(),
            "Failed to compile shard regex"
        );
        AosError::Internal("Failed to compile shard regex".to_string())
    })?;

    let mut first_shard_path: Option<PathBuf> = None;
    let mut total_shards: Option<usize> = None;
    let mut found_shards: Vec<usize> = Vec::new();

    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().to_string();
        if let Some(caps) = shard_pattern.captures(&file_name) {
            let shard_num: usize = caps[1].parse().unwrap_or(0);
            let total: usize = caps[2].parse().unwrap_or(0);

            if total_shards.is_none() {
                total_shards = Some(total);
            } else if total_shards != Some(total) {
                // Inconsistent total shard count - this shouldn't happen in valid models
                warn!(
                    file = %file_name,
                    expected_total = ?total_shards,
                    found_total = total,
                    "Inconsistent shard total in filename"
                );
            }

            found_shards.push(shard_num);

            if shard_num == 1 {
                first_shard_path = Some(entry.path());
            }
        }
    }

    match (first_shard_path, total_shards) {
        (Some(path), Some(total)) => {
            found_shards.sort();
            Ok(Some((path, total, found_shards)))
        }
        (None, Some(total)) => {
            // Found shard metadata but no first shard
            Err(AosError::Config(format!(
                "Sharded model at '{}' is missing first shard (model-00001-of-{:05}.safetensors)",
                model_path.display(),
                total
            )))
        }
        _ => Ok(None),
    }
}
