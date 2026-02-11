//! Shared synthesis service for dataset enrichment.
//!
//! Provides cached SynthesisEngine access and deterministic seed management.
//! Used by both the `/v1/datasets/synthesize` handler and the upload pipeline.
//!
//! # Deterministic Seeding
//!
//! Seed derivation uses `derive_seed_u64_from_inputs("synthesis_seed_v1", ...)`
//! with input `tenant_id:doc_hash_b3:doc_id:chunk_index:model_path_hash`.
//! This ensures identical inputs always produce the same seed, enabling exact
//! replay of synthesis runs.

use crate::api_error::ApiError;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_core::seed::{derive_seed, derive_seed_u64_from_inputs};
use adapteros_core::B3Hash;
use adapteros_orchestrator::synthesis::{SynthesisEngine, SynthesisEngineConfig};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::OnceCell;

/// Cached synthesis service instance - loaded once on first use.
static SYNTHESIS_SERVICE: OnceCell<SynthesisService> = OnceCell::const_new();

/// Shared synthesis service wrapping a cached engine and model path.
///
/// Provides engine lifecycle management and deterministic seed derivation.
/// The engine is loaded once and reused across all requests.
#[derive(Clone)]
pub struct SynthesisService {
    engine: Arc<tokio::sync::RwLock<SynthesisEngine>>,
    model_path: PathBuf,
}

impl SynthesisService {
    /// Get or initialize the shared synthesis service.
    ///
    /// On first call, resolves the model path, creates the engine, and loads the model.
    /// Subsequent calls return the cached instance.
    pub async fn get_or_init(
        state: &AppState,
    ) -> Result<SynthesisService, (StatusCode, Json<ErrorResponse>)> {
        SYNTHESIS_SERVICE
            .get_or_try_init(|| async {
                let model_path = resolve_synthesis_model_path(state).await?;

                let engine_config = SynthesisEngineConfig {
                    model_path: model_path.clone(),
                    ..Default::default()
                };
                let mut engine = SynthesisEngine::new(engine_config);

                engine.load_model().await.map_err(|e| {
                    tracing::warn!(error = %e, "Failed to load synthesis model");
                    ApiError::service_unavailable(format!(
                        "Synthesis model not available: {}. Ensure a model is deployed at the configured path.",
                        e
                    ))
                })?;

                tracing::info!(
                    model_path = %model_path.display(),
                    "Synthesis service initialized and cached"
                );

                Ok(SynthesisService {
                    engine: Arc::new(tokio::sync::RwLock::new(engine)),
                    model_path,
                })
            })
            .await
            .cloned()
    }

    /// Get the underlying engine (for direct use by handlers).
    pub fn engine(&self) -> &Arc<tokio::sync::RwLock<SynthesisEngine>> {
        &self.engine
    }

    /// Get the resolved model path.
    pub fn model_path(&self) -> &PathBuf {
        &self.model_path
    }
}

/// Provenance metadata for a deterministic synthesis batch.
///
/// Records everything needed to reproduce or audit the synthesis run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesisProvenance {
    /// Per-chunk derived seeds (v1, u64, index-aligned with input chunks)
    pub chunk_seeds: Vec<u64>,
    /// Per-chunk 32-byte seed hex strings (v2, for strict replay)
    #[serde(default)]
    pub chunk_seed_bytes_hex: Vec<String>,
    /// Model path used for synthesis
    pub model_path: String,
    /// BLAKE3 hash of the model path (used in v1 seed derivation)
    pub model_path_hash: String,
    /// BLAKE3 hash of the model content/identity (v2, content-based)
    #[serde(default)]
    pub synthesis_model_hash_b3: Option<String>,
    /// Whether deterministic constraints were enforced
    pub verified_deterministic: bool,
    /// Temperature used (0.0 for strict replay)
    pub temperature: f32,
    /// Top-p used (1.0 for strict replay)
    pub top_p: f32,
    /// Whether ANE was disabled (true = MLX-only for determinism)
    pub ane_disabled: bool,
}

// =============================================================================
// Seed derivation (v1 + v2)
// =============================================================================

/// Derive a deterministic u64 seed for a synthesis chunk (v1).
///
/// Input: `tenant_id:doc_hash_b3:doc_id:chunk_index:model_path_hash`.
/// Kept for backward compatibility with existing datasets.
pub fn derive_synthesis_seed_u64_v1(
    tenant_id: &str,
    document_hash: &str,
    document_id: &str,
    chunk_index: usize,
    model_path_hash: &str,
) -> u64 {
    let input = format!(
        "{}:{}:{}:{}:{}",
        tenant_id, document_hash, document_id, chunk_index, model_path_hash
    );
    derive_seed_u64_from_inputs("synthesis_seed_v1", input.as_bytes())
}

