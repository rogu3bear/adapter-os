# Phase 1: Unblock the Token Bottleneck

## Problem

`crates/adapteros-lora-worker/src/training/loader.rs` lines 25-27:

```rust
const MAX_INPUT_TOKENS: usize = 256;
const MAX_TARGET_TOKENS: usize = 128;
const STRIDE_TOKENS: usize = 256;
```

These are hard-coded constants. The average Rust function in this codebase is
30-80 lines, which is 200-600 tokens. The average struct with impl block is
100-300 lines (700-2000 tokens). With 256 input tokens, you can fit about
15 lines of Rust — not enough for a single non-trivial function.

The `TrainingConfig` already has `max_seq_length: Option<u32>` (default 2048),
but the loader ignores it and truncates at 256/128.

## Approach

Follow the existing pattern in `limits.rs` where dataset size limits are
configurable via environment variables:

```rust
pub const DEFAULT_MAX_FILES: usize = 1000;
// ...
pub fn from_env() -> Self {
    Self {
        max_files: parse_env_usize("AOS_DATASET_MAX_FILES", DEFAULT_MAX_FILES),
```

Apply the same pattern to the framing constants.

## Changes

### 1. `crates/adapteros-lora-worker/src/training/loader.rs`

Replace hard-coded constants with configurable values derived from TrainingConfig
or environment variables:

```rust
// Default framing constants (overridable via TrainingConfig.max_seq_length or env).
const DEFAULT_MAX_INPUT_TOKENS: usize = 256;
const DEFAULT_MAX_TARGET_TOKENS: usize = 128;
const DEFAULT_STRIDE_TOKENS: usize = 256;

/// Framing constants, resolved from config or environment.
struct FramingConfig {
    max_input_tokens: usize,
    max_target_tokens: usize,
    stride_tokens: usize,
}

impl FramingConfig {
    fn from_env_or_default() -> Self {
        Self {
            max_input_tokens: parse_env_usize(
                "AOS_LOADER_MAX_INPUT_TOKENS",
                DEFAULT_MAX_INPUT_TOKENS,
            ),
            max_target_tokens: parse_env_usize(
                "AOS_LOADER_MAX_TARGET_TOKENS",
                DEFAULT_MAX_TARGET_TOKENS,
            ),
            stride_tokens: parse_env_usize(
                "AOS_LOADER_STRIDE_TOKENS",
                DEFAULT_STRIDE_TOKENS,
            ),
        }
    }

    fn from_training_config(config: &TrainingConfig) -> Self {
        let base = Self::from_env_or_default();
        match config.max_seq_length {
            Some(max_seq) if max_seq > 0 => {
                let max_seq = max_seq as usize;
                // Split sequence budget: 2/3 input, 1/3 target
                Self {
                    max_input_tokens: (max_seq * 2 / 3).max(base.max_input_tokens),
                    max_target_tokens: (max_seq / 3).max(base.max_target_tokens),
                    stride_tokens: (max_seq * 2 / 3).max(base.stride_tokens),
                }
            }
            _ => base,
        }
    }
}
```

### 2. Update `load_examples_with_encoder` and `load_examples_from_manifest`

Add a `FramingConfig` parameter (or derive it inside the function from an
optional `TrainingConfig` parameter). The existing callers in the codebase
ingestion pipeline already have access to `TrainingConfig`.

### 3. `crates/adapteros-lora-worker/src/training/limits.rs`

Add the `parse_env_usize` function if it's not already public, or make the
existing one `pub(crate)`.

## Existing Code to Reuse

- `DatasetSizeLimits::from_env()` pattern in `limits.rs`
- `TrainingConfig.max_seq_length` field already exists
- The `load_examples_with_encoder` already accepts arbitrary encoders

## Tests

1. Existing tests in `loader.rs` must continue passing with default constants
2. New test: set `AOS_LOADER_MAX_INPUT_TOKENS=1024` via env, verify loader
   respects it
3. New test: pass `TrainingConfig` with `max_seq_length=2048`, verify framing
   uses 1365/683 split
4. New test: verify raw_continuation framing produces chunks of correct size
   with larger limits

## Verification

```bash
cargo test -p adapteros-lora-worker -- loader
cargo check -p adapteros-lora-worker
```

## Risk

Low. This is a configuration change that defaults to current behavior. No
existing behavior changes unless the caller explicitly opts in.

## Hours: 40

- Implementation: 16h
- Tests: 12h
- Integration with codebase ingestion caller: 8h
- Review and edge cases: 4h
