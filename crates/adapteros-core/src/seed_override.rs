//! # Deterministic Seed Override System
//!
//! This module provides mechanisms for overriding the global seed used in
//! deterministic seed derivation. It enables:
//!
//! - **Environment variable overrides**: Set `ADAPTEROS_SEED_OVERRIDE` or `AOS_SEED_OVERRIDE`
//!   to a 64-character hex string to override the global seed
//! - **Configuration file support**: Load seed overrides from config files
//! - **Thread-local propagation**: Propagate seed contexts across async boundaries
//! - **RAII guards**: Safely manage seed context lifecycles
//!
//! ## Usage
//!
//! ```ignore
//! use adapteros_core::{SeedContext, SeedContextGuard, init_global_seed_override};
//!
//! // Initialize at startup
//! init_global_seed_override(None)?;
//!
//! // Create a seed context for a request
//! let ctx = SeedContext::new(global_seed, manifest_hash, SeedMode::BestEffort, worker_id, tenant);
//! let _guard = SeedContextGuard::new(ctx);
//!
//! // All downstream code can now use derive_seed_contextual
//! let seed = derive_seed_contextual("router")?;
//! ```

use crate::hash::B3Hash;
use crate::seed::{derive_request_seed, derive_seed, derive_seed_typed, SeedLabel, SeedMode};
use crate::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::sync::OnceLock;

// =============================================================================
// Constants
// =============================================================================

/// Environment variable for global seed override.
///
/// Set this to a 64-character hex string (32 bytes) to override the global seed
/// for all derivations. This is useful for:
/// - Testing determinism across runs
/// - Reproducing specific behavior in debugging
/// - CI/CD reproducibility
///
/// Example: `ADAPTEROS_SEED_OVERRIDE=0123456789abcdef...` (64 hex chars)
pub const SEED_OVERRIDE_ENV_VAR: &str = "ADAPTEROS_SEED_OVERRIDE";

/// Alternative environment variable name (shorter)
pub const SEED_OVERRIDE_ENV_VAR_SHORT: &str = "AOS_SEED_OVERRIDE";

// =============================================================================
// Global State
// =============================================================================

/// Global seed override storage.
static GLOBAL_SEED_OVERRIDE: OnceLock<Option<B3Hash>> = OnceLock::new();

// Thread-local seed context for propagation across async boundaries.
thread_local! {
    static THREAD_SEED_CONTEXT: RefCell<Option<SeedContext>> = const { RefCell::new(None) };
}

// =============================================================================
// Types
// =============================================================================

/// Seed context for thread-local and request-scoped propagation.
///
/// This struct carries all the seed-related context needed for deterministic
/// seed derivation within a request scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedContext {
    /// The base global seed (possibly overridden)
    pub global_seed: B3Hash,
    /// Optional manifest hash for request-scoped derivation
    pub manifest_hash: Option<B3Hash>,
    /// Seed mode for this context
    pub seed_mode: SeedMode,
    /// Worker ID for isolation
    pub worker_id: u32,
    /// Tenant ID for scoping
    pub tenant_id: String,
    /// Request ID for tracing
    pub request_id: Option<String>,
    /// Nonce counter for unique derivations within this context
    nonce_counter: u64,
}

impl SeedContext {
    /// Create a new seed context with the given parameters.
    ///
    /// Uses the global seed override if set, otherwise uses the provided global seed.
    pub fn new(
        global_seed: B3Hash,
        manifest_hash: Option<B3Hash>,
        seed_mode: SeedMode,
        worker_id: u32,
        tenant_id: String,
    ) -> Self {
        let effective_global = get_global_seed_override().unwrap_or(global_seed);
        Self {
            global_seed: effective_global,
            manifest_hash,
            seed_mode,
            worker_id,
            tenant_id,
            request_id: None,
            nonce_counter: 0,
        }
    }

    /// Create a new seed context with a request ID.
    pub fn with_request_id(mut self, request_id: String) -> Self {
        self.request_id = Some(request_id);
        self
    }

    /// Get the next nonce and increment the counter.
    pub fn next_nonce(&mut self) -> u64 {
        let nonce = self.nonce_counter;
        self.nonce_counter += 1;
        nonce
    }

    /// Derive a seed for a specific label using this context.
    pub fn derive(&mut self, _label: &str) -> Result<[u8; 32]> {
        let nonce = self.next_nonce();
        derive_request_seed(
            &self.global_seed,
            self.manifest_hash.as_ref(),
            &self.tenant_id,
            self.request_id.as_deref().unwrap_or("unknown"),
            self.worker_id,
            nonce,
            self.seed_mode,
        )
    }

