//! JSON-based dataset loader for LoRA training
//!
//! Supports structured JSON training data with positive/negative weight separation
//! and flexible input/output formats for different training scenarios.

use super::limits::DatasetSizeLimits;
use adapteros_core::{AosError, Result};
use adapteros_secure_fs::path_policy::canonicalize_strict;
use adapteros_types::training::{provenance_from_map, ExampleMetadataV1, TrainingExampleV1};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;
use tokenizers::Tokenizer;
use tracing::{info, warn};

/// JSON training dataset structure
#[derive(Debug, Deserialize, serde::Serialize)]
pub struct JsonTrainingDataset {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    pub examples: Vec<JsonTrainingExample>,
    #[serde(default)]
    pub metadata: HashMap<String, Value>,
}

/// Individual JSON training example
#[derive(Debug, Deserialize, serde::Serialize)]
pub struct JsonTrainingExample {
    /// Example ID (optional)
    #[serde(default)]
    pub id: Option<String>,

    /// Input data (flexible format)
    pub input: JsonInput,

    /// Target/output data (flexible format)
    pub target: JsonTarget,

    /// Example weight (+1.0 for positive, -1.0 for negative)
    #[serde(default = "default_example_weight")]
    pub weight: f32,

    /// Example metadata
    #[serde(default)]
    pub metadata: Option<HashMap<String, Value>>,