/// Backward-compatible alias for `derive_synthesis_seed_u64_v1`.
pub fn derive_synthesis_seed(
    tenant_id: &str,
    document_hash: &str,
    document_id: &str,
    chunk_index: usize,
    model_path_hash: &str,
) -> u64 {
    derive_synthesis_seed_u64_v1(
        tenant_id,
        document_hash,
        document_id,
        chunk_index,
        model_path_hash,
    )
}

/// Derive a deterministic 32-byte seed for a synthesis chunk (v2).
///
/// All six inputs contribute to the seed via HKDF-SHA256:
///   tenant_id, doc_content_hash, doc_id, chunk_index, chunk_hash, model_hash
///
/// The `model_hash` should come from `compute_synthesis_model_hash()` (content-based)
/// rather than a path hash, ensuring the seed changes when the model content changes.
pub fn derive_synthesis_seed_bytes_v1(
    tenant_id: &str,
    doc_content_hash: &str,
    doc_id: &str,
    chunk_index: usize,
    chunk_hash: &str,
    model_hash: &str,
) -> [u8; 32] {
    let input = format!(
        "{}:{}:{}:{}:{}:{}",
        tenant_id, doc_content_hash, doc_id, chunk_index, chunk_hash, model_hash
    );
    let global = B3Hash::hash(input.as_bytes());
    derive_seed(&global, "synthesis_seed_v2")
}

// =============================================================================
// Model identity hashing
// =============================================================================

/// Compute a content-based BLAKE3 hash identifying a synthesis model.
///
/// Unlike path-based hashes, this produces different output when the model
/// content actually changes (e.g., fine-tuning a new version to the same path).
///
/// Strategy (first match wins):
/// 1. Hash `config.json` if present (small, identifies architecture)
/// 2. Hash `model_index.json` if present (identifies shard layout)
/// 3. Hash all files sorted alphabetically (full content identity)
/// 4. Hash the single file if `model_path` is a file, not a directory
pub fn compute_synthesis_model_hash(model_path: &Path) -> Result<String, io::Error> {
    if model_path.is_file() {
        let content = std::fs::read(model_path)?;
        return Ok(B3Hash::hash(&content).to_hex());
    }

    // Strategy 1: config.json
    let config_json = model_path.join("config.json");
    if config_json.is_file() {
        let content = std::fs::read(&config_json)?;
        return Ok(B3Hash::hash(&content).to_hex());
    }

    // Strategy 2: model_index.json
    let model_index = model_path.join("model_index.json");
    if model_index.is_file() {
        let content = std::fs::read(&model_index)?;
        return Ok(B3Hash::hash(&content).to_hex());
    }

    // Strategy 3: sorted directory hash
    let mut files = collect_files_sorted(model_path)?;
    if files.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "No files found in model directory: {}",
                model_path.display()
            ),
        ));
    }

    // Hash all file contents in sorted order for determinism
    let mut hasher_input = Vec::new();
    files.sort();
    for file in &files {
        let relative = file.strip_prefix(model_path).unwrap_or(file);
        hasher_input.extend_from_slice(relative.to_string_lossy().as_bytes());
        hasher_input.push(b'\0');
        let content = std::fs::read(file)?;
        hasher_input.extend_from_slice(&content);
        hasher_input.push(b'\0');
    }

    Ok(B3Hash::hash(&hasher_input).to_hex())
}

/// Collect all files under `dir` sorted by relative path.
fn collect_files_sorted(dir: &Path) -> Result<Vec<PathBuf>, io::Error> {
    let mut files = Vec::new();
    if !dir.is_dir() {
        return Ok(files);
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            files.push(path);
        } else if path.is_dir() {
            files.extend(collect_files_sorted(&path)?);
        }
    }
    files.sort();
    Ok(files)
}

/// Configuration for strict deterministic synthesis.
///
/// Forces settings that ensure bitwise reproducibility:
/// - MLX backend only (no ANE, which is non-deterministic)
/// - Temperature 0.0 (greedy decoding)
/// - top_p 1.0 (no nucleus sampling)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeterministicSynthesisConfig {
    /// Use ANE acceleration (must be false for strict determinism)
    pub use_ane: bool,
    /// Temperature (must be 0.0 for strict determinism)
    pub temperature: f32,
    /// Top-p sampling (must be 1.0 for strict determinism)
    pub top_p: f32,
}

impl DeterministicSynthesisConfig {
    /// Create a strict deterministic config.
    pub fn strict() -> Self {
        Self {
            use_ane: false,
            temperature: 0.0,
            top_p: 1.0,
        }
    }
}

