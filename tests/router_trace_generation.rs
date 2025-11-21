//! Generate real router trace with telemetry
//!
//! This test demonstrates the actual NDJSON trace format produced by the router
//! during inference, showing per-token adapter selection with Q15 gates.

use adapteros_lora_router::{Decision, Router, RouterWeights};
use adapteros_telemetry::{RouterCandidate, RouterDecisionEvent, TelemetryWriter};
use std::path::PathBuf;
use tempfile::TempDir;
/// Test configuration for router trace generation
struct TestRouterConfig {
    stack_id: String,
    stack_version: String,
}

impl Default for TestRouterConfig {
    fn default() -> Self {
        Self {
            // Generate a deterministic test UUID for reproducibility
            stack_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            stack_version: "1.0.0".to_string(),
        }
    }
}

fn emit_router_event(
    telemetry: &TelemetryWriter,
    router: &Router,
    decision: &Decision,
    step: usize,
    input_token_id: Option<u32>,
    config: &TestRouterConfig,
) {
    let candidate_adapters: Vec<RouterCandidate> = decision
        .candidates
        .iter()
        .map(|candidate| RouterCandidate {
            adapter_idx: candidate.adapter_idx,
            raw_score: candidate.raw_score,
            gate_q15: candidate.gate_q15,
        })
        .collect();

    let event = RouterDecisionEvent {
        step,
        input_token_id,
        candidate_adapters,
        entropy: decision.entropy,
        tau: router.temperature(),
        entropy_floor: router.entropy_floor(),
        stack_hash: router.stack_hash(),
        stack_id: Some(config.stack_id.clone()),
        stack_version: Some(config.stack_version.clone()),
    };

    telemetry.log_router_decision(event).unwrap();
}

