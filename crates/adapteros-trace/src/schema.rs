//! Event schema definitions for AdapterOS trace system

use std::collections::HashMap;

use adapteros_core::B3Hash;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// Re-export canonical TraceBundleMetadata from adapteros-telemetry-types
pub use adapteros_telemetry_types::TraceBundleMetadata as BundleMetadata;

/// Core event schema for AdapterOS traces
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Unique event identifier
    pub event_id: Uuid,
    /// Logical tick counter for deterministic ordering
    pub tick_id: u64,
    /// Operation identifier
    pub op_id: String,
    /// Event type (e.g., "inference.start", "kernel.execute", "adapter.load")
    pub event_type: String,
    /// Input data for the operation
    pub inputs: HashMap<String, serde_json::Value>,
    /// Output data from the operation
    pub outputs: HashMap<String, serde_json::Value>,
    /// Interval identifier for fused weight spans (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval_id: Option<String>,
    /// Hash of the fused weights applied in this interval (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fused_weight_hash: Option<B3Hash>,
    /// BLAKE3 hash of the event data
    pub blake3_hash: B3Hash,
    /// Additional metadata
    pub metadata: EventMetadata,
    /// Timestamp for the event
    pub timestamp: u128,
}

/// Event metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMetadata {
    /// Global seed used for deterministic execution
    pub global_seed: B3Hash,
    /// Plan ID for the inference run
    pub plan_id: String,
    /// CPID (Control Plane ID) for the run
    pub cpid: String,
    /// Tenant ID
    pub tenant_id: String,
    /// Session ID
    pub session_id: String,
    /// Adapter IDs involved in this event
    pub adapter_ids: Vec<String>,
    /// Memory usage at time of event
    pub memory_usage_mb: u64,
    /// GPU utilization percentage
    pub gpu_utilization_pct: f32,
    /// Additional custom metadata
    pub custom: HashMap<String, serde_json::Value>,
}

impl Event {
    /// Create a new event
    pub fn new(
        tick_id: u64,
        op_id: String,
        event_type: String,
        inputs: HashMap<String, serde_json::Value>,
        outputs: HashMap<String, serde_json::Value>,
        metadata: EventMetadata,
    ) -> Self {
        let event_id = Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext));
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_nanos();

        // Compute hash of the event data
        let event_data = EventData {
            event_id,
            tick_id,
            op_id: op_id.clone(),
            event_type: event_type.clone(),
            inputs: inputs.clone(),
            outputs: outputs.clone(),
            interval_id: None,
            fused_weight_hash: None,
            metadata: metadata.clone(),
            timestamp,
        };

        let blake3_hash = event_data.compute_hash();

        Self {
            event_id,
            tick_id,
            op_id,
            event_type,
            inputs,
            outputs,
            interval_id: None,
            fused_weight_hash: None,
            blake3_hash,
            metadata,
            timestamp,
        }
    }

    /// Compute the hash of this event
    pub fn compute_hash(&self) -> B3Hash {
        let event_data = EventData {
            event_id: self.event_id,
            tick_id: self.tick_id,
            op_id: self.op_id.clone(),
            event_type: self.event_type.clone(),
            inputs: self.inputs.clone(),
            outputs: self.outputs.clone(),
            interval_id: self.interval_id.clone(),
            fused_weight_hash: self.fused_weight_hash,
            metadata: self.metadata.clone(),
            timestamp: self.timestamp,
        };

        event_data.compute_hash()
    }

    /// Verify the event's hash
    pub fn verify_hash(&self) -> bool {
        self.compute_hash() == self.blake3_hash
    }

    /// Attach fusion interval metadata and recompute the hash.
    pub fn with_interval(
        mut self,
        interval_id: Option<String>,
        fused_weight_hash: Option<B3Hash>,
    ) -> Self {
        self.interval_id = interval_id;
        self.fused_weight_hash = fused_weight_hash;
        self.blake3_hash = self.compute_hash();
        self
    }
}

/// Event data for hashing (excludes the hash field itself)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct EventData {
    pub event_id: Uuid,
    pub tick_id: u64,
    pub op_id: String,
    pub event_type: String,
    pub inputs: HashMap<String, serde_json::Value>,
    pub outputs: HashMap<String, serde_json::Value>,
    pub interval_id: Option<String>,
    pub fused_weight_hash: Option<B3Hash>,
    pub metadata: EventMetadata,
    pub timestamp: u128,
}

impl EventData {
    /// Compute BLAKE3 hash of the event data
    fn compute_hash(&self) -> B3Hash {
        // Serialize to canonical JSON for deterministic hashing
        let canonical_bytes =
            serde_jcs::to_vec(self).expect("Failed to serialize event data to canonical JSON");

        B3Hash::hash(&canonical_bytes)
    }
}

