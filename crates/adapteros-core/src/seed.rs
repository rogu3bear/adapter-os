//! # Deterministic Seed Derivation System
//!
//! This module provides cryptographically secure, deterministic seed derivation for all
//! random number generation in AdapterOS. It ensures **replay reproducibility**: given the
//! same inputs (manifest hash, request parameters), the system produces identical outputs.
//!
//! ## Architecture Overview
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │                        Global Seed (B3Hash)                         │
//! │              BLAKE3 hash of manifest/request inputs                 │
//! └─────────────────────────────────┬───────────────────────────────────┘
//!                                   │
//!                            HKDF-SHA256
//!                                   │
//!         ┌─────────────────────────┼─────────────────────────┐
//!         ▼                         ▼                         ▼
//!   ┌───────────┐            ┌───────────┐            ┌───────────┐
//!   │  Router   │            │  Dropout  │            │ Sampling  │
//!   │   Seed    │            │   Seed    │            │   Seed    │
//!   └───────────┘            └───────────┘            └───────────┘
//!   Tie-breaking             Training dropout         Token sampling
//!   in K-sparse              mask generation          temperature/top-p
//! ```
//!
//! ## Seed Modes
//!
//! | Mode              | Behavior                                           | Use Case              |
//! |-------------------|----------------------------------------------------|-----------------------|
//! | `Strict`          | Requires manifest hash; fails if missing           | Production inference  |
//! | `BestEffort`      | Uses manifest hash when present; fallback hash     | Dev/testing           |
//! | `NonDeterministic`| Random seed (non-replayable)                       | Benchmarking only     |
//!
//! ## Key Functions
//!
//! - [`derive_seed`]: Core HKDF derivation from global seed + label
//! - [`derive_seed_typed`]: Type-safe derivation using [`SeedLabel`] enum
//! - [`ExecutionProfile`]: Request-scoped seed mode + backend configuration
//! - [`DeterminismConfig`]: Global determinism knobs for testing and replay
//!
//! ## Determinism Controls
//!
//! The [`DeterminismConfig`] struct provides global controls for determinism:
//!
//! ```ignore
//! use adapteros_core::seed::{DeterminismConfig, set_determinism_config, get_deterministic_timestamp};
//!
//! // Set fixed seed and timestamp for replay
//! let config = DeterminismConfig::builder()
//!     .fixed_seed(12345)
//!     .fixed_timestamp(DateTime::parse_from_rfc3339("2025-01-01T00:00:00Z").unwrap().into())
//!     .stable_ordering(true)
//!     .build();
//!
//! set_determinism_config(config);
//!
//! // Now all deterministic helpers use the fixed values
//! let ts = get_deterministic_timestamp(); // Returns fixed timestamp
//! let rng = get_deterministic_rng();      // Returns seeded RNG
//! ```
//!
//! ## Critical Invariants
//!
//! 1. **Same inputs → Same seed**: `derive_seed(hash_A, "router")` always returns identical bytes
//! 2. **Label uniqueness**: Different labels produce cryptographically distinct seeds
//! 3. **No seed reuse**: Registry tracks (label, request_id) to detect accidental reuse
//! 4. **HKDF-SHA256 only**: Do not use other KDFs; breaks replay compatibility
//!
//! ## Example
//!
//! ```ignore
//! let global = B3Hash::hash(manifest_bytes);
//! let router_seed = derive_seed(&global, "router");
//! let mut rng = ChaCha20Rng::from_seed(router_seed);
//! // All RNG operations now deterministic
//! ```

use crate::backend::BackendKind;
use crate::defaults::DEFAULT_SEED_MODE;
use crate::hash::B3Hash;
use crate::{AosError, Result};
use chrono::{DateTime, Utc};
use hkdf::Hkdf;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use std::sync::{Mutex, OnceLock};

lazy_static::lazy_static! {
    /// Seed registry to prevent reuse
    static ref SEED_REGISTRY: Mutex<HashMap<(String, u64), bool>> = Mutex::new(HashMap::new());
}

// =============================================================================
// DeterminismConfig - Global determinism controls
// =============================================================================

