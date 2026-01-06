//! Thread-local seed tracking and propagation
//!
//! This module provides thread-local storage for cryptographic seeds,
//! collision detection, and deterministic propagation across async tasks.

use adapteros_core::seed::SeedMode;
use parking_lot::Mutex;
use rand::{Rng, RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::ThreadId;
use tracing::{debug, error, info, warn};

/// Global seed registry for collision detection
#[allow(clippy::type_complexity)]
static GLOBAL_SEED_REGISTRY: std::sync::OnceLock<Arc<Mutex<HashMap<ThreadId, [u8; 32]>>>> =
    std::sync::OnceLock::new();

/// Global collision counter
static SEED_COLLISION_COUNT: AtomicU64 = AtomicU64::new(0);

/// Global propagation failure counter
static SEED_PROPAGATION_FAILURES: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static THREAD_SEED: RefCell<Option<ThreadSeed>> = const { RefCell::new(None) };
}

/// Thread-local seed with collision detection
#[derive(Clone, Debug)]
pub struct ThreadSeed {
    /// The actual seed bytes
    seed: [u8; 32],
    /// Thread ID for collision detection
    thread_id: ThreadId,
    /// Generation counter to detect overwrites
    generation: u64,
    /// Parent seed (for propagation tracking)
    #[allow(dead_code)]
    parent_seed: Option<[u8; 32]>,
}

impl ThreadSeed {
    /// Create a new thread seed
    pub fn new(seed: [u8; 32]) -> Self {
        let thread_id = std::thread::current().id();
        let generation = 0;

        Self {
            seed,
            thread_id,
            generation,
            parent_seed: None,
        }
    }

    /// Create a child seed derived from parent
    pub fn derive_child(&self, label: &str) -> Self {
        let derived = adapteros_core::derive_seed(&adapteros_core::B3Hash::new(self.seed), label);

        Self {
            seed: derived,
            thread_id: std::thread::current().id(),
            generation: self.generation + 1,
            parent_seed: Some(self.seed),
        }
    }

    /// Get the seed bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.seed
    }

    /// Get the thread ID
    pub fn thread_id(&self) -> ThreadId {
        self.thread_id
    }

    /// Get the generation counter
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Create a deterministic RNG from this seed
    pub fn rng(&self) -> ChaCha20Rng {
        ChaCha20Rng::from_seed(self.seed)
    }

    /// Generate deterministic random value
    pub fn random<T>(&self) -> T
    where
        rand::distributions::Standard: rand::distributions::Distribution<T>,
    {
        let mut rng = self.rng();
        rng.gen()
    }
}

/// Initialize global seed registry
fn init_global_registry() -> Arc<Mutex<HashMap<ThreadId, [u8; 32]>>> {
    Arc::new(Mutex::new(HashMap::new()))
}

/// Get or initialize global seed registry
fn global_seed_registry() -> &'static Arc<Mutex<HashMap<ThreadId, [u8; 32]>>> {
    GLOBAL_SEED_REGISTRY.get_or_init(init_global_registry)
}

/// Seed registry for managing thread-local seeds
#[derive(Clone, Debug)]
pub struct SeedRegistry {
    registry: Arc<Mutex<HashMap<ThreadId, [u8; 32]>>>,
}

impl SeedRegistry {
    /// Create a new seed registry
    pub fn new() -> Self {
        Self {
            registry: global_seed_registry().clone(),
        }
    }

    /// Register a seed for the current thread
    pub fn register_seed(&self, seed: [u8; 32]) -> Result<(), SeedError> {
        let thread_id = std::thread::current().id();

        let mut registry = self.registry.lock();
        if let Some(existing_seed) = registry.get(&thread_id) {
            if *existing_seed != seed {
                SEED_COLLISION_COUNT.fetch_add(1, Ordering::Relaxed);
                warn!(
                    thread_id = ?thread_id,
                    "Seed collision detected for thread"
                );
                return Err(SeedError::CollisionDetected);
            }
        }

        registry.insert(thread_id, seed);
        debug!(thread_id = ?thread_id, "Registered seed for thread");
        Ok(())
    }

