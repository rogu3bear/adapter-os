//! Debug logging utilities
//!
//! Provides conditional logging macros that compile to console output in debug builds
//! and are completely stripped from release builds.
//!
//! # Usage
//!
//! ```rust
//! debug_log!("[Component] Initialized");
//! debug_warn!("[Component] Unusual state: {}", state);
//! debug_error!("[Component] Failed: {}", error);
//! ```
//!
//! In debug builds (`cargo build`), these expand to `web_sys::console::log_1`.
//! In release builds (`cargo build --release`), they expand to nothing (zero cost).

/// Debug logging macro that compiles to console.log in debug builds,
/// and is completely stripped in release builds.
///
/// # Example
/// ```
/// debug_log!("[App] Rendering component");
/// debug_log!("[Worker] Processed {} items", count);
/// ```
#[macro_export]
macro_rules! debug_log {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        {
            web_sys::console::log_1(&format!($($arg)*).into());
        }
    };
}

/// Debug warning macro (console.warn in debug builds only).
///
/// # Example
/// ```
/// debug_warn!("[Auth] Token near expiration");
/// ```
#[macro_export]
macro_rules! debug_warn {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        {
            web_sys::console::warn_1(&format!($($arg)*).into());
        }
    };
}

/// Debug error macro (console.error in debug builds only).
///
/// # Example
/// ```
/// debug_error!("[API] Request failed: {}", error);
/// ```
#[macro_export]
macro_rules! debug_error {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        {
            web_sys::console::error_1(&format!($($arg)*).into());
        }
    };
}
