//! K reduction coordinator for lifecycle manager
//!
//! Implements the lifecycle manager's side of the K reduction protocol.
//! Evaluates K reduction requests from memory manager and determines
//! which adapters to unload based on activation percentages and state.

#![allow(unused_mut)]

use crate::{AdapterHeatRecord, AdapterHeatState};
use adapteros_memory::{KReductionRequest, KReductionResponse};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// K reduction coordinator for lifecycle manager
pub struct LifecycleKReductionCoordinator {
    /// Current K value
    current_k: usize,
    /// Minimum K value (never reduce below this)
    min_k: usize,
    /// Critical pressure threshold for approval
    critical_pressure_threshold: f32,
}

impl LifecycleKReductionCoordinator {
    /// Create a new lifecycle K reduction coordinator
    pub fn new(current_k: usize, min_k: usize, critical_pressure_threshold: f32) -> Self {
        Self {
            current_k,
            min_k,
            critical_pressure_threshold,
        }
    }

    /// Evaluate a K reduction request based on adapter states
    pub fn evaluate_request(
        &self,
        request: &KReductionRequest,
        adapter_states: &HashMap<u16, AdapterHeatRecord>,
    ) -> KReductionResponse {
        // Validate request
        if !request.is_valid() {
            return KReductionResponse::reject(
                request.request_id.clone(),
                self.current_k,
                "Invalid K reduction request: target >= current".to_string(),
            );
        }

        // Check minimum K threshold
        if request.target_k < self.min_k {
            return KReductionResponse::reject(
                request.request_id.clone(),
                self.current_k,
                format!(
                    "Target K ({}) below minimum K ({})",
                    request.target_k, self.min_k
                ),
            );
        }

        // Check if pressure is critical enough to justify K reduction
        if request.pressure_level < self.critical_pressure_threshold {
            return KReductionResponse::reject(
                request.request_id.clone(),
                self.current_k,
                format!(
                    "Pressure level {:.2} below critical threshold {:.2}",
                    request.pressure_level, self.critical_pressure_threshold
                ),
            );
        }

        // Select adapters to unload (lowest activation percentage first)
        let adapters_to_unload =
            self.select_adapters_for_unload(request.target_k, request.current_k, adapter_states);

        if adapters_to_unload.len() != request.current_k - request.target_k {
            return KReductionResponse::reject(
                request.request_id.clone(),
                self.current_k,
                format!(
                    "Could not find enough unloadable adapters: need {}, found {}",
                    request.current_k - request.target_k,
                    adapters_to_unload.len()
                ),
            );
        }

        // Estimate memory freed (assume ~1MB per adapter as conservative estimate)
        let num_to_unload = adapters_to_unload.len();
        let estimated_freed = (num_to_unload as u64) * 1024 * 1024;

        debug!(
            request_id = %request.request_id,
            current_k = request.current_k,
            target_k = request.target_k,
            adapters_to_unload = num_to_unload,
            estimated_freed = estimated_freed,
            "K reduction request approved"
        );

        KReductionResponse::approve(
            request.request_id.clone(),
            request.target_k,
            adapters_to_unload,
            estimated_freed,
            format!(
                "K reduction approved: {} -> {}, unloading {} adapters",
                request.current_k, request.target_k, num_to_unload
            ),
        )
    }

    /// Select adapters to unload based on activation counts
    /// Prioritizes unloading adapters with lowest activation count
    fn select_adapters_for_unload(
        &self,
        target_k: usize,
        current_k: usize,
        adapter_states: &HashMap<u16, AdapterHeatRecord>,
    ) -> Vec<u16> {
        let num_to_unload = current_k - target_k;

        // Collect all adapters with their activation counts
        let mut adapter_activations: Vec<(u16, u64, AdapterHeatState)> = adapter_states
            .iter()
            .map(|(idx, record)| (*idx, record.activation_count, record.state))
            .collect();

        // Sort by activation count (ascending) - unload least active first
        adapter_activations.sort_by(|a, b| a.1.cmp(&b.1));

        let mut unload_list = Vec::new();

        // Select adapters for unload, respecting pinned status
        for (idx, activation_pct, state) in &adapter_activations {
            if unload_list.len() >= num_to_unload {
                break;
            }

            // Skip pinned adapters
            if let Some(record) = adapter_states.get(idx) {
                if record.pinned {
                    debug!(adapter_id = idx, "Skipping pinned adapter in K reduction");
                    continue;
                }
            }

            // Skip adapters in critical states
            if matches!(state, AdapterHeatState::Resident) {
                debug!(adapter_id = idx, "Skipping resident adapter in K reduction");
                continue;
            }

            unload_list.push(*idx);
        }

        unload_list
    }

