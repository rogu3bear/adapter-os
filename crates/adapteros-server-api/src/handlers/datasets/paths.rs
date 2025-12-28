use adapteros_core::{reject_forbidden_tmp_path, AosError, Result};
use std::env;
use std::path::{Path, PathBuf};

pub const FILES_DIR_NAME: &str = "files";
pub const TEMP_DIR_NAME: &str = "temp";
pub const CHUNKED_DIR_NAME: &str = "chunked";
pub const LOGS_DIR_NAME: &str = "logs";

#[derive(Debug, Clone)]
pub struct DatasetPaths {
    pub files: PathBuf,
    pub temp: PathBuf,
    pub chunked: PathBuf,
    pub logs: PathBuf,
}

impl DatasetPaths {
    pub fn new(root: PathBuf) -> Self {
        Self {
            files: root.join(FILES_DIR_NAME),
            temp: root.join(TEMP_DIR_NAME),
            chunked: root.join(CHUNKED_DIR_NAME),
            logs: root.join(LOGS_DIR_NAME),
        }
    }

    pub fn dataset_dir(&self, workspace_id: &str, dataset_id: &str) -> PathBuf {
        self.files.join(workspace_id).join(dataset_id)
    }

    pub fn dataset_temp_dir(&self, workspace_id: &str, dataset_id: &str) -> PathBuf {
        self.temp.join(workspace_id).join(dataset_id)
    }

    /// Legacy helper for callers that do not yet provide workspace scoping.
    pub fn dataset_dir_unscoped(&self, dataset_id: &str) -> PathBuf {
        self.files.join(dataset_id)
    }
}

/// Resolve dataset root preferring env override and returning an absolute path.
pub fn resolve_dataset_root(state: &crate::state::AppState) -> Result<PathBuf> {
    let root_str = env::var("AOS_DATASETS_DIR").unwrap_or_else(|_| match state.config.read() {
        Ok(config) => config.paths.datasets_root.clone(),
        Err(_) => {
            tracing::error!("Config lock poisoned in resolve_dataset_root");
            "var/datasets".to_string()
        }
    });

    let root = PathBuf::from(root_str);
    if root.is_absolute() {
        reject_forbidden_tmp_path(&root, "datasets-root")?;
        // SECURITY: Canonicalize to resolve symlinks after validation
        // This prevents symlink attacks that bypass the /tmp check
        let canonical = root
            .canonicalize()
            .map_err(|e| AosError::Validation(format!("Invalid datasets root path: {}", e)))?;
        reject_forbidden_tmp_path(&canonical, "datasets-root-canonical")?;
        return Ok(canonical);
    }

    let resolved = env::current_dir()
        .unwrap_or_else(|_| Path::new("/").to_path_buf())
        .join(root);
    reject_forbidden_tmp_path(&resolved, "datasets-root")?;
    // SECURITY: Canonicalize to resolve symlinks after validation
    let canonical = resolved
        .canonicalize()
        .map_err(|e| AosError::Validation(format!("Invalid datasets path: {}", e)))?;
    reject_forbidden_tmp_path(&canonical, "datasets-root-canonical")?;
    Ok(canonical)
}
