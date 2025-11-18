// Synthetic Repository for Code Ingestion Testing
// This file contains various Rust patterns for testing AdapterOS code ingestion

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

/// A content-addressable hash structure using BLAKE3
///
/// This struct represents a cryptographic hash that uniquely identifies
/// content in the system. It's used for deterministic content addressing
/// across distributed systems.
///
/// # Examples
///
/// ```
/// let hash = ContentHash::from_bytes(b"example data");
/// assert_eq!(hash.to_hex().len(), 64); // BLAKE3 produces 32 bytes = 64 hex chars
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentHash {
    bytes: [u8; 32],
}

impl ContentHash {
    /// Creates a new ContentHash from raw data
    ///
    /// # Arguments
    ///
    /// * `data` - The input data to hash
    ///
    /// # Returns
    ///
    /// A new ContentHash instance representing the BLAKE3 hash of the input
    pub fn from_bytes(data: &[u8]) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(data);
        let hash = hasher.finalize();
        ContentHash {
            bytes: *hash.as_bytes(),
        }
    }

    /// Converts the hash to a hexadecimal string representation
    ///
    /// # Returns
    ///
    /// A 64-character hex string representing the 32-byte hash
    pub fn to_hex(&self) -> String {
        hex::encode(&self.bytes)
    }

    /// Derives a domain-separated seed from this hash using HKDF
    ///
    /// This is critical for deterministic execution - the same base hash
    /// and domain label will always produce the same derived seed.
    ///
    /// # Arguments
    ///
    /// * `domain` - A domain label like "router", "dropout", or "sampling"
    ///
    /// # Returns
    ///
    /// A 32-byte array suitable for seeding a ChaCha20 RNG
    pub fn derive_seed(&self, domain: &str) -> [u8; 32] {
        use hkdf::Hkdf;
        use sha2::Sha256;

        let hkdf = Hkdf::<Sha256>::new(None, &self.bytes);
        let mut output = [0u8; 32];
        hkdf.expand(domain.as_bytes(), &mut output)
            .expect("HKDF expansion failed");
        output
    }
}

/// Manages a pool of LoRA adapters with lifecycle tracking
///
/// This struct implements a least-recently-used (LRU) eviction policy
/// with memory pressure awareness. Adapters transition through states:
/// Unloaded → Cold → Warm → Hot → Resident
pub struct AdapterPool {
    /// Map of adapter IDs to their metadata
    adapters: Arc<Mutex<HashMap<String, AdapterMetadata>>>,
    /// Maximum memory in bytes
    max_memory_bytes: usize,
    /// Current memory usage in bytes
    current_memory_bytes: Arc<Mutex<usize>>,
}

/// Metadata for a single adapter in the pool
#[derive(Debug, Clone)]
pub struct AdapterMetadata {
    pub id: String,
    pub state: AdapterState,
    pub memory_mb: usize,
    pub activation_count: u64,
    pub last_used: std::time::Instant,
}

/// Lifecycle states for adapters
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterState {
    Unloaded,
    Cold,
    Warm,
    Hot,
    Resident,
}

impl AdapterPool {
    /// Creates a new adapter pool with specified memory limit
    ///
    /// # Arguments
    ///
    /// * `max_memory_mb` - Maximum memory in megabytes
    ///
    /// # Returns
    ///
    /// A new AdapterPool instance
    pub fn new(max_memory_mb: usize) -> Self {
        AdapterPool {
            adapters: Arc::new(Mutex::new(HashMap::new())),
            max_memory_bytes: max_memory_mb * 1024 * 1024,
            current_memory_bytes: Arc::new(Mutex::new(0)),
        }
    }

