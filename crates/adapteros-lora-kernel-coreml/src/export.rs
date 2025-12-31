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
use std::time::Duration;
#[allow(unused_imports)]
use tracing::{info, warn};

/// Default export timeout (5 minutes)
fn default_export_timeout() -> Duration {
    Duration::from_secs(300)
}

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
    /// Allow overwriting existing output path.
    #[serde(default)]
    pub allow_overwrite: bool,
    /// Export timeout duration.
    #[serde(default = "default_export_timeout")]
    pub timeout: Duration,
    /// Skip ops compatibility validation.
    #[serde(default)]
    pub skip_ops_check: bool,
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
    #[serde(default)]
    pub fusion_verified: bool,
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
    #[serde(default)]
    pub fusion_verified: bool,
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

// ============================================================================
// Pre-export validation functions
// ============================================================================

/// Supported CoreML operations for LoRA fusion export.
///
/// This list includes operations commonly used in transformer models that are
/// compatible with CoreML's Neural Engine execution path. Operations are stored
/// in canonical lowercase form for exact matching.
const SUPPORTED_COREML_OPS: &[&str] = &[
    // Linear algebra
    "linear",
    "matmul",
    "inner_product",
    "batched_matmul",
    // Convolutions
    "conv2d",
    "convolution",
    "conv",
    // Normalization
    "batchnorm",
    "batch_normalization",
    "layernorm",
    "layer_normalization",
    "instancenorm",
    "instance_normalization",
    "l2_normalize",
    // Activations
    "relu",
    "leaky_relu",
    "prelu",
    "elu",
    "selu",
    "gelu",
    "sigmoid",
    "tanh",
    "softmax",
    "softplus",
    "softsign",
    "hard_sigmoid",
    "hard_swish",
    "silu",
    "swish",
    // Attention
    "attention",
    "multihead_attention",
    "scaled_dot_product_attention",
    // Embedding
    "embedding",
    "embedding_nd",
    // Element-wise
    "add",
    "sub",
    "mul",
    "div",
    "maximum",
    "minimum",
    // Shape operations
    "concat",
    "reshape",
    "transpose",
    "permute",
    "split",
    "gather",
    "gather_nd",
    "slice",
    "slice_by_index",
    "slice_by_size",
    "squeeze",
    "expand_dims",
    "flatten",
    "tile",
    "pad",
    // Reduction
    "reduce_mean",
    "reduce_sum",
    "reduce_max",
    "reduce_min",
    "reduce_prod",
    "reduce_l2",
    // Math
    "sqrt",
    "rsqrt",
    "exp",
    "log",
    "pow",
    "abs",
    "neg",
    "sign",
    "floor",
    "ceil",
    "round",
    "clip",
    // Type casting
    "cast",
    "const",
    "identity",
    // Pooling
    "max_pool",
    "avg_pool",
    "global_avg_pool",
    "global_max_pool",
    // Misc
    "dropout",
    "where",
    "select",
];

/// Validate that the export output path is safe to write.
///
/// # Arguments
/// * `output_path` - Destination path for exported package
/// * `allow_overwrite` - If true, allows overwriting existing paths
///
/// # Returns
/// * `Ok(())` if path is safe to write
/// * `Err(CoreMLExportPathExists)` if path exists and overwrite not allowed
pub fn validate_output_path(output_path: &Path, allow_overwrite: bool) -> Result<()> {
    if !output_path.exists() {
        return Ok(());
    }

    if allow_overwrite {
        warn!(
            path = %output_path.display(),
            "Export output path exists; will be overwritten"
        );
        return Ok(());
    }

    let file_count = if output_path.is_dir() {
        fs::read_dir(output_path)
            .map(|entries| entries.count())
            .unwrap_or(1)
    } else {
        1
    };

    Err(AosError::CoreMLExportPathExists {
        path: output_path.display().to_string(),
        file_count,
    })
}

/// Required file patterns for CoreML package validation.
const REQUIRED_WEIGHT_EXTENSIONS: &[&str] = &[".mlmodel", ".bin", ".weights", ".mil"];

