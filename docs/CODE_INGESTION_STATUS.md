# Code Ingestion Pipeline - Status Report

## Executive Summary

**The codebase ingestion pipeline is fully implemented in code but cannot be verified in the current Linux environment.** The implementation exists at `crates/adapteros-orchestrator/src/code_ingestion.rs` (772 lines) with comprehensive integration tests, but requires macOS to build and run.

## Implementation Status

### ✅ What Exists (Code Complete)

1. **Core Pipeline** (`crates/adapteros-orchestrator/src/code_ingestion.rs`)
   - `CodeIngestionPipeline::run()` - End-to-end orchestration (lines 99-278)
   - Repository preparation (Git clone or local path)
   - Code graph extraction via `adapteros-codegraph`
   - Dataset construction with positive/negative sampling
   - Training via `MicroLoRATrainer`
   - `.aos` package creation with metadata
   - Optional registry registration

2. **Dataset Generation** (lines 342-493)
   - **Positive samples**: Q&A pairs from documented symbols
     ```
     Q: "In the {project} project, what does {symbol} do?"
     A: "{symbol} is a {kind} defined in {file}... Documentation: {docstring}"
     ```
   - **Negative samples**: Abstention training for undocumented code
     ```
     Q: "Explain the undocumented {symbol}..."
     A: "I don't know. {symbol} lacks documentation, so I won't speculate."
     ```
   - Configurable sampling (max_symbols, include_private, weights)
   - Deterministic ordering (sorted by qualified name)

3. **Determinism Mechanisms**
   - Seed derivation: `BLAKE3(commit_sha + dataset_hash + training_config)` (line 540-554)
   - Dataset hashing: `BLAKE3(all samples)` (line 526-538)
   - Stable symbol ordering (line 353)
   - ChaCha20Rng for reproducible weight initialization

4. **CLI Command** (`crates/adapteros-cli/src/commands/adapter_train_from_code.rs`)
   - Full argument parsing and validation
   - Repository resolution (local path vs Git URL)
   - Training configuration
   - Output formatting (text + JSON)

5. **Integration Test** (`crates/adapteros-cli/tests/train_from_code_tests.rs:60-146`)
   - Test: `train_from_code_pipeline_is_deterministic`
   - Verifies: Same code + config → identical BLAKE3 hash
   - Checks: Dataset content, positive/negative samples, registry integration

6. **Test Data** (`crates/adapteros-cli/tests/data/train_from_code_repo/`)
   - Sample Rust library with documented and undocumented functions
   - Provides realistic test case for pipeline

### ❌ What's Broken (Can't Verify)

1. **Build Fails on Linux**
   ```
   error: failed to run custom build command for `objc_exception v0.1.2`
   ```
   - Root cause: Metal framework dependencies (macOS-only)
   - Dependency chain: `orchestrator` → `lora-worker` → `lora-kernel-mtl` → `metal`
   - Even basic crates like `adapteros-crypto` fail to compile
   - **Conclusion**: This is a macOS-only codebase

2. **Missing Tokenizer**
   - Required: `models/qwen2.5-7b-mlx/tokenizer.json`
   - Download command from docs (line 48-50 in `scripts/download_model.sh`):
     ```bash
     huggingface-cli download mlx-community/Qwen2.5-7B-Instruct \
       --include tokenizer.json \
       --local-dir models/qwen2.5-7b-mlx
     ```
   - HuggingFace access denied in current environment
   - Alternative: Mistral tokenizer exists but is incompatible (different token IDs)

3. **Cannot Run Tests**
   - Integration test requires `feature = "extended-tests"`
   - Requires successful compilation
   - Requires valid tokenizer file
   - **None of these preconditions can be met on Linux**

## Detailed Code Analysis

### Pipeline Flow