/// Global determinism configuration for testing and replay scenarios.
///
/// This configuration controls how the system handles randomness, timestamps,
/// and ordering to enable deterministic replay of operations.
///
/// # Examples
///
/// ```ignore
/// use adapteros_core::seed::{DeterminismConfig, set_determinism_config};
/// use chrono::Utc;
///
/// // Enable full determinism for testing
/// let config = DeterminismConfig::builder()
///     .fixed_seed(42)
///     .fixed_timestamp(Utc::now())
///     .stable_ordering(true)
///     .build();
///
/// set_determinism_config(config);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DeterminismConfig {
    /// Fixed seed for RNG. When set, all RNG operations use this seed.
    /// When None, uses the standard seed derivation system.
    pub fixed_seed: Option<u64>,

    /// Fixed timestamp for all time operations. When set, `get_deterministic_timestamp()`
    /// returns this value instead of the current time.
    pub fixed_timestamp: Option<DateTime<Utc>>,

    /// Force stable ordering everywhere. When true, operations that normally
    /// might produce non-deterministic ordering (e.g., HashMap iteration)
    /// should use sorted/stable alternatives.
    pub stable_ordering: bool,

    /// Disable all sources of non-determinism. This is a meta-flag that
    /// enables strict validation of determinism invariants.
    pub strict_mode: bool,

    /// Trace seed derivation for debugging. When true, logs detailed
    /// information about seed derivation operations.
    pub trace_seeds: bool,
}

impl DeterminismConfig {
    /// Create a new determinism config with default settings (non-deterministic).
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a builder for constructing a DeterminismConfig.
    pub fn builder() -> DeterminismConfigBuilder {
        DeterminismConfigBuilder::new()
    }

    /// Create a fully deterministic config for testing.
    ///
    /// Uses a fixed seed of 0, the Unix epoch as timestamp, and enables
    /// stable ordering.
    pub fn fully_deterministic() -> Self {
        Self {
            fixed_seed: Some(0),
            fixed_timestamp: Some(DateTime::UNIX_EPOCH),
            stable_ordering: true,
            strict_mode: true,
            trace_seeds: false,
        }
    }

    /// Create a config for replay with specific seed and timestamp.
    pub fn for_replay(seed: u64, timestamp: DateTime<Utc>) -> Self {
        Self {
            fixed_seed: Some(seed),
            fixed_timestamp: Some(timestamp),
            stable_ordering: true,
            strict_mode: true,
            trace_seeds: false,
        }
    }

    /// Check if this config enforces determinism.
    pub fn is_deterministic(&self) -> bool {
        self.fixed_seed.is_some() || self.fixed_timestamp.is_some() || self.stable_ordering
    }

    /// Check if strict mode is enabled.
    pub fn is_strict(&self) -> bool {
        self.strict_mode
    }
}

/// Builder for DeterminismConfig.
#[derive(Debug, Default)]
pub struct DeterminismConfigBuilder {
    fixed_seed: Option<u64>,
    fixed_timestamp: Option<DateTime<Utc>>,
    stable_ordering: bool,
    strict_mode: bool,
    trace_seeds: bool,
}

impl DeterminismConfigBuilder {
    /// Create a new builder with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a fixed seed for RNG operations.
    pub fn fixed_seed(mut self, seed: u64) -> Self {
        self.fixed_seed = Some(seed);
        self
    }

    /// Set an optional fixed seed for RNG operations.
    pub fn fixed_seed_opt(mut self, seed: Option<u64>) -> Self {
        self.fixed_seed = seed;
        self
    }

    /// Set a fixed timestamp for time operations.
    pub fn fixed_timestamp(mut self, timestamp: DateTime<Utc>) -> Self {
        self.fixed_timestamp = Some(timestamp);
        self
    }

    /// Set an optional fixed timestamp for time operations.
    pub fn fixed_timestamp_opt(mut self, timestamp: Option<DateTime<Utc>>) -> Self {
        self.fixed_timestamp = timestamp;
        self
    }

    /// Enable or disable stable ordering.
    pub fn stable_ordering(mut self, enabled: bool) -> Self {
        self.stable_ordering = enabled;
        self
    }

    /// Enable or disable strict mode.
    pub fn strict_mode(mut self, enabled: bool) -> Self {
        self.strict_mode = enabled;
        self
    }

    /// Enable or disable seed tracing.
    pub fn trace_seeds(mut self, enabled: bool) -> Self {
        self.trace_seeds = enabled;
        self
    }