/// Validate that a CoreML package contains all required weight files.
///
/// Performs thorough validation of the CoreML package structure:
/// 1. Checks for Manifest.json
/// 2. Validates Data directory exists and contains model subdirectories
/// 3. Verifies weight files exist within model directories
/// 4. Validates Manifest.json is valid JSON with required fields
///
/// # Arguments
/// * `package_path` - Path to CoreML package directory or Manifest.json
///
/// # Returns
/// * `Ok(())` if all required files exist and are valid
/// * `Err(CoreMLMissingWeights)` listing missing or invalid files
pub fn validate_coreml_weights(package_path: &Path) -> Result<()> {
    let package_dir = if package_path.is_dir() {
        package_path.to_path_buf()
    } else {
        package_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| package_path.to_path_buf())
    };

    let mut missing = Vec::new();

    // Check for Manifest.json and validate its contents
    let manifest_path = package_dir.join("Manifest.json");
    if !manifest_path.exists() {
        missing.push("Manifest.json".to_string());
    } else {
        // Validate Manifest.json is valid JSON with required structure
        match validate_manifest_structure(&manifest_path) {
            Ok(()) => {}
            Err(e) => missing.push(format!("Manifest.json (invalid: {})", e)),
        }
    }

    // Check for Data directory with model assets
    let data_dir = package_dir.join("Data");
    if !data_dir.exists() || !data_dir.is_dir() {
        missing.push("Data/".to_string());
    } else {
        // Find model directories within Data
        let model_dirs: Vec<_> = fs::read_dir(&data_dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().is_dir())
                    .collect()
            })
            .unwrap_or_default();

        if model_dirs.is_empty() {
            missing.push("Data/*/ (no model directories)".to_string());
        } else {
            // Validate each model directory contains required files
            for model_dir in &model_dirs {
                let model_path = model_dir.path();
                let model_name = model_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown".to_string());

                // Check for model.mlmodel or model spec file
                let has_model_spec = model_path.join("model.mlmodel").exists()
                    || model_path.join("model.mlmodelc").exists()
                    || model_path.join("coremldata.bin").exists();

                if !has_model_spec {
                    // Check for any weight files
                    let has_weights = fs::read_dir(&model_path)
                        .map(|entries| {
                            entries.filter_map(|e| e.ok()).any(|e| {
                                let path = e.path();
                                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                                    REQUIRED_WEIGHT_EXTENSIONS
                                        .iter()
                                        .any(|req| req.trim_start_matches('.') == ext)
                                } else {
                                    // Check for binary files without extension
                                    path.file_name()
                                        .map(|n| {
                                            let name = n.to_string_lossy();
                                            name.contains("weight")
                                                || name.contains("model")
                                                || name.ends_with(".bin")
                                        })
                                        .unwrap_or(false)
                                }
                            })
                        })
                        .unwrap_or(false);

                    if !has_weights {
                        missing.push(format!(
                            "Data/{model_name}/ (no model spec or weight files)"
                        ));
                    }
                }
            }
        }
    }

    if missing.is_empty() {
        Ok(())
    } else {
        Err(AosError::CoreMLMissingWeights {
            package_path: package_dir.display().to_string(),
            missing,
        })
    }
}

/// Validate the structure of a CoreML Manifest.json file.
fn validate_manifest_structure(manifest_path: &Path) -> std::result::Result<(), String> {
    let bytes = fs::read(manifest_path).map_err(|e| format!("read error: {}", e))?;

    let manifest: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("JSON parse error: {}", e))?;

    // Check for required top-level fields
    let obj = manifest
        .as_object()
        .ok_or_else(|| "not a JSON object".to_string())?;

    // Manifest should have at least one of these fields
    let has_required_field = obj.contains_key("itemInfoEntries")
        || obj.contains_key("rootModelIdentifier")
        || obj.contains_key("modelIdentifier");

    if !has_required_field {
        return Err(
            "missing required fields (itemInfoEntries, rootModelIdentifier, or modelIdentifier)"
                .to_string(),
        );
    }

    // If itemInfoEntries exists, validate it's an object
    if let Some(items) = obj.get("itemInfoEntries") {
        if !items.is_object() {
            return Err("itemInfoEntries is not an object".to_string());
        }
    }

    Ok(())
}

