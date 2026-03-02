//! UI constants and standardized strings.
//!
//! Centralizes wording for consistency across the UI.

/// Standardized UI action strings.
///
/// Use these constants for consistent wording across the application:
/// - `RETRY` for refetch/retry operations
/// - `RELOAD_PAGE` for full page reload
/// - `LOG_IN` for authentication actions
/// - `LOG_OUT` for logout actions
pub mod strings {
    /// Button text for retry/refetch operations
    pub const RETRY: &str = "Retry";

    /// Button text for full page reload
    pub const RELOAD_PAGE: &str = "Reload page";

    /// Button text for login action
    pub const LOG_IN: &str = "Log in";

    /// Button text for logout action
    pub const LOG_OUT: &str = "Log out";

    /// Button text for refresh data (not full page)
    pub const REFRESH: &str = "Refresh";
}

/// Human-facing product language for runtime concepts.
///
/// These labels intentionally avoid exposing backend implementation terms in
/// primary UI copy.
pub mod ui_language {
    /// Global identity badge in the top bar.
    pub const CONFIG_FINGERPRINT_LABEL: &str = "Current Configuration Fingerprint";
    pub const CONFIG_FINGERPRINT_LOADING: &str = "Fingerprint syncing";
    pub const CONFIG_FINGERPRINT_EMPTY: &str = "Fingerprint unavailable";
    pub const CONFIG_FINGERPRINT_COPY: &str = "Copy fingerprint";
    pub const CONFIG_FINGERPRINT_PROVENANCE: &str = "View proof trail";
    pub const CONFIG_FINGERPRINT_HELP: &str = "The trusted identity of the exact active setup.";

    /// Determinism/replay trust states.
    pub const REPRODUCIBLE_MODE: &str = "Reproducible Mode";
    pub const LOCKED_OUTPUT: &str = "Locked Output";
    pub const REPRODUCIBLE_READY: &str = "Outputs can be reproduced exactly.";
    pub const REPRODUCIBLE_PENDING: &str = "System is preparing reproducible execution safeguards.";

    /// Startup and runtime health wording.
    pub const KERNEL_BOOT_SEQUENCE: &str = "Kernel Boot Sequence";
    pub const BOOT_READY: &str = "Kernel ready";
    pub const BOOTING: &str = "Booting";
    pub const SELF_HEALING_OS: &str = "Self-Healing OS";
    pub const EVENT_VIEWER: &str = "Event Viewer";
    pub const SIGNED_SYSTEM_LOGS: &str = "Signed System Logs";
    pub const SYSTEM_RESTORE_POINTS: &str = "System Restore Points";
    pub const SAFETY_SHIELD: &str = "Safety Shield";

    /// Infrastructure surfaces translated into product language.
    pub const BASE_MODEL_REGISTRY: &str = "Base Model Registry";
    pub const REGISTER_NEW_BASE: &str = "Register New Base";
    pub const INFERENCE_ENGINES: &str = "Inference Engines";
    pub const UPDATE_CENTER: &str = "Update Center";
}

/// URL helpers for UI navigation.
pub mod urls {
    #[cfg(target_arch = "wasm32")]
    const RUNTIME_DOCS_URL_KEY: &str = "adapteros_runtime_docs_url";

    /// Default documentation base URL.
    /// Override at build time with `AOS_DOCS_URL`.
    pub const DEFAULT_DOCS_URL: &str = "/docs";

    fn build_docs_url() -> String {
        #[cfg(target_arch = "wasm32")]
        if let Some(url) = web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())
            .and_then(|s| s.get_item(RUNTIME_DOCS_URL_KEY).ok().flatten())
        {
            let trimmed = url.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }

        option_env!("AOS_DOCS_URL")
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_DOCS_URL)
            .to_string()
    }

    /// Persist a runtime docs URL override for future reads.
    pub fn set_runtime_docs_url(url: &str) {
        #[cfg(target_arch = "wasm32")]
        if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
            let trimmed = url.trim();
            if trimmed.is_empty() {
                let _ = storage.remove_item(RUNTIME_DOCS_URL_KEY);
            } else {
                let _ = storage.set_item(RUNTIME_DOCS_URL_KEY, trimmed);
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = url;
        }
    }

    /// Base docs URL used for help and documentation links.
    pub fn docs_url() -> String {
        build_docs_url()
    }

    /// Build a docs URL for a specific path segment.
    pub fn docs_link(path: &str) -> String {
        let base = docs_url();
        let base = base.trim_end_matches('/');
        let path = path.trim_start_matches('/');
        format!("{}/{}", base, path)
    }
}

/// External links for the UI.
pub mod links {
    /// Official documentation URL.
    pub fn docs_url() -> String {
        super::urls::docs_url()
    }
}

/// Pagination defaults for UI data loads.
pub mod pagination {
    /// Token decisions page size for trace detail views.
    pub const TOKEN_DECISIONS_PAGE_SIZE: u32 = 100;
    /// Max token decision rows to keep mounted in the DOM.
    pub const TOKEN_DECISIONS_DOM_CAP: usize = 400;
}
