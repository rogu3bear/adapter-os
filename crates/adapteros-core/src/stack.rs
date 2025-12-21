//! Stack hash computation utilities
//!
//! Provides canonical stack hash computation used across all backend components.
//! Stack hashes represent the semantic content of adapter stacks.
//!
//! # Canonical Formula
//!
//! 1. Collect adapter entries as (adapter_id, adapter_hash_b3) pairs
//! 2. Sort by adapter_id lexicographically
//! 3. Build buffer: "id:hash_hex" for each adapter, concatenated
//! 4. Hash with BLAKE3
//!
//! This gives a stable, backends-agnostic, order-independent hash.

use crate::B3Hash;

/// Compute canonical stack hash from adapter ID and hash pairs
///
/// This is the authoritative stack hash computation used across:
/// - Database operations
/// - Worker hot-swap verification
/// - API handlers
/// - Router state tracking
///
/// # Arguments
/// * `adapters` - Iterator of (adapter_id, adapter_hash_b3) pairs
///
/// # Returns
/// Canonical stack hash as B3Hash
///
/// # Examples
/// ```rust
/// use adapteros_core::{B3Hash, stack::compute_stack_hash};
///
/// let adapters = vec![
///     ("adapter_b".to_string(), B3Hash::hash(b"hash_b")),
///     ("adapter_a".to_string(), B3Hash::hash(b"hash_a")),
/// ];
///
/// let stack_hash = compute_stack_hash(adapters);
/// ```
pub fn compute_stack_hash<I>(adapters: I) -> B3Hash
where
    I: IntoIterator<Item = (String, B3Hash)>,
{
    let mut pairs: Vec<_> = adapters.into_iter().collect();

    // Sort by adapter_id for deterministic ordering
    pairs.sort_by(|a, b| a.0.cmp(&b.0));

    // Build canonical representation: "id:hash_hex" for each adapter
    let mut buffer = String::new();
    for (id, hash) in pairs {
        buffer.push_str(&id);
        buffer.push(':');
        buffer.push_str(&hash.to_hex());
    }

    // Hash the canonical representation
    B3Hash::hash(buffer.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stack_hash_deterministic() {
        let adapters1 = vec![
            ("adapter_b".to_string(), B3Hash::hash(b"hash_b")),
            ("adapter_a".to_string(), B3Hash::hash(b"hash_a")),
        ];

        let adapters2 = vec![
            ("adapter_a".to_string(), B3Hash::hash(b"hash_a")),
            ("adapter_b".to_string(), B3Hash::hash(b"hash_b")),
        ];

        let hash1 = compute_stack_hash(adapters1);
        let hash2 = compute_stack_hash(adapters2);

        assert_eq!(hash1, hash2, "Stack hash must be order-independent");
    }

    #[test]
    fn test_stack_hash_unique() {
        let adapters1 = vec![("adapter_a".to_string(), B3Hash::hash(b"hash_a"))];

        let adapters2 = vec![("adapter_a".to_string(), B3Hash::hash(b"hash_b"))];

        let hash1 = compute_stack_hash(adapters1);
        let hash2 = compute_stack_hash(adapters2);

        assert_ne!(
            hash1, hash2,
            "Different adapter content must produce different hashes"
        );
    }
}
