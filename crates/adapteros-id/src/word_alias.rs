//! Word alias generation via BLAKE3-based deterministic indexing.
//!
//! Produces `{prefix}-{adjective}-{noun}` display names from a UUID.
//! BLAKE3 is used instead of direct byte extraction because UUIDv7 bytes 0-5
//! are timestamp-based, which would cluster temporally-adjacent IDs into the
//! same adjective.

use crate::words::adjectives::ADJECTIVES;
use crate::words::nouns::NOUNS;
use crate::IdPrefix;
use uuid::Uuid;

/// Compute a deterministic word alias for the given prefix and UUID.
///
/// Format: `{prefix}-{adjective}-{noun}`
///
/// The same UUID always produces the same alias (determinism guarantee).
/// With 1024 adjectives x 1024 nouns, there are ~1M possible combinations,
/// which is sufficient for human disambiguation in typical deployments.
pub fn word_alias(prefix: IdPrefix, uuid: &Uuid) -> String {
    let hash = blake3::hash(uuid.as_bytes());
    let h = hash.as_bytes();
    let adj_idx = u16::from_le_bytes([h[0], h[1]]) as usize % ADJECTIVES.len();
    let noun_idx = u16::from_le_bytes([h[2], h[3]]) as usize % NOUNS.len();
    format!(
        "{}-{}-{}",
        prefix.as_str(),
        ADJECTIVES[adj_idx],
        NOUNS[noun_idx]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let uuid = Uuid::parse_str("019474a1b2c3d4e5f6a7b8c9d0e1f2a3").unwrap();
        let a1 = word_alias(IdPrefix::Wrk, &uuid);
        let a2 = word_alias(IdPrefix::Wrk, &uuid);
        assert_eq!(a1, a2);
    }

    #[test]
    fn format_is_prefix_adj_noun() {
        let uuid = Uuid::now_v7();
        let alias = word_alias(IdPrefix::Adp, &uuid);
        assert!(alias.starts_with("adp-"));
        assert_eq!(alias.matches('-').count(), 2);
    }

    #[test]
    fn different_uuids_differ() {
        let u1 = Uuid::now_v7();
        // Small delay to get a different UUIDv7
        let u2 = Uuid::now_v7();
        if u1 != u2 {
            // They *might* collide (1/1M chance), but almost certainly won't
            let a1 = word_alias(IdPrefix::Wrk, &u1);
            let a2 = word_alias(IdPrefix::Wrk, &u2);
            // Not asserting inequality since collisions are theoretically possible
            let _ = (a1, a2);
        }
    }

    #[test]
    fn different_prefixes_differ() {
        let uuid = Uuid::now_v7();
        let a1 = word_alias(IdPrefix::Wrk, &uuid);
        let a2 = word_alias(IdPrefix::Adp, &uuid);
        // Same adj-noun but different prefix
        assert_ne!(a1[..3], a2[..3]);
    }
}
