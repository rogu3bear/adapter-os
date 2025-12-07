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

    pub fn dataset_dir(&self, dataset_id: &str) -> PathBuf {
        self.files.join(dataset_id)
    }

    pub fn dataset_temp_dir(&self, dataset_id: &str) -> PathBuf {
        self.temp.join(dataset_id)
    }
}

/// Resolve dataset root preferring env override and returning an absolute path.
pub fn resolve_dataset_root(state: &crate::state::AppState) -> PathBuf {
    let root_str = env::var("AOS_DATASETS_DIR").unwrap_or_else(|_| {
        let config = state.config.read().expect("Config lock poisoned");
        config.paths.datasets_root.clone()
    });

    let root = PathBuf::from(root_str);
    if root.is_absolute() {
        return root;
    }

    env::current_dir()
        .unwrap_or_else(|_| Path::new("/").to_path_buf())
        .join(root)
}
