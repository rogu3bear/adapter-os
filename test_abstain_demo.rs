// Demonstration of abstain detection feature
// This file shows how to use the abstain detection API

use adapteros_lora_router::{Router, RouterWeights, AdapterInfo};
use adapteros_policy::packs::router::RouterConfig;
use std::sync::Arc;

fn main() {
    println!("=== Abstain Detection Demo ===\n");

    // Example 1: Creating router with abstain thresholds from policy
    println!("1. Creating router with policy-configured abstain thresholds:");
    let mut policy_config = RouterConfig::default();
    policy_config.abstain_entropy_threshold = Some(0.9);
    policy_config.abstain_confidence_threshold = Some(0.3);

    let router = Router::new_with_policy_config(
        RouterWeights::default(),
        3,
        1.0,
        &policy_config,
    );

    println!("   Entropy threshold: {:?}", router.abstain_entropy_threshold);
    println!("   Confidence threshold: {:?}\n", router.abstain_confidence_threshold);

    // Example 2: Setting thresholds programmatically
    println!("2. Setting abstain thresholds programmatically:");
    let mut router2 = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);
    router2.set_abstain_thresholds(Some(0.85), Some(0.25));
    println!("   Entropy threshold: {:?}", router2.abstain_entropy_threshold);
    println!("   Confidence threshold: {:?}\n", router2.abstain_confidence_threshold);

    // Example 3: How abstain detection works during routing
    println!("3. Abstain detection during routing:");
    println!("   - High entropy (> threshold): Router is uncertain about adapter selection");
    println!("   - Low confidence (max gate < threshold): No strong adapter preference");
    println!("   - Events are emitted via TelemetryWriter.log_abstain() when triggered");
    println!("\n   To enable telemetry:");
    println!("     let telemetry_writer = Arc::new(TelemetryWriter::new(...));");
    println!("     router.set_abstain_telemetry_writer(telemetry_writer);\n");

    println!("=== Key Components ===");
    println!("RouterConfig fields:");
    println!("  - abstain_entropy_threshold: Option<f32>");
    println!("  - abstain_confidence_threshold: Option<f32>");
    println!("\nRouter methods:");
    println!("  - set_abstain_thresholds(entropy: Option<f32>, confidence: Option<f32>)");
    println!("  - set_abstain_telemetry_writer(Arc<TelemetryWriter>)");
    println!("\nAbstainEvent factory methods:");
    println!("  - AbstainEvent::high_entropy(entropy, threshold)");
    println!("  - AbstainEvent::low_confidence(max_gate, threshold)");
}