    /// Get seed for a thread
    pub fn get_seed(&self, thread_id: ThreadId) -> Option<[u8; 32]> {
        self.registry.lock().get(&thread_id).copied()
    }

    /// Unregister seed for a thread
    pub fn unregister_seed(&self, thread_id: ThreadId) {
        self.registry.lock().remove(&thread_id);
        debug!(thread_id = ?thread_id, "Unregistered seed for thread");
    }

    /// Get collision count
    pub fn collision_count(&self) -> u64 {
        SEED_COLLISION_COUNT.load(Ordering::Relaxed)
    }

    /// Get propagation failure count
    pub fn propagation_failure_count(&self) -> u64 {
        SEED_PROPAGATION_FAILURES.load(Ordering::Relaxed)
    }

    /// Get all registered threads and their seeds
    pub fn registered_threads(&self) -> HashMap<ThreadId, [u8; 32]> {
        self.registry.lock().clone()
    }
}

impl Default for SeedRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Set the thread-local seed
pub fn set_thread_seed(seed: [u8; 32]) -> Result<(), SeedError> {
    let registry = SeedRegistry::new();
    registry.register_seed(seed)?;

    let thread_seed = ThreadSeed::new(seed);
    THREAD_SEED.with(|ts| {
        *ts.borrow_mut() = Some(thread_seed);
    });

    debug!("Set thread-local seed");
    Ok(())
}

/// Get the current thread-local seed
pub fn get_thread_seed() -> Option<ThreadSeed> {
    THREAD_SEED.with(|ts| ts.borrow().clone())
}

/// Check if current thread has a seed
pub fn has_thread_seed() -> bool {
    THREAD_SEED.with(|ts| ts.borrow().is_some())
}

/// Derive a child seed from current thread seed
pub fn derive_child_seed(label: &str) -> Result<ThreadSeed, SeedError> {
    let parent_seed = get_thread_seed().ok_or(SeedError::NoThreadSeed)?;

    let child_seed = parent_seed.derive_child(label);

    Ok(child_seed)
}

/// Execute a function with a specific thread seed
pub fn with_thread_seed<F, T>(seed: [u8; 32], f: F) -> Result<T, SeedError>
where
    F: FnOnce() -> T,
{
    let registry = SeedRegistry::new();
    registry.register_seed(seed)?;

    let thread_seed = ThreadSeed::new(seed);
    let previous_seed = THREAD_SEED.with(|ts| ts.replace(Some(thread_seed)));

    let result = f();

    // Restore previous seed
    THREAD_SEED.with(|ts| {
        *ts.borrow_mut() = previous_seed;
    });

    Ok(result)
}

/// Execute an async function with a specific thread seed
pub async fn with_thread_seed_async<F, Fut, T>(seed: [u8; 32], f: F) -> Result<T, SeedError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    let registry = SeedRegistry::new();
    registry.register_seed(seed)?;

    let thread_seed = ThreadSeed::new(seed);
    let previous_seed = THREAD_SEED.with(|ts| ts.replace(Some(thread_seed)));

    let result = f().await;

    // Restore previous seed
    THREAD_SEED.with(|ts| {
        *ts.borrow_mut() = previous_seed;
    });

    Ok(result)
}

/// Generate deterministic random value using thread seed
pub fn deterministic_random<T>() -> Result<T, SeedError>
where
    rand::distributions::Standard: rand::distributions::Distribution<T>,
{
    let seed = get_thread_seed().ok_or(SeedError::NoThreadSeed)?;

    Ok(seed.random())
}

/// Create a deterministic RNG from thread seed
pub fn deterministic_rng() -> Result<ChaCha20Rng, SeedError> {
    let seed = get_thread_seed().ok_or(SeedError::NoThreadSeed)?;

    Ok(seed.rng())
}

