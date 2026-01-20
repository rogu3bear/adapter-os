//! Adapter helper functions for reducing boilerplate in handlers
//!
//! This module provides common patterns for adapter lookups with proper
//! error handling and tenant isolation validation.

use crate::api_error::ApiError;
use crate::auth::Claims;
use crate::security::validate_tenant_isolation;
use adapteros_db::{adapters::Adapter, ProtectedDb};
use tracing::error;

/// Fetch an adapter for a tenant with proper error handling and isolation validation.
///
/// This helper consolidates the common pattern of:
/// 1. Looking up an adapter by tenant + adapter_id
/// 2. Handling database errors
/// 3. Handling not-found cases
/// 4. Validating tenant isolation
///
/// # Example
///
/// ```ignore
/// use crate::adapter_helpers::fetch_adapter_for_tenant;
///
/// pub async fn my_handler(
///     State(state): State<AppState>,
///     Extension(claims): Extension<Claims>,
///     Path(adapter_id): Path<String>,
/// ) -> Result<Json<Response>, ApiError> {
///     let adapter = fetch_adapter_for_tenant(&state.db, &claims, &adapter_id).await?;
///     // ... use adapter
///     Ok(Json(response))
/// }
/// ```
pub async fn fetch_adapter_for_tenant(
    db: &ProtectedDb,
    claims: &Claims,
    adapter_id: &str,
) -> Result<Adapter, ApiError> {
    let adapter = db
        .get_adapter_for_tenant(&claims.tenant_id, adapter_id)
        .await
        .map_err(|e| {
            error!(
                tenant_id = %claims.tenant_id,
                adapter_id = %adapter_id,
                error = %e,
                "Failed to fetch adapter"
            );
            ApiError::db_error(e)
        })?
        .ok_or_else(|| ApiError::not_found("adapter"))?;

    // Validate tenant isolation - converts to ApiError
    validate_tenant_isolation(claims, &adapter.tenant_id).map_err(|(status, json)| {
        ApiError::new(status, "TENANT_ISOLATION_ERROR", json.0.message.clone())
            .with_details(json.0.details.map(|d| d.to_string()).unwrap_or_default())
    })?;

    Ok(adapter)
}

#[cfg(test)]
mod tests {
    // Tests would go here but require mocking the database
    // which is outside the scope of this helper extraction
}
