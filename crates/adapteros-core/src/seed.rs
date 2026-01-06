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

// =============================================================================
// HKDF Constants and Invariants
// =============================================================================

/// HKDF algorithm version for schema compatibility tracking.
///
/// Increment this version if the derivation algorithm changes in a way that
/// would produce different outputs for the same inputs. This allows downstream
/// systems to detect version mismatches and handle migrations.
///
/// # Version History
/// - v1: Initial HKDF-SHA256 implementation with 32-byte output
/// - v2: Canonical HKDF extract+expand using the BLAKE3 global seed as IKM
pub const HKDF_ALGORITHM_VERSION: u32 = 2;

/// Required output length for all seed derivations.
///
/// All HKDF-derived seeds MUST be exactly 32 bytes to ensure compatibility
/// with ChaCha20Rng and other consumers that expect this size.
pub const HKDF_OUTPUT_LENGTH: usize = 32;

lazy_static::lazy_static! {
    /// Seed registry to prevent reuse
    static ref SEED_REGISTRY: Mutex<HashMap<(String, u64), bool>> = Mutex::new(HashMap::new());
}

// =============================================================================
// TypedSeed - Versioned seed with integrity checks
// =============================================================================

/// A versioned seed with integrity validation for cross-boundary determinism.
///
/// TypedSeed ensures that seeds are validated at every FFI and context boundary,
/// preventing "seed contract broke" scenarios where a backend accepts a seed from
/// the wrong derivation scheme.
///
/// # Invariants
///
/// 1. **Version must match**: The `version` field must equal `HKDF_ALGORITHM_VERSION`
///    when used for inference. Mismatches indicate schema drift.
/// 2. **Checksum must validate**: The `checksum` field must equal `BLAKE3(bytes)`
///    to detect corruption or tampering.
/// 3. **Fail closed**: In strict determinism mode, version/checksum mismatches
///    cause immediate failure rather than silent drift.
///
/// # Example
///
/// ```ignore
/// use adapteros_core::seed::{TypedSeed, derive_typed_seed, HKDF_ALGORITHM_VERSION};
///
/// let global = B3Hash::hash(b"model-manifest");
/// let typed_seed = derive_typed_seed(&global, "mlx");
///
/// // Validate before use
/// assert!(typed_seed.validate().is_ok());
/// assert_eq!(typed_seed.version, HKDF_ALGORITHM_VERSION);
///
/// // Use raw bytes for FFI
/// let seed_bytes = typed_seed.bytes();
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypedSeed {
    /// HKDF algorithm version used to derive this seed.
    /// Must match `HKDF_ALGORITHM_VERSION` for the current schema.
    pub version: u32,
    /// The 32-byte derived seed value.
    bytes: [u8; HKDF_OUTPUT_LENGTH],
    /// BLAKE3 checksum of `bytes` for integrity validation.
    pub checksum: B3Hash,
}

impl TypedSeed {
    /// Create a new TypedSeed from raw bytes with the current algorithm version.
    ///
    /// The checksum is computed automatically from the bytes.
    pub fn new(bytes: [u8; HKDF_OUTPUT_LENGTH]) -> Self {
        let checksum = B3Hash::hash(&bytes);
        Self {
            version: HKDF_ALGORITHM_VERSION,
            bytes,
            checksum,
        }
    }

    /// Create a TypedSeed with a specific version (for testing/migration).
    pub fn with_version(bytes: [u8; HKDF_OUTPUT_LENGTH], version: u32) -> Self {
        let checksum = B3Hash::hash(&bytes);
        Self {
            version,
            bytes,
            checksum,
        }
    }

    /// Get the raw seed bytes.
    pub fn bytes(&self) -> &[u8; HKDF_OUTPUT_LENGTH] {
        &self.bytes
    }

    /// Get the raw seed bytes as a slice.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Validate the seed's integrity (checksum matches bytes).
    ///
    /// # Returns
    /// `Ok(())` if checksum is valid, `Err` with details otherwise.
    pub fn validate_checksum(&self) -> Result<()> {
        let computed = B3Hash::hash(&self.bytes);
        if computed != self.checksum {
            return Err(AosError::DeterminismViolation(format!(
                "TypedSeed checksum mismatch: expected {}, computed {}",
                self.checksum.to_short_hex(),
                computed.to_short_hex()
            )));
        }
        Ok(())
    }