    /// Loads an adapter into memory, evicting cold adapters if needed
    ///
    /// This method implements tiered eviction:
    /// 1. Try to fit adapter in current memory
    /// 2. If insufficient space, evict Cold adapters first
    /// 3. Then evict Warm adapters if still needed
    /// 4. Never evict Hot or Resident adapters automatically
    ///
    /// # Arguments
    ///
    /// * `adapter_id` - Unique identifier for the adapter
    /// * `memory_mb` - Memory required in megabytes
    ///
    /// # Returns
    ///
    /// Result indicating success or memory pressure error
    pub fn load_adapter(&self, adapter_id: &str, memory_mb: usize) -> Result<(), String> {
        let memory_bytes = memory_mb * 1024 * 1024;

        // Check if we need to evict
        let mut current = self.current_memory_bytes.lock().unwrap();
        if *current + memory_bytes > self.max_memory_bytes {
            // Try evicting cold adapters first
            self.evict_by_tier(AdapterState::Cold)?;

            // Still not enough? Try warm adapters
            if *current + memory_bytes > self.max_memory_bytes {
                self.evict_by_tier(AdapterState::Warm)?;
            }

            // Still not enough? Return error
            if *current + memory_bytes > self.max_memory_bytes {
                return Err(format!(
                    "Insufficient memory: need {} MB, have {} MB available",
                    memory_mb,
                    (self.max_memory_bytes - *current) / 1024 / 1024
                ));
            }
        }

        // Load the adapter
        let mut adapters = self.adapters.lock().unwrap();
        adapters.insert(
            adapter_id.to_string(),
            AdapterMetadata {
                id: adapter_id.to_string(),
                state: AdapterState::Cold,
                memory_mb,
                activation_count: 0,
                last_used: std::time::Instant::now(),
            },
        );
        *current += memory_bytes;

        Ok(())
    }

    /// Evicts adapters in a specific tier to free memory
    ///
    /// # Arguments
    ///
    /// * `tier` - The adapter state tier to evict from
    ///
    /// # Returns
    ///
    /// Result indicating success or if no adapters were available to evict
    fn evict_by_tier(&self, tier: AdapterState) -> Result<(), String> {
        let mut adapters = self.adapters.lock().unwrap();
        let mut current_memory = self.current_memory_bytes.lock().unwrap();

        // Find all adapters in this tier
        let to_evict: Vec<String> = adapters
            .iter()
            .filter(|(_, meta)| meta.state == tier)
            .map(|(id, _)| id.clone())
            .collect();

        // Evict them
        for id in to_evict {
            if let Some(meta) = adapters.remove(&id) {
                *current_memory -= meta.memory_mb * 1024 * 1024;
            }
        }

        Ok(())
    }

    /// Records an adapter activation for lifecycle promotion
    ///
    /// This implements the promotion logic:
    /// - Cold → Warm: after 10 activations
    /// - Warm → Hot: after 50 activations
    /// - Hot → Resident: manual pinning only
    ///
    /// # Arguments
    ///
    /// * `adapter_id` - The adapter that was activated
    pub fn record_activation(&self, adapter_id: &str) {
        let mut adapters = self.adapters.lock().unwrap();
        if let Some(meta) = adapters.get_mut(adapter_id) {
            meta.activation_count += 1;
            meta.last_used = std::time::Instant::now();

            // Promotion logic
            match meta.state {
                AdapterState::Cold if meta.activation_count >= 10 => {
                    meta.state = AdapterState::Warm;
                }
                AdapterState::Warm if meta.activation_count >= 50 => {
                    meta.state = AdapterState::Hot;
                }
                _ => {}
            }
        }
    }

    /// Returns current memory usage as a percentage
    ///
    /// # Returns
    ///
    /// Memory usage from 0.0 to 1.0 (0% to 100%)
    pub fn memory_usage_percent(&self) -> f32 {
        let current = *self.current_memory_bytes.lock().unwrap();
        (current as f32) / (self.max_memory_bytes as f32)
    }
}

/// A K-sparse router for selecting top-K adapters based on gating scores
///
/// This struct implements the core routing logic for LoRA adapter selection.
/// It uses Q15 fixed-point quantization for deterministic computation.
pub struct KSparseRouter {
    k: usize,
    num_adapters: usize,
}

