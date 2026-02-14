# Phase 2: Code-Aware Training Data Generation

## Problem

The existing codebase ingestion (`codebase_ingestion.rs`) generates QA pairs like:

```
Q: What does the function `derive_seed_u64` in seed.rs do?
A: `derive_seed_u64` is a function defined in `seed.rs` (lines 42-58).
   Signature: pub fn derive_seed_u64(global: &B3Hash, context: &str) -> u64.
   Returns `u64`. Documentation: Derives a deterministic seed...
```

This teaches **comprehension** (answering questions about code), not **generation**
(writing code). For the system to write itself, we need pairs like:

```
Q: Implement a function that derives a deterministic u64 seed from a BLAKE3
   hash and context string using HKDF-SHA256. It should be public, take a
   &B3Hash and &str, and return u64.
A: pub fn derive_seed_u64(global: &B3Hash, context: &str) -> u64 {
       let hk = Hkdf::<Sha256>::new(Some(global.as_bytes()), context.as_bytes());
       let mut output = [0u8; 8];
       hk.expand(b"aos-seed-u64", &mut output)
           .expect("HKDF expand for seed derivation");
       u64::from_le_bytes(output)
   }
```

## Approach

Build on what exists:
1. **CodeGraph** already extracts functions with signatures, docstrings, spans
2. **Tree-sitter Rust parser** already parses ASTs
3. **Chunking** already does semantic boundary detection

What's missing: extracting **function bodies** and generating **code generation**
pairs instead of **code comprehension** pairs.

## New Module: `code_training_gen.rs`

Location: `crates/adapteros-orchestrator/src/code_training_gen.rs`

### Strategy 1: Signature → Body (Code Generation)

For each function/method extracted by CodeGraph:

```
prompt: "Implement the following Rust function:\n\n```rust\n{signature}\n```\n\n
         Context: This function is in `{file_path}`, module `{module}`.
         {docstring if available}\n\nDependencies: {imports used by this function}"

completion: "{function body}"
```

This requires reading the actual source file to extract the body (CodeGraph
stores file_path and span lines). The tree-sitter parser can extract the
complete function node.

### Strategy 2: Context → Function (Contextual Generation)

Given surrounding code, generate the function:

```
prompt: "Given the following Rust code context:\n\n```rust\n{preceding 20 lines}\n
         // TODO: implement {function_name}\n{following 20 lines}\n```\n\n
         Implement `{function_name}` that {docstring}."

completion: "{complete function}"
```

This teaches the model to write code that fits its surrounding context.

### Strategy 3: Docstring → Implementation

For documented functions:

```
prompt: "{docstring}\n\nImplement this in Rust:"
completion: "{complete function including signature}"
```

### Strategy 4: FIM (Fill-in-the-Middle)

For the eventual code completion use case:

```
prompt: "<|fim_prefix|>{code before cursor}<|fim_suffix|>{code after cursor}<|fim_middle|>"
completion: "{code at cursor position}"
```

This requires FIM tokens in the tokenizer and model. Qwen2.5 supports FIM
natively with `<|fim_prefix|>`, `<|fim_suffix|>`, `<|fim_middle|>` tokens.

## Implementation Plan

### 1. Source File Reader

Add a function that reads the actual source file and extracts a specific
function body using the span information from CodeGraph:

```rust
/// Extract function body from source file using CodeGraph span info.
fn extract_function_body(file_path: &str, start_line: usize, end_line: usize) -> Result<String> {
    let content = std::fs::read_to_string(file_path)?;
    let lines: Vec<&str> = content.lines().collect();
    let start = start_line.saturating_sub(1); // 1-indexed to 0-indexed
    let end = end_line.min(lines.len());
    Ok(lines[start..end].join("\n"))
}
```

### 2. Import Resolver

Extract imports used by a function (for context in prompts):

```rust
/// Extract imports from the file that are used by the function body.
fn extract_relevant_imports(file_content: &str, function_body: &str) -> Vec<String> {
    // Parse use statements from file
    // Filter to those whose imported names appear in function_body
}
```

### 3. Context Extractor

Extract surrounding code for contextual generation:

```rust
/// Extract N lines before and after a function for context.
fn extract_context(
    file_content: &str,
    start_line: usize,
    end_line: usize,
    context_lines: usize,
) -> (String, String) {
    // (prefix_context, suffix_context)
}
```

### 4. Training Pair Generator

New `CodeTrainingStrategy` enum and generator that produces all four pair types:

```rust
pub enum CodeTrainingStrategy {
    SignatureToBody,
    ContextToFunction,
    DocstringToImplementation,
    FillInTheMiddle,
    All, // generates all applicable types per function
}
```

### 5. Quality Filters

Not all functions make good training pairs:
- Skip trivial getters/setters (< 3 lines body)
- Skip generated code (macro expansions, derive impls)
- Skip test functions (they test, not teach)
- Prefer functions with docstrings (higher signal)
- Weight by complexity (more complex = more valuable)
- Skip functions with only `todo!()` or `unimplemented!()`

## Existing Code to Reuse

- `CodeGraph::from_directory()` — already walks repos and extracts symbols
- `SymbolNode` — has `file_path`, `span`, `signature`, `docstring`, `visibility`
- `SymbolKind::Function | Method | Struct | Trait | Enum` — filtering
- `encode_qa_samples()` in codebase_ingestion.rs — tokenization + metadata
- `compute_samples_hash()` — deterministic hashing
- `derive_training_seed()` — seed derivation

## Estimated Pair Counts for AdapterOS

~2,452 Rust source files, ~1.16M lines:
- Estimated public functions: ~3,000-5,000
- Estimated public methods: ~5,000-8,000
- Estimated documented functions: ~2,000-3,000
- With all 4 strategies: ~20,000-40,000 training pairs
- After quality filtering: ~10,000-25,000 high-quality pairs

This is a solid dataset size for LoRA fine-tuning on a 7B model.

## Tests

1. Generate pairs from a test fixture (small Rust file with known functions)
2. Verify pair quality: prompt describes the function, completion is the body
3. Verify deterministic ordering (sorted by qualified name)
4. Verify BLAKE3 hash consistency across runs
5. Verify quality filters exclude trivial functions
6. Verify FIM pairs have correct special token placement

## Verification

```bash
cargo test -p adapteros-orchestrator -- code_training
cargo check -p adapteros-orchestrator
```

## Hours: 120

- Source reader + import resolver: 20h
- Context extractor: 16h
- Training pair generator (4 strategies): 40h
- Quality filters: 16h
- Integration with existing CodeGraph: 12h
- Tests: 16h
