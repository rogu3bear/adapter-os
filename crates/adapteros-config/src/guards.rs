//! Configuration guards to prevent environment variable access after freeze

use crate::types::*;
use adapteros_core::{AosError, Result};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{OnceLock, RwLock};

/// Global guard state
static GUARD_STATE: OnceLock<RwLock<GuardState>> = OnceLock::new();

/// Global feature flag registry
static FEATURE_FLAGS: OnceLock<RwLock<FeatureFlagRegistry>> = OnceLock::new();

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

/// Feature flag registry for runtime feature detection
#[derive(Debug, Clone)]
struct FeatureFlagRegistry {
    flags: HashMap<String, FeatureFlag>,
    environment: String,
    tenant_id: Option<String>,
}

impl FeatureFlagRegistry {
    fn new() -> Self {
        Self {
            flags: HashMap::new(),
            environment: std::env::var("ADAPTEROS_ENV")
                .unwrap_or_else(|_| "development".to_string()),
            tenant_id: None,
        }
    }

    fn register(&mut self, flag: FeatureFlag) {
        self.flags.insert(flag.name.clone(), flag);
    }

    fn is_enabled(&self, name: &str) -> bool {
        self.flags
            .get(name)
            .map(|flag| self.evaluate_flag(flag))
            .unwrap_or(false)
    }

    fn evaluate_flag(&self, flag: &FeatureFlag) -> bool {
        if !flag.enabled {
            return false;
        }

        // Check conditions if present
        if let Some(ref conditions) = flag.conditions {
            // Check environment condition
            if let Some(ref envs) = conditions.environments {
                if !envs.contains(&self.environment) {
                    return false;
                }
            }

            // Check tenant condition
            if let Some(ref tenant_ids) = conditions.tenant_ids {
                if let Some(ref current_tenant) = self.tenant_id {
                    if !tenant_ids.contains(current_tenant) {
                        return false;
                    }
                } else {
                    return false;
                }
            }

            // Check date conditions
            let now = chrono::Utc::now();
            if let Some(ref after) = conditions.enabled_after {
                if let Ok(date) = chrono::DateTime::parse_from_rfc3339(after) {
                    if now < date {
                        return false;
                    }
                }
            }
            if let Some(ref before) = conditions.enabled_before {
                if let Ok(date) = chrono::DateTime::parse_from_rfc3339(before) {
                    if now > date {
                        return false;
                    }
                }
            }

            // Check rollout percentage (deterministic based on flag name hash)
            if let Some(percentage) = conditions.rollout_percentage {
                let hash = {
                    let mut hasher = DefaultHasher::new();
                    flag.name.hash(&mut hasher);
                    hasher.finish()
                };
                let bucket = (hash % 100) as u8;
                if bucket >= percentage {
                    return false;
                }
            }
        }

        true
    }

    fn set_environment(&mut self, env: String) {
        self.environment = env;
    }

    fn set_tenant(&mut self, tenant_id: Option<String>) {
        self.tenant_id = tenant_id;
    }

    fn get_all_flags(&self) -> Vec<FeatureFlag> {
        self.flags.values().cloned().collect()
    }

    fn remove(&mut self, name: &str) -> Option<FeatureFlag> {
        self.flags.remove(name)
    }
}

/// Configuration guards for enforcing freeze behavior
pub struct ConfigGuards;

impl ConfigGuards {
    /// Initialize the guard system
    pub fn initialize() -> Result<()> {
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

    /// Safe environment variable access that respects freeze state
    ///
    /// This function should be used instead of `std::env::var` throughout the codebase.
    /// After configuration is frozen, this will:
    /// - In permissive mode: log a warning and return the value
    /// - In strict mode: return an error
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use adapteros_config::ConfigGuards;
    ///
    /// // Before freeze - works normally
    /// let value = ConfigGuards::safe_env_var("AOS_MODEL_PATH")?;
    ///
    /// // After freeze - logs warning or errors depending on mode
    /// ConfigGuards::freeze()?;
    /// let value = ConfigGuards::safe_env_var("AOS_MODEL_PATH")?; // Warning logged
    /// ```
    pub fn safe_env_var(key: &str) -> Result<String> {
        if Self::is_frozen() {
            let _ = Self::record_violation(
                "env_var_access",
                &format!("Attempted to read {} after freeze", key),
            );

            // In permissive mode, still return the value but log warning
            tracing::warn!(
                key = %key,
                "Environment variable accessed after configuration freeze. \
                 This should be read during initialization."
            );
        }

        std::env::var(key).map_err(|_| {
            AosError::Config(format!("Environment variable {} not set", key))
        })
    }

    /// Safe environment variable access with default fallback
    ///
    /// Like `safe_env_var` but returns a default value if the variable is not set.
    pub fn safe_env_var_or(key: &str, default: &str) -> String {
        Self::safe_env_var(key).unwrap_or_else(|_| default.to_string())
    }

    /// Check if an environment variable is set (freeze-aware)
    pub fn env_var_exists(key: &str) -> bool {
        if Self::is_frozen() {
            let _ = Self::record_violation(
                "env_var_check",
                &format!("Checked existence of {} after freeze", key),
            );
            tracing::warn!(key = %key, "Environment variable check after freeze");
        }
        std::env::var(key).is_ok()
    }

    /// Reset guards for testing (clears frozen state and violations)
    #[cfg(test)]
    pub fn reset_for_testing() -> Result<()> {
        if let Some(guard_state) = GUARD_STATE.get() {
            let mut state = guard_state
                .write()
                .map_err(|_| AosError::Config("Failed to acquire guard state lock".to_string()))?;
            state.frozen = false;
            state.violations.clear();
        }
        Ok(())
    }
}

/// Feature flag management for runtime feature detection
pub struct FeatureFlags;

impl FeatureFlags {
    /// Initialize the feature flag system
    pub fn initialize() -> Result<()> {
        FEATURE_FLAGS
            .set(RwLock::new(FeatureFlagRegistry::new()))
            .map_err(|_| AosError::Config("Feature flag system already initialized".to_string()))?;

        tracing::info!("Feature flag system initialized");
        Ok(())
    }

