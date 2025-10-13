//! Adapter state machine
//!
//! State transitions:
//! Unloaded → Cold → Warm → Hot → Resident
//!            ↑______|______|_____|

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Eviction priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EvictionPriority {
    Never,
    Low,
    Normal,
    High,
    Critical,
}

impl EvictionPriority {
    /// Get numeric priority (higher = more likely to evict)
    pub fn numeric_value(&self) -> u8 {
        match self {
            Self::Never => 0,
            Self::Low => 1,
            Self::Normal => 2,
            Self::High => 3,
            Self::Critical => 4,
        }
    }
}

/// Adapter lifecycle state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AdapterState {
    /// Not in memory, metadata only
    Unloaded,
    /// Weights loaded, not in active rotation
    Cold,
    /// In rotation pool, occasionally selected
    Warm,
    /// Frequently selected, prioritized
    Hot,
    /// Always active (pinned adapters)
    Resident,
}

impl AdapterState {
    /// Get the next higher state
    pub fn promote(&self) -> Option<Self> {
        match self {
            Self::Unloaded => Some(Self::Cold),
            Self::Cold => Some(Self::Warm),
            Self::Warm => Some(Self::Hot),
            Self::Hot => Some(Self::Resident),
            Self::Resident => None, // Already at top
        }
    }

    /// Get the next lower state
    pub fn demote(&self) -> Option<Self> {
        match self {
            Self::Unloaded => None, // Already at bottom
            Self::Cold => Some(Self::Unloaded),
            Self::Warm => Some(Self::Cold),
            Self::Hot => Some(Self::Warm),
            Self::Resident => Some(Self::Hot),
        }
    }

    /// Check if adapter is loaded in memory
    pub fn is_loaded(&self) -> bool {
        !matches!(self, Self::Unloaded)
    }

    /// Check if adapter is available for routing
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Warm | Self::Hot | Self::Resident)
    }

    /// Check if adapter is pinned (resident)
    pub fn is_pinned(&self) -> bool {
        matches!(self, Self::Resident)
    }

    /// Get priority boost for routing (higher for better states)
    pub fn priority_boost(&self) -> f32 {
        match self {
            Self::Unloaded => 0.0,
            Self::Cold => 0.0,
            Self::Warm => 0.1,
            Self::Hot => 0.3,
            Self::Resident => 0.5,
        }
    }

    /// Category-specific promotion rules
    pub fn can_promote(&self, category: &str) -> bool {
        match (self, category) {
            // Code adapters promote quickly
            (Self::Cold, "code") => true,
            (Self::Warm, "code") => true,

            // Framework adapters need more usage
            (Self::Cold, "framework") => false,
            (Self::Warm, "framework") => true,

            // Codebase adapters promote based on tenant usage
            (Self::Cold, "codebase") => false,
            (Self::Warm, "codebase") => true,

            // Ephemeral adapters stay low-rank
            (_, "ephemeral") => false,

            _ => true,
        }
    }

    /// Category-specific demotion rules
    pub fn should_demote(&self, category: &str, last_used: Duration) -> bool {
        match (self, category) {
            // Code adapters rarely demote
            (Self::Hot, "code") => last_used > Duration::from_secs(24 * 3600),

            // Framework adapters demote moderately
            (Self::Hot, "framework") => last_used > Duration::from_secs(12 * 3600),
            (Self::Warm, "framework") => last_used > Duration::from_secs(6 * 3600),

            // Codebase adapters demote quickly
            (Self::Hot, "codebase") => last_used > Duration::from_secs(4 * 3600),
            (Self::Warm, "codebase") => last_used > Duration::from_secs(2 * 3600),

            // Ephemeral adapters demote immediately on TTL
            (_, "ephemeral") => true,

            _ => false,
        }
    }

    /// Get eviction priority based on category and state
    pub fn eviction_priority(&self, category: &str) -> EvictionPriority {
        match (self, category) {
            // Resident adapters are never evicted
            (Self::Resident, _) => EvictionPriority::Never,

            // Ephemeral adapters have highest eviction priority
            (_, "ephemeral") => EvictionPriority::Critical,

            // Codebase adapters evict quickly
            (Self::Cold, "codebase") => EvictionPriority::High,
            (Self::Warm, "codebase") => EvictionPriority::High,

            // Framework adapters have medium priority
            (Self::Cold, "framework") => EvictionPriority::Normal,
            (Self::Warm, "framework") => EvictionPriority::Normal,

            // Code adapters have lowest eviction priority
            (Self::Cold, "code") => EvictionPriority::Low,
            (Self::Warm, "code") => EvictionPriority::Low,

            _ => EvictionPriority::Normal,
        }
    }
}

impl std::fmt::Display for AdapterState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unloaded => write!(f, "unloaded"),
            Self::Cold => write!(f, "cold"),
            Self::Warm => write!(f, "warm"),
            Self::Hot => write!(f, "hot"),
            Self::Resident => write!(f, "resident"),
        }
    }
}

/// Adapter state record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterStateRecord {
    pub adapter_id: String,
    pub adapter_idx: u16,
    pub state: AdapterState,
    pub pinned: bool,
    pub memory_bytes: usize,
    pub category: String,
    pub scope: String,
    pub last_activated: Option<std::time::SystemTime>,
    pub activation_count: u64,
}

