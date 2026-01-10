//! CoreML-aware data pipeline for LoRA training.
//!
//! Provides a deterministic, stateless preparation path that:
//! - Validates input/target pairs against context and vocab.
//! - Scales token ids to [-1, 1] and pads to `hidden_dim`.
//! - Batches examples with a conservative token budget to avoid OOM.
//! - Emits a dataset summary for observability.
use super::limits::DatasetSizeLimits;
use adapteros_types::training::{ExampleMetadataV1, TrainingExampleV1};
type TrainingExample = TrainingExampleV1;
use adapteros_core::{AosError, Result};
use std::collections::HashMap;

/// Input specification for preparing CoreML-friendly tensors.
#[derive(Debug, Clone)]
pub struct CoreMLInputSpec {
    pub hidden_dim: usize,
    pub vocab_size: usize,
    pub context_window: usize,
}

impl CoreMLInputSpec {
    fn validate(&self) -> Result<()> {
        if self.hidden_dim == 0 {
            return Err(AosError::Training(
                "hidden_dim must be greater than zero".to_string(),
            ));
        }
        if self.context_window == 0 {
            return Err(AosError::Training(
                "context_window must be greater than zero".to_string(),
            ));
        }
        if self.vocab_size < 2 {
            return Err(AosError::Training(
                "vocab_size must be at least 2".to_string(),
            ));
        }
        Ok(())
    }

    fn scale_token(&self, token: u32) -> f32 {
        // Map token id to [-1, 1] using vocab_size as denominator.
        let denom = (self.vocab_size.saturating_sub(1)).max(1) as f32;
        ((token as f32) / denom) * 2.0 - 1.0
    }
}

/// Prepared example with CoreML-friendly tensors and masks.
#[derive(Debug, Clone)]
pub struct PreparedExample {
    pub input_tokens: Vec<u32>,
    pub target_tokens: Vec<u32>,
    pub padded_input: Vec<u32>,
    pub padded_target: Vec<u32>,
    pub scaled_input: Vec<f32>,
    pub preprocessed: Option<Vec<f32>>,
    pub input_mask: Vec<u8>,
    pub target_mask: Vec<u8>,
    pub input_len: usize,
    pub target_len: usize,
    pub metadata: ExampleMetadataV1,
}

/// Batch of prepared examples with pre-computed token accounting.
#[derive(Debug, Clone)]
pub struct PreparedBatch {
    pub examples: Vec<PreparedExample>,
    pub tokens: u64,
}

/// Histogram of example lengths (bucketed for observability).
#[derive(Debug, Clone)]
pub struct LengthHistogram {
    pub bucket_size: usize,
    pub buckets: Vec<usize>,
}

/// Dataset-level summary emitted after validation.
#[derive(Debug, Clone)]
pub struct DatasetSummary {
    pub total_examples: usize,
    pub total_tokens: u64,
    pub min_seq_len: usize,
    pub max_seq_len: usize,
    pub avg_seq_len: f32,
    pub length_histogram: LengthHistogram,
}

/// Batching plan used to keep CoreML/ANE within safe token budgets.
#[derive(Debug, Clone)]
pub struct BatchPlan {
    pub effective_batch_size: usize,
    pub max_tokens_per_batch: usize,
    pub sequences_truncated: usize,
    pub sequences_dropped: usize,
}

/// Prepared dataset ready for training.
#[derive(Debug, Clone)]
pub struct PreparedDataset {
    pub examples: Vec<PreparedExample>,
    pub batches: Vec<PreparedBatch>,
    pub summary: DatasetSummary,
    pub batch_plan: BatchPlan,
}

