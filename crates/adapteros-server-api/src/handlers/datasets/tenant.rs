use crate::api_error::ApiError;
use crate::types::ErrorResponse;
use adapteros_db::Db;
use axum::http::StatusCode;
use axum::Json;

pub async fn bind_dataset_to_tenant(
    db: &Db,
    dataset_id: &str,
    tenant_id: &str,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    db.update_dataset_extended_fields(dataset_id, None, None, None, None, None, Some(tenant_id))
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to set dataset tenant: {}", e)).into())
}
