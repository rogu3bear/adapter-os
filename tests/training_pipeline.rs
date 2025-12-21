#![cfg(all(test, feature = "extended-tests"))]

//! Integration tests for micro-LoRA training pipeline
//!
//! Tests dataset generation, training loop, quantization, and packaging.

use adapteros_lora_worker::{
    DatasetGenerator, LoRAQuantizer, MicroLoRATrainer, TrainingConfig, TrainingExample,
};
use std::collections::HashMap;

#[test]
fn test_dataset_generator_tokenization() {
    let gen = DatasetGenerator::default();
    let tokens = gen.tokenize("hello world");

    assert!(!tokens.is_empty());
    assert_eq!(tokens.len(), "hello world".len());
}

#[test]
fn test_dataset_generator_creates_pairs() {
    let gen = DatasetGenerator::new(10, 1);
    let old_tokens: Vec<u32> = (0..20).collect();
    let new_tokens: Vec<u32> = (0..20).collect();

    let pairs = gen.create_pairs(&old_tokens, &new_tokens);

    assert!(!pairs.is_empty());
    assert!(pairs[0].0.len() <= 10);
    assert!(pairs[0].1.len() <= 10);
}

#[test]
fn test_dataset_validation() {
    let gen = DatasetGenerator::default();
    let examples = vec![TrainingExample {
        input: vec![1, 2, 3],
        target: vec![4, 5, 6],
        metadata: HashMap::new(),
        weight: 1.0,
    }];

    assert!(gen.validate_examples(&examples).is_ok());
}

#[test]
fn test_dataset_validation_empty() {
    let gen = DatasetGenerator::default();
    let examples: Vec<TrainingExample> = vec![];

    assert!(gen.validate_examples(&examples).is_err());
}

#[test]
fn test_training_config_default() {
    let config = TrainingConfig::default();

    assert_eq!(config.rank, 4);
    assert_eq!(config.alpha, 16.0);
    assert_eq!(config.batch_size, 8);
    assert_eq!(config.epochs, 3);
}

#[tokio::test]
async fn test_training_loop_small_dataset() {
    let config = TrainingConfig {
        rank: 2,
        hidden_dim: 64,
        batch_size: 2,
        epochs: 1,
        learning_rate: 0.01,
        ..Default::default()
    };

    let trainer = MicroLoRATrainer::new(config);
    let examples = vec![
        TrainingExample {
            input: vec![1, 2, 3],
            target: vec![4, 5, 6],
            metadata: HashMap::new(),
            weight: 1.0,
        },
        TrainingExample {
            input: vec![7, 8, 9],
            target: vec![10, 11, 12],
            metadata: HashMap::new(),
            weight: 1.0,
        },
    ];

    let result = trainer.train(&examples).await;

    assert!(result.is_ok());
    let result = result.unwrap();
    assert!(result.final_loss >= 0.0);
    // Use microsecond precision to verify training actually ran
    assert!(
        result.training_time_us > 0,
        "Training time should be positive, got: {}us",
        result.training_time_us
    );
    assert_eq!(result.weights.lora_a.len(), 2);
    assert_eq!(result.weights.lora_b.len(), 64);
}

#[test]
fn test_quantization_round_trip() {
    use adapteros_lora_worker::LoRAWeights;

    let original = LoRAWeights {
        lora_a: vec![vec![0.1, -0.2, 0.3], vec![-0.1, 0.2, -0.3]],
        lora_b: vec![vec![0.5, -0.5], vec![0.4, -0.4], vec![0.3, -0.3]],
    };

    // Quantize
    let quantized = LoRAQuantizer::quantize_to_q15(&original);

    // Verify structure
    assert_eq!(quantized.lora_a_q15.len(), 2);
    assert_eq!(quantized.lora_b_q15.len(), 3);
    assert_eq!(quantized.scale_a.len(), 2);
    assert_eq!(quantized.scale_b.len(), 3);

    // Dequantize
    let dequantized = LoRAQuantizer::dequantize_from_q15(&quantized);

    // Check structure matches
    assert_eq!(dequantized.lora_a.len(), original.lora_a.len());
    assert_eq!(dequantized.lora_b.len(), original.lora_b.len());

    // Calculate error
    let error = LoRAQuantizer::calculate_error(&original, &quantized);
    assert!(error < 0.01, "Quantization error too high: {}", error);
}

#[test]
fn test_quantization_value_conversion() {
    use adapteros_lora_worker::LoRAWeights;

    let original = LoRAWeights {
        lora_a: vec![vec![0.0, 0.5, -0.5, 1.0, -1.0]],
        lora_b: vec![vec![0.0]],
    };

    let quantized = LoRAQuantizer::quantize_to_q15(&original);
    let dequantized = LoRAQuantizer::dequantize_from_q15(&quantized);

    // Check that values are close
    for (orig, deq) in original.lora_a[0].iter().zip(dequantized.lora_a[0].iter()) {
        let diff = (orig - deq).abs();
        assert!(diff < 0.01, "Value mismatch: {} vs {}", orig, deq);
    }
}

#[tokio::test]
async fn test_end_to_end_training_and_quantization() {
    let config = TrainingConfig {
        rank: 2,
        hidden_dim: 32,
        batch_size: 2,
        epochs: 1,
        learning_rate: 0.01,
        ..Default::default()
    };

    let mut trainer = MicroLoRATrainer::new(config).unwrap();
    let examples = vec![TrainingExample {
        input: vec![1, 2, 3],
        target: vec![4, 5, 6],
        metadata: HashMap::new(),
        weight: 1.0,
    }];

    // Train
    let result = trainer.train(&examples).await.unwrap();

    // Quantize
    let quantized = LoRAQuantizer::quantize_to_q15(&result.weights);

    // Verify quantized structure
    assert_eq!(quantized.lora_a_q15.len(), 2);
    assert!(quantized.scale_a.iter().all(|&s| s > 0.0));
}

// Performance target: training < 30s for rank-4, 100 examples
#[tokio::test]
#[ignore = "Expensive benchmark test - run with: cargo test --release -- --ignored test_training_performance_benchmark [tracking: STAB-IGN-0214]"]
async fn test_training_performance_benchmark() {
    use std::time::Instant;

    let config = TrainingConfig {
        rank: 4,
        hidden_dim: 768,
        batch_size: 8,
        epochs: 3,
        learning_rate: 1e-4,
        ..Default::default()
    };

    let mut trainer = MicroLoRATrainer::new(config).unwrap();

    // Generate 100 examples
    let mut examples = Vec::new();
    for i in 0..100 {
        examples.push(TrainingExample {
            input: vec![i as u32; 50],
            target: vec![i as u32 + 1; 50],
            metadata: HashMap::new(),
            weight: 1.0,
        });
    }

    let start = Instant::now();
    let result = trainer.train(&examples).await.unwrap();
    let duration = start.elapsed();

    println!("Training completed in {:?}", duration);
    println!("Final loss: {}", result.final_loss);

    // Should complete in < 30s
    assert!(
        duration.as_secs() < 30,
        "Training took too long: {:?}",
        duration
    );
}