    /// Derive a seed using a typed label.
    pub fn derive_typed(&mut self, label: SeedLabel) -> [u8; 32] {
        let nonce = self.next_nonce();
        let manifest = self
            .manifest_hash
            .unwrap_or_else(|| B3Hash::hash(format!("no_manifest:{}", self.tenant_id).as_bytes()));
        derive_seed_typed(&self.global_seed, label, &manifest, self.worker_id, nonce)
    }
}

/// Configuration for seed overrides.
///
/// This can be loaded from a config file or set programmatically.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SeedOverrideConfig {
    /// Explicit seed override as a hex string (64 characters = 32 bytes)
    pub seed_hex: Option<String>,
    /// Whether to allow environment variable override
    #[serde(default = "default_allow_env_override")]
    pub allow_env_override: bool,
    /// Default seed mode when not specified per-request
    pub default_seed_mode: Option<SeedMode>,
}

fn default_allow_env_override() -> bool {
    true
}

impl SeedOverrideConfig {
    /// Parse the seed hex string into a B3Hash.
    pub fn parse_seed(&self) -> Result<Option<B3Hash>> {
        match &self.seed_hex {
            Some(hex) => {
                let hash = B3Hash::from_hex(hex).map_err(|e| {
                    AosError::Config(format!(
                        "Invalid seed_hex in config (expected 64 hex chars): {}",
                        e
                    ))
                })?;
                Ok(Some(hash))
            }
            None => Ok(None),
        }
    }
}

/// RAII guard for seed context.
///
/// Sets the seed context on creation and restores the previous on drop.
#[derive(Debug)]
pub struct SeedContextGuard {
    previous: Option<SeedContext>,
}

impl SeedContextGuard {
    /// Create a new guard that sets the given seed context.
    pub fn new(ctx: SeedContext) -> Self {
        let previous = THREAD_SEED_CONTEXT.with(|cell| cell.replace(Some(ctx)));
        Self { previous }
    }
}

impl Drop for SeedContextGuard {
    fn drop(&mut self) {
        THREAD_SEED_CONTEXT.with(|cell| *cell.borrow_mut() = self.previous.take());
    }
}

// =============================================================================
// Functions
// =============================================================================

/// Initialize the global seed override from environment and/or config.
///
/// This should be called once at application startup. The priority order is:
/// 1. Environment variable `ADAPTEROS_SEED_OVERRIDE` (if allow_env_override is true)
/// 2. Environment variable `AOS_SEED_OVERRIDE` (if allow_env_override is true)
/// 3. Config file seed_hex
///
/// Returns the effective seed override if one was set.
pub fn init_global_seed_override(config: Option<&SeedOverrideConfig>) -> Result<Option<B3Hash>> {
    let result = GLOBAL_SEED_OVERRIDE.get_or_init(|| {
        let allow_env = config.map(|c| c.allow_env_override).unwrap_or(true);
        if allow_env {
            if let Ok(hex) = std::env::var(SEED_OVERRIDE_ENV_VAR) {
                if let Ok(hash) = B3Hash::from_hex(&hex) {
                    tracing::info!(
                        env_var = SEED_OVERRIDE_ENV_VAR,
                        "Global seed override set from environment"
                    );
                    return Some(hash);
                }
            }
            if let Ok(hex) = std::env::var(SEED_OVERRIDE_ENV_VAR_SHORT) {
                if let Ok(hash) = B3Hash::from_hex(&hex) {
                    tracing::info!(
                        env_var = SEED_OVERRIDE_ENV_VAR_SHORT,
                        "Global seed override set from environment"
                    );
                    return Some(hash);
                }
            }
        }
        if let Some(cfg) = config {
            if let Ok(Some(hash)) = cfg.parse_seed() {
                tracing::info!(source = "config", "Global seed override set from config");
                return Some(hash);
            }
        }
        None
    });
    Ok(*result)
}

/// Get the current global seed override, if set.
pub fn get_global_seed_override() -> Option<B3Hash> {
    GLOBAL_SEED_OVERRIDE.get().cloned().flatten()
}

/// Check if a global seed override is active.
pub fn has_global_seed_override() -> bool {
    GLOBAL_SEED_OVERRIDE
        .get()
        .map(|o| o.is_some())
        .unwrap_or(false)
}

/// Set the thread-local seed context.
///
/// This should be called at the start of a request handler or task
/// to establish the seed context for all downstream operations.
pub fn set_thread_seed_context(ctx: SeedContext) {
    THREAD_SEED_CONTEXT.with(|cell| *cell.borrow_mut() = Some(ctx));
}

/// Get the current thread-local seed context.
pub fn get_thread_seed_context() -> Option<SeedContext> {
    THREAD_SEED_CONTEXT.with(|cell| cell.borrow().clone())
}

