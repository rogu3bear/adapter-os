use crate::api_error::ApiError;
use adapteros_id::{is_readable_id, TypedId};

/// Resolve incoming IDs to canonical form.
///
/// Accepts three formats, all returned as-is:
/// 1. **TypedId** (`{prefix}-{uuid_hex32}`): Current format.
/// 2. **Old readable** (`kind.slug.suffix`): Legacy format, still accepted.
/// 3. **Anything else**: Passed through unchanged (best-effort).
pub async fn resolve_id(
    _db: &adapteros_db::ProtectedDb,
    _kind: &str,
    input: &str,
) -> Result<String, ApiError> {
    Ok(normalize_id(input))
}

/// Resolve incoming IDs to canonical form without requiring a kind.
pub async fn resolve_any_id(
    _db: &adapteros_db::ProtectedDb,
    input: &str,
) -> Result<String, ApiError> {
    Ok(normalize_id(input))
}

fn normalize_id(input: &str) -> String {
    // TypedId — canonical
    if TypedId::parse(input).is_some() {
        return input.to_string();
    }

    // Old readable — accepted for backward compat
    if is_readable_id(input) {
        return input.to_string();
    }

    // Pass through anything else unchanged
    input.to_string()
}
