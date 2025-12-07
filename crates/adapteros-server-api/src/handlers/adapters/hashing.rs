use crate::error_helpers::internal_error;
use crate::types::ErrorResponse;
use adapteros_core::B3Hash;
use axum::http::StatusCode;
use axum::Json;
use std::path::Path;
use tokio::fs;
use tokio::io::AsyncReadExt;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ManifestHashInfo {
    pub manifest_hash: String,
    pub bundle_hash: String,
    pub combined_hash: String,
}

pub fn hash_bytes_b3(bytes: &[u8]) -> String {
    B3Hash::hash(bytes).to_hex()
}

pub fn hash_multi_bytes(chunks: &[&[u8]]) -> String {
    B3Hash::hash_multi(chunks).to_hex()
}

#[allow(dead_code)]
pub async fn hash_file_b3(path: &Path) -> Result<String, (StatusCode, Json<ErrorResponse>)> {
    let mut file = fs::File::open(path)
        .await
        .map_err(|e| internal_error(format!("Failed to open file {}: {}", path.display(), e)))?;

    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).await.map_err(|e| {
            internal_error(format!("Failed to read file {}: {}", path.display(), e))
        })?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    Ok(hasher.finalize().to_hex().to_string())
}

#[allow(dead_code)]
pub async fn hash_manifest_and_bundle(
    manifest_bytes: &[u8],
    bundle_path: &Path,
) -> Result<ManifestHashInfo, (StatusCode, Json<ErrorResponse>)> {
    let manifest_hash = hash_bytes_b3(manifest_bytes);
    let bundle_hash = hash_file_b3(bundle_path).await?;
    let combined_hash = hash_multi_bytes(&[manifest_bytes, bundle_hash.as_bytes()]);
    Ok(ManifestHashInfo {
        manifest_hash,
        bundle_hash,
        combined_hash,
    })
}