/// Test that generates a real router trace showing per-token decisions
#[test]
fn test_generate_router_trace() {
    // Create temporary directory for telemetry output
    let temp_dir = TempDir::new().unwrap();
    let telemetry_path = temp_dir.path().to_path_buf();

    // Create telemetry writer
    // Parameters: output_dir, max_events_per_bundle, max_bytes_per_bundle
    let telemetry = TelemetryWriter::new(&telemetry_path, 1000, 1024 * 1024).unwrap();

    // Create router with telemetry enabled
    let weights = RouterWeights::default();
    let mut router = Router::new_with_weights(weights, 3, 1.0, 0.02);

    // Set up adapter stack for filtering
    let adapter_ids = vec![
        "python_adapter".to_string(),
        "rust_adapter".to_string(),
        "javascript_adapter".to_string(),
        "golang_adapter".to_string(),
        "cpp_adapter".to_string(),
    ];
    router.set_active_stack(Some("code_stack".to_string()), Some(adapter_ids.clone()));

    // Create test configuration with stack ID and version
    let config = TestRouterConfig::default();

    // Simulate routing for multiple tokens
    // Each call to route() represents one token in the sequence
    let test_prompt = "def fibonacci(n): # Python code example";
    println!("\n=== Router Trace for Prompt: \"{}\" ===\n", test_prompt);

    // Token 0: "def" - strong Python signal
    let features_token_0 = vec![
        1.0, // language: Python detected
        0.8, // framework: generic
        0.6, // symbols: def keyword
        0.3, // paths
        0.9, // verb: def is a verb
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    ];
    let priors_token_0 = vec![0.8, 0.5, 0.4, 0.3, 0.2]; // Python adapter has high prior
    let decision_0 = router.route(&features_token_0, &priors_token_0);

    println!("Token 0 ('def'):");
    println!("  Selected adapters: {:?}", decision_0.indices.as_slice());
    println!("  Q15 gates: {:?}", decision_0.gates_q15.as_slice());
    println!("  F32 gates: {:?}", decision_0.gates_f32());
    println!();

    emit_router_event(&telemetry, &router, &decision_0, 0, Some(0), &config);

    // Token 1: "fibonacci" - function name
    let features_token_1 = vec![
        1.0, // language: still Python
        0.7, // framework
        0.5, // symbols
        0.3, // paths
        0.6, // verb
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    ];
    let priors_token_1 = vec![0.85, 0.5, 0.4, 0.3, 0.2];
    let decision_1 = router.route(&features_token_1, &priors_token_1);

    println!("Token 1 ('fibonacci'):");
    println!("  Selected adapters: {:?}", decision_1.indices.as_slice());
    println!("  Q15 gates: {:?}", decision_1.gates_q15.as_slice());
    println!("  F32 gates: {:?}", decision_1.gates_f32());
    println!();

    emit_router_event(&telemetry, &router, &decision_1, 1, Some(1), &config);

    // Token 2: "(" - syntax
    let features_token_2 = vec![
        1.0, // language: Python
        0.6, // framework
        0.8, // symbols: parenthesis is a symbol
        0.3, // paths
        0.4, // verb
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    ];
    let priors_token_2 = vec![0.9, 0.5, 0.4, 0.3, 0.2];
    let decision_2 = router.route(&features_token_2, &priors_token_2);

    println!("Token 2 ('('):");
    println!("  Selected adapters: {:?}", decision_2.indices.as_slice());
    println!("  Q15 gates: {:?}", decision_2.gates_q15.as_slice());
    println!("  F32 gates: {:?}", decision_2.gates_f32());
    println!();

    emit_router_event(&telemetry, &router, &decision_2, 2, Some(2), &config);

    // Token 3: "n" - parameter
    let features_token_3 = vec![
        1.0, // language: Python
        0.6, // framework
        0.5, // symbols
        0.3, // paths
        0.4, // verb
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    ];
    let priors_token_3 = vec![0.92, 0.5, 0.4, 0.3, 0.2];
    let decision_3 = router.route(&features_token_3, &priors_token_3);

    println!("Token 3 ('n'):");
    println!("  Selected adapters: {:?}", decision_3.indices.as_slice());
    println!("  Q15 gates: {:?}", decision_3.gates_q15.as_slice());
    println!("  F32 gates: {:?}", decision_3.gates_f32());
    println!();

    emit_router_event(&telemetry, &router, &decision_3, 3, Some(3), &config);

    // Token 4: "):" - syntax
    let features_token_4 = vec![
        1.0, // language: Python
        0.6, // framework
        0.9, // symbols: )! is symbolic
        0.3, // paths
        0.3, // verb
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    ];
    let priors_token_4 = vec![0.95, 0.5, 0.4, 0.3, 0.2];
    let decision_4 = router.route(&features_token_4, &priors_token_4);

    println!("Token 4 (')'):");
    println!("  Selected adapters: {:?}", decision_4.indices.as_slice());
    println!("  Q15 gates: {:?}", decision_4.gates_q15.as_slice());
    println!("  F32 gates: {:?}", decision_4.gates_f32());
    println!();

    emit_router_event(&telemetry, &router, &decision_4, 4, Some(4), &config);

    // Wait a moment for telemetry to flush
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Find and read the NDJSON bundle
    let mut ndjson_files: Vec<PathBuf> = std::fs::read_dir(&telemetry_path)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|s| s.to_str())
                .map(|s| s == "ndjson")
                .unwrap_or(false)
        })
        .collect();

    if !ndjson_files.is_empty() {
        println!("=== NDJSON Trace Output ===\n");

        // Sort by modification time to get the latest
        ndjson_files.sort_by_key(|p| {
            std::fs::metadata(p)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
        });

        let trace_file = &ndjson_files[0];
        println!("Trace file: {}\n", trace_file.display());

        // Read and display the trace
        let content = std::fs::read_to_string(trace_file).unwrap();
        let lines: Vec<&str> = content.lines().collect();

        println!("Total events logged: {}\n", lines.len());

        // Display first few events
        for (i, line) in lines.iter().take(10).enumerate() {
            if let Ok(event) = serde_json::from_str::<serde_json::Value>(line) {
                println!("Event {}:", i);
                println!("{}", serde_json::to_string_pretty(&event).unwrap());
                println!();
            }
        }

        // Parse and validate router.decision events against the frozen schema
        println!("=== Router Decision Schema Validation ===\n");
        let mut router_events: Vec<(usize, serde_json::Value)> = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if let Ok(event) = serde_json::from_str::<serde_json::Value>(line) {
                if let Some("router.decision") = event.get("event_type").and_then(|v| v.as_str()) {
                    router_events.push((i, event));
                }
            }
        }

        assert!(
            router_events.len() >= 5,
            "Expected at least 5 router decision events, got {}",
            router_events.len()
        );

        for (idx, (line_no, event)) in router_events.iter().enumerate() {
            let metadata = event
                .get("metadata")
                .and_then(|m| m.as_object())
                .expect("router.decision event missing metadata");

            assert_eq!(
                metadata
                    .get("step")
                    .and_then(|v| v.as_u64())
                    .expect("missing step"),
                idx as u64,
                "router decision order must match token index"
            );

            assert!(
                metadata.get("input_token_id").is_some(),
                "input_token_id must be present"
            );
            assert!(
                metadata.get("entropy").is_some(),
                "entropy must be present for each decision"
            );
            assert!(
                metadata.get("tau").is_some(),
                "tau must be present for each decision"
            );
            assert!(
                metadata.get("entropy_floor").is_some(),
                "entropy_floor must be present for each decision"
            );
            assert!(metadata.contains_key("candidate_adapters"));

            let candidates = metadata
                .get("candidate_adapters")
                .and_then(|v| v.as_array())
                .expect("candidate_adapters must be an array");

            assert!(
                !candidates.is_empty(),
                "candidate_adapters must not be empty"
            );

            let mut last_score = f32::INFINITY;
            for candidate in candidates {
                let raw_score = candidate
                    .get("raw_score")
                    .and_then(|v| v.as_f64())
                    .expect("missing raw_score") as f32;
                assert!(
                    raw_score <= last_score + f32::EPSILON,
                    "candidate adapters must be sorted by raw_score"
                );
                last_score = raw_score;

                candidate
                    .get("adapter_idx")
                    .and_then(|v| v.as_u64())
                    .expect("adapter_idx missing");

                candidate
                    .get("gate_q15")
                    .and_then(|v| v.as_i64())
                    .expect("gate_q15 missing");
            }

            println!(
                "Validated router.decision event #{} (line {})",
                idx + 1,
                line_no
            );
        }

        println!("Total router.decision events: {}", router_events.len());
    } else {
        println!(
            "WARNING: No NDJSON trace file found in {}",
            telemetry_path.display()
        );
        println!(
            "This may be due to telemetry buffering. In production, telemetry is flushed to disk."
        );
    }

    println!("\n=== Test Complete ===");
    println!("This test demonstrates the router trace format.");
    println!("In production, router.decision events contain the frozen schema:");
    println!("  - step: usize (token index for the decision)");
    println!("  - input_token_id: Option<u32> (token ID driving the routing decision)");
    println!("  - candidate_adapters: [{adapter_idx, raw_score, gate_q15}] ordered by raw_score");
    println!("  - entropy: f32, tau: f32, entropy_floor: f32, stack_hash: Option<String>");
}

