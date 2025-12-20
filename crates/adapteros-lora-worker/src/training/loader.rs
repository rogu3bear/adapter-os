//! Dataset manifest loader for DIR training
//!
//! Supports positive/negative weighted JSONL files, converting them into `TrainingExample`
//! instances encoded with the Qwen tokenizer.

use super::dataset::TrainingExample;
use crate::tokenizer::QwenTokenizer;
use adapteros_core::{AosError, Result};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use tracing::warn;

/// Dataset manifest describing positive/negative inputs.
#[derive(Debug, Deserialize)]
pub struct DatasetManifest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
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

#[derive(Debug, Deserialize)]
struct JsonlSample {
    #[serde(default)]
    id: Option<String>,
    prompt: String,
    response: String,
    #[serde(default = "default_sample_weight")]
    weight: f32,
    #[serde(default)]
    metadata: Option<HashMap<String, Value>>,
}

/// Load training examples from a manifest using the provided tokenizer.
pub fn load_examples_from_manifest<P: AsRef<Path>>(
    manifest_path: P,
    tokenizer: &QwenTokenizer,
) -> Result<Vec<TrainingExample>> {
    load_examples_with_encoder(manifest_path, |text| tokenizer.encode(text))
}

/// Load training examples using a caller-provided encoding closure (useful for testing).
pub fn load_examples_with_encoder<P, F>(
    manifest_path: P,
    encoder: F,
) -> Result<Vec<TrainingExample>>
where
    P: AsRef<Path>,
    F: Fn(&str) -> Result<Vec<u32>>,
{
    let manifest_path = manifest_path.as_ref();
    let manifest_dir = manifest_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    let manifest_str = fs::read_to_string(manifest_path).map_err(|e| {
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

    let mut all_examples = Vec::new();

    for entry in manifest.entries.iter() {
        if entry.format != "jsonl" {
            return Err(AosError::Training(format!(
                "Unsupported dataset format '{}' in {}",
                entry.format, entry.path
            )));
        }

        let entry_path = manifest_dir.join(&entry.path);
        let file = fs::File::open(&entry_path).map_err(|e| {
            AosError::Training(format!(
                "Failed to open dataset entry {}: {}",
                entry_path.display(),
                e
            ))
        })?;
        let reader = BufReader::new(file);

        for (line_idx, line) in reader.lines().enumerate() {
            let line = line.map_err(|e| {
                AosError::Training(format!(
                    "Failed to read line {} in {}: {}",
                    line_idx + 1,
                    entry_path.display(),
                    e
                ))
            })?;

            if line.trim().is_empty() {
                continue;
            }

            let sample: JsonlSample = serde_json::from_str(&line).map_err(|e| {
                AosError::Training(format!(
                    "Failed to parse JSON line {} in {}: {}",
                    line_idx + 1,
                    entry_path.display(),
                    e
                ))
            })?;

            let entry_weight = entry.weight;
            let combined_weight = sample.weight * entry_weight;
            if combined_weight.abs() < f32::EPSILON {
                continue;
            }

            let input_tokens = encoder(&sample.prompt)?;
            let target_tokens = encoder(&sample.response)?;

            if input_tokens.is_empty() || target_tokens.is_empty() {
                warn!(
                    "Skipping dataset example {} due to empty token sequence",
                    sample.id.as_deref().unwrap_or("<unknown>")
                );
                continue;
            }

            if let Some(max_id) = input_tokens
                .iter()
                .chain(target_tokens.iter())
                .copied()
                .max()
            {
                if max_id > 10_000_000 {
                    warn!(
                        "Token id {} exceeds expected range for example {}",
                        max_id,
                        sample.id.as_deref().unwrap_or("<unknown>")
                    );
                }
            }

            let mut metadata = HashMap::new();
            metadata.insert(
                "source_path".to_string(),
                entry_path.to_string_lossy().to_string(),
            );
            metadata.insert("dataset_name".to_string(), manifest.name.clone());
            if let Some(ref id) = sample.id {
                metadata.insert("example_id".to_string(), id.clone());
            }
            if let Some(ref role) = entry.role {
                metadata.insert("entry_role".to_string(), role.clone());
            }
            if let Some(ref notes) = entry.notes {
                metadata.insert("entry_notes".to_string(), notes.clone());
            }
            if let Some(map) = sample.metadata {
                for (key, value) in map {
                    metadata.insert(key, flatten_metadata_value(&value));
                }
            }

            all_examples.push(TrainingExample {
                input: input_tokens,
                target: target_tokens,
                metadata,
                weight: combined_weight,
            });
        }
    }

    if all_examples.is_empty() {
        return Err(AosError::Training(format!(
            "Dataset manifest {} produced zero examples",
            manifest_path.display()
        )));
    }

    Ok(all_examples)
}

fn flatten_metadata_value(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        Value::Array(arr) => arr
            .iter()
            .map(flatten_metadata_value)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(obj) => {
            let mut parts = Vec::new();
            for (k, v) in obj {
                let val = flatten_metadata_value(v);
                if !val.is_empty() {
                    parts.push(format!("{}={}", k, val));
                }
            }
            parts.join(";")
        }
    }
}

fn default_format() -> String {
    "jsonl".to_string()
}

const fn default_entry_weight() -> f32 {
    1.0
}

const fn default_sample_weight() -> f32 {
    1.0
}

#[cfg(test)]
mod tests {
    use super::*;
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
                "id": "pos1",
                "prompt": "Say hello",
                "response": "Hello!",
                "metadata": { "tags": ["greeting"] }
            })
        )
        .unwrap();

        let mut negative_file = File::create(&negative_path).unwrap();
        writeln!(
            negative_file,
            "{}",
            serde_json::json!({
                "id": "neg1",
                "prompt": "Do a bad thing",
                "response": "I can't help with that.",
                "weight": 0.5
            })
        )
        .unwrap();

        let encoder =
            |text: &str| -> Result<Vec<u32>> { Ok(text.chars().map(|c| c as u32).collect()) };
        let examples = load_examples_with_encoder(&manifest_path, encoder).unwrap();

        assert_eq!(examples.len(), 2);
        let pos = &examples[0];
        assert_eq!(pos.metadata.get("example_id").unwrap(), "pos1");
        assert!((pos.weight - 1.0).abs() < f32::EPSILON);

        let neg = &examples[1];
        assert_eq!(neg.metadata.get("example_id").unwrap(), "neg1");
        assert!((neg.weight + 0.5).abs() < f32::EPSILON);
        assert_eq!(neg.target.len(), "I can't help with that.".len());
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
                "id": "w1",
                "prompt": "ping",
                "response": "pong",
                "weight": 0.25
            })
        )
        .unwrap();
        writeln!(
            weighted_file,
            "{}",
            serde_json::json!({
                "id": "w0",
                "prompt": "skip",
                "response": "ignored",
                "weight": 0.0
            })
        )
        .unwrap();

        let encoder =
            |text: &str| -> Result<Vec<u32>> { Ok(text.chars().map(|c| c as u32).collect()) };
        let examples = load_examples_with_encoder(&manifest_path, encoder).unwrap();

        assert_eq!(examples.len(), 1);
        assert!((examples[0].weight - 0.125).abs() < f32::EPSILON);
    }
}
