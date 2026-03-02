//! Canonical training receipt digest computation.
//!
//! This module provides deterministic serialization and digesting for
//! training pipeline receipts, mirroring inference receipt canonicalization.

use crate::B3Hash;
use serde::{Deserialize, Serialize};

/// Canonical training receipt schema version.
pub const TRAINING_RECEIPT_SCHEMA_V1: u8 = 1;
/// Alias for consistency with inference receipt naming.
pub const TRAINING_RECEIPT_DIGEST_SCHEMA_V1: u8 = TRAINING_RECEIPT_SCHEMA_V1;

/// Canonical phase status fields bound into the training receipt digest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct TrainingPhaseDigestStatusV1 {
    pub phase: String,
    pub status: String,
    #[serde(default)]
    pub phase_id: String,
    pub inputs_hash: String,
    pub outputs_hash: String,
}

/// Canonical training receipt digest input.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct TrainingReceiptDigestInputV1 {
    /// Schema version retained for compatibility; digest framing pins schema byte to v1.
    #[serde(default = "default_training_receipt_schema_version")]
    pub schema_version: u8,
    pub pipeline_id: String,
    pub contract_version: u32,
    #[serde(default)]
    pub training_contract_version: Option<String>,
    pub dataset_id: String,
    pub dataset_content_hash: String,
    #[serde(default)]
    pub preprocess_id: Option<String>,
    #[serde(default)]
    pub preprocess_hash: Option<String>,
    pub split_hash: String,
    pub training_config_hash: String,
    pub base_model_hash: String,
    #[serde(default)]
    pub phase_statuses: Vec<TrainingPhaseDigestStatusV1>,
}

fn default_training_receipt_schema_version() -> u8 {
    TRAINING_RECEIPT_SCHEMA_V1
}

impl Default for TrainingReceiptDigestInputV1 {
    fn default() -> Self {
        Self {
            schema_version: TRAINING_RECEIPT_SCHEMA_V1,
            pipeline_id: String::new(),
            contract_version: 1,
            training_contract_version: None,
            dataset_id: String::new(),
            dataset_content_hash: String::new(),
            preprocess_id: None,
            preprocess_hash: None,
            split_hash: String::new(),
            training_config_hash: String::new(),
            base_model_hash: String::new(),
            phase_statuses: Vec::new(),
        }
    }
}

/// Compute the canonical training receipt digest for schema v1.
///
/// The digest is `BLAKE3(canonical_binary_encoding(input))` where the canonical
/// encoding uses fixed field order, length-prefixed strings, and stable
/// ordering for phase status entries.
pub fn compute_training_receipt_digest_v1(input: &TrainingReceiptDigestInputV1) -> B3Hash {
    let canonical = canonical_training_receipt_bytes_v1(input);
    B3Hash::hash(&canonical)
}

/// Serialize a value to canonical JSON with deterministic key ordering.
pub fn canonical_training_receipt_json_string<T: Serialize>(
    value: &T,
) -> Result<String, serde_json::Error> {
    let v = serde_json::to_value(value)?;
    let canonical = canonicalize_json_value(v);
    serde_json::to_string(&canonical)
}

/// Produce canonical bytes for schema v1 training receipt digest input.
pub fn canonical_training_receipt_bytes_v1(input: &TrainingReceiptDigestInputV1) -> Vec<u8> {
    let mut out = Vec::with_capacity(1024);

    out.push(TRAINING_RECEIPT_SCHEMA_V1);
    push_len_prefixed_str(&mut out, &input.pipeline_id);
    out.extend_from_slice(&input.contract_version.to_le_bytes());
    push_len_prefixed_str(
        &mut out,
        input.training_contract_version.as_deref().unwrap_or(""),
    );
    push_len_prefixed_str(&mut out, &input.dataset_id);
    push_len_prefixed_str(&mut out, &input.dataset_content_hash);
    push_len_prefixed_str(&mut out, input.preprocess_id.as_deref().unwrap_or(""));
    push_len_prefixed_str(&mut out, input.preprocess_hash.as_deref().unwrap_or(""));
    push_len_prefixed_str(&mut out, &input.split_hash);
    push_len_prefixed_str(&mut out, &input.training_config_hash);
    push_len_prefixed_str(&mut out, &input.base_model_hash);

    let mut statuses = input.phase_statuses.clone();
    statuses.sort_by(|a, b| {
        phase_ordinal(&a.phase)
            .cmp(&phase_ordinal(&b.phase))
            .then_with(|| a.phase_id.cmp(&b.phase_id))
    });

    out.extend_from_slice(&(statuses.len() as u32).to_le_bytes());
    for status in statuses {
        push_len_prefixed_str(&mut out, &status.phase);
        push_len_prefixed_str(&mut out, &status.status);
        push_len_prefixed_str(&mut out, &status.phase_id);
        push_len_prefixed_str(&mut out, &status.inputs_hash);
        push_len_prefixed_str(&mut out, &status.outputs_hash);
    }

    out
}

