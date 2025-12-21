//! Event type definitions and builders

use std::collections::HashMap;

use crate::schema::{Event, EventMetadata};
use adapteros_core::B3Hash;
use adapteros_telemetry::events::RouterDecisionEvent;

/// Builder for creating events
pub struct EventBuilder {
    tick_id: u64,
    op_id: String,
    event_type: String,
    inputs: HashMap<String, serde_json::Value>,
    outputs: HashMap<String, serde_json::Value>,
    metadata: EventMetadata,
    interval_id: Option<String>,
    fused_weight_hash: Option<B3Hash>,
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
            interval_id: None,
            fused_weight_hash: None,
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

    /// Set fusion interval identifier for this event
    pub fn with_interval_id(mut self, interval_id: String) -> Self {
        self.interval_id = Some(interval_id);
        self
    }

    /// Set fused weight hash for this event
    pub fn with_fused_weight_hash(mut self, fused_weight_hash: B3Hash) -> Self {
        self.fused_weight_hash = Some(fused_weight_hash);
        self
    }

    /// Build the event
    pub fn build(self) -> Event {
        let event = Event::new(
            self.tick_id,
            self.op_id,
            self.event_type,
            self.inputs,
            self.outputs,
            self.metadata,
        );

        if self.interval_id.is_some() || self.fused_weight_hash.is_some() {
            return event.with_interval(self.interval_id, self.fused_weight_hash);
        }

        event
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
) -> Event {
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
    .build()
}

/// Inference end event
pub fn inference_end_event(
    tick_id: u64,
    _session_id: String,
    total_tokens: u32,
    total_time_ms: u64,
) -> Event {
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
    .build()
}

/// Token generation event
pub fn token_generated_event(
    tick_id: u64,
    token_id: u32,
    logits: Vec<f32>,
    adapter_ids: Vec<String>,
) -> Event {
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
    .build()
}

/// Kernel execution event
pub fn kernel_execute_event(
    tick_id: u64,
    kernel_name: String,
    input_tensors: Vec<String>,
    output_tensors: Vec<String>,
    execution_time_ms: u64,
) -> Event {
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
    .build()
}

/// Adapter load event
pub fn adapter_load_event(
    tick_id: u64,
    adapter_id: String,
    adapter_size_mb: u64,
    load_time_ms: u64,
) -> Event {
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
    .build()
}

/// Adapter unload event
pub fn adapter_unload_event(tick_id: u64, adapter_id: String, unload_time_ms: u64) -> Event {
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
    .build()
}

/// Router decision event
pub fn router_decision_event(tick_id: u64, decision: RouterDecisionEvent) -> Event {
    let payload = serde_json::to_value(decision)
        .expect("Failed to serialize RouterDecisionEvent to canonical payload");

    EventBuilder::new(
        tick_id,
        format!("router_decision_{}", tick_id),
        "router.decision".to_string(),
    )
    .add_output("router_decision".to_string(), payload)
    .build()
}

/// Fusion interval boundary event with fused weight hash evidence
pub fn fusion_interval_event(
    tick_id: u64,
    interval_id: String,
    start_token: usize,
    end_token: usize,
    fused_weight_hash: B3Hash,
) -> Event {
    EventBuilder::new(
        tick_id,
        format!("fusion_interval_{}", interval_id),
        "fusion.interval".to_string(),
    )
    .add_input(
        "interval_id".to_string(),
        serde_json::Value::String(interval_id.clone()),
    )
    .add_input(
        "start_token".to_string(),
        serde_json::Value::Number((start_token as u64).into()),
    )
    .add_input(
        "end_token".to_string(),
        serde_json::Value::Number((end_token as u64).into()),
    )
    .add_output(
        "fused_weight_hash".to_string(),
        serde_json::Value::String(fused_weight_hash.to_hex()),
    )
    .with_interval_id(interval_id)
    .with_fused_weight_hash(fused_weight_hash)
    .build()
}

/// Memory allocation event
pub fn memory_alloc_event(
    tick_id: u64,
    allocation_id: String,
    size_bytes: u64,
    memory_type: String,
) -> Event {
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
    .build()
}

/// Memory deallocation event
pub fn memory_dealloc_event(tick_id: u64, allocation_id: String, size_bytes: u64) -> Event {
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
    .build()
}

/// Policy check event
pub fn policy_check_event(
    tick_id: u64,
    policy_name: String,
    result: bool,
    details: String,
) -> Event {
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
    .build()
}

/// Telemetry event
pub fn telemetry_event(tick_id: u64, event_type: String, payload: serde_json::Value) -> Event {
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
    .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_builder() {
        let event = EventBuilder::new(1, "test_op".to_string(), "test_event".to_string())
            .add_input(
                "key1".to_string(),
                serde_json::Value::String("value1".to_string()),
            )
            .add_output("key2".to_string(), serde_json::Value::Number(42.into()))
            .build();

        assert_eq!(event.tick_id, 1);
        assert_eq!(event.op_id, "test_op");
        assert_eq!(event.event_type, "test_event");
        assert_eq!(event.inputs.len(), 1);
        assert_eq!(event.outputs.len(), 1);
    }

    #[test]
    fn test_inference_start_event() {
        let event = inference_start_event(
            1,
            "test_plan".to_string(),
            "test_cpid".to_string(),
            "test_tenant".to_string(),
            "test_session".to_string(),
            B3Hash::hash(b"test_seed"),
        );

        assert_eq!(event.tick_id, 1);
        assert_eq!(event.event_type, "inference.start");
        assert_eq!(event.metadata.plan_id, "test_plan");
        assert_eq!(event.metadata.cpid, "test_cpid");
    }

    #[test]
    fn test_token_generated_event() {
        let event =
            token_generated_event(1, 123, vec![0.1, 0.2, 0.3], vec!["adapter1".to_string()]);

        assert_eq!(event.tick_id, 1);
        assert_eq!(event.event_type, "inference.token");
        assert_eq!(event.metadata.adapter_ids.len(), 1);
    }

    #[test]
    fn test_kernel_execute_event() {
        let event = kernel_execute_event(
            1,
            "attention".to_string(),
            vec!["input1".to_string()],
            vec!["output1".to_string()],
            100,
        );

        assert_eq!(event.tick_id, 1);
        assert_eq!(event.event_type, "kernel.execute");
        assert!(event.inputs.contains_key("kernel_name"));
        assert!(event.outputs.contains_key("execution_time_ms"));
    }

    #[test]
    fn test_router_decision_event() {
        use adapteros_telemetry::events::{RouterCandidate, RouterDecisionEvent};

        let decision = RouterDecisionEvent {
            step: 0,
            input_token_id: None,
            candidate_adapters: vec![
                RouterCandidate {
                    adapter_idx: 0,
                    raw_score: 0.8,
                    gate_q15: 26214, // 0.8 * 32767
                },
                RouterCandidate {
                    adapter_idx: 1,
                    raw_score: 0.2,
                    gate_q15: 6553, // 0.2 * 32767
                },
            ],
            entropy: 0.5,
            tau: 1.0,
            entropy_floor: 0.1,
            stack_hash: None,
            stack_id: Some("test-stack".to_string()),
            stack_version: Some(1),
        };

        let event = router_decision_event(1, decision);

        assert_eq!(event.tick_id, 1);
        assert_eq!(event.event_type, "router.decision");
        assert!(event.outputs.contains_key("selected_adapters"));
        assert!(event.outputs.contains_key("gate_values"));
        assert!(event.outputs.contains_key("entropy"));
    }

    #[test]
    fn test_fusion_interval_event() {
        let hash = B3Hash::hash(b"fused");
        let event = fusion_interval_event(1, "request-0".to_string(), 0, 4, hash);

        assert_eq!(event.interval_id.as_deref(), Some("request-0"));
        assert_eq!(event.fused_weight_hash, Some(hash));
        assert!(event.outputs.contains_key("fused_weight_hash"));
        assert!(event.verify_hash());
    }
}
