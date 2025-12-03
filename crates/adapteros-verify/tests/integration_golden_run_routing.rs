//! Integration tests for golden run routing decision capture and verification

use adapteros_core::B3Hash;
use adapteros_telemetry::events::{RouterCandidate, RouterDecisionEvent};
use adapteros_telemetry::replay::{ReplayBundle, ReplayEvent};
use adapteros_verify::{
    create_golden_run, verify_against_golden, ComparisonConfig, GoldenRunArchive, StrictnessLevel,
};
use std::fs;
use tempfile::TempDir;

/// Create a mock replay bundle with routing decisions
fn create_mock_replay_bundle(routing_decisions: Vec<RouterDecisionEvent>) -> ReplayBundle {
    let mut events = vec![
        // Metadata event (first)
        ReplayEvent {
            event_type: "bundle.metadata".to_string(),
            timestamp: 0,
            event_hash: B3Hash::hash(b"metadata"),
            payload: serde_json::json!({
                "cpid": "test-cpid",
                "plan_id": "test-plan",
                "seed_global": "0000000000000000000000000000000000000000000000000000000000000001"
            }),
        },
    ];

    // Add routing decision events
    for decision in routing_decisions {
        events.push(ReplayEvent {
            event_type: "router.decision".to_string(),
            timestamp: (decision.step as u128) * 1000,
            event_hash: B3Hash::hash(format!("router-{}", decision.step).as_bytes()),
            payload: serde_json::to_value(&decision).unwrap(),
        });
    }

    ReplayBundle {
        cpid: "test-cpid".to_string(),
        plan_id: "test-plan".to_string(),
        seed_global: B3Hash::from_hex(
            "0000000000000000000000000000000000000000000000000000000000000001",
        )
        .unwrap(),
        events,
        rng_checkpoints: Vec::new(),
    }
}

/// Create test routing decisions
fn create_test_routing_decisions() -> Vec<RouterDecisionEvent> {
    vec![
        RouterDecisionEvent {
            step: 0,
            input_token_id: Some(42),
            candidate_adapters: vec![
                RouterCandidate {
                    adapter_idx: 0,
                    raw_score: 0.8,
                    gate_q15: 26214, // ~0.8 in Q15
                },
                RouterCandidate {
                    adapter_idx: 1,
                    raw_score: 0.2,
                    gate_q15: 6553, // ~0.2 in Q15
                },
            ],
            entropy: 0.5,
            tau: 0.1,
            entropy_floor: 0.01,
            stack_hash: Some("test-stack-hash".to_string()),
            stack_id: None,
            stack_version: None,
        },
        RouterDecisionEvent {
            step: 1,
            input_token_id: Some(43),
            candidate_adapters: vec![
                RouterCandidate {
                    adapter_idx: 0,
                    raw_score: 0.6,
                    gate_q15: 19660,
                },
                RouterCandidate {
                    adapter_idx: 2,
                    raw_score: 0.4,
                    gate_q15: 13107,
                },
            ],
            entropy: 0.6,
            tau: 0.1,
            entropy_floor: 0.01,
            stack_hash: Some("test-stack-hash".to_string()),
            stack_id: None,
            stack_version: None,
        },
    ]
}

