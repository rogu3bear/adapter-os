# Codebase Ingestion & Adapter Training

## Overview

AdapterOS now supports **automated codebase ingestion** to train LoRA adapters directly from repository code. This feature eliminates the need for manual dataset preparation by automatically:

1. **Extracting** code symbols, documentation, and metadata using CodeGraph
2. **Generating** Q&A training pairs from function signatures, docstrings, and usage examples
3. **Training** a LoRA adapter deterministically with the MicroLoRA trainer
4. **Packaging** and registering the adapter for immediate use

This achieves **Goal 4** from the feature requirements: point AdapterOS at a repository and automatically get a fine-tuned adapter.

## Quick Start

```bash
# Train an adapter from a repository
aosctl train-from-code \
  --repo /path/to/your/repo \
  --adapter-id my_project_adapter \
  --output ./adapters

# Train with custom configuration
aosctl train-from-code \
  --repo /path/to/your/repo \
  --adapter-id my_adapter \
  --rank 16 \
  --alpha 32.0 \
  --epochs 4 \
  --output ./adapters

# Train and register in database
aosctl train-from-code \
  --repo /path/to/your/repo \
  --adapter-id my_adapter \
  --register \
  --db-path ./var/cp.db \
  --tier 2
```

## Architecture

### Pipeline Components

```
Repository → CodeGraph → Dataset Generation → MicroLoRA Training → Packaging → Registration
```

1. **Code Extraction** (`crates/adapteros-codegraph`)
   - Parses source files using tree-sitter
   - Extracts symbols (functions, structs, traits)
   - Captures docstrings and type annotations
   - Deterministic content hashing

2. **Dataset Construction** (`crates/adapteros-orchestrator/codebase_ingestion`)
   - Generates Q&A pairs from documentation
   - Creates training examples for:
     - "What does X do?" → Docstring content
     - "How do I use X?" → Usage examples + type signatures
     - "What is the signature of X?" → Type information
   - Generates negative examples for abstention training
   - Tokenizes all text using the base model tokenizer

3. **Deterministic Training** (`crates/adapteros-lora-worker/training`)
   - Uses content-based seed for reproducibility
   - Runs MicroLoRA trainer with Q15 quantization
   - Same codebase always produces same adapter hash

4. **Packaging** (`crates/adapteros-lora-worker/packager`)
   - Quantizes weights to Q15
   - Creates manifest with lineage information
   - Computes BLAKE3 hash for verification

5. **Registration** (`crates/adapteros-db`)
   - Stores adapter metadata in database
   - Links to repository and commit SHA
   - Tracks training provenance

### File Structure

```
crates/
  adapteros-orchestrator/
    src/
      codebase_ingestion.rs          # Main ingestion pipeline
      lib.rs                          # Exports CodebaseIngestion
    tests/
      codebase_ingestion_test.rs     # Integration tests

  adapteros-cli/
    src/
      commands/
        train_from_code.rs            # CLI command implementation
      app.rs                          # Command registration

tests/
  data/
    test_repo/                        # Test repository with documented code
      src/lib.rs
      README.md
```

## Configuration Options

### Training Parameters

- `--rank`: LoRA rank (default: 16)
- `--alpha`: LoRA alpha scaling factor (default: 32.0)
- `--learning-rate`: Training learning rate (default: 0.0001)
- `--batch-size`: Training batch size (default: 8)
- `--epochs`: Number of training epochs (default: 3)
- `--hidden-dim`: Hidden dimension size (default: 768)

### Ingestion Parameters

- `--max-pairs-per-symbol`: Maximum Q&A pairs per symbol (default: 3)
- `--include-private`: Include private APIs (default: false, only public)
- `--min-doc-length`: Minimum documentation length (default: 20 chars)
- `--generate-negative`: Generate negative examples (default: true)

### Output Parameters

- `--output`: Output directory for adapter (default: ./adapters)
- `--tokenizer`: Custom tokenizer path (default: models/qwen2.5-7b-mlx/tokenizer.json)
- `--base-model`: Base model identifier (default: qwen2.5-7b)

### Registration Parameters

- `--register`: Register adapter in database after training
- `--db-path`: Database path (required with --register)
- `--tier`: Adapter tier (default: 2)
- `--category`: Adapter category (default: code)
- `--scope`: Adapter scope (default: codebase)

## Example Usage

### Basic Usage

Train an adapter for a Rust project:

```bash
aosctl train-from-code \
  --repo ~/projects/my-rust-app \
  --adapter-id my_rust_app_v1
```

This will:
- Parse all Rust source files
- Extract documented functions, structs, traits
- Generate Q&A pairs from docstrings
- Train a rank-16 adapter with 3 epochs
- Save to `./adapters/my_rust_app_v1/`

### Advanced Configuration

Train a high-rank adapter with custom settings:

```bash
aosctl train-from-code \
  --repo ~/projects/complex-framework \
  --adapter-id framework_expert \
  --rank 32 \
  --alpha 64.0 \
  --epochs 5 \
  --include-private \
  --max-pairs-per-symbol 5 \
  --register \
  --db-path ./var/aos-cp.sqlite3 \
  --tier 2 \
  --category framework \
  --scope codebase
```

