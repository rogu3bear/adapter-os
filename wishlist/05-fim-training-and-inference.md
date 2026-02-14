# Phase 5: FIM Training and Inference

## Problem

The current inference pipeline is chat-only. It applies chat templates
(ChatML/Llama/Mistral) and generates autoregressive completions. There is
no Fill-in-the-Middle (FIM) support for code completion at cursor position.

For a self-writing system, FIM is essential: the model needs to fill in
function bodies given surrounding context, not just respond to chat prompts.

## Background: FIM in Qwen2.5

Qwen2.5 models natively support FIM with these special tokens:
- `<|fim_prefix|>` (token ID varies by model)
- `<|fim_suffix|>` (token ID varies by model)
- `<|fim_middle|>` (token ID varies by model)

The FIM format:
```
<|fim_prefix|>{code before cursor}<|fim_suffix|>{code after cursor}<|fim_middle|>{generated code}
```

This is already supported by the Qwen2.5-7B tokenizer that AdapterOS uses.

## Approach

### Training Side

Phase 2 already generates FIM training pairs (Strategy 4). The training
pipeline needs no changes — FIM pairs are just prompt/completion pairs where
the prompt contains FIM tokens and the completion is the middle section.

What we need: ensure the tokenizer correctly encodes FIM special tokens
and that the loader doesn't strip them.

### Inference Side

Add a FIM inference path alongside the existing chat inference path.

## Changes

### 1. FIM Token Resolution

In `crates/adapteros-lora-plan/src/config.rs` or the tokenizer wrapper:

```rust
pub struct FIMTokens {
    pub prefix_id: u32,
    pub suffix_id: u32,
    pub middle_id: u32,
}

impl FIMTokens {
    pub fn resolve(tokenizer: &Tokenizer) -> Option<Self> {
        let prefix = tokenizer.token_to_id("<|fim_prefix|>")?;
        let suffix = tokenizer.token_to_id("<|fim_suffix|>")?;
        let middle = tokenizer.token_to_id("<|fim_middle|>")?;
        Some(Self {
            prefix_id: prefix,
            suffix_id: suffix,
            middle_id: middle,
        })
    }
}
```

### 2. FIM Request Type

In `crates/adapteros-api-types/src/inference.rs`:

```rust
pub struct FIMRequest {
    pub prefix: String,
    pub suffix: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub stop_sequences: Vec<String>,
    pub adapter_id: Option<String>,
}
```

### 3. FIM Prompt Builder

In the inference pipeline, add a FIM prompt path:

```rust
fn build_fim_prompt(
    tokenizer: &Tokenizer,
    fim_tokens: &FIMTokens,
    prefix: &str,
    suffix: &str,
) -> Vec<u32> {
    let mut tokens = Vec::new();
    tokens.push(fim_tokens.prefix_id);
    tokens.extend(tokenizer.encode(prefix, false).unwrap().get_ids());
    tokens.push(fim_tokens.suffix_id);
    tokens.extend(tokenizer.encode(suffix, false).unwrap().get_ids());
    tokens.push(fim_tokens.middle_id);
    tokens
}
```

### 4. FIM Stop Conditions

FIM generation should stop on:
- End of text token
- `<|fim_prefix|>` token (start of next FIM block)
- `<|endoftext|>` token
- Closing brace that balances the opening context

### 5. FIM API Endpoint

Add `/v1/fim/completions` endpoint (or extend `/v1/completions`):

```rust
async fn fim_completions(
    State(state): State<AppState>,
    Json(request): Json<FIMRequest>,
) -> Result<Json<FIMResponse>> {
    // Build FIM prompt
    // Route to adapter (existing router)
    // Generate with FIM stop conditions
    // Return generated middle section
}
```

### 6. FIM Training Data Validation

Ensure the loader doesn't strip FIM tokens. The current loader's
`raw_continuation_v1` schema already handles arbitrary token sequences.
For `supervised` schema, verify that FIM tokens in prompts survive encoding.

## Existing Code to Reuse

- `QwenTokenizer` — already wraps the tokenizer, add FIM token methods
- `InferencePipeline` — add FIM path alongside chat path
- `StopController` — add FIM-specific stop conditions
- `AdapterRouter` — routing works the same for FIM requests
- Streaming infrastructure — SSE streaming works for FIM too

## Test Plan

1. Tokenizer correctly resolves FIM special tokens for Qwen2.5
2. FIM prompt builder produces correct token sequence
3. FIM stop conditions terminate generation appropriately
4. FIM training pairs round-trip through tokenizer without corruption
5. End-to-end: FIM request → adapter selection → generation → response
6. FIM generation respects brace balancing

## Verification

```bash
cargo test -p adapteros-lora-worker -- fim
cargo test -p adapteros-lora-plan -- fim
cargo test -p adapteros-api-types -- fim
cargo check -p adapteros-server-api
```

## Hours: 160

- FIM token resolution: 8h
- FIM request types: 8h
- FIM prompt builder: 16h
- FIM stop conditions: 24h
- FIM API endpoint: 24h
- FIM training data validation: 16h
- Integration with inference pipeline: 32h
- Tests: 24h
- Edge cases (multi-file context, large prefixes): 8h
