# Code Ingestion Training Data

**Purpose:** Train adapters to parse documents/code and generate training examples

## Overview

Code ingestion transforms source code, PDFs, and documentation into training examples for LoRA adapters. The pipeline supports Identity and QuestionAnswer strategies.

## Key Concepts

- **Document Parsing:** PDF, Markdown, code extraction
- **Tokenization:** Model-specific tokenizer patterns
- **Training Strategies:** Identity (unsupervised), QA
- **Quality Filtering:** Relevance, confidence thresholds
- **Content Addressing:** BLAKE3 hashing for datasets

## Training Example Schema

```jsonl
{
  "input": {
    "source_file": "src/router.rs",
    "source_type": "rust_code",
    "tokenizer": "tiktoken-gpt4",
    "strategy": "identity"
  },
  "target": {
    "examples": [
      {
        "input": "pub fn select_top_k(",
        "target": "adapters: &[Adapter], k: usize) -> Vec<usize>",
        "metadata": {
          "relevance": 0.95,
          "confidence": 0.90
        }
      }
    ],
    "num_examples": 100,
    "total_tokens": 5000
  },
  "metadata": {
    "quality": 0.90,
    "label": "positive"
  }
}
```

## Ingestion Strategies

### 1. Identity (Unsupervised)
```rust
Input:  "pub fn load_adapter("
Target: "adapter_id: &str) -> Result<()>"
```

### 2. QuestionAnswer
```rust
Input:  "What does the router do?"
Target: "The router selects top-K adapters using gate scores"
```

## Quality Criteria

- **Min Examples:** 1000
- **Min Relevance:** 0.85
- **Min Confidence:** 0.90
- **Token Coverage:** >80% of source

## Data Sources

1. **Codebase:** AdapterOS Rust source code
2. **Documentation:** AGENTS.md, README.md, docs/
3. **PDFs:** Technical papers, specifications
4. **Tests:** Integration test patterns

## Example Datasets

- `rust_patterns/` - AdapterOS Rust code
- `policy_examples/` - Policy pack implementations
- `api_contracts/` - REST API patterns
- `test_patterns/` - Test code examples
- `documentation/` - Markdown docs

## Pipeline

```bash
# 1. Ingest document
DocumentIngestor::new(opts, tokenizer).ingest_pdf_path(path)?

# 2. Generate examples
generate_training_data(&doc, &tokenizer, &config)?

# 3. Create dataset
TrainingDatasetManager::create_dataset_from_documents(req).await?

# 4. Validate
dataset.validation_status == "valid"

# 5. Train
MicroLoRATrainer::train(examples, adapter_id).await?
```

## References

- `crates/adapteros-ingest-docs/` - Document ingestion
- `crates/adapteros-orchestrator/src/training_dataset_integration.rs` - Dataset manager
- `crates/adapteros-lora-worker/src/training/` - Training pipeline
- `AGENTS.md` - Document processing pipeline