/// Clear the thread-local seed context.
///
/// This should be called at the end of a request handler or task.
pub fn clear_thread_seed_context() {
    THREAD_SEED_CONTEXT.with(|cell| *cell.borrow_mut() = None);
}

// =============================================================================
// Thread-Local State Isolation (for middleware)
// =============================================================================

/// Information about leaked thread-local state (for diagnostics).
#[derive(Debug, Clone)]
pub struct LeakedStateInfo {
    /// Tenant ID from the leaked context
    pub tenant_id: Option<String>,
    /// Request ID from the leaked context
    pub request_id: Option<String>,
    /// Nonce counter value (indicates how much state was accumulated)
    pub nonce_counter: Option<u64>,
}

/// Check if the thread-local seed context is clean (None).
///
/// Returns true if no seed context is set, false otherwise.
/// Use this to detect leaked state from previous requests.
pub fn is_thread_local_clean() -> bool {
    THREAD_SEED_CONTEXT.with(|cell| cell.borrow().is_none())
}

/// Get information about leaked thread-local state for diagnostics.
///
/// Returns Some(info) if there's a leaked context, None if clean.
pub fn get_leaked_state_info() -> Option<LeakedStateInfo> {
    THREAD_SEED_CONTEXT.with(|cell| {
        cell.borrow().as_ref().map(|ctx| LeakedStateInfo {
            tenant_id: Some(ctx.tenant_id.clone()),
            request_id: ctx.request_id.clone(),
            nonce_counter: Some(ctx.nonce_counter),
        })
    })
}

/// Assert that thread-local seed state is clean.
///
/// In debug builds, panics if state is not clean (catches determinism bugs).
/// In release builds, this is a no-op for performance.
#[inline]
pub fn assert_thread_local_clean() {
    #[cfg(debug_assertions)]
    {
        if !is_thread_local_clean() {
            if let Some(info) = get_leaked_state_info() {
                panic!(
                    "DETERMINISM BUG: Thread-local seed state leaked from previous request! \
                     tenant_id={:?}, request_id={:?}, nonce_counter={:?}",
                    info.tenant_id, info.request_id, info.nonce_counter
                );
            } else {
                panic!("DETERMINISM BUG: Thread-local seed state is not clean!");
            }
        }
    }
}

/// Reset all thread-local seed state.
///
/// This clears the thread-local seed context, ensuring a clean slate
/// for the next request. Alias for `clear_thread_seed_context()`.
#[inline]
pub fn reset_thread_local_state() {
    clear_thread_seed_context();
}

/// Execute a function with a specific seed context.
///
/// The context is set for the duration of the function and restored after.
pub fn with_seed_context<F, T>(ctx: SeedContext, f: F) -> T
where
    F: FnOnce() -> T,
{
    let previous = THREAD_SEED_CONTEXT.with(|cell| cell.replace(Some(ctx)));
    let result = f();
    THREAD_SEED_CONTEXT.with(|cell| *cell.borrow_mut() = previous);
    result
}

/// Get the effective global seed for derivation.
///
/// Returns the override if set, otherwise returns the provided default.
pub fn get_effective_global_seed(default_hash: &B3Hash) -> B3Hash {
    get_global_seed_override().unwrap_or(*default_hash)
}

