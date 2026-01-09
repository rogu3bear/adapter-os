//! Training example contract (tokenized form).
//!
//! Defines a single, versioned contract for training data across all backends.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Current training data contract version.
pub const TRAINING_DATA_CONTRACT_VERSION: &str = "1.0";

/// Metadata contract for a training example.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "server", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub struct ExampleMetadataV1 {
    /// Logical source identifier for the example (dataset or file origin).
    pub source_id: String,
    /// Stable row identifier within the source.
    pub row_id: u64,
    /// Provenance payload (canonical JSON string recommended).
    pub provenance: String,
    /// Created-at timestamp in Unix milliseconds.
    pub created_at_unix_ms: u64,
}

impl ExampleMetadataV1 {
    /// Create new example metadata.
    pub fn new(
        source_id: impl Into<String>,
        row_id: u64,
        provenance: impl Into<String>,
        created_at_unix_ms: u64,
    ) -> Self {
        Self {
            source_id: source_id.into(),
            row_id,
            provenance: provenance.into(),
            created_at_unix_ms,
        }
    }
}

/// A single training example (tokenized) for contract v1.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "server", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub struct TrainingExampleV1 {
    /// Input token IDs (prompt).
    pub input_tokens: Vec<u32>,
    /// Target token IDs (completion).
    pub target_tokens: Vec<u32>,
    /// Attention mask aligned with `input_tokens` (1 = real token, 0 = pad).
    pub attention_mask: Vec<u8>,
    /// Canonical metadata payload.
    pub metadata: ExampleMetadataV1,
}

impl TrainingExampleV1 {
    /// Create a training example with explicit mask and metadata.
    pub fn new(
        input_tokens: Vec<u32>,
        target_tokens: Vec<u32>,
        attention_mask: Vec<u8>,
        metadata: ExampleMetadataV1,
    ) -> Self {
        Self {
            input_tokens,
            target_tokens,
            attention_mask,
            metadata,
        }
    }

    /// Create a training example and derive the attention mask from pad token ID.
    pub fn with_pad_token(
        input_tokens: Vec<u32>,
        target_tokens: Vec<u32>,
        pad_token_id: u32,
        metadata: ExampleMetadataV1,
    ) -> Self {
        let attention_mask = Self::attention_mask_from_tokens(&input_tokens, pad_token_id);
        Self::new(input_tokens, target_tokens, attention_mask, metadata)
    }

    /// Build an attention mask from input tokens and pad token.
    pub fn attention_mask_from_tokens(input_tokens: &[u32], pad_token_id: u32) -> Vec<u8> {
        input_tokens
            .iter()
            .map(|token| if *token == pad_token_id { 0 } else { 1 })
            .collect()
    }
}

/// Contract config required to validate training examples.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "server", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub struct TrainingDataContractConfig {
    /// Training data contract version.
    pub contract_version: String,
    /// Explicit pad token ID for attention masking.
    pub pad_token_id: u32,
    /// Explicit ignore index for loss masking (-1 disables masking).
    pub ignore_index: i32,
}

impl TrainingDataContractConfig {
    pub fn new(pad_token_id: u32, ignore_index: i32) -> Self {
        Self {
            contract_version: TRAINING_DATA_CONTRACT_VERSION.to_string(),
            pad_token_id,
            ignore_index,
        }
    }
}

/// Location of a token sequence for validation errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrainingTokenLocation {
    /// Tokens from the input sequence.
    Input,
    /// Tokens from the target sequence.
    Target,
}

impl std::fmt::Display for TrainingTokenLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrainingTokenLocation::Input => f.write_str("input_tokens"),
            TrainingTokenLocation::Target => f.write_str("target_tokens"),
        }
    }
}

/// Summary for a validated training example batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrainingExampleBatchSummary {
    pub contract_version: String,
    pub pad_token_id: u32,
    pub ignore_index: i32,
    pub total_examples: usize,
    pub total_tokens: u64,
}

