//! CoreML-aware preprocessing for tokenized training examples.
//!
//! This stage runs deterministic, inference-only preprocessing to convert token
//! IDs into model-optimized representations that can be cached and reused.
//!
//! Cache layout (versioned):
//! - preprocess_manifest.json: summary + compatibility hashes
//! - features.bin: contiguous feature data (f32 or q15)
//! - features.index: per-example offsets + shapes + hashes

use adapteros_types::training::{
    validate_training_examples, ExampleMetadataV1, PreprocessedExampleV1,
    TrainingDataContractConfig, TrainingExampleV1, PREPROCESSED_EXAMPLE_SCHEMA_VERSION,
    PREPROCESSED_FEATURE_BACKEND_COREML, PREPROCESSED_FEATURE_DTYPE_F32,
};

use super::trainer::{PreprocessCompression, PreprocessOutputFeature, PreprocessingConfig};
use adapteros_config::{resolve_base_model_location, ModelConfig};
use adapteros_core::io_utils::{ensure_temp_dir, get_directory_size};
use adapteros_core::path_normalization::normalize_path_for_sorting;
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_storage::platform::common::PlatformUtils;
use blake3::Hasher;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

const PREPROCESS_MANIFEST_VERSION: &str = "1.0";
const DEFAULT_COREML_EMBEDDING_OUTPUT: &str = "embeddings";
const DEFAULT_COREML_HIDDEN_OUTPUT: &str = "hidden_states";
const Q15_MAX: f32 = 32767.0;
const Q15_MIN: f32 = -32768.0;
const Q15_DENOM: f32 = 32767.0;

#[derive(Debug, Clone)]
pub struct PreprocessStats {
    pub backend: String,
    pub cache_hit: bool,
    pub cached_examples: usize,
    pub processed_examples: usize,
    pub elapsed_ms: u128,
    pub cache_dir: String,
    pub preprocess_id: String,
    pub cache_key: String,
    pub coreml_model_hash: Option<String>,
    pub produced_at_unix_ms: u64,
}

#[derive(Debug)]
pub struct PreprocessResult {
    pub examples: Vec<PreprocessedExampleV1>,
    pub stats: PreprocessStats,
}

