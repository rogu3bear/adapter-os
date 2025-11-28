//! Adapter state machine
//!
//! State transitions:
//! Unloaded → Cold → Warm → Hot → Resident
//!            ↑______|______|_____|

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Acquisition state for model download/cache tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum AcquisitionState {
    #[default]
    NotCached,
    Queued,
    Downloading {
        progress_pct: u8,
    },
    Verifying,
    Available,
    Failed,
}

impl AcquisitionState {
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available)
    }

    pub fn is_downloading(&self) -> bool {
        matches!(self, Self::Downloading { .. })
    }

    pub fn progress_pct(&self) -> Option<u8> {
        if let Self::Downloading { progress_pct } = self {
            Some(*progress_pct)
        } else {
            None
        }
    }
}

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

/// Memory allocation tiers for eviction decisions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AllocationTier {
    /// Extra capacity - least critical for eviction
    Extra,
    /// Critical capacity - most critical for eviction
    Critical,
}

impl From<AdapterState> for AllocationTier {
    fn from(state: AdapterState) -> Self {
        match state {
            AdapterState::Unloaded => AllocationTier::Extra,
            AdapterState::Cold => AllocationTier::Extra,
            AdapterState::Warm => AllocationTier::Extra,
            AdapterState::Hot => AllocationTier::Critical,
            AdapterState::Resident => AllocationTier::Critical,
        }
    }
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

    /// FIX 4: Compare-and-swap (CAS) operation for state transitions
    /// Verify expected state before transition to prevent concurrent load/unload races
    ///
    /// Returns Ok(new_state) if transition succeeded, Err(current_state) if CAS failed
    pub fn cas_promote(&self, expected: AdapterState) -> std::result::Result<Self, AdapterState> {
        if *self != expected {
            return Err(*self);
        }
        self.promote().ok_or(*self)
    }

    /// FIX 4: Compare-and-swap (CAS) operation for demotion
    /// Verify expected state before transition to prevent concurrent load/unload races
    ///
    /// Returns Ok(new_state) if transition succeeded, Err(current_state) if CAS failed
    pub fn cas_demote(&self, expected: AdapterState) -> std::result::Result<Self, AdapterState> {
        if *self != expected {
            return Err(*self);
        }
        self.demote().ok_or(*self)
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
    /// Optional parent adapter ID for lineage stacking
    #[serde(default)]
    pub parent_adapter_id: Option<String>,
    /// Marks adapter as safety layer (used for Safe Mode routing)
    #[serde(default)]
    pub is_safety_adapter: bool,
    /// Domain tags for routing (e.g., "code", "vision", "finance")
    #[serde(default)]
    pub domains: Vec<String>,
    /// Acquisition state for model download/cache tracking
    #[serde(default)]
    pub acquisition_state: AcquisitionState,
    /// Download progress percentage (0-100)
    #[serde(default)]
    pub download_progress_pct: Option<u8>,
    /// Local filesystem path to cached model
    #[serde(default)]
    pub local_path: Option<PathBuf>,
    /// HuggingFace repo ID (e.g., "meta-llama/Llama-2-7b")
    #[serde(default)]
    pub repo_id: Option<String>,
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
            parent_adapter_id: None,
            is_safety_adapter: false,
            domains: Vec::new(),
            acquisition_state: AcquisitionState::default(),
            download_progress_pct: None,
            local_path: None,
            repo_id: None,
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
            parent_adapter_id: None,
            is_safety_adapter: false,
            domains: Vec::new(),
            acquisition_state: AcquisitionState::default(),
            download_progress_pct: None,
            local_path: None,
            repo_id: None,
        }
    }

    pub fn new_with_metadata(
        adapter_id: String,
        adapter_idx: u16,
        category: String,
        scope: String,
        parent_adapter_id: Option<String>,
        is_safety_adapter: bool,
        domains: Vec<String>,
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
            parent_adapter_id,
            is_safety_adapter,
            domains,
            acquisition_state: AcquisitionState::default(),
            download_progress_pct: None,
            local_path: None,
            repo_id: None,
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

    /// FIX 4: CAS-based promote with expected state verification
    /// Returns Ok(true) if promoted, Ok(false) if can't promote, Err if state changed
    pub fn cas_promote(
        &mut self,
        expected: AdapterState,
    ) -> std::result::Result<bool, AdapterState> {
        if self.state != expected {
            return Err(self.state);
        }
        if self.state.can_promote(&self.category) {
            if let Some(new_state) = self.state.promote() {
                self.state = new_state;
                Ok(true)
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
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

    /// FIX 4: CAS-based demote with expected state verification
    /// Returns Ok(true) if demoted, Ok(false) if can't demote, Err if state changed
    pub fn cas_demote(
        &mut self,
        expected: AdapterState,
    ) -> std::result::Result<bool, AdapterState> {
        if self.state != expected {
            return Err(self.state);
        }
        if self.pinned {
            return Ok(false); // Cannot demote pinned adapters
        }

        // Check if we should demote based on last activation time
        if let Some(last_activated) = self.last_activated {
            let time_since_activation = last_activated.elapsed().unwrap_or(Duration::from_secs(0));
            if !self
                .state
                .should_demote(&self.category, time_since_activation)
            {
                return Ok(false);
            }
        }

        if let Some(new_state) = self.state.demote() {
            self.state = new_state;
            Ok(true)
        } else {
            Ok(false)
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
