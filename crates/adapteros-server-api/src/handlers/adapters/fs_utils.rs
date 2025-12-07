use crate::error_helpers::internal_error;
use crate::types::ErrorResponse;
use axum::http::StatusCode;
use axum::Json;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tokio::fs;
use uuid::Uuid;

use adapteros_core::adapter_repo_paths::AdapterPaths;

pub async fn ensure_repo_dirs(
    paths: &AdapterPaths,
    tenant_id: &str,
    adapter_name: &str,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    for dir in [
        paths.repo_root.as_path(),
        paths.repo_root.join(tenant_id).as_path(),
        paths.repo_root.join(tenant_id).join(adapter_name).as_path(),
    ] {
        if let Err(e) = fs::create_dir_all(dir).await {
            return Err(internal_error(format!(
                "Failed to create adapter directory {}: {}",
                dir.display(),
                e
            )));
        }
    }
    if let Err(e) = fs::create_dir_all(&paths.cache_root).await {
        return Err(internal_error(format!(
            "Failed to create adapter cache directory {}: {}",
            paths.cache_root.display(),
            e
        )));
    }
    Ok(())
}

pub async fn write_temp_bundle(
    paths: &AdapterPaths,
) -> Result<(PathBuf, fs::File), (StatusCode, Json<ErrorResponse>)> {
    let temp_dir = paths.repo_root.join("temp");
    if let Err(e) = fs::create_dir_all(&temp_dir).await {
        return Err(internal_error(format!(
            "Failed to create temp adapter directory {}: {}",
            temp_dir.display(),
            e
        )));
    }

    let temp_path = temp_dir.join(format!("{}.aos.tmp", Uuid::now_v7()));
    let file = fs::File::create(&temp_path).await.map_err(|e| {
        internal_error(format!(
            "Failed to create temp file {}: {}",
            temp_path.display(),
            e
        ))
    })?;

    Ok((temp_path, file))
}

pub async fn finalize_bundle_move(
    from: &Path,
    to: &Path,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if let Some(parent) = to.parent() {
        if let Err(e) = fs::create_dir_all(parent).await {
            return Err(internal_error(format!(
                "Failed to create final adapter directory {}: {}",
                parent.display(),
                e
            )));
        }
    }

    let rename_result = if should_force_copy_fallback() {
        Err(std::io::Error::from_raw_os_error(EXDEV))
    } else {
        fs::rename(from, to).await
    };

    match rename_result {
        Ok(_) => Ok(()),
        Err(e) if is_cross_device(&e) => {
            fs::copy(from, to).await.map_err(|copy_err| {
                internal_error(format!(
                    "Failed to copy adapter bundle to {}: {}",
                    to.display(),
                    copy_err
                ))
            })?;

            if let Ok(f) = fs::File::open(to).await {
                let _ = f.sync_all().await;
            }

            let _ = fs::remove_file(from).await;
            Ok(())
        }
        Err(e) => Err(internal_error(format!(
            "Failed to move adapter bundle to {}: {}",
            to.display(),
            e
        ))),
    }
}

#[allow(dead_code)]
pub async fn clean_temp_bundle(path: &Path) {
    let _ = fs::remove_file(path).await;
}

#[allow(dead_code)]
pub async fn clean_adapter_dir(path: &Path) {
    let _ = fs::remove_dir_all(path).await;
}

fn should_force_copy_fallback() -> bool {
    static FORCE: OnceLock<bool> = OnceLock::new();
    *FORCE.get_or_init(|| std::env::var("AOS_TEST_FORCE_COPY").is_ok())
}

const EXDEV: i32 = 18; // Cross-device link

fn is_cross_device(err: &std::io::Error) -> bool {
    err.raw_os_error() == Some(EXDEV)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn finalize_uses_copy_fallback_when_forced() {
        std::env::set_var("AOS_TEST_FORCE_COPY", "1");
        let dir = tempdir().unwrap();
        let from = dir.path().join("from.aos");
        let to = dir.path().join("dest.aos");

        {
            let mut f = fs::File::create(&from).await.unwrap();
            f.write_all(b"hello").await.unwrap();
            f.sync_all().await.unwrap();
        }

        finalize_bundle_move(&from, &to).await.unwrap();

        assert!(!from.exists());
        assert!(to.exists());
        let data = fs::read(&to).await.unwrap();
        assert_eq!(data, b"hello");
        std::env::remove_var("AOS_TEST_FORCE_COPY");
    }
}