/// Summary of an on-disk preprocessing manifest and compatibility state.
#[derive(Debug, Clone)]
pub struct PreprocessCacheStatus {
    pub preprocess_id: String,
    pub cache_key_b3: String,
    pub cache_dir: String,
    pub manifest_hash_b3: String,
    pub produced_at_unix_ms: Option<u64>,
    pub feature_dtype: String,
    pub backend: String,
    pub compression: String,
    pub cache_hit: bool,
    pub needs_reprocess: bool,
    pub reasons: Vec<String>,
    pub dataset_hash_b3: String,
    pub tokenizer_hash_b3: String,
    pub tokenizer_cfg_hash_b3: String,
    pub preprocessing_config_hash_b3: String,
    pub coreml_model_hash_b3: Option<String>,
    pub base_model_hash_b3: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PreprocessManifest {
    schema_version: String,
    contract_version: String,
    #[serde(default)]
    training_contract_version: Option<String>,
    #[serde(default)]
    pad_token_id: Option<u32>,
    #[serde(default)]
    ignore_index: Option<i32>,
    preprocess_id: String,
    cache_key_b3: String,
    preprocessing_config_hash_b3: String,
    dataset_hash_b3: String,
    split_hash_b3: Option<String>,
    dataset_id: Option<String>,
    base_model_hash_b3: Option<String>,
    coreml_model_hash_b3: Option<String>,
    tokenizer_hash_b3: String,
    tokenizer_cfg_hash_b3: String,
    output_feature: String,
    layer_key: Option<String>,
    max_seq_len: u32,
    batch_size: u32,
    compression: String,
    feature_dtype: String,
    backend: String,
    seed: u64,
    training_seed: u64,
    example_count: usize,
    processed_count: usize,
    produced_at_unix_ms: u64,
    cache_root: String,
    coreml_model_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FeatureIndexEntry {
    example_index: usize,
    offset_bytes: u64,
    element_count: u32,
    feature_shape: Vec<u32>,
    feature_hash: String,
    scale: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
struct PreprocessConfigHashPayload {
    enabled: bool,
    coreml_model_id: Option<String>,
    coreml_model_path: Option<String>,
    output_feature: String,
    layer_key: Option<String>,
    max_seq_len: u32,
    batch_size: u32,
    compression: String,
    seed: u64,
}

#[allow(clippy::too_many_arguments)]
pub fn preprocess_examples(
    examples: &[TrainingExampleV1],
    contract: &TrainingDataContractConfig,
    config: &PreprocessingConfig,
    hidden_dim: usize,
    vocab_size: usize,
    base_model_path: &Path,
    dataset_id: Option<&str>,
    artifacts_root: Option<&Path>,
    split_hash_b3: Option<&str>,
    training_seed: u64,
) -> Result<PreprocessResult> {
    if examples.is_empty() {
        return Err(AosError::Training(
            "preprocessing requested with empty dataset".to_string(),
        ));
    }
    if hidden_dim == 0 {
        return Err(AosError::Training(
            "preprocessing hidden_dim must be greater than zero".to_string(),
        ));
    }
    if !config.enabled {
        return Err(AosError::Config(
            "preprocessing requested but config.enabled=false".to_string(),
        ));
    }

    validate_training_examples(examples, vocab_size, contract).map_err(|e| {
        AosError::Training(format!(
            "Training example contract validation failed before preprocessing: {}",
            e
        ))
    })?;

    let base_model_path = canonicalize_existing_path(base_model_path, "base_model_path")?;
    let model_config = ModelConfig::from_config_json(&base_model_path)?;
    if model_config.hidden_size != hidden_dim {
        return Err(AosError::Training(format!(
            "Preprocessing hidden_dim mismatch: model hidden_size={} config hidden_dim={}",
            model_config.hidden_size, hidden_dim
        )));
    }

    let (tokenizer_hash, tokenizer_cfg_hash) = compute_tokenizer_hashes(&base_model_path)?;
    let dataset_hash = compute_dataset_hash(examples);
    let seed = config.seed.unwrap_or(training_seed);
    let compression = config.compression.unwrap_or(PreprocessCompression::None);
    let output_name = resolve_output_name(config);
    let coreml_model_path = resolve_coreml_model_path(config)?;
    let coreml_model_hash = Some(hash_coreml_model(&coreml_model_path)?);
    let base_model_hash = hash_optional_file(base_model_path.join("config.json"))?;

    let config_hash = compute_preprocess_config_hash(config, &coreml_model_path, seed)?;
    let cache_key = compute_cache_key(
        &dataset_hash,
        split_hash_b3,
        &config_hash,
        coreml_model_hash.as_ref().or(base_model_hash.as_ref()),
    );
    let preprocess_id = cache_key.to_hex();

    let cache_root = resolve_dataset_cache_root(config, artifacts_root, dataset_id, &dataset_hash)?;
    let cache_root_str = normalize_path_for_sorting(&cache_root);
    let artifact_dir = cache_root.join(&preprocess_id);
    fs::create_dir_all(&artifact_dir).map_err(|e| {
        AosError::Io(format!(
            "Failed to create preprocessing cache dir {}: {}",
            artifact_dir.display(),
            e
        ))
    })?;
    let artifact_dir = canonicalize_existing_path(&artifact_dir, "preprocess_cache_dir")?;

    let manifest_path = artifact_dir.join("preprocess_manifest.json");
    let data_path = artifact_dir.join("features.bin");
    let index_path = artifact_dir.join("features.index");

    let mut manifest = if manifest_path.exists() {
        let existing = load_manifest(&manifest_path)?;
        validate_manifest(
            &existing,
            &dataset_hash,
            split_hash_b3,
            &cache_key,
            &tokenizer_hash,
            &tokenizer_cfg_hash,
            &config_hash,
            coreml_model_hash.as_ref(),
            base_model_hash.as_ref(),
            config,
            contract,
        )?;
        existing
    } else {
        let now_ms = current_unix_ms();
        let manifest = PreprocessManifest {
            schema_version: PREPROCESS_MANIFEST_VERSION.to_string(),
            contract_version: PREPROCESSED_EXAMPLE_SCHEMA_VERSION.to_string(),
            training_contract_version: Some(contract.contract_version.clone()),
            pad_token_id: Some(contract.pad_token_id),
            ignore_index: Some(contract.ignore_index),
            preprocess_id: preprocess_id.clone(),
            cache_key_b3: cache_key.to_hex(),
            preprocessing_config_hash_b3: config_hash.to_hex(),
            dataset_hash_b3: dataset_hash.to_hex(),
            split_hash_b3: split_hash_b3.map(|s| s.to_string()),
            dataset_id: dataset_id.map(|id| id.to_string()),
            base_model_hash_b3: base_model_hash.as_ref().map(|hash| hash.to_hex()),
            coreml_model_hash_b3: coreml_model_hash.as_ref().map(|hash| hash.to_hex()),
            tokenizer_hash_b3: tokenizer_hash.to_hex(),
            tokenizer_cfg_hash_b3: tokenizer_cfg_hash.to_hex(),
            output_feature: config.output_feature.as_str().to_string(),
            layer_key: config.layer_key.clone(),
            max_seq_len: config.max_seq_len,
            batch_size: config.batch_size,
            compression: compression.as_str().to_string(),
            feature_dtype: PREPROCESSED_FEATURE_DTYPE_F32.to_string(),
            backend: PREPROCESSED_FEATURE_BACKEND_COREML.to_string(),
            seed,
            training_seed,
            example_count: examples.len(),
            processed_count: 0,
            produced_at_unix_ms: now_ms,
            cache_root: cache_root_str.clone(),
            coreml_model_path: Some(normalize_path_for_sorting(&coreml_model_path)),
        };
        store_manifest(&manifest_path, &manifest)?;
        manifest
    };

    if manifest.example_count != examples.len() {
        return Err(AosError::Training(format!(
            "Preprocessing cache example count mismatch: {} != {}",
            manifest.example_count,
            examples.len()
        )));
    }

    let mut index_entries = load_index_entries(&index_path)?;
    let data_blob = if index_entries.is_empty() {
        Vec::new()
    } else {
        fs::read(&data_path).map_err(|e| {
            AosError::Io(format!(
                "Failed to read preprocessing cache data {}: {}",
                data_path.display(),
                e
            ))
        })?
    };

    let (cached_entries, data_blob) = validate_index_entries(
        index_entries,
        &data_blob,
        compression,
        &index_path,
        &data_path,
        &mut manifest,
        &manifest_path,
    )?;
    let cached_examples = cached_entries.len();

    let mut preprocessed_examples = Vec::with_capacity(examples.len());
    if cached_examples > 0 {
        for (entry, example) in cached_entries.iter().zip(examples.iter()) {
            let features = read_features_from_blob(&data_blob, entry, compression, hidden_dim)?;
            let feature_hash = hash_features(&features);
            if feature_hash != entry.feature_hash {
                return Err(AosError::Training(format!(
                    "Preprocessing cache feature hash mismatch at {}",
                    entry.example_index
                )));
            }
            preprocessed_examples.push(build_preprocessed_example(
                example,
                features,
                entry.feature_shape.clone(),
                feature_hash,
            ));
        }
    }

    let start = Instant::now();
    let mut backend = select_backend(&coreml_model_path, hidden_dim)?;
    let mut processed_examples = 0usize;

    let mut data_writer = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&data_path)
        .map_err(|e| {
            AosError::Io(format!(
                "Failed to open preprocessing data file {}: {}",
                data_path.display(),
                e
            ))
        })?;
    let mut index_writer = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&index_path)
        .map_err(|e| {
            AosError::Io(format!(
                "Failed to open preprocessing index file {}: {}",
                index_path.display(),
                e
            ))
        })?;

    for (idx, example) in examples.iter().enumerate().skip(cached_examples) {
        if config.max_seq_len > 0 && example.input_tokens.len() > config.max_seq_len as usize {
            return Err(AosError::Training(format!(
                "Example {} input exceeds preprocessing max_seq_len: {} > {}",
                idx,
                example.input_tokens.len(),
                config.max_seq_len
            )));
        }

        let raw = backend.encode_tokens(&example.input_tokens, &output_name)?;
        let (features, shape) = extract_features(&raw, hidden_dim, config.output_feature)?;
        let feature_hash = hash_features(&features);

        let (element_count, scale, bytes_written) = match compression {
            PreprocessCompression::None => {
                let bytes = write_f32_tensor(&mut data_writer, &features)?;
                (features.len() as u32, None, bytes)
            }
            PreprocessCompression::Q15 => {
                let (quantized, scale) = quantize_q15(&features);
                let bytes = write_i16_tensor(&mut data_writer, &quantized)?;
                (quantized.len() as u32, Some(scale), bytes)
            }
        };

        let entry = FeatureIndexEntry {
            example_index: idx,
            offset_bytes: current_offset(&data_path)? - bytes_written,
            element_count,
            feature_shape: shape.clone(),
            feature_hash: feature_hash.clone(),
            scale,
        };
        append_index_entry(&mut index_writer, &entry)?;

        preprocessed_examples.push(build_preprocessed_example(
            example,
            features,
            shape,
            feature_hash,
        ));

        processed_examples += 1;
        manifest.processed_count += 1;
        if manifest.processed_count == manifest.example_count {
            manifest.produced_at_unix_ms = current_unix_ms();
        }
        store_manifest(&manifest_path, &manifest)?;
    }

    let elapsed_ms = start.elapsed().as_millis();
    if processed_examples > 0 {
        info!(
            processed = processed_examples,
            cache_dir = %artifact_dir.display(),
            "Preprocessing complete"
        );
    }

    let stats = PreprocessStats {
        backend: PREPROCESSED_FEATURE_BACKEND_COREML.to_string(),
        cache_hit: cached_examples == examples.len(),
        cached_examples,
        processed_examples,
        elapsed_ms,
        cache_dir: artifact_dir.display().to_string(),
        preprocess_id: preprocess_id.clone(),
        cache_key: cache_key.to_hex(),
        coreml_model_hash: coreml_model_hash.as_ref().map(|hash| hash.to_hex()),
        produced_at_unix_ms: manifest.produced_at_unix_ms,
    };

    Ok(PreprocessResult {
        examples: preprocessed_examples,
        stats,
    })
}

/// Inspect preprocessing cache state without mutating or reprocessing data.
///
/// Returns a compatibility summary that can be used to decide whether
/// preprocessing needs to run again for the given dataset/model/config tuple.
#[allow(clippy::too_many_arguments)]
pub fn inspect_preprocess_cache(
    examples: &[TrainingExampleV1],
    contract: &TrainingDataContractConfig,
    config: &PreprocessingConfig,
    hidden_dim: usize,
    vocab_size: usize,
    base_model_path: &Path,
    dataset_id: Option<&str>,
    artifacts_root: Option<&Path>,
    split_hash_b3: Option<&str>,
    training_seed: u64,
) -> Result<PreprocessCacheStatus> {
    if examples.is_empty() {
        return Err(AosError::Training(
            "preprocessing requested with empty dataset".to_string(),
        ));
    }
    if hidden_dim == 0 {
        return Err(AosError::Training(
            "preprocessing hidden_dim must be greater than zero".to_string(),
        ));
    }
    if !config.enabled {
        return Err(AosError::Config(
            "preprocessing requested but config.enabled=false".to_string(),
        ));
    }

    validate_training_examples(examples, vocab_size, contract).map_err(|e| {
        AosError::Training(format!(
            "Training example contract validation failed before preprocessing: {}",
            e
        ))
    })?;

    let base_model_path = canonicalize_existing_path(base_model_path, "base_model_path")?;
    let model_config = ModelConfig::from_config_json(&base_model_path)?;
    if model_config.hidden_size != hidden_dim {
        return Err(AosError::Training(format!(
            "Preprocessing hidden_dim mismatch: model hidden_size={} config hidden_dim={}",
            model_config.hidden_size, hidden_dim
        )));
    }

    let (tokenizer_hash, tokenizer_cfg_hash) = compute_tokenizer_hashes(&base_model_path)?;
    let dataset_hash = compute_dataset_hash(examples);
    let seed = config.seed.unwrap_or(training_seed);
    let compression = config.compression.unwrap_or(PreprocessCompression::None);
    let output_name = resolve_output_name(config);
    let coreml_model_path = resolve_coreml_model_path(config)?;
    let coreml_model_hash = Some(hash_coreml_model(&coreml_model_path)?);
    let base_model_hash = hash_optional_file(base_model_path.join("config.json"))?;
    let config_hash = compute_preprocess_config_hash(config, &coreml_model_path, seed)?;
    let cache_key = compute_cache_key(
        &dataset_hash,
        split_hash_b3,
        &config_hash,
        coreml_model_hash.as_ref().or(base_model_hash.as_ref()),
    );
    let preprocess_id = cache_key.to_hex();
    let cache_root = resolve_dataset_cache_root(config, artifacts_root, dataset_id, &dataset_hash)?;
    let manifest_path = cache_root
        .join(&preprocess_id)
        .join("preprocess_manifest.json");

    if !manifest_path.exists() {
        return Ok(PreprocessCacheStatus {
            preprocess_id,
            cache_key_b3: cache_key.to_hex(),
            cache_dir: normalize_path_for_sorting(&cache_root),
            manifest_hash_b3: B3Hash::zero().to_hex(),
            produced_at_unix_ms: None,
            feature_dtype: "unknown".to_string(),
            backend: PREPROCESSED_FEATURE_BACKEND_COREML.to_string(),
            compression: compression.as_str().to_string(),
            cache_hit: false,
            needs_reprocess: true,
            reasons: vec!["cache_miss".to_string()],
            dataset_hash_b3: dataset_hash.to_hex(),
            tokenizer_hash_b3: tokenizer_hash.to_hex(),
            tokenizer_cfg_hash_b3: tokenizer_cfg_hash.to_hex(),
            preprocessing_config_hash_b3: config_hash.to_hex(),
            coreml_model_hash_b3: coreml_model_hash.as_ref().map(|hash| hash.to_hex()),
            base_model_hash_b3: base_model_hash.as_ref().map(|hash| hash.to_hex()),
        });
    }

    let manifest = load_manifest(&manifest_path)?;
    let manifest_hash = hash_json_value(&manifest)?;
    let mut reasons = Vec::new();

    if manifest.dataset_hash_b3 != dataset_hash.to_hex() {
        reasons.push("dataset_hash_mismatch".to_string());
    }
    if manifest.tokenizer_hash_b3 != tokenizer_hash.to_hex() {
        reasons.push("tokenizer_hash_mismatch".to_string());
    }
    if manifest.tokenizer_cfg_hash_b3 != tokenizer_cfg_hash.to_hex() {
        reasons.push("tokenizer_cfg_hash_mismatch".to_string());
    }
    if manifest.preprocessing_config_hash_b3 != config_hash.to_hex() {
        reasons.push("preprocess_config_mismatch".to_string());
    }
    if manifest.coreml_model_hash_b3 != coreml_model_hash.as_ref().map(|hash| hash.to_hex()) {
        reasons.push("coreml_model_hash_mismatch".to_string());
    }
    if manifest.base_model_hash_b3 != base_model_hash.as_ref().map(|hash| hash.to_hex()) {
        reasons.push("base_model_hash_mismatch".to_string());
    }
    if manifest.split_hash_b3.as_deref() != split_hash_b3 {
        reasons.push("split_hash_mismatch".to_string());
    }
    if manifest.output_feature != output_name {
        reasons.push("output_feature_mismatch".to_string());
    }
    if manifest.layer_key != config.layer_key {
        reasons.push("layer_key_mismatch".to_string());
    }
    if manifest.max_seq_len != config.max_seq_len {
        reasons.push("max_seq_len_mismatch".to_string());
    }
    if manifest.batch_size != config.batch_size {
        reasons.push("batch_size_mismatch".to_string());
    }
    if manifest.compression != compression.as_str() {
        reasons.push("compression_mismatch".to_string());
    }
    if manifest.pad_token_id != Some(contract.pad_token_id) {
        reasons.push("pad_token_id_mismatch".to_string());
    }
    if manifest.ignore_index != Some(contract.ignore_index) {
        reasons.push("ignore_index_mismatch".to_string());
    }
    if manifest.seed != seed {
        reasons.push("seed_mismatch".to_string());
    }
    if manifest.processed_count < manifest.example_count {
        reasons.push("cache_incomplete".to_string());
    }

    let cache_hit = reasons.is_empty();

    Ok(PreprocessCacheStatus {
        preprocess_id: manifest.preprocess_id,
        cache_key_b3: manifest.cache_key_b3,
        cache_dir: normalize_path_for_sorting(&cache_root),
        manifest_hash_b3: manifest_hash.to_hex(),
        produced_at_unix_ms: Some(manifest.produced_at_unix_ms),
        feature_dtype: manifest.feature_dtype,
        backend: manifest.backend,
        compression: manifest.compression,
        cache_hit,
        needs_reprocess: !cache_hit,
        reasons,
        dataset_hash_b3: manifest.dataset_hash_b3,
        tokenizer_hash_b3: manifest.tokenizer_hash_b3,
        tokenizer_cfg_hash_b3: manifest.tokenizer_cfg_hash_b3,
        preprocessing_config_hash_b3: manifest.preprocessing_config_hash_b3,
        coreml_model_hash_b3: manifest.coreml_model_hash_b3,
        base_model_hash_b3: manifest.base_model_hash_b3,
    })
}

fn compute_dataset_hash(examples: &[TrainingExampleV1]) -> B3Hash {
    let mut hasher = Hasher::new();
    for example in examples {
        for token in &example.input_tokens {
            hasher.update(&token.to_le_bytes());
        }
        for token in &example.target_tokens {
            hasher.update(&token.to_le_bytes());
        }
        hasher.update(&example.attention_mask);
    }
    B3Hash::from_bytes(*hasher.finalize().as_bytes())
}

fn compute_tokenizer_hashes(model_path: &Path) -> Result<(B3Hash, B3Hash)> {
    let tokenizer_path = model_path.join("tokenizer.json");
    if !tokenizer_path.exists() {
        return Err(AosError::Training(format!(
            "Tokenizer file missing at {}",
            tokenizer_path.display()
        )));
    }
    let tokenizer_hash = B3Hash::hash_file(&tokenizer_path).map_err(|e| {
        AosError::Io(format!(
            "Failed to hash tokenizer file {}: {}",
            tokenizer_path.display(),
            e
        ))
    })?;

    let tokenizer_cfg_path = model_path.join("tokenizer_config.json");
    let tokenizer_cfg_hash = if tokenizer_cfg_path.exists() {
        B3Hash::hash_file(&tokenizer_cfg_path).map_err(|e| {
            AosError::Io(format!(
                "Failed to hash tokenizer_config file {}: {}",
                tokenizer_cfg_path.display(),
                e
            ))
        })?
    } else {
        warn!(
            path = %tokenizer_cfg_path.display(),
            "tokenizer_config.json missing; using zero hash"
        );
        B3Hash::zero()
    };

    Ok((tokenizer_hash, tokenizer_cfg_hash))
}

fn hash_optional_file(path: PathBuf) -> Result<Option<B3Hash>> {
    if !path.exists() {
        return Ok(None);
    }
    let hash = B3Hash::hash_file(&path)
        .map_err(|e| AosError::Io(format!("Failed to hash file {}: {}", path.display(), e)))?;
    Ok(Some(hash))
}

fn hash_coreml_model(path: &Path) -> Result<B3Hash> {
    let hash_path = if path.is_dir() {
        let manifest = path.join("Manifest.json");
        if manifest.exists() {
            manifest
        } else {
            let weights = path.join("Data/com.apple.CoreML/weights/weight.bin");
            if weights.exists() {
                weights
            } else {
                return Err(AosError::Training(format!(
                    "CoreML model missing Manifest.json in {}",
                    path.display()
                )));
            }
        }
    } else {
        path.to_path_buf()
    };
    B3Hash::hash_file(&hash_path).map_err(|e| {
        AosError::Io(format!(
            "Failed to hash CoreML model {}: {}",
            hash_path.display(),
            e
        ))
    })
}

fn resolve_output_name(config: &PreprocessingConfig) -> String {
    if let Some(ref layer_key) = config.layer_key {
        return layer_key.clone();
    }
    match config.output_feature {
        PreprocessOutputFeature::Embedding => DEFAULT_COREML_EMBEDDING_OUTPUT.to_string(),
        PreprocessOutputFeature::HiddenStateLast | PreprocessOutputFeature::Pooled => {
            DEFAULT_COREML_HIDDEN_OUTPUT.to_string()
        }
    }
}

fn compute_preprocess_config_hash(
    config: &PreprocessingConfig,
    coreml_model_path: &Path,
    seed: u64,
) -> Result<B3Hash> {
    let payload = PreprocessConfigHashPayload {
        enabled: config.enabled,
        coreml_model_id: config.coreml_model_id.clone(),
        coreml_model_path: Some(normalize_path_for_sorting(coreml_model_path)),
        output_feature: config.output_feature.as_str().to_string(),
        layer_key: config.layer_key.clone(),
        max_seq_len: config.max_seq_len,
        batch_size: config.batch_size,
        compression: config
            .compression
            .unwrap_or(PreprocessCompression::None)
            .as_str()
            .to_string(),
        seed,
    };
    let bytes = serde_json::to_vec(&payload).map_err(AosError::Serialization)?;
    Ok(B3Hash::hash(&bytes))
}

fn hash_json_value<T: Serialize>(value: &T) -> Result<B3Hash> {
    let bytes = serde_json::to_vec(value).map_err(AosError::Serialization)?;
    Ok(B3Hash::hash(&bytes))
}

fn compute_cache_key(
    dataset_hash: &B3Hash,
    split_hash_b3: Option<&str>,
    config_hash: &B3Hash,
    model_hash: Option<&B3Hash>,
) -> B3Hash {
    let mut hasher = Hasher::new();
    hasher.update(b"preprocess_cache_v1");
    hasher.update(dataset_hash.as_bytes());
    if let Some(split) = split_hash_b3 {
        hasher.update(split.as_bytes());
    }
    if let Some(hash) = model_hash {
        hasher.update(hash.as_bytes());
    }
    hasher.update(config_hash.as_bytes());
    hasher.update(PREPROCESSED_EXAMPLE_SCHEMA_VERSION.as_bytes());
    B3Hash::from_bytes(*hasher.finalize().as_bytes())
}

fn resolve_dataset_cache_root(
    config: &PreprocessingConfig,
    artifacts_root: Option<&Path>,
    dataset_id: Option<&str>,
    dataset_hash: &B3Hash,
) -> Result<PathBuf> {
    let root = match config.cache_dir.as_ref() {
        Some(path) => path.clone(),
        None => artifacts_root
            .map(|root| root.join("datasets"))
            .unwrap_or_else(|| PlatformUtils::aos_artifacts_dir().join("datasets")),
    };
    let dataset_dir = dataset_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| format!("dataset-{}", dataset_hash.to_hex()));
    let preprocess_root = root.join(dataset_dir).join("preprocessed");
    ensure_temp_dir(&preprocess_root).map_err(|e| {
        AosError::Io(format!(
            "Failed to create preprocessing cache root {}: {}",
            preprocess_root.display(),
            e
        ))
    })
}