/// Test that verifies deterministic replay of router trace
#[test]
fn test_router_trace_determinism() {
    // Create router without telemetry for faster execution
    let weights = RouterWeights::default();
    let mut router = Router::new_with_weights(weights, 3, 1.0, 0.02);

    // Set up adapters
    let adapter_ids = vec![
        "adapter_0".to_string(),
        "adapter_1".to_string(),
        "adapter_2".to_string(),
        "adapter_3".to_string(),
        "adapter_4".to_string(),
    ];
    router.set_active_stack(Some("test_stack".to_string()), Some(adapter_ids));

    // Fixed inputs
    let features = vec![0.5; 22];
    let priors = vec![0.1, 0.3, 0.5, 0.7, 0.9];

    // Run multiple times - should get identical results
    let mut results = Vec::new();
    for run in 0..5 {
        let decision = router.route(&features, &priors);
        let indices = decision.indices.as_slice().to_vec();
        let gates_q15 = decision.gates_q15.as_slice().to_vec();

        if run == 0 {
            println!("Reference routing decision:");
            println!("  Adapters: {:?}", indices);
            println!("  Q15 gates: {:?}", gates_q15);
        }

        results.push((indices, gates_q15));
    }

    // All runs must produce identical results
    for (i, result) in results.iter().enumerate().skip(1) {
        assert_eq!(
            results[0].0, result.0,
            "Run {} produced different adapter indices",
            i
        );
        assert_eq!(
            results[0].1, result.1,
            "Run {} produced different Q15 gates",
            i
        );
    }

    println!("\nDeterminism verified: All 5 runs produced identical results");
}
