//! Test helper functions for replay tests

#![allow(dead_code)]

use adapteros_core::B3Hash;
use adapteros_trace::schema::{Event, EventMetadata, TraceBundle};
use std::collections::HashMap;
use uuid::Uuid;

fn build_event(
    tick_id: u64,
    event_type: &str,
    inputs: HashMap<String, serde_json::Value>,
    outputs: HashMap<String, serde_json::Value>,
    metadata: EventMetadata,
) -> Event {
    let event_id = Uuid::from_u128(tick_id as u128);
    let timestamp = tick_id as u128;

    let mut event = Event {
        event_id,
        tick_id,
        op_id: format!("op_{}", tick_id),
        event_type: event_type.to_string(),
        inputs,
        outputs,
        interval_id: None,
        fused_weight_hash: None,
        blake3_hash: B3Hash::hash(b"placeholder"),
        metadata,
        timestamp,
    };

    event.blake3_hash = event.compute_hash();
    event
}

/// Create a test event with minimal required fields
pub fn create_test_event(tick_id: u64, event_type: &str) -> Event {
    let global_seed = B3Hash::hash(b"test_global_seed");

    let metadata = EventMetadata {
        global_seed,
        plan_id: "test_plan".to_string(),
        cpid: "test_cpid".to_string(),
        tenant_id: "test_tenant".to_string(),
        session_id: "test_session".to_string(),
        adapter_ids: vec![],
        memory_usage_mb: 0,
        gpu_utilization_pct: 0.0,
        custom: HashMap::new(),
    };

    build_event(
        tick_id,
        event_type,
        HashMap::new(),
        HashMap::new(),
        metadata,
    )
}

/// Create a test event with custom values
pub fn create_deterministic_event(tick_id: u64, event_type: &str, value: i32) -> Event {
    let global_seed = B3Hash::hash(b"test_global_seed");

    let metadata = EventMetadata {
        global_seed,
        plan_id: "test_plan".to_string(),
        cpid: "test_cpid".to_string(),
        tenant_id: "test_tenant".to_string(),
        session_id: "test_session".to_string(),
        adapter_ids: vec![],
        memory_usage_mb: 0,
        gpu_utilization_pct: 0.0,
        custom: HashMap::new(),
    };

    let mut inputs = HashMap::new();
    inputs.insert("value".to_string(), serde_json::json!(value));

    let mut outputs = HashMap::new();
    outputs.insert("result".to_string(), serde_json::json!(value * 2));

    build_event(tick_id, event_type, inputs, outputs, metadata)
}

/// Create a test trace bundle with a specified number of events
pub fn create_test_trace_bundle(num_events: usize) -> TraceBundle {
    let global_seed = B3Hash::hash(b"test_global_seed");

    let mut bundle = TraceBundle::new(
        global_seed,
        "test_plan".to_string(),
        "test_cpid".to_string(),
        "test_tenant".to_string(),
        "test_session".to_string(),
    );

    for i in 0..num_events {
        let event = create_test_event(i as u64, "test_event");
        bundle.add_event(event);
    }

    bundle
}

/// Create a trace bundle with specific seed
pub fn create_trace_bundle_with_seed(seed_bytes: &[u8]) -> TraceBundle {
    let global_seed = B3Hash::hash(seed_bytes);

    let mut bundle = TraceBundle::new(
        global_seed,
        "test_plan".to_string(),
        "test_cpid".to_string(),
        "test_tenant".to_string(),
        "test_session".to_string(),
    );

    for i in 0..5 {
        let event = create_test_event(i as u64, "test_event");
        bundle.add_event(event);
    }

    bundle
}

/// Create a trace bundle with specific values for determinism testing
pub fn create_trace_bundle_with_values(values: Vec<i32>) -> TraceBundle {
    let global_seed = B3Hash::hash(b"deterministic_seed");

    let mut bundle = TraceBundle::new(
        global_seed,
        "test_plan".to_string(),
        "test_cpid".to_string(),
        "test_tenant".to_string(),
        "test_session".to_string(),
    );

    for (i, value) in values.iter().enumerate() {
        let event = create_deterministic_event(i as u64, "compute", *value);
        bundle.add_event(event);
    }

    bundle
}