    /// Check if this seed's version matches the expected version.
    ///
    /// # Arguments
    /// * `expected` - The expected HKDF algorithm version
    ///
    /// # Returns
    /// `true` if versions match, `false` otherwise.
    pub fn version_matches(&self, expected: u32) -> bool {
        self.version == expected
    }

    /// Validate the seed for use with the current algorithm version.
    ///
    /// Checks both version compatibility and checksum integrity.
    /// In strict mode, this will fail if version doesn't match current.
    ///
    /// # Returns
    /// `Ok(())` if seed is valid for current schema, `Err` with details otherwise.
    pub fn validate(&self) -> Result<()> {
        // First check checksum integrity
        self.validate_checksum()?;

        // Then check version compatibility
        if !self.version_matches(HKDF_ALGORITHM_VERSION) {
            return Err(AosError::DeterminismViolation(format!(
                "TypedSeed version mismatch: seed version {} != current algorithm version {}. \
                 This seed was derived with an incompatible HKDF scheme.",
                self.version, HKDF_ALGORITHM_VERSION
            )));
        }

        Ok(())
    }

    /// Validate the seed, respecting the current determinism configuration.
    ///
    /// In strict mode, both version and checksum must be valid.
    /// In best-effort mode, logs a warning for version mismatch but allows use.
    ///
    /// # Returns
    /// `Ok(())` if seed passes validation for current mode, `Err` otherwise.
    pub fn validate_with_config(&self) -> Result<()> {
        let config = get_determinism_config();

        // Checksum is always validated
        self.validate_checksum()?;

        // Version check depends on strict mode
        if !self.version_matches(HKDF_ALGORITHM_VERSION) {
            if config.strict_mode {
                return Err(AosError::DeterminismViolation(format!(
                    "TypedSeed version mismatch in strict mode: seed v{} != algorithm v{}",
                    self.version, HKDF_ALGORITHM_VERSION
                )));
            } else {
                tracing::warn!(
                    seed_version = self.version,
                    algorithm_version = HKDF_ALGORITHM_VERSION,
                    "TypedSeed version mismatch (allowing in best-effort mode)"
                );
            }
        }

        Ok(())
    }

    /// Convert to a compact hex representation for logging/debugging.
    pub fn to_debug_string(&self) -> String {
        format!(
            "TypedSeed(v{}, bytes={}, checksum={})",
            self.version,
            hex::encode(&self.bytes[..8]),
            self.checksum.to_short_hex()
        )
    }
}

impl std::fmt::Display for TypedSeed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "TypedSeed(v{}, {}...)",
            self.version,
            hex::encode(&self.bytes[..4])
        )
    }
}

// =============================================================================
// Seed Digest and Lineage for Receipt Binding (PR-004)
// =============================================================================

/// Compute a digest of a seed for receipt binding.
///
/// This produces a BLAKE3 hash of the seed bytes, suitable for
/// inclusion in receipts without exposing the raw seed.
///
/// # Security Note
///
/// The digest is one-way: the original seed cannot be recovered.
/// However, if an attacker knows the seed, they can verify the digest.
/// This is acceptable since the digest is for binding, not secrecy.
pub fn compute_seed_digest(seed: &[u8; 32]) -> B3Hash {
    B3Hash::hash(seed)
}

/// Compute a seed digest from a TypedSeed.
pub fn compute_typed_seed_digest(seed: &TypedSeed) -> B3Hash {
    B3Hash::hash(seed.bytes())
}

/// Seed lineage information for receipt binding.
///
/// Captures the cryptographic binding between a receipt and its seed
/// derivation context, enabling detection of seed manipulation without
/// exposing raw seed material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeedLineage {
    /// BLAKE3 digest of the request seed (never raw seed)
    pub root_seed_digest: B3Hash,
    /// Seed mode used for derivation
    pub seed_mode: SeedMode,
    /// Whether seed was derived from manifest hash
    pub has_manifest_binding: bool,
    /// HKDF algorithm version used
    pub hkdf_version: u32,
}

