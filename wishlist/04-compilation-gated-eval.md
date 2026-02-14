# Phase 4: Compilation-Gated Eval Harness

## Problem

The existing eval harness (`adapteros-retrieval/src/eval.rs`) measures retrieval
quality (Recall@K, nDCG@K, MRR). This tells us if the RAG system finds the
right code chunks, but not if a trained adapter can **generate correct code**.

For a self-writing system, the quality gate must be: **does the generated code
compile and pass tests?**

## Approach

Build a code generation evaluation harness that:
1. Takes a trained adapter
2. Generates code for held-out functions (from the codebase)
3. Checks if generated code compiles
4. Checks if generated code passes existing tests
5. Produces a quality score that gates adapter promotion

## Architecture

```
┌─────────────────┐
│ Held-out         │  Functions removed from training set
│ Test Functions   │  (stratified sample: 10-20% of codebase)
└────────┬────────┘
         │
         v
┌─────────────────┐
│ Prompt Generator │  Creates prompts from function signatures
│ (Phase 2 reuse)  │  using the same strategies
└────────┬────────┘
         │
         v
┌─────────────────┐
│ Inference Engine │  Adapter + base model generates code
│ (existing)       │
└────────┬────────┘
         │
         v
┌─────────────────┐
│ Compilation Gate │  1. Syntax check (tree-sitter parse)
│                  │  2. Type check (cargo check on patched file)
│                  │  3. Test pass (cargo test on patched crate)
└────────┬────────┘
         │
         v
┌─────────────────┐
│ Quality Metrics  │  compile_rate, test_pass_rate, exact_match,
│                  │  bleu_score, edit_distance
└─────────────────┘
```

## Metrics

### Hard Gates (must pass for promotion)
- **compile_rate**: % of generated functions that compile (target: >80%)
- **test_pass_rate**: % of generated functions where crate tests still pass (target: >70%)

### Soft Metrics (tracked for improvement)
- **exact_match**: % of generated functions that match the original exactly
- **edit_distance**: normalized Levenshtein distance between generated and original
- **token_overlap**: Jaccard similarity of token sets
- **ast_similarity**: structural similarity of parsed ASTs (using tree-sitter)

## Implementation

### 1. Held-out Split

During training data generation (Phase 2), split the dataset:
- 85% training set
- 10% validation set (for early stopping)
- 5% held-out test set (never seen during training)

Use deterministic splitting based on function hash:

```rust
fn split_function(hash: &str) -> Split {
    let byte = u8::from_str_radix(&hash[0..2], 16).unwrap_or(0);
    match byte {
        0..=216 => Split::Train,      // 85%
        217..=241 => Split::Validation, // 10%
        242..=255 => Split::HeldOut,    // 5%
    }
}
```

### 2. Compilation Checker

```rust
pub struct CompilationChecker {
    workspace_root: PathBuf,
}

impl CompilationChecker {
    /// Check if a generated function compiles in context.
    ///
    /// 1. Copy the original file
    /// 2. Replace the function body with generated code
    /// 3. Run `cargo check -p <crate>` on a temp worktree
    /// 4. Return success/failure with diagnostics
    pub async fn check_compilation(
        &self,
        file_path: &str,
        function_span: Span,
        generated_code: &str,
    ) -> CompilationResult { ... }
}
```

### 3. Quality Report

```rust
pub struct CodeGenQualityReport {
    pub adapter_id: String,
    pub adapter_hash: String,
    pub timestamp: String,
    pub held_out_count: usize,
    pub compile_rate: f32,
    pub test_pass_rate: f32,
    pub exact_match_rate: f32,
    pub avg_edit_distance: f32,
    pub avg_token_overlap: f32,
    pub passed_promotion_gate: bool,
}
```

### 4. Promotion Gate

The adapter promotion system already exists (`adapteros-db/src/promotions.rs`).
Add a new promotion requirement:

```rust
pub enum PromotionRequirement {
    // ...existing...
    CodeGenQualityGate {
        min_compile_rate: f32,
        min_test_pass_rate: f32,
    },
}
```

## Existing Code to Reuse

- `adapteros-retrieval/src/eval.rs` — evaluation harness patterns
- `adapteros-retrieval/src/benchmark.rs` — benchmark infrastructure
- `adapteros-db/src/promotions.rs` — promotion workflow
- `CodeGraph` — function span information for replacement
- `tree-sitter` — syntax validation without full cargo check

## Tests

1. Compilation checker correctly identifies valid/invalid Rust
2. Held-out split is deterministic across runs
3. Quality report serialization/deserialization
4. Promotion gate rejects adapters below threshold
5. End-to-end: generate → check → report

## Hours: 80

- Held-out split: 8h
- Compilation checker: 24h
- Quality metrics: 16h
- Quality report: 8h
- Promotion gate integration: 16h
- Tests: 8h
