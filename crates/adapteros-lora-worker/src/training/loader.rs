//! Dataset manifest loader for DIR training
//!
//! Supports positive/negative weighted JSONL files, converting them into `TrainingExample`
//! instances encoded with the Qwen tokenizer.

use super::limits::DatasetSizeLimits;
use crate::tokenizer::QwenTokenizer;
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_secure_fs::path_policy::{canonicalize_strict, canonicalize_strict_in_allowed_roots};
use adapteros_types::training::{
    provenance_from_map, validate_training_examples, ExampleMetadataV1, TrainingDataContractConfig,
    TrainingExampleV1, TRAINING_DATA_CONTRACT_VERSION,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use tracing::warn;

// Locked framing constants (Plan 4).
const MAX_INPUT_TOKENS: usize = 256;
const MAX_TARGET_TOKENS: usize = 128;
const STRIDE_TOKENS: usize = 256;
const SCHEMA_SUPERVISED: &str = "supervised";
const SCHEMA_RAW_CONTINUATION: &str = "raw_continuation_v1";

/// Dataset manifest describing positive/negative inputs.
#[derive(Debug, Deserialize)]
pub struct DatasetManifest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    pub training_contract_version: String,
    pub entries: Vec<DatasetEntry>,
    #[serde(default)]
    pub provenance: Option<Value>,
}

/// Manifest entry pointing at an input file.
#[derive(Debug, Deserialize)]
pub struct DatasetEntry {
    pub path: String,
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(default = "default_entry_weight")]
    pub weight: f32,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

/// Load training examples from a manifest using the provided tokenizer.
pub fn load_examples_from_manifest<P: AsRef<Path>>(
    manifest_path: P,
    tokenizer: &QwenTokenizer,
) -> Result<Vec<TrainingExampleV1>> {
    let pad_token_id = tokenizer.pad_token_id().ok_or_else(|| {
        AosError::Training("Tokenizer missing pad_token_id for dataset manifest".to_string())
    })?;
    let vocab_size = tokenizer.vocab_size(true);
    load_examples_with_encoder(manifest_path, pad_token_id, vocab_size, |text| {
        tokenizer.encode(text)
    })
}