    /// Build the DeterminismConfig.
    pub fn build(self) -> DeterminismConfig {
        DeterminismConfig {
            fixed_seed: self.fixed_seed,
            fixed_timestamp: self.fixed_timestamp,
            stable_ordering: self.stable_ordering,
            strict_mode: self.strict_mode,
            trace_seeds: self.trace_seeds,
        }
    }
}

// =============================================================================
// Global and Thread-Local Config Storage
// =============================================================================

/// Global determinism configuration.
static GLOBAL_DETERMINISM_CONFIG: OnceLock<RwLock<DeterminismConfig>> = OnceLock::new();

fn global_config() -> &'static RwLock<DeterminismConfig> {
    GLOBAL_DETERMINISM_CONFIG.get_or_init(|| RwLock::new(DeterminismConfig::default()))
}

// Thread-local override for determinism configuration.
// Takes precedence over the global config when set.
thread_local! {
    static THREAD_LOCAL_CONFIG: RefCell<Option<DeterminismConfig>> = const { RefCell::new(None) };
}

/// Set the global determinism configuration.
///
/// This affects all threads that don't have a thread-local override.
pub fn set_determinism_config(config: DeterminismConfig) {
    *global_config().write() = config;
}

/// Get the current determinism configuration.
///
/// Returns the thread-local config if set, otherwise the global config.
pub fn get_determinism_config() -> DeterminismConfig {
    THREAD_LOCAL_CONFIG.with(|local| {
        if let Some(config) = local.borrow().as_ref() {
            return config.clone();
        }
        global_config().read().clone()
    })
}

/// Set a thread-local determinism configuration override.
///
/// This takes precedence over the global configuration for this thread only.
pub fn set_thread_local_determinism_config(config: DeterminismConfig) {
    THREAD_LOCAL_CONFIG.with(|local| {
        *local.borrow_mut() = Some(config);
    });
}

/// Clear the thread-local determinism configuration override.
///
/// After calling this, the thread will use the global configuration.
pub fn clear_thread_local_determinism_config() {
    THREAD_LOCAL_CONFIG.with(|local| {
        *local.borrow_mut() = None;
    });
}

/// Reset the global determinism configuration to defaults.
///
/// This is primarily useful for testing.
pub fn reset_determinism_config() {
    *global_config().write() = DeterminismConfig::default();
    clear_thread_local_determinism_config();
}

/// RAII guard for temporarily setting thread-local determinism config.
///
/// Restores the previous config when dropped.
pub struct DeterminismConfigGuard {
    previous: Option<DeterminismConfig>,
}

impl DeterminismConfigGuard {
    /// Create a new guard that sets the thread-local config.
    pub fn new(config: DeterminismConfig) -> Self {
        let previous = THREAD_LOCAL_CONFIG.with(|local| local.borrow().clone());
        set_thread_local_determinism_config(config);
        Self { previous }
    }
}

impl Drop for DeterminismConfigGuard {
    fn drop(&mut self) {
        THREAD_LOCAL_CONFIG.with(|local| {
            *local.borrow_mut() = self.previous.take();
        });
    }
}

/// Execute a closure with a specific determinism configuration.
///
/// The configuration is scoped to this call and automatically restored afterward.
pub fn with_determinism_config<T, F>(config: DeterminismConfig, f: F) -> T
where
    F: FnOnce() -> T,
{
    let _guard = DeterminismConfigGuard::new(config);
    f()
}

// =============================================================================
// Deterministic Helper Functions
// =============================================================================

/// Get a deterministic timestamp.
///
/// If a fixed timestamp is configured, returns that. Otherwise returns the
/// current UTC time.
pub fn get_deterministic_timestamp() -> DateTime<Utc> {
    let config = get_determinism_config();
    config.fixed_timestamp.unwrap_or_else(Utc::now)
}

/// Get a deterministic Unix timestamp in seconds.
///
/// If a fixed timestamp is configured, returns that. Otherwise returns the
/// current Unix timestamp.
pub fn get_deterministic_unix_timestamp() -> i64 {
    get_deterministic_timestamp().timestamp()
}

/// Get a deterministic Unix timestamp in milliseconds.
pub fn get_deterministic_unix_timestamp_millis() -> i64 {
    get_deterministic_timestamp().timestamp_millis()
}

/// Get a deterministic RNG based on the current configuration.
///
/// If a fixed seed is configured, returns a seeded RNG. Otherwise returns
/// a randomly-seeded RNG.
pub fn get_deterministic_rng() -> fastrand::Rng {
    let config = get_determinism_config();
    match config.fixed_seed {
        Some(seed) => fastrand::Rng::with_seed(seed),
        None => fastrand::Rng::new(),
    }
}

