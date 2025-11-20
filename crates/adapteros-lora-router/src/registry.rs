//! Adapter registry for mapping adapter IDs to u16 indices
//!
//! This module provides thread-safe management of adapter ID → u16 index mappings
//! for the K-sparse router. It ensures:
//! - Consistent index assignment across hot-swaps
//! - Index reuse when adapters are unloaded
//! - Thread-safe concurrent access
//! - Efficient lookup in both directions (ID → index, index → ID)

use adapteros_core::{AosError, Result};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// Maximum number of adapters that can be registered (u16::MAX)
pub const MAX_ADAPTERS: usize = u16::MAX as usize;

/// Thread-safe adapter registry mapping adapter IDs to u16 indices
#[derive(Debug, Clone)]
pub struct AdapterRegistry {
    inner: Arc<RwLock<AdapterRegistryInner>>,
}

/// Internal registry state
#[derive(Debug)]
struct AdapterRegistryInner {
    /// Map from adapter_id (String) to u16 index
    id_to_index: HashMap<String, u16>,
    /// Map from u16 index to adapter_id (String)
    index_to_id: HashMap<u16, String>,
    /// Free indices available for reuse
    free_indices: Vec<u16>,
    /// Next index to allocate (if no free indices available)
    next_index: u16,
}

impl AdapterRegistry {
    /// Create a new empty adapter registry
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(AdapterRegistryInner {
                id_to_index: HashMap::new(),
                index_to_id: HashMap::new(),
                free_indices: Vec::new(),
                next_index: 0,
            })),
        }
    }

    /// Create a registry pre-populated with adapter IDs from a manifest
    ///
    /// This assigns sequential indices to adapters in the order they appear
    /// in the manifest, ensuring deterministic index assignment.
    ///
    /// # Arguments
    /// * `adapter_ids` - Ordered list of adapter IDs from manifest
    ///
    /// # Returns
    /// A new registry with adapters registered in order
    ///
    /// # Errors
    /// Returns an error if more than MAX_ADAPTERS are provided
    pub fn from_manifest(adapter_ids: &[String]) -> Result<Self> {
        if adapter_ids.len() > MAX_ADAPTERS {
            return Err(AosError::Config(format!(
                "Too many adapters: {} exceeds maximum {}",
                adapter_ids.len(),
                MAX_ADAPTERS
            )));
        }

        let mut id_to_index = HashMap::with_capacity(adapter_ids.len());
        let mut index_to_id = HashMap::with_capacity(adapter_ids.len());

        for (idx, id) in adapter_ids.iter().enumerate() {
            let index = idx as u16;
            id_to_index.insert(id.clone(), index);
            index_to_id.insert(index, id.clone());
        }

        Ok(Self {
            inner: Arc::new(RwLock::new(AdapterRegistryInner {
                id_to_index,
                index_to_id,
                free_indices: Vec::new(),
                next_index: adapter_ids.len() as u16,
            })),
        })
    }

    /// Register a new adapter and assign it a u16 index
    ///
    /// If the adapter is already registered, returns its existing index.
    /// Otherwise, allocates a new index (reusing free indices first).
    ///
    /// # Arguments
    /// * `adapter_id` - String identifier for the adapter
    ///
    /// # Returns
    /// The u16 index assigned to this adapter
    ///
    /// # Errors
    /// Returns an error if all indices are exhausted
    pub fn register(&self, adapter_id: String) -> Result<u16> {
        let mut inner = self.inner.write();

        // Check if already registered
        if let Some(&index) = inner.id_to_index.get(&adapter_id) {
            return Ok(index);
        }

        // Allocate new index (prefer reusing free indices)
        let index = if let Some(free_index) = inner.free_indices.pop() {
            free_index
        } else {
            let index = inner.next_index;
            if index == u16::MAX {
                return Err(AosError::Config(format!(
                    "Adapter index overflow: cannot register more than {} adapters",
                    MAX_ADAPTERS
                )));
            }
            inner.next_index += 1;
            index
        };

        // Register the mapping
        inner.id_to_index.insert(adapter_id.clone(), index);
        inner.index_to_id.insert(index, adapter_id);

        Ok(index)
    }

    /// Unregister an adapter and free its index for reuse
    ///
    /// # Arguments
    /// * `adapter_id` - String identifier for the adapter
    ///
    /// # Returns
    /// The u16 index that was freed
    ///
    /// # Errors
    /// Returns an error if the adapter is not registered
    pub fn unregister(&self, adapter_id: &str) -> Result<u16> {
        let mut inner = self.inner.write();

        let index = inner.id_to_index.remove(adapter_id).ok_or_else(|| {
            AosError::NotFound(format!("Adapter '{}' not registered", adapter_id))
        })?;

        inner.index_to_id.remove(&index);
        inner.free_indices.push(index);

        Ok(index)
    }

    /// Get the u16 index for an adapter ID
    ///
    /// # Arguments
    /// * `adapter_id` - String identifier for the adapter
    ///
    /// # Returns
    /// The u16 index for this adapter, or None if not registered
    pub fn get_index(&self, adapter_id: &str) -> Option<u16> {
        let inner = self.inner.read();
        inner.id_to_index.get(adapter_id).copied()
    }

    /// Get the adapter ID for a u16 index
    ///
    /// # Arguments
    /// * `index` - u16 index
    ///
    /// # Returns
    /// The adapter ID for this index, or None if not registered
    pub fn get_id(&self, index: u16) -> Option<String> {
        let inner = self.inner.read();
        inner.index_to_id.get(&index).cloned()
    }

    /// Check if an adapter is registered
    ///
    /// # Arguments
    /// * `adapter_id` - String identifier for the adapter
    ///
    /// # Returns
    /// True if the adapter is registered
    pub fn contains(&self, adapter_id: &str) -> bool {
        let inner = self.inner.read();
        inner.id_to_index.contains_key(adapter_id)
    }

    /// Check if an index is allocated
    ///
    /// # Arguments
    /// * `index` - u16 index
    ///
    /// # Returns
    /// True if the index is allocated
    pub fn contains_index(&self, index: u16) -> bool {
        let inner = self.inner.read();
        inner.index_to_id.contains_key(&index)
    }

    /// Get the number of registered adapters
    pub fn len(&self) -> usize {
        let inner = self.inner.read();
        inner.id_to_index.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        let inner = self.inner.read();
        inner.id_to_index.is_empty()
    }

    /// Get all registered adapter IDs
    pub fn adapter_ids(&self) -> Vec<String> {
        let inner = self.inner.read();
        inner.id_to_index.keys().cloned().collect()
    }

    /// Get all registered adapter indices
    pub fn adapter_indices(&self) -> Vec<u16> {
        let inner = self.inner.read();
        inner.index_to_id.keys().copied().collect()
    }

    /// Get all registered adapter ID-index pairs
    pub fn entries(&self) -> Vec<(String, u16)> {
        let inner = self.inner.read();
        inner
            .id_to_index
            .iter()
            .map(|(id, &idx): (&String, &u16)| (id.clone(), idx))
            .collect()
    }

    /// Clear all registrations
    pub fn clear(&self) {
        let mut inner = self.inner.write();
        inner.id_to_index.clear();
        inner.index_to_id.clear();
        inner.free_indices.clear();
        inner.next_index = 0;
    }

    /// Get statistics about the registry
    pub fn stats(&self) -> RegistryStats {
        let inner = self.inner.read();
        RegistryStats {
            total_registered: inner.id_to_index.len(),
            free_indices_count: inner.free_indices.len(),
            next_index: inner.next_index,
            max_capacity: MAX_ADAPTERS,
        }
    }
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about the adapter registry
#[derive(Debug, Clone)]
pub struct RegistryStats {
    /// Number of currently registered adapters
    pub total_registered: usize,
    /// Number of free indices available for reuse
    pub free_indices_count: usize,
    /// Next index to be allocated (if no free indices)
    pub next_index: u16,
    /// Maximum capacity of the registry
    pub max_capacity: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_new() {
        let registry = AdapterRegistry::new();
        assert_eq!(registry.len(), 0);
        assert!(registry.is_empty());
    }