    /// Update current K value
    pub fn set_current_k(&mut self, k: usize) {
        self.current_k = k;
    }

    /// Get current K value
    pub fn get_current_k(&self) -> usize {
        self.current_k
    }

    /// Check if further K reduction is possible
    pub fn can_reduce_further(&self, proposed_k: usize) -> bool {
        proposed_k >= self.min_k
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coordinator_creation() {
        let coordinator = LifecycleKReductionCoordinator::new(10, 2, 0.70);
        assert_eq!(coordinator.get_current_k(), 10);
    }

    #[test]
    fn test_coordinator_rejects_invalid_request() {
        let coordinator = LifecycleKReductionCoordinator::new(10, 2, 0.70);
        let mut states = HashMap::new();

        let request = KReductionRequest::new(
            10, // target >= current
            8,
            0.85,
            1024 * 1024,
            10.0,
            "Invalid".to_string(),
        );

        let response = coordinator.evaluate_request(&request, &states);
        assert!(!response.approved);
    }

    #[test]
    fn test_coordinator_rejects_low_pressure() {
        let coordinator = LifecycleKReductionCoordinator::new(10, 2, 0.70);
        let mut states = HashMap::new();

        let request = KReductionRequest::new(
            8,
            10,
            0.50, // Below threshold
            1024 * 1024,
            20.0,
            "Low pressure".to_string(),
        );

        let response = coordinator.evaluate_request(&request, &states);
        assert!(!response.approved);
    }

    #[test]
    fn test_coordinator_approves_high_pressure() {
        let coordinator = LifecycleKReductionCoordinator::new(10, 2, 0.70);
        let mut states = HashMap::new();

        // Add some adapters to the states
        for i in 0..10 {
            let mut record = AdapterHeatRecord::new(format!("adapter_{}", i), i as u16);
            record.activation_count = i as u64; // 0, 1, 2, ...
            record.pinned = false; // Not pinned
            record.state = AdapterHeatState::Cold;
            states.insert(i as u16, record);
        }

        let request = KReductionRequest::new(
            8,
            10,
            0.85, // Above threshold
            1024 * 1024,
            10.0,
            "High pressure".to_string(),
        );

        let response = coordinator.evaluate_request(&request, &states);
        assert!(response.approved);
        assert_eq!(response.new_k, 8);
        assert_eq!(response.adapters_to_unload.len(), 2);
    }

    #[test]
    fn test_coordinator_respects_pinned_adapters() {
        let coordinator = LifecycleKReductionCoordinator::new(10, 2, 0.70);
        let mut states = HashMap::new();

        // Add adapters, pin the ones with lowest activation
        for i in 0..10 {
            let mut record = AdapterHeatRecord::new(format!("adapter_{}", i), i as u16);
            record.activation_count = i as u64;
            record.pinned = i < 2; // Pin first 2 adapters (lowest activation)
            record.state = AdapterHeatState::Cold;
            states.insert(i as u16, record);
        }

        let request = KReductionRequest::new(8, 10, 0.85, 1024 * 1024, 10.0, "Test".to_string());

        let response = coordinator.evaluate_request(&request, &states);
        assert!(response.approved);

        // Should not include pinned adapters in unload list
        let unload_set: std::collections::HashSet<_> =
            response.adapters_to_unload.iter().copied().collect();
        assert!(!unload_set.contains(&0));
        assert!(!unload_set.contains(&1));
    }

    #[test]
    fn test_coordinator_rejects_below_min_k() {
        let coordinator = LifecycleKReductionCoordinator::new(10, 2, 0.70);
        let states = HashMap::new();

        let request = KReductionRequest::new(
            1, // Below min_k of 2
            10,
            0.85,
            1024 * 1024,
            10.0,
            "Below min K".to_string(),
        );

        let response = coordinator.evaluate_request(&request, &states);
        assert!(!response.approved);
    }
}
