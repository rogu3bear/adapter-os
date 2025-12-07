use super::adapter_fs_utils::{ensure_repo_dirs, finalize_bundle_move};
use super::adapter_paths::resolve_adapter_roots;
use super::adapter_progress::emit_adapter_progress;
use super::adapter_tenant::bind_adapter_to_tenant;
use crate::error_helpers::internal_error;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_core::{B3Hash, RepoAdapterPaths};
use adapteros_db::AdapterRegistrationParams;
use axum::http::StatusCode;
use axum::Json;
use std::path::PathBuf;
use tokio::fs;
use tracing::{error, warn};

#[derive(thiserror::Error, Debug)]
pub enum AdapterRepoError {
    #[error("io error: {0}")]
    Io(String),
    #[error("invalid path: {0}")]
    InvalidPath(String),
    #[allow(dead_code)]
    #[error("hash error: {0}")]
    Hash(String),
    #[error("db error: {0}")]
    Db(String),
    #[error("policy error: {0}")]
    Policy(String),
}

pub struct StoreBundleRequest {
    pub tenant_id: String,
    pub adapter_name: String,
    pub version: String,
    pub temp_path: PathBuf,
    pub precomputed_hash: Option<String>,
}

pub struct StoreBundleResult {
    pub final_path: PathBuf,
    pub manifest_hash: String,
}

#[async_trait::async_trait]
pub trait AdapterRepo {
    async fn store_bundle(
        &self,
        req: StoreBundleRequest,
    ) -> Result<StoreBundleResult, AdapterRepoError>;

    async fn register_bundle(
        &self,
        adapter_id: &str,
        tenant_id: &str,
        params: AdapterRegistrationParams,
    ) -> Result<String, AdapterRepoError>;
}

pub struct DefaultAdapterRepo<'a> {
    state: &'a AppState,
    paths: RepoAdapterPaths,
}

pub fn map_repo_error(err: AdapterRepoError) -> (StatusCode, Json<ErrorResponse>) {
    match err {
        AdapterRepoError::InvalidPath(msg) => {
            warn!(repo_error_kind = "invalid_path", error = %msg, "adapter repo error");
            (
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("invalid adapter path")
                        .with_code("INVALID_PATH")
                        .with_string_details(msg),
                ),
            )
        }
        AdapterRepoError::Hash(msg) => {
            warn!(repo_error_kind = "hash", error = %msg, "adapter repo error");
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(
                    ErrorResponse::new("invalid adapter hash")
                        .with_code("HASH_ERROR")
                        .with_string_details(msg),
                ),
            )
        }
        AdapterRepoError::Db(msg) => {
            error!(repo_error_kind = "db", error = %msg, "adapter repo error");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(msg),
                ),
            )
        }
        AdapterRepoError::Policy(msg) => (
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("policy violation")
                    .with_code("POLICY_VIOLATION")
                    .with_string_details(msg),
            ),
        ),
        AdapterRepoError::Io(msg) => {
            error!(repo_error_kind = "io", error = %msg, "adapter repo error");
            internal_error(msg)
        }
    }
}

impl<'a> DefaultAdapterRepo<'a> {
    pub fn new(state: &'a AppState) -> Self {
        let paths = resolve_adapter_roots(state);
        Self { state, paths }
    }

    async fn hash_file(path: &PathBuf) -> Result<String, AdapterRepoError> {
        let data = fs::read(path)
            .await
            .map_err(|e| AdapterRepoError::Io(e.to_string()))?;
        Ok(B3Hash::hash(&data).to_hex())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_repo_error_produces_http_status() {
        let (status, _) = map_repo_error(AdapterRepoError::Db("boom".into()));
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);

        let (status, _) = map_repo_error(AdapterRepoError::Policy("deny".into()));
        assert_eq!(status, StatusCode::FORBIDDEN);

        let (status, _) = map_repo_error(AdapterRepoError::InvalidPath("bad".into()));
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, _) = map_repo_error(AdapterRepoError::Hash("bad".into()));
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

        let (status, _) = map_repo_error(AdapterRepoError::Io("disk".into()));
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }
}

#[async_trait::async_trait]
impl<'a> AdapterRepo for DefaultAdapterRepo<'a> {
    async fn store_bundle(
        &self,
        req: StoreBundleRequest,
    ) -> Result<StoreBundleResult, AdapterRepoError> {
        ensure_repo_dirs(&self.paths, &req.tenant_id, &req.adapter_name)
            .await
            .map_err(|e| AdapterRepoError::Io(format!("{:?}", e)))?;

        let final_path = self
            .paths
            .bundle_path(&req.tenant_id, &req.adapter_name, &req.version)
            .map_err(|err| AdapterRepoError::InvalidPath(err.to_string()))?;

        if let Err(e) = finalize_bundle_move(&req.temp_path, &final_path).await {
            let _ = fs::remove_file(&req.temp_path).await;
            return Err(AdapterRepoError::Io(format!("{:?}", e)));
        }

        let manifest_hash = if let Some(hash) = req.precomputed_hash {
            hash
        } else {
            Self::hash_file(&final_path).await?
        };

        Ok(StoreBundleResult {
            final_path,
            manifest_hash,
        })
    }

    async fn register_bundle(
        &self,
        adapter_id: &str,
        tenant_id: &str,
        params: AdapterRegistrationParams,
    ) -> Result<String, AdapterRepoError> {
        let registered_id = self
            .state
            .db
            .register_adapter(params)
            .await
            .map_err(|e| AdapterRepoError::Db(e.to_string()))?;

        bind_adapter_to_tenant(&self.state.db, adapter_id, tenant_id)
            .await
            .map_err(|e| AdapterRepoError::Policy(format!("{:?}", e)))?;

        emit_adapter_progress(
            adapter_id,
            "registered",
            None,
            90.0,
            "Registered adapter in repository",
        );

        Ok(registered_id)
    }
}
