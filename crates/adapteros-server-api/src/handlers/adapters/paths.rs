use adapteros_core::adapter_repo_paths::{
    resolve_adapter_roots_from_strings, RepoAdapterPaths, ENV_ADAPTERS_DIR_COMPAT,
};
use adapteros_core::path_security::reject_forbidden_tmp_path;
use std::env;
use std::path::Path;

/// Validate environment-sourced path for security issues.
///
/// Returns `None` if the path is invalid (forbidden location or contains path traversal).
fn validate_env_path(path_str: &str, env_var_name: &str) -> Option<String> {
    let path = Path::new(path_str);

    // Validate path is not in forbidden locations
    if let Err(e) = reject_forbidden_tmp_path(path, env_var_name) {
        tracing::error!(
            path = %path_str,
            env_var = %env_var_name,
            error = %e,
            "Invalid environment path: forbidden location"
        );
        return None;
    }

    // Check for path traversal sequences
    if path_str.contains("..") {
        tracing::error!(
            path = %path_str,
            env_var = %env_var_name,
            "Invalid environment path: contains path traversal sequence"
        );
        return None;
    }

    Some(path_str.to_string())
}

pub fn resolve_adapter_roots(state: &crate::state::AppState) -> RepoAdapterPaths {
    let repo_root = env::var("AOS_ADAPTERS_ROOT")
        .ok()
        .and_then(|p| validate_env_path(&p, "AOS_ADAPTERS_ROOT"))
        .or_else(|| {
            env::var(ENV_ADAPTERS_DIR_COMPAT)
                .ok()
                .and_then(|p| validate_env_path(&p, ENV_ADAPTERS_DIR_COMPAT))
        });
    let cache_root = env::var("AOS_ADAPTER_CACHE_DIR")
        .ok()
        .and_then(|p| validate_env_path(&p, "AOS_ADAPTER_CACHE_DIR"));
    let config_root = {
        let config = match state.config.read() {
            Ok(cfg) => cfg,
            Err(_) => {
                tracing::error!(
                    component = "adapter_paths",
                    reason = "config_lock_poisoned",
                    "Config lock poisoned while resolving adapter roots"
                );
                return resolve_adapter_roots_from_strings(repo_root, cache_root, None);
            }
        };
        if config.paths.adapters_root.is_empty() {
            None
        } else {
            Some(config.paths.adapters_root.clone())
        }
    };

    resolve_adapter_roots_from_strings(repo_root, cache_root, config_root)
}
