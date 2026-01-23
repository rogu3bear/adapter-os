//! Retrieval receipts with cryptographic signing
//!
//! This module provides audit receipts for retrieval operations. Each receipt
//! captures the full identity chain (model, corpus, index, query) and can be
//! signed for tamper-evident logging.

use crate::corpus::ChunkingConfig;
use adapteros_core::B3Hash;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Full audit receipt for a retrieval operation.
///
/// Captures complete identity information for deterministic replay:
/// - Model identity (embedder + tokenizer hashes)
/// - Corpus identity (version hash + chunking params)
/// - Index identity (type + params + optional seed)
/// - Query identity (text hash + embedding hash)
/// - Results in deterministic order (score DESC, chunk_id ASC)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalReceipt {
    // Model identity
    /// BLAKE3 hash of the embedder model weights
    pub embedder_model_hash: B3Hash,
    /// BLAKE3 hash of the tokenizer configuration
    pub tokenizer_hash: B3Hash,

    // Corpus identity
    /// Deterministic version hash of the corpus
    pub corpus_version_hash: B3Hash,
    /// Chunking configuration used for the corpus
    pub chunking_params: ChunkingConfig,

    // Index identity
    /// Type of index (e.g., "flat", "hnsw")
    pub index_type: String,
    /// BLAKE3 hash of index parameters
    pub index_params_hash: B3Hash,
    /// Optional seed for deterministic index construction
    pub index_seed: Option<u64>,

    // Query identity
    /// BLAKE3 hash of the query text
    pub query_text_hash: B3Hash,
    /// BLAKE3 hash of the query embedding vector
    pub query_embedding_hash: B3Hash,

    // Results (deterministic order: score DESC, chunk_id ASC)
    /// Top-k results as (chunk_id, score) pairs
    pub top_k: Vec<(String, f32)>,

    // Tenant context
    /// Tenant identifier
    pub tenant_id: String,
    /// Unique request identifier
    pub request_id: String,
    /// Timestamp of the retrieval operation
    pub timestamp: DateTime<Utc>,

    // Metrics snapshot
    /// Time spent computing the query embedding (milliseconds)
    pub embed_latency_ms: f64,
    /// Time spent searching the index (milliseconds)
    pub search_latency_ms: f64,
}

/// Signable subset of receipt (excludes non-deterministic fields like latency metrics).
///
/// This structure contains only the fields that are included in the cryptographic
/// digest, ensuring that the signature covers the semantically meaningful parts
/// of the receipt while excluding timing information that may vary across runs.
#[derive(Serialize)]
struct SignableReceipt<'a> {
    embedder_model_hash: &'a B3Hash,
    tokenizer_hash: &'a B3Hash,
    corpus_version_hash: &'a B3Hash,
    index_type: &'a str,
    index_params_hash: &'a B3Hash,
    index_seed: Option<u64>,
    query_text_hash: &'a B3Hash,
    query_embedding_hash: &'a B3Hash,
    top_k: &'a [(String, f32)],
    tenant_id: &'a str,
    request_id: &'a str,
    timestamp: &'a DateTime<Utc>,
}

impl RetrievalReceipt {
    /// Compute deterministic digest of receipt for signing.
    ///
    /// The digest excludes non-deterministic fields (latency metrics, chunking params)
    /// to ensure that receipts with identical semantic content produce identical digests.
    ///
    /// # Returns
    ///
    /// A BLAKE3 hash of the canonical JSON representation of the signable fields.
    pub fn compute_digest(&self) -> B3Hash {
        let signable = SignableReceipt {
            embedder_model_hash: &self.embedder_model_hash,
            tokenizer_hash: &self.tokenizer_hash,
            corpus_version_hash: &self.corpus_version_hash,
            index_type: &self.index_type,
            index_params_hash: &self.index_params_hash,
            index_seed: self.index_seed,
            query_text_hash: &self.query_text_hash,
            query_embedding_hash: &self.query_embedding_hash,
            top_k: &self.top_k,
            tenant_id: &self.tenant_id,
            request_id: &self.request_id,
            timestamp: &self.timestamp,
        };
        let canonical =
            serde_json::to_vec(&signable).expect("Receipt serialization should not fail");
        B3Hash::hash(&canonical)
    }