    /// Example category/tags
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Flexible input format for JSON training
#[derive(Debug, Deserialize, serde::Serialize)]
#[serde(untagged)]
pub enum JsonInput {
    /// Simple text input
    Text(String),
    /// Structured JSON input
    Structured(Value),
    /// Code input with language specification
    Code { content: String, language: String },
    /// Multimodal input (text + additional data)
    Multimodal { text: String, data: Value },
}

/// Flexible target format for JSON training
#[derive(Debug, Deserialize, serde::Serialize)]
#[serde(untagged)]
pub enum JsonTarget {
    /// Simple text output
    Text(String),
    /// Structured JSON output
    Structured(Value),
    /// Code output with language specification
    Code { content: String, language: String },
    /// Multiple possible outputs
    Multiple(Vec<String>),
}

/// JSON dataset loader configuration
#[derive(Clone)]
pub struct JsonLoaderConfig {
    /// Tokenizer to use for text encoding
    pub tokenizer: Option<Tokenizer>,
    /// Maximum input length (tokens)
    pub max_input_length: usize,
    /// Maximum target length (tokens)
    pub max_target_length: usize,
    /// Whether to separate positive/negative examples
    pub separate_weights: bool,
    /// Explicit pad token ID
    pub pad_token_id: u32,
}

impl Default for JsonLoaderConfig {
    fn default() -> Self {
        Self {
            tokenizer: None,
            max_input_length: 2048,
            max_target_length: 512,
            separate_weights: true,
            pad_token_id: 0,
        }
    }
}

/// Load training examples from JSON dataset
pub fn load_json_dataset<P: AsRef<Path>>(
    path: P,
    config: JsonLoaderConfig,
) -> Result<Vec<TrainingExampleV1>> {
    let path = canonicalize_strict(path.as_ref())?;

    // Read JSON file
    let content = fs::read_to_string(&path).map_err(|e| {
        AosError::Training(format!(
            "Failed to read JSON dataset {}: {}",
            path.display(),
            e
        ))
    })?;

    let dataset: JsonTrainingDataset = serde_json::from_str(&content).map_err(|e| {
        AosError::Training(format!(
            "Failed to parse JSON dataset {}: {}",
            path.display(),
            e
        ))
    })?;

    let limits = DatasetSizeLimits::from_env();
    if content.len() as u64 > limits.max_total_bytes {
        return Err(AosError::Training(format!(
            "JSON dataset exceeds size limit: {} > {} bytes",
            content.len(),
            limits.max_total_bytes
        )));
    }
    if dataset.examples.len() > limits.max_samples {
        return Err(AosError::Training(format!(
            "JSON dataset exceeds sample limit: {} > {}",
            dataset.examples.len(),
            limits.max_samples
        )));
    }

    info!(
        "Loading JSON dataset: {} ({} examples)",
        dataset.name,
        dataset.examples.len()
    );

    // Convert JSON examples to training examples
    let mut training_examples = Vec::new();
    let created_at_unix_ms = chrono::Utc::now().timestamp_millis() as u64;
    let mut positive_count = 0;
    let mut negative_count = 0;
    let mut total_tokens: u64 = 0;

    for (idx, example) in dataset.examples.iter().enumerate() {
        let input_tokens = encode_input(&example.input, &config)?;
        let target_tokens = encode_target(&example.target, &config)?;

        total_tokens =
            total_tokens.saturating_add((input_tokens.len() + target_tokens.len()) as u64);
        if total_tokens > limits.max_tokens {
            return Err(AosError::Training(format!(
                "JSON dataset token count exceeds limit: {} > {}",
                total_tokens, limits.max_tokens
            )));
        }

        // Validate lengths
        if input_tokens.len() > config.max_input_length {
            warn!(
                "Example {} input too long: {} tokens (max: {})",
                idx,
                input_tokens.len(),
                config.max_input_length
            );
            continue;
        }

        if target_tokens.len() > config.max_target_length {
            warn!(
                "Example {} target too long: {} tokens (max: {})",
                idx,
                target_tokens.len(),
                config.max_target_length
            );
            continue;
        }

        // Build metadata provenance
        let mut provenance = BTreeMap::new();
        provenance.insert(
            "dataset_name".to_string(),
            serde_json::Value::String(dataset.name.clone()),
        );
        provenance.insert(
            "example_index".to_string(),
            serde_json::Value::String(idx.to_string()),
        );

        if let Some(ref id) = example.id {
            provenance.insert(
                "example_id".to_string(),
                serde_json::Value::String(id.clone()),
            );
        }

        if !example.tags.is_empty() {
            provenance.insert(
                "tags".to_string(),
                serde_json::Value::String(example.tags.join(",")),
            );
        }

        if let Some(ref example_metadata) = example.metadata {
            for (key, value) in example_metadata {
                provenance.insert(
                    key.clone(),
                    serde_json::Value::String(flatten_json_value(value)),
                );
            }
        }

        // Add dataset metadata
        for (key, value) in &dataset.metadata {
            provenance.insert(
                format!("dataset_{}", key),
                serde_json::Value::String(flatten_json_value(value)),
            );
        }

        if let Some(num) = serde_json::Number::from_f64(example.weight as f64) {
            provenance.insert("weight".to_string(), serde_json::Value::Number(num));
        } else {
            provenance.insert(
                "weight".to_string(),
                serde_json::Value::String(example.weight.to_string()),
            );
        }

        let metadata = ExampleMetadataV1::new(
            dataset.name.clone(),
            idx as u64,
            provenance_from_map(&provenance)
                .map_err(|e| AosError::Training(format!("Metadata error: {}", e)))?,
            created_at_unix_ms,
        );
        let attention_mask =
            TrainingExampleV1::attention_mask_from_tokens(&input_tokens, config.pad_token_id);
        let training_example =
            TrainingExampleV1::new(input_tokens, target_tokens, attention_mask, metadata);

        training_examples.push(training_example);

        if example.weight > 0.0 {
            positive_count += 1;
        } else if example.weight < 0.0 {
            negative_count += 1;
        }
    }

    info!(
        "Loaded {} examples: {} positive, {} negative",
        training_examples.len(),
        positive_count,
        negative_count
    );

    if training_examples.is_empty() {
        return Err(AosError::Training(format!(
            "JSON dataset {} produced zero valid examples",
            path.display()
        )));
    }

    Ok(training_examples)
}

/// Encode input to token sequence
fn encode_input(input: &JsonInput, config: &JsonLoaderConfig) -> Result<Vec<u32>> {
    match input {
        JsonInput::Text(text) => {
            if let Some(ref tokenizer) = config.tokenizer {
                let encoding = tokenizer
                    .encode(text.as_str(), false)
                    .map_err(|e| AosError::Training(format!("Encoding failed: {}", e)))?;
                Ok(encoding.get_ids().to_vec())
            } else {
                // Fallback to character-level encoding
                Ok(text.chars().map(|c| c as u32).collect())
            }
        }
        JsonInput::Structured(value) => {
            // Convert structured data to text representation
            let text = serde_json::to_string(value).map_err(|e| {
                AosError::Training(format!("Failed to serialize structured input: {}", e))
            })?;
            encode_input(&JsonInput::Text(text), config)
        }
        JsonInput::Code { content, language } => {
            // Format as code block
            let formatted = format!("```{}\n{}\n```", language, content);
            encode_input(&JsonInput::Text(formatted), config)
        }
        JsonInput::Multimodal { text, data } => {
            // Combine text and structured data
            let data_text = serde_json::to_string(data).map_err(|e| {
                AosError::Training(format!("Failed to serialize multimodal data: {}", e))
            })?;
            let combined = format!("{}\n\nData: {}", text, data_text);
            encode_input(&JsonInput::Text(combined), config)
        }
    }
}

/// Encode target to token sequence
fn encode_target(target: &JsonTarget, config: &JsonLoaderConfig) -> Result<Vec<u32>> {
    match target {
        JsonTarget::Text(text) => {
            if let Some(ref tokenizer) = config.tokenizer {
                let encoding = tokenizer
                    .encode(text.as_str(), false)
                    .map_err(|e| AosError::Training(format!("Encoding failed: {}", e)))?;
                Ok(encoding.get_ids().to_vec())
            } else {
                // Fallback to character-level encoding
                Ok(text.chars().map(|c| c as u32).collect())
            }
        }
        JsonTarget::Structured(value) => {
            // Convert structured data to text representation
            let text = serde_json::to_string(value).map_err(|e| {
                AosError::Training(format!("Failed to serialize structured target: {}", e))
            })?;
            encode_target(&JsonTarget::Text(text), config)
        }
        JsonTarget::Code { content, language } => {
            // Format as code block
            let formatted = format!("```{}\n{}\n```", language, content);
            encode_target(&JsonTarget::Text(formatted), config)
        }
        JsonTarget::Multiple(outputs) => {
            // Use the first output (could be extended to handle multiple targets)
            if let Some(first_output) = outputs.first() {
                encode_target(&JsonTarget::Text(first_output.clone()), config)
            } else {
                Err(AosError::Training("Empty multiple target".to_string()))
            }
        }
    }
}

/// Flatten JSON value to string
fn flatten_json_value(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        Value::Array(arr) => arr
            .iter()
            .map(flatten_json_value)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(obj) => {
            let mut parts = Vec::new();
            for (k, v) in obj {
                let val = flatten_json_value(v);
                if !val.is_empty() {
                    parts.push(format!("{}={}", k, val));
                }
            }
            parts.join(";")
        }
    }
}