/// Propagate current thread seed to a new async task
pub fn propagate_seed_to_task<F, Fut>(f: F) -> impl std::future::Future<Output = Fut::Output>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future,
{
    let current_seed = get_thread_seed();

    async move {
        if let Some(seed) = current_seed {
            // Set the seed in the new task context
            if let Err(e) = set_thread_seed(*seed.as_bytes()) {
                SEED_PROPAGATION_FAILURES.fetch_add(1, Ordering::Relaxed);
                error!(error = %e, "Failed to propagate thread seed to task");
            } else {
                debug!("Propagated thread seed to task");
            }
        }

        f().await
    }
}

/// Spawn a deterministic task with seed propagation
pub fn spawn_with_seed_propagation<F, Fut>(
    description: String,
    f: F,
) -> Result<super::DeterministicJoinHandle, SeedError>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let current_seed = get_thread_seed();

    let future = async move {
        if let Some(seed) = current_seed {
            if let Err(e) = set_thread_seed(*seed.as_bytes()) {
                SEED_PROPAGATION_FAILURES.fetch_add(1, Ordering::Relaxed);
                error!(error = %e, "Failed to propagate thread seed to spawned task");

                // Emit telemetry event for seed propagation failure
                // Note: In a real implementation, we would need access to telemetry here
                // For now, we just increment the counter
            }
        }

        f().await
    };

    super::spawn_deterministic(description, future)
        .map_err(|e| SeedError::DeterministicExecError(e.to_string()))
}

/// Seed-related error types
#[derive(Debug, Clone, thiserror::Error)]
pub enum SeedError {
    #[error("Seed collision detected")]
    CollisionDetected,
    #[error("No thread-local seed available")]
    NoThreadSeed,
    #[error("Deterministic execution error: {0}")]
    DeterministicExecError(String),
    #[error("Seed validation failed: {0}")]
    ValidationError(String),
    /// PRD-DET-001: Strict mode rejects fallback seeds
    #[error("Strict mode requires primary seed; fallback rejected")]
    StrictModeFallbackRejected,
}

/// Seed telemetry metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedMetrics {
    /// Total seed collisions detected
    pub collision_count: u64,
    /// Total seed propagation failures
    pub propagation_failure_count: u64,
    /// Number of threads with registered seeds
    pub active_threads: usize,
    /// Current thread generations (by thread ID)
    pub thread_generations: HashMap<String, u64>,
}

impl SeedMetrics {
    /// Collect current seed metrics
    pub fn collect() -> Self {
        let registry = SeedRegistry::new();

        let active_threads = registry.registered_threads().len();

        // Note: thread_generations would need access to ThreadSeed instances
        // For now, we'll leave it empty and populate it when we have access
        // to the actual ThreadSeed data
        let thread_generations = HashMap::new();

        Self {
            collision_count: registry.collision_count(),
            propagation_failure_count: registry.propagation_failure_count(),
            active_threads,
            thread_generations,
        }
    }
}

/// Global seed manager for cross-thread coordination
#[derive(Clone, Debug)]
pub struct GlobalSeedManager {
    fallback_rng: Arc<Mutex<Option<ChaCha20Rng>>>,
}

impl GlobalSeedManager {
    /// Create a new global seed manager
    pub fn new() -> Self {
        Self {
            fallback_rng: Arc::new(Mutex::new(None)),
        }
    }

