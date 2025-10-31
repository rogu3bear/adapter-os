//! Configuration guards to prevent environment variable access after freeze

use crate::types::*;
use adapteros_core::{AosError, Result};
use std::sync::{OnceLock, RwLock};

/// Global guard state
static GUARD_STATE: OnceLock<RwLock<GuardState>> = OnceLock::new();

/// Guard state tracking
#[derive(Debug, Clone)]
struct GuardState {
    frozen: bool,
    violations: Vec<ConfigFreezeError>,
    stack_traces: bool,
}

impl GuardState {
    fn new() -> Self {
        Self {
            frozen: false,
            violations: Vec::new(),
            stack_traces: true,
        }
    }

    fn freeze(&mut self) {
        self.frozen = true;
    }

    fn add_violation(&mut self, error: ConfigFreezeError) {
        self.violations.push(error);
    }

    fn get_violations(&self) -> Vec<ConfigFreezeError> {
        self.violations.clone()
    }
}

/// Configuration guards for enforcing freeze behavior
pub struct ConfigGuards;

impl ConfigGuards {
    /// Initialize the guard system
    pub fn initialize() -> Result<()> {
        if let Some(lock) = GUARD_STATE.get() {
            let mut state = lock
                .write()
                .map_err(|_| AosError::Config("Failed to acquire guard state lock".to_string()))?;
            *state = GuardState::new();
            return Ok(());
        }

        GUARD_STATE
            .set(RwLock::new(GuardState::new()))
            .map_err(|_| AosError::Config("Guard system already initialized".to_string()))?;
        Ok(())
    }

    /// Freeze the guard system, preventing further environment variable access
    pub fn freeze() -> Result<()> {
        let guard_state = GUARD_STATE
            .get()
            .ok_or_else(|| AosError::Config("Guard system not initialized".to_string()))?;

        let mut state = guard_state
            .write()
            .map_err(|_| AosError::Config("Failed to acquire guard state lock".to_string()))?;

        state.freeze();

        tracing::info!("Configuration guards frozen - environment variable access now prohibited");
        Ok(())
    }

    /// Check if guards are frozen
    pub fn is_frozen() -> bool {
        GUARD_STATE
            .get()
            .and_then(|state| state.read().ok())
            .map(|state| state.frozen)
            .unwrap_or(false)
    }

    /// Record a violation of the freeze
    pub fn record_violation(operation: &str, message: &str) -> Result<()> {
        let guard_state = GUARD_STATE
            .get()
            .ok_or_else(|| AosError::Config("Guard system not initialized".to_string()))?;

        let mut state = guard_state
            .write()
            .map_err(|_| AosError::Config("Failed to acquire guard state lock".to_string()))?;

        let stack_trace = if state.stack_traces {
            Some(format!("{:?}", std::backtrace::Backtrace::capture()))
        } else {
            None
        };

        let error = ConfigFreezeError {
            message: message.to_string(),
            attempted_operation: operation.to_string(),
            stack_trace,
        };

        state.add_violation(error);

        tracing::error!(
            "Configuration freeze violation: {} - {}",
            operation,
            message
        );
        Ok(())
    }

    /// Get all recorded violations
    pub fn get_violations() -> Result<Vec<ConfigFreezeError>> {
        let guard_state = GUARD_STATE
            .get()
            .ok_or_else(|| AosError::Config("Guard system not initialized".to_string()))?;

        let state = guard_state
            .read()
            .map_err(|_| AosError::Config("Failed to acquire guard state lock".to_string()))?;

        Ok(state.get_violations())
    }

    /// Clear all violations (for testing)
    pub fn clear_violations() -> Result<()> {
        let guard_state = GUARD_STATE
            .get()
            .ok_or_else(|| AosError::Config("Guard system not initialized".to_string()))?;

        let mut state = guard_state
            .write()
            .map_err(|_| AosError::Config("Failed to acquire guard state lock".to_string()))?;

        state.violations.clear();
        Ok(())
    }