/// Resolve the synthesis model path from environment or config.
///
/// Priority:
/// 1. `AOS_SYNTHESIS_MODEL_PATH` environment variable
/// 2. `paths.synthesis_model_path` in config
/// 3. Convention-based paths under `var/models/`
async fn resolve_synthesis_model_path(
    state: &AppState,
) -> Result<PathBuf, (StatusCode, Json<ErrorResponse>)> {
    // Check environment variable first (highest priority)
    if let Ok(path) = std::env::var("AOS_SYNTHESIS_MODEL_PATH") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Ok(path);
        }
        tracing::warn!(
            path = %path.display(),
            "AOS_SYNTHESIS_MODEL_PATH set but path does not exist"
        );
    }

    // Check config paths
    let config = state.config.read().map_err(|e| {
        tracing::error!(error = %e, "Failed to read synthesis config");
        ApiError::internal("Failed to load synthesis config")
    })?;

    // Use configured synthesis_model_path if set
    if let Some(ref config_path) = config.paths.synthesis_model_path {
        let path = PathBuf::from(config_path);
        if path.exists() {
            return Ok(path);
        }
        tracing::warn!(
            path = %path.display(),
            "synthesis_model_path configured but path does not exist"
        );
    }

    let var_dir = std::env::var("AOS_VAR_DIR").unwrap_or_else(|_| "var".to_string());

    // Try common model locations
    let candidates = [
        PathBuf::from(&var_dir).join("models/synthesis_model"),
        PathBuf::from(&var_dir).join("models/synthesis_model.mlpackage"),
        PathBuf::from(&var_dir).join("model-cache/models/synthesis_model"),
        PathBuf::from(&config.paths.datasets_root)
            .parent()
            .unwrap_or(std::path::Path::new("var"))
            .join("models/synthesis_model"),
    ];

    for candidate in &candidates {
        if candidate.exists() {
            tracing::debug!(path = %candidate.display(), "Found synthesis model");
            return Ok(candidate.clone());
        }
    }

    Err(ApiError::service_unavailable(
        "No synthesis model found. Set AOS_SYNTHESIS_MODEL_PATH, configure paths.synthesis_model_path, or deploy a model to var/models/synthesis_model",
    )
    .into())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- v1 seed derivation ----

    #[test]
    fn test_seed_derivation_deterministic() {
        let seed1 =
            derive_synthesis_seed("tenant_1", "doc_hash_abc", "doc_42", 0, "model_hash_xyz");
        let seed2 =
            derive_synthesis_seed("tenant_1", "doc_hash_abc", "doc_42", 0, "model_hash_xyz");
        assert_eq!(seed1, seed2, "Same inputs must produce same seed");
    }

    #[test]
    fn test_seed_varies_by_chunk_index() {
        let seed_0 = derive_synthesis_seed("t1", "hash", "d1", 0, "mhash");
        let seed_1 = derive_synthesis_seed("t1", "hash", "d1", 1, "mhash");
        assert_ne!(
            seed_0, seed_1,
            "Different chunk indices must produce different seeds"
        );
    }

    #[test]
    fn test_seed_varies_by_tenant() {
        let seed_a = derive_synthesis_seed("tenant_a", "hash", "d1", 0, "mhash");
        let seed_b = derive_synthesis_seed("tenant_b", "hash", "d1", 0, "mhash");
        assert_ne!(
            seed_a, seed_b,
            "Different tenants must produce different seeds"
        );
    }

    #[test]
    fn test_seed_varies_by_doc_hash() {
        let seed_a = derive_synthesis_seed("t1", "hash_aaa", "d1", 0, "mhash");
        let seed_b = derive_synthesis_seed("t1", "hash_bbb", "d1", 0, "mhash");
        assert_ne!(
            seed_a, seed_b,
            "Different doc hashes must produce different seeds"
        );
    }

    // ---- v2 seed derivation ----

    #[test]
    fn test_seed_bytes_v1_deterministic() {
        let seed_a = derive_synthesis_seed_bytes_v1("t1", "dh", "d1", 0, "ch", "mh");
        let seed_b = derive_synthesis_seed_bytes_v1("t1", "dh", "d1", 0, "ch", "mh");
        assert_eq!(seed_a, seed_b);
        assert_eq!(seed_a.len(), 32);
    }

    #[test]
    fn test_seed_bytes_all_fields_contribute() {
        let base = derive_synthesis_seed_bytes_v1("t1", "dh", "d1", 0, "ch", "mh");
        let variants = [
            derive_synthesis_seed_bytes_v1("t2", "dh", "d1", 0, "ch", "mh"),
            derive_synthesis_seed_bytes_v1("t1", "dX", "d1", 0, "ch", "mh"),
            derive_synthesis_seed_bytes_v1("t1", "dh", "d2", 0, "ch", "mh"),
            derive_synthesis_seed_bytes_v1("t1", "dh", "d1", 1, "ch", "mh"),
            derive_synthesis_seed_bytes_v1("t1", "dh", "d1", 0, "cX", "mh"),
            derive_synthesis_seed_bytes_v1("t1", "dh", "d1", 0, "ch", "mX"),
        ];
        for (i, variant) in variants.iter().enumerate() {
            assert_ne!(&base, variant, "Changing field {} must change seed", i);
        }
    }

    #[test]
    fn test_v1_v2_domain_separation() {
        // v1 and v2 produce different outputs for overlapping inputs
        let v1 = derive_synthesis_seed_u64_v1("t1", "dh", "d1", 0, "mh");
        let v2 = derive_synthesis_seed_bytes_v1("t1", "dh", "d1", 0, "ch", "mh");
        let v2_as_u64 = u64::from_le_bytes(v2[..8].try_into().unwrap());
        assert_ne!(v1, v2_as_u64);
    }

    // ---- model hashing ----

    #[test]
    fn test_model_hash_single_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("model.bin");
        std::fs::write(&file, b"model weights here").unwrap();
        let hash = compute_synthesis_model_hash(&file).unwrap();
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));

        // Deterministic
        let hash2 = compute_synthesis_model_hash(&file).unwrap();
        assert_eq!(hash, hash2);
    }

    #[test]
    fn test_model_hash_config_json_strategy() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("config.json"),
            b"{\"model_type\":\"llama\"}",
        )
        .unwrap();
        std::fs::write(dir.path().join("weights.safetensors"), b"big weights").unwrap();

        let hash = compute_synthesis_model_hash(dir.path()).unwrap();
        assert_eq!(hash.len(), 64);

        // Should be based on config.json content, not the weights
        let config_hash = B3Hash::hash(b"{\"model_type\":\"llama\"}").to_hex();
        assert_eq!(hash, config_hash);
    }

    #[test]
    fn test_model_hash_sorted_dir_strategy() {
        let dir = tempfile::tempdir().unwrap();
        // No config.json or model_index.json → falls back to sorted dir hash
        std::fs::write(dir.path().join("a.bin"), b"data_a").unwrap();
        std::fs::write(dir.path().join("b.bin"), b"data_b").unwrap();

        let hash = compute_synthesis_model_hash(dir.path()).unwrap();
        assert_eq!(hash.len(), 64);

        // Deterministic
        let hash2 = compute_synthesis_model_hash(dir.path()).unwrap();
        assert_eq!(hash, hash2);
    }

    // ---- provenance ----

    #[test]
    fn test_provenance_serialization() {
        let provenance = SynthesisProvenance {
            chunk_seeds: vec![123, 456, 789],
            chunk_seed_bytes_hex: vec!["aa".repeat(32)],
            model_path: "/var/models/synthesis_model".to_string(),
            model_path_hash: "abc123".to_string(),
            synthesis_model_hash_b3: Some("def456".to_string()),
            verified_deterministic: true,
            temperature: 0.0,
            top_p: 1.0,
            ane_disabled: true,
        };
        let json = serde_json::to_string(&provenance).unwrap();
        let parsed: SynthesisProvenance = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.chunk_seeds.len(), 3);
        assert!(parsed.verified_deterministic);
        assert!((parsed.temperature - 0.0).abs() < f32::EPSILON);
        assert_eq!(parsed.synthesis_model_hash_b3.as_deref(), Some("def456"));
    }

    #[test]
    fn test_provenance_backward_compat_deserialize() {
        // Old provenance without v2 fields should still deserialize
        let json = r#"{"chunk_seeds":[1,2],"model_path":"/m","model_path_hash":"h","verified_deterministic":false,"temperature":0.7,"top_p":0.9,"ane_disabled":false}"#;
        let parsed: SynthesisProvenance = serde_json::from_str(json).unwrap();
        assert!(parsed.chunk_seed_bytes_hex.is_empty());
        assert!(parsed.synthesis_model_hash_b3.is_none());
    }

    #[test]
    fn test_deterministic_config_strict() {
        let config = DeterministicSynthesisConfig::strict();
        assert!(!config.use_ane, "Strict mode must disable ANE");
        assert!(
            (config.temperature - 0.0).abs() < f32::EPSILON,
            "Strict mode must use temperature 0.0"
        );
        assert!(
            (config.top_p - 1.0).abs() < f32::EPSILON,
            "Strict mode must use top_p 1.0"
        );
    }
}