/// Prepare a dataset for CoreML-friendly training.
///
/// - Validates token ranges and context window limits.
/// - Scales tokens to [-1, 1] and pads to `hidden_dim`.
/// - Batches deterministically using `batch_size_hint` and token budget.
pub fn prepare_coreml_dataset(
    examples: &[TrainingExampleV1],
    spec: CoreMLInputSpec,
    batch_size_hint: usize,
    max_tokens_per_batch: Option<usize>,
) -> Result<PreparedDataset> {
    spec.validate()?;

    if examples.is_empty() {
        return Err(AosError::Training(
            "No training examples provided for CoreML pipeline".to_string(),
        ));
    }

    if batch_size_hint == 0 {
        return Err(AosError::Training(
            "batch_size_hint must be greater than zero".to_string(),
        ));
    }

    let limits = DatasetSizeLimits::from_env();
    if examples.len() > limits.max_samples {
        return Err(AosError::Training(format!(
            "Dataset sample count exceeds limit: {} > {}",
            examples.len(),
            limits.max_samples
        )));
    }

    let token_budget = max_tokens_per_batch.unwrap_or_else(|| {
        // Conservative default: batch_size_hint * (input + target) capped by context_window
        batch_size_hint
            .saturating_mul(spec.context_window.saturating_mul(2))
            .max(spec.context_window)
    });

    if token_budget == 0 {
        return Err(AosError::Training(
            "max_tokens_per_batch must be greater than zero".to_string(),
        ));
    }

    let mut prepared_examples = Vec::with_capacity(examples.len());
    let mut total_tokens: u64 = 0;
    let mut min_seq_len = usize::MAX;
    let mut max_seq_len = 0usize;
    let mut lengths: Vec<usize> = Vec::with_capacity(examples.len());

    for (idx, ex) in examples.iter().enumerate() {
        if ex.input_tokens.is_empty() || ex.target_tokens.is_empty() {
            return Err(AosError::Training(format!(
                "Example {} has empty input or target",
                idx
            )));
        }
        if ex.input_tokens.len() > spec.context_window {
            return Err(AosError::Training(format!(
                "Example {} input exceeds context window: {} > {}",
                idx,
                ex.input_tokens.len(),
                spec.context_window
            )));
        }
        if ex.target_tokens.len() > spec.context_window {
            return Err(AosError::Training(format!(
                "Example {} target exceeds context window: {} > {}",
                idx,
                ex.target_tokens.len(),
                spec.context_window
            )));
        }

        // Token range validation
        let max_token = ex
            .input_tokens
            .iter()
            .chain(ex.target_tokens.iter())
            .copied()
            .max()
            .unwrap_or(0);
        if max_token as usize >= spec.vocab_size {
            return Err(AosError::Training(format!(
                "Example {} contains token id {} outside vocab size {}",
                idx, max_token, spec.vocab_size
            )));
        }

        // Pad inputs/targets to hidden_dim for CoreML tensor expectations.
        let mut padded_input = vec![0u32; spec.hidden_dim];
        let mut scaled_input = vec![0.0f32; spec.hidden_dim];
        let mut input_mask = vec![0u8; spec.hidden_dim];
        for (i, tok) in ex.input_tokens.iter().enumerate() {
            padded_input[i] = *tok;
            scaled_input[i] = spec.scale_token(*tok);
        }
        for (i, &mask_value) in ex.attention_mask.iter().enumerate() {
            input_mask[i] = mask_value;
        }

        let mut padded_target = vec![0u32; spec.hidden_dim];
        let mut target_mask = vec![0u8; spec.hidden_dim];
        for (i, tok) in ex.target_tokens.iter().enumerate() {
            padded_target[i] = *tok;
            target_mask[i] = 1;
        }

        let input_len = ex.input_tokens.len();
        let target_len = ex.target_tokens.len();
        min_seq_len = min_seq_len.min(input_len);
        max_seq_len = max_seq_len.max(input_len);
        total_tokens += (input_len + target_len) as u64;
        if total_tokens > limits.max_tokens {
            return Err(AosError::Training(format!(
                "Dataset token count exceeds limit: {} > {}",
                total_tokens, limits.max_tokens
            )));
        }
        lengths.push(input_len);

        prepared_examples.push(PreparedExample {
            input_tokens: ex.input_tokens.clone(),
            target_tokens: ex.target_tokens.clone(),
            padded_input,
            padded_target,
            scaled_input,
            preprocessed: None,
            input_mask,
            target_mask,
            input_len,
            target_len,
            metadata: ex.metadata.clone(),
        });
    }

    let avg_seq_len = total_tokens as f32 / prepared_examples.len() as f32 / 2.0;
    let histogram = build_histogram(&lengths, spec.context_window);

    // Deterministic batching based on token budget.
    let mut batches: Vec<PreparedBatch> = Vec::new();
    let mut current: Vec<PreparedExample> = Vec::new();
    let mut tokens_in_batch: u64 = 0;

    for ex in prepared_examples.into_iter() {
        let ex_tokens = (ex.input_len + ex.target_len) as u64;
        let would_overflow = !current.is_empty()
            && (current.len() >= batch_size_hint
                || tokens_in_batch.saturating_add(ex_tokens) as usize > token_budget);

        if would_overflow {
            batches.push(PreparedBatch {
                tokens: tokens_in_batch,
                examples: current,
            });
            current = Vec::new();
            tokens_in_batch = 0;
        }

        tokens_in_batch += ex_tokens;
        current.push(ex);
    }

    if !current.is_empty() {
        batches.push(PreparedBatch {
            tokens: tokens_in_batch,
            examples: current,
        });
    }

    let summary = DatasetSummary {
        total_examples: batches.iter().map(|b| b.examples.len()).sum(),
        total_tokens,
        min_seq_len: if min_seq_len == usize::MAX {
            0
        } else {
            min_seq_len
        },
        max_seq_len,
        avg_seq_len,
        length_histogram: histogram,
    };

    let batch_plan = BatchPlan {
        effective_batch_size: batch_size_hint,
        max_tokens_per_batch: token_budget,
        sequences_truncated: 0,
        sequences_dropped: 0,
    };

    let flat_examples = batches
        .iter()
        .flat_map(|b| b.examples.clone())
        .collect::<Vec<_>>();

    Ok(PreparedDataset {
        examples: flat_examples,
        batches,
        summary,
        batch_plan,
    })
}