```
1. prepare_repo()                         [line 664-679]
   ├─ Local path → Repository::discover()
   └─ Git URL → Repository::clone()

2. CodeGraph::from_directory()            [line 127]
   └─ Parse all source files with tree-sitter

3. build_symbol_samples()                 [line 342-374]
   ├─ Filter symbols (functions, classes, etc.)
   ├─ Sort by qualified name (determinism)
   ├─ Truncate to max_symbols
   ├─ For each symbol:
   │  ├─ Generate positive sample (line 376-454)
   │  └─ Generate negative sample if undocumented (line 456-493)
   └─ Return Vec<SymbolSample>

4. encode_samples()                       [line 495-524]
   └─ QwenTokenizer::encode() each sample

5. MicroLoRATrainer::train_separated()    [line 174]
   ├─ Override training seed (deterministic)
   └─ Train on positive + negative examples

6. save_as_aos_package_with_metadata()    [line 231-233]
   └─ Package weights + training data + metadata

7. register_adapter() [optional]          [line 556-601]
   └─ Insert into SQLite registry
```

### Determinism Design

**Seed Derivation** (line 540-554):
```rust
fn derive_seed(commit_sha: &str, dataset_hash: &str, config: &TrainingConfig) -> u64 {
    let mut hasher = Hasher::new();
    hasher.update(commit_sha.as_bytes());
    hasher.update(dataset_hash.as_bytes());
    hasher.update(&config.rank.to_le_bytes());
    hasher.update(&config.alpha.to_le_bytes());
    hasher.update(&config.learning_rate.to_le_bytes());
    hasher.update(&config.batch_size.to_le_bytes());
    hasher.update(&config.epochs.to_le_bytes());
    hasher.update(&config.hidden_dim.to_le_bytes());
    let digest = hasher.finalize();
    u64::from_le_bytes(digest.as_bytes()[..8])
}
```

This ensures:
- **Same repository state** (commit SHA) + **same config** → **same adapter**
- Seed depends on all training hyperparameters
- BLAKE3 provides cryptographic collision resistance

**Dataset Hashing** (line 526-538):
```rust
fn compute_dataset_hash(samples: &[SymbolSample]) -> String {
    let mut hasher = Hasher::new();
    for sample in samples {
        hasher.update(sample.prompt.as_bytes());
        hasher.update(sample.response.as_bytes());
        hasher.update(&sample.weight.to_le_bytes());
        for (key, value) in &sample.metadata {
            hasher.update(key.as_bytes());
            hasher.update(value.as_bytes());
        }
    }
    hasher.finalize().to_hex().to_string()
}
```

This provides:
- Stable dataset fingerprinting
- Metadata integrity verification
- Inputs to seed derivation

### Integration Test Design

Test: `train_from_code_pipeline_is_deterministic` (line 60-146)

```rust
// 1. Create test repository
copy_fixture(&fixture_repo_path(), &repo_dir);
init_git_repo(&repo_dir);  // Creates deterministic commit

// 2. Run pipeline once
let args = TrainFromCodeArgs { /* ... */ seed: Some(42) };
adapter_train_from_code::run(&args, &writer).await?;
let hash_one = aos_hash(&aos_path);

// 3. Run again with same parameters
adapter_train_from_code::run(&args, &writer).await?;
let hash_two = aos_hash(&aos_path);

// 4. Verify bit-for-bit identical
assert_eq!(hash_one, hash_two);  // ✓ Determinism

// 5. Verify content
let adapter = SingleFileAdapterLoader::load(&aos_path).await?;
let decoded = tokenizer.decode(&adapter.training_data.first().target)?;
assert!(decoded.contains("Widget size"));  // ✓ Positive sample

let abstain = adapter.training_data.iter()
    .find(|ex| ex.metadata.get("reason") == Some("missing_docstring"))?;
let decoded_abstain = tokenizer.decode(&abstain.target)?;
assert!(decoded_abstain.contains("I don't know"));  // ✓ Negative sample

// 6. Verify registry
let db = Db::connect(&db_path).await?;
let stored = db.get_adapter(&adapter_id).await?.unwrap();
assert_eq!(stored.hash_b3, hash_one);  // ✓ Registry integration
```

