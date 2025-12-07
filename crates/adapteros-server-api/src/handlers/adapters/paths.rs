use adapteros_core::adapter_repo_paths::{
    resolve_adapter_roots_from_strings, AdapterPaths, ENV_ADAPTERS_DIR_COMPAT,
};
use std::env;

pub fn resolve_adapter_roots(state: &crate::state::AppState) -> AdapterPaths {
    let repo_root = env::var("AOS_ADAPTERS_ROOT")
        .ok()
        .or_else(|| env::var(ENV_ADAPTERS_DIR_COMPAT).ok());
    let cache_root = env::var("AOS_ADAPTER_CACHE_DIR").ok();
    let config_root = {
        let config = state.config.read().expect("Config lock poisoned");
        if config.paths.adapters_root.is_empty() {
            None
        } else {
            Some(config.paths.adapters_root.clone())
        }
    };

    resolve_adapter_roots_from_strings(repo_root, cache_root, config_root)
}