/// Validate a batch of training examples and return a summary.
pub fn validate_training_examples(
    examples: &[TrainingExampleV1],
    vocab_size: usize,
    contract: &TrainingDataContractConfig,
) -> Result<TrainingExampleBatchSummary, TrainingExampleValidationError> {
    if examples.is_empty() {
        return Err(TrainingExampleValidationError::EmptyBatch);
    }

    validate_training_contract_config(contract, vocab_size)?;

    let mut total_tokens = 0u64;

    for (index, example) in examples.iter().enumerate() {
        validate_training_example(example, index, vocab_size, contract.pad_token_id)?;
        total_tokens += (example.input_tokens.len() + example.target_tokens.len()) as u64;
    }

    Ok(TrainingExampleBatchSummary {
        contract_version: contract.contract_version.clone(),
        pad_token_id: contract.pad_token_id,
        ignore_index: contract.ignore_index,
        total_examples: examples.len(),
        total_tokens,
    })
}

/// Validate a single training example against the contract invariants.
pub fn validate_training_example(
    example: &TrainingExampleV1,
    index: usize,
    vocab_size: usize,
    pad_token_id: u32,
) -> Result<(), TrainingExampleValidationError> {
    if example.input_tokens.is_empty() {
        return Err(TrainingExampleValidationError::EmptyInput { index });
    }
    if example.target_tokens.is_empty() {
        return Err(TrainingExampleValidationError::EmptyTarget { index });
    }
    if example.attention_mask.len() != example.input_tokens.len() {
        return Err(TrainingExampleValidationError::AttentionMaskLengthMismatch {
            index,
            input_len: example.input_tokens.len(),
            mask_len: example.attention_mask.len(),
        });
    }
    for (pos, &value) in example.attention_mask.iter().enumerate() {
        if value > 1 {
            return Err(TrainingExampleValidationError::AttentionMaskValueInvalid {
                index,
                position: pos,
                value,
            });
        }
    }

    let pad_token_id_usize = pad_token_id as usize;
    if pad_token_id_usize >= vocab_size {
        return Err(TrainingExampleValidationError::PadTokenOutOfVocab {
            pad_token_id,
            vocab_size,
        });
    }
    for (pos, &token) in example.input_tokens.iter().enumerate() {
        let mask_value = example.attention_mask[pos];
        let is_pad = token == pad_token_id;
        if is_pad && mask_value != 0 {
            return Err(TrainingExampleValidationError::PadTokenMaskMismatch {
                index,
                position: pos,
                token,
                pad_token_id,
                mask_value,
            });
        }
        if !is_pad && mask_value == 0 {
            return Err(TrainingExampleValidationError::PadTokenMaskMismatch {
                index,
                position: pos,
                token,
                pad_token_id,
                mask_value,
            });
        }
    }

    if let Some(token) = example
        .input_tokens
        .iter()
        .find(|&&t| t as usize >= vocab_size)
    {
        return Err(TrainingExampleValidationError::TokenOutOfVocab {
            index,
            token: *token,
            vocab_size,
            location: TrainingTokenLocation::Input,
        });
    }
    if let Some(token) = example
        .target_tokens
        .iter()
        .find(|&&t| t as usize >= vocab_size)
    {
        return Err(TrainingExampleValidationError::TokenOutOfVocab {
            index,
            token: *token,
            vocab_size,
            location: TrainingTokenLocation::Target,
        });
    }

    Ok(())
}

/// Validate training contract configuration.
pub fn validate_training_contract_config(
    contract: &TrainingDataContractConfig,
    vocab_size: usize,
) -> Result<(), TrainingExampleValidationError> {
    if contract.contract_version != TRAINING_DATA_CONTRACT_VERSION {
        return Err(TrainingExampleValidationError::ContractVersionMismatch {
            expected: TRAINING_DATA_CONTRACT_VERSION.to_string(),
            actual: contract.contract_version.clone(),
        });
    }
    if contract.pad_token_id as usize >= vocab_size {
        return Err(TrainingExampleValidationError::PadTokenOutOfVocab {
            pad_token_id: contract.pad_token_id,
            vocab_size,
        });
    }
    if contract.ignore_index >= 0 && contract.ignore_index as usize >= vocab_size {
        return Err(TrainingExampleValidationError::IgnoreIndexOutOfVocab {
            ignore_index: contract.ignore_index,
            vocab_size,
        });
    }
    Ok(())
}