/// Load training examples using a caller-provided encoding closure (useful for testing).
pub fn load_examples_with_encoder<P, F>(
    manifest_path: P,
    pad_token_id: u32,
    vocab_size: usize,
    encoder: F,
) -> Result<Vec<TrainingExampleV1>>
where
    P: AsRef<Path>,
    F: Fn(&str) -> Result<Vec<u32>>,
{
    let manifest_path = canonicalize_strict(manifest_path.as_ref())?;
    let manifest_dir = manifest_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    let manifest_str = fs::read_to_string(&manifest_path).map_err(|e| {
        AosError::Training(format!(
            "Failed to read dataset manifest {}: {}",
            manifest_path.display(),
            e
        ))
    })?;
    let manifest: DatasetManifest = serde_json::from_str(&manifest_str).map_err(|e| {
        AosError::Training(format!(
            "Failed to parse dataset manifest {}: {}",
            manifest_path.display(),
            e
        ))
    })?;
    if manifest.training_contract_version != TRAINING_DATA_CONTRACT_VERSION {
        return Err(AosError::Training(format!(
            "Dataset manifest contract version mismatch: expected {}, got {}",
            TRAINING_DATA_CONTRACT_VERSION, manifest.training_contract_version
        )));
    }

    let limits = DatasetSizeLimits::from_env();
    if manifest.entries.len() > limits.max_files {
        return Err(AosError::Training(format!(
            "Dataset manifest exceeds file limit: {} > {}",
            manifest.entries.len(),
            limits.max_files
        )));
    }

    let mut all_examples = Vec::new();
    let mut total_tokens: u64 = 0;
    let mut total_bytes: u64 = 0;
    let created_at_unix_ms = chrono::Utc::now().timestamp_millis() as u64;
    let mut schema_mode: Option<&'static str> = None;

    for entry in manifest.entries.iter() {
        if entry.format != "jsonl" {
            return Err(AosError::Training(format!(
                "Unsupported dataset format '{}' in {}",
                entry.format, entry.path
            )));
        }

        let entry_candidate = if Path::new(&entry.path).is_absolute() {
            PathBuf::from(&entry.path)
        } else {
            manifest_dir.join(&entry.path)
        };
        let allowed_roots = [manifest_dir.clone()];
        let entry_path = canonicalize_strict_in_allowed_roots(&entry_candidate, &allowed_roots)
            .map_err(|e| AosError::Training(format!("Dataset entry path rejected: {}", e)))?;

        let entry_size = fs::metadata(&entry_path).map_err(|e| {
            AosError::Training(format!(
                "Failed to read dataset entry metadata {}: {}",
                entry_path.display(),
                e
            ))
        })?;
        total_bytes = total_bytes.saturating_add(entry_size.len());
        if total_bytes > limits.max_total_bytes {
            return Err(AosError::Training(format!(
                "Dataset total size exceeds limit: {} > {} bytes",
                total_bytes, limits.max_total_bytes
            )));
        }
        let file = fs::File::open(&entry_path).map_err(|e| {
            AosError::Training(format!(
                "Failed to open dataset entry {}: {}",
                entry_path.display(),
                e
            ))
        })?;
        let reader = BufReader::new(file);

        let entry_weight = entry.weight;
        if entry_weight.abs() < f32::EPSILON {
            continue;
        }

        for (line_idx, line) in reader.lines().enumerate() {
            let line_number = line_idx + 1;
            let line = line.map_err(|e| {
                AosError::Training(format!(
                    "Failed to read line {} in {}: {}",
                    line_number,
                    entry_path.display(),
                    e
                ))
            })?;

            if line.trim().is_empty() {
                return Err(AosError::Training(format!(
                    "Empty JSONL line {} in {}",
                    line_number,
                    entry_path.display()
                )));
            }

            let source_hash = B3Hash::hash(line.as_bytes()).to_hex();
            let value: Value = serde_json::from_str(&line).map_err(|e| {
                AosError::Training(format!(
                    "Failed to parse JSON line {} in {}: {}",
                    line_number,
                    entry_path.display(),
                    e
                ))
            })?;
            let obj = value.as_object().ok_or_else(|| {
                AosError::Training(format!(
                    "Expected JSON object at {}:{}",
                    entry_path.display(),
                    line_number
                ))
            })?;

            let is_supervised =
                obj.len() == 2 && obj.contains_key("prompt") && obj.contains_key("completion");
            let is_raw = obj.len() == 1 && obj.contains_key("text");

            if !is_supervised && !is_raw {
                return Err(AosError::Training(format!(
                    "Unsupported JSONL schema at {}:{}; expected {{\"prompt\",\"completion\"}} or {{\"text\"}} only",
                    entry_path.display(),
                    line_number
                )));
            }

            let line_schema = if is_supervised {
                SCHEMA_SUPERVISED
            } else {
                SCHEMA_RAW_CONTINUATION
            };
            if let Some(active) = schema_mode {
                if active != line_schema {
                    return Err(AosError::Training(format!(
                        "Mixed JSONL schemas detected: expected {}, found {} at {}:{}",
                        active,
                        line_schema,
                        entry_path.display(),
                        line_number
                    )));
                }
            } else {
                schema_mode = Some(line_schema);
            }

            let source_path = entry_path.to_string_lossy().to_string();
            let mut provenance = BTreeMap::new();
            provenance.insert(
                "source_path".to_string(),
                serde_json::Value::String(source_path.clone()),
            );
            provenance.insert(
                "dataset_name".to_string(),
                serde_json::Value::String(manifest.name.clone()),
            );
            provenance.insert(
                "line_number".to_string(),
                serde_json::Value::String(line_number.to_string()),
            );
            if let Some(ref role) = entry.role {
                provenance.insert(
                    "entry_role".to_string(),
                    serde_json::Value::String(role.clone()),
                );
            }
            if let Some(ref notes) = entry.notes {
                provenance.insert(
                    "entry_notes".to_string(),
                    serde_json::Value::String(notes.clone()),
                );
            }
            if let Some(num) = serde_json::Number::from_f64(entry_weight as f64) {
                provenance.insert("weight".to_string(), serde_json::Value::Number(num));
            } else {
                provenance.insert(
                    "weight".to_string(),
                    serde_json::Value::String(entry_weight.to_string()),
                );
            }

            if is_supervised {
                let prompt = obj
                    .get("prompt")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| {
                        AosError::Training(format!(
                            "Line {} in {} has empty prompt",
                            line_number,
                            entry_path.display()
                        ))
                    })?;
                let completion = obj
                    .get("completion")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| {
                        AosError::Training(format!(
                            "Line {} in {} has empty completion",
                            line_number,
                            entry_path.display()
                        ))
                    })?;

                let input_tokens = encoder(prompt)?;
                let target_tokens = encoder(completion)?;
                if input_tokens.is_empty() || target_tokens.is_empty() {
                    return Err(AosError::Training(format!(
                        "Line {} in {} produced empty token sequence",
                        line_number,
                        entry_path.display()
                    )));
                }

                total_tokens =
                    total_tokens.saturating_add((input_tokens.len() + target_tokens.len()) as u64);
                if total_tokens > limits.max_tokens {
                    return Err(AosError::Training(format!(
                        "Dataset token count exceeds limit: {} > {}",
                        total_tokens, limits.max_tokens
                    )));
                }

                provenance.insert(
                    "schema".to_string(),
                    serde_json::Value::String(SCHEMA_SUPERVISED.to_string()),
                );
                let metadata = ExampleMetadataV1::new(
                    source_path,
                    line_number as u64,
                    source_hash,
                    provenance_from_map(&provenance)
                        .map_err(|e| AosError::Training(format!("Metadata error: {}", e)))?,
                    created_at_unix_ms,
                );
                let attention_mask =
                    TrainingExampleV1::attention_mask_from_tokens(&input_tokens, pad_token_id);
                all_examples.push(TrainingExampleV1::new(
                    input_tokens,
                    target_tokens,
                    attention_mask,
                    metadata,
                ));
                if all_examples.len() > limits.max_samples {
                    return Err(AosError::Training(format!(
                        "Dataset sample count exceeds limit: {} > {}",
                        all_examples.len(),
                        limits.max_samples
                    )));
                }
                continue;
            }

            let text = obj
                .get("text")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    AosError::Training(format!(
                        "Line {} in {} has empty text",
                        line_number,
                        entry_path.display()
                    ))
                })?;
            let tokens = encoder(text)?;
            if tokens.len() <= MAX_INPUT_TOKENS {
                warn!(
                    line_number,
                    token_count = tokens.len(),
                    "Raw text row too short for continuation framing; dropping row"
                );
                continue;
            }

            let mut produced = 0usize;
            let mut start = 0usize;
            while start < tokens.len() {
                let input_end = start + MAX_INPUT_TOKENS;
                if input_end >= tokens.len() {
                    break;
                }
                let target_end = input_end + MAX_TARGET_TOKENS;
                let input_tokens = tokens[start..input_end].to_vec();
                let target_tokens = tokens[input_end..tokens.len().min(target_end)].to_vec();
                if input_tokens.is_empty() || target_tokens.is_empty() {
                    break;
                }

                total_tokens =
                    total_tokens.saturating_add((input_tokens.len() + target_tokens.len()) as u64);
                if total_tokens > limits.max_tokens {
                    return Err(AosError::Training(format!(
                        "Dataset token count exceeds limit: {} > {}",
                        total_tokens, limits.max_tokens
                    )));
                }

                let mut chunk_provenance = provenance.clone();
                chunk_provenance.insert(
                    "schema".to_string(),
                    serde_json::Value::String(SCHEMA_RAW_CONTINUATION.to_string()),
                );
                chunk_provenance.insert(
                    "chunk_index".to_string(),
                    serde_json::Value::String(produced.to_string()),
                );
                let metadata = ExampleMetadataV1::new(
                    source_path.clone(),
                    line_number as u64,
                    source_hash.clone(),
                    provenance_from_map(&chunk_provenance)
                        .map_err(|e| AosError::Training(format!("Metadata error: {}", e)))?,
                    created_at_unix_ms,
                );
                let attention_mask =
                    TrainingExampleV1::attention_mask_from_tokens(&input_tokens, pad_token_id);
                all_examples.push(TrainingExampleV1::new(
                    input_tokens,
                    target_tokens,
                    attention_mask,
                    metadata,
                ));
                if all_examples.len() > limits.max_samples {
                    return Err(AosError::Training(format!(
                        "Dataset sample count exceeds limit: {} > {}",
                        all_examples.len(),
                        limits.max_samples
                    )));
                }

                produced += 1;
                start = start.saturating_add(STRIDE_TOKENS);
            }

            if produced == 0 {
                warn!(
                    line_number,
                    token_count = tokens.len(),
                    "Raw text row produced no training chunks"
                );
            }
        }
    }

    if all_examples.is_empty() {
        return Err(AosError::Training(format!(
            "Dataset manifest {} produced zero examples",
            manifest_path.display()
        )));
    }

    let contract = TrainingDataContractConfig::new(pad_token_id, -1);
    validate_training_examples(&all_examples, vocab_size, &contract)
        .map_err(|err| AosError::Training(format!("Dataset example validation failed: {}", err)))?;

    Ok(all_examples)
}