/// Default example weight
const fn default_example_weight() -> f32 {
    1.0
}

/// Load JSON dataset with tokenizer
pub fn load_json_dataset_with_tokenizer<P: AsRef<Path>>(
    path: P,
    tokenizer: &Tokenizer,
) -> Result<Vec<TrainingExampleV1>> {
    let config = JsonLoaderConfig {
        tokenizer: Some(tokenizer.clone()),
        ..Default::default()
    };

    load_json_dataset(path, config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_types::training::weight_from_metadata;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_load_json_dataset() {
        let tmp = tempdir().expect("tempdir");
        let json_path = tmp.path().join("dataset.json");

        // Write test JSON dataset
        let dataset = JsonTrainingDataset {
            name: "test_dataset".to_string(),
            description: Some("Test dataset".to_string()),
            version: Some("1.0.0".to_string()),
            examples: vec![
                JsonTrainingExample {
                    id: Some("pos1".to_string()),
                    input: JsonInput::Text("Say hello".to_string()),
                    target: JsonTarget::Text("Hello!".to_string()),
                    weight: 1.0,
                    metadata: Some(HashMap::from([(
                        "source".to_string(),
                        Value::String("test".to_string()),
                    )])),
                    tags: vec!["greeting".to_string()],
                },
                JsonTrainingExample {
                    id: Some("neg1".to_string()),
                    input: JsonInput::Text("Do something bad".to_string()),
                    target: JsonTarget::Text("I can't help with that.".to_string()),
                    weight: -1.0,
                    metadata: None,
                    tags: vec!["refusal".to_string()],
                },
            ],
            metadata: HashMap::from([(
                "author".to_string(),
                Value::String("test_author".to_string()),
            )]),
        };

        let mut file = File::create(&json_path).unwrap();
        writeln!(file, "{}", serde_json::to_string_pretty(&dataset).unwrap()).unwrap();

        // Load dataset
        let config = JsonLoaderConfig::default();
        let examples = load_json_dataset(&json_path, config).unwrap();

        assert_eq!(examples.len(), 2);
        assert_eq!(weight_from_metadata(&examples[0].metadata), Some(1.0));
        assert_eq!(weight_from_metadata(&examples[1].metadata), Some(-1.0));

        // Check metadata
        let provenance: serde_json::Value =
            serde_json::from_str(&examples[0].metadata.provenance).unwrap();
        assert_eq!(
            provenance.get("dataset_name").and_then(|v| v.as_str()),
            Some("test_dataset")
        );
        assert_eq!(
            provenance.get("example_id").and_then(|v| v.as_str()),
            Some("pos1")
        );
        assert_eq!(
            provenance.get("tags").and_then(|v| v.as_str()),
            Some("greeting")
        );
        assert_eq!(
            provenance.get("dataset_author").and_then(|v| v.as_str()),
            Some("test_author")
        );
    }
}