/// Get a deterministic 32-byte seed for use with other RNG libraries.
///
/// If a fixed seed is configured, derives a 32-byte seed from it using HKDF.
/// Otherwise returns random bytes.
pub fn get_deterministic_seed_bytes() -> [u8; 32] {
    let config = get_determinism_config();
    match config.fixed_seed {
        Some(seed) => {
            // Expand the u64 seed into 32 bytes using HKDF
            let mut seed_bytes = [0u8; 8];
            seed_bytes.copy_from_slice(&seed.to_le_bytes());
            let hk = Hkdf::<Sha256>::new(None, &seed_bytes);
            let mut okm = [0u8; 32];
            hk.expand(b"determinism-config-seed", &mut okm)
                .expect("HKDF expand failed");
            okm
        }
        None => {
            let mut bytes = [0u8; 32];
            fastrand::Rng::new().fill(&mut bytes);
            bytes
        }
    }
}

/// Check if stable ordering should be used.
///
/// Returns true if the current configuration requires stable ordering.
pub fn should_use_stable_ordering() -> bool {
    get_determinism_config().stable_ordering
}

/// Check if strict determinism mode is enabled.
pub fn is_strict_determinism_mode() -> bool {
    get_determinism_config().strict_mode
}

/// Check if seed tracing is enabled.
pub fn is_seed_tracing_enabled() -> bool {
    get_determinism_config().trace_seeds
}

/// Sort a vector if stable ordering is required.
///
/// This is a convenience helper that sorts the vector in place only if
/// stable ordering is enabled in the current configuration.
pub fn maybe_stable_sort<T: Ord>(items: &mut [T]) {
    if should_use_stable_ordering() {
        items.sort();
    }
}

/// Sort a vector by key if stable ordering is required.
pub fn maybe_stable_sort_by_key<T, K: Ord, F: FnMut(&T) -> K>(items: &mut [T], f: F) {
    if should_use_stable_ordering() {
        items.sort_by_key(f);
    }
}

// =============================================================================
// Original seed.rs content continues below
// =============================================================================

fn determinism_debug_enabled() -> bool {
    static FLAG: OnceLock<bool> = OnceLock::new();
    *FLAG.get_or_init(|| match std::env::var("AOS_DEBUG_DETERMINISM") {
        Ok(val) => {
            let normalized = val.to_ascii_lowercase();
            normalized == "1" || normalized == "true" || normalized == "yes"
        }
        Err(_) => false,
    })
}

/// Seed label enum for type-safe seed derivation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SeedLabel {
    Router,
    Dropout,
    Sampling,
    Adapter(usize),
}

/// Execution seed strategy for per-request derivation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum SeedMode {
    /// Requires manifest hash; fails if missing
    Strict,
    /// Uses manifest hash when present; otherwise uses a scoped fallback hash
    BestEffort,
    /// Dev-only random seed (non-replayable)
    NonDeterministic,
}

impl SeedMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            SeedMode::Strict => "strict",
            SeedMode::BestEffort => "best_effort",
            SeedMode::NonDeterministic => "non_deterministic",
        }
    }
}

impl Default for SeedMode {
    fn default() -> Self {
        DEFAULT_SEED_MODE
    }
}

impl fmt::Display for SeedMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SeedMode {
    type Err = AosError;

    fn from_str(s: &str) -> Result<Self> {
        let normalized = s.to_ascii_lowercase().replace(['-', '_'], "");
        match normalized.as_str() {
            "strict" => Ok(SeedMode::Strict),
            "besteffort" => Ok(SeedMode::BestEffort),
            "nondeterministic" | "nondet" => Ok(SeedMode::NonDeterministic),
            _ => Err(AosError::Config(format!(
                "Invalid seed mode: {} (expected strict, best_effort, non_deterministic)",
                s
            ))),
        }
    }
}

/// Shared execution profile for request-scoped execution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct ExecutionProfile {
    pub seed_mode: SeedMode,
    /// Backend preference for this request. Defaults to the CoreML-first
    /// inference priority (`BackendKind::inference_priority()`), falling back
    /// through MLX → Metal → CPU as capabilities allow.
    pub backend_profile: BackendKind,
}