**Test Coverage**:
- ✅ Determinism (same input → same output hash)
- ✅ Dataset construction (positive + negative samples)
- ✅ Content quality (documentation extracted correctly)
- ✅ Abstention training (undocumented symbols → "I don't know")
- ✅ Registry integration (database insertion)
- ✅ Duplicate handling (second run reuses registry entry)

## What Would Be Needed to Verify

### Prerequisites

1. **macOS Environment** (Apple Silicon M1+)
   - Metal framework available
   - Xcode command-line tools installed

2. **Qwen Tokenizer**
   ```bash
   huggingface-cli download mlx-community/Qwen2.5-7B-Instruct \
     --include tokenizer.json \
     --local-dir models/qwen2.5-7b-mlx
   ```

3. **Database Setup**
   ```bash
   export DATABASE_URL=sqlite:./test.db
   cargo run --bin adapteros-orchestrator -- migrate
   ```

### Verification Steps

```bash
# 1. Build workspace
cargo build --workspace --release

# 2. Run unit tests
cargo test --package adapteros-orchestrator code_ingestion

# 3. Run integration test (requires tokenizer)
cargo test --package adapteros-cli --features extended-tests train_from_code

# 4. Manual test on real repository
cargo run --bin aosctl -- adapter train-from-code \
  --repo /path/to/rust/project \
  --adapter-id test_adapter \
  --rank 16 \
  --epochs 3 \
  --max-symbols 64

# 5. Verify determinism manually
# Run command twice, compare BLAKE3 hashes:
b3sum adapters/test_adapter.aos  # Should be identical both times

# 6. Inspect adapter contents
cargo run --bin adapteros-cli -- adapter inspect adapters/test_adapter.aos
```

### Expected Results

If working correctly:
1. **Build succeeds** with no errors
2. **Unit tests pass** (symbol filtering, dataset construction)
3. **Integration test passes** (determinism verified)
4. **Manual pipeline** produces `.aos` file
5. **Determinism verified**: Two runs with same commit → identical hash
6. **Adapter loads** successfully
7. **Registry integration** works (adapter appears in `aosctl adapter list`)

## Architecture Strengths

Despite being unverifiable in this environment, the code demonstrates:

1. **Good Separation of Concerns**
   - CodeGraph: AST parsing
   - CodeIngestion: Dataset construction
   - MicroLoRATrainer: Training logic
   - SingleFileAdapter: Serialization

2. **Robust Error Handling**
   - All functions return `Result<T>`
   - Detailed error messages with context
   - Graceful fallbacks (e.g., missing remote URL)

3. **Determinism by Design**
   - Seed derivation from content hashes
   - Stable ordering of all inputs
   - No external randomness sources

4. **Comprehensive Metadata**
   - Repository provenance (commit SHA, remote URL)
   - Dataset fingerprinting (BLAKE3 hash)
   - Training configuration captured
   - Generator attribution

5. **Testing Strategy**
   - Integration test covers full pipeline
   - Determinism is primary assertion
   - Content validation (positive/negative samples)
   - Database integration verified

## Limitations & Gaps

### Implementation Limitations

1. **Language Support**
   - Depends on adapteros-codegraph's tree-sitter grammars
   - May not support all languages equally
   - Custom DSLs likely unsupported

2. **Symbol Selection**
   - Simple filtering (public functions, classes, etc.)
   - No semantic ranking (importance, centrality)
   - Max symbols is a hard cutoff (no smart sampling)

3. **Dataset Quality**
   - Prompt templates are fixed
   - No code examples or usage patterns
   - Negative samples only for missing docs (not wrong docs)

4. **Training Configuration**
   - Fixed architecture (MicroLoRA)
   - No automatic hyperparameter tuning
   - Small default rank (16) may underfit large codebases

### Untested Edge Cases

1. **Empty Repositories**
   - What if no public symbols found?
   - Should error or create empty adapter?

2. **Large Repositories**
   - max_symbols=64 default may be too small
   - No progress indication for long parsing
   - Memory usage for large codegraphs unknown