fn resolve_coreml_model_path(config: &PreprocessingConfig) -> Result<PathBuf> {
    if let Some(ref path) = config.coreml_model_path {
        return canonicalize_existing_path(path, "coreml_model_path");
    }
    if let Some(ref model_id) = config.coreml_model_id {
        let location = resolve_base_model_location(Some(model_id), None, true)?;
        return canonicalize_existing_path(&location.full_path, "coreml_model_id");
    }
    Err(AosError::Config(
        "preprocessing enabled but coreml_model_path/coreml_model_id missing".to_string(),
    ))
}

fn canonicalize_existing_path(path: &Path, label: &str) -> Result<PathBuf> {
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| AosError::Io(format!("Failed to resolve cwd: {}", e)))?
            .join(path)
    };
    candidate.canonicalize().map_err(|e| {
        AosError::Io(format!(
            "Failed to canonicalize {} {}: {}",
            label,
            candidate.display(),
            e
        ))
    })
}

fn current_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn load_manifest(path: &Path) -> Result<PreprocessManifest> {
    let data = fs::read(path).map_err(|e| {
        AosError::Io(format!(
            "Failed to read preprocessing manifest {}: {}",
            path.display(),
            e
        ))
    })?;
    serde_json::from_slice(&data).map_err(|e| {
        AosError::Training(format!(
            "Failed to parse preprocessing manifest {}: {}",
            path.display(),
            e
        ))
    })
}

