//! Event type definitions and builders

use std::collections::HashMap;

use crate::logical_clock::{LogicalClock, LogicalTimestamp};
use crate::schema::{Event, EventMetadata};
use adapteros_core::B3Hash;

/// Builder for creating events with logical timestamps
///
/// This builder automatically generates logical timestamps using a
/// provided `LogicalClock`, ensuring deterministic timestamp derivation.
pub struct EventBuilder {
    tick_id: u64,
    op_id: String,
    event_type: String,
    inputs: HashMap<String, serde_json::Value>,
    outputs: HashMap<String, serde_json::Value>,
    metadata: EventMetadata,
}

impl EventBuilder {
    /// Create a new event builder
    pub fn new(tick_id: u64, op_id: String, event_type: String) -> Self {
        Self {
            tick_id,
            op_id,
            event_type,
            inputs: HashMap::new(),
            outputs: HashMap::new(),
            metadata: EventMetadata {
                global_seed: B3Hash::hash(b"default"),
                plan_id: "default".to_string(),
                cpid: "default".to_string(),
                tenant_id: "default".to_string(),
                session_id: "default".to_string(),
                adapter_ids: Vec::new(),
                memory_usage_mb: 0,
                gpu_utilization_pct: 0.0,
                custom: HashMap::new(),
            },
        }
    }

    /// Set the inputs
    pub fn with_inputs(mut self, inputs: HashMap<String, serde_json::Value>) -> Self {
        self.inputs = inputs;
        self
    }

    /// Add a single input
    pub fn add_input(mut self, key: String, value: serde_json::Value) -> Self {
        self.inputs.insert(key, value);
        self
    }

    /// Set the outputs
    pub fn with_outputs(mut self, outputs: HashMap<String, serde_json::Value>) -> Self {
        self.outputs = outputs;
        self
    }

    /// Add a single output
    pub fn add_output(mut self, key: String, value: serde_json::Value) -> Self {
        self.outputs.insert(key, value);
        self
    }

