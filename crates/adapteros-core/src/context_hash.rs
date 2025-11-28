//! Context hash computation for deterministic provenance tracking.
//!
//! Computes a blake3 hash over the canonicalized inference context:
//! - User prompt
//! - Retrieved chunks (sorted by chunk_id)
//! - Adapter stack (sorted)
//! - Embedding model identifier

use crate::B3Hash;

/// Chunk reference for context hash computation
#[derive(Debug, Clone)]
pub struct ChunkRef {
    pub chunk_id: String,
    pub chunk_hash: String,
}

/// Compute deterministic context hash for inference provenance
pub fn compute_context_hash(
    prompt: &str,
    chunks: &[ChunkRef],
    adapter_stack: &[String],
    embedding_model: &str,
) -> B3Hash {
    let mut hasher = blake3::Hasher::new();

    // Prompt
    hasher.update(prompt.as_bytes());
    hasher.update(&[0xFF]); // Field separator

    // Chunks sorted by chunk_id for determinism
    let mut sorted_chunks = chunks.to_vec();
    sorted_chunks.sort_by(|a, b| a.chunk_id.cmp(&b.chunk_id));
    for chunk in &sorted_chunks {
        hasher.update(chunk.chunk_id.as_bytes());
        hasher.update(chunk.chunk_hash.as_bytes());
        hasher.update(&[0xFF]);
    }

    // Adapters sorted
    let mut sorted_adapters = adapter_stack.to_vec();
    sorted_adapters.sort();
    for adapter in &sorted_adapters {
        hasher.update(adapter.as_bytes());
        hasher.update(&[0xFF]);
    }

    // Embedding model
    hasher.update(embedding_model.as_bytes());

    B3Hash::from_bytes(*hasher.finalize().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_hash_deterministic() {
        let chunks = vec![
            ChunkRef {
                chunk_id: "c2".into(),
                chunk_hash: "h2".into(),
            },
            ChunkRef {
                chunk_id: "c1".into(),
                chunk_hash: "h1".into(),
            },
        ];
        let adapters = vec!["adapter-b".into(), "adapter-a".into()];

        let hash1 = compute_context_hash("test prompt", &chunks, &adapters, "minilm");
        let hash2 = compute_context_hash("test prompt", &chunks, &adapters, "minilm");

        assert_eq!(hash1, hash2, "Same inputs should produce same hash");
    }

    #[test]
    fn test_context_hash_sorting() {
        // Test that input order doesn't affect hash (due to sorting)
        let chunks1 = vec![
            ChunkRef {
                chunk_id: "c1".into(),
                chunk_hash: "h1".into(),
            },
            ChunkRef {
                chunk_id: "c2".into(),
                chunk_hash: "h2".into(),
            },
        ];
        let chunks2 = vec![
            ChunkRef {
                chunk_id: "c2".into(),
                chunk_hash: "h2".into(),
            },
            ChunkRef {
                chunk_id: "c1".into(),
                chunk_hash: "h1".into(),
            },
        ];

        let adapters1 = vec!["adapter-a".into(), "adapter-b".into()];
        let adapters2 = vec!["adapter-b".into(), "adapter-a".into()];

        let hash1 = compute_context_hash("test", &chunks1, &adapters1, "minilm");
        let hash2 = compute_context_hash("test", &chunks2, &adapters2, "minilm");

        assert_eq!(hash1, hash2, "Hash should be order-independent");
    }

    #[test]
    fn test_context_hash_uniqueness() {
        let chunks = vec![ChunkRef {
            chunk_id: "c1".into(),
            chunk_hash: "h1".into(),
        }];
        let adapters = vec!["adapter-a".into()];

        // Different prompts should produce different hashes
        let hash1 = compute_context_hash("prompt1", &chunks, &adapters, "minilm");
        let hash2 = compute_context_hash("prompt2", &chunks, &adapters, "minilm");
        assert_ne!(
            hash1, hash2,
            "Different prompts should produce different hashes"
        );

        // Different chunks should produce different hashes
        let chunks2 = vec![ChunkRef {
            chunk_id: "c2".into(),
            chunk_hash: "h2".into(),
        }];
        let hash3 = compute_context_hash("prompt1", &chunks2, &adapters, "minilm");
        assert_ne!(
            hash1, hash3,
            "Different chunks should produce different hashes"
        );

        // Different adapters should produce different hashes
        let adapters2 = vec!["adapter-b".into()];
        let hash4 = compute_context_hash("prompt1", &chunks, &adapters2, "minilm");
        assert_ne!(
            hash1, hash4,
            "Different adapters should produce different hashes"
        );

        // Different embedding models should produce different hashes
        let hash5 = compute_context_hash("prompt1", &chunks, &adapters, "bert");
        assert_ne!(
            hash1, hash5,
            "Different embedding models should produce different hashes"
        );
    }
}
