use crate::{AosError, Result};
use std::path::Path;

pub const FORBIDDEN_TMP_PREFIXES: [&str; 3] = ["/tmp", "/private/tmp", "/var/tmp"];

pub fn is_forbidden_tmp_path_str(candidate: &str) -> bool {
    candidate == "/tmp"
        || candidate.starts_with("/tmp/")
        || candidate == "/private/tmp"
        || candidate.starts_with("/private/tmp/")
        || candidate == "/var/tmp"
        || candidate.starts_with("/var/tmp/")
}

pub fn is_forbidden_tmp_path(path: &Path) -> bool {
    is_forbidden_tmp_path_str(&path.to_string_lossy())
}

pub fn reject_forbidden_tmp_path(path: &Path, kind: &str) -> Result<()> {
    if is_forbidden_tmp_path(path) {
        return Err(AosError::Config(format!(
            "{kind} path must not be under /tmp: {}",
            path.display()
        )));
    }
    Ok(())
}

pub fn reject_forbidden_tmp_path_like(candidate: &str, kind: &str) -> Result<()> {
    let mut normalized = candidate;

    for prefix in ["sqlite://", "sqlite:", "file://", "file:"] {
        if let Some(stripped) = normalized.strip_prefix(prefix) {
            normalized = stripped;
            break;
        }
    }

    while normalized.starts_with("//") {
        normalized = &normalized[1..];
    }

    if is_forbidden_tmp_path_str(normalized) {
        return Err(AosError::Config(format!(
            "{kind} path must not be under /tmp: {candidate}"
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn tmp_detection_rejects_tmp_and_private_tmp() {
        assert!(is_forbidden_tmp_path(&PathBuf::from("/tmp/file.txt")));
        assert!(is_forbidden_tmp_path(&PathBuf::from(
            "/private/tmp/file.txt"
        )));
        assert!(is_forbidden_tmp_path(&PathBuf::from("/var/tmp/file.txt")));
        assert!(!is_forbidden_tmp_path(&PathBuf::from(
            "/var/run/adapteros.sock"
        )));
    }

    #[test]
    fn path_like_rejects_schemes() {
        reject_forbidden_tmp_path_like("sqlite:///tmp/cp.db", "database-url").unwrap_err();
        reject_forbidden_tmp_path_like("file:///private/tmp/cp.db", "database-url").unwrap_err();
    }
}
