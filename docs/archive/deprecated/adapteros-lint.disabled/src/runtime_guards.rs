//! Runtime guards for nondeterminism detection
//!
//! This module provides runtime guards that panic when nondeterministic
//! operations are detected. These guards are used in `--strict` mode to
//! halt execution on first impurity detection.


use tracing::info;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Global state for runtime guards
static GUARDS_ENABLED: AtomicBool = AtomicBool::new(false);
static STRICT_MODE: AtomicBool = AtomicBool::new(false);
static VIOLATION_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Configuration for runtime guards
#[derive(Debug, Clone)]
pub struct GuardConfig {
    /// Whether guards are enabled
    pub enabled: bool,
    /// Whether to panic on first violation (strict mode)
    pub strict_mode: bool,
    /// Maximum number of violations before panic
    pub max_violations: u64,
    /// Whether to log violations
    pub log_violations: bool,
}

impl Default for GuardConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            strict_mode: false,
            max_violations: 1,
            log_violations: true,
        }
    }
}

/// Initialize runtime guards with configuration
pub fn init_guards(config: GuardConfig) {
    GUARDS_ENABLED.store(config.enabled, Ordering::Relaxed);
    STRICT_MODE.store(config.strict_mode, Ordering::Relaxed);
    
    if config.enabled {
        einfo!("🔒 AdapterOS determinism guards enabled");
        if config.strict_mode {
            einfo!("⚠️  Strict mode: will panic on first nondeterminism violation");
        }
    }
}

/// Check if guards are enabled
pub fn guards_enabled() -> bool {
    GUARDS_ENABLED.load(Ordering::Relaxed)
}

/// Check if strict mode is enabled
pub fn strict_mode() -> bool {
    STRICT_MODE.load(Ordering::Relaxed)
}

/// Report a nondeterminism violation
pub fn report_violation(violation_type: &str, details: &str) {
    if !guards_enabled() {
        return;
    }

    let count = VIOLATION_COUNT.fetch_add(1, Ordering::Relaxed);
    
    if strict_mode() {
        panic!(
            "🚨 NONDETERMINISM VIOLATION DETECTED (strict mode)\n\
             Type: {}\n\
             Details: {}\n\
             Violation #{}",
            violation_type, details, count + 1
        );
    } else {
        einfo!(
            "⚠️  Nondeterminism violation #{}: {} - {}",
            count + 1, violation_type, details
        );
        
        // Check if we've exceeded max violations
        if count + 1 >= 10 { // Default max violations
            panic!(
                "🚨 Too many nondeterminism violations detected ({}). Halting execution.",
                count + 1
            );
        }
    }
}

/// Guard for spawn_blocking calls
pub fn guard_spawn_blocking() {
    report_violation(
        "spawn_blocking",
        "tokio::task::spawn_blocking detected - non-deterministic thread pool scheduling"
    );
}

/// Guard for wall-clock time usage
pub fn guard_wall_clock_time(function_name: &str) {
    report_violation(
        "wall_clock_time",
        &format!("{} detected - wall-clock time introduces non-determinism", function_name)
    );
}

/// Guard for random number generation
pub fn guard_random_generation(function_name: &str) {
    report_violation(
        "random_generation",
        &format!("{} detected - unseeded random number generation", function_name)
    );
}

/// Guard for file I/O operations
pub fn guard_file_io(operation: &str) {
    report_violation(
        "file_io",
        &format!("File I/O operation '{}' detected - can introduce non-determinism", operation)
    );
}

/// Guard for system calls
pub fn guard_syscall(operation: &str) {
    report_violation(
        "syscall",
        &format!("System call '{}' detected - can introduce non-determinism", operation)
    );
}

/// Wrapper for SystemTime::now() that checks guards
pub fn guarded_system_time_now() -> SystemTime {
    if guards_enabled() {
        guard_wall_clock_time("SystemTime::now()");
    }
    SystemTime::now()
}

/// Wrapper for Instant::now() that checks guards
pub fn guarded_instant_now() -> std::time::Instant {
    if guards_enabled() {
        guard_wall_clock_time("Instant::now()");
    }
    std::time::Instant::now()
}

/// Wrapper for random number generation that checks guards
pub fn guarded_random<T>() -> T 
where
    rand::distributions::Standard: rand::distributions::Distribution<T>,
{
    if guards_enabled() {
        guard_random_generation("rand::random()");
    }
    rand::random()
}

/// Wrapper for thread_rng that checks guards
pub fn guarded_thread_rng() -> rand::rngs::ThreadRng {
    if guards_enabled() {
        guard_random_generation("rand::thread_rng()");
    }
    rand::thread_rng()
}

/// Get current violation count
pub fn violation_count() -> u64 {
    VIOLATION_COUNT.load(Ordering::Relaxed)
}

/// Reset violation count
pub fn reset_violation_count() {
    VIOLATION_COUNT.store(0, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guard_config_default() {
        let config = GuardConfig::default();
        assert!(!config.enabled);
        assert!(!config.strict_mode);
        assert_eq!(config.max_violations, 1);
        assert!(config.log_violations);
    }

    #[test]
    fn test_guards_disabled_by_default() {
        assert!(!guards_enabled());
        assert!(!strict_mode());
    }

    #[test]
    fn test_violation_count_starts_at_zero() {
        assert_eq!(violation_count(), 0);
    }

    #[test]
    fn test_reset_violation_count() {
        reset_violation_count();
        assert_eq!(violation_count(), 0);
    }
}