    /// Initialize global seed with fallback entropy
    pub fn init_with_fallback(
        &self,
        primary_seed: Option<[u8; 32]>,
    ) -> Result<[u8; 32], SeedError> {
        let seed = if let Some(seed) = primary_seed {
            seed
        } else {
            // Use a fixed deterministic fallback seed derived via HKDF
            // This maintains determinism but should only be used in development/testing
            use hkdf::Hkdf;
            use sha2::Sha256;

            // Fixed entropy source for fallback - deterministic but unique per domain
            const FALLBACK_IKM: &[u8] = b"adapteros-deterministic-exec-fallback-seed-v1";
            const FALLBACK_SALT: &[u8] = b"global-seed-manager-emergency";

            let hk = Hkdf::<Sha256>::new(Some(FALLBACK_SALT), FALLBACK_IKM);
            let mut fallback_seed = [0u8; 32];
            hk.expand(b"fallback", &mut fallback_seed)
                .expect("HKDF expansion failed for fallback seed");

            error!(
                seed_source = "fallback",
                "No primary seed provided; using deterministic fallback seed (dev/test only)"
            );
            fallback_seed
        };

        // Store fallback RNG for emergency use
        let fallback_rng = ChaCha20Rng::from_seed(seed);
        *self.fallback_rng.lock() = Some(fallback_rng);

        info!(
            seed_source = "fallback",
            "Initialized global seed manager with fallback entropy"
        );
        Ok(seed)
    }

    /// Get fallback RNG (for emergency use only)
    pub fn fallback_rng(&self) -> Option<ChaCha20Rng> {
        self.fallback_rng.lock().as_ref().cloned()
    }

    /// Emergency seed generation using fallback RNG
    pub fn emergency_seed(&self) -> Result<[u8; 32], SeedError> {
        let mut rng = self.fallback_rng.lock();
        if let Some(ref mut rng) = *rng {
            let mut seed = [0u8; 32];
            rng.fill_bytes(&mut seed);
            warn!(
                seed_source = "fallback_rng",
                "Generated emergency seed using fallback RNG"
            );
            Ok(seed)
        } else {
            Err(SeedError::ValidationError(
                "No fallback RNG available".to_string(),
            ))
        }
    }

    /// Initialize global seed with mode-aware fallback handling (PRD-DET-001).
    ///
    /// This method respects the `SeedMode` when deciding whether to allow fallback:
    ///
    /// - **Strict**: Returns `StrictModeFallbackRejected` if no primary seed provided.
    ///   Production deployments MUST provide explicit seeds.
    /// - **BestEffort**: Uses deterministic fallback seed when primary is absent.
    ///   Suitable for development and testing.
    /// - **NonDeterministic**: Generates a random seed (useful for benchmarking).
    ///
    /// # Arguments
    ///
    /// * `primary_seed` - Optional explicit seed bytes
    /// * `mode` - The seed mode controlling fallback behavior
    ///
    /// # Returns
    ///
    /// The initialized seed bytes, or an error if strict mode rejects fallback.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let manager = GlobalSeedManager::new();
    ///
    /// // Production: must provide seed
    /// let result = manager.init_with_mode(None, SeedMode::Strict);
    /// assert!(result.is_err()); // StrictModeFallbackRejected
    ///
    /// // Development: fallback allowed
    /// let seed = manager.init_with_mode(None, SeedMode::BestEffort)?;
    /// ```
    pub fn init_with_mode(
        &self,
        primary_seed: Option<[u8; 32]>,
        mode: SeedMode,
    ) -> Result<[u8; 32], SeedError> {
        let seed = match (primary_seed, mode) {
            // Primary seed provided - use it regardless of mode
            (Some(seed), _) => {
                info!(
                    seed_mode = ?mode,
                    seed_source = "primary",
                    "Initialized with explicit primary seed"
                );
                seed
            }

            // Strict mode without primary seed - reject
            (None, SeedMode::Strict) => {
                error!(
                    seed_mode = "strict",
                    "Strict mode requires primary seed; fallback rejected"
                );
                return Err(SeedError::StrictModeFallbackRejected);
            }

            // BestEffort mode without primary seed - use deterministic fallback
            (None, SeedMode::BestEffort) => {
                use hkdf::Hkdf;
                use sha2::Sha256;

                const FALLBACK_IKM: &[u8] = b"adapteros-deterministic-exec-fallback-seed-v1";
                const FALLBACK_SALT: &[u8] = b"global-seed-manager-emergency";

                let hk = Hkdf::<Sha256>::new(Some(FALLBACK_SALT), FALLBACK_IKM);
                let mut fallback_seed = [0u8; 32];
                hk.expand(b"fallback", &mut fallback_seed)
                    .expect("HKDF expansion failed for fallback seed");

                warn!(
                    seed_mode = "best_effort",
                    seed_source = "fallback",
                    "No primary seed provided; using deterministic fallback (dev/test only)"
                );
                fallback_seed
            }

            // NonDeterministic mode - generate random seed
            (None, SeedMode::NonDeterministic) => {
                let mut random_seed = [0u8; 32];
                rand::thread_rng().fill_bytes(&mut random_seed);

                warn!(
                    seed_mode = "non_deterministic",
                    seed_source = "random",
                    "Generated random seed (non-replayable, benchmarking only)"
                );
                random_seed
            }
        };

        // Store fallback RNG for emergency use
        let fallback_rng = ChaCha20Rng::from_seed(seed);
        *self.fallback_rng.lock() = Some(fallback_rng);

        Ok(seed)
    }
}

