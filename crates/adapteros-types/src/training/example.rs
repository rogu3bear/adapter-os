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
    /// Dataset identifier for the example source.
    #[serde(alias = "source_id")]
    pub dataset_id: String,
    /// Stable row identifier within the source.
    pub row_id: u64,
    /// Hash of the raw row payload.
    pub source_hash: String,
    /// Provenance payload (canonical JSON string recommended).
    pub provenance: String,
    /// Created-at timestamp in Unix milliseconds.
    pub created_at_unix_ms: u64,
}

impl ExampleMetadataV1 {
    /// Create new example metadata.
    pub fn new(
        dataset_id: impl Into<String>,
        row_id: u64,
        source_hash: impl Into<String>,
        provenance: impl Into<String>,
        created_at_unix_ms: u64,
    ) -> Self {
        Self {
            dataset_id: dataset_id.into(),
            row_id,
            source_hash: source_hash.into(),
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
    /// Build a contract config using the current contract version.
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
    /// Contract version used for validation.
    pub contract_version: String,
    /// Pad token ID used for attention masks.
    pub pad_token_id: u32,
    /// Ignore index used for loss masking.
    pub ignore_index: i32,
    /// Total examples validated.
    pub total_examples: usize,
    /// Total tokens across the batch.
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
    if example.metadata.dataset_id.trim().is_empty() {
        return Err(TrainingExampleValidationError::MissingDatasetId { index });
    }
    if example.metadata.source_hash.trim().is_empty() {
        return Err(TrainingExampleValidationError::MissingSourceHash { index });
    }
    if example.input_tokens.is_empty() {
        return Err(TrainingExampleValidationError::EmptyInput { index });
    }
    if example.target_tokens.is_empty() {
        return Err(TrainingExampleValidationError::EmptyTarget { index });
    }
    if example.attention_mask.len() != example.input_tokens.len() {
        return Err(
            TrainingExampleValidationError::AttentionMaskLengthMismatch {
                index,
                input_len: example.input_tokens.len(),
                mask_len: example.attention_mask.len(),
            },
        );
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
pub fn provenance_from_map(
    map: &BTreeMap<String, serde_json::Value>,
) -> Result<String, serde_json::Error> {
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
    dataset_id: impl Into<String>,
    row_id: u64,
    source_hash: impl Into<String>,
    created_at_unix_ms: u64,
    pairs: I,
) -> Result<ExampleMetadataV1, serde_json::Error>
where
    I: IntoIterator<Item = (String, String)>,
{
    let provenance = provenance_from_pairs(pairs)?;
    Ok(ExampleMetadataV1::new(
        dataset_id,
        row_id,
        source_hash,
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

/// Extract sample role from provenance JSON (if present).
///
/// Returns the sample_role field (e.g., "knowledge", "abstention", "negative").
/// Used to classify examples for separated training.
pub fn sample_role_from_provenance(provenance: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(provenance).ok()?;
    value
        .get("sample_role")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Extract sample role from metadata provenance (if present).
pub fn sample_role_from_metadata(metadata: &ExampleMetadataV1) -> Option<String> {
    sample_role_from_provenance(&metadata.provenance)
}

// =============================================================================
// Preference Pairs (Contrastive Training)
// =============================================================================

/// Default preference margin for preference pairs.
fn default_margin() -> f32 {
    1.0
}

/// A preference pair for contrastive/DPO-style training.
///
/// Represents a single preference comparison where `chosen` response is
/// preferred over `rejected` response for the given `prompt`. Used for
/// Direct Preference Optimization (DPO) and similar preference-based methods.
///
/// # Patent Alignment
///
/// Supports "positive and negative conditions" for verifiable preference
/// as required by the deterministic inference patent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "server", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub struct PreferencePairV1 {
    /// Shared prompt/context token IDs for both responses.
    pub prompt_tokens: Vec<u32>,
    /// Chosen (preferred) response token IDs.
    pub chosen_tokens: Vec<u32>,
    /// Rejected (non-preferred) response token IDs.
    pub rejected_tokens: Vec<u32>,
    /// Preference margin indicating strength of preference.
    /// Higher margin = stronger preference. Default 1.0.
    #[serde(default = "default_margin")]
    pub margin: f32,
    /// Attention mask for prompt tokens (1 = real token, 0 = pad).
    pub prompt_attention_mask: Vec<u8>,
    /// Canonical metadata payload.
    pub metadata: ExampleMetadataV1,
}

impl PreferencePairV1 {
    /// Create a new preference pair.
    pub fn new(
        prompt_tokens: Vec<u32>,
        chosen_tokens: Vec<u32>,
        rejected_tokens: Vec<u32>,
        margin: f32,
        prompt_attention_mask: Vec<u8>,
        metadata: ExampleMetadataV1,
    ) -> Self {
        Self {
            prompt_tokens,
            chosen_tokens,
            rejected_tokens,
            margin,
            prompt_attention_mask,
            metadata,
        }
    }

    /// Create a preference pair and derive the attention mask from pad token ID.
    pub fn with_pad_token(
        prompt_tokens: Vec<u32>,
        chosen_tokens: Vec<u32>,
        rejected_tokens: Vec<u32>,
        margin: f32,
        pad_token_id: u32,
        metadata: ExampleMetadataV1,
    ) -> Self {
        let prompt_attention_mask =
            TrainingExampleV1::attention_mask_from_tokens(&prompt_tokens, pad_token_id);
        Self::new(
            prompt_tokens,
            chosen_tokens,
            rejected_tokens,
            margin,
            prompt_attention_mask,
            metadata,
        )
    }
}

/// Unified training example supporting both SFT and preference-based training.
///
/// This enum allows a single dataset to contain mixed example types,
/// enabling hybrid training strategies.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "server", derive(schemars::JsonSchema))]
#[serde(tag = "example_type", rename_all = "snake_case")]
pub enum TrainingExample {
    /// Supervised fine-tuning example (input → target).
    Sft(TrainingExampleV1),
    /// Preference pair for contrastive training (prompt → chosen vs rejected).
    Preference(PreferencePairV1),
}

impl TrainingExample {
    /// Get the metadata for this example.
    pub fn metadata(&self) -> &ExampleMetadataV1 {
        match self {
            TrainingExample::Sft(ex) => &ex.metadata,
            TrainingExample::Preference(pair) => &pair.metadata,
        }
    }

    /// Get the dataset ID for this example.
    pub fn dataset_id(&self) -> &str {
        &self.metadata().dataset_id
    }

    /// Back-compat accessor for source_id callers.
    pub fn source_id(&self) -> &str {
        self.dataset_id()
    }

    /// Get the row ID for this example.
    pub fn row_id(&self) -> u64 {
        self.metadata().row_id
    }
}

/// Validation failures specific to preference pairs.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum PreferencePairValidationError {
    /// Prompt tokens were empty.
    #[error("Preference pair {index} prompt_tokens must be non-empty")]
    EmptyPrompt {
        /// Pair index.
        index: usize,
    },
    /// Chosen tokens were empty.
    #[error("Preference pair {index} chosen_tokens must be non-empty")]
    EmptyChosen {
        /// Pair index.
        index: usize,
    },
    /// Rejected tokens were empty.
    #[error("Preference pair {index} rejected_tokens must be non-empty")]
    EmptyRejected {
        /// Pair index.
        index: usize,
    },
    /// Margin must be positive.
    #[error("Preference pair {index} margin must be positive, got {margin}")]
    InvalidMargin {
        /// Pair index.
        index: usize,
        /// Invalid margin value.
        margin: f32,
    },
    /// Attention mask length mismatched prompt length.
    #[error("Preference pair {index} prompt_attention_mask length {mask_len} does not match prompt_tokens length {prompt_len}")]
    AttentionMaskLengthMismatch {
        /// Pair index.
        index: usize,
        /// Prompt token length.
        prompt_len: usize,
        /// Attention mask length.
        mask_len: usize,
    },
    /// Token exceeds the vocab size.
    #[error("Preference pair {index} token {token} exceeds vocab size {vocab_size} in {location}")]
    TokenOutOfVocab {
        /// Pair index.
        index: usize,
        /// Token ID that exceeded vocab.
        token: u32,
        /// Vocabulary size.
        vocab_size: usize,
        /// Location of the token.
        location: PreferenceTokenLocation,
    },
}

/// Location of a token sequence in a preference pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreferenceTokenLocation {
    /// Tokens from the prompt sequence.
    Prompt,
    /// Tokens from the chosen response.
    Chosen,
    /// Tokens from the rejected response.
    Rejected,
}

impl std::fmt::Display for PreferenceTokenLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PreferenceTokenLocation::Prompt => f.write_str("prompt_tokens"),
            PreferenceTokenLocation::Chosen => f.write_str("chosen_tokens"),
            PreferenceTokenLocation::Rejected => f.write_str("rejected_tokens"),
        }
    }
}

/// Validate a single preference pair against contract invariants.
pub fn validate_preference_pair(
    pair: &PreferencePairV1,
    index: usize,
    vocab_size: usize,
) -> Result<(), PreferencePairValidationError> {
    // Check non-empty sequences
    if pair.prompt_tokens.is_empty() {
        return Err(PreferencePairValidationError::EmptyPrompt { index });
    }
    if pair.chosen_tokens.is_empty() {
        return Err(PreferencePairValidationError::EmptyChosen { index });
    }
    if pair.rejected_tokens.is_empty() {
        return Err(PreferencePairValidationError::EmptyRejected { index });
    }

    // Check margin is positive
    if pair.margin <= 0.0 {
        return Err(PreferencePairValidationError::InvalidMargin {
            index,
            margin: pair.margin,
        });
    }

    // Check attention mask length
    if pair.prompt_attention_mask.len() != pair.prompt_tokens.len() {
        return Err(PreferencePairValidationError::AttentionMaskLengthMismatch {
            index,
            prompt_len: pair.prompt_tokens.len(),
            mask_len: pair.prompt_attention_mask.len(),
        });
    }

    // Check vocab bounds for all token sequences
    if let Some(&token) = pair
        .prompt_tokens
        .iter()
        .find(|&&t| t as usize >= vocab_size)
    {
        return Err(PreferencePairValidationError::TokenOutOfVocab {
            index,
            token,
            vocab_size,
            location: PreferenceTokenLocation::Prompt,
        });
    }
    if let Some(&token) = pair
        .chosen_tokens
        .iter()
        .find(|&&t| t as usize >= vocab_size)
    {
        return Err(PreferencePairValidationError::TokenOutOfVocab {
            index,
            token,
            vocab_size,
            location: PreferenceTokenLocation::Chosen,
        });
    }
    if let Some(&token) = pair
        .rejected_tokens
        .iter()
        .find(|&&t| t as usize >= vocab_size)
    {
        return Err(PreferencePairValidationError::TokenOutOfVocab {
            index,
            token,
            vocab_size,
            location: PreferenceTokenLocation::Rejected,
        });
    }

    Ok(())
}

/// Validation failures for training example batches.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TrainingExampleValidationError {
    /// No examples were provided.
    #[error("Training example batch is empty")]
    EmptyBatch,
    /// Contract version mismatch.
    #[error("Training contract version mismatch: expected {expected}, got {actual}")]
    ContractVersionMismatch {
        /// Expected contract version.
        expected: String,
        /// Actual contract version.
        actual: String,
    },
    /// Dataset id missing from metadata.
    #[error("Example {index} metadata.dataset_id must be non-empty")]
    MissingDatasetId {
        /// Example index.
        index: usize,
    },
    /// Source hash missing from metadata.
    #[error("Example {index} metadata.source_hash must be non-empty")]
    MissingSourceHash {
        /// Example index.
        index: usize,
    },
    /// Input tokens were empty.
    #[error("Example {index} input_tokens must be non-empty")]
    EmptyInput {
        /// Example index.
        index: usize,
    },
    /// Target tokens were empty.
    #[error("Example {index} target_tokens must be non-empty")]
    EmptyTarget {
        /// Example index.
        index: usize,
    },
    /// Attention mask length mismatched input length.
    #[error("Example {index} attention_mask length {mask_len} does not match input_tokens length {input_len}")]
    AttentionMaskLengthMismatch {
        /// Example index.
        index: usize,
        /// Input token length.
        input_len: usize,
        /// Attention mask length.
        mask_len: usize,
    },
    /// Attention mask contains an invalid value.
    #[error("Example {index} attention_mask value {value} is invalid at position {position}")]
    AttentionMaskValueInvalid {
        /// Example index.
        index: usize,
        /// Token position with invalid value.
        position: usize,
        /// Invalid attention mask value.
        value: u8,
    },
    /// Pad token ID is outside the vocab range.
    #[error("Pad token id {pad_token_id} is out of vocab range (vocab size {vocab_size})")]
    PadTokenOutOfVocab {
        /// Pad token ID.
        pad_token_id: u32,
        /// Vocabulary size.
        vocab_size: usize,
    },
    /// Ignore index is outside the vocab range.
    #[error("Ignore index {ignore_index} is out of vocab range (vocab size {vocab_size})")]
    IgnoreIndexOutOfVocab {
        /// Ignore index value.
        ignore_index: i32,
        /// Vocabulary size.
        vocab_size: usize,
    },
    /// Attention mask does not match pad token positions.
    #[error("Example {index} attention_mask mismatch for pad token at position {position} (token {token}, pad {pad_token_id}, mask {mask_value})")]
    PadTokenMaskMismatch {
        /// Example index.
        index: usize,
        /// Token position.
        position: usize,
        /// Token ID at the position.
        token: u32,
        /// Pad token ID.
        pad_token_id: u32,
        /// Attention mask value.
        mask_value: u8,
    },
    /// Token exceeds the vocab size.
    #[error("Example {index} token {token} exceeds vocab size {vocab_size} in {location}")]
    TokenOutOfVocab {
        /// Example index.
        index: usize,
        /// Token ID that exceeded vocab.
        token: u32,
        /// Vocabulary size.
        vocab_size: usize,
        /// Location of the token (input or target).
        location: TrainingTokenLocation,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn metadata() -> ExampleMetadataV1 {
        ExampleMetadataV1::new("dataset", 1, "row-hash", "{}", 0)
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
    fn rejects_missing_dataset_id() {
        let metadata = ExampleMetadataV1::new("", 1, "row-hash", "{}", 0);
        let example = TrainingExampleV1::new(vec![1], vec![1], vec![1], metadata);
        let contract = TrainingDataContractConfig::new(0, -1);
        let err = validate_training_examples(&[example], 10, &contract).unwrap_err();
        assert_eq!(
            err,
            TrainingExampleValidationError::MissingDatasetId { index: 0 }
        );
    }

    #[test]
    fn rejects_missing_source_hash() {
        let metadata = ExampleMetadataV1::new("dataset", 1, "", "{}", 0);
        let example = TrainingExampleV1::new(vec![1], vec![1], vec![1], metadata);
        let contract = TrainingDataContractConfig::new(0, -1);
        let err = validate_training_examples(&[example], 10, &contract).unwrap_err();
        assert_eq!(
            err,
            TrainingExampleValidationError::MissingSourceHash { index: 0 }
        );
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
        let schema_str = include_str!("../../../../docs/contracts/training-example.schema.json");
        let schema: serde_json::Value =
            serde_json::from_str(schema_str).expect("parse training example schema");

        let required = schema
            .get("required")
            .and_then(|v| v.as_array())
            .expect("schema required array");
        let required_fields: HashSet<&str> = required.iter().filter_map(|v| v.as_str()).collect();
        for field in [
            "input_tokens",
            "target_tokens",
            "attention_mask",
            "metadata",
        ] {
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
        let metadata_fields: HashSet<&str> = metadata.iter().filter_map(|v| v.as_str()).collect();
        for field in [
            "dataset_id",
            "row_id",
            "source_hash",
            "provenance",
            "created_at_unix_ms",
        ] {
            assert!(metadata_fields.contains(field));
        }
    }
}