    /// Register a new feature flag
    pub fn register(flag: FeatureFlag) -> Result<()> {
        let registry = FEATURE_FLAGS
            .get()
            .ok_or_else(|| AosError::Config("Feature flag system not initialized".to_string()))?;

        let mut flags = registry
            .write()
            .map_err(|_| AosError::Config("Failed to acquire feature flag lock".to_string()))?;

        tracing::debug!(flag_name = %flag.name, enabled = flag.enabled, "Registering feature flag");
        flags.register(flag);
        Ok(())
    }

    /// Check if a feature flag is enabled
    pub fn is_enabled(name: &str) -> bool {
        FEATURE_FLAGS
            .get()
            .and_then(|registry| registry.read().ok())
            .map(|flags| flags.is_enabled(name))
            .unwrap_or(false)
    }

    /// Set the current environment for feature flag evaluation
    pub fn set_environment(env: &str) -> Result<()> {
        let registry = FEATURE_FLAGS
            .get()
            .ok_or_else(|| AosError::Config("Feature flag system not initialized".to_string()))?;

        let mut flags = registry
            .write()
            .map_err(|_| AosError::Config("Failed to acquire feature flag lock".to_string()))?;

        flags.set_environment(env.to_string());
        tracing::debug!(environment = env, "Feature flag environment updated");
        Ok(())
    }

    /// Set the current tenant for feature flag evaluation
    pub fn set_tenant(tenant_id: Option<&str>) -> Result<()> {
        let registry = FEATURE_FLAGS
            .get()
            .ok_or_else(|| AosError::Config("Feature flag system not initialized".to_string()))?;

        let mut flags = registry
            .write()
            .map_err(|_| AosError::Config("Failed to acquire feature flag lock".to_string()))?;

        flags.set_tenant(tenant_id.map(|s| s.to_string()));
        tracing::debug!(tenant_id = ?tenant_id, "Feature flag tenant updated");
        Ok(())
    }

    /// Get all registered feature flags
    pub fn get_all() -> Result<Vec<FeatureFlag>> {
        let registry = FEATURE_FLAGS
            .get()
            .ok_or_else(|| AosError::Config("Feature flag system not initialized".to_string()))?;

        let flags = registry
            .read()
            .map_err(|_| AosError::Config("Failed to acquire feature flag lock".to_string()))?;

        Ok(flags.get_all_flags())
    }

    /// Remove a feature flag
    pub fn remove(name: &str) -> Result<Option<FeatureFlag>> {
        let registry = FEATURE_FLAGS
            .get()
            .ok_or_else(|| AosError::Config("Feature flag system not initialized".to_string()))?;

        let mut flags = registry
            .write()
            .map_err(|_| AosError::Config("Failed to acquire feature flag lock".to_string()))?;

        tracing::debug!(flag_name = name, "Removing feature flag");
        Ok(flags.remove(name))
    }

    /// Register multiple feature flags from configuration
    pub fn register_from_config(flags: Vec<FeatureFlag>) -> Result<()> {
        for flag in flags {
            Self::register(flag)?;
        }
        Ok(())
    }

    /// Check if the feature flag system is initialized
    pub fn is_initialized() -> bool {
        FEATURE_FLAGS.get().is_some()
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

        // Check if strict panic mode is enabled via feature flag
        if FeatureFlags::is_enabled("strict_panic_on_freeze") {
            panic!(
                "STRICT MODE: Environment variable access prohibited after freeze: {}",
                key
            );
        }

        return Err(error);
    }