    #[test]
    fn test_registry_from_manifest() {
        let adapter_ids = vec![
            "adapter-1".to_string(),
            "adapter-2".to_string(),
            "adapter-3".to_string(),
        ];

        let registry = AdapterRegistry::from_manifest(&adapter_ids).unwrap();
        assert_eq!(registry.len(), 3);

        // Check deterministic index assignment
        assert_eq!(registry.get_index("adapter-1"), Some(0));
        assert_eq!(registry.get_index("adapter-2"), Some(1));
        assert_eq!(registry.get_index("adapter-3"), Some(2));
    }

    #[test]
    fn test_registry_register() {
        let registry = AdapterRegistry::new();

        let idx1 = registry.register("adapter-1".to_string()).unwrap();
        assert_eq!(idx1, 0);

        let idx2 = registry.register("adapter-2".to_string()).unwrap();
        assert_eq!(idx2, 1);

        // Re-registering returns same index
        let idx1_again = registry.register("adapter-1".to_string()).unwrap();
        assert_eq!(idx1_again, idx1);

        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn test_registry_unregister() {
        let registry = AdapterRegistry::new();

        let idx1 = registry.register("adapter-1".to_string()).unwrap();
        let idx2 = registry.register("adapter-2".to_string()).unwrap();

        assert_eq!(registry.len(), 2);

        // Unregister first adapter
        let freed_idx = registry.unregister("adapter-1").unwrap();
        assert_eq!(freed_idx, idx1);
        assert_eq!(registry.len(), 1);

        // Check it's gone
        assert!(!registry.contains("adapter-1"));
        assert!(registry.contains("adapter-2"));

        // Unregistering again should fail
        assert!(registry.unregister("adapter-1").is_err());
    }

    #[test]
    fn test_registry_index_reuse() {
        let registry = AdapterRegistry::new();

        // Register and unregister
        let idx1 = registry.register("adapter-1".to_string()).unwrap();
        registry.unregister("adapter-1").unwrap();

        // Next registration should reuse the freed index
        let idx2 = registry.register("adapter-2".to_string()).unwrap();
        assert_eq!(idx1, idx2);
    }

    #[test]
    fn test_registry_bidirectional_lookup() {
        let registry = AdapterRegistry::new();

        let idx = registry.register("test-adapter".to_string()).unwrap();

        // ID → index
        assert_eq!(registry.get_index("test-adapter"), Some(idx));

        // index → ID
        assert_eq!(registry.get_id(idx), Some("test-adapter".to_string()));

        // Non-existent lookups
        assert_eq!(registry.get_index("nonexistent"), None);
        assert_eq!(registry.get_id(999), None);
    }

    #[test]
    fn test_registry_entries() {
        let adapter_ids = vec![
            "adapter-1".to_string(),
            "adapter-2".to_string(),
            "adapter-3".to_string(),
        ];

        let registry = AdapterRegistry::from_manifest(&adapter_ids).unwrap();
        let entries = registry.entries();

        assert_eq!(entries.len(), 3);

        // Verify all mappings are present
        for (id, idx) in entries {
            assert_eq!(registry.get_index(&id), Some(idx));
            assert_eq!(registry.get_id(idx), Some(id));
        }
    }

    #[test]
    fn test_registry_clear() {
        let registry = AdapterRegistry::new();

        registry.register("adapter-1".to_string()).unwrap();
        registry.register("adapter-2".to_string()).unwrap();
        assert_eq!(registry.len(), 2);

        registry.clear();
        assert_eq!(registry.len(), 0);
        assert!(registry.is_empty());
    }

    #[test]
    fn test_registry_stats() {
        let registry = AdapterRegistry::new();

        let stats = registry.stats();
        assert_eq!(stats.total_registered, 0);
        assert_eq!(stats.free_indices_count, 0);
        assert_eq!(stats.next_index, 0);

        registry.register("adapter-1".to_string()).unwrap();
        registry.register("adapter-2".to_string()).unwrap();

        let stats = registry.stats();
        assert_eq!(stats.total_registered, 2);
        assert_eq!(stats.next_index, 2);

        registry.unregister("adapter-1").unwrap();

        let stats = registry.stats();
        assert_eq!(stats.total_registered, 1);
        assert_eq!(stats.free_indices_count, 1);
    }

    #[test]
    fn test_registry_thread_safety() {
        use std::sync::Arc;
        use std::thread;

        let registry = Arc::new(AdapterRegistry::new());
        let mut handles = vec![];

        // Spawn multiple threads to register adapters
        for i in 0..10 {
            let registry_clone = Arc::clone(&registry);
            let handle = thread::spawn(move || {
                for j in 0..10 {
                    let adapter_id = format!("adapter-{}-{}", i, j);
                    registry_clone.register(adapter_id).unwrap();
                }
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Should have 100 adapters registered
        assert_eq!(registry.len(), 100);
    }

    #[test]
    fn test_registry_max_adapters_error() {
        let too_many: Vec<String> = (0..=MAX_ADAPTERS)
            .map(|i| format!("adapter-{}", i))
            .collect();

        let result = AdapterRegistry::from_manifest(&too_many);
        assert!(result.is_err());
    }
}