fn store_manifest(path: &Path, manifest: &PreprocessManifest) -> Result<()> {
    let data = serde_json::to_vec_pretty(manifest).map_err(AosError::Serialization)?;
    write_atomic(path, &data)
}

#[allow(clippy::too_many_arguments)]
fn validate_manifest(
    manifest: &PreprocessManifest,
    dataset_hash: &B3Hash,
    split_hash_b3: Option<&str>,
    cache_key: &B3Hash,
    tokenizer_hash: &B3Hash,
    tokenizer_cfg_hash: &B3Hash,
    config_hash: &B3Hash,
    coreml_model_hash: Option<&B3Hash>,
    base_model_hash: Option<&B3Hash>,
    config: &PreprocessingConfig,
    contract: &TrainingDataContractConfig,
) -> Result<()> {
    if manifest.schema_version != PREPROCESS_MANIFEST_VERSION {
        return Err(AosError::Training(format!(
            "Preprocessing cache manifest version mismatch: {} != {}",
            manifest.schema_version, PREPROCESS_MANIFEST_VERSION
        )));
    }
    if manifest.contract_version != PREPROCESSED_EXAMPLE_SCHEMA_VERSION {
        return Err(AosError::Training(
            "Preprocessing contract version mismatch".to_string(),
        ));
    }
    if manifest.training_contract_version.as_deref() != Some(contract.contract_version.as_str()) {
        return Err(AosError::Training(
            "Preprocessing training contract version mismatch".to_string(),
        ));
    }
    if manifest.pad_token_id != Some(contract.pad_token_id) {
        return Err(AosError::Training(
            "Preprocessing pad_token_id mismatch".to_string(),
        ));
    }
    if manifest.ignore_index != Some(contract.ignore_index) {
        return Err(AosError::Training(
            "Preprocessing ignore_index mismatch".to_string(),
        ));
    }
    if manifest.dataset_hash_b3 != dataset_hash.to_hex() {
        return Err(AosError::Training(
            "Preprocessing cache dataset hash mismatch".to_string(),
        ));
    }
    if manifest.split_hash_b3.as_deref() != split_hash_b3 {
        return Err(AosError::Training(
            "Preprocessing cache split hash mismatch".to_string(),
        ));
    }
    if manifest.cache_key_b3 != cache_key.to_hex() {
        return Err(AosError::Training(
            "Preprocessing cache key mismatch".to_string(),
        ));
    }
    if manifest.preprocessing_config_hash_b3 != config_hash.to_hex() {
        return Err(AosError::Training(
            "Preprocessing cache config hash mismatch".to_string(),
        ));
    }
    if manifest.tokenizer_hash_b3 != tokenizer_hash.to_hex()
        || manifest.tokenizer_cfg_hash_b3 != tokenizer_cfg_hash.to_hex()
    {
        return Err(AosError::Training(
            "Preprocessing cache tokenizer mismatch".to_string(),
        ));
    }
    if manifest.coreml_model_hash_b3 != coreml_model_hash.as_ref().map(|hash| hash.to_hex()) {
        return Err(AosError::Training(
            "Preprocessing cache CoreML model hash mismatch".to_string(),
        ));
    }
    if manifest.base_model_hash_b3 != base_model_hash.as_ref().map(|hash| hash.to_hex()) {
        return Err(AosError::Training(
            "Preprocessing cache base model hash mismatch".to_string(),
        ));
    }
    if manifest.output_feature != config.output_feature.as_str() {
        return Err(AosError::Training(
            "Preprocessing cache output_feature mismatch".to_string(),
        ));
    }
    if manifest.layer_key != config.layer_key {
        return Err(AosError::Training(
            "Preprocessing cache layer_key mismatch".to_string(),
        ));
    }
    Ok(())
}