#[tokio::test]
async fn test_golden_run_routing_save_load() {
    // Simplified test that focuses on save/load of routing decisions
    let temp_dir = TempDir::new().unwrap();
    let golden_dir = temp_dir.path().join("golden-001");

    // Create a golden run archive with routing decisions
    let routing_decisions = create_test_routing_decisions();

    use adapteros_verify::GoldenRunMetadata;
    let metadata = GoldenRunMetadata::new(
        "test-cpid".to_string(),
        "test-plan".to_string(),
        "1.75.0".to_string(),
        vec!["adapter-001".to_string()],
        B3Hash::from_hex("0000000000000000000000000000000000000000000000000000000000000001")
            .unwrap(),
    );

    use adapteros_verify::EpsilonStatistics;
    let epsilon_stats = EpsilonStatistics {
        layer_stats: std::collections::HashMap::new(),
    };

    let bundle_hash = B3Hash::hash(b"test-bundle");

    let archive = GoldenRunArchive::with_routing_decisions(
        metadata,
        epsilon_stats,
        bundle_hash,
        routing_decisions.clone(),
    );

    // Verify routing decisions in archive
    assert_eq!(
        archive.routing_decisions.len(),
        2,
        "Should have 2 routing decisions"
    );
    assert_eq!(archive.routing_decisions[0].step, 0);
    assert_eq!(archive.routing_decisions[1].step, 1);

    // Save to disk
    archive.save(&golden_dir).unwrap();

    // Verify routing_decisions.json exists
    assert!(golden_dir.join("routing_decisions.json").exists());

    // Load back
    let loaded = GoldenRunArchive::load(&golden_dir).unwrap();

    // Verify routing decisions preserved
    assert_eq!(loaded.routing_decisions.len(), 2);
    assert_eq!(loaded.routing_decisions[0].step, 0);
    assert_eq!(loaded.routing_decisions[0].entropy, 0.5);
    assert_eq!(loaded.routing_decisions[1].step, 1);
    assert_eq!(loaded.routing_decisions[1].entropy, 0.6);

    // Verify comparison works
    use adapteros_verify::{compare_routing_decisions, StrictnessLevel};
    let config = ComparisonConfig {
        strictness: StrictnessLevel::EpsilonTolerant,
        verify_toolchain: false,
        verify_adapters: false,
        verify_device: false,
        verify_signature: false,
    };

    let (matched, divergences) =
        compare_routing_decisions(&loaded.routing_decisions, &routing_decisions, &config);

    assert!(matched, "Routing decisions should match");
    assert_eq!(divergences.len(), 0);
}

#[tokio::test]
async fn test_golden_run_backwards_compatibility() {
    let temp_dir = TempDir::new().unwrap();
    let bundle_path = temp_dir.path().join("bundle.ndjson");
    let golden_dir = temp_dir.path().join("golden-old");

    // Create bundle with routing decisions
    let routing_decisions = create_test_routing_decisions();
    let bundle = create_mock_replay_bundle(routing_decisions);

    let mut bundle_content = String::new();
    for event in &bundle.events {
        bundle_content.push_str(&serde_json::to_string(event).unwrap());
        bundle_content.push('\n');
    }
    fs::write(&bundle_path, &bundle_content).unwrap();

    // Create golden run and save
    let mut archive = create_golden_run(&bundle_path, "1.75.0", &["adapter-001"])
        .await
        .unwrap();

    // Clear routing decisions to simulate old archive
    archive.routing_decisions.clear();
    archive.save(&golden_dir).unwrap();

    // Remove routing_decisions.json to simulate old format
    let routing_file = golden_dir.join("routing_decisions.json");
    if routing_file.exists() {
        fs::remove_file(&routing_file).unwrap();
    }

    // Load old archive (should succeed with empty routing decisions)
    let loaded = GoldenRunArchive::load(&golden_dir).unwrap();
    assert_eq!(loaded.routing_decisions.len(), 0);

    // Verify against bundle (should skip routing check)
    let config = ComparisonConfig::default();
    let report = verify_against_golden(&golden_dir, &bundle_path, &config)
        .await
        .unwrap();

    // Routing check should be skipped (backwards compatible)
    assert!(report.routing_decisions_match);
    assert_eq!(report.routing_decision_count, 0);

    let summary = report.summary();
    assert!(summary.contains("backwards compatible"));
}

#[test]
fn test_routing_decision_serialization() {
    // Test that routing decisions serialize/deserialize correctly
    let decision = RouterDecisionEvent {
        step: 5,
        input_token_id: Some(100),
        candidate_adapters: vec![
            RouterCandidate {
                adapter_idx: 0,
                raw_score: 0.7,
                gate_q15: 22937,
            },
            RouterCandidate {
                adapter_idx: 1,
                raw_score: 0.3,
                gate_q15: 9830,
            },
        ],
        entropy: 0.55,
        tau: 0.1,
        entropy_floor: 0.01,
        stack_hash: Some("abc123".to_string()),
        stack_id: Some("stack-001".to_string()),
        stack_version: Some(1),
    };

    // Serialize
    let json = serde_json::to_string(&decision).unwrap();

    // Deserialize
    let deserialized: RouterDecisionEvent = serde_json::from_str(&json).unwrap();

    // Verify
    assert_eq!(deserialized.step, 5);
    assert_eq!(deserialized.input_token_id, Some(100));
    assert_eq!(deserialized.candidate_adapters.len(), 2);
    assert_eq!(deserialized.entropy, 0.55);
    assert_eq!(deserialized.stack_id, Some("stack-001".to_string()));
}
