//! Deterministic retrieval with receipts and benchmarking
//!
//! Provides:
//! - Hybrid chunking (token + semantic)
//! - Flat and HNSW index backends
//! - Retrieval receipts with Ed25519 signing
//! - Benchmark harness with eval metrics

pub mod benchmark;
pub mod chunking;
pub mod corpus;
pub mod eval;
pub mod index;
pub mod query_gen;
pub mod receipt;

pub use benchmark::{BenchmarkConfig, BenchmarkHarness, BenchmarkReport};
pub use corpus::{Chunk, ChunkType, Corpus};
pub use index::{IndexBackend, IndexMetadata, SearchResult};
pub use receipt::RetrievalReceipt;
