//! Shared path policy enforcement for canonicalization and allowed-root checks.

use super::traversal::check_path_traversal;
use adapteros_core::{AosError, Result};
use std::path::{Path, PathBuf};
use tracing::{error, warn};

/// Canonicalize a path strictly (must exist) after traversal checks.
pub fn canonicalize_strict(path: impl AsRef<Path>) -> Result<PathBuf> {
    let path = path.as_ref();
    check_path_traversal(path)?;

    let canonical = path.canonicalize().map_err(|e| {
        error!(
            original = %path.display(),
            canonical = "<unavailable>",
            error = %e,
            "Failed to canonicalize path"
        );
        if e.kind() == std::io::ErrorKind::NotFound {
            AosError::NotFound(format!("Path does not exist: {}", path.display()))
        } else {
            AosError::Io(format!(
                "Failed to canonicalize path '{}': {}",
                path.display(),
                e
            ))
        }
    })?;

    Ok(canonical)
}

/// Enforce that a canonicalized path stays within an allowed root.
pub fn enforce_allowed_root(
    canonical_path: impl AsRef<Path>,
    allowed_root: impl AsRef<Path>,
) -> Result<()> {
    let canonical_path = canonical_path.as_ref();
    let canonical_root = canonicalize_strict(allowed_root)?;

    if canonical_path.starts_with(&canonical_root) {
        Ok(())
    } else {
        Err(AosError::Validation(format!(
            "Path '{}' is not within allowed root '{}'",
            canonical_path.display(),
            canonical_root.display()
        )))
    }
}

/// Reject paths that escape allowed roots after canonicalization.
pub fn reject_symlink_escape(
    original: impl AsRef<Path>,
    canonical: impl AsRef<Path>,
    allowed_root: impl AsRef<Path>,
) -> Result<()> {
    let original = original.as_ref();
    let canonical = canonical.as_ref();
    let canonical_root = canonicalize_strict(allowed_root)?;

    if canonical.starts_with(&canonical_root) {
        Ok(())
    } else {
        error!(
            original = %original.display(),
            canonical = %canonical.display(),
            allowed_root = %canonical_root.display(),
            "Path rejected: canonical path escapes allowed root"
        );
        Err(AosError::Validation(format!(
            "Path '{}' (canonical '{}') escapes allowed root '{}'",
            original.display(),
            canonical.display(),
            canonical_root.display()
        )))
    }
}

/// Canonicalize a path and enforce it stays within one of the allowed roots.
pub fn canonicalize_strict_in_allowed_roots<P: AsRef<Path>, R: AsRef<Path>>(
    path: P,
    allowed_roots: &[R],
) -> Result<PathBuf> {
    if allowed_roots.is_empty() {
        return Err(AosError::Config(
            "No allowed roots configured for path policy".to_string(),
        ));
    }

    let original = path.as_ref();
    let canonical = canonicalize_strict(original)?;

    for root in allowed_roots {
        if enforce_allowed_root(&canonical, root).is_ok() {
            reject_symlink_escape(original, &canonical, root)?;
            return Ok(canonical);
        }
    }

    warn!(
        original = %original.display(),
        canonical = %canonical.display(),
        "Path rejected: not within allowed roots"
    );

    Err(AosError::Validation(format!(
        "Path '{}' (canonical '{}') is not within allowed roots",
        original.display(),
        canonical.display()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::common::PlatformUtils;
    use tempfile::{Builder, TempDir};

    fn new_temp_dir() -> TempDir {
        let root = PlatformUtils::temp_dir();
        std::fs::create_dir_all(&root).expect("create var tmp");
        Builder::new()
            .prefix("aos-test-")
            .tempdir_in(&root)
            .expect("tempdir")
    }

    #[test]
    fn test_canonicalize_strict_rejects_traversal() {
        let temp_dir = new_temp_dir();
        let traversal_path = temp_dir.path().join("..").join("escape.txt");
        let result = canonicalize_strict(&traversal_path);
        assert!(result.is_err(), "Traversal path should be rejected");
    }

    #[test]
    fn test_canonicalize_strict_enforces_allowed_root() -> Result<()> {
        let temp_dir = new_temp_dir();
        let allowed_root = temp_dir.path().join("allowed");
        let outside_root = temp_dir.path().join("outside");
        std::fs::create_dir_all(&allowed_root)?;
        std::fs::create_dir_all(&outside_root)?;

        let allowed_file = allowed_root.join("ok.txt");
        let outside_file = outside_root.join("nope.txt");
        std::fs::write(&allowed_file, "ok")?;
        std::fs::write(&outside_file, "no")?;

        let resolved = canonicalize_strict_in_allowed_roots(&allowed_file, &[&allowed_root])?;
        assert!(resolved.starts_with(&allowed_root.canonicalize()?));
        assert!(
            canonicalize_strict_in_allowed_roots(&outside_file, &[&allowed_root]).is_err(),
            "Outside path should be rejected"
        );

        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn test_canonicalize_strict_blocks_symlink_escape() -> Result<()> {
        use std::os::unix::fs::symlink;

        let temp_dir = new_temp_dir();
        let allowed_root = temp_dir.path().join("allowed");
        let outside_root = temp_dir.path().join("outside");
        std::fs::create_dir_all(&allowed_root)?;
        std::fs::create_dir_all(&outside_root)?;

        let escape_link = allowed_root.join("escape");
        symlink(&outside_root, &escape_link)?;

        let escaped_file = outside_root.join("secret.txt");
        std::fs::write(&escaped_file, "secret")?;

        let candidate = escape_link.join("secret.txt");
        let result = canonicalize_strict_in_allowed_roots(&candidate, &[&allowed_root]);
        assert!(result.is_err(), "Symlink escape should be rejected");

        Ok(())
    }
}