impl SeedLineage {
    /// Create seed lineage from a TypedSeed and derivation context.
    pub fn from_typed_seed(seed: &TypedSeed, mode: SeedMode, has_manifest: bool) -> Self {
        Self {
            root_seed_digest: compute_typed_seed_digest(seed),
            seed_mode: mode,
            has_manifest_binding: has_manifest,
            hkdf_version: seed.version,
        }
    }

    /// Create seed lineage from raw seed bytes.
    pub fn from_raw_seed(seed: &[u8; 32], mode: SeedMode, has_manifest: bool) -> Self {
        Self {
            root_seed_digest: compute_seed_digest(seed),
            seed_mode: mode,
            has_manifest_binding: has_manifest,
            hkdf_version: HKDF_ALGORITHM_VERSION,
        }
    }

    /// Verify that a seed matches this lineage.
    pub fn verify_seed(&self, seed: &[u8; 32]) -> bool {
        let digest = B3Hash::hash(seed);
        digest == self.root_seed_digest
    }

    /// Verify that a typed seed matches this lineage.
    pub fn verify_typed_seed(&self, seed: &TypedSeed) -> bool {
        self.verify_seed(seed.bytes())
    }

    /// Get the root seed digest as a hex string.
    pub fn digest_hex(&self) -> String {
        self.root_seed_digest.to_hex()
    }
}

/// Derive a typed seed with version tracking and checksum validation.
///
/// This is the preferred method for seed derivation when the seed will be
/// passed across FFI boundaries or stored for replay.
///
/// # Arguments
/// * `global` - The global seed (BLAKE3 hash of manifest/inputs)
/// * `label` - Domain separation label (e.g., "mlx", "router", "sampling")
///
/// # Returns
/// A `TypedSeed` with the current algorithm version and computed checksum.
pub fn derive_typed_seed(global: &B3Hash, label: &str) -> TypedSeed {
    let bytes = derive_seed(global, label);
    TypedSeed::new(bytes)
}

