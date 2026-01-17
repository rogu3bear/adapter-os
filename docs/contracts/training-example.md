# Training Example Contract (v1)

This contract defines the canonical, tokenized training example used across
worker, server, CLI, and stored artifacts.

## Version
- Contract version: `1.0`
- Source of truth: `crates/adapteros-types/src/training/example.rs`
- JSON schema: `docs/contracts/training-example.schema.json`

## Types

### TrainingExampleV1
- `input_tokens: Vec<u32>`
- `target_tokens: Vec<u32>`
- `attention_mask: Vec<u8>`
- `metadata: ExampleMetadataV1`

### ExampleMetadataV1
- `dataset_id: String`
- `row_id: u64`
- `source_hash: String`
- `provenance: String`
- `created_at_unix_ms: u64`

### TrainingDataContractConfig
- `contract_version: String`
- `pad_token_id: u32`
- `ignore_index: i32`

## Invariants
- `input_tokens.len() > 0`
- `target_tokens.len() > 0`
- `attention_mask.len() == input_tokens.len()`
- `attention_mask` values must be `0` or `1`
- Tokens must be `< vocab_size`
- `pad_token_id` must be `< vocab_size`
- `ignore_index` must be `-1` or `< vocab_size`
- `attention_mask[i] == 0` iff `input_tokens[i] == pad_token_id`

## Provenance
- `provenance` is a canonical JSON string (sorted keys recommended).
- Store any auxiliary metadata in the provenance JSON payload.

## Required Version Embedding
The training data contract version MUST be recorded in:
- Dataset manifests
- Preprocessing manifests
- Training receipts
- Training checkpoints

Mismatched contract versions are rejected at runtime.
