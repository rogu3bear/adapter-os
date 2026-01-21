use anyhow::Result;
use std::path::{Path, PathBuf};

/// Resolve a bundle path from a file or directory.
///
/// If `path` is a directory, it looks for `bundle_000000.ndjson` inside it.
/// If `path` is a file, it returns it directly.
///
/// This helper unifies behavior across `aosctl golden create`, `verify`, and `replay` commands.
pub fn resolve_bundle_path(path: &Path) -> Result<PathBuf> {
    if path.is_dir() {
        let candidate = path.join("bundle_000000.ndjson");
        if candidate.exists() {
            Ok(candidate)
        } else {
            anyhow::bail!(
                "Directory provided but no 'bundle_000000.ndjson' found in: {}",
                path.display()
            );
        }
    } else {
        Ok(path.to_path_buf())
    }
}
