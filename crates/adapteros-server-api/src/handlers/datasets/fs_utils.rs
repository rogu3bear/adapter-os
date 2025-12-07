use crate::error_helpers::internal_error;
use crate::types::ErrorResponse;
use axum::http::StatusCode;
use axum::Json;
use std::path::Path;
use tokio::fs;
use tokio::io::AsyncWriteExt;

pub async fn ensure_dirs<'a>(
    paths: impl IntoIterator<Item = &'a Path>,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    for path in paths {
        if let Err(e) = fs::create_dir_all(path).await {
            return Err(internal_error(format!(
                "Failed to create directory {}: {}",
                path.display(),
                e
            )));
        }
    }
    Ok(())
}

pub async fn write_temp_file(
    path: &Path,
    data: &[u8],
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let mut file = fs::File::create(path).await.map_err(|e| {
        internal_error(format!(
            "Failed to create temp file {}: {}",
            path.display(),
            e
        ))
    })?;

    file.write_all(data)
        .await
        .map_err(|e| internal_error(format!("Failed to write file {}: {}", path.display(), e)))?;
    file.flush()
        .await
        .map_err(|e| internal_error(format!("Failed to flush file {}: {}", path.display(), e)))?;
    Ok(())
}

pub async fn finalize_file_move(
    from: &Path,
    to: &Path,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    fs::rename(from, to)
        .await
        .map_err(|e| internal_error(format!("Failed to move file to {}: {}", to.display(), e)))
}

pub async fn clean_temp(path: &Path) {
    let _ = fs::remove_dir_all(path).await;
}

pub async fn clean_dataset_dir(path: &Path) {
    let _ = fs::remove_dir_all(path).await;
}
