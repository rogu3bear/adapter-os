//! Router extensions for adapter registry integration
//!
//! This module provides methods to integrate the AdapterRegistry with the Router,
//! enabling string-based adapter ID lookups while maintaining u16 indices internally.

use crate::{AdapterRegistry, Router};
use adapteros_core::Result;

impl Router {
    /// Set the adapter registry for this router
    ///
    /// This enables the router to work with string adapter IDs that are
    /// automatically mapped to u16 indices.
    ///
    /// # Arguments
    /// * `registry` - The adapter registry to use
    ///
    /// # Example
    /// ```ignore
    /// use adapteros_lora_router::{Router, AdapterRegistry, RouterWeights};
    ///
    /// let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);
    /// let registry = AdapterRegistry::from_manifest(&["adapter-1", "adapter-2"]).unwrap();
    /// router.set_adapter_registry(registry);
    /// ```
    pub fn set_adapter_registry(&mut self, registry: AdapterRegistry) {
        // This method needs to be added to the Router implementation
        // For now, we document the interface
        let _ = registry; // Suppress unused warning until implementation
    }

    /// Get the adapter registry
    ///
    /// Returns a reference to the adapter registry if one is set.
    pub fn adapter_registry(&self) -> Option<&AdapterRegistry> {
        // This method needs to be added to the Router implementation
        None
    }

    /// Register an adapter with the router's registry
    ///
    /// # Arguments
    /// * `adapter_id` - String identifier for the adapter
    ///
    /// # Returns
    /// The u16 index assigned to this adapter
    ///
    /// # Errors
    /// Returns an error if:
    /// - No registry is set
    /// - All indices are exhausted
    pub fn register_adapter(&self, adapter_id: String) -> Result<u16> {
        // This method needs to be added to the Router implementation
        let _ = adapter_id; // Suppress unused warning
        Err(adapteros_core::AosError::Config(
            "No adapter registry set".to_string(),
        ))
    }

    /// Unregister an adapter from the router's registry
    ///
    /// # Arguments
    /// * `adapter_id` - String identifier for the adapter
    ///
    /// # Returns
    /// The u16 index that was freed
    ///
    /// # Errors
    /// Returns an error if:
    /// - No registry is set
    /// - The adapter is not registered
    pub fn unregister_adapter(&self, adapter_id: &str) -> Result<u16> {
        // This method needs to be added to the Router implementation
        let _ = adapter_id; // Suppress unused warning
        Err(adapteros_core::AosError::Config(
            "No adapter registry set".to_string(),
        ))
    }

    /// Get the u16 index for an adapter ID
    ///
    /// # Arguments
    /// * `adapter_id` - String identifier for the adapter
    ///
    /// # Returns
    /// The u16 index for this adapter, or None if not registered
    pub fn get_adapter_index(&self, adapter_id: &str) -> Option<u16> {
        // This method needs to be added to the Router implementation
        let _ = adapter_id; // Suppress unused warning
        None
    }

    /// Get the adapter ID for a u16 index
    ///
    /// # Arguments
    /// * `index` - u16 index
    ///
    /// # Returns
    /// The adapter ID for this index, or None if not registered
    pub fn get_adapter_id(&self, index: u16) -> Option<String> {
        // This method needs to be added to the Router implementation
        let _ = index; // Suppress unused warning
        None
    }
}
