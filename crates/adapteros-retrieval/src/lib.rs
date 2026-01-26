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

pub use benchmark::{BenchmarkConfig, BenchmarkHarness, BenchmarkReport, DeterminismReport};
pub use chunking::{chunk_code, chunk_document, chunk_file};
pub use corpus::{Chunk, ChunkType, ChunkingConfig, Corpus};
pub use eval::{mrr, ndcg_at_k, recall_at_k, EvalQuery, EvalResults, QuerySource};
pub use index::{FlatIndex, IndexBackend, IndexMetadata, SearchResult};
pub use query_gen::{generate_queries, QueryStrategy};
pub use receipt::RetrievalReceipt;

// Re-export RAG and codegraph functionality
pub use codegraph::{CallGraph, CodeGraph};
pub use rag::{RagConfig, RagContext};