fn build_histogram(lengths: &[usize], context_window: usize) -> LengthHistogram {
    if lengths.is_empty() {
        return LengthHistogram {
            bucket_size: 1,
            buckets: vec![],
        };
    }

    let bucket_size = std::cmp::max(1, context_window / 8);
    let bucket_count = (context_window / bucket_size) + 1;
    let mut buckets = vec![0usize; bucket_count];

    for len in lengths {
        let idx = std::cmp::min(len / bucket_size, bucket_count - 1);
        buckets[idx] += 1;
    }

    LengthHistogram {
        bucket_size,
        buckets,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_types::training::ExampleMetadataV1;

    fn spec() -> CoreMLInputSpec {
        CoreMLInputSpec {
            hidden_dim: 8,
            vocab_size: 32,
            context_window: 8,
        }
    }

    fn make_example(
        input_tokens: Vec<u32>,
        target_tokens: Vec<u32>,
        row_id: u64,
    ) -> TrainingExample {
        let metadata = ExampleMetadataV1::new("test", row_id, "{}", 0);
        let attention_mask = TrainingExample::attention_mask_from_tokens(&input_tokens, 0);
        TrainingExample::new(input_tokens, target_tokens, attention_mask, metadata)
    }

    #[test]
    fn prepare_valid_dataset() {
        let examples = vec![make_example(vec![1, 2, 3], vec![3, 2, 1], 1)];

        let prepared = prepare_coreml_dataset(&examples, spec(), 2, None).unwrap();
        assert_eq!(prepared.summary.total_examples, 1);
        assert_eq!(prepared.summary.total_tokens, 6);
        assert_eq!(prepared.batches.len(), 1);
        assert_eq!(prepared.batches[0].examples.len(), 1);
        assert_eq!(prepared.batches[0].examples[0].scaled_input.len(), 8);
    }

    #[test]
    fn reject_out_of_vocab() {
        let examples = vec![make_example(vec![99], vec![1], 1)];

        let err = prepare_coreml_dataset(&examples, spec(), 1, None).unwrap_err();
        assert!(
            err.to_string().contains("outside vocab"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn reject_context_overflow() {
        let examples = vec![make_example(vec![1, 2, 3, 4, 5, 6, 7, 8, 9], vec![1], 1)];

        let err = prepare_coreml_dataset(&examples, spec(), 1, None).unwrap_err();
        assert!(err.to_string().contains("context window"));
    }

    #[test]
    fn batches_respect_token_budget() {
        let examples = vec![
            make_example(vec![1, 2, 3, 4], vec![1], 1),
            make_example(vec![1, 2, 3, 4], vec![1], 2),
        ];

        // Force one example per batch via token budget.
        let prepared = prepare_coreml_dataset(&examples, spec(), 2, Some(6)).unwrap();
        assert_eq!(prepared.batches.len(), 2);
        assert_eq!(prepared.batch_plan.effective_batch_size, 2);
        assert_eq!(prepared.batch_plan.max_tokens_per_batch, 6);
    }
}
