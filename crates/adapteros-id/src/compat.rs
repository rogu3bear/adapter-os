//! Compatibility helpers for legacy ID formats.
//!
//! Recognizes and extracts UUIDs from:
//! - Old readable IDs: `kind.slug.suffix` (e.g., `worker.my-node.k7m3qp`)
//! - Bare UUIDs: `550e8400-e29b-41d4-a716-446655440000`
//! - Prefixed UUIDs: `worker-550e8400-e29b-41d4-a716-446655440000`

use uuid::Uuid;

/// Check if a string looks like a legacy ID format (not a new TypedId).
///
/// Legacy formats:
/// - `kind.slug.suffix` (dot-separated readable ID)
/// - Bare UUID with hyphens
/// - `prefix-{uuid-with-hyphens}`
pub fn is_legacy_id(input: &str) -> bool {
    if input.is_empty() {
        return false;
    }

    // Dot-separated old readable format: kind.slug.suffix
    if input.contains('.') && input.split('.').count() == 3 {
        return true;
    }

    // Bare UUID (hyphenated): 8-4-4-4-12
    if Uuid::parse_str(input).is_ok() && input.contains('-') && input.len() == 36 {
        return true;
    }

    // Prefixed UUID: e.g. "worker-550e8400-e29b-41d4-a716-446655440000"
    if let Some(dash_pos) = input.find('-') {
        let after = &input[dash_pos + 1..];
        if Uuid::parse_str(after).is_ok() && after.len() == 36 {
            return true;
        }
    }

    false
}

/// Check if a string matches the old `kind.slug.suffix` readable ID format.
///
/// This recognizes IDs like `worker.my-node.k7m3qp` or `adapter.llama.abc123`.
/// Requires exactly three dot-separated non-empty parts where the first part
/// is a known old-format prefix.
pub fn is_readable_id(input: &str) -> bool {
    let mut parts = input.split('.');
    let Some(prefix) = parts.next() else {
        return false;
    };
    let Some(slug) = parts.next() else {
        return false;
    };
    let Some(suffix) = parts.next() else {
        return false;
    };
    if parts.next().is_some() {
        return false;
    }
    if slug.is_empty() || suffix.is_empty() {
        return false;
    }
    matches!(
        prefix,
        "tenant"
            | "user"
            | "node"
            | "model"
            | "adapter"
            | "plan"
            | "job"
            | "worker"
            | "dataset"
            | "doc"
            | "chunk"
            | "file"
            | "coll"
            | "stack"
            | "run"
            | "trace"
            | "req"
            | "session"
            | "msg"
            | "policy"
            | "audit"
            | "incident"
            | "decision"
            | "error"
            | "upload"
            | "report"
            | "export"
            | "repo"
            | "ws"
            | "ver"
            | "event"
            | "replay"
    )
}

/// Try to extract a UUID from a legacy ID string.
///
/// Returns `None` if no UUID can be extracted.
pub fn extract_uuid_from_legacy(input: &str) -> Option<Uuid> {
    // Try bare UUID
    if let Ok(uuid) = Uuid::parse_str(input) {
        return Some(uuid);
    }

    // Try prefixed UUID: "worker-{uuid}" or "rot-{uuid}"
    if let Some(dash_pos) = input.find('-') {
        let after = &input[dash_pos + 1..];
        if let Ok(uuid) = Uuid::parse_str(after) {
            return Some(uuid);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dot_separated_is_legacy() {
        assert!(is_legacy_id("worker.my-node.k7m3qp"));
        assert!(is_legacy_id("adapter.llama.abc123"));
    }

    #[test]
    fn bare_uuid_is_legacy() {
        assert!(is_legacy_id("550e8400-e29b-41d4-a716-446655440000"));
    }

    #[test]
    fn prefixed_uuid_is_legacy() {
        assert!(is_legacy_id("worker-550e8400-e29b-41d4-a716-446655440000"));
    }

    #[test]
    fn typed_id_is_not_legacy() {
        // TypedId uses simple (non-hyphenated) UUID
        assert!(!is_legacy_id("wrk-019474a1b2c3d4e5f6a7b8c9d0e1f2a3"));
    }

    #[test]
    fn empty_is_not_legacy() {
        assert!(!is_legacy_id(""));
    }

    #[test]
    fn extract_bare_uuid() {
        let uuid = extract_uuid_from_legacy("550e8400-e29b-41d4-a716-446655440000");
        assert!(uuid.is_some());
        assert_eq!(
            uuid.unwrap().to_string(),
            "550e8400-e29b-41d4-a716-446655440000"
        );
    }

    #[test]
    fn extract_prefixed_uuid() {
        let uuid = extract_uuid_from_legacy("worker-550e8400-e29b-41d4-a716-446655440000");
        assert!(uuid.is_some());
    }

    #[test]
    fn extract_from_dot_format_returns_none() {
        let uuid = extract_uuid_from_legacy("worker.my-node.k7m3qp");
        assert!(uuid.is_none());
    }

    #[test]
    fn readable_id_valid() {
        assert!(is_readable_id("worker.my-node.k7m3qp"));
        assert!(is_readable_id("adapter.llama.abc123"));
        assert!(is_readable_id("tenant.acme.x2y3z4"));
    }

    #[test]
    fn readable_id_invalid() {
        assert!(!is_readable_id(""));
        assert!(!is_readable_id("worker"));
        assert!(!is_readable_id("worker.slug"));
        assert!(!is_readable_id("worker.slug.suffix.extra"));
        assert!(!is_readable_id("unknown.slug.suffix"));
        assert!(!is_readable_id("worker..suffix"));
        assert!(!is_readable_id("worker.slug."));
        // TypedId format is NOT readable
        assert!(!is_readable_id("wrk-019474a1b2c3d4e5f6a7b8c9d0e1f2a3"));
    }
}
