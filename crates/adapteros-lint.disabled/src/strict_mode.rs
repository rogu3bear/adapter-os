//! Strict mode implementation for AdapterOS determinism enforcement
//!
//! Strict mode halts execution on the first nondeterminism violation detected.
//! This is used for testing and development to ensure code paths are deterministic.

use std::env;
use std::sync::atomic::{AtomicBool, Ordering};

/// Global strict mode flag
static STRICT_MODE_ENABLED: AtomicBool = AtomicBool::new(false);

/// Initialize strict mode from environment variables and command line args
pub fn init_strict_mode() {
    // Check for environment variable
    if env::var("ADAPTEROS_STRICT_MODE").is_ok() {
        STRICT_MODE_ENABLED.store(true, Ordering::Relaxed);
        eprintln!("🔒 AdapterOS strict mode enabled via environment variable");
        return;
    }

    // Check command line arguments
    let args: Vec<String> = env::args().collect();
    for arg in &args {
        if arg == "--strict" || arg == "--deterministic" {
            STRICT_MODE_ENABLED.store(true, Ordering::Relaxed);
            eprintln!("🔒 AdapterOS strict mode enabled via command line");
            return;
        }
    }
}

/// Check if strict mode is enabled
pub fn is_strict_mode() -> bool {
    STRICT_MODE_ENABLED.load(Ordering::Relaxed)
}

/// Enable strict mode programmatically
pub fn enable_strict_mode() {
    STRICT_MODE_ENABLED.store(true, Ordering::Relaxed);
    eprintln!("🔒 AdapterOS strict mode enabled programmatically");
}

/// Disable strict mode programmatically
pub fn disable_strict_mode() {
    STRICT_MODE_ENABLED.store(false, Ordering::Relaxed);
    eprintln!("🔓 AdapterOS strict mode disabled");
}

/// Macro to check strict mode and panic if enabled
#[macro_export]
macro_rules! strict_mode_check {
    ($violation_type:expr, $details:expr) => {
        if adapteros_lint::strict_mode::is_strict_mode() {
            panic!(
                "🚨 STRICT MODE VIOLATION: {} - {}",
                $violation_type, $details
            );
        }
    };
}

/// Macro to check strict mode and return error if enabled
#[macro_export]
macro_rules! strict_mode_result {
    ($violation_type:expr, $details:expr) => {
        if adapteros_lint::strict_mode::is_strict_mode() {
            return Err(adapteros_core::AosError::DeterminismViolation(format!(
                "Strict mode violation: {} - {}",
                $violation_type, $details
            )));
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strict_mode_disabled_by_default() {
        assert!(!is_strict_mode());
    }

    #[test]
    fn test_enable_disable_strict_mode() {
        assert!(!is_strict_mode());
        
        enable_strict_mode();
        assert!(is_strict_mode());
        
        disable_strict_mode();
        assert!(!is_strict_mode());
    }
}