/// Validate that a CoreML package only uses supported operations.
///
/// # Arguments
/// * `manifest_path` - Path to CoreML Manifest.json
///
/// # Returns
/// * `Ok(())` if all ops are supported
/// * `Err(CoreMLUnsupportedOps)` listing unsupported operations
pub fn validate_coreml_ops(manifest_path: &Path) -> Result<()> {
    let manifest_bytes = fs::read(manifest_path).map_err(|e| {
        AosError::Io(format!(
            "Failed to read manifest for ops validation {}: {}",
            manifest_path.display(),
            e
        ))
    })?;

    let manifest: serde_json::Value =
        serde_json::from_slice(&manifest_bytes).map_err(AosError::Serialization)?;

    let ops = extract_ops_from_manifest(&manifest);
    let unsupported: Vec<String> = ops
        .iter()
        .filter(|op| !is_supported_op(op))
        .cloned()
        .collect();

    if unsupported.is_empty() {
        Ok(())
    } else {
        Err(AosError::CoreMLUnsupportedOps {
            model_path: manifest_path.display().to_string(),
            ops: unsupported,
        })
    }
}

/// Check if an operation is supported using exact matching.
///
/// Normalizes the operation name to lowercase and checks against
/// the supported operations list. This uses exact matching to avoid
/// false positives (e.g., "nonlinear" should NOT match "linear").
pub fn is_supported_op(op: &str) -> bool {
    let normalized = normalize_op_name(op);
    SUPPORTED_COREML_OPS.contains(&normalized.as_str())
}

/// Normalize an operation name for comparison.
///
/// Handles common naming variations:
/// - Converts to lowercase
/// - Strips common prefixes like "coreml.", "com.apple.coreml."
/// - Removes version suffixes like "_v2", "_v3"
pub fn normalize_op_name(op: &str) -> String {
    let mut normalized = op.to_lowercase();

    // Strip common CoreML prefixes
    for prefix in &["coreml.", "com.apple.coreml.", "com.apple."] {
        if normalized.starts_with(prefix) {
            normalized = normalized[prefix.len()..].to_string();
        }
    }

    // Strip version suffixes like _v2, _v3, etc.
    if let Some(pos) = normalized.rfind("_v") {
        if normalized[pos + 2..].chars().all(|c| c.is_ascii_digit()) {
            normalized = normalized[..pos].to_string();
        }
    }

    // Strip trailing underscores and numbers (e.g., "add_0" -> "add")
    while normalized.ends_with('_')
        || normalized
            .chars()
            .last()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
    {
        normalized.pop();
    }

    normalized
}

/// Extract operation types from CoreML manifest and model specification.
///
/// Parses the manifest JSON and recursively extracts operation type identifiers
/// from the model specification structure.
pub fn extract_ops_from_manifest(manifest: &serde_json::Value) -> Vec<String> {
    let mut ops = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Extract from itemInfoEntries (top-level manifest structure)
    if let Some(items) = manifest.get("itemInfoEntries").and_then(|v| v.as_object()) {
        for (_key, item) in items {
            extract_ops_from_value(item, &mut ops, &mut seen);
        }
    }

    // Extract from rootModelIdentifier
    if let Some(root) = manifest.get("rootModelIdentifier").and_then(|v| v.as_str()) {
        if seen.insert(root.to_string()) {
            ops.push(root.to_string());
        }
    }

    // Recursively search for operation types in the entire manifest
    extract_ops_recursive(manifest, &mut ops, &mut seen);

    ops
}

/// Recursively extract operation types from a JSON value.
fn extract_ops_recursive(
    value: &serde_json::Value,
    ops: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, val) in map {
                // Look for operation type indicators
                if key == "type" || key == "op" || key == "opType" || key == "operation" {
                    if let Some(op_str) = val.as_str() {
                        if seen.insert(op_str.to_string()) {
                            ops.push(op_str.to_string());
                        }
                    }
                }
                // Recurse into nested objects
                extract_ops_recursive(val, ops, seen);
            }
        }
        serde_json::Value::Array(arr) => {
            for val in arr {
                extract_ops_recursive(val, ops, seen);
            }
        }
        _ => {}
    }
}