    std::env::var(key)
        .map_err(|e| AosError::Config(format!("Environment variable not found: {} - {}", key, e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guard_initialization() {
        ConfigGuards::initialize().unwrap();
        assert!(!ConfigGuards::is_frozen());
    }

    #[test]
    fn test_guard_freeze() {
        ConfigGuards::initialize().unwrap();
        ConfigGuards::freeze().unwrap();
        assert!(ConfigGuards::is_frozen());
    }

    #[test]
    fn test_violation_recording() {
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
        ConfigGuards::initialize().unwrap();
        ConfigGuards::freeze().unwrap();

        ConfigGuards::record_violation("test", "message").unwrap();
        assert_eq!(ConfigGuards::get_violations().unwrap().len(), 1);

        ConfigGuards::clear_violations().unwrap();
        assert_eq!(ConfigGuards::get_violations().unwrap().len(), 0);
    }

    #[test]
    fn test_feature_flag_initialization() {
        FeatureFlags::initialize().unwrap();
        assert!(FeatureFlags::is_initialized());
    }

    #[test]
    fn test_feature_flag_registration() {
        FeatureFlags::initialize().unwrap();

        let flag = FeatureFlag {
            name: "test_feature".to_string(),
            enabled: true,
            description: Some("Test feature".to_string()),
            conditions: None,
        };

        FeatureFlags::register(flag).unwrap();
        assert!(FeatureFlags::is_enabled("test_feature"));
        assert!(!FeatureFlags::is_enabled("nonexistent_feature"));
    }

    #[test]
    fn test_feature_flag_disabled() {
        FeatureFlags::initialize().unwrap();

        let flag = FeatureFlag {
            name: "disabled_feature".to_string(),
            enabled: false,
            description: None,
            conditions: None,
        };

        FeatureFlags::register(flag).unwrap();
        assert!(!FeatureFlags::is_enabled("disabled_feature"));
    }

    #[test]
    fn test_feature_flag_environment_condition() {
        FeatureFlags::initialize().unwrap();

        let flag = FeatureFlag {
            name: "prod_only_feature".to_string(),
            enabled: true,
            description: None,
            conditions: Some(FeatureFlagConditions {
                environments: Some(vec!["production".to_string()]),
                tenant_ids: None,
                enabled_after: None,
                enabled_before: None,
                rollout_percentage: None,
            }),
        };

        FeatureFlags::register(flag).unwrap();

        // Should be disabled in development (default)
        FeatureFlags::set_environment("development").unwrap();
        assert!(!FeatureFlags::is_enabled("prod_only_feature"));

        // Should be enabled in production
        FeatureFlags::set_environment("production").unwrap();
        assert!(FeatureFlags::is_enabled("prod_only_feature"));
    }

    #[test]
    fn test_feature_flag_tenant_condition() {
        FeatureFlags::initialize().unwrap();

        let flag = FeatureFlag {
            name: "tenant_specific_feature".to_string(),
            enabled: true,
            description: None,
            conditions: Some(FeatureFlagConditions {
                environments: None,
                tenant_ids: Some(vec!["tenant-a".to_string(), "tenant-b".to_string()]),
                enabled_after: None,
                enabled_before: None,
                rollout_percentage: None,
            }),
        };

        FeatureFlags::register(flag).unwrap();

        // Should be disabled without tenant
        assert!(!FeatureFlags::is_enabled("tenant_specific_feature"));

        // Should be disabled for wrong tenant
        FeatureFlags::set_tenant(Some("tenant-c")).unwrap();
        assert!(!FeatureFlags::is_enabled("tenant_specific_feature"));

        // Should be enabled for correct tenant
        FeatureFlags::set_tenant(Some("tenant-a")).unwrap();
        assert!(FeatureFlags::is_enabled("tenant_specific_feature"));
    }

    #[test]
    fn test_feature_flag_get_all() {
        FeatureFlags::initialize().unwrap();

        let flag1 = FeatureFlag {
            name: "feature_1".to_string(),
            enabled: true,
            description: None,
            conditions: None,
        };

        let flag2 = FeatureFlag {
            name: "feature_2".to_string(),
            enabled: false,
            description: None,
            conditions: None,
        };

        FeatureFlags::register(flag1).unwrap();
        FeatureFlags::register(flag2).unwrap();

        let all_flags = FeatureFlags::get_all().unwrap();
        assert_eq!(all_flags.len(), 2);
    }

    #[test]
    fn test_feature_flag_remove() {
        FeatureFlags::initialize().unwrap();

        let flag = FeatureFlag {
            name: "removable_feature".to_string(),
            enabled: true,
            description: None,
            conditions: None,
        };

        FeatureFlags::register(flag).unwrap();
        assert!(FeatureFlags::is_enabled("removable_feature"));

        let removed = FeatureFlags::remove("removable_feature").unwrap();
        assert!(removed.is_some());
        assert!(!FeatureFlags::is_enabled("removable_feature"));
    }
}
