# Phase 6: RAG-Grounded Code Generation

## Problem

Even with a fine-tuned adapter, the model's context window is limited. The
codebase is ~4M tokens — far beyond any context window. Without retrieval,
the model generates code in a vacuum, missing:

- Existing patterns and conventions
- Type definitions it needs to import
- Related functions it should call
- Error types it should propagate
- Test patterns it should follow

## Approach

Wire the existing RAG system to the code generation pipeline so that:
1. Before generating code, retrieve relevant context from the codebase
2. Include retrieved context in the generation prompt
3. Cite which files/functions informed the generation

The RAG system already exists: embeddings, vector index, chunking, retrieval.
The code-aware chunking already does semantic boundary detection. We just
need to wire it to the generation pipeline.

## Architecture

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│ Generation   │────>│ RAG Retrieval│────>│ Context      │
│ Request      │     │ (existing)   │     │ Assembly     │
│ (signature,  │     │              │     │              │
│  docstring)  │     │ • Vector     │     │ • Imports    │
│              │     │ • FTS        │     │ • Types      │
│              │     │ • CodeGraph  │     │ • Patterns   │
└──────────────┘     └──────────────┘     └──────┬───────┘
                                                  │
                                                  v
                                          ┌──────────────┐
                                          │ Enriched     │
                                          │ Prompt       │
                                          │              │
                                          │ "Given these │
                                          │  types and   │
                                          │  patterns,   │
                                          │  implement:" │
                                          └──────────────┘
```

## Implementation

### 1. Code Context Retriever

New module that combines multiple retrieval signals:

```rust
pub struct CodeContextRetriever {
    rag: RagSystem,
    codegraph: CodeGraph,
}

impl CodeContextRetriever {
    /// Retrieve relevant context for generating a function.
    ///
    /// Retrieves:
    /// 1. Type definitions used in the function signature
    /// 2. Similar functions (by embedding similarity)
    /// 3. Functions called by the original (from call graph)
    /// 4. Import statements needed
    pub async fn retrieve_context(
        &self,
        signature: &str,
        docstring: Option<&str>,
        file_path: &str,
        max_context_tokens: usize,
    ) -> Result<CodeContext> { ... }
}
```

### 2. Code Context Structure

```rust
pub struct CodeContext {
    /// Type definitions referenced in the signature
    pub type_definitions: Vec<TypeSnippet>,
    /// Similar functions from the codebase
    pub similar_functions: Vec<FunctionSnippet>,
    /// Functions this function likely calls
    pub callee_signatures: Vec<String>,
    /// Required imports
    pub imports: Vec<String>,
    /// Total token count of context
    pub total_tokens: usize,
    /// Citations for provenance
    pub citations: Vec<Citation>,
}
```

### 3. Context-Enriched Prompt Builder

```rust
fn build_rag_enriched_prompt(
    signature: &str,
    docstring: Option<&str>,
    context: &CodeContext,
) -> String {
    let mut prompt = String::new();

    // Add type definitions
    if !context.type_definitions.is_empty() {
        prompt.push_str("// Type definitions used:\n");
        for td in &context.type_definitions {
            prompt.push_str(&format!("{}\n\n", td.code));
        }
    }

    // Add similar function patterns
    if !context.similar_functions.is_empty() {
        prompt.push_str("// Similar functions in the codebase:\n");
        for sf in context.similar_functions.iter().take(3) {
            prompt.push_str(&format!("// From {}:\n{}\n\n", sf.file_path, sf.code));
        }
    }

    // Add the generation request
    prompt.push_str("// Implement the following function:\n");
    if let Some(doc) = docstring {
        prompt.push_str(&format!("/// {}\n", doc));
    }
    prompt.push_str(signature);

    prompt
}
```

### 4. Citation Tracking

The existing RAG system already has citation support in inference responses.
Extend it to code generation:

```rust
pub struct CodeGenerationResult {
    pub generated_code: String,
    pub citations: Vec<Citation>,
    pub context_tokens_used: usize,
    pub generation_tokens: usize,
}
```

### 5. Token Budget Management

With RAG context + generation, token budgets matter:

```
Total budget: model max_seq_length (e.g., 32K for Qwen2.5-7B)
├── System prompt: ~100 tokens
├── RAG context: up to 4K tokens (configurable)
├── Generation prompt: ~500 tokens
└── Generation output: remaining tokens
```

Implement a token budget allocator that prioritizes:
1. Signature + docstring (always included)
2. Direct type dependencies (most important context)
3. Similar functions (pattern examples)
4. Call graph neighbors (implementation hints)

## Existing Code to Reuse

- `RagSystem` in `adapteros-retrieval/src/rag/mod.rs`
- `CodeGraph` call graph for callee resolution
- `FTSIndex` for full-text search of type names
- `EmbeddingModel` for similarity search
- `InferResponse.citations` for citation format
- `chunk_file()` for code-aware chunking

## Tests

1. Context retrieval returns relevant types for a known function
2. Token budget is respected (never exceeds limit)
3. Citations point to real files and line numbers
4. Context-enriched prompt improves generation quality (A/B test)
5. Graceful degradation when RAG index is empty

## Hours: 120

- Code context retriever: 32h
- Context structure and assembly: 16h
- Prompt builder: 16h
- Citation tracking: 8h
- Token budget management: 16h
- Integration with inference pipeline: 16h
- Tests: 16h