    /// Total latency for the retrieval operation (embedding + search).
    pub fn total_latency_ms(&self) -> f64 {
        self.embed_latency_ms + self.search_latency_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn make_test_receipt() -> RetrievalReceipt {
        RetrievalReceipt {
            embedder_model_hash: B3Hash::hash(b"model"),
            tokenizer_hash: B3Hash::hash(b"tokenizer"),
            corpus_version_hash: B3Hash::hash(b"corpus"),
            chunking_params: ChunkingConfig::default(),
            index_type: "flat".to_string(),
            index_params_hash: B3Hash::hash(b"params"),
            index_seed: None,
            query_text_hash: B3Hash::hash(b"query"),
            query_embedding_hash: B3Hash::hash(b"embedding"),
            top_k: vec![
                ("chunk1".to_string(), 0.95),
                ("chunk2".to_string(), 0.87),
            ],
            tenant_id: "test".to_string(),
            request_id: "req1".to_string(),
            timestamp: Utc.with_ymd_and_hms(2026, 1, 23, 0, 0, 0).unwrap(),
            embed_latency_ms: 10.5,
            search_latency_ms: 5.2,
        }
    }

    #[test]
    fn test_receipt_digest_deterministic() {
        let receipt = make_test_receipt();

        let digest1 = receipt.compute_digest();
        let digest2 = receipt.compute_digest();
        assert_eq!(digest1, digest2);
    }

    #[test]
    fn test_receipt_digest_excludes_latency() {
        let mut receipt1 = make_test_receipt();
        let mut receipt2 = make_test_receipt();

        // Different latencies should produce same digest
        receipt1.embed_latency_ms = 100.0;
        receipt1.search_latency_ms = 200.0;
        receipt2.embed_latency_ms = 1.0;
        receipt2.search_latency_ms = 2.0;

        assert_eq!(receipt1.compute_digest(), receipt2.compute_digest());
    }

    #[test]
    fn test_receipt_digest_excludes_chunking_params() {
        let mut receipt1 = make_test_receipt();
        let mut receipt2 = make_test_receipt();

        // Different chunking params should produce same digest
        receipt1.chunking_params.token_chunk_size = 256;
        receipt2.chunking_params.token_chunk_size = 1024;

        assert_eq!(receipt1.compute_digest(), receipt2.compute_digest());
    }

    #[test]
    fn test_receipt_digest_changes_with_results() {
        let receipt1 = make_test_receipt();
        let mut receipt2 = make_test_receipt();

        receipt2.top_k = vec![("different".to_string(), 0.99)];

        assert_ne!(receipt1.compute_digest(), receipt2.compute_digest());
    }

    #[test]
    fn test_receipt_digest_changes_with_query() {
        let receipt1 = make_test_receipt();
        let mut receipt2 = make_test_receipt();

        receipt2.query_text_hash = B3Hash::hash(b"different query");

        assert_ne!(receipt1.compute_digest(), receipt2.compute_digest());
    }

    #[test]
    fn test_receipt_digest_changes_with_index_seed() {
        let mut receipt1 = make_test_receipt();
        let mut receipt2 = make_test_receipt();

        receipt1.index_seed = Some(42);
        receipt2.index_seed = Some(43);

        assert_ne!(receipt1.compute_digest(), receipt2.compute_digest());
    }

    #[test]
    fn test_receipt_serialization() {
        let receipt = RetrievalReceipt {
            embedder_model_hash: B3Hash::hash(b"model"),
            tokenizer_hash: B3Hash::hash(b"tokenizer"),
            corpus_version_hash: B3Hash::hash(b"corpus"),
            chunking_params: ChunkingConfig::default(),
            index_type: "flat".to_string(),
            index_params_hash: B3Hash::hash(b"params"),
            index_seed: Some(42),
            query_text_hash: B3Hash::hash(b"query"),
            query_embedding_hash: B3Hash::hash(b"embedding"),
            top_k: vec![("chunk1".to_string(), 0.95)],
            tenant_id: "test".to_string(),
            request_id: "req1".to_string(),
            timestamp: Utc::now(),
            embed_latency_ms: 10.5,
            search_latency_ms: 5.2,
        };

        let json = serde_json::to_string(&receipt).unwrap();
        let parsed: RetrievalReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.tenant_id, "test");
        assert_eq!(parsed.index_seed, Some(42));
        assert_eq!(parsed.top_k.len(), 1);
        assert_eq!(parsed.top_k[0].0, "chunk1");
    }

    #[test]
    fn test_receipt_total_latency() {
        let receipt = make_test_receipt();
        assert!((receipt.total_latency_ms() - 15.7).abs() < 0.001);
    }

    #[test]
    fn test_receipt_with_empty_top_k() {
        let mut receipt = make_test_receipt();
        receipt.top_k = vec![];

        // Should still compute digest without panic
        let digest = receipt.compute_digest();
        assert!(!digest.as_bytes().iter().all(|&b| b == 0));
    }
}