/// Derive a seed using the thread-local context if available.
///
/// Falls back to direct derivation if no context is set.
pub fn derive_seed_contextual(label: &str) -> Result<[u8; 32]> {
    if let Some(mut ctx) = get_thread_seed_context() {
        let seed = ctx.derive(label)?;
        set_thread_seed_context(ctx);
        Ok(seed)
    } else {
        let fallback = B3Hash::hash(b"adapteros-fallback-no-context");
        let effective = get_effective_global_seed(&fallback);
        Ok(derive_seed(&effective, label))
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seed_override_config_parse() {
        // Valid hex string
        let config = SeedOverrideConfig {
            seed_hex: Some(
                "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
            ),
            allow_env_override: true,
            default_seed_mode: None,
        };
        let result = config.parse_seed();
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());

        // Invalid hex string
        let config_invalid = SeedOverrideConfig {
            seed_hex: Some("invalid".to_string()),
            allow_env_override: true,
            default_seed_mode: None,
        };
        let result_invalid = config_invalid.parse_seed();
        assert!(result_invalid.is_err());

        // No seed
        let config_none = SeedOverrideConfig::default();
        let result_none = config_none.parse_seed();
        assert!(result_none.is_ok());
        assert!(result_none.unwrap().is_none());
    }

    #[test]
    fn test_seed_context_creation() {
        let global = B3Hash::hash(b"test-global");
        let manifest = B3Hash::hash(b"test-manifest");

        let ctx = SeedContext::new(
            global,
            Some(manifest),
            SeedMode::BestEffort,
            1,
            "tenant-1".to_string(),
        );

        assert_eq!(ctx.seed_mode, SeedMode::BestEffort);
        assert_eq!(ctx.worker_id, 1);
        assert_eq!(ctx.tenant_id, "tenant-1");
        assert!(ctx.manifest_hash.is_some());
    }

    #[test]
    fn test_seed_context_with_request_id() {
        let global = B3Hash::hash(b"test-global");
        let ctx = SeedContext::new(global, None, SeedMode::BestEffort, 1, "tenant".to_string())
            .with_request_id("req-123".to_string());

        assert_eq!(ctx.request_id, Some("req-123".to_string()));
    }

    #[test]
    fn test_seed_context_nonce_increment() {
        let global = B3Hash::hash(b"test-global");
        let mut ctx = SeedContext::new(global, None, SeedMode::BestEffort, 1, "tenant".to_string());

        assert_eq!(ctx.next_nonce(), 0);
        assert_eq!(ctx.next_nonce(), 1);
        assert_eq!(ctx.next_nonce(), 2);
    }

    #[test]
    fn test_seed_context_derive_typed() {
        let global = B3Hash::hash(b"test-global");
        let mut ctx = SeedContext::new(global, None, SeedMode::BestEffort, 1, "tenant".to_string());

        let seed1 = ctx.derive_typed(SeedLabel::Router);
        let seed2 = ctx.derive_typed(SeedLabel::Router);

        // Different nonces should produce different seeds
        assert_ne!(seed1, seed2);
    }

    #[test]
    fn test_thread_seed_context_set_get_clear() {
        // Ensure clean state
        clear_thread_seed_context();
        assert!(get_thread_seed_context().is_none());

        let global = B3Hash::hash(b"test-global");
        let ctx = SeedContext::new(global, None, SeedMode::BestEffort, 1, "tenant".to_string());

        set_thread_seed_context(ctx.clone());
        let retrieved = get_thread_seed_context();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().tenant_id, "tenant");

        clear_thread_seed_context();
        assert!(get_thread_seed_context().is_none());
    }

    #[test]
    fn test_with_seed_context() {
        clear_thread_seed_context();

        let global = B3Hash::hash(b"test-global");
        let ctx = SeedContext::new(global, None, SeedMode::BestEffort, 1, "inner".to_string());

        let result = with_seed_context(ctx, || {
            let inner_ctx = get_thread_seed_context();
            assert!(inner_ctx.is_some());
            assert_eq!(inner_ctx.unwrap().tenant_id, "inner");
            42
        });

        assert_eq!(result, 42);
        assert!(get_thread_seed_context().is_none());
    }

    #[test]
    fn test_seed_context_guard() {
        clear_thread_seed_context();

        let global = B3Hash::hash(b"test-global");
        let ctx = SeedContext::new(global, None, SeedMode::BestEffort, 1, "guarded".to_string());

        {
            let _guard = SeedContextGuard::new(ctx);
            let inner_ctx = get_thread_seed_context();
            assert!(inner_ctx.is_some());
            assert_eq!(inner_ctx.unwrap().tenant_id, "guarded");
        }

        assert!(get_thread_seed_context().is_none());
    }

    #[test]
    fn test_seed_context_guard_restores_previous() {
        clear_thread_seed_context();

        let global = B3Hash::hash(b"test-global");
        let ctx1 = SeedContext::new(global, None, SeedMode::BestEffort, 1, "outer".to_string());
        let ctx2 = SeedContext::new(global, None, SeedMode::BestEffort, 2, "inner".to_string());

        set_thread_seed_context(ctx1);

        {
            let _guard = SeedContextGuard::new(ctx2);
            let inner_ctx = get_thread_seed_context();
            assert_eq!(inner_ctx.unwrap().tenant_id, "inner");
        }

        let restored = get_thread_seed_context();
        assert!(restored.is_some());
        assert_eq!(restored.unwrap().tenant_id, "outer");

        clear_thread_seed_context();
    }

    #[test]
    fn test_get_effective_global_seed_without_override() {
        let default_hash = B3Hash::hash(b"default");
        let effective = get_effective_global_seed(&default_hash);
        assert_eq!(effective.as_bytes().len(), 32);
    }

    #[test]
    fn test_derive_seed_contextual_without_context() {
        clear_thread_seed_context();

        let result = derive_seed_contextual("test_label");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 32);
    }

    #[test]
    fn test_derive_seed_contextual_with_context() {
        clear_thread_seed_context();

        let global = B3Hash::hash(b"test-global");
        let ctx = SeedContext::new(
            global,
            None,
            SeedMode::BestEffort,
            1,
            "contextual".to_string(),
        );
        set_thread_seed_context(ctx);

        let result = derive_seed_contextual("test_label");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 32);

        clear_thread_seed_context();
    }
}