/// Trace bundle containing multiple events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceBundle {
    /// Bundle identifier
    pub bundle_id: Uuid,
    /// Bundle version
    pub version: u32,
    /// Global seed for the trace
    pub global_seed: B3Hash,
    /// Plan ID
    pub plan_id: String,
    /// CPID
    pub cpid: String,
    /// Tenant ID
    pub tenant_id: String,
    /// Session ID
    pub session_id: String,
    /// Events in this bundle
    pub events: Vec<Event>,
    /// Bundle metadata
    pub metadata: BundleMetadata,
    /// Bundle hash
    pub bundle_hash: B3Hash,
}

impl TraceBundle {
    /// Create a new trace bundle
    pub fn new(
        global_seed: B3Hash,
        plan_id: String,
        cpid: String,
        tenant_id: String,
        session_id: String,
    ) -> Self {
        let bundle_id = Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext));
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_nanos();

        let metadata = BundleMetadata {
            created_at,
            event_count: 0,
            total_size_bytes: 0,
            compression: "none".to_string(),
            signature: None,
            custom: HashMap::new(),
        };

        Self {
            bundle_id,
            version: 1,
            global_seed,
            plan_id,
            cpid,
            tenant_id,
            session_id,
            events: Vec::new(),
            metadata,
            bundle_hash: B3Hash::hash(b"empty"),
        }
    }

    /// Add an event to the bundle
    pub fn add_event(&mut self, event: Event) {
        self.events.push(event);
        self.metadata.event_count = self.events.len();
        self.update_bundle_hash();
    }

    /// Update the bundle hash
    fn update_bundle_hash(&mut self) {
        let bundle_data = BundleData {
            bundle_id: self.bundle_id,
            version: self.version,
            global_seed: self.global_seed,
            plan_id: self.plan_id.clone(),
            cpid: self.cpid.clone(),
            tenant_id: self.tenant_id.clone(),
            session_id: self.session_id.clone(),
            events: self.events.clone(),
            metadata: self.metadata.clone(),
        };

        self.bundle_hash = bundle_data.compute_hash();
    }

    /// Verify the bundle hash
    pub fn verify_hash(&self) -> bool {
        let bundle_data = BundleData {
            bundle_id: self.bundle_id,
            version: self.version,
            global_seed: self.global_seed,
            plan_id: self.plan_id.clone(),
            cpid: self.cpid.clone(),
            tenant_id: self.tenant_id.clone(),
            session_id: self.session_id.clone(),
            events: self.events.clone(),
            metadata: self.metadata.clone(),
        };

        bundle_data.compute_hash() == self.bundle_hash
    }

    /// Get events by type
    pub fn get_events_by_type(&self, event_type: &str) -> Vec<&Event> {
        self.events
            .iter()
            .filter(|e| e.event_type == event_type)
            .collect()
    }

    /// Get events by operation ID
    pub fn get_events_by_op_id(&self, op_id: &str) -> Vec<&Event> {
        self.events.iter().filter(|e| e.op_id == op_id).collect()
    }

    /// Get events in tick order
    pub fn get_events_by_tick(&self) -> Vec<&Event> {
        let mut events: Vec<&Event> = self.events.iter().collect();
        events.sort_by_key(|e| e.tick_id);
        events
    }
}

/// Bundle data for hashing (excludes the hash field itself)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BundleData {
    pub bundle_id: Uuid,
    pub version: u32,
    pub global_seed: B3Hash,
    pub plan_id: String,
    pub cpid: String,
    pub tenant_id: String,
    pub session_id: String,
    pub events: Vec<Event>,
    pub metadata: BundleMetadata,
}

impl BundleData {
    /// Compute BLAKE3 hash of the bundle data
    fn compute_hash(&self) -> B3Hash {
        // Serialize to canonical JSON for deterministic hashing
        let canonical_bytes =
            serde_jcs::to_vec(self).expect("Failed to serialize bundle data to canonical JSON");

        B3Hash::hash(&canonical_bytes)
    }
}

/// Event types used in AdapterOS traces
pub mod event_types {
    /// Inference start event
    pub const INFERENCE_START: &str = "inference.start";
    /// Inference end event
    pub const INFERENCE_END: &str = "inference.end";
    /// Token generation event
    pub const TOKEN_GENERATED: &str = "inference.token";
    /// Kernel execution event
    pub const KERNEL_EXECUTE: &str = "kernel.execute";
    /// Adapter load event
    pub const ADAPTER_LOAD: &str = "adapter.load";
    /// Adapter unload event
    pub const ADAPTER_UNLOAD: &str = "adapter.unload";
    /// Router decision event
    pub const ROUTER_DECISION: &str = "router.decision";
    /// Memory allocation event
    pub const MEMORY_ALLOC: &str = "memory.alloc";
    /// Memory deallocation event
    pub const MEMORY_DEALLOC: &str = "memory.dealloc";
    /// Policy check event
    pub const POLICY_CHECK: &str = "policy.check";
    /// Telemetry event
    pub const TELEMETRY: &str = "telemetry";
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn create_test_metadata() -> EventMetadata {
        EventMetadata {
            global_seed: B3Hash::hash(b"test_seed"),
            plan_id: "test_plan".to_string(),
            cpid: "test_cpid".to_string(),
            tenant_id: "test_tenant".to_string(),
            session_id: "test_session".to_string(),
            adapter_ids: vec!["adapter_1".to_string()],
            memory_usage_mb: 1024,
            gpu_utilization_pct: 50.0,
            custom: HashMap::new(),
        }
    }