/// Build canonical provenance JSON from a stable map.
pub fn provenance_from_map(map: &BTreeMap<String, serde_json::Value>) -> Result<String, serde_json::Error> {
    serde_json::to_string(map)
}

/// Build canonical provenance JSON from string pairs.
pub fn provenance_from_pairs<I>(pairs: I) -> Result<String, serde_json::Error>
where
    I: IntoIterator<Item = (String, String)>,
{
    let mut map = BTreeMap::new();
    for (key, value) in pairs {
        map.insert(key, serde_json::Value::String(value));
    }
    provenance_from_map(&map)
}

/// Build metadata using provenance derived from string pairs.
pub fn metadata_from_pairs<I>(
    source_id: impl Into<String>,
    row_id: u64,
    created_at_unix_ms: u64,
    pairs: I,
) -> Result<ExampleMetadataV1, serde_json::Error>
where
    I: IntoIterator<Item = (String, String)>,
{
    let provenance = provenance_from_pairs(pairs)?;
    Ok(ExampleMetadataV1::new(
        source_id,
        row_id,
        provenance,
        created_at_unix_ms,
    ))
}

/// Extract weight from provenance JSON (if present).
pub fn weight_from_provenance(provenance: &str) -> Option<f32> {
    let value: serde_json::Value = serde_json::from_str(provenance).ok()?;
    let weight = value.get("weight")?;
    if let Some(num) = weight.as_f64() {
        return Some(num as f32);
    }
    if let Some(text) = weight.as_str() {
        return text.parse::<f32>().ok();
    }
    None
}

