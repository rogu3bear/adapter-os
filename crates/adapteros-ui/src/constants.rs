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

    /// Message for version skew banner title
    pub const VERSION_AVAILABLE: &str = "A new version is available";

    /// Message for offline banner title
    pub const BACKEND_OFFLINE: &str = "Backend offline";

    /// Message for offline banner description
    pub const CACHED_DATA_AVAILABLE: &str = "You can keep viewing cached data.";
}