    /// Set the metadata
    pub fn with_metadata(mut self, metadata: EventMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// Build the event with a logical timestamp from a clock
    pub fn build_with_clock(self, clock: &LogicalClock) -> adapteros_core::Result<Event> {
        let logical_timestamp =
            clock.advance_for_operation(&self.op_id, &self.event_type, &self.inputs)?;

        Ok(Event::new(
            self.tick_id,
            self.op_id,
            self.event_type,
            self.inputs,
            self.outputs,
            self.metadata,
            logical_timestamp,
        ))
    }

    /// Build the event with an explicit logical timestamp
    pub fn build_with_timestamp(self, logical_timestamp: LogicalTimestamp) -> Event {
        Event::new(
            self.tick_id,
            self.op_id,
            self.event_type,
            self.inputs,
            self.outputs,
            self.metadata,
            logical_timestamp,
        )
    }

    /// Build the event without wall-clock timestamp (for deterministic replay)
    pub fn build_deterministic(self, clock: &LogicalClock) -> adapteros_core::Result<Event> {
        let logical_timestamp =
            clock.advance_for_operation(&self.op_id, &self.event_type, &self.inputs)?;

        Ok(Event::new_deterministic(
            self.tick_id,
            self.op_id,
            self.event_type,
            self.inputs,
            self.outputs,
            self.metadata,
            logical_timestamp,
        ))
    }
}

/// Inference start event
pub fn inference_start_event(
    tick_id: u64,
    plan_id: String,
    cpid: String,
    tenant_id: String,
    session_id: String,
    global_seed: B3Hash,
    clock: &LogicalClock,
) -> adapteros_core::Result<Event> {
    EventBuilder::new(
        tick_id,
        "inference_start".to_string(),
        "inference.start".to_string(),
    )
    .with_metadata(EventMetadata {
        global_seed,
        plan_id,
        cpid,
        tenant_id,
        session_id,
        adapter_ids: Vec::new(),
        memory_usage_mb: 0,
        gpu_utilization_pct: 0.0,
        custom: HashMap::new(),
    })
    .build_with_clock(clock)
}

/// Inference end event
pub fn inference_end_event(
    tick_id: u64,
    _session_id: String,
    total_tokens: u32,
    total_time_ms: u64,
    clock: &LogicalClock,
) -> adapteros_core::Result<Event> {
    EventBuilder::new(
        tick_id,
        "inference_end".to_string(),
        "inference.end".to_string(),
    )
    .add_output(
        "total_tokens".to_string(),
        serde_json::Value::Number(total_tokens.into()),
    )
    .add_output(
        "total_time_ms".to_string(),
        serde_json::Value::Number(total_time_ms.into()),
    )
    .build_with_clock(clock)
}

/// Token generation event
pub fn token_generated_event(
    tick_id: u64,
    token_id: u32,
    logits: Vec<f32>,
    adapter_ids: Vec<String>,
    clock: &LogicalClock,
) -> adapteros_core::Result<Event> {
    let logits_json: Vec<serde_json::Value> = logits
        .into_iter()
        .map(|f| serde_json::Value::Number(serde_json::Number::from_f64(f as f64).unwrap()))
        .collect();

    EventBuilder::new(
        tick_id,
        format!("token_{}", token_id),
        "inference.token".to_string(),
    )
    .add_input(
        "token_id".to_string(),
        serde_json::Value::Number(token_id.into()),
    )
    .add_output("logits".to_string(), serde_json::Value::Array(logits_json))
    .with_metadata(EventMetadata {
        global_seed: B3Hash::hash(b"default"),
        plan_id: "default".to_string(),
        cpid: "default".to_string(),
        tenant_id: "default".to_string(),
        session_id: "default".to_string(),
        adapter_ids,
        memory_usage_mb: 0,
        gpu_utilization_pct: 0.0,
        custom: HashMap::new(),
    })
    .build_with_clock(clock)
}

/// Kernel execution event
pub fn kernel_execute_event(
    tick_id: u64,
    kernel_name: String,
    input_tensors: Vec<String>,
    output_tensors: Vec<String>,
    execution_time_ms: u64,
    clock: &LogicalClock,
) -> adapteros_core::Result<Event> {
    EventBuilder::new(
        tick_id,
        format!("kernel_{}", kernel_name),
        "kernel.execute".to_string(),
    )
    .add_input(
        "kernel_name".to_string(),
        serde_json::Value::String(kernel_name),
    )
    .add_input(
        "input_tensors".to_string(),
        serde_json::Value::Array(
            input_tensors
                .into_iter()
                .map(serde_json::Value::String)
                .collect(),
        ),
    )
    .add_output(
        "output_tensors".to_string(),
        serde_json::Value::Array(
            output_tensors
                .into_iter()
                .map(serde_json::Value::String)
                .collect(),
        ),
    )
    .add_output(
        "execution_time_ms".to_string(),
        serde_json::Value::Number(execution_time_ms.into()),
    )
    .build_with_clock(clock)
}

/// Adapter load event
pub fn adapter_load_event(
    tick_id: u64,
    adapter_id: String,
    adapter_size_mb: u64,
    load_time_ms: u64,
    clock: &LogicalClock,
) -> adapteros_core::Result<Event> {
    EventBuilder::new(
        tick_id,
        format!("load_{}", adapter_id),
        "adapter.load".to_string(),
    )
    .add_input(
        "adapter_id".to_string(),
        serde_json::Value::String(adapter_id),
    )
    .add_output(
        "adapter_size_mb".to_string(),
        serde_json::Value::Number(adapter_size_mb.into()),
    )
    .add_output(
        "load_time_ms".to_string(),
        serde_json::Value::Number(load_time_ms.into()),
    )
    .build_with_clock(clock)
}

/// Adapter unload event
pub fn adapter_unload_event(
    tick_id: u64,
    adapter_id: String,
    unload_time_ms: u64,
    clock: &LogicalClock,
) -> adapteros_core::Result<Event> {
    EventBuilder::new(
        tick_id,
        format!("unload_{}", adapter_id),
        "adapter.unload".to_string(),
    )
    .add_input(
        "adapter_id".to_string(),
        serde_json::Value::String(adapter_id),
    )
    .add_output(
        "unload_time_ms".to_string(),
        serde_json::Value::Number(unload_time_ms.into()),
    )
    .build_with_clock(clock)
}

/// Router decision event
pub fn router_decision_event(
    tick_id: u64,
    selected_adapters: Vec<String>,
    gate_values: Vec<f32>,
    entropy: f32,
    clock: &LogicalClock,
) -> adapteros_core::Result<Event> {
    let gate_values_json: Vec<serde_json::Value> = gate_values
        .into_iter()
        .map(|f| serde_json::Value::Number(serde_json::Number::from_f64(f as f64).unwrap()))
        .collect();

    EventBuilder::new(
        tick_id,
        "router_decision".to_string(),
        "router.decision".to_string(),
    )
    .add_output(
        "selected_adapters".to_string(),
        serde_json::Value::Array(
            selected_adapters
                .into_iter()
                .map(serde_json::Value::String)
                .collect(),
        ),
    )
    .add_output(
        "gate_values".to_string(),
        serde_json::Value::Array(gate_values_json),
    )
    .add_output(
        "entropy".to_string(),
        serde_json::Value::Number(serde_json::Number::from_f64(entropy as f64).unwrap()),
    )
    .build_with_clock(clock)
}

/// Memory allocation event
pub fn memory_alloc_event(
    tick_id: u64,
    allocation_id: String,
    size_bytes: u64,
    memory_type: String,
    clock: &LogicalClock,
) -> adapteros_core::Result<Event> {
    EventBuilder::new(
        tick_id,
        format!("alloc_{}", allocation_id),
        "memory.alloc".to_string(),
    )
    .add_input(
        "allocation_id".to_string(),
        serde_json::Value::String(allocation_id),
    )
    .add_input(
        "size_bytes".to_string(),
        serde_json::Value::Number(size_bytes.into()),
    )
    .add_input(
        "memory_type".to_string(),
        serde_json::Value::String(memory_type),
    )
    .build_with_clock(clock)
}

/// Memory deallocation event
pub fn memory_dealloc_event(
    tick_id: u64,
    allocation_id: String,
    size_bytes: u64,
    clock: &LogicalClock,
) -> adapteros_core::Result<Event> {
    EventBuilder::new(
        tick_id,
        format!("dealloc_{}", allocation_id),
        "memory.dealloc".to_string(),
    )
    .add_input(
        "allocation_id".to_string(),
        serde_json::Value::String(allocation_id),
    )
    .add_input(
        "size_bytes".to_string(),
        serde_json::Value::Number(size_bytes.into()),
    )
    .build_with_clock(clock)
}

/// Policy check event
pub fn policy_check_event(
    tick_id: u64,
    policy_name: String,
    result: bool,
    details: String,
    clock: &LogicalClock,
) -> adapteros_core::Result<Event> {
    EventBuilder::new(
        tick_id,
        format!("policy_{}", policy_name),
        "policy.check".to_string(),
    )
    .add_input(
        "policy_name".to_string(),
        serde_json::Value::String(policy_name),
    )
    .add_output("result".to_string(), serde_json::Value::Bool(result))
    .add_output("details".to_string(), serde_json::Value::String(details))
    .build_with_clock(clock)
}

/// Telemetry event
pub fn telemetry_event(
    tick_id: u64,
    event_type: String,
    payload: serde_json::Value,
    clock: &LogicalClock,
) -> adapteros_core::Result<Event> {
    EventBuilder::new(
        tick_id,
        format!("telemetry_{}", event_type),
        "telemetry".to_string(),
    )
    .add_input(
        "event_type".to_string(),
        serde_json::Value::String(event_type),
    )
    .add_output("payload".to_string(), payload)
    .build_with_clock(clock)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_clock() -> LogicalClock {
        LogicalClock::new(B3Hash::hash(b"test_seed"))
    }

    #[test]
    fn test_event_builder_with_clock() {
        let clock = create_test_clock();
        let event = EventBuilder::new(1, "test_op".to_string(), "test_event".to_string())
            .add_input(
                "key1".to_string(),
                serde_json::Value::String("value1".to_string()),
            )
            .add_output("key2".to_string(), serde_json::Value::Number(42.into()))
            .build_with_clock(&clock)
            .unwrap();

        assert_eq!(event.tick_id, 1);
        assert_eq!(event.op_id, "test_op");
        assert_eq!(event.event_type, "test_event");
        assert_eq!(event.inputs.len(), 1);
        assert_eq!(event.outputs.len(), 1);
        assert_eq!(event.logical_timestamp.global_tick, 0);
    }

    #[test]
    fn test_inference_start_event() {
        let clock = create_test_clock();
        let event = inference_start_event(
            1,
            "test_plan".to_string(),
            "test_cpid".to_string(),
            "test_tenant".to_string(),
            "test_session".to_string(),
            B3Hash::hash(b"test_seed"),
            &clock,
        )
        .unwrap();

        assert_eq!(event.tick_id, 1);
        assert_eq!(event.event_type, "inference.start");
        assert_eq!(event.metadata.plan_id, "test_plan");
        assert_eq!(event.metadata.cpid, "test_cpid");
    }

    #[test]
    fn test_token_generated_event() {
        let clock = create_test_clock();
        let event = token_generated_event(
            1,
            123,
            vec![0.1, 0.2, 0.3],
            vec!["adapter1".to_string()],
            &clock,
        )
        .unwrap();

        assert_eq!(event.tick_id, 1);
        assert_eq!(event.event_type, "inference.token");
        assert_eq!(event.metadata.adapter_ids.len(), 1);
        assert_eq!(event.logical_timestamp.token_position, Some(123));
    }

    #[test]
    fn test_kernel_execute_event() {
        let clock = create_test_clock();
        let event = kernel_execute_event(
            1,
            "attention".to_string(),
            vec!["input1".to_string()],
            vec!["output1".to_string()],
            100,
            &clock,
        )
        .unwrap();

        assert_eq!(event.tick_id, 1);
        assert_eq!(event.event_type, "kernel.execute");
        assert!(event.inputs.contains_key("kernel_name"));
        assert!(event.outputs.contains_key("execution_time_ms"));
    }

    #[test]
    fn test_router_decision_event() {
        let clock = create_test_clock();
        let event = router_decision_event(
            1,
            vec!["adapter1".to_string(), "adapter2".to_string()],
            vec![0.8, 0.2],
            0.5,
            &clock,
        )
        .unwrap();

        assert_eq!(event.tick_id, 1);
        assert_eq!(event.event_type, "router.decision");
        assert!(event.outputs.contains_key("selected_adapters"));
        assert!(event.outputs.contains_key("gate_values"));
        assert!(event.outputs.contains_key("entropy"));
    }

    #[test]
    fn test_deterministic_build() {
        let clock = create_test_clock();
        let event1 = EventBuilder::new(1, "test_op".to_string(), "test_event".to_string())
            .build_deterministic(&clock)
            .unwrap();

        let event2 = EventBuilder::new(1, "test_op".to_string(), "test_event".to_string())
            .build_deterministic(&clock)
            .unwrap();

        assert_eq!(event1.wall_clock_timestamp, None);
        assert_eq!(event2.wall_clock_timestamp, None);
        // Different timestamps due to advancing clock
        assert_ne!(
            event1.logical_timestamp.global_tick,
            event2.logical_timestamp.global_tick
        );
    }
}
