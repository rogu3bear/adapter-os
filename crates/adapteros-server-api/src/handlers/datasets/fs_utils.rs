use std::path::Path;
use tokio::fs;

use axum::http::StatusCode;
use axum::Json;

use crate::api_error::ApiError;
use crate::types::ErrorResponse;

pub async fn ensure_dirs<'a>(
    paths: impl IntoIterator<Item = &'a Path>,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    for path in paths {
        if let Err(e) = fs::create_dir_all(path).await {
            return Err(ApiError::internal(format!(
                "Failed to create directory {}: {}",
                path.display(),
                e
            ))
            .into());
        }
    }
    Ok(())
}

pub async fn clean_temp(path: &Path) {
    let _ = fs::remove_dir_all(path).await;
}

pub async fn clean_dataset_dir(path: &Path) {
    let _ = fs::remove_dir_all(path).await;
}