/// Derive a typed seed with full entropy isolation.
///
/// Combines manifest hash, adapter directory hash, worker ID, label, and nonce
/// for complete isolation between execution contexts.
pub fn derive_typed_seed_full(
    global: &B3Hash,
    manifest_hash: &B3Hash,
    adapter_dir_hash: &B3Hash,
    worker_id: u32,
    label: &str,
    nonce: u64,
) -> TypedSeed {
    let bytes = derive_seed_full(
        global,
        manifest_hash,
        adapter_dir_hash,
        worker_id,
        label,
        nonce,
    );
    TypedSeed::new(bytes)
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
        self.fixed_seed.is_some()
            || self.fixed_timestamp.is_some()
            || self.stable_ordering
            || self.strict_mode
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
///
/// # Entropy Note
///
/// When expanding from a u64 fixed_seed, the input entropy is limited to 64 bits.
/// HKDF's extract-then-expand paradigm ensures the output is cryptographically
/// uniform, but the effective entropy cannot exceed the input entropy. This is
/// acceptable for deterministic replay (where reproducibility is the goal), but
/// callers requiring high-entropy seeds for security purposes should use the
/// random path (no fixed_seed configured).
pub fn get_deterministic_seed_bytes() -> [u8; 32] {
    let config = get_determinism_config();
    match config.fixed_seed {
        Some(seed) => {
            // Canonical derivation: hash the fixed seed into a BLAKE3 global seed,
            // then derive a domain-separated HKDF seed via derive_seed().
            let seed_bytes = seed.to_le_bytes();
            let global = B3Hash::hash(&seed_bytes);
            derive_seed(&global, "determinism-config-seed")
        }
        None => {
            let mut bytes = [0u8; HKDF_OUTPUT_LENGTH];
            fastrand::Rng::new().fill(&mut bytes);
            bytes
        }
    }
}

/// Check if stable ordering should be used.
///
/// Returns true if the current configuration requires stable ordering.
pub fn should_use_stable_ordering() -> bool {
    let config = get_determinism_config();
    config.stable_ordering || config.strict_mode
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
    let env_enabled = *FLAG.get_or_init(|| match std::env::var("AOS_DEBUG_DETERMINISM") {
        Ok(val) => {
            let normalized = val.to_ascii_lowercase();
            normalized == "1" || normalized == "true" || normalized == "yes"
        }
        Err(_) => false,
    });
    env_enabled || get_determinism_config().trace_seeds
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

/// Derive a deterministic seed from a global seed and label.
///
/// Uses HKDF-SHA256 for key derivation. All RNG in the system
/// must derive from these seeds to ensure determinism.
///
/// # HKDF Invariants
///
/// - **Algorithm**: HKDF-SHA256 is the ONLY supported KDF. Do not substitute
///   other algorithms as this would break replay compatibility.
/// - **Output size**: Always produces exactly [`HKDF_OUTPUT_LENGTH`] (32) bytes,
///   matching ChaCha20Rng's seed requirement.
/// - **Determinism**: Given identical `(global, label)` inputs, always produces
///   identical output bytes across all platforms and versions.
///
pub fn derive_seed(global: &B3Hash, label: &str) -> [u8; HKDF_OUTPUT_LENGTH] {
    let mut effective_global = *global;
    if let Some(seed) = get_determinism_config().fixed_seed {
        let seed_bytes = seed.to_le_bytes();
        effective_global = B3Hash::hash(&seed_bytes);
        if determinism_debug_enabled() {
            tracing::info!(
                target: "determinism",
                label = label,
                fixed_seed = seed,
                "Overriding global seed with determinism config"
            );
        }
    }

    // Canonical HKDF: treat the global seed as IKM and run extract+expand.
    let hk = Hkdf::<Sha256>::new(None, effective_global.as_bytes());
    let mut okm = [0u8; HKDF_OUTPUT_LENGTH];
    hk.expand(label.as_bytes(), &mut okm)
        .expect("HKDF_OUTPUT_LENGTH is valid for HKDF-SHA256");

    // Validate HKDF output matches expected length
    debug_assert_eq!(
        okm.len(),
        HKDF_OUTPUT_LENGTH,
        "HKDF output must be exactly {} bytes",
        HKDF_OUTPUT_LENGTH
    );

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

/// Derive a deterministic u64 seed from a global seed and label.
pub fn derive_seed_u64(global: &B3Hash, label: &str) -> u64 {
    let seed_bytes = derive_seed(global, label);
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&seed_bytes[..8]);
    u64::from_le_bytes(bytes)
}

/// Derive a deterministic u64 seed from raw input bytes and a label.
///
/// This hashes the inputs into a BLAKE3 global seed, then runs HKDF-SHA256
/// expansion using `label` to ensure domain separation.
pub fn derive_seed_u64_from_inputs(label: &str, inputs: &[u8]) -> u64 {
    let global = B3Hash::hash(inputs);
    derive_seed_u64(&global, label)
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
    let manifest_hex = manifest_hash.to_hex();
    if determinism_debug_enabled() {
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
        manifest_hex,
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
    if matches!(mode, SeedMode::NonDeterministic) && is_strict_determinism_mode() {
        return Err(AosError::DeterminismViolation(
            "NonDeterministic seed_mode is not permitted when strict determinism is enabled"
                .to_string(),
        ));
    }
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
/// Converts path to canonical form and normalizes path separators for
/// cross-platform consistency, then hashes it for use in seed derivation.
///
/// # Platform Consistency
///
/// This function normalizes path separators to forward slashes (`/`) before
/// hashing, ensuring that the same logical path produces identical hashes
/// regardless of whether it's processed on Windows, macOS, or Linux.
///
/// See [`crate::path_normalization`] for details on the normalization rules.
pub fn hash_adapter_dir(adapter_dir: &std::path::Path) -> B3Hash {
    use crate::path_normalization::normalize_path_for_sorting;

    // Canonicalize path to handle symlinks and relative paths
    let canonical_path = adapter_dir
        .canonicalize()
        .unwrap_or_else(|_| adapter_dir.to_path_buf());

    // Normalize path separators for cross-platform determinism
    let normalized = normalize_path_for_sorting(&canonical_path);

    // Hash the normalized path string
    B3Hash::hash(normalized.as_bytes())
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
    let mut registry = SEED_REGISTRY
        .lock()
        .map_err(|e| format!("Seed registry lock poisoned: {}", e))?;
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
///
/// Logs a warning if the registry lock is poisoned (indicates prior panic in seed path).
pub fn clear_seed_registry() {
    match SEED_REGISTRY.lock() {
        Ok(mut registry) => {
            registry.clear();
            tracing::debug!("Cleared seed registry");
        }
        Err(e) => {
            // Lock is poisoned - a previous thread panicked while holding it.
            // Clear the poisoned mutex to recover, since seed registry is non-critical state.
            let mut registry = e.into_inner();
            registry.clear();
            tracing::warn!("Cleared poisoned seed registry (prior panic in seed derivation path)");
        }
    }
}

/// Check if seed registry is empty (for tests/validation)
///
/// Returns `true` if empty or if the registry lock is poisoned.
pub fn is_seed_registry_empty() -> bool {
    match SEED_REGISTRY.lock() {
        Ok(registry) => registry.is_empty(),
        Err(e) => {
            // Lock is poisoned - treat as empty for validation purposes
            // but log a warning since this indicates a prior panic.
            tracing::warn!("Seed registry lock poisoned during is_empty check: {}", e);
            true
        }
    }
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
    fn test_fixed_seed_overrides_global() {
        let global1 = B3Hash::hash(b"test1");
        let global2 = B3Hash::hash(b"test2");
        let config = DeterminismConfig::builder().fixed_seed(42).build();

        let seed1 = with_determinism_config(config.clone(), || derive_seed(&global1, "component"));
        let seed2 = with_determinism_config(config, || derive_seed(&global2, "component"));

        assert_eq!(seed1, seed2);
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

    // =========================================================================
    // Golden Vector Tests for Version Stability
    // =========================================================================

    /// Golden vector test for HKDF derivation.
    ///
    /// This test ensures the HKDF derivation produces consistent output across
    /// versions. If this test fails, it means the derivation algorithm has changed
    /// and HKDF_ALGORITHM_VERSION must be incremented.
    #[test]
    fn test_hkdf_golden_vector_stability() {
        // Known test vector: hash of "determinism-golden-test-vector"
        let global = B3Hash::hash(b"determinism-golden-test-vector");
        let seed = derive_seed(&global, "golden-test-label");

        // Compute checksum of derived seed for version verification
        let checksum = B3Hash::hash(&seed);
        let checksum_hex = checksum.to_hex();

        // This is the expected checksum for HKDF_ALGORITHM_VERSION = 2
        // If this changes, the HKDF algorithm has drifted!
        //
        // To update: Run the test, get the new checksum, increment
        // HKDF_ALGORITHM_VERSION, and update this expected value.
        let expected_prefix = "a1425ff4"; // First 8 hex chars of checksum

        assert_eq!(
            &checksum_hex[..8],
            expected_prefix,
            "HKDF derivation has changed! If intentional, increment \
             HKDF_ALGORITHM_VERSION (currently {}) and update this test.",
            HKDF_ALGORITHM_VERSION
        );
    }

    /// Test that HKDF algorithm version is at least 2 (current canonical version).
    #[test]
    fn test_hkdf_algorithm_version_minimum() {
        const { assert!(HKDF_ALGORITHM_VERSION >= 2) };
    }

    /// Test that derive_seed produces exactly HKDF_OUTPUT_LENGTH bytes.
    #[test]
    fn test_hkdf_output_length_invariant() {
        let global = B3Hash::hash(b"test-output-length");
        let seed = derive_seed(&global, "length-test");
        assert_eq!(
            seed.len(),
            HKDF_OUTPUT_LENGTH,
            "HKDF output must be exactly {} bytes",
            HKDF_OUTPUT_LENGTH
        );
    }

    // =========================================================================
    // TypedSeed Tests
    // =========================================================================

    #[test]
    fn typed_seed_new_sets_current_version() {
        let global = B3Hash::hash(b"test");
        let typed = derive_typed_seed(&global, "test-label");
        assert_eq!(
            typed.version, HKDF_ALGORITHM_VERSION,
            "TypedSeed should use current algorithm version"
        );
    }

    #[test]
    fn typed_seed_checksum_validation_passes() {
        let global = B3Hash::hash(b"test");
        let typed = derive_typed_seed(&global, "test-label");
        assert!(
            typed.validate_checksum().is_ok(),
            "Valid TypedSeed should pass checksum validation"
        );
    }

    #[test]
    fn typed_seed_checksum_validation_detects_corruption() {
        let global = B3Hash::hash(b"test");
        let mut typed = derive_typed_seed(&global, "test-label");
        // Corrupt the checksum by creating a new one with wrong hash
        typed.checksum = B3Hash::hash(b"wrong");
        assert!(
            typed.validate_checksum().is_err(),
            "Corrupted TypedSeed should fail checksum validation"
        );
    }

    #[test]
    fn typed_seed_version_mismatch_detected() {
        let bytes = [0u8; HKDF_OUTPUT_LENGTH];
        let typed = TypedSeed::with_version(bytes, 1); // Old version
        assert!(
            !typed.version_matches(HKDF_ALGORITHM_VERSION),
            "Version 1 should not match current version"
        );
        assert!(
            typed.validate().is_err(),
            "Version mismatch should fail validation"
        );
    }

    #[test]
    fn typed_seed_validate_passes_for_current_version() {
        let global = B3Hash::hash(b"test");
        let typed = derive_typed_seed(&global, "test-label");
        assert!(
            typed.validate().is_ok(),
            "Current version TypedSeed should pass validation"
        );
    }

    #[test]
    fn typed_seed_bytes_match_derive_seed() {
        let global = B3Hash::hash(b"determinism-test");
        let raw_seed = derive_seed(&global, "consistency");
        let typed_seed = derive_typed_seed(&global, "consistency");
        assert_eq!(
            *typed_seed.bytes(),
            raw_seed,
            "TypedSeed bytes must match raw derive_seed output"
        );
    }

    #[test]
    fn typed_seed_deterministic() {
        let global = B3Hash::hash(b"determinism-check");
        let typed1 = derive_typed_seed(&global, "label");
        let typed2 = derive_typed_seed(&global, "label");
        assert_eq!(
            typed1, typed2,
            "Same inputs should produce identical TypedSeed"
        );
    }

    #[test]
    fn typed_seed_full_deterministic() {
        let global = B3Hash::hash(b"global");
        let manifest = B3Hash::hash(b"manifest");
        let adapter_dir = B3Hash::hash(b"/adapters");

        let typed1 = derive_typed_seed_full(&global, &manifest, &adapter_dir, 1, "label", 0);
        let typed2 = derive_typed_seed_full(&global, &manifest, &adapter_dir, 1, "label", 0);
        assert_eq!(
            typed1, typed2,
            "Same inputs should produce identical TypedSeed"
        );
    }

    #[test]
    fn typed_seed_validate_with_config_strict_mode() {
        let bytes = [0u8; HKDF_OUTPUT_LENGTH];
        let typed = TypedSeed::with_version(bytes, 1); // Old version

        let config = DeterminismConfig::builder().strict_mode(true).build();
        let result = with_determinism_config(config, || typed.validate_with_config());
        assert!(
            result.is_err(),
            "Strict mode should fail on version mismatch"
        );
    }

    #[test]
    fn typed_seed_display_format() {
        let global = B3Hash::hash(b"test");
        let typed = derive_typed_seed(&global, "test");
        let display = format!("{}", typed);
        assert!(
            display.starts_with("TypedSeed(v"),
            "Display format should start with TypedSeed(v"
        );
        assert!(
            display.contains(&format!("v{}", HKDF_ALGORITHM_VERSION)),
            "Display format should include version"
        );
    }

    #[test]
    fn typed_seed_serialization_roundtrip() {
        let global = B3Hash::hash(b"serialization-test");
        let typed = derive_typed_seed(&global, "serde");

        let json = serde_json::to_string(&typed).expect("serialize");
        let deserialized: TypedSeed = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(
            typed, deserialized,
            "Serialization roundtrip must preserve TypedSeed"
        );
        assert!(
            deserialized.validate().is_ok(),
            "Deserialized seed must validate"
        );
    }

    // =========================================================================
    // SeedLineage Tests (PR-004)
    // =========================================================================

    #[test]
    fn test_seed_digest_deterministic() {
        let seed = [42u8; 32];
        let digest1 = compute_seed_digest(&seed);
        let digest2 = compute_seed_digest(&seed);
        assert_eq!(digest1, digest2, "Same seed must produce same digest");
    }

    #[test]
    fn test_different_seeds_different_digests() {
        let seed1 = [1u8; 32];
        let seed2 = [2u8; 32];
        let digest1 = compute_seed_digest(&seed1);
        let digest2 = compute_seed_digest(&seed2);
        assert_ne!(
            digest1, digest2,
            "Different seeds must produce different digests"
        );
    }

    #[test]
    fn test_seed_lineage_from_typed_seed() {
        let global = B3Hash::hash(b"lineage-test");
        let typed_seed = derive_typed_seed(&global, "lineage");
        let lineage = SeedLineage::from_typed_seed(&typed_seed, SeedMode::Strict, true);

        assert_eq!(lineage.seed_mode, SeedMode::Strict);
        assert!(lineage.has_manifest_binding);
        assert_eq!(lineage.hkdf_version, HKDF_ALGORITHM_VERSION);
    }

    #[test]
    fn test_seed_lineage_from_raw_seed() {
        let seed = [42u8; 32];
        let lineage = SeedLineage::from_raw_seed(&seed, SeedMode::BestEffort, false);

        assert_eq!(lineage.seed_mode, SeedMode::BestEffort);
        assert!(!lineage.has_manifest_binding);
        assert_eq!(lineage.hkdf_version, HKDF_ALGORITHM_VERSION);
    }

    #[test]
    fn test_seed_lineage_verification() {
        let seed = [42u8; 32];
        let lineage = SeedLineage::from_raw_seed(&seed, SeedMode::Strict, true);

        assert!(lineage.verify_seed(&seed), "Lineage should verify matching seed");
        assert!(
            !lineage.verify_seed(&[0u8; 32]),
            "Lineage should reject non-matching seed"
        );
    }

    #[test]
    fn test_seed_lineage_typed_verification() {
        let global = B3Hash::hash(b"typed-lineage-test");
        let typed_seed = derive_typed_seed(&global, "verification");
        let lineage = SeedLineage::from_typed_seed(&typed_seed, SeedMode::Strict, true);

        assert!(
            lineage.verify_typed_seed(&typed_seed),
            "Lineage should verify matching typed seed"
        );

        let other_typed = derive_typed_seed(&global, "other-label");
        assert!(
            !lineage.verify_typed_seed(&other_typed),
            "Lineage should reject different typed seed"
        );
    }

    #[test]
    fn test_seed_lineage_serialization() {
        let seed = [42u8; 32];
        let lineage = SeedLineage::from_raw_seed(&seed, SeedMode::Strict, true);

        let json = serde_json::to_string(&lineage).expect("serialize lineage");
        let deserialized: SeedLineage = serde_json::from_str(&json).expect("deserialize lineage");

        assert_eq!(
            lineage.root_seed_digest, deserialized.root_seed_digest,
            "Digest must survive serialization"
        );
        assert_eq!(lineage.seed_mode, deserialized.seed_mode);
        assert_eq!(lineage.has_manifest_binding, deserialized.has_manifest_binding);
        assert_eq!(lineage.hkdf_version, deserialized.hkdf_version);
    }

    #[test]
    fn test_seed_lineage_digest_hex() {
        let seed = [42u8; 32];
        let lineage = SeedLineage::from_raw_seed(&seed, SeedMode::Strict, true);
        let hex = lineage.digest_hex();

        assert_eq!(hex.len(), 64, "BLAKE3 hex should be 64 chars");
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_typed_seed_digest_matches_raw() {
        let raw_seed = [42u8; 32];
        let typed_seed = TypedSeed::new(raw_seed);

        let raw_digest = compute_seed_digest(&raw_seed);
        let typed_digest = compute_typed_seed_digest(&typed_seed);

        assert_eq!(
            raw_digest, typed_digest,
            "Raw and typed digests must match for same bytes"
        );
    }
}