    /// Enable or disable stack trace collection
    pub fn set_stack_traces(enabled: bool) -> Result<()> {
        let guard_state = GUARD_STATE
            .get()
            .ok_or_else(|| AosError::Config("Guard system not initialized".to_string()))?;

        let mut state = guard_state
            .write()
            .map_err(|_| AosError::Config("Failed to acquire guard state lock".to_string()))?;

        state.stack_traces = enabled;
        Ok(())
    }

    /// Check if any violations have been recorded
    pub fn has_violations() -> bool {
        GUARD_STATE
            .get()
            .and_then(|state| state.read().ok())
            .map(|state| !state.violations.is_empty())
            .unwrap_or(false)
    }

    /// Reset guard state (intended for tests)
    #[doc(hidden)]
    pub fn reset_for_tests() {
        if let Some(lock) = GUARD_STATE.get() {
            if let Ok(mut state) = lock.write() {
                *state = GuardState::new();
            }
        } else {
            let _ = GUARD_STATE.set(RwLock::new(GuardState::new()));
        }
    }
}

/// Safe environment variable access that respects freeze
pub fn safe_env_var(key: &str) -> Result<Option<String>> {
    if ConfigGuards::is_frozen() {
        ConfigGuards::record_violation(
            "std::env::var",
            "Environment variable access prohibited after freeze",
        )?;
        return Err(AosError::Config(format!(
            "Environment variable access prohibited after freeze: {}",
            key
        )));
    }

    Ok(std::env::var(key).ok())
}

/// Safe environment variable access with default
pub fn safe_env_var_or(key: &str, default: &str) -> Result<String> {
    if ConfigGuards::is_frozen() {
        ConfigGuards::record_violation(
            "std::env::var",
            "Environment variable access prohibited after freeze",
        )?;
        return Err(AosError::Config(format!(
            "Environment variable access prohibited after freeze: {}",
            key
        )));
    }

    Ok(std::env::var(key).unwrap_or_else(|_| default.to_string()))
}

/// Safe environment variable access that panics in strict mode
pub fn strict_env_var(key: &str) -> Result<String> {
    if ConfigGuards::is_frozen() {
        let error = AosError::Config(format!(
            "Environment variable access prohibited after freeze: {}",
            key
        ));

        ConfigGuards::record_violation(
            "std::env::var",
            "Environment variable access prohibited after freeze",
        )?;

        // In strict mode, panic on freeze violations
        // For now, always return error instead of panicking
        // TODO: Add proper feature flag support

        return Err(error);
    }

    std::env::var(key)
        .map_err(|e| AosError::Config(format!("Environment variable not found: {} - {}", key, e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    #[test]
    fn test_guard_initialization() {
        let _lock = test_lock();
        ConfigGuards::initialize().unwrap();
        assert!(!ConfigGuards::is_frozen());
    }

    #[test]
    fn test_guard_freeze() {
        let _lock = test_lock();
        ConfigGuards::initialize().unwrap();
        ConfigGuards::freeze().unwrap();
        assert!(ConfigGuards::is_frozen());
    }

    #[test]
    fn test_violation_recording() {
        let _lock = test_lock();
        ConfigGuards::initialize().unwrap();
        ConfigGuards::freeze().unwrap();

        ConfigGuards::record_violation("test_operation", "test_message").unwrap();

        let violations = ConfigGuards::get_violations().unwrap();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].attempted_operation, "test_operation");
        assert_eq!(violations[0].message, "test_message");
    }

    #[test]
    fn test_safe_env_access() {
        let _lock = test_lock();
        ConfigGuards::initialize().unwrap();

        // Should work before freeze
        let result = safe_env_var("PATH");
        assert!(result.is_ok());

        // Freeze configuration
        ConfigGuards::freeze().unwrap();

        // Should fail after freeze
        let result = safe_env_var("PATH");
        assert!(result.is_err());
    }

    #[test]
    fn test_clear_violations() {
        let _lock = test_lock();
        ConfigGuards::initialize().unwrap();
        ConfigGuards::freeze().unwrap();

        ConfigGuards::record_violation("test", "message").unwrap();
        assert_eq!(ConfigGuards::get_violations().unwrap().len(), 1);

        ConfigGuards::clear_violations().unwrap();
        assert_eq!(ConfigGuards::get_violations().unwrap().len(), 0);
    }
}