impl Default for ExecutionProfile {
    fn default() -> Self {
        Self {
            seed_mode: DEFAULT_SEED_MODE,
            backend_profile: BackendKind::default_inference_backend(),
        }
    }
}

impl SeedLabel {
    pub fn as_str(&self) -> String {
        match self {
            SeedLabel::Router => "router".to_string(),
            SeedLabel::Dropout => "dropout".to_string(),
            SeedLabel::Sampling => "sampling".to_string(),
            SeedLabel::Adapter(id) => format!("adapter_{}", id),
        }
    }
}

/// Derive a deterministic seed from a global seed and label
///
/// Uses HKDF-SHA256 for key derivation. All RNG in the system
/// must derive from these seeds to ensure determinism.
pub fn derive_seed(global: &B3Hash, label: &str) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::from_prk(global.as_bytes()).expect("valid PRK");
    let mut okm = [0u8; 32];
    hk.expand(label.as_bytes(), &mut okm)
        .expect("32 bytes is valid length");

    // Validate HKDF output is exactly 32 bytes
    assert_eq!(okm.len(), 32, "HKDF output must be exactly 32 bytes");

    // Compute checksum for audit
    let checksum = B3Hash::hash(&okm);
    if determinism_debug_enabled() {
        let global_hex = global.to_hex();
        let checksum_hex = checksum.to_hex();
        tracing::info!(
            target: "determinism",
            label = label,
            global_prefix = %global_hex.get(..16).unwrap_or(&global_hex),
            checksum_prefix = %checksum_hex.get(..16).unwrap_or(&checksum_hex),
            "Derived seed with validation (AOS_DEBUG_DETERMINISM=1)"
        );
    } else {
        let checksum_hex = checksum.to_hex();
        tracing::debug!(
            label = label,
            checksum = %checksum_hex.get(..16).unwrap_or(&checksum_hex),
            "Derived seed with validation"
        );
    }

    okm
}

/// Derive seed with typed label
///
/// Seeds are scoped by `manifest_hash`, `adapter_dir_hash`, `worker_id`,
/// label, and nonce so that two requests with the same inputs get the
/// same 32-byte seed, while any differing input forces a new seed.
pub fn derive_seed_typed(
    global: &B3Hash,
    label: SeedLabel,
    manifest_hash: &B3Hash,
    worker_id: u32,
    nonce: u64,
) -> [u8; 32] {
    if determinism_debug_enabled() {
        let manifest_hex = manifest_hash.to_hex();
        tracing::info!(
            target: "determinism",
            label = %label.as_str(),
            manifest_prefix = %manifest_hex.get(..16).unwrap_or(&manifest_hex),
            worker_id,
            nonce,
            "Deriving typed seed (AOS_DEBUG_DETERMINISM=1)"
        );
    }

    let composite_label = format!(
        "{}:{}:{}:{}",
        label.as_str(),
        &manifest_hash.to_hex()[..16],
        worker_id,
        nonce
    );
    derive_seed(global, &composite_label)
}

/// Derive a deterministic seed with an index for array-like derivations
///
/// Allows deriving multiple seeds for the same component by index
pub fn derive_seed_indexed(global: &B3Hash, label: &str, index: usize) -> [u8; 32] {
    let indexed_label = format!("{}:{}", label, index);
    derive_seed(global, &indexed_label)
}

