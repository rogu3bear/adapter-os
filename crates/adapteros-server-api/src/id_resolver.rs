use crate::api_error::ApiError;
use adapteros_core::ids::is_readable_id;
use adapteros_db::ProtectedDb;

/// Resolve incoming IDs to canonical readable IDs.
///
/// - If the input is already readable, return as-is.
/// - If an alias exists, return the readable ID.
/// - Otherwise, return the input (backward compatibility for legacy IDs).
pub async fn resolve_id(db: &ProtectedDb, kind: &str, input: &str) -> Result<String, ApiError> {
    if is_readable_id(input) {
        return Ok(input.to_string());
    }

    let resolved: Option<String> =
        sqlx::query_scalar("SELECT new_id FROM id_aliases WHERE legacy_id = ? AND kind = ?")
            .bind(input)
            .bind(kind)
            .fetch_optional(db.pool())
            .await
            .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?;

    Ok(resolved.unwrap_or_else(|| input.to_string()))
}

/// Resolve incoming IDs to canonical readable IDs without requiring a kind.
pub async fn resolve_any_id(db: &ProtectedDb, input: &str) -> Result<String, ApiError> {
    if is_readable_id(input) {
        return Ok(input.to_string());
    }

    let resolved: Option<String> =
        sqlx::query_scalar("SELECT new_id FROM id_aliases WHERE legacy_id = ?")
            .bind(input)
            .fetch_optional(db.pool())
            .await
            .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?;

    Ok(resolved.unwrap_or_else(|| input.to_string()))
}
