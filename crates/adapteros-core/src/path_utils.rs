use crate::path_security::is_forbidden_tmp_path;
use std::path::{Component, Path, PathBuf};

/// Resolve a path to an absolute path, using current_dir for relative inputs.
pub fn absolutize_path<P: AsRef<Path>>(path: P) -> PathBuf {
    let path = path.as_ref();
    if path.is_absolute() {
        return path.to_path_buf();
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("/"))
        .join(path)
}

fn var_dir_override_raw() -> Option<String> {
    std::env::var("AOS_VAR_DIR")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn repo_root_from(start: &Path) -> Option<PathBuf> {
    // Best-effort: when running inside the AdapterOS repo, prefer anchoring
    // runtime paths under the workspace root to avoid crate-local `var/`.
    for dir in start.ancestors() {
        if dir.join("Cargo.lock").exists() || dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
    }
    None
}

fn runtime_base_dir() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    repo_root_from(&cwd).unwrap_or(cwd)
}

/// Resolve AOS_VAR_DIR (or default) as an absolute path, rejecting forbidden tmp paths.
pub fn resolve_var_dir() -> PathBuf {
    if let Some(raw) = var_dir_override_raw() {
        let raw_path = PathBuf::from(&raw);
        let candidate = if raw_path.is_absolute() {
            raw_path
        } else {
            runtime_base_dir().join(raw_path)
        };
        if is_forbidden_tmp_path(&candidate) {
            tracing::warn!(
                path = %candidate.display(),
                env = %"AOS_VAR_DIR",
                "Refusing to use forbidden temp directory for runtime state; falling back to default"
            );
            return runtime_base_dir().join("var");
        }
        return candidate;
    }

    runtime_base_dir().join("var")
}

fn strip_var_prefix(path: &Path) -> Option<PathBuf> {
    if path.is_absolute() {
        return None;
    }

    let mut components = path.components();
    if matches!(components.clone().next(), Some(Component::CurDir)) {
        components.next();
    }

    match components.next() {
        Some(Component::Normal(seg)) if seg == "var" => {
            let mut rest = PathBuf::new();
            for component in components {
                rest.push(component.as_os_str());
            }
            Some(rest)
        }
        _ => None,
    }
}

/// Rebase a relative var/ path under AOS_VAR_DIR when set, otherwise absolutize.
pub fn rebase_var_path<P: AsRef<Path>>(path: P) -> PathBuf {
    let path = path.as_ref();
    if path.is_absolute() {
        return path.to_path_buf();
    }

    // For relative `var/...` inputs, always rebase under the resolved runtime var dir,
    // so tests/binaries invoked from subdirectories don't create `crates/*/var`.
    if let Some(rest) = strip_var_prefix(path) {
        let base = resolve_var_dir();
        return if rest.as_os_str().is_empty() {
            base
        } else {
            base.join(rest)
        };
    }

    absolutize_path(path)
}