3. **Network Failures**
   - Git clone timeout handling
   - Partial clones
   - Invalid URLs

4. **Registry Conflicts**
   - Adapter ID collision with different hash
   - Currently errors (line 575-578)
   - Should it offer to update or version?

5. **Tokenization Failures**
   - Very long docstrings (> max token length)
   - Special characters or unicode
   - Empty tokenizations (line 503 skips silently)

## Recommendations

### For Immediate Verification (macOS Required)

1. Set up macOS development environment
2. Download Qwen tokenizer from HuggingFace
3. Run `cargo test --package adapteros-cli --features extended-tests train_from_code`
4. Verify test passes with identical hashes
5. Test on 3-5 real repositories of varying sizes
6. Measure determinism across runs
7. Inspect adapter quality (load and query)

### For Production Readiness

1. **Add Progress Reporting**
   ```rust
   output.info(&format!("Parsing {} files...", file_count));
   output.info(&format!("Found {} symbols, selecting {}...", total, selected));
   output.info(&format!("Training epoch {}/{}...", epoch, total_epochs));
   ```

2. **Improve Symbol Selection**
   - PageRank on call graph for importance
   - Stratified sampling by file/module
   - Minimum documentation quality threshold

3. **Add Validation**
   - Verify adapter can load before saving
   - Test encoding/decoding round-trip
   - Check for empty training data

4. **Handle Edge Cases**
   - Empty repository → clear error message
   - No public symbols → suggest `--include-private`
   - Tokenization failures → warn and skip gracefully

5. **Add Observability**
   - Structured logging (tracing)
   - Timing breakdowns (parsing, encoding, training)
   - Memory usage tracking

6. **Documentation**
   - Add docstring examples to code_ingestion.rs
   - Update CLAUDE.md with usage examples
   - Add troubleshooting section for common errors

### For Linux/Cross-Platform Support

To make this work on Linux:

1. **Make Metal Optional**
   ```toml
   # In lora-worker/Cargo.toml
   [target.'cfg(target_os = "macos")'.dependencies]
   adapteros-lora-kernel-mtl = { path = "../adapteros-lora-kernel-mtl" }
   ```

2. **Provide CPU Fallback**
   - Already partially implemented in trainer
   - Need to complete CPU training path
   - Add feature flag `metal` (default on macOS)

3. **Test on Linux**
   - Set up Linux CI
   - Run tests with `--no-default-features`
   - Verify CPU training works (slower but functional)

## Conclusion

**The code is well-written and appears correct, but is currently unverifiable due to platform constraints.**

### What We Know
- ✅ Implementation is complete (772 lines)
- ✅ Architecture is sound
- ✅ Determinism mechanisms are in place
- ✅ Integration test exists and looks comprehensive
- ✅ CLI command is wired up correctly

### What We Don't Know
- ❓ Does it actually compile on macOS?
- ❓ Do the tests pass?
- ❓ Does determinism work in practice?
- ❓ Does the adapter quality meet expectations?
- ❓ Are there edge case bugs?

### Risk Assessment

**Low Risk**:
- Code structure is clean
- Error handling is comprehensive
- Design patterns are correct
- Similar to existing working features (base adapter training)

**Medium Risk**:
- Untested on real codebases
- Unknown performance on large repos
- Edge case handling incomplete

**High Risk**:
- No verification possible in current environment
- Claims of "production-ready" cannot be substantiated
- Tokenizer dependency is fragile

### Honest Assessment

This is **production-quality code that needs macOS verification**. The implementation looks correct and follows best practices, but without running it, we cannot guarantee:
- Absence of runtime bugs
- Actual determinism in practice
- Performance characteristics
- Edge case handling

**Recommendation**: Get access to macOS environment, run full test suite, verify on 5-10 real repositories, then deploy with confidence.

---

**Status**: Implementation complete, verification pending macOS environment.

**Next Action**: Run on macOS with proper tokenizer, or document as macOS-only feature with verification deferred.
