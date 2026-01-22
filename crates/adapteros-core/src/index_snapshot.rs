use crate::B3Hash;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IndexSnapshot {
    AdapterGraph(Vec<GraphNode>),                   // Sorted by ID
    AdapterStacks(Vec<String>),                     // Changed to String IDs
    RouterTable(BTreeMap<String, f32>),             // Assume RouterPrior is f32
    TelemetrySecondary(BTreeMap<String, Vec<u64>>), // EventId as u64
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub edges: Vec<String>, // Sorted
                            // ...
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouterPrior {
    pub adapter_id: String,
    pub weight: f64,
    // ...
}

// Similar for others...

impl IndexSnapshot {
    /// Compute a deterministic hash of this snapshot.
    ///
    /// Uses JSON serialization with BTreeMap for consistent key ordering.
    /// Falls back to an empty hash if serialization fails (should never happen
    /// for these simple types, but we avoid panicking in production).
    pub fn compute_hash(&self) -> B3Hash {
        match serde_json::to_string(self) {
            Ok(json) => B3Hash::hash(json.as_bytes()),
            Err(e) => {
                tracing::error!(error = %e, "IndexSnapshot serialization failed, using empty hash");
                B3Hash::hash(b"serialization_failed")
            }
        }
    }

    pub fn from_tenant_data(/* db query */) -> Self {
        // Placeholder: query and build canonical
        // e.g., for AdapterGraph: sort nodes/edges
        IndexSnapshot::AdapterGraph(vec![])
    }
}
