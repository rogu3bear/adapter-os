# PLAN_4

## Purpose
Define the only supported dataset-to-training path. The goal is deterministic framing with recorded provenance, yielding adapters with verifiable metadata.

## Accepted Dataset Schemas (Locked)
- Supervised JSONL: { "prompt": "string", "completion": "string" }
  - Both fields required, UTF-8, non-empty.
- Raw JSONL: { "text": "string" }
  - Field required, UTF-8, non-empty.

Any other schema must be rejected with a clear error.

## Framing Rules (Locked)
- Supervised:
  - input = prompt
  - target = completion
- Raw (raw_continuation_v1):
  - Tokenize full text.
  - Split into fixed chunks.
  - For chunk i:
    - input_tokens = tokens[i : i + MAX_INPUT_TOKENS]
    - target_tokens = tokens[i + MAX_INPUT_TOKENS : i + MAX_INPUT_TOKENS + MAX_TARGET_TOKENS]

Locked constants (do not make configurable):
- MAX_INPUT_TOKENS = 256
- MAX_TARGET_TOKENS = 128
- STRIDE_TOKENS = 256

If text is too short, drop the row with an explicit warning.

## Tokenization Rules (Locked)
- Tokenizer must come from the base model directory.
- Tokenization must be deterministic.
- Tokenizer hash must be recorded.
- Tokenizer/base model mismatch must hard-fail training.

## TrainingExampleV1 Contract (Locked)
Each produced example must include:
- input_tokens
- target_tokens
- attention_mask
- metadata:
  - dataset_id
  - row_id
  - source_hash (hash of raw row)

Invariants:
- len(input_tokens) > 0
- len(target_tokens) > 0
- Mask length aligns with tokens.

Violations must fail early with clear errors.

## Determinism and Provenance (Locked)
Training must record:
- Dataset hash
- Framing policy identifier (supervised or raw_continuation_v1)
- Tokenizer hash
- Training config hash
- Determinism tier compatibility

These must be embedded in adapter metadata.

## Non-Goals (Explicit)
PLAN_4 does not:
- Support instruction synthesis
- Support multi-turn framing
- Support custom framing knobs
- Optimize training quality
- Support unsupervised objectives
