use crate::error::{BootError, BootResult};
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Exit code for configuration or environment errors that should not panic.
pub const EXIT_CONFIG_ERROR: i32 = 1;

/// Runtime directory selection outcome.
#[derive(Debug, Clone)]
pub struct RuntimeDir {
    /// Directory that should be used for runtime state.
    pub path: PathBuf,
    /// Whether a fallback (ephemeral) directory was used because the preferred path was not writable.
    pub used_fallback: bool,
}

/// Ensure a writable runtime directory exists.
///
/// Attempts to create and write to the preferred path first. If that path is
/// read-only or cannot be created, a fallback path (default: system temp dir)
/// is used instead. Returns an error if neither location is writable.
pub fn ensure_runtime_dir<P: AsRef<Path>>(
    preferred: P,
    fallback: Option<P>,
) -> BootResult<RuntimeDir> {
    let preferred = preferred.as_ref();
    if is_writable(preferred) {
        info!(path = %preferred.display(), "Runtime directory writable");
        return Ok(RuntimeDir {
            path: preferred.to_path_buf(),
            used_fallback: false,
        });
    }

    let fallback_path = fallback
        .as_ref()
        .map(|p| p.as_ref().to_path_buf())
        .unwrap_or_else(|| std::env::temp_dir().join("adapteros-ephemeral"));

    warn!(
        path = %preferred.display(),
        fallback = %fallback_path.display(),
        "Preferred runtime directory not writable; switching to ephemeral fallback"
    );

    if is_writable(&fallback_path) {
        return Ok(RuntimeDir {
            path: fallback_path,
            used_fallback: true,
        });
    }

    Err(BootError::Config(format!(
        "Runtime directory not writable: {} (fallback: {})",
        preferred.display(),
        fallback_path.display()
    )))
}

fn is_writable(path: &Path) -> bool {
    if let Err(e) = fs::create_dir_all(path) {
        // Only treat permission/read-only errors as unwritable; other errors propagate.
        if e.kind() == std::io::ErrorKind::PermissionDenied
            || e.kind() == std::io::ErrorKind::ReadOnlyFilesystem
        {
            return false;
        }
        // For non-permission errors, consider the path unusable.
        return false;
    }

    let probe_path = path.join(".aos-permcheck");
    let result = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&probe_path)
        .and_then(|_| fs::remove_file(&probe_path));

    result.is_ok()
}
