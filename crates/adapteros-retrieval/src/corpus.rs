//! Corpus management with deterministic versioning
//!
//! This module provides types for managing document collections (corpora) and their
//! constituent chunks. All hashing is done with BLAKE3 for deterministic versioning.

use adapteros_core::{AosError, B3Hash, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::warn;
use uuid::Uuid;

/// Type of chunk content
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChunkType {
    /// Code content with language and semantic type
    Code {
        /// Programming language (e.g., "rust", "python")
        language: String,
        /// Semantic type (e.g., "function", "class", "module")
        semantic_type: String,
    },
    /// Document content with format
    Document {
        /// Document format (e.g., "markdown", "plain", "html")
        format: String,
    },
}

/// A chunk of content from a document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    /// Unique chunk ID: blake3(source_path + start_offset + length)
    pub chunk_id: String,
    /// Source file path
    pub source_path: String,
    /// Chunk content
    pub content: String,
    /// Start offset in source
    pub start_offset: usize,
    /// End offset in source
    pub end_offset: usize,
    /// Chunk type (code or document)
    pub chunk_type: ChunkType,
    /// BLAKE3 hash of content
    pub content_hash: B3Hash,
}

impl Chunk {
    /// Generate a deterministic chunk ID from source path, offset, and length.
    ///
    /// The ID is computed as: blake3(source_path + ":" + start_offset + ":" + length)
    pub fn generate_id(source_path: &str, start_offset: usize, length: usize) -> String {
        let data = format!("{}:{}:{}", source_path, start_offset, length);
        B3Hash::hash(data.as_bytes()).to_hex()
    }

    /// Create a new chunk with automatically generated ID and content hash.
    pub fn new(
        source_path: String,
        content: String,
        start_offset: usize,
        end_offset: usize,
        chunk_type: ChunkType,
    ) -> Self {
        let length = end_offset - start_offset;
        let chunk_id = Self::generate_id(&source_path, start_offset, length);
        let content_hash = B3Hash::hash(content.as_bytes());
        Self {
            chunk_id,
            source_path,
            content,
            start_offset,
            end_offset,
            chunk_type,
            content_hash,
        }
    }
}

/// Configuration for chunking documents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkingConfig {
    /// Target chunk size in tokens for documents (default: 512)
    pub token_chunk_size: usize,
    /// Token overlap between adjacent chunks (default: 128)
    pub token_overlap: usize,
    /// Average characters per token for heuristic estimation (default: 4.0)
    pub chars_per_token: f32,
    /// Target size in characters for code chunks (default: 1000)
    pub code_target_size: usize,
    /// Maximum size in characters for code chunks (default: 2000)
    pub code_max_size: usize,
}

impl Default for ChunkingConfig {
    fn default() -> Self {
        Self {
            token_chunk_size: 512,
            token_overlap: 128,
            chars_per_token: 4.0,
            code_target_size: 1000,
            code_max_size: 2000,
        }
    }
}

/// A corpus of documents for retrieval
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Corpus {
    /// Unique corpus identifier
    pub corpus_id: String,
    /// Deterministic version hash computed from chunks
    pub version_hash: B3Hash,
    /// All chunks in the corpus
    pub chunks: Vec<Chunk>,
    /// Configuration used for chunking
    pub chunking_config: ChunkingConfig,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
}

impl Corpus {
    /// Compute deterministic version hash from chunks.
    ///
    /// The hash is computed by:
    /// 1. Sorting chunks by chunk_id to ensure order-independent hashing
    /// 2. Concatenating all content hashes in sorted order
    /// 3. Computing BLAKE3 hash of the concatenated hashes
    pub fn compute_version_hash(chunks: &[Chunk]) -> B3Hash {
        let mut hasher = blake3::Hasher::new();
        let mut sorted_ids: Vec<_> = chunks
            .iter()
            .map(|c| (&c.chunk_id, &c.content_hash))
            .collect();
        sorted_ids.sort_by_key(|(id, _)| *id);
        for (_, content_hash) in sorted_ids {
            hasher.update(content_hash.as_bytes());
        }
        B3Hash::new(*hasher.finalize().as_bytes())
    }

    /// Create a new corpus with the given chunks and configuration.
    ///
    /// Automatically generates a corpus ID and computes the version hash.
    /// Logs a warning if the chunk set is empty; use [`Corpus::new_validated`]
    /// to reject empty chunk sets outright.
    pub fn new(chunks: Vec<Chunk>, chunking_config: ChunkingConfig) -> Self {
        if chunks.is_empty() {
            warn!("Creating corpus with zero chunks — downstream retrieval will return no results");
        }
        let corpus_id = Uuid::new_v4().to_string();
        let version_hash = Self::compute_version_hash(&chunks);
        Self {
            corpus_id,
            version_hash,
            chunks,
            chunking_config,
            created_at: Utc::now(),
        }
    }