fn load_index_entries(path: &Path) -> Result<Vec<FeatureIndexEntry>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = fs::File::open(path).map_err(|e| {
        AosError::Io(format!(
            "Failed to open preprocessing index {}: {}",
            path.display(),
            e
        ))
    })?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();
    for (idx, line) in reader.lines().enumerate() {
        let line = match line {
            Ok(line) => line,
            Err(e) => {
                warn!(error = %e, "Failed reading preprocessing index line; stopping");
                break;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<FeatureIndexEntry>(&line) {
            Ok(entry) => entries.push(entry),
            Err(e) => {
                warn!(
                    index_line = idx,
                    error = %e,
                    "Failed parsing preprocessing index line; stopping"
                );
                break;
            }
        }
    }
    Ok(entries)
}

fn validate_index_entries(
    entries: Vec<FeatureIndexEntry>,
    data_blob: &[u8],
    compression: PreprocessCompression,
    index_path: &Path,
    data_path: &Path,
    manifest: &mut PreprocessManifest,
    manifest_path: &Path,
) -> Result<(Vec<FeatureIndexEntry>, Vec<u8>)> {
    if entries.is_empty() {
        return Ok((Vec::new(), data_blob.to_vec()));
    }
    let elem_size = match compression {
        PreprocessCompression::None => 4usize,
        PreprocessCompression::Q15 => 2usize,
    };

    let mut valid = Vec::new();
    let mut last_end = 0usize;
    for (idx, entry) in entries.into_iter().enumerate() {
        if entry.example_index != idx {
            return Err(AosError::Training(format!(
                "Preprocessing cache index out of order at {}",
                idx
            )));
        }
        let expected_count = entry
            .feature_shape
            .iter()
            .fold(1u64, |acc, v| acc.saturating_mul(*v as u64));
        if expected_count != entry.element_count as u64 {
            return Err(AosError::Training(format!(
                "Preprocessing cache feature shape mismatch at {}",
                idx
            )));
        }
        let start = entry.offset_bytes as usize;
        let len = entry.element_count as usize * elem_size;
        let end = start.saturating_add(len);
        if end > data_blob.len() {
            warn!(
                index = idx,
                "Preprocessing cache truncated; truncating index and data"
            );
            break;
        }
        last_end = end;
        valid.push(entry);
    }

    if manifest.processed_count != valid.len() {
        manifest.processed_count = valid.len();
        store_manifest(manifest_path, manifest)?;
    }

    if valid.len() < manifest.example_count {
        if let Ok(mut file) = fs::OpenOptions::new().write(true).open(data_path) {
            let _ = file.set_len(last_end as u64);
        }
        rewrite_index_entries(index_path, &valid)?;
    }

    Ok((valid, data_blob.to_vec()))
}

fn rewrite_index_entries(path: &Path, entries: &[FeatureIndexEntry]) -> Result<()> {
    let mut buf = Vec::new();
    for entry in entries {
        let line = serde_json::to_string(entry).map_err(AosError::Serialization)?;
        buf.extend_from_slice(line.as_bytes());
        buf.push(b'\n');
    }
    write_atomic(path, &buf)
}

fn append_index_entry(writer: &mut dyn Write, entry: &FeatureIndexEntry) -> Result<()> {
    let line = serde_json::to_string(entry).map_err(AosError::Serialization)?;
    writer
        .write_all(line.as_bytes())
        .map_err(|e| AosError::Io(format!("Failed to write preprocessing index entry: {}", e)))?;
    writer.write_all(b"\n").map_err(|e| {
        AosError::Io(format!(
            "Failed to write preprocessing index newline: {}",
            e
        ))
    })?;
    Ok(())
}

fn read_features_from_blob(
    data_blob: &[u8],
    entry: &FeatureIndexEntry,
    compression: PreprocessCompression,
    hidden_dim: usize,
) -> Result<Vec<f32>> {
    let elem_size = match compression {
        PreprocessCompression::None => 4usize,
        PreprocessCompression::Q15 => 2usize,
    };
    let start = entry.offset_bytes as usize;
    let len_bytes = entry.element_count as usize * elem_size;
    let end = start + len_bytes;
    if end > data_blob.len() {
        return Err(AosError::Training(format!(
            "Preprocessing cache truncated: {} < {} bytes",
            data_blob.len(),
            end
        )));
    }
    let slice = &data_blob[start..end];
    let features = match compression {
        PreprocessCompression::None => slice
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect::<Vec<_>>(),
        PreprocessCompression::Q15 => {
            let scale = entry.scale.ok_or_else(|| {
                AosError::Training("Missing scale for Q15 preprocessing entry".to_string())
            })?;
            slice
                .chunks_exact(2)
                .map(|chunk| {
                    let q = i16::from_le_bytes([chunk[0], chunk[1]]) as f32;
                    (q / Q15_DENOM) * scale
                })
                .collect::<Vec<_>>()
        }
    };
    if !features.is_empty() && features.len() != hidden_dim {
        return Err(AosError::Training(format!(
            "Cached feature length mismatch: {} != {}",
            features.len(),
            hidden_dim
        )));
    }
    Ok(features)
}

fn write_f32_tensor(writer: &mut dyn Write, values: &[f32]) -> Result<u64> {
    let mut bytes_written = 0u64;
    for value in values {
        writer
            .write_all(&value.to_le_bytes())
            .map_err(|e| AosError::Io(format!("Failed to write preprocessing data: {}", e)))?;
        bytes_written += 4;
    }
    Ok(bytes_written)
}

fn write_i16_tensor(writer: &mut dyn Write, values: &[i16]) -> Result<u64> {
    let mut bytes_written = 0u64;
    for value in values {
        writer
            .write_all(&value.to_le_bytes())
            .map_err(|e| AosError::Io(format!("Failed to write preprocessing data: {}", e)))?;
        bytes_written += 2;
    }
    Ok(bytes_written)
}

fn current_offset(path: &Path) -> Result<u64> {
    let metadata = fs::metadata(path).map_err(|e| {
        AosError::Io(format!(
            "Failed to read preprocessing data metadata {}: {}",
            path.display(),
            e
        ))
    })?;
    Ok(metadata.len())
}

fn hash_features(features: &[f32]) -> String {
    let mut hasher = Hasher::new();
    for value in features {
        hasher.update(&value.to_le_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn build_preprocessed_example(
    example: &TrainingExampleV1,
    features: Vec<f32>,
    feature_shape: Vec<u32>,
    feature_hash: String,
) -> PreprocessedExampleV1 {
    PreprocessedExampleV1 {
        schema_version: PREPROCESSED_EXAMPLE_SCHEMA_VERSION.to_string(),
        input_tokens: example.input_tokens.clone(),
        target_tokens: example.target_tokens.clone(),
        attention_mask: example.attention_mask.clone(),
        features,
        feature_shape,
        feature_dtype: PREPROCESSED_FEATURE_DTYPE_F32.to_string(),
        backend: PREPROCESSED_FEATURE_BACKEND_COREML.to_string(),
        feature_hash,
    }
}

fn extract_features(
    raw: &[f32],
    hidden_dim: usize,
    output_feature: PreprocessOutputFeature,
) -> Result<(Vec<f32>, Vec<u32>)> {
    if raw.len() == hidden_dim {
        return Ok((raw.to_vec(), vec![hidden_dim as u32]));
    }
    if !raw.len().is_multiple_of(hidden_dim) {
        return Err(AosError::Training(format!(
            "Preprocessing output size {} not divisible by hidden_dim {}",
            raw.len(),
            hidden_dim
        )));
    }
    let seq_len = raw.len() / hidden_dim;
    if seq_len == 0 {
        return Err(AosError::Training(
            "Preprocessing produced empty sequence".to_string(),
        ));
    }
    let features = match output_feature {
        PreprocessOutputFeature::Embedding => {
            let offset = 0;
            raw[offset..offset + hidden_dim].to_vec()
        }
        PreprocessOutputFeature::HiddenStateLast => {
            let offset = (seq_len - 1) * hidden_dim;
            raw[offset..offset + hidden_dim].to_vec()
        }
        PreprocessOutputFeature::Pooled => {
            let mut pooled = vec![0.0f32; hidden_dim];
            for t in 0..seq_len {
                let offset = t * hidden_dim;
                for i in 0..hidden_dim {
                    pooled[i] += raw[offset + i];
                }
            }
            let denom = seq_len as f32;
            for value in &mut pooled {
                *value /= denom;
            }
            pooled
        }
    };
    Ok((features, vec![hidden_dim as u32]))
}

fn quantize_q15(values: &[f32]) -> (Vec<i16>, f32) {
    let mut max_abs = 0.0f32;
    for value in values {
        max_abs = max_abs.max(value.abs());
    }
    let scale = if max_abs == 0.0 { 1.0 } else { max_abs };
    let mut quantized = Vec::with_capacity(values.len());
    for value in values {
        let normalized = if scale > 0.0 { value / scale } else { 0.0 };
        let q = (normalized * Q15_DENOM).round().clamp(Q15_MIN, Q15_MAX) as i16;
        quantized.push(q);
    }
    (quantized, scale)
}

fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            AosError::Io(format!(
                "Failed to create preprocessing directory {}: {}",
                parent.display(),
                e
            ))
        })?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, data).map_err(|e| {
        AosError::Io(format!(
            "Failed to write preprocessing temp file {}: {}",
            tmp.display(),
            e
        ))
    })?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        AosError::Io(format!(
            "Failed to rename preprocessing file {}: {}",
            path.display(),
            e
        ))
    })
}

