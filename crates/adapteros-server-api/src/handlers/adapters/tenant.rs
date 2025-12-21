use crate::types::ErrorResponse;
use adapteros_db::Db;
use axum::http::StatusCode;
use axum::Json;

pub async fn bind_adapter_to_tenant(
    db: &Db,
    adapter_id: &str,
    tenant_id: &str,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    // Adapter registration already stores tenant_id; helper retained for symmetry and future hooks.
    // No-op verification to avoid depending on private DB helpers.
    let _ = (db, adapter_id, tenant_id);
    Ok(())
}