    /// Create a new corpus, returning an error if chunks are empty.
    ///
    /// Prefer this over [`Corpus::new`] in ingestion pipelines where an
    /// empty corpus indicates silent data loss.
    pub fn new_validated(chunks: Vec<Chunk>, chunking_config: ChunkingConfig) -> Result<Self> {
        if chunks.is_empty() {
            return Err(AosError::Validation(
                "Cannot create corpus with zero chunks — input may have been silently dropped"
                    .to_string(),
            ));
        }
        Ok(Self::new(chunks, chunking_config))
    }

    /// Return the number of chunks in the corpus.
    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    /// Check if the corpus is empty.
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_corpus_version_hash_deterministic() {
        let chunks = vec![
            Chunk {
                chunk_id: "a".to_string(),
                source_path: "test.rs".to_string(),
                content: "fn main() {}".to_string(),
                start_offset: 0,
                end_offset: 12,
                chunk_type: ChunkType::Code {
                    language: "rust".to_string(),
                    semantic_type: "function".to_string(),
                },
                content_hash: B3Hash::hash(b"fn main() {}"),
            },
            Chunk {
                chunk_id: "b".to_string(),
                source_path: "test.md".to_string(),
                content: "# Hello".to_string(),
                start_offset: 0,
                end_offset: 7,
                chunk_type: ChunkType::Document {
                    format: "markdown".to_string(),
                },
                content_hash: B3Hash::hash(b"# Hello"),
            },
        ];

        let hash1 = Corpus::compute_version_hash(&chunks);
        let hash2 = Corpus::compute_version_hash(&chunks);
        assert_eq!(hash1, hash2);

        // Order shouldn't matter (sorted by chunk_id)
        let mut reversed = chunks.clone();
        reversed.reverse();
        let hash3 = Corpus::compute_version_hash(&reversed);
        assert_eq!(hash1, hash3);
    }

    #[test]
    fn test_chunk_new() {
        let chunk = Chunk::new(
            "test.rs".to_string(),
            "fn main() {}".to_string(),
            0,
            12,
            ChunkType::Code {
                language: "rust".to_string(),
                semantic_type: "function".to_string(),
            },
        );

        // Verify chunk ID is deterministic
        let expected_id = Chunk::generate_id("test.rs", 0, 12);
        assert_eq!(chunk.chunk_id, expected_id);

        // Verify content hash is correct
        assert_eq!(chunk.content_hash, B3Hash::hash(b"fn main() {}"));
    }

    #[test]
    fn test_chunk_generate_id_deterministic() {
        let id1 = Chunk::generate_id("test.rs", 0, 100);
        let id2 = Chunk::generate_id("test.rs", 0, 100);
        assert_eq!(id1, id2);

        // Different parameters should produce different IDs
        let id3 = Chunk::generate_id("test.rs", 0, 101);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_chunking_config_default() {
        let config = ChunkingConfig::default();
        assert_eq!(config.token_chunk_size, 512);
        assert_eq!(config.token_overlap, 128);
        assert_eq!(config.code_target_size, 1000);
        assert_eq!(config.code_max_size, 2000);
    }

    #[test]
    fn test_corpus_new() {
        let chunks = vec![Chunk::new(
            "test.md".to_string(),
            "# Test".to_string(),
            0,
            6,
            ChunkType::Document {
                format: "markdown".to_string(),
            },
        )];

        let corpus = Corpus::new(chunks.clone(), ChunkingConfig::default());

        assert_eq!(corpus.len(), 1);
        assert!(!corpus.is_empty());
        assert!(!corpus.corpus_id.is_empty());
        assert_eq!(corpus.version_hash, Corpus::compute_version_hash(&chunks));
    }

    #[test]
    fn test_corpus_empty() {
        let corpus = Corpus::new(vec![], ChunkingConfig::default());
        assert_eq!(corpus.len(), 0);
        assert!(corpus.is_empty());
    }

    #[test]
    fn test_corpus_new_validated_rejects_empty() {
        let result = Corpus::new_validated(vec![], ChunkingConfig::default());
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("zero chunks"),
            "error should mention zero chunks, got: {}",
            msg
        );
    }

    #[test]
    fn test_corpus_new_validated_accepts_nonempty() {
        let chunks = vec![Chunk::new(
            "test.md".to_string(),
            "# Hello".to_string(),
            0,
            7,
            ChunkType::Document {
                format: "markdown".to_string(),
            },
        )];
        let result = Corpus::new_validated(chunks, ChunkingConfig::default());
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }
}
