//! Deterministic retrieval with receipts and benchmarking
//!
//! Provides:
//! - Hybrid chunking (token + semantic)
//! - Flat and HNSW index backends
//! - Retrieval receipts with Ed25519 signing
//! - Benchmark harness with eval metrics
//! - RAG (Retrieval-Augmented Generation) integration
//! - Code graph analysis and query generation

pub mod benchmark;
pub mod chunking;
pub mod codegraph;
pub mod corpus;
pub mod eval;
pub mod index;
pub mod query_gen;
pub mod rag;
pub mod receipt;

// Re-export codegraph submodules at crate level for internal use
pub use codegraph::parsers;
pub use codegraph::types;

// Re-export commonly needed types at crate level
pub use codegraph::types::{
    Language, ParseResult, Span, SymbolId, SymbolKind, SymbolNode, TypeAnnotation, Visibility,
};

pub use benchmark::{BenchmarkConfig, BenchmarkHarness, BenchmarkReport, DeterminismReport};
pub use chunking::{chunk_code, chunk_document, chunk_file};
pub use corpus::{Chunk, ChunkType, ChunkingConfig, Corpus};
pub use eval::{mrr, ndcg_at_k, recall_at_k, EvalQuery, EvalResults, QuerySource};
pub use index::{FlatIndex, IndexBackend, IndexMetadata, SearchResult};
pub use query_gen::{generate_queries, QueryStrategy};
pub use receipt::RetrievalReceipt;

// Re-export RAG and codegraph functionality
pub use codegraph::{CallGraph, CodeGraph, DbConfig};
pub use rag::{
    DocMetadata, EvidenceSpan, IndexNamespaceId, RagSystem, RetrievalResult, TenantIndex,
};

// Re-export rag submodules at crate level for internal use
pub use rag::chunking as rag_chunking;
pub use rag::fts_index;
pub use rag::retrieval;
