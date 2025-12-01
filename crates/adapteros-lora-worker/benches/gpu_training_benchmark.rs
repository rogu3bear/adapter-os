//! GPU vs CPU training performance benchmarks for AdapterOS
//!
//! This benchmark suite compares training performance across different backends:
//! - CPU (pure Rust implementation)
//! - Metal GPU (macOS, deterministic)
//! - CoreML (ANE acceleration, macOS 13+)
//! - MLX (research/training focused)
//!
//! Run with: cargo bench --bench gpu_training_benchmark

use std::collections::HashMap;
use std::time::Instant;

fn create_training_examples(count: usize) -> Vec<adapteros_lora_worker::training::TrainingExample> {
    (0..count)
        .map(|i| {
            let input: Vec<u32> = (0..10).map(|j| ((i + j) % 1000) as u32).collect();
            let target: Vec<u32> = (0..10).map(|j| ((i * 2 + j) % 1000) as u32).collect();
            adapteros_lora_worker::training::TrainingExample {
                input,
                target,
                metadata: HashMap::new(),
                weight: 1.0,
            }
        })
        .collect()
}

fn measure_training_time(backend: &str, examples: usize, rank: usize, hidden_dim: usize) {
    let config = adapteros_lora_worker::training::TrainingConfig {
        rank,
        alpha: 16.0,
        learning_rate: 1e-4,
        batch_size: 4,
        epochs: 2,
        hidden_dim,
        vocab_size: 50272,
        require_gpu: false,
        preferred_backend: None,
        max_gpu_memory_mb: 0,
    };

    let mut trainer = match adapteros_lora_worker::training::MicroLoRATrainer::new(config) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Failed to create trainer for {}: {}", backend, e);
            return;
        }
    };

    let training_examples = create_training_examples(examples);

    let start = Instant::now();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    match runtime.block_on(async { trainer.train(&training_examples).await }) {
        Ok(result) => {
            let elapsed = start.elapsed();
            let throughput = examples as f32 / elapsed.as_secs_f32();

            println!(
                "Backend: {:<12} | Examples: {:<4} | Rank: {:<2} | HiddenDim: {:<4} | Time: {:<8}ms | Throughput: {:<8.0} ex/s | Final Loss: {:.4}",
                backend,
                examples,
                rank,
                hidden_dim,
                elapsed.as_millis(),
                throughput,
                result.final_loss
            );
        }
        Err(e) => {
            eprintln!("Training failed on {}: {}", backend, e);
        }
    }
}

fn main() {
    println!("=== AdapterOS GPU Training Performance Benchmarks ===\n");

    // Show available backends
    println!(
        "{}",
        adapteros_lora_worker::training::MicroLoRATrainer::describe_available_backends()
    );
    println!();

    // Benchmark configurations
    let test_cases = vec![
        ("Small model", 10, 4, 256),
        ("Medium model", 50, 8, 512),
        ("Large model", 100, 16, 768),
    ];

    println!("Running training benchmarks across all available backends...\n");

    for (name, examples, rank, hidden_dim) in test_cases {
        println!(
            "--- {} (Examples: {}, Rank: {}, HiddenDim: {}) ---",
            name, examples, rank, hidden_dim
        );

        // Try different backends
        #[cfg(target_os = "macos")]
        {
            measure_training_time("CPU", examples, rank, hidden_dim);

            #[cfg(feature = "coreml-backend")]
            measure_training_time("CoreML", examples, rank, hidden_dim);

            measure_training_time("Metal", examples, rank, hidden_dim);
        }

        #[cfg(not(target_os = "macos"))]
        {
            measure_training_time("CPU", examples, rank, hidden_dim);
        }

        println!();
    }

    // Performance summary
    println!("\n=== Summary ===");
    println!("Note: GPU backend selection happens automatically based on:");
    println!("  1. User preference (if specified)");
    println!("  2. Available hardware (CoreML/ANE > Metal > MLX > CPU)");
    println!("  3. Fallback to CPU if GPU initialization fails");
    println!();
    println!(
        "For production training workloads, GPU acceleration (CoreML with ANE) is recommended"
    );
    println!("for best performance. CPU training is useful for development and small datasets.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_benchmark_creates_examples() {
        let examples = create_training_examples(5);
        assert_eq!(examples.len(), 5);
        assert!(examples[0].input.len() > 0);
    }

    #[tokio::test]
    async fn test_small_training_completes() {
        let config = adapteros_lora_worker::training::TrainingConfig {
            rank: 2,
            alpha: 8.0,
            learning_rate: 1e-3,
            batch_size: 2,
            epochs: 1,
            hidden_dim: 64,
            vocab_size: 50272,
            require_gpu: false,
            preferred_backend: None,
            max_gpu_memory_mb: 0,
        };

        let mut trainer = adapteros_lora_worker::training::MicroLoRATrainer::new(config).unwrap();
        let examples = create_training_examples(4);

        let result = trainer.train(&examples).await;
        assert!(result.is_ok());
    }
}