This creates a more specialized adapter that:
- Uses higher rank (32) for more capacity
- Includes private APIs in training data
- Generates more examples per symbol (5)
- Trains for more epochs (5)
- Registers in the database for production use

### Testing Determinism

Run the same ingestion twice to verify deterministic behavior:

```bash
# First run
aosctl train-from-code \
  --repo ~/projects/my-lib \
  --adapter-id test_v1 \
  --output ./output1

# Second run
aosctl train-from-code \
  --repo ~/projects/my-lib \
  --adapter-id test_v2 \
  --output ./output2

# Compare adapter hashes (should match)
cat ./output1/test_v1/manifest.json | jq '.hash_b3'
cat ./output2/test_v2/manifest.json | jq '.hash_b3'
```

If the hashes match, training is deterministic.

## Training Data Format

### Generated Q&A Pairs

For a documented function like:

```rust
/// Add two numbers together
///
/// This function takes two integers and returns their sum.
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

The pipeline generates:

**Example 1: "What does it do?"**
- Prompt: "What does function 'add' do in this codebase?"
- Response: "Function 'add': Add two numbers together. This function takes two integers and returns their sum."

**Example 2: "How to use?"**
- Prompt: "How do I use the function 'add'?"
- Response: "Function 'add' has signature: fn(i32, i32) -> i32. Add two numbers together. This function takes two integers and returns their sum."

**Example 3: "Signature?"**
- Prompt: "What is the signature of 'add'?"
- Response: "fn(i32, i32) -> i32"

### Negative Examples

For abstention training:

- Prompt: "What does the function 'nonexistent_magic_function' do?"
- Response: "I don't have information about a function called 'nonexistent_magic_function' in this codebase."
- Weight: -1.0 (trains model to avoid hallucination)

## Determinism Guarantees

The pipeline ensures **bit-for-bit reproducible** adapters:

1. **Content Hashing**: Repository content is hashed with BLAKE3
2. **Deterministic Seed**: Training seed derived from content hash
3. **Sorted Data**: All symbols and examples sorted consistently
4. **Fixed Tokenization**: Same tokenizer produces same token IDs
5. **Deterministic Training**: MicroLoRA uses seeded RNG

### Verification

Two runs on the same codebase will produce:
- **Same content hash**: Ensures identical extracted data
- **Same adapter hash**: Ensures identical trained weights
- **Same training loss**: Validates reproducibility

## Integration Testing

### Running Tests

```bash
# Run all codebase ingestion tests
cargo test --package adapteros-orchestrator codebase_ingestion

# Run specific test
cargo test --package adapteros-orchestrator test_codebase_ingestion_end_to_end

# Run determinism test
cargo test --package adapteros-orchestrator test_determinism
```

### Test Repository

A minimal test repository is included at `tests/data/test_repo/`:

```
tests/data/test_repo/
├── README.md          # Project documentation
└── src/
    └── lib.rs         # Documented Rust code
```

This repository is used for integration testing to verify:
- Symbol extraction
- Q&A generation
- Training convergence
- Deterministic hashing

## Metadata Tracking

Each trained adapter includes metadata:

```json
{
  "adapter_id": "my_project_adapter",
  "adapter_hash": "b3:abc123...",
  "content_hash": "def456...",
  "repo_path": "/path/to/repo",
  "commit_sha": "abc123def456...",
  "symbols_count": 42,
  "examples_count": 126,
  "final_loss": 0.234567,
  "training_time_ms": 12345,
  "timestamp": "2025-01-15T10:30:00Z"
}
```

## Limitations & Future Work

### Current Limitations

1. **Language Support**: Currently optimized for Rust
   - tree-sitter parsers exist for other languages
   - Symbol extraction logic is language-specific

2. **Documentation Dependency**: Requires docstrings
   - Undocumented code generates fewer examples
   - Quality depends on documentation quality

3. **No Cross-Repository**: One adapter per repository
   - Cannot train on multiple codebases simultaneously
   - Would need monorepo-scale ingestion

### Future Enhancements

1. **Multi-Language Support**
   - Add parsers for Python, TypeScript, Go, etc.
   - Language-specific Q&A generation strategies

2. **Usage Pattern Extraction**
   - Analyze test files for usage examples
   - Extract common patterns from actual usage

3. **Incremental Training**
   - Train deltas for new commits
   - Merge adapters for incremental updates

4. **Smart Example Selection**
   - Prioritize frequently-used APIs
   - Weight examples by code complexity

## References

- [CodeGraph Documentation](./codegraph.md)
- [MicroLoRA Trainer](./training/base_adapter.md)
- [CLAUDE.md Developer Guide](../CLAUDE.md)
- [Adapter Packaging Spec](../specs/adapter_format.md)

## See Also

- `aosctl train --help` - General training command
- `aosctl train-base-adapter --help` - Base adapter training
- `aosctl adapter list --help` - List registered adapters
- `aosctl adapter info <id> --help` - Show adapter details
