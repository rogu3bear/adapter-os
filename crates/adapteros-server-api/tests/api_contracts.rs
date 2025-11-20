// Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.
//
// API Contract Tests
//
// Purpose: Validate API response schemas against canonical reference data.
// Tests ensure backward compatibility and contract compliance.

use adapteros_server_api::types::{
    AdapterResponse, AdapterStats, ComponentHealth, ComponentStatus, SystemHealthResponse,
};
use serde::{Deserialize, Serialize};
use serde_json::{from_str, Value};

// ============================================================================
// Test Data Structures
// ============================================================================

#[derive(Debug, Deserialize, Serialize)]
struct AdaptersListResponse {
    adapters: Vec<AdapterResponse>,
    total: usize,
    page: usize,
    page_size: usize,
}

#[derive(Debug, Deserialize, Serialize)]
struct AdapterLineageResponse {
    adapter_id: String,
    ancestors: Vec<LineageNode>,
    self_node: LineageNode,
    descendants: Vec<LineageNode>,
    total_nodes: usize,
}

#[derive(Debug, Deserialize, Serialize)]
struct LineageNode {
    adapter_id: String,
    adapter_name: Option<String>,
    tenant_namespace: Option<String>,
    domain: Option<String>,
    purpose: Option<String>,
    revision: Option<String>,
    parent_id: Option<String>,
    fork_type: Option<String>,
    fork_reason: Option<String>,
    current_state: String,
    tier: String,
    created_at: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct RoutingDecisionsResponse {
    decisions: Vec<RoutingDecisionResponse>,
    total: usize,
    page: usize,
    page_size: usize,
}

#[derive(Debug, Deserialize, Serialize)]
struct RoutingDecisionResponse {
    id: String,
    tenant_id: String,
    timestamp: String,
    request_id: Option<String>,
    step: i64,
    input_token_id: Option<i64>,
    stack_id: Option<String>,
    stack_hash: Option<String>,
    entropy: f64,
    tau: f64,
    entropy_floor: f64,
    k_value: Option<i64>,
    candidates: Vec<RouterCandidateResponse>,
    router_latency_us: Option<i64>,
    total_inference_latency_us: Option<i64>,
    overhead_pct: Option<f64>,
}

#[derive(Debug, Deserialize, Serialize)]
struct RouterCandidateResponse {
    adapter_idx: u16,
    raw_score: f32,
    gate_q15: i16,
    gate_float: f32,
    selected: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct BasicHealthResponse {
    status: String,
    timestamp: u64,
}

// ============================================================================
// Adapter Endpoints Contract Tests
// ============================================================================

#[test]
fn test_adapters_list_contract_schema() {
    let json = include_str!("../../../tests/training/datasets/cli_contract/adapters_list.json");
    let response: AdaptersListResponse =
        from_str(json).expect("adapters_list.json should match AdaptersListResponse schema");

    // Validate response structure
    assert_eq!(response.adapters.len(), 3, "Should have 3 adapters");
    assert_eq!(response.total, 3, "Total count should match");
    assert_eq!(response.page, 1, "Should be page 1");
    assert_eq!(response.page_size, 10, "Page size should be 10");

    // Validate first adapter (full validation)
    let adapter = &response.adapters[0];
    assert_eq!(adapter.id, "abc123");
    assert_eq!(adapter.adapter_id, "abc123");
    assert_eq!(adapter.name, "tenant-a/engineering/code-review/r001");
    assert_eq!(
        adapter.hash_b3,
        "blake3:1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef"
    );
    assert_eq!(adapter.rank, 16);
    assert_eq!(adapter.tier, "tier_1");
    assert_eq!(adapter.languages.len(), 2);
    assert_eq!(adapter.languages[0], "rust");
    assert_eq!(adapter.languages[1], "python");
    assert_eq!(adapter.framework, Some("llama".to_string()));

    // Validate stats
    let stats = adapter.stats.as_ref().expect("Should have stats");
    assert_eq!(stats.total_activations, 1250);
    assert_eq!(stats.selected_count, 980);
    assert!((stats.avg_gate_value - 0.87).abs() < 0.01);
    assert!((stats.selection_rate - 0.784).abs() < 0.001);
}

#[test]
fn test_adapters_list_contract_semantic_naming() {
    let json = include_str!("../../../tests/training/datasets/cli_contract/adapters_list.json");
    let response: AdaptersListResponse = from_str(json).unwrap();

    // Validate semantic naming fields (PRD-08)
    for adapter in &response.adapters {
        // All adapters should have semantic naming
        assert!(
            adapter.name.contains('/'),
            "Adapter {} should have semantic name format",
            adapter.id
        );

        // Count slashes (should be 3 for tenant/domain/purpose/revision)
        let slash_count = adapter.name.matches('/').count();
        assert_eq!(
            slash_count, 3,
            "Adapter {} should have 3 slashes in semantic name",
            adapter.id
        );
    }
}

#[test]
fn test_adapters_list_contract_tier_values() {
    let json = include_str!("../../../tests/training/datasets/cli_contract/adapters_list.json");
    let response: AdaptersListResponse = from_str(json).unwrap();

    let valid_tiers = ["tier_1", "tier_2", "tier_3"];

    for adapter in &response.adapters {
        assert!(
            valid_tiers.contains(&adapter.tier.as_str()),
            "Adapter {} has invalid tier: {}",
            adapter.id,
            adapter.tier
        );
    }
}

#[test]
fn test_adapters_list_contract_state_values() {
    let json = include_str!("../../../tests/training/datasets/cli_contract/adapters_list.json");
    let response: Value = from_str(json).unwrap();

    let valid_states = ["unloaded", "cold", "warm", "hot", "resident"];

    let adapters = response["adapters"].as_array().unwrap();
    for adapter in adapters {
        let state = adapter["current_state"].as_str().unwrap();
        assert!(valid_states.contains(&state), "Invalid state: {}", state);
    }
}

#[test]
fn test_adapter_lineage_contract_schema() {
    let json = include_str!("../../../tests/training/datasets/cli_contract/adapter_lineage.json");
    let response: AdapterLineageResponse =
        from_str(json).expect("adapter_lineage.json should match schema");

    // Validate structure
    assert_eq!(response.adapter_id, "def456");
    assert_eq!(response.ancestors.len(), 2, "Should have 2 ancestors");
    assert_eq!(response.descendants.len(), 2, "Should have 2 descendants");
    assert_eq!(response.total_nodes, 5, "Total nodes should be 5");

    // Validate self node
    assert_eq!(response.self_node.adapter_id, "def456");
    assert_eq!(
        response.self_node.adapter_name,
        Some("tenant-b/ml/sentiment-analysis/r003".to_string())
    );
    assert_eq!(response.self_node.parent_id, Some("def234".to_string()));
    assert_eq!(
        response.self_node.fork_type,
        Some("incremental_improvement".to_string())
    );
}

#[test]
fn test_adapter_lineage_contract_ancestry_chain() {
    let json = include_str!("../../../tests/training/datasets/cli_contract/adapter_lineage.json");
    let response: AdapterLineageResponse = from_str(json).unwrap();

    // Validate ancestry chain integrity
    assert_eq!(
        response.ancestors[0].parent_id, None,
        "Root ancestor should have no parent"
    );
    assert_eq!(
        response.ancestors[1].parent_id,
        Some("def123".to_string()),
        "Second ancestor should point to first"
    );
    assert_eq!(
        response.self_node.parent_id,
        Some("def234".to_string()),
        "Self should point to immediate ancestor"
    );

    // Validate descendants reference self
    for desc in &response.descendants {
        assert_eq!(
            desc.parent_id,
            Some("def456".to_string()),
            "All descendants should reference self as parent"
        );
    }
}

#[test]
fn test_adapter_lineage_contract_fork_types() {
    let json = include_str!("../../../tests/training/datasets/cli_contract/adapter_lineage.json");
    let response: AdapterLineageResponse = from_str(json).unwrap();

    let valid_fork_types = [
        "incremental_improvement",
        "experimental",
        "domain_adaptation",
        "bug_fix",
        "refactor",
    ];

    // Check all nodes with fork_type
    let all_nodes: Vec<&LineageNode> = response
        .ancestors
        .iter()
        .chain(std::iter::once(&response.self_node))
        .chain(response.descendants.iter())
        .collect();

    for node in all_nodes {
        if let Some(ref fork_type) = node.fork_type {
            assert!(
                valid_fork_types.contains(&fork_type.as_str()),
                "Invalid fork_type: {}",
                fork_type
            );
        }
    }
}

// ============================================================================
// Routing Decisions Contract Tests
// ============================================================================

#[test]
fn test_routing_decisions_contract_schema() {
    let json = include_str!("../../../tests/training/datasets/routing/routing_decisions.json");
    let response: RoutingDecisionsResponse =
        from_str(json).expect("routing_decisions.json should match schema");

    // Validate response structure
    assert_eq!(response.decisions.len(), 3, "Should have 3 decisions");
    assert_eq!(response.total, 3);
    assert_eq!(response.page, 1);
    assert_eq!(response.page_size, 50);

    // Validate first decision
    let decision = &response.decisions[0];
    assert_eq!(decision.id, "route-001");
    assert_eq!(decision.tenant_id, "tenant-a");
    assert_eq!(decision.step, 42);
    assert_eq!(decision.k_value, Some(3));
    assert_eq!(decision.candidates.len(), 4, "Should have 4 candidates");
}

#[test]
fn test_routing_decisions_contract_candidate_selection() {
    let json = include_str!("../../../tests/training/datasets/routing/routing_decisions.json");
    let response: RoutingDecisionsResponse = from_str(json).unwrap();

    for decision in &response.decisions {
        let k = decision.k_value.unwrap_or(0);
        let selected_count = decision.candidates.iter().filter(|c| c.selected).count() as i64;

        assert_eq!(
            selected_count, k,
            "Decision {} should have exactly k={} selected candidates, got {}",
            decision.id, k, selected_count
        );

        // Validate top-k are selected (candidates sorted by score)
        let mut sorted_candidates = decision.candidates.clone();
        sorted_candidates.sort_by(|a, b| b.raw_score.partial_cmp(&a.raw_score).unwrap());

        for (i, candidate) in sorted_candidates.iter().enumerate() {
            if i < k as usize {
                assert!(
                    candidate.selected,
                    "Top-{} candidate should be selected in decision {}",
                    i + 1,
                    decision.id
                );
            } else {
                assert!(
                    !candidate.selected,
                    "Non-top-k candidate should not be selected in decision {}",
                    decision.id
                );
            }
        }
    }
}

#[test]
fn test_routing_decisions_contract_q15_quantization() {
    let json = include_str!("../../../tests/training/datasets/routing/routing_decisions.json");
    let response: RoutingDecisionsResponse = from_str(json).unwrap();

    for decision in &response.decisions {
        for candidate in &decision.candidates {
            // Q15 range: -32768 to 32767
            assert!(
                candidate.gate_q15 >= -32768 && candidate.gate_q15 <= 32767,
                "Q15 value out of range: {}",
                candidate.gate_q15
            );

            // Validate Q15 <-> float conversion (gate_float ≈ gate_q15 / 32768.0)
            let expected_float = (candidate.gate_q15 as f32) / 32768.0;
            let diff = (candidate.gate_float - expected_float).abs();
            assert!(
                diff < 0.01,
                "Q15 conversion mismatch: q15={}, float={}, expected={}",
                candidate.gate_q15,
                candidate.gate_float,
                expected_float
            );
        }
    }
}

#[test]
fn test_routing_decisions_contract_overhead_metrics() {
    let json = include_str!("../../../tests/training/datasets/routing/routing_decisions.json");
    let response: RoutingDecisionsResponse = from_str(json).unwrap();

    for decision in &response.decisions {
        if let (Some(router_us), Some(total_us), Some(overhead)) = (
            decision.router_latency_us,
            decision.total_inference_latency_us,
            decision.overhead_pct,
        ) {
            // Validate overhead calculation
            let expected_overhead = (router_us as f64 / total_us as f64) * 100.0;
            let diff = (overhead - expected_overhead).abs();

            // Allow 0.5% tolerance for rounding
            assert!(
                diff < 0.5,
                "Overhead calculation mismatch in {}: expected {:.2}%, got {:.2}%",
                decision.id,
                expected_overhead,
                overhead
            );
        }
    }
}

#[test]
fn test_routing_decisions_contract_entropy_bounds() {
    let json = include_str!("../../../tests/training/datasets/routing/routing_decisions.json");
    let response: RoutingDecisionsResponse = from_str(json).unwrap();

    for decision in &response.decisions {
        // Entropy should be non-negative
        assert!(
            decision.entropy >= 0.0,
            "Entropy cannot be negative: {}",
            decision.entropy
        );

        // Entropy floor should be less than entropy
        assert!(
            decision.entropy_floor <= decision.entropy,
            "Entropy floor {} cannot exceed entropy {} in decision {}",
            decision.entropy_floor,
            decision.entropy,
            decision.id
        );

        // Tau (temperature) should be positive
        assert!(decision.tau > 0.0, "Tau must be positive: {}", decision.tau);
    }
}

// ============================================================================
// Health Check Contract Tests
// ============================================================================

#[test]
fn test_healthz_basic_contract_schema() {
    let json = include_str!("../../../tests/training/datasets/metrics/healthz_basic.json");
    let response: BasicHealthResponse =
        from_str(json).expect("healthz_basic.json should match schema");

    assert_eq!(response.status, "healthy");
    assert!(response.timestamp > 0, "Timestamp should be non-zero");
}

#[test]
fn test_healthz_all_contract_schema() {
    let json = include_str!("../../../tests/training/datasets/metrics/healthz_all.json");
    let response: SystemHealthResponse =
        from_str(json).expect("healthz_all.json should match schema");

    assert_eq!(response.components.len(), 6, "Should have 6 components");
    assert!(response.timestamp > 0);

    // Validate component names
    let component_names: Vec<String> = response
        .components
        .iter()
        .map(|c| c.component.clone())
        .collect();

    let expected_components = [
        "router",
        "loader",
        "kernel",
        "db",
        "telemetry",
        "system-metrics",
    ];

    for expected in expected_components {
        assert!(
            component_names.contains(&expected.to_string()),
            "Missing component: {}",
            expected
        );
    }
}

#[test]
fn test_healthz_all_contract_component_status() {
    let json = include_str!("../../../tests/training/datasets/metrics/healthz_all.json");
    let response: Value = from_str(json).unwrap();

    let valid_statuses = ["healthy", "degraded", "unhealthy"];

    for component in response["components"].as_array().unwrap() {
        let status = component["status"].as_str().unwrap();
        assert!(
            valid_statuses.contains(&status),
            "Invalid status: {}",
            status
        );

        // All components should have required fields
        assert!(component["component"].is_string());
        assert!(component["message"].is_string());
        assert!(component["timestamp"].is_number());
    }
}

#[test]
fn test_healthz_degraded_contract_overall_status() {
    let json = include_str!("../../../tests/training/datasets/metrics/healthz_degraded.json");
    let response: Value = from_str(json).unwrap();

    // Overall status should be degraded
    assert_eq!(response["overall_status"].as_str().unwrap(), "degraded");

    // Should have at least one degraded component
    let components = response["components"].as_array().unwrap();
    let degraded_count = components
        .iter()
        .filter(|c| c["status"].as_str().unwrap() == "degraded")
        .count();

    assert!(
        degraded_count > 0,
        "Should have at least one degraded component"
    );
}

#[test]
fn test_healthz_router_contract_details() {
    let json = include_str!("../../../tests/training/datasets/metrics/healthz_router.json");
    let response: ComponentHealth =
        from_str(json).expect("healthz_router.json should match ComponentHealth schema");

    assert_eq!(response.component, "router");

    // Validate details structure
    let details = response.details.expect("Router should have details");
    assert!(details["avg_decision_rate"].is_number());
    assert!(details["avg_overhead_pct"].is_number());
    assert!(details["anomaly_rate"].is_number());
}

// ============================================================================
// Cross-Endpoint Contract Tests
// ============================================================================

#[test]
fn test_contract_timestamp_format_consistency() {
    // Validate ISO-8601 timestamps across endpoints
    let routing_json =
        include_str!("../../../tests/training/datasets/api-contracts/routing_decisions.json");
    let routing: RoutingDecisionsResponse = from_str(routing_json).unwrap();

    for decision in &routing.decisions {
        // Should be ISO-8601 format (contains 'T' and 'Z')
        assert!(
            decision.timestamp.contains('T'),
            "Timestamp should be ISO-8601: {}",
            decision.timestamp
        );
        assert!(
            decision.timestamp.ends_with('Z'),
            "Timestamp should end with Z: {}",
            decision.timestamp
        );
    }

    // Validate lineage timestamps
    let lineage_json =
        include_str!("../../../tests/training/datasets/api-contracts/adapter_lineage.json");
    let lineage: AdapterLineageResponse = from_str(lineage_json).unwrap();

    assert!(lineage.self_node.created_at.contains('T'));
    assert!(lineage.self_node.created_at.ends_with('Z'));
}

#[test]
fn test_contract_blake3_hash_format_consistency() {
    // Validate BLAKE3 hash format across endpoints
    let adapters_json =
        include_str!("../../../tests/training/datasets/api-contracts/adapters_list.json");
    let adapters: AdaptersListResponse = from_str(adapters_json).unwrap();

    for adapter in &adapters.adapters {
        assert!(
            adapter.hash_b3.starts_with("blake3:"),
            "Hash should start with blake3: prefix"
        );
        assert_eq!(
            adapter.hash_b3.len(),
            71,
            "BLAKE3 hash should be 71 chars (blake3: + 64 hex)"
        );
    }

    let routing_json =
        include_str!("../../../tests/training/datasets/api-contracts/routing_decisions.json");
    let routing: RoutingDecisionsResponse = from_str(routing_json).unwrap();

    for decision in &routing.decisions {
        if let Some(ref hash) = decision.stack_hash {
            assert!(hash.starts_with("blake3:"));
            assert_eq!(hash.len(), 71);
        }
    }
}

#[test]
fn test_contract_pagination_consistency() {
    // Validate pagination fields across list endpoints
    let adapters_json =
        include_str!("../../../tests/training/datasets/api-contracts/adapters_list.json");
    let adapters: AdaptersListResponse = from_str(adapters_json).unwrap();

    assert_eq!(adapters.page, 1);
    assert_eq!(adapters.page_size, 10);
    assert_eq!(adapters.total, 3);
    assert_eq!(adapters.adapters.len(), adapters.total);

    let routing_json =
        include_str!("../../../tests/training/datasets/api-contracts/routing_decisions.json");
    let routing: RoutingDecisionsResponse = from_str(routing_json).unwrap();

    assert_eq!(routing.page, 1);
    assert_eq!(routing.page_size, 50);
    assert_eq!(routing.total, 3);
    assert_eq!(routing.decisions.len(), routing.total);
}

// ============================================================================
// JSON Schema Validation Tests
// ============================================================================

#[test]
fn test_all_contract_files_are_valid_json() {
    let files = [
        include_str!("../../../tests/training/datasets/cli_contract/adapters_list.json"),
        include_str!("../../../tests/training/datasets/cli_contract/adapter_lineage.json"),
        include_str!("../../../tests/training/datasets/routing/routing_decisions.json"),
        include_str!("../../../tests/training/datasets/metrics/healthz_basic.json"),
        include_str!("../../../tests/training/datasets/metrics/healthz_all.json"),
        include_str!("../../../tests/training/datasets/metrics/healthz_degraded.json"),
        include_str!("../../../tests/training/datasets/metrics/healthz_router.json"),
    ];

    for (i, json) in files.iter().enumerate() {
        serde_json::from_str::<Value>(json)
            .unwrap_or_else(|e| panic!("File {} is not valid JSON: {}", i, e));
    }
}
