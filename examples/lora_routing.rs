//! K-sparse LoRA routing example
//!
//! This example demonstrates:
//! 1. K-sparse adapter selection
//! 2. Q15 gate quantization
//! 3. Entropy floor application
//! 4. Multi-adapter composition
//!
//! # Usage
//!
//! ```bash
//! cargo run --example lora_routing
//! ```

use mplora_mlx::routing::{apply_entropy_floor, select_top_k};

fn main() {
    println!("🎯 K-Sparse LoRA Routing Example\n");

    // Simulate router logits for 8 adapters
    let router_logits = vec![
        2.5, // Adapter 0: High activation
        0.3, // Adapter 1: Low activation
        3.1, // Adapter 2: Highest activation
        0.8, // Adapter 3: Low activation
        2.0, // Adapter 4: Medium-high activation
        0.1, // Adapter 5: Very low activation
        1.5, // Adapter 6: Medium activation
        0.5, // Adapter 7: Low activation
    ];

    println!("📊 Router logits:");
    for (i, &logit) in router_logits.iter().enumerate() {
        println!("   Adapter {}: {:.2}", i, logit);
    }

    // Test different K values
    for k in [1, 3, 5] {
        println!("\n🔍 K-sparse selection with K={}:", k);

        let (indices, gates) = select_top_k(&router_logits, k);

        println!("   Selected adapters:");
        for (idx, &adapter_id) in indices.iter().enumerate() {
            let gate_q15 = gates[idx];
            let gate_prob = gate_q15 as f32 / 32767.0;
            println!(
                "     {}. Adapter {} - Gate: {} (Q15) = {:.4} (prob)",
                idx + 1,
                adapter_id,
                gate_q15,
                gate_prob
            );
        }

        // Verify gates sum to ~32767
        let gate_sum: u32 = gates.iter().map(|&g| g as u32).sum();
        println!("   Gate sum: {} (target: 32767)", gate_sum);
    }

    // Demonstrate entropy floor
    println!("\n🌡️  Entropy Floor Example:");

    // Highly peaked distribution (low entropy)
    let peaked_gates = vec![30000, 1500, 1267]; // One adapter dominates
    println!("\n   Original gates (low entropy):");
    for (i, &gate) in peaked_gates.iter().enumerate() {
        println!(
            "     Adapter {}: {} ({:.4})",
            i,
            gate,
            gate as f32 / 32767.0
        );
    }

    // Apply entropy floor
    let entropy_floor = 0.5; // Force more uniform distribution
    let adjusted_gates = apply_entropy_floor(&peaked_gates, entropy_floor);

    println!("\n   Adjusted gates (entropy floor = {}):", entropy_floor);
    for (i, &gate) in adjusted_gates.iter().enumerate() {
        println!(
            "     Adapter {}: {} ({:.4})",
            i,
            gate,
            gate as f32 / 32767.0
        );
    }

    // Calculate entropy
    let original_entropy = calculate_entropy(&peaked_gates);
    let adjusted_entropy = calculate_entropy(&adjusted_gates);

    println!("\n   Entropy comparison:");
    println!("     Original: {:.4}", original_entropy);
    println!("     Adjusted: {:.4}", adjusted_entropy);
    println!(
        "     Improvement: {:.4}",
        adjusted_entropy - original_entropy
    );

    println!("\n✅ Example complete!");
    println!("\n💡 Key insights:");
    println!("   - Higher K allows more adapter diversity");
    println!("   - Q15 quantization ensures deterministic gates");
    println!("   - Entropy floor prevents single-adapter collapse");
    println!("   - Gate probabilities sum to 1.0 (Q15 sum ≈ 32767)");
}

fn calculate_entropy(gates: &[u16]) -> f32 {
    let total: f32 = gates.iter().map(|&g| g as f32).sum();
    let mut entropy = 0.0;

    for &gate in gates {
        if gate > 0 {
            let p = gate as f32 / total;
            entropy -= p * p.ln();
        }
    }

    entropy
}