impl KSparseRouter {
    /// Creates a new K-sparse router
    ///
    /// # Arguments
    ///
    /// * `k` - Number of adapters to select (K in K-sparse)
    /// * `num_adapters` - Total number of available adapters
    pub fn new(k: usize, num_adapters: usize) -> Self {
        KSparseRouter { k, num_adapters }
    }

    /// Selects top-K adapters based on gating scores
    ///
    /// The selection process:
    /// 1. Compute gating scores (Q15 quantized)
    /// 2. Sort by score (descending)
    /// 3. Select top K
    /// 4. Use deterministic tie-breaking if scores are equal
    ///
    /// # Arguments
    ///
    /// * `gate_logits` - Raw logit scores for each adapter
    /// * `seed` - Seed for deterministic tie-breaking
    ///
    /// # Returns
    ///
    /// Indices of the selected K adapters
    pub fn select_top_k(&self, gate_logits: &[f32], seed: [u8; 32]) -> Vec<usize> {
        // Quantize to Q15 for determinism
        let quantized: Vec<i16> = gate_logits
            .iter()
            .map(|&x| (x * 32767.0).clamp(-32768.0, 32767.0) as i16)
            .collect();

        // Create (score, index) pairs
        let mut scored: Vec<(i16, usize)> = quantized
            .iter()
            .enumerate()
            .map(|(i, &score)| (score, i))
            .collect();

        // Sort by score descending, use seeded RNG for ties
        use rand::SeedableRng;
        use rand_chacha::ChaCha20Rng;
        let mut rng = ChaCha20Rng::from_seed(seed);

        scored.sort_by(|a, b| {
            match b.0.cmp(&a.0) {
                std::cmp::Ordering::Equal => {
                    // Deterministic tie-breaking using seeded random
                    use rand::Rng;
                    if rng.gen::<bool>() {
                        std::cmp::Ordering::Greater
                    } else {
                        std::cmp::Ordering::Less
                    }
                }
                other => other,
            }
        });

        // Take top K
        scored.iter().take(self.k).map(|(_, idx)| *idx).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_hash_determinism() {
        let data = b"test data";
        let hash1 = ContentHash::from_bytes(data);
        let hash2 = ContentHash::from_bytes(data);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hkdf_domain_separation() {
        let hash = ContentHash::from_bytes(b"base seed");
        let router_seed = hash.derive_seed("router");
        let dropout_seed = hash.derive_seed("dropout");
        assert_ne!(router_seed, dropout_seed);
    }

    #[test]
    fn test_adapter_pool_eviction() {
        let pool = AdapterPool::new(100); // 100 MB max

        // Load adapters
        pool.load_adapter("adapter1", 40).unwrap();
        pool.load_adapter("adapter2", 40).unwrap();

        // This should trigger eviction
        pool.load_adapter("adapter3", 40).unwrap();

        assert!(pool.memory_usage_percent() <= 1.0);
    }

    #[test]
    fn test_k_sparse_selection() {
        let router = KSparseRouter::new(2, 5);
        let logits = vec![0.1, 0.9, 0.3, 0.8, 0.2];
        let seed = [0u8; 32];

        let selected = router.select_top_k(&logits, seed);
        assert_eq!(selected.len(), 2);
        assert!(selected.contains(&1)); // 0.9 should be selected
        assert!(selected.contains(&3)); // 0.8 should be selected
    }

    #[test]
    fn test_adapter_promotion() {
        let pool = AdapterPool::new(1000);
        pool.load_adapter("test_adapter", 50).unwrap();

        // Activate 10 times to promote Cold → Warm
        for _ in 0..10 {
            pool.record_activation("test_adapter");
        }

        let adapters = pool.adapters.lock().unwrap();
        let meta = adapters.get("test_adapter").unwrap();
        assert_eq!(meta.state, AdapterState::Warm);
    }
}