/// Derive a request-scoped seed with configurable determinism mode.
///
/// Seed label includes:
/// - manifest hash (or tenant-scoped fallback when absent)
/// - tenant id
/// - request id
/// - worker id
/// - nonce
///
/// This makes it explicit that re-running the same request on a different
/// worker yields a different seed, while the exact same tuple reproduces.
pub fn derive_request_seed(
    global: &B3Hash,
    manifest: Option<&B3Hash>,
    tenant_id: &str,
    request_id: &str,
    worker_id: u32,
    nonce: u64,
    mode: SeedMode,
) -> Result<[u8; 32]> {
    match mode {
        SeedMode::Strict => {
            let manifest_hash = manifest.ok_or_else(|| {
                AosError::DeterminismViolation(
                    "Strict seed_mode requires manifest hash".to_string(),
                )
            })?;
            if determinism_debug_enabled() {
                let manifest_hex = manifest_hash.to_hex();
                tracing::info!(
                    target: "determinism",
                    mode = %mode,
                    tenant_id,
                    request_id,
                    worker_id,
                    nonce,
                    manifest_prefix = %manifest_hex.get(..16).unwrap_or(&manifest_hex),
                    "Deriving strict request seed (AOS_DEBUG_DETERMINISM=1)"
                );
            }
            let label = format!(
                "request:{}:{}:{}:{}:{}",
                manifest_hash.to_hex(),
                tenant_id,
                request_id,
                worker_id,
                nonce
            );
            Ok(derive_seed(global, &label))
        }
        SeedMode::BestEffort => {
            let manifest_hash = manifest
                .cloned()
                .unwrap_or_else(|| B3Hash::hash(format!("no_manifest:{}", tenant_id).as_bytes()));
            if determinism_debug_enabled() {
                let manifest_hex = manifest_hash.to_hex();
                tracing::info!(
                    target: "determinism",
                    mode = %mode,
                    tenant_id,
                    request_id,
                    worker_id,
                    nonce,
                    manifest_prefix = %manifest_hex.get(..16).unwrap_or(&manifest_hex),
                    "Deriving best-effort request seed (AOS_DEBUG_DETERMINISM=1)"
                );
            }
            let label = format!(
                "request:{}:{}:{}:{}:{}",
                manifest_hash.to_hex(),
                tenant_id,
                request_id,
                worker_id,
                nonce
            );
            Ok(derive_seed(global, &label))
        }
        SeedMode::NonDeterministic => {
            if cfg!(debug_assertions) {
                let mut bytes = [0u8; 32];
                fastrand::Rng::new().fill(&mut bytes);
                Ok(bytes)
            } else {
                Err(AosError::DeterminismViolation(
                    "NonDeterministic seed_mode is only permitted in debug builds".to_string(),
                ))
            }
        }
    }
}

/// Derive multiple seeds at once
pub fn derive_seeds(global: &B3Hash, labels: &[&str]) -> Vec<[u8; 32]> {
    labels.iter().map(|l| derive_seed(global, l)).collect()
}

/// Derive a deterministic seed with full entropy isolation
///
/// Incorporates: manifest_hash || adapter_dir || worker_id || label || nonce.
/// This ensures complete isolation between different:
/// - Manifests (different model configurations)
/// - Adapter directories (different adapter sets)
/// - Workers (different execution contexts)
/// - Labels (router vs sampling vs adapter-scoped)
/// - Nonces (different RNG instances)
///
/// Per Determinism Ruleset #2: All seeds MUST incorporate full context so that
/// the same request context produces identical seeds while any changed
/// parameter yields a distinct seed.
pub fn derive_seed_full(
    global: &B3Hash,
    manifest_hash: &B3Hash,
    adapter_dir_hash: &B3Hash,
    worker_id: u32,
    label: &str,
    nonce: u64,
) -> [u8; 32] {
    // Construct composite label with all entropy sources
    let composite_label = format!(
        "{}:{}:{}:{}:{}",
        label,
        manifest_hash.to_hex(),
        adapter_dir_hash.to_hex(),
        worker_id,
        nonce
    );

    derive_seed(global, &composite_label)
}

/// Hash an adapter directory path deterministically
///
/// Converts path to canonical form and hashes it for use in seed derivation
pub fn hash_adapter_dir(adapter_dir: &std::path::Path) -> B3Hash {
    // Canonicalize path to handle symlinks and relative paths
    let canonical_path = adapter_dir
        .canonicalize()
        .unwrap_or_else(|_| adapter_dir.to_path_buf());

    // Hash the canonical path string
    B3Hash::hash(canonical_path.to_string_lossy().as_bytes())
}

/// Derive per-adapter seed with layer isolation and reuse prevention
pub fn derive_adapter_seed(
    global: &B3Hash,
    adapter_id: usize,
    layer: usize,
    nonce: u64,
) -> std::result::Result<[u8; 32], String> {
    let label = format!("adapter_{}:layer_{}", adapter_id, layer);

    // Check for reuse
    let key = (label.clone(), nonce);
    let mut registry = SEED_REGISTRY.lock().unwrap();
    if registry.contains_key(&key) {
        return Err(format!(
            "Seed reuse detected: {} with nonce {}",
            label, nonce
        ));
    }
    registry.insert(key, true);

    Ok(derive_seed(global, &label))
}