enum PreprocessBackend {
    #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
    CoreML(CoreMLRunner),
    /// Marker variant to ensure the enum is never empty (for platforms without coreml).
    #[cfg(not(all(target_os = "macos", feature = "coreml-backend")))]
    _NeverConstructed(std::convert::Infallible),
}

impl PreprocessBackend {
    #[cfg_attr(
        not(all(target_os = "macos", feature = "coreml-backend")),
        allow(unused_variables)
    )]
    fn encode_tokens(&mut self, token_ids: &[u32], output_name: &str) -> Result<Vec<f32>> {
        match self {
            #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
            PreprocessBackend::CoreML(runner) => runner.encode_tokens(token_ids, output_name),
            #[cfg(not(all(target_os = "macos", feature = "coreml-backend")))]
            PreprocessBackend::_NeverConstructed(infallible) => match *infallible {},
        }
    }
}

#[cfg_attr(
    not(all(target_os = "macos", feature = "coreml-backend")),
    allow(unused_variables)
)]
fn select_backend(model_path: &Path, hidden_dim: usize) -> Result<PreprocessBackend> {
    #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
    {
        if adapteros_lora_kernel_coreml::is_coreml_available() {
            let runner = CoreMLRunner::new(model_path, hidden_dim)?;
            return Ok(PreprocessBackend::CoreML(runner));
        }
    }

    Err(AosError::Training(
        "CoreML preprocessing requires macOS + coreml-backend".to_string(),
    ))
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
struct CoreMLRunner {
    handle: *mut std::ffi::c_void,
    model_id: String,
    hidden_dim: usize,
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
impl CoreMLRunner {
    fn new(model_path: &Path, hidden_dim: usize) -> Result<Self> {
        adapteros_lora_kernel_coreml::init_coreml()?;
        let settings = crate::backend_factory::resolve_coreml_backend_settings();
        let mut compute_units = settings.compute_units;
        if matches!(
            compute_units,
            adapteros_lora_kernel_coreml::ComputeUnits::CpuAndGpu
                | adapteros_lora_kernel_coreml::ComputeUnits::All
        ) {
            let fallback = if settings.ane_available {
                adapteros_lora_kernel_coreml::ComputeUnits::CpuAndNeuralEngine
            } else {
                adapteros_lora_kernel_coreml::ComputeUnits::CpuOnly
            };
            warn!(
                requested = ?compute_units,
                fallback = ?fallback,
                "Preprocessing forcing deterministic CoreML compute units"
            );
            compute_units = fallback;
        }
        let compute_unit_int = match compute_units {
            adapteros_lora_kernel_coreml::ComputeUnits::CpuOnly => 0,
            adapteros_lora_kernel_coreml::ComputeUnits::CpuAndGpu => 1,
            adapteros_lora_kernel_coreml::ComputeUnits::CpuAndNeuralEngine => 2,
            adapteros_lora_kernel_coreml::ComputeUnits::All => 3,
        };

        let path_str = model_path.to_string_lossy();
        // SAFETY: CoreML FFI call to load model. path_str is a valid string, path_str.len()
        // matches the string length, compute_unit_int is a valid compute unit specifier.
        let handle = unsafe {
            adapteros_lora_kernel_coreml::ffi::coreml_load_model(
                path_str.as_ptr() as *const i8,
                path_str.len(),
                compute_unit_int,
            )
        };
        if handle.is_null() {
            let mut err_buf = [0i8; 512];
            // SAFETY: err_buf is a valid 512-byte array for receiving error messages.
            let len = unsafe {
                adapteros_lora_kernel_coreml::ffi::coreml_get_last_error(
                    err_buf.as_mut_ptr(),
                    err_buf.len(),
                )
            };
            let message = if len > 0 {
                // SAFETY: len is returned from FFI and bounded by err_buf.len().
                let bytes =
                    unsafe { std::slice::from_raw_parts(err_buf.as_ptr() as *const u8, len) };
                String::from_utf8_lossy(bytes).to_string()
            } else {
                "Unknown CoreML load error".to_string()
            };
            return Err(AosError::Kernel(format!(
                "Failed to load CoreML model: {}",
                message
            )));
        }
        // Track model memory for ANE metrics
        let model_id = format!("preprocess:{}", path_str);

        // Use exact on-disk size for memory tracking
        // Note: Actual ANE memory usage is often compressed/optimized, but
        // on-disk size is a reliable, conservative proxy for resource budgeting.
        let model_size = get_directory_size(model_path).unwrap_or(50 * 1024 * 1024);

        adapteros_lora_kernel_coreml::ffi::record_model_load(&model_id, model_size);

        Ok(Self {
            handle,
            model_id,
            hidden_dim,
        })
    }

    fn encode_tokens(&self, token_ids: &[u32], output_name: &str) -> Result<Vec<f32>> {
        if token_ids.is_empty() {
            return Err(AosError::Training(
                "CoreML preprocessing received empty input".to_string(),
            ));
        }
        let output_len = token_ids.len().saturating_mul(self.hidden_dim.max(1));
        if output_len == 0 {
            return Err(AosError::Training(
                "CoreML preprocessing output length is zero".to_string(),
            ));
        }
        let mut output = vec![0.0f32; output_len];
        // SAFETY: CoreML FFI inference call. handle is valid from load(), token_ids and output
        // are valid slices with correct lengths, output_name is a valid byte string. The FFI
        // writes results to output buffer within the specified bounds.
        let result = unsafe {
            adapteros_lora_kernel_coreml::ffi::coreml_run_inference_named_output(
                self.handle,
                token_ids.as_ptr(),
                token_ids.len(),
                output.as_mut_ptr(),
                output.len(),
                output_name.as_ptr() as *const i8,
                output_name.len(),
            )
        };
        if result < 0 {
            return Err(AosError::Kernel(format!(
                "CoreML inference failed with code {}",
                result
            )));
        }
        let copied = result as usize;
        if copied == 0 {
            return Err(AosError::Training(
                "CoreML preprocessing returned zero output".to_string(),
            ));
        }
        if copied < output.len() {
            output.truncate(copied);
        }
        Ok(output)
    }
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
impl Drop for CoreMLRunner {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // Untrack model memory
            adapteros_lora_kernel_coreml::ffi::record_model_unload(&self.model_id);

            // SAFETY: handle is valid from coreml_load_model and non-null.
            // Drop is called exactly once, ensuring no double-free.
            unsafe {
                adapteros_lora_kernel_coreml::ffi::coreml_unload_model(self.handle);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_config::ModelConfig;
    use adapteros_types::training::TRAINING_DATA_CONTRACT_VERSION;
    use std::fs;
    use std::path::PathBuf;

    fn temp_model_path() -> PathBuf {
        std::env::temp_dir().join("model.mlpackage")
    }

    fn base_config() -> PreprocessingConfig {
        PreprocessingConfig {
            enabled: true,
            coreml_model_path: Some(temp_model_path()),
            output_feature: PreprocessOutputFeature::Pooled,
            layer_key: None,
            max_seq_len: 128,
            batch_size: 4,
            compression: Some(PreprocessCompression::None),
            seed: Some(7),
            ..Default::default()
        }
    }

    #[test]
    fn preprocess_config_hash_changes_with_output_feature() {
        let mut cfg = base_config();
        let model_path = temp_model_path();
        let hash_a = compute_preprocess_config_hash(&cfg, &model_path, 7)
            .expect("failed to compute preprocessing config hash for base configuration - model path should be valid");

        cfg.output_feature = PreprocessOutputFeature::Embedding;
        let hash_b = compute_preprocess_config_hash(&cfg, &model_path, 7)
            .expect("failed to compute preprocessing config hash with Embedding feature - model path should be valid");

        assert_ne!(hash_a, hash_b);
    }

    #[test]
    fn cache_key_differs_with_split_hash() {
        let dataset_hash = B3Hash::hash(b"dataset");
        let config_hash = B3Hash::hash(b"config");
        let model_hash = B3Hash::hash(b"model");
        let key_a = compute_cache_key(&dataset_hash, None, &config_hash, Some(&model_hash));
        let key_b = compute_cache_key(
            &dataset_hash,
            Some("split"),
            &config_hash,
            Some(&model_hash),
        );

        assert_ne!(key_a, key_b);
        assert_eq!(
            key_a,
            compute_cache_key(&dataset_hash, None, &config_hash, Some(&model_hash))
        );
    }

    #[test]
    fn preprocessing_errors_on_hidden_dim_mismatch() {
        let temp = tempfile::tempdir().expect(
            "failed to create temporary directory for preprocessing test - check disk space",
        );
        let base_model_path = temp.path().join("model");
        fs::create_dir_all(&base_model_path)
            .expect("failed to create model directory - temp directory should be writable");

        let mut model_cfg = ModelConfig::dev_fixture();
        model_cfg.path = base_model_path.clone();
        model_cfg.hidden_size = 8;
        model_cfg.vocab_size = 16;
        let cfg_bytes = serde_json::to_vec_pretty(&model_cfg)
            .expect("failed to serialize model config - ModelConfig should always be serializable");
        fs::write(base_model_path.join("config.json"), cfg_bytes)
            .expect("failed to write config.json - model directory should be writable");
        fs::write(base_model_path.join("tokenizer.json"), b"{}")
            .expect("failed to write tokenizer.json - model directory should be writable");

        // CoreML model placeholder
        let coreml_path = base_model_path.join("preprocess.mlpackage");
        fs::write(&coreml_path, b"coreml")
            .expect("failed to write CoreML stub file - model directory should be writable");

        let cfg = PreprocessingConfig {
            enabled: true,
            coreml_model_path: Some(coreml_path),
            cache_dir: Some(temp.path().to_path_buf()),
            ..Default::default()
        };

        let contract = TrainingDataContractConfig::new(0, -1);
        let examples = vec![TrainingExampleV1::new(
            vec![1, 2],
            vec![3],
            vec![1, 1],
            ExampleMetadataV1::new("src", 0, "row-hash", "{}", 0),
        )];

        let result = preprocess_examples(
            &examples,
            &contract,
            &cfg,
            model_cfg.hidden_size + 1,
            model_cfg.vocab_size,
            &base_model_path,
            Some("ds"),
            None,
            None,
            0,
        );
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("hidden_dim mismatch"));
    }

    fn manifest_hash(manifest: &PreprocessManifest) -> B3Hash {
        let bytes = serde_json::to_vec(manifest).expect(
            "failed to serialize PreprocessManifest - manifest should always be serializable",
        );
        B3Hash::hash(&bytes)
    }

    fn base_manifest() -> PreprocessManifest {
        PreprocessManifest {
            schema_version: PREPROCESS_MANIFEST_VERSION.to_string(),
            contract_version: PREPROCESSED_EXAMPLE_SCHEMA_VERSION.to_string(),
            training_contract_version: Some(TRAINING_DATA_CONTRACT_VERSION.to_string()),
            pad_token_id: Some(0),
            ignore_index: Some(-1),
            preprocess_id: "preprocess".to_string(),
            cache_key_b3: B3Hash::hash(b"cache").to_hex(),
            preprocessing_config_hash_b3: B3Hash::hash(b"config").to_hex(),
            dataset_hash_b3: B3Hash::hash(b"dataset").to_hex(),
            split_hash_b3: None,
            dataset_id: Some("dataset".to_string()),
            base_model_hash_b3: Some(B3Hash::hash(b"base").to_hex()),
            coreml_model_hash_b3: Some(B3Hash::hash(b"coreml").to_hex()),
            tokenizer_hash_b3: B3Hash::hash(b"tokenizer").to_hex(),
            tokenizer_cfg_hash_b3: B3Hash::hash(b"tokenizer_cfg").to_hex(),
            output_feature: PreprocessOutputFeature::Pooled.as_str().to_string(),
            layer_key: None,
            max_seq_len: 128,
            batch_size: 4,
            compression: PreprocessCompression::None.as_str().to_string(),
            feature_dtype: PREPROCESSED_FEATURE_DTYPE_F32.to_string(),
            backend: PREPROCESSED_FEATURE_BACKEND_COREML.to_string(),
            seed: 7,
            training_seed: 11,
            example_count: 2,
            processed_count: 2,
            produced_at_unix_ms: 1234,
            cache_root: std::env::temp_dir()
                .join("cache")
                .to_string_lossy()
                .to_string(),
            coreml_model_path: Some(temp_model_path().to_string_lossy().to_string()),
        }
    }

    #[test]
    fn manifest_hash_changes_with_processed_count() {
        let mut manifest = base_manifest();
        let hash_a = manifest_hash(&manifest);
        manifest.processed_count = 1;
        let hash_b = manifest_hash(&manifest);
        assert_ne!(hash_a, hash_b);
    }

    #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
    #[test]
    fn preprocess_smoke_coreml_cache_deterministic() {
        if !adapteros_lora_kernel_coreml::is_coreml_available() {
            return;
        }
        let model_path = match std::env::var("AOS_MODEL_PATH") {
            Ok(path) => PathBuf::from(path),
            Err(_) => return,
        };
        let coreml_model_path = match std::env::var("AOS_COREML_PREPROCESS_MODEL") {
            Ok(path) => PathBuf::from(path),
            Err(_) => return,
        };

        let model_config = match ModelConfig::from_config_json(&model_path) {
            Ok(cfg) => cfg,
            Err(_) => return,
        };

        let temp_dir = tempfile::tempdir().expect(
            "failed to create temporary directory for CoreML preprocessing test - check disk space",
        );
        let cfg = PreprocessingConfig {
            enabled: true,
            coreml_model_path: Some(coreml_model_path),
            output_feature: PreprocessOutputFeature::HiddenStateLast,
            cache_dir: Some(temp_dir.path().to_path_buf()),
            ..Default::default()
        };

        let contract = TrainingDataContractConfig::new(0, -1);
        let examples = vec![
            TrainingExampleV1::new(
                vec![1, 2, 3, 4],
                vec![5, 6],
                TrainingExampleV1::attention_mask_from_tokens(&[1, 2, 3, 4], 0),
                ExampleMetadataV1::new("test", 0, "row-hash-0", "{}", 0),
            ),
            TrainingExampleV1::new(
                vec![7, 8, 9, 10],
                vec![11, 12],
                TrainingExampleV1::attention_mask_from_tokens(&[7, 8, 9, 10], 0),
                ExampleMetadataV1::new("test", 1, "row-hash-1", "{}", 0),
            ),
        ];

        let first = preprocess_examples(
            &examples,
            &contract,
            &cfg,
            model_config.hidden_size,
            model_config.vocab_size,
            &model_path,
            None,
            None,
            None,
            0,
        )
        .expect("first preprocessing run should succeed with valid model and config");
        let second = preprocess_examples(
            &examples,
            &contract,
            &cfg,
            model_config.hidden_size,
            model_config.vocab_size,
            &model_path,
            None,
            None,
            None,
            0,
        )
        .expect("second preprocessing run should succeed and use cached features from first run");

        assert_eq!(first.stats.cache_key, second.stats.cache_key);
        assert_eq!(
            first.examples[0].feature_hash,
            second.examples[0].feature_hash
        );
    }
}
