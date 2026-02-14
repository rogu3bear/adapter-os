# Phase 3: Ungate and Rewire train-from-code

## Problem

`crates/adapteros-cli/src/commands/train_from_code.rs` line 214:

```rust
fn validate(&self) -> Result<()> {
    Err(AosError::Validation(
        "train-from-code is disabled by PLAN_4; use JSONL datasets only".to_string(),
    ))
}
```

The command exists, is fully wired (CLI args, tokenizer resolution, ingestion
pipeline, adapter registration), but `validate()` always returns an error.
The rest of `execute()` after `validate()` is live, tested code.

## Approach

Replace the PLAN_4 gate with real validation. The infrastructure behind the
gate is already built and tested — `CodebaseIngestion::ingest_and_train()`
works end-to-end.

## Changes

### 1. Replace validate() with real validation

```rust
fn validate(&self) -> Result<()> {
    if !self.repo.exists() {
        return Err(AosError::Validation(format!(
            "Repository path does not exist: {}", self.repo.display()
        )));
    }
    if self.adapter_id.is_empty() {
        return Err(AosError::Validation(
            "Adapter ID must not be empty".to_string()
        ));
    }
    if self.common.rank == 0 || self.common.rank > 256 {
        return Err(AosError::Validation(format!(
            "LoRA rank must be 1-256, got {}", self.common.rank
        )));
    }
    if self.common.learning_rate <= 0.0 || self.common.learning_rate > 1.0 {
        return Err(AosError::Validation(format!(
            "Learning rate must be (0, 1], got {}", self.common.learning_rate
        )));
    }
    Ok(())
}
```

### 2. Wire Phase 2 code generation strategies

Update `IngestionConfig` to accept the new `CodeTrainingStrategy`:

```rust
pub struct IngestionConfig {
    pub training_config: TrainingConfig,
    pub training_strategy: CodeTrainingStrategy, // NEW
    // ...existing fields...
}
```

Update `CodebaseIngestion::generate_qa_pairs()` to dispatch to the new
code-aware generator when the strategy is not QA-only.

### 3. Add --strategy CLI flag

```rust
/// Training data generation strategy
#[arg(long, default_value = "all")]
pub strategy: String, // "signature_to_body", "context", "docstring", "fim", "all"
```

### 4. Update training config defaults for code

The `train-from-code` command should use the `codebase-specific` template
by default (rank=24, alpha=48, 4 epochs) and set `max_seq_length=2048`
to work with the Phase 1 token limit changes.

### 5. PLAN_4 audit

Search for other PLAN_4 gates and document which ones remain gated vs ungated:

```bash
grep -r "PLAN_4\|Plan.4\|plan_4\|plan 4" crates/
```

The `train-docs` command has the same gate. It should remain gated until
Phase 2's data generation proves itself on code first.

## Existing Code to Reuse

- The entire `execute()` method after `validate()` is already correct
- `CodebaseIngestion::ingest_and_train()` works end-to-end
- `AdapterPackager::package_aos_with_metadata()` packages adapters
- `LoRAQuantizer::quantize_to_q15()` quantizes weights
- `QwenTokenizer::from_file()` handles tokenizer loading
- `adapteros_config::resolve_tokenizer_path()` resolves tokenizer

## Tests

1. `validate()` accepts valid args
2. `validate()` rejects missing repo, empty adapter_id, bad rank, bad LR
3. Integration test: small fixture repo → trained adapter (if CI has tokenizer)
4. PLAN_4 gate removed — verify command runs past validate()

## Verification

```bash
cargo test -p adapteros-cli -- train_from_code
cargo check -p adapteros-cli
```

## Hours: 60

- Replace validate(): 4h
- Wire Phase 2 strategies: 20h
- CLI flag and defaults: 8h
- PLAN_4 audit: 4h
- Tests: 16h
- Integration testing: 8h