/// Extract weight from metadata provenance (if present).
pub fn weight_from_metadata(metadata: &ExampleMetadataV1) -> Option<f32> {
    weight_from_provenance(&metadata.provenance)
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TrainingExampleValidationError {
    #[error("Training example batch is empty")]
    EmptyBatch,
    #[error("Training contract version mismatch: expected {expected}, got {actual}")]
    ContractVersionMismatch { expected: String, actual: String },
    #[error("Example {index} input_tokens must be non-empty")]
    EmptyInput { index: usize },
    #[error("Example {index} target_tokens must be non-empty")]
    EmptyTarget { index: usize },
    #[error("Example {index} attention_mask length {mask_len} does not match input_tokens length {input_len}")]
    AttentionMaskLengthMismatch {
        index: usize,
        input_len: usize,
        mask_len: usize,
    },
    #[error("Example {index} attention_mask value {value} is invalid at position {position}")]
    AttentionMaskValueInvalid {
        index: usize,
        position: usize,
        value: u8,
    },
    #[error("Pad token id {pad_token_id} is out of vocab range (vocab size {vocab_size})")]
    PadTokenOutOfVocab { pad_token_id: u32, vocab_size: usize },
    #[error("Ignore index {ignore_index} is out of vocab range (vocab size {vocab_size})")]
    IgnoreIndexOutOfVocab { ignore_index: i32, vocab_size: usize },
    #[error("Example {index} attention_mask mismatch for pad token at position {position} (token {token}, pad {pad_token_id}, mask {mask_value})")]
    PadTokenMaskMismatch {
        index: usize,
        position: usize,
        token: u32,
        pad_token_id: u32,
        mask_value: u8,
    },
    #[error("Example {index} token {token} exceeds vocab size {vocab_size} in {location}")]
    TokenOutOfVocab {
        index: usize,
        token: u32,
        vocab_size: usize,
        location: TrainingTokenLocation,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn metadata() -> ExampleMetadataV1 {
        ExampleMetadataV1::new("source", 1, "{}", 0)
    }

    #[test]
    fn builds_attention_mask_with_pad_token() {
        let mask = TrainingExampleV1::attention_mask_from_tokens(&[1, 0, 2], 0);
        assert_eq!(mask, vec![1, 0, 1]);
    }

    #[test]
    fn rejects_empty_input() {
        let example = TrainingExampleV1::new(vec![], vec![1], vec![], metadata());
        let contract = TrainingDataContractConfig::new(0, -1);
        let err = validate_training_examples(&[example], 10, &contract).unwrap_err();
        assert_eq!(err, TrainingExampleValidationError::EmptyInput { index: 0 });
    }

    #[test]
    fn rejects_contract_version_mismatch() {
        let example = TrainingExampleV1::new(vec![1], vec![1], vec![1], metadata());
        let contract = TrainingDataContractConfig {
            contract_version: "0.9".to_string(),
            pad_token_id: 0,
            ignore_index: -1,
        };
        let err = validate_training_examples(&[example], 10, &contract).unwrap_err();
        assert_eq!(
            err,
            TrainingExampleValidationError::ContractVersionMismatch {
                expected: TRAINING_DATA_CONTRACT_VERSION.to_string(),
                actual: "0.9".to_string(),
            }
        );
    }

    #[test]
    fn rejects_attention_mask_length_mismatch() {
        let example = TrainingExampleV1::new(vec![1, 2], vec![3], vec![1], metadata());
        let contract = TrainingDataContractConfig::new(0, -1);
        let err = validate_training_examples(&[example], 10, &contract).unwrap_err();
        assert_eq!(
            err,
            TrainingExampleValidationError::AttentionMaskLengthMismatch {
                index: 0,
                input_len: 2,
                mask_len: 1,
            }
        );
    }

    #[test]
    fn rejects_token_out_of_vocab() {
        let example = TrainingExampleV1::new(vec![10], vec![1], vec![1], metadata());
        let contract = TrainingDataContractConfig::new(0, -1);
        let err = validate_training_examples(&[example], 5, &contract).unwrap_err();
        assert_eq!(
            err,
            TrainingExampleValidationError::TokenOutOfVocab {
                index: 0,
                token: 10,
                vocab_size: 5,
                location: TrainingTokenLocation::Input,
            }
        );
    }

    #[test]
    fn rejects_pad_token_out_of_vocab() {
        let example = TrainingExampleV1::new(vec![1], vec![2], vec![1], metadata());
        let contract = TrainingDataContractConfig::new(12, -1);
        let err = validate_training_examples(&[example], 10, &contract).unwrap_err();
        assert_eq!(
            err,
            TrainingExampleValidationError::PadTokenOutOfVocab {
                pad_token_id: 12,
                vocab_size: 10,
            }
        );
    }

    #[test]
    fn training_example_schema_matches_contract() {
        let schema_str = include_str!("../../../docs/contracts/training-example.schema.json");
        let schema: serde_json::Value =
            serde_json::from_str(schema_str).expect("parse training example schema");

        let required = schema
            .get("required")
            .and_then(|v| v.as_array())
            .expect("schema required array");
        let required_fields: HashSet<&str> =
            required.iter().filter_map(|v| v.as_str()).collect();
        for field in ["input_tokens", "target_tokens", "attention_mask", "metadata"] {
            assert!(
                required_fields.contains(field),
                "missing required field {field}"
            );
        }

        let properties = schema
            .get("properties")
            .and_then(|v| v.as_object())
            .expect("schema properties object");

        let input_min = properties
            .get("input_tokens")
            .and_then(|v| v.get("minItems"))
            .and_then(|v| v.as_u64())
            .expect("input_tokens minItems");
        assert_eq!(input_min, 1);

        let target_min = properties
            .get("target_tokens")
            .and_then(|v| v.get("minItems"))
            .and_then(|v| v.as_u64())
            .expect("target_tokens minItems");
        assert_eq!(target_min, 1);

        let mask_values: HashSet<u64> = properties
            .get("attention_mask")
            .and_then(|v| v.get("items"))
            .and_then(|v| v.get("enum"))
            .and_then(|v| v.as_array())
            .expect("attention_mask enum")
            .iter()
            .filter_map(|v| v.as_u64())
            .collect();
        assert_eq!(mask_values, HashSet::from([0, 1]));

        let metadata = properties
            .get("metadata")
            .and_then(|v| v.get("required"))
            .and_then(|v| v.as_array())
            .expect("metadata required");
        let metadata_fields: HashSet<&str> =
            metadata.iter().filter_map(|v| v.as_str()).collect();
        for field in ["source_id", "row_id", "provenance", "created_at_unix_ms"] {
            assert!(metadata_fields.contains(field));
        }
    }
}
