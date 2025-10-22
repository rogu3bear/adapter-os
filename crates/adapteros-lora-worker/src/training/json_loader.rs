//! JSON-based dataset loader for LoRA training
//!
//! Supports structured JSON training data with positive/negative weight separation
//! and flexible input/output formats for different training scenarios.

use super::dataset::TrainingExample;
use tokenizers::Tokenizer;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// JSON training dataset structure
#[derive(Debug, Deserialize)]
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
#[derive(Debug, Deserialize)]
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
#[derive(Debug, Deserialize)]
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
#[derive(Debug, Deserialize)]
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
#[derive(Debug, Clone)]
pub struct JsonLoaderConfig {
    /// Tokenizer to use for text encoding
    pub tokenizer: Option<Tokenizer>,
    /// Maximum input length (tokens)
    pub max_input_length: usize,
    /// Maximum target length (tokens)
    pub max_target_length: usize,
    /// Whether to separate positive/negative examples
    pub separate_weights: bool,
    /// Custom encoding function for non-text inputs
    pub custom_encoder: Option<Box<dyn Fn(&JsonInput) -> Result<Vec<u32>> + Send + Sync>>,
}

impl Default for JsonLoaderConfig {
    fn default() -> Self {
        Self {
            tokenizer: None,
            max_input_length: 2048,
            max_target_length: 512,
            separate_weights: true,
            custom_encoder: None,
        }
    }
}

/// Load training examples from JSON dataset
pub fn load_json_dataset<P: AsRef<Path>>(
    path: P,
    config: JsonLoaderConfig,
) -> Result<Vec<TrainingExample>> {
    let path = path.as_ref();
    
    // Read JSON file
    let content = fs::read_to_string(path)
        .map_err(|e| AosError::Training(format!("Failed to read JSON dataset {}: {}", path.display(), e)))?;
    
    let dataset: JsonTrainingDataset = serde_json::from_str(&content)
        .map_err(|e| AosError::Training(format!("Failed to parse JSON dataset {}: {}", path.display(), e)))?;
    
    info!("Loading JSON dataset: {} ({} examples)", dataset.name, dataset.examples.len());
    
    // Convert JSON examples to training examples
    let mut training_examples = Vec::new();
    let mut positive_count = 0;
    let mut negative_count = 0;
    
    for (idx, example) in dataset.examples.iter().enumerate() {
        let input_tokens = encode_input(&example.input, &config)?;
        let target_tokens = encode_target(&example.target, &config)?;
        
        // Validate lengths
        if input_tokens.len() > config.max_input_length {
            warn!("Example {} input too long: {} tokens (max: {})", idx, input_tokens.len(), config.max_input_length);
            continue;
        }
        
        if target_tokens.len() > config.max_target_length {
            warn!("Example {} target too long: {} tokens (max: {})", idx, target_tokens.len(), config.max_target_length);
            continue;
        }
        
        // Build metadata
        let mut metadata = HashMap::new();
        metadata.insert("dataset_name".to_string(), dataset.name.clone());
        metadata.insert("example_index".to_string(), idx.to_string());
        
        if let Some(ref id) = example.id {
            metadata.insert("example_id".to_string(), id.clone());
        }
        
        if !example.tags.is_empty() {
            metadata.insert("tags".to_string(), example.tags.join(","));
        }
        
        if let Some(ref example_metadata) = example.metadata {
            for (key, value) in example_metadata {
                metadata.insert(key.clone(), flatten_json_value(value));
            }
        }
        
        // Add dataset metadata
        for (key, value) in &dataset.metadata {
            metadata.insert(format!("dataset_{}", key), flatten_json_value(value));
        }
        
        let training_example = TrainingExample {
            input: input_tokens,
            target: target_tokens,
            metadata,
            weight: example.weight,
        };
        
        training_examples.push(training_example);
        
        if example.weight > 0.0 {
            positive_count += 1;
        } else if example.weight < 0.0 {
            negative_count += 1;
        }
    }
    
    info!("Loaded {} examples: {} positive, {} negative", 
          training_examples.len(), positive_count, negative_count);
    
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
                tokenizer.encode(text)
            } else {
                // Fallback to character-level encoding
                Ok(text.chars().map(|c| c as u32).collect())
            }
        }
        JsonInput::Structured(value) => {
            // Convert structured data to text representation
            let text = serde_json::to_string(value)
                .map_err(|e| AosError::Training(format!("Failed to serialize structured input: {}", e)))?;
            encode_input(&JsonInput::Text(text), config)
        }
        JsonInput::Code { content, language } => {
            // Format as code block
            let formatted = format!("```{}\n{}\n```", language, content);
            encode_input(&JsonInput::Text(formatted), config)
        }
        JsonInput::Multimodal { text, data } => {
            // Combine text and structured data
            let data_text = serde_json::to_string(data)
                .map_err(|e| AosError::Training(format!("Failed to serialize multimodal data: {}", e)))?;
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
                tokenizer.encode(text)
            } else {
                // Fallback to character-level encoding
                Ok(text.chars().map(|c| c as u32).collect())
            }
        }
        JsonTarget::Structured(value) => {
            // Convert structured data to text representation
            let text = serde_json::to_string(value)
                .map_err(|e| AosError::Training(format!("Failed to serialize structured target: {}", e)))?;
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
        Value::Array(arr) => {
            arr.iter()
                .map(flatten_json_value)
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(",")
        }
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
) -> Result<Vec<TrainingExample>> {
    let config = JsonLoaderConfig {
        tokenizer: Some(tokenizer.clone()),
        ..Default::default()
    };

    load_json_dataset(path, config)
}

/// Load JSON dataset with custom encoder
pub fn load_json_dataset_with_encoder<P, F>(
    path: P,
    encoder: F,
) -> Result<Vec<TrainingExample>>
where
    P: AsRef<Path>,
    F: Fn(&JsonInput) -> Result<Vec<u32>> + Send + Sync + 'static,
{
    let config = JsonLoaderConfig {
        custom_encoder: Some(Box::new(encoder)),
        ..Default::default()
    };
    
    load_json_dataset(path, config)
}

#[cfg(test)]
mod tests {
    use super::*;
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
                    metadata: Some(HashMap::from([
                        ("source".to_string(), Value::String("test".to_string())),
                    ])),
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
            metadata: HashMap::from([
                ("author".to_string(), Value::String("test_author".to_string())),
            ]),
        };
        
        let mut file = File::create(&json_path).unwrap();
        writeln!(file, "{}", serde_json::to_string_pretty(&dataset).unwrap()).unwrap();
        
        // Load dataset
        let config = JsonLoaderConfig::default();
        let examples = load_json_dataset(&json_path, config).unwrap();
        
        assert_eq!(examples.len(), 2);
        assert_eq!(examples[0].weight, 1.0);
        assert_eq!(examples[1].weight, -1.0);
        
        // Check metadata
        assert_eq!(examples[0].metadata.get("dataset_name").unwrap(), "test_dataset");
        assert_eq!(examples[0].metadata.get("example_id").unwrap(), "pos1");
        assert_eq!(examples[0].metadata.get("tags").unwrap(), "greeting");
        assert_eq!(examples[0].metadata.get("dataset_author").unwrap(), "test_author");
    }
}