    #[test]
    fn test_event_creation() {
        let metadata = create_test_metadata();
        let inputs = HashMap::new();
        let outputs = HashMap::new();

        let event = Event::new(
            1,
            "test_op".to_string(),
            "test_event".to_string(),
            inputs,
            outputs,
            metadata,
        );

        assert_eq!(event.tick_id, 1);
        assert_eq!(event.op_id, "test_op");
        assert_eq!(event.event_type, "test_event");
    }

    #[test]
    fn test_event_hash_verification() {
        let metadata = create_test_metadata();
        let inputs = HashMap::new();
        let outputs = HashMap::new();

        let event = Event::new(
            1,
            "test_op".to_string(),
            "test_event".to_string(),
            inputs,
            outputs,
            metadata,
        );

        assert!(event.verify_hash());
    }

    #[test]
    fn test_trace_bundle_creation() {
        let bundle = TraceBundle::new(
            B3Hash::hash(b"test_seed"),
            "test_plan".to_string(),
            "test_cpid".to_string(),
            "test_tenant".to_string(),
            "test_session".to_string(),
        );

        assert_eq!(bundle.version, 1);
        assert_eq!(bundle.plan_id, "test_plan");
        assert_eq!(bundle.cpid, "test_cpid");
        assert_eq!(bundle.events.len(), 0);
    }

    #[test]
    fn test_trace_bundle_add_event() {
        let mut bundle = TraceBundle::new(
            B3Hash::hash(b"test_seed"),
            "test_plan".to_string(),
            "test_cpid".to_string(),
            "test_tenant".to_string(),
            "test_session".to_string(),
        );

        let metadata = create_test_metadata();
        let inputs = HashMap::new();
        let outputs = HashMap::new();

        let event = Event::new(
            1,
            "test_op".to_string(),
            "test_event".to_string(),
            inputs,
            outputs,
            metadata,
        );

        bundle.add_event(event);

        assert_eq!(bundle.events.len(), 1);
        assert_eq!(bundle.metadata.event_count, 1);
    }

    #[test]
    fn test_trace_bundle_hash_verification() {
        let mut bundle = TraceBundle::new(
            B3Hash::hash(b"test_seed"),
            "test_plan".to_string(),
            "test_cpid".to_string(),
            "test_tenant".to_string(),
            "test_session".to_string(),
        );

        let metadata = create_test_metadata();
        let inputs = HashMap::new();
        let outputs = HashMap::new();

        let event = Event::new(
            1,
            "test_op".to_string(),
            "test_event".to_string(),
            inputs,
            outputs,
            metadata,
        );

        bundle.add_event(event);

        assert!(bundle.verify_hash());
    }

    #[test]
    fn test_get_events_by_type() {
        let mut bundle = TraceBundle::new(
            B3Hash::hash(b"test_seed"),
            "test_plan".to_string(),
            "test_cpid".to_string(),
            "test_tenant".to_string(),
            "test_session".to_string(),
        );

        let metadata = create_test_metadata();
        let inputs = HashMap::new();
        let outputs = HashMap::new();

        let event1 = Event::new(
            1,
            "op1".to_string(),
            "type1".to_string(),
            inputs.clone(),
            outputs.clone(),
            metadata.clone(),
        );

        let event2 = Event::new(
            2,
            "op2".to_string(),
            "type2".to_string(),
            inputs,
            outputs,
            metadata,
        );

        bundle.add_event(event1);
        bundle.add_event(event2);

        let type1_events = bundle.get_events_by_type("type1");
        assert_eq!(type1_events.len(), 1);

        let type2_events = bundle.get_events_by_type("type2");
        assert_eq!(type2_events.len(), 1);
    }

    #[test]
    fn test_get_events_by_tick() {
        let mut bundle = TraceBundle::new(
            B3Hash::hash(b"test_seed"),
            "test_plan".to_string(),
            "test_cpid".to_string(),
            "test_tenant".to_string(),
            "test_session".to_string(),
        );

        let metadata = create_test_metadata();
        let inputs = HashMap::new();
        let outputs = HashMap::new();

        // Add events in reverse tick order
        let event2 = Event::new(
            2,
            "op2".to_string(),
            "event2".to_string(),
            inputs.clone(),
            outputs.clone(),
            metadata.clone(),
        );

        let event1 = Event::new(
            1,
            "op1".to_string(),
            "event1".to_string(),
            inputs,
            outputs,
            metadata,
        );

        bundle.add_event(event2);
        bundle.add_event(event1);

        let ordered_events = bundle.get_events_by_tick();
        assert_eq!(ordered_events.len(), 2);
        assert_eq!(ordered_events[0].tick_id, 1);
        assert_eq!(ordered_events[1].tick_id, 2);
    }
}
