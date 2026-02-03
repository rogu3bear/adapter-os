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

/// URL helpers for UI navigation.
pub mod urls {
    /// Default documentation base URL.
    /// Override at build time with `AOS_DOCS_URL`.
    pub const DEFAULT_DOCS_URL: &str = "/docs";

    /// Base docs URL used for help and documentation links.
    pub fn docs_url() -> &'static str {
        option_env!("AOS_DOCS_URL").unwrap_or(DEFAULT_DOCS_URL)
    }

    /// Build a docs URL for a specific path segment.
    pub fn docs_link(path: &str) -> String {
        let base = docs_url().trim_end_matches('/');
        let path = path.trim_start_matches('/');
        format!("{}/{}", base, path)
    }
}

/// External links for the UI.
pub mod links {
    /// Official documentation URL.
    /// TODO: source from runtime config when available.
    pub const DOCS_URL: &str = "https://docs.adapteros.com";
}