/// Clear seed registry (call at inference boundaries)
pub fn clear_seed_registry() {
    let mut registry = SEED_REGISTRY.lock().unwrap();
    registry.clear();
    tracing::debug!("Cleared seed registry");
}

/// Check if seed registry is empty (for tests/validation)
pub fn is_seed_registry_empty() -> bool {
    let registry = SEED_REGISTRY.lock().unwrap();
    registry.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_mode_default_matches_central_default() {
        assert_eq!(SeedMode::default(), crate::defaults::DEFAULT_SEED_MODE);
    }

    #[test]
    fn execution_profile_default_uses_central_seed_mode() {
        let profile = ExecutionProfile::default();
        assert_eq!(profile.seed_mode, crate::defaults::DEFAULT_SEED_MODE);
    }

    #[test]
    fn test_seed_deterministic() {
        let global = B3Hash::hash(b"test");
        let seed1 = derive_seed(&global, "component_a");
        let seed2 = derive_seed(&global, "component_a");
        assert_eq!(seed1, seed2);
    }

    #[test]
    fn derive_seed_typed_same_tuple_is_stable() {
        let global = B3Hash::hash(b"global-seed");
        let manifest = B3Hash::hash(b"manifest-a");
        let seed_a = derive_seed_typed(&global, SeedLabel::Router, &manifest, 42, 7);
        let seed_b = derive_seed_typed(&global, SeedLabel::Router, &manifest, 42, 7);

        assert_eq!(
            seed_a, seed_b,
            "Identical derivation tuple must yield identical seed bytes"
        );
    }

    #[test]
    fn derive_seed_typed_nonce_changes_output() {
        let global = B3Hash::hash(b"global-seed");
        let manifest = B3Hash::hash(b"manifest-a");
        let seed_a = derive_seed_typed(&global, SeedLabel::Router, &manifest, 42, 7);
        let seed_b = derive_seed_typed(&global, SeedLabel::Router, &manifest, 42, 8);

        assert_ne!(
            seed_a, seed_b,
            "Changing nonce must change derived seed bytes"
        );
    }

    #[test]
    fn test_different_labels() {
        let global = B3Hash::hash(b"test");
        let seed1 = derive_seed(&global, "component_a");
        let seed2 = derive_seed(&global, "component_b");
        assert_ne!(seed1, seed2);
    }

    #[test]
    fn test_different_globals() {
        let global1 = B3Hash::hash(b"test1");
        let global2 = B3Hash::hash(b"test2");
        let seed1 = derive_seed(&global1, "component");
        let seed2 = derive_seed(&global2, "component");
        assert_ne!(seed1, seed2);
    }

    #[test]
    fn test_derive_seed_full_deterministic() {
        let global = B3Hash::hash(b"global");
        let manifest = B3Hash::hash(b"manifest");
        let adapter_dir = B3Hash::hash(b"/adapters/test");

        let seed1 = derive_seed_full(&global, &manifest, &adapter_dir, 1, "router", 0);
        let seed2 = derive_seed_full(&global, &manifest, &adapter_dir, 1, "router", 0);

        assert_eq!(seed1, seed2, "Same inputs should produce same seed");
    }

    #[test]
    fn test_derive_seed_full_isolation() {
        let global = B3Hash::hash(b"global");
        let manifest1 = B3Hash::hash(b"manifest1");
        let manifest2 = B3Hash::hash(b"manifest2");
        let adapter_dir = B3Hash::hash(b"/adapters/test");

        let seed1 = derive_seed_full(&global, &manifest1, &adapter_dir, 1, "router", 0);
        let seed2 = derive_seed_full(&global, &manifest2, &adapter_dir, 1, "router", 0);

        assert_ne!(
            seed1, seed2,
            "Different manifests should produce different seeds"
        );
    }

    #[test]
    fn test_derive_seed_full_nonce_isolation() {
        let global = B3Hash::hash(b"global");
        let manifest = B3Hash::hash(b"manifest");
        let adapter_dir = B3Hash::hash(b"/adapters/test");

        let seed1 = derive_seed_full(&global, &manifest, &adapter_dir, 1, "router", 0);
        let seed2 = derive_seed_full(&global, &manifest, &adapter_dir, 1, "router", 1);

        assert_ne!(
            seed1, seed2,
            "Different nonces should produce different seeds"
        );
    }

    #[test]
    fn test_derive_seed_full_worker_isolation() {
        let global = B3Hash::hash(b"global");
        let manifest = B3Hash::hash(b"manifest");
        let adapter_dir = B3Hash::hash(b"/adapters/test");

        let seed_worker1 = derive_seed_full(&global, &manifest, &adapter_dir, 1, "router", 0);
        let seed_worker2 = derive_seed_full(&global, &manifest, &adapter_dir, 2, "router", 0);

        assert_ne!(
            seed_worker1, seed_worker2,
            "Different worker IDs should produce different seeds"
        );
    }

    #[test]
    fn test_derive_seed_full_adapter_dir_isolation() {
        let global = B3Hash::hash(b"global");
        let manifest = B3Hash::hash(b"manifest");
        let adapter_dir_a = B3Hash::hash(b"/adapters/a");
        let adapter_dir_b = B3Hash::hash(b"/adapters/b");

        let seed_a = derive_seed_full(&global, &manifest, &adapter_dir_a, 1, "router", 0);
        let seed_b = derive_seed_full(&global, &manifest, &adapter_dir_b, 1, "router", 0);

        assert_ne!(
            seed_a, seed_b,
            "Different adapter directories should produce different seeds"
        );
    }

    #[test]
    fn test_hash_adapter_dir() {
        use std::path::Path;

        let path1 = Path::new("/adapters/test");
        let hash1 = hash_adapter_dir(path1);
        let hash2 = hash_adapter_dir(path1);

        assert_eq!(hash1, hash2, "Same path should produce same hash");
    }

    #[test]
    fn test_request_seed_strict_requires_manifest() {
        let global = B3Hash::hash(b"global");
        let result = derive_request_seed(&global, None, "tenant", "req", 1, 0, SeedMode::Strict);
        assert!(result.is_err());
    }

    #[test]
    fn test_request_seed_best_effort_with_manifest() {
        let global = B3Hash::hash(b"global");
        let manifest = B3Hash::hash(b"manifest");
        let seed_a = derive_request_seed(
            &global,
            Some(&manifest),
            "tenant",
            "req",
            1,
            0,
            SeedMode::BestEffort,
        )
        .unwrap();
        let seed_b = derive_request_seed(
            &global,
            Some(&manifest),
            "tenant",
            "req",
            1,
            0,
            SeedMode::BestEffort,
        )
        .unwrap();
        assert_eq!(seed_a, seed_b, "Deterministic with manifest");
    }

    #[test]
    fn test_request_seed_best_effort_without_manifest_is_tenant_scoped() {
        let global = B3Hash::hash(b"global");
        let seed_a =
            derive_request_seed(&global, None, "tenant_a", "req", 1, 0, SeedMode::BestEffort)
                .unwrap();
        let seed_b =
            derive_request_seed(&global, None, "tenant_a", "req", 1, 0, SeedMode::BestEffort)
                .unwrap();
        let seed_c =
            derive_request_seed(&global, None, "tenant_b", "req", 1, 0, SeedMode::BestEffort)
                .unwrap();

        assert_eq!(seed_a, seed_b, "Same tenant uses stable fallback hash");
        assert_ne!(seed_a, seed_c, "Different tenant fallback differs");
    }

    #[test]
    fn request_seed_varies_by_worker_id() {
        let global = B3Hash::hash(b"global");
        let manifest = B3Hash::hash(b"manifest");

        let worker_a = derive_request_seed(
            &global,
            Some(&manifest),
            "tenant",
            "req-1",
            7,
            0,
            SeedMode::Strict,
        )
        .expect("worker A seed derives");
        let worker_b = derive_request_seed(
            &global,
            Some(&manifest),
            "tenant",
            "req-1",
            8,
            0,
            SeedMode::Strict,
        )
        .expect("worker B seed derives");

        assert_ne!(
            worker_a, worker_b,
            "Changing worker id must change derived request seed"
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    fn test_request_seed_nondeterministic_varies() {
        let global = B3Hash::hash(b"global");
        let seed_a = derive_request_seed(
            &global,
            None,
            "tenant",
            "req",
            1,
            0,
            SeedMode::NonDeterministic,
        )
        .unwrap();
        let seed_b = derive_request_seed(
            &global,
            None,
            "tenant",
            "req",
            1,
            0,
            SeedMode::NonDeterministic,
        )
        .unwrap();
        assert_ne!(
            seed_a, seed_b,
            "NonDeterministic should produce different seeds"
        );
    }
}