impl Default for GlobalSeedManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_thread_seed_basic() {
        let seed = [42u8; 32];
        let thread_seed = ThreadSeed::new(seed);

        assert_eq!(thread_seed.as_bytes(), &seed);
        assert_eq!(thread_seed.generation(), 0);
        assert!(thread_seed.parent_seed.is_none());
    }

    #[test]
    fn test_thread_seed_derivation() {
        let seed = [42u8; 32];
        let parent_seed = ThreadSeed::new(seed);
        let child_seed = parent_seed.derive_child("test");

        assert_ne!(child_seed.as_bytes(), &seed);
        assert_eq!(child_seed.generation(), 1);
        assert_eq!(child_seed.parent_seed, Some(seed));
    }

    #[test]
    fn test_seed_registry() {
        let registry = SeedRegistry::new();
        let seed1 = [1u8; 32];
        let seed2 = [2u8; 32];

        // Register seed
        registry.register_seed(seed1).unwrap();

        // Check it's registered
        assert_eq!(registry.get_seed(std::thread::current().id()), Some(seed1));

        // Try to register different seed (should fail)
        assert!(registry.register_seed(seed2).is_err());

        // Collision count should be incremented
        assert_eq!(registry.collision_count(), 1);
    }

    #[test]
    fn test_thread_local_seed() {
        let seed = [123u8; 32];

        // Initially no seed
        assert!(!has_thread_seed());

        // Set seed
        set_thread_seed(seed).unwrap();
        assert!(has_thread_seed());

        // Get seed
        let retrieved = get_thread_seed().unwrap();
        assert_eq!(retrieved.as_bytes(), &seed);

        // Generate random value
        let random_val: u32 = deterministic_random().unwrap();
        let mut rng = deterministic_rng().unwrap();
        let random_val2 = rng.gen::<u32>();
        assert_eq!(random_val, random_val2);
    }

    #[test]
    fn test_child_seed_derivation() {
        // Use a unique seed for this test to avoid collisions
        let seed = [
            99u8, 98u8, 97u8, 96u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0,
        ];

        // Use with_thread_seed to isolate the test
        let child = with_thread_seed(seed, || derive_child_seed("test").unwrap()).unwrap();

        assert_eq!(child.generation(), 1);
        assert_eq!(child.parent_seed, Some(seed));
        assert_ne!(child.as_bytes(), &seed);
    }

    #[test]
    fn test_seed_metrics() {
        let seed = [
            77u8, 76u8, 75u8, 74u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0,
        ];

        // Use with_thread_seed to isolate the test
        let metrics = with_thread_seed(seed, || SeedMetrics::collect()).unwrap();

        // Check that we have at least 1 active thread and the metrics are being tracked
        assert!(metrics.active_threads >= 1);
        // Note: collision_count and propagation_failure_count may be > 0 due to other tests
        // Counters are u64 so they are guaranteed to be >= 0
    }

    #[tokio::test]
    async fn test_seed_propagation() {
        let seed = [88u8; 32];

        // Test that seed propagation works by using propagate_seed_to_task
        let propagated_seed = Arc::new(std::sync::Mutex::new(None));

        // Set up the seed context
        set_thread_seed(seed).unwrap();

        // Create a task function that captures the seed
        let propagated_clone = propagated_seed.clone();
        let task_fn = move || async move {
            let captured_seed = get_thread_seed();
            *propagated_clone.lock().unwrap() = captured_seed;
            42 // return some value
        };

        // Propagate the seed to the task
        let propagated_task = propagate_seed_to_task(task_fn);

        // Execute the task
        let result = propagated_task.await;

        // Check that the seed was propagated correctly
        assert_eq!(result, 42);
        let captured_seed = propagated_seed.lock().unwrap().as_ref().unwrap().clone();
        assert_eq!(captured_seed.as_bytes(), &seed);
    }

    #[test]
    fn test_global_seed_manager() {
        let manager = GlobalSeedManager::new();
        let seed = [55u8; 32];

        let initialized_seed = manager.init_with_fallback(Some(seed)).unwrap();
        assert_eq!(initialized_seed, seed);

        // Test fallback seed generation
        let fallback_seed = manager.emergency_seed().unwrap();
        assert_ne!(fallback_seed, seed); // Should be different due to RNG
    }

    /// PRD-DET-001: Strict mode rejects fallback seeds
    #[test]
    fn test_init_with_mode_strict_rejects_fallback() {
        let manager = GlobalSeedManager::new();

        // Strict mode without primary seed MUST fail
        let result = manager.init_with_mode(None, SeedMode::Strict);
        assert!(
            matches!(result, Err(SeedError::StrictModeFallbackRejected)),
            "Strict mode should reject None seed, got: {:?}",
            result
        );
    }

    /// PRD-DET-001: Strict mode accepts explicit seeds
    #[test]
    fn test_init_with_mode_strict_accepts_primary() {
        let manager = GlobalSeedManager::new();
        let seed = [42u8; 32];

        // Strict mode with primary seed should succeed
        let result = manager.init_with_mode(Some(seed), SeedMode::Strict);
        assert!(result.is_ok(), "Strict mode should accept primary seed");
        assert_eq!(result.unwrap(), seed);
    }

    /// PRD-DET-001: BestEffort mode allows fallback
    #[test]
    fn test_init_with_mode_best_effort_allows_fallback() {
        let manager = GlobalSeedManager::new();

        // BestEffort mode without primary seed should use deterministic fallback
        let result = manager.init_with_mode(None, SeedMode::BestEffort);
        assert!(
            result.is_ok(),
            "BestEffort mode should allow fallback, got: {:?}",
            result
        );

        // Fallback should be deterministic (same seed every time)
        let manager2 = GlobalSeedManager::new();
        let result2 = manager2.init_with_mode(None, SeedMode::BestEffort);
        assert_eq!(
            result.unwrap(),
            result2.unwrap(),
            "BestEffort fallback should be deterministic"
        );
    }

    /// PRD-DET-001: NonDeterministic mode generates random seeds
    #[test]
    fn test_init_with_mode_nondeterministic_generates_random() {
        let manager1 = GlobalSeedManager::new();
        let manager2 = GlobalSeedManager::new();

        // NonDeterministic mode without primary seed should generate random seeds
        let result1 = manager1.init_with_mode(None, SeedMode::NonDeterministic);
        let result2 = manager2.init_with_mode(None, SeedMode::NonDeterministic);

        assert!(result1.is_ok());
        assert!(result2.is_ok());

        // Seeds should be different (with overwhelming probability)
        assert_ne!(
            result1.unwrap(),
            result2.unwrap(),
            "NonDeterministic mode should generate different seeds"
        );
    }
}