fn default_format() -> String {
    "jsonl".to_string()
}

const fn default_entry_weight() -> f32 {
    1.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_types::training::{weight_from_metadata, TRAINING_DATA_CONTRACT_VERSION};
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_load_examples_with_encoder() {
        let tmp = tempdir().expect("tempdir");
        let manifest_path = tmp.path().join("manifest.json");
        let positive_path = tmp.path().join("positive.jsonl");
        let negative_path = tmp.path().join("negative.jsonl");
        let positive_name = positive_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let negative_name = negative_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();

        // Write manifest
        fs::write(
            &manifest_path,
            serde_json::json!({
                "name": "test_dataset",
                "training_contract_version": TRAINING_DATA_CONTRACT_VERSION,
                "entries": [
                    { "path": positive_name, "format": "jsonl", "weight": 1.0 },
                    { "path": negative_name, "format": "jsonl", "weight": -1.0 }
                ]
            })
            .to_string(),
        )
        .unwrap();

        // Write samples
        let mut positive_file = File::create(&positive_path).unwrap();
        writeln!(
            positive_file,
            "{}",
            serde_json::json!({
                "prompt": "Say hello",
                "completion": "Hello!"
            })
        )
        .unwrap();

        let mut negative_file = File::create(&negative_path).unwrap();
        writeln!(
            negative_file,
            "{}",
            serde_json::json!({
                "prompt": "Do a bad thing",
                "completion": "I can't help with that."
            })
        )
        .unwrap();

        let encoder =
            |text: &str| -> Result<Vec<u32>> { Ok(text.chars().map(|c| c as u32).collect()) };
        let examples = load_examples_with_encoder(&manifest_path, 0, 1024, encoder).unwrap();

        assert_eq!(examples.len(), 2);
        let pos = &examples[0];
        let pos_prov: serde_json::Value = serde_json::from_str(&pos.metadata.provenance).unwrap();
        assert_eq!(
            pos_prov.get("schema").and_then(|v| v.as_str()),
            Some(SCHEMA_SUPERVISED)
        );
        assert_eq!(weight_from_metadata(&pos.metadata), Some(1.0));

        let neg = &examples[1];
        let neg_prov: serde_json::Value = serde_json::from_str(&neg.metadata.provenance).unwrap();
        assert_eq!(
            neg_prov.get("schema").and_then(|v| v.as_str()),
            Some(SCHEMA_SUPERVISED)
        );
        assert_eq!(weight_from_metadata(&neg.metadata), Some(-1.0));
        assert_eq!(neg.target_tokens.len(), "I can't help with that.".len());
    }

    #[test]
    fn test_manifest_entry_weight_is_applied() {
        let tmp = tempdir().expect("tempdir");
        let manifest_path = tmp.path().join("manifest.json");
        let weighted_path = tmp.path().join("weighted.jsonl");

        fs::write(
            &manifest_path,
            serde_json::json!({
                "name": "weighting_dataset",
                "training_contract_version": TRAINING_DATA_CONTRACT_VERSION,
                "entries": [
                    { "path": "weighted.jsonl", "format": "jsonl", "weight": 0.5 }
                ]
            })
            .to_string(),
        )
        .unwrap();

        let mut weighted_file = File::create(&weighted_path).unwrap();
        writeln!(
            weighted_file,
            "{}",
            serde_json::json!({
                "prompt": "ping",
                "completion": "pong"
            })
        )
        .unwrap();

        let encoder =
            |text: &str| -> Result<Vec<u32>> { Ok(text.chars().map(|c| c as u32).collect()) };
        let examples = load_examples_with_encoder(&manifest_path, 0, 1024, encoder).unwrap();

        assert_eq!(examples.len(), 1);
        assert_eq!(weight_from_metadata(&examples[0].metadata), Some(0.5));
    }
}
