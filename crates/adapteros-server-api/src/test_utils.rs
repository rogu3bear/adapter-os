#[cfg(test)]
use std::path::{Path, PathBuf};

#[cfg(test)]
use std::sync::OnceLock;

#[cfg(test)]
use tempfile::TempDir;

#[cfg(test)]
fn workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_dir
        .ancestors()
        .nth(2)
        .unwrap_or(manifest_dir.as_path());
    root.to_path_buf()
}

#[cfg(test)]
pub(crate) fn test_temp_base_dir() -> PathBuf {
    static BASE: OnceLock<PathBuf> = OnceLock::new();
    BASE.get_or_init(|| {
        let base = workspace_root().join("var").join("tmp");
        let _ = std::fs::create_dir_all(&base);
        base
    })
    .clone()
}

#[cfg(test)]
pub(crate) fn tempdir_with_prefix(prefix: &str) -> TempDir {
    tempfile::Builder::new()
        .prefix(prefix)
        .tempdir_in(test_temp_base_dir())
        .expect("create temp dir")
}

#[cfg(test)]
pub(crate) fn tempdir() -> TempDir {
    tempdir_with_prefix("aos-test-")
}