fn phase_ordinal(phase: &str) -> u8 {
    match phase {
        "dataset_build" => 0,
        "preprocess" => 1,
        "train_validation_split" => 2,
        "training_loop" => 3,
        "validation_early_stopping" => 4,
        "packaging" => 5,
        "complete" => 6,
        _ => u8::MAX,
    }
}

fn push_len_prefixed_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn canonicalize_json_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut entries: Vec<(String, serde_json::Value)> = map
                .into_iter()
                .map(|(k, v)| (k, canonicalize_json_value(v)))
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let mut ordered = serde_json::Map::with_capacity(entries.len());
            for (k, v) in entries {
                ordered.insert(k, v);
            }
            serde_json::Value::Object(ordered)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.into_iter().map(canonicalize_json_value).collect())
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_input() -> TrainingReceiptDigestInputV1 {
        TrainingReceiptDigestInputV1 {
            schema_version: TRAINING_RECEIPT_SCHEMA_V1,
            pipeline_id: "pipeline-1".to_string(),
            contract_version: 1,
            training_contract_version: Some("1.0".to_string()),
            dataset_id: "dataset-1".to_string(),
            dataset_content_hash: "dataset-hash".to_string(),
            preprocess_id: Some("pre-1".to_string()),
            preprocess_hash: Some("pre-hash".to_string()),
            split_hash: "split-hash".to_string(),
            training_config_hash: "cfg-hash".to_string(),
            base_model_hash: "base-hash".to_string(),
            phase_statuses: vec![
                TrainingPhaseDigestStatusV1 {
                    phase: "training_loop".to_string(),
                    status: "completed".to_string(),
                    phase_id: "phase-2".to_string(),
                    inputs_hash: "in-2".to_string(),
                    outputs_hash: "out-2".to_string(),
                },
                TrainingPhaseDigestStatusV1 {
                    phase: "dataset_build".to_string(),
                    status: "completed".to_string(),
                    phase_id: "phase-1".to_string(),
                    inputs_hash: "in-1".to_string(),
                    outputs_hash: "out-1".to_string(),
                },
            ],
        }
    }

    #[test]
    fn digest_is_stable_for_equivalent_phase_orderings() {
        let a = sample_input();
        let mut b = sample_input();
        b.phase_statuses.reverse();

        assert_eq!(
            compute_training_receipt_digest_v1(&a),
            compute_training_receipt_digest_v1(&b)
        );
    }

    #[test]
    fn digest_changes_when_bound_field_changes() {
        let a = sample_input();
        let mut b = sample_input();
        b.dataset_content_hash = "changed".to_string();

        assert_ne!(
            compute_training_receipt_digest_v1(&a),
            compute_training_receipt_digest_v1(&b)
        );
    }

    #[test]
    fn digest_normalizes_none_and_empty_optional_strings() {
        let mut a = sample_input();
        let mut b = sample_input();
        a.preprocess_id = None;
        b.preprocess_id = Some(String::new());
        a.training_contract_version = None;
        b.training_contract_version = Some(String::new());

        assert_eq!(
            compute_training_receipt_digest_v1(&a),
            compute_training_receipt_digest_v1(&b)
        );
    }
}