/// Extract operation types from a single item info entry.
fn extract_ops_from_value(
    item: &serde_json::Value,
    ops: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    // Direct type field
    if let Some(op_type) = item.get("type").and_then(|v| v.as_str()) {
        if seen.insert(op_type.to_string()) {
            ops.push(op_type.to_string());
        }
    }

    // Operation field
    if let Some(op_type) = item.get("op").and_then(|v| v.as_str()) {
        if seen.insert(op_type.to_string()) {
            ops.push(op_type.to_string());
        }
    }

    // Name field - extract operation type from path-like names
    if let Some(op_name) = item.get("name").and_then(|v| v.as_str()) {
        // Extract the last meaningful component from paths like "model/layer_0/attention"
        if let Some(last_part) = op_name.split('/').next_back() {
            // Filter out numeric-only parts (layer indices)
            if !last_part.chars().all(|c| c.is_ascii_digit() || c == '_') {
                // Extract operation type, removing layer indices
                let op_part = last_part
                    .split('_')
                    .filter(|s| !s.chars().all(|c| c.is_ascii_digit()))
                    .collect::<Vec<_>>()
                    .join("_");
                if !op_part.is_empty() && seen.insert(op_part.clone()) {
                    ops.push(op_part);
                }
            }
        }
    }

    // Recurse into nested structures
    if let Some(obj) = item.as_object() {
        for (key, val) in obj {
            if key != "type" && key != "op" && key != "name" {
                extract_ops_from_value(val, ops, seen);
            }
        }
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
    // Pre-validation: check output path
    validate_output_path(&req.output_package, req.allow_overwrite)?;

    // Pre-validation: check base package has required files
    validate_coreml_weights(&req.base_package)?;

    let manifest_path = manifest_path_for(&req.base_package)?;
    if !manifest_path.exists() {
        return Err(AosError::NotFound(format!(
            "CoreML manifest not found at {}",
            manifest_path.display()
        )));
    }

    // Pre-validation: check ops compatibility (unless skipped)
    if !req.skip_ops_check {
        validate_coreml_ops(&manifest_path)?;
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
    let metadata_only = fused_manifest_hash == base_manifest_hash;
    let fusion_verified = !metadata_only;
    if metadata_only {
        warn!(
            base_hash = %base_manifest_hash,
            fused_hash = %fused_manifest_hash,
            "CoreML export produced metadata-only package (fused manifest matches base)"
        );
    }

    let metadata = CoreMLFusionMetadata {
        base_manifest_hash,
        fused_manifest_hash,
        adapter_hash,
        base_package: req.base_package.clone(),
        fused_package: req.output_package.clone(),
        adapter_path: req.adapter_aos.clone(),
        fusion_verified,
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
        fusion_verified = fusion_verified,
        "CoreML export completed"
    );

    Ok(CoreMLExportOutcome {
        fused_package: req.output_package.clone(),
        metadata_path,
        base_manifest_hash,
        fused_manifest_hash,
        adapter_hash,
        fusion_verified,
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

/// Export an adapter into a CoreML package with timeout protection (async version).
///
/// Wraps the synchronous `export_coreml_adapter` in a blocking task with a configurable
/// timeout. This prevents the export from blocking indefinitely on large packages.
///
/// # Arguments
/// * `req` - Export request configuration including timeout
///
/// # Returns
/// * `Ok(CoreMLExportOutcome)` on success
/// * `Err(CoreMLExportTimeout)` if export exceeds configured timeout
/// * Other errors from `export_coreml_adapter`
pub async fn export_coreml_adapter_async(req: &CoreMLExportRequest) -> Result<CoreMLExportOutcome> {
    let req_clone = req.clone();
    let timeout_duration = req.timeout;
    let output_path = req.output_package.display().to_string();

    let result = tokio::time::timeout(
        timeout_duration,
        tokio::task::spawn_blocking(move || export_coreml_adapter(&req_clone)),
    )
    .await;

    match result {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(join_err)) => Err(AosError::Internal(format!(
            "Export task panicked: {}",
            join_err
        ))),
        Err(_elapsed) => Err(AosError::CoreMLExportTimeout {
            operation: format!("export to {}", output_path),
            duration: timeout_duration,
        }),
    }
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