impl AdapterStateRecord {
    pub fn new(adapter_id: String, adapter_idx: u16) -> Self {
        Self {
            adapter_id,
            adapter_idx,
            state: AdapterState::Unloaded,
            pinned: false,
            memory_bytes: 0,
            category: "code".to_string(),
            scope: "global".to_string(),
            last_activated: None,
            activation_count: 0,
        }
    }

    pub fn new_with_category(
        adapter_id: String,
        adapter_idx: u16,
        category: String,
        scope: String,
    ) -> Self {
        Self {
            adapter_id,
            adapter_idx,
            state: AdapterState::Unloaded,
            pinned: false,
            memory_bytes: 0,
            category,
            scope,
            last_activated: None,
            activation_count: 0,
        }
    }

    /// Promote to next state with category awareness
    pub fn promote(&mut self) -> bool {
        if self.state.can_promote(&self.category) {
            if let Some(new_state) = self.state.promote() {
                self.state = new_state;
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Demote to previous state with category awareness
    pub fn demote(&mut self) -> bool {
        if self.pinned {
            return false; // Cannot demote pinned adapters
        }

        // Check if we should demote based on last activation time
        if let Some(last_activated) = self.last_activated {
            let time_since_activation = last_activated.elapsed().unwrap_or(Duration::from_secs(0));
            if !self
                .state
                .should_demote(&self.category, time_since_activation)
            {
                return false;
            }
        }

        if let Some(new_state) = self.state.demote() {
            self.state = new_state;
            true
        } else {
            false
        }
    }

    /// Pin adapter to resident state
    pub fn pin(&mut self) {
        self.pinned = true;
        self.state = AdapterState::Resident;
    }

    /// Unpin adapter
    pub fn unpin(&mut self) {
        self.pinned = false;
        // Don't change state when unpinning
    }

    /// Record activation
    pub fn record_activation(&mut self) {
        self.last_activated = Some(std::time::SystemTime::now());
        self.activation_count += 1;
    }

    /// Get eviction priority
    pub fn eviction_priority(&self) -> EvictionPriority {
        self.state.eviction_priority(&self.category)
    }

    /// Check if adapter should be evicted based on memory pressure
    pub fn should_evict(&self, memory_pressure: f32) -> bool {
        if self.pinned {
            return false;
        }

        let priority = self.eviction_priority();
        match priority {
            EvictionPriority::Never => false,
            EvictionPriority::Low => memory_pressure > 0.9,
            EvictionPriority::Normal => memory_pressure > 0.8,
            EvictionPriority::High => memory_pressure > 0.7,
            EvictionPriority::Critical => memory_pressure > 0.5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_transitions() {
        let mut state = AdapterState::Unloaded;

        // Test promotions
        assert_eq!(state.promote(), Some(AdapterState::Cold));
        state = state
            .promote()
            .expect("Test state promotion should succeed");
        assert_eq!(state, AdapterState::Cold);

        state = state
            .promote()
            .expect("Test state promotion should succeed");
        assert_eq!(state, AdapterState::Warm);

        state = state
            .promote()
            .expect("Test state promotion should succeed");
        assert_eq!(state, AdapterState::Hot);

        state = state
            .promote()
            .expect("Test state promotion should succeed");
        assert_eq!(state, AdapterState::Resident);

        assert_eq!(state.promote(), None); // At top

        // Test demotions
        state = state.demote().expect("Test state demotion should succeed");
        assert_eq!(state, AdapterState::Hot);

        state = state.demote().expect("Test state demotion should succeed");
        assert_eq!(state, AdapterState::Warm);

        state = state.demote().expect("Test state demotion should succeed");
        assert_eq!(state, AdapterState::Cold);

        state = state.demote().expect("Test state demotion should succeed");
        assert_eq!(state, AdapterState::Unloaded);

        assert_eq!(state.demote(), None); // At bottom
    }

    #[test]
    fn test_state_properties() {
        assert!(!AdapterState::Unloaded.is_loaded());
        assert!(AdapterState::Cold.is_loaded());
        assert!(AdapterState::Warm.is_loaded());

        assert!(!AdapterState::Unloaded.is_available());
        assert!(!AdapterState::Cold.is_available());
        assert!(AdapterState::Warm.is_available());
        assert!(AdapterState::Hot.is_available());
        assert!(AdapterState::Resident.is_available());

        assert!(!AdapterState::Hot.is_pinned());
        assert!(AdapterState::Resident.is_pinned());
    }

    #[test]
    fn test_pinned_adapter() {
        let mut record = AdapterStateRecord::new("test".to_string(), 0);

        record.pin();
        assert_eq!(record.state, AdapterState::Resident);
        assert!(record.pinned);

        // Cannot demote pinned adapter
        assert!(!record.demote());
        assert_eq!(record.state, AdapterState::Resident);

        record.unpin();
        assert!(!record.pinned);
        // State remains Resident after unpinning
        assert_eq!(record.state, AdapterState::Resident);

        // Now can demote
        assert!(record.demote());
        assert_eq!(record.state, AdapterState::Hot);
    }
}
