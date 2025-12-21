# Synthetic Repository Dataset - Code Ingestion

## Overview

This dataset contains a synthetic Rust codebase with corresponding Q/A pairs designed to test AdapterOS's code ingestion capabilities. It demonstrates how the system can learn from actual code structures, documentation, and implementation patterns.

## Dataset Structure

### Files

1. **synthetic_code.rs** - A complete Rust module implementing realistic AdapterOS patterns:
   - Content-addressable hashing with BLAKE3
   - HKDF-based seed derivation for deterministic execution
   - Adapter lifecycle management with tiered eviction
   - K-sparse routing with Q15 quantization
   - Comprehensive unit tests

2. **synthetic_repo-code_ingestion.jsonl** - 20 Q/A pairs covering:
   - Struct explanations (`ContentHash`, `AdapterPool`, `AdapterMetadata`)
   - Function implementations (`derive_seed`, `load_adapter`, `select_top_k`)
   - Enum definitions (`AdapterState`)
   - Design rationales (Q15 quantization, HKDF domain separation)
   - Test explanations
   - Architecture patterns
   - Error handling strategies
   - Dependency relationships

## Dataset Statistics

- **Total Examples**: 20
- **Code Lines**: 346 (including tests and comments)
- **Question Types**:
  - Struct explanations: 3
  - Function explanations: 9
  - Design rationale: 4
  - Test explanations: 4
  - Architecture/dependency: 3
  - Error handling: 1

## Code Coverage

The synthetic repository demonstrates:

### Core Patterns
- ✅ Content-addressable hashing (BLAKE3)
- ✅ Deterministic seed derivation (HKDF)
- ✅ Lifecycle state machines (5 states)
- ✅ Tiered eviction strategies
- ✅ K-sparse routing with quantization
- ✅ Thread-safe memory management (Arc<Mutex<T>>)

### Documentation Patterns
- ✅ Struct-level docstrings with examples
- ✅ Function-level documentation with Args/Returns
- ✅ Inline comments explaining algorithms
- ✅ Example usage in doctests

### Testing Patterns
- ✅ Determinism verification
- ✅ Domain separation validation
- ✅ Memory management tests
- ✅ Routing correctness tests
- ✅ State transition tests

## Q/A Coverage Map

| Code Element | Questions | Types |
|--------------|-----------|-------|
| `ContentHash` | 3 | Struct, method, design rationale |
| `derive_seed` | 2 | Function explanation, HKDF mechanics |
| `AdapterPool` | 4 | Lifecycle, eviction, memory tracking |
| `AdapterState` | 1 | Enum values and transitions |
| `load_adapter` | 2 | Tiered eviction, error handling |
| `record_activation` | 2 | Promotion logic, activation thresholds |
| `KSparseRouter` | 3 | Q15 quantization, selection algorithm |
| `select_top_k` | 2 | Routing mechanics, seeded RNG |
| Tests | 4 | Test verification strategies |
| Metadata | 1 | Data structure purpose |

## Metadata Schema

Each JSONL entry includes:

```json
{
  "input": "Question about the code",
  "target": "Detailed answer explaining the concept",
  "metadata": {
    "source": "repo_unit",
    "type": "struct_explanation | function_explanation | test_explanation | design_rationale | architecture_explanation | error_handling | dependency_explanation",
    "file": "synthetic_code.rs",
    "lines": "start-end" // Optional: specific line references
  }
}
```

## Training Use Cases

This dataset is designed for:

1. **Code comprehension** - Understanding Rust patterns and AdapterOS architecture
2. **Documentation generation** - Learning to explain code functionality
3. **Design pattern recognition** - Identifying common patterns (lifecycle, eviction, routing)
4. **Test-driven learning** - Understanding verification strategies
5. **Codebase navigation** - Mapping questions to specific code locations

## Integration with AdapterOS

To use this dataset with AdapterOS training pipeline:

```bash
# 1. Ingest the synthetic repository
aosctl ingest --type rust --path ./training/datasets/codebase/synthetic_repo_dataset/synthetic_code.rs

# 2. Create training dataset
aosctl dataset create \
  --name synthetic-repo-v1 \
  --files ./training/datasets/codebase/synthetic_repo_dataset/synthetic_repo-code_ingestion.jsonl \
  --format jsonl \
  --validation-threshold 0.95

# 3. Train adapter
aosctl train \
  --dataset synthetic-repo-v1 \
  --template general-code \
  --rank 16 \
  --alpha 32 \
  --adapter-id tenant-a/engineering/code-comprehension/r001
```

## Quality Metrics

- **Accuracy**: All code examples compile and pass tests
- **Coverage**: 20 Q/A pairs covering 15+ distinct code elements
- **Diversity**: 7 different question types (struct, function, test, design, etc.)
- **Metadata completeness**: 100% of entries include source, type, and file fields
- **Answer depth**: Average 3-4 sentences per answer with technical details

## Future Enhancements

Potential expansions:
- [ ] Add multi-file repository examples
- [ ] Include cross-reference questions (how X relates to Y)
- [ ] Add refactoring/optimization questions
- [ ] Include error scenarios and debugging questions
- [ ] Add performance analysis questions
- [ ] Include integration examples with other modules

## License

Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.

Part of the AdapterOS synthetic training dataset collection.
