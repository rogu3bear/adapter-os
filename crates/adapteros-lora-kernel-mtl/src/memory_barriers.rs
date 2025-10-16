//! Advanced Metal 3.x memory barrier planner
//!
//! The planner keeps track of resource access patterns and computes the
//! minimum set of barriers required to maintain deterministic execution.

use std::collections::HashMap;

/// Type of resource access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessType {
    Read,
    Write,
}

/// Scope in which a barrier must be applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarrierScope {
    Threadgroup,
    Grid,
    Device,
}

/// Planned barrier action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarrierAction {
    pub resource_id: String,
    pub required_scope: BarrierScope,
    pub previous_access: AccessType,
    pub next_access: AccessType,
}

#[derive(Debug, Default)]
struct AccessRecord {
    last_access: Option<AccessType>,
    reads: u64,
    writes: u64,
}

/// Planner responsible for computing Metal barrier requirements.
#[derive(Debug, Default)]
pub struct MemoryBarrierPlanner {
    resources: HashMap<String, AccessRecord>,
    pending: Vec<BarrierAction>,
}

impl MemoryBarrierPlanner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a read access for a resource. If the last access was a
    /// write, a threadgroup barrier is planned.
    pub fn record_read(&mut self, resource_id: impl Into<String>) {
        self.record_access(resource_id.into(), AccessType::Read);
    }

    /// Record a write access. If the last access was also a write we plan
    /// a device-wide barrier. If it was a read we plan a threadgroup
    /// barrier to ensure coherence across the workgroup.
    pub fn record_write(&mut self, resource_id: impl Into<String>) {
        self.record_access(resource_id.into(), AccessType::Write);
    }

    fn record_access(&mut self, resource_id: String, access: AccessType) {
        let entry = self.resources.entry(resource_id.clone()).or_default();
        let required_scope = match (entry.last_access, access) {
            (Some(AccessType::Write), AccessType::Read) => Some(BarrierScope::Threadgroup),
            (Some(AccessType::Read), AccessType::Write) => Some(BarrierScope::Threadgroup),
            (Some(AccessType::Write), AccessType::Write) => Some(BarrierScope::Device),
            (Some(AccessType::Read), AccessType::Read) => None,
            (None, _) => None,
        };

        if let Some(scope) = required_scope {
            self.pending.push(BarrierAction {
                resource_id: resource_id.clone(),
                required_scope: scope,
                previous_access: entry
                    .last_access
                    .expect("`required_scope` implies previous access"),
                next_access: access,
            });
        }

        match access {
            AccessType::Read => entry.reads += 1,
            AccessType::Write => entry.writes += 1,
        }

        entry.last_access = Some(access);
    }

    /// Take the pending barrier actions.
    pub fn take_pending_actions(&mut self) -> Vec<BarrierAction> {
        std::mem::take(&mut self.pending)
    }

    /// Retrieve accumulated statistics for a resource.
    pub fn stats(&self, resource_id: &str) -> Option<ResourceStats> {
        self.resources.get(resource_id).map(|record| ResourceStats {
            reads: record.reads,
            writes: record.writes,
            last_access: record.last_access,
        })
    }

    /// Reset all tracked state.
    pub fn reset(&mut self) {
        self.resources.clear();
        self.pending.clear();
    }
}

/// Statistics about a tracked resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceStats {
    pub reads: u64,
    pub writes: u64,
    pub last_access: Option<AccessType>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planner_emits_barrier_on_write_then_read() {
        let mut planner = MemoryBarrierPlanner::new();
        planner.record_write("buffer");
        planner.record_read("buffer");

        let actions = planner.take_pending_actions();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].required_scope, BarrierScope::Threadgroup);
        assert_eq!(actions[0].previous_access, AccessType::Write);
        assert_eq!(actions[0].next_access, AccessType::Read);
    }

    #[test]
    fn planner_records_stats() {
        let mut planner = MemoryBarrierPlanner::new();
        planner.record_read("buffer");
        planner.record_write("buffer");
        planner.record_write("buffer");
        planner.record_read("other");

        let stats = planner.stats("buffer").unwrap();
        assert_eq!(stats.reads, 1);
        assert_eq!(stats.writes, 2);
        assert_eq!(stats.last_access, Some(AccessType::Write));

        let actions = planner.take_pending_actions();
        assert_eq!(actions.len(), 2); // read->write and write->write
        assert!(actions
            .iter()
            .any(|action| action.required_scope == BarrierScope::Device));
    }
}
