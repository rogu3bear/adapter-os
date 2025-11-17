use crate::tenant_snapshot::{EventId, StackInfo};
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
    pub fn compute_hash(&self) -> B3Hash {
        let json = serde_json::to_string(self).expect("Serialization failed"); // BTreeMap/Vectors sorted
        B3Hash::hash(json.as_bytes())
    }

    pub fn from_tenant_data(/* db query */) -> Self {
        // Placeholder: query and build canonical
        // e.g., for AdapterGraph: sort nodes/edges
        IndexSnapshot::AdapterGraph(vec![])
    }
}
