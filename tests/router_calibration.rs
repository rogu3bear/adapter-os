#![cfg(all(test, feature = "extended-tests"))]

//! Router calibration algorithm validation tests

use adapteros_lora_router::{
    CalibrationDataset, CalibrationSample, Calibrator, OptimizationMethod,
};
use anyhow::Result;
use std::path::PathBuf;

#[test]
fn test_calibration_dataset_creation() -> Result<()> {
    let samples = vec![
        CalibrationSample {
            features: vec![1.0, 0.5, 0.3, 0.2, 0.1],
            ground_truth_adapters: vec![0, 1],
            metadata: serde_json::json!({"prompt": "test prompt 1"}),
        },
        CalibrationSample {
            features: vec![0.8, 0.6, 0.4, 0.3, 0.2],
            ground_truth_adapters: vec![1, 2],
            metadata: serde_json::json!({"prompt": "test prompt 2"}),
        },
    ];

    let dataset = CalibrationDataset { samples };
    assert_eq!(dataset.samples.len(), 2);

    Ok(())
}

#[test]
fn test_train_val_split() -> Result<()> {
    let samples: Vec<CalibrationSample> = (0..100)
        .map(|i| CalibrationSample {
            features: vec![i as f32 / 100.0; 5],
            ground_truth_adapters: vec![i % 3],
            metadata: serde_json::json!({"index": i}),
        })
        .collect();

    let dataset = CalibrationDataset { samples };
    let (train, val) = dataset.train_val_split(0.8);

    assert_eq!(train.samples.len(), 80);
    assert_eq!(val.samples.len(), 20);

    Ok(())
}

#[test]
fn test_calibration_with_synthetic_data() -> Result<()> {
    // Create synthetic calibration data with known optimal weights
    let mut samples = Vec::new();

    // Language feature strongly predicts adapter 0
    for _ in 0..20 {
        samples.push(CalibrationSample {
            features: vec![1.0, 0.0, 0.0, 0.0, 0.0], // High language score
            ground_truth_adapters: vec![0],
            metadata: serde_json::json!({}),
        });
    }

    // Framework feature strongly predicts adapter 1
    for _ in 0..20 {
        samples.push(CalibrationSample {
            features: vec![0.0, 1.0, 0.0, 0.0, 0.0], // High framework score
            ground_truth_adapters: vec![1],
            metadata: serde_json::json!({}),
        });
    }

    let dataset = CalibrationDataset { samples };
    let (train_set, val_set) = dataset.train_val_split(0.8);

    let calibrator = Calibrator::new(train_set, OptimizationMethod::GridSearch, 3);
    let weights = calibrator.train()?;

    // Validate that calibrated weights make sense
    assert!(weights.total_weight() > 0.0);
    assert!(weights.language_weight >= 0.0);
    assert!(weights.framework_weight >= 0.0);

    // Validate on validation set
    let val_calibrator = Calibrator::new(val_set, OptimizationMethod::GridSearch, 3);
    let metrics = val_calibrator.validate(&weights);

    // Should have decent accuracy on this simple synthetic data
    assert!(metrics.accuracy > 0.5, "Accuracy should be > 0.5");

    Ok(())
}

#[test]
fn test_weights_save_load() -> Result<()> {
    let calibrator = Calibrator::new(
        CalibrationDataset {
            samples: Vec::new(),
        },
        OptimizationMethod::GridSearch,
        3,
    );

    // Create test weights
    let weights = adapteros_lora_router::RouterWeights::new(0.3, 0.25, 0.2, 0.15, 0.1);

    // Save to temp file
    let temp_dir = tempfile::TempDir::with_prefix("aos-test-calibration-")?;
    let weights_path = temp_dir.path().join("test_weights.json");
    weights.save(&weights_path)?;

    // Load and verify
    let loaded_weights = adapteros_lora_router::RouterWeights::load(&weights_path)?;
    assert!((loaded_weights.language_weight - weights.language_weight).abs() < 0.001);
    assert!((loaded_weights.framework_weight - weights.framework_weight).abs() < 0.001);

    Ok(())
}

#[test]
fn test_validation_metrics() -> Result<()> {
    // Create a simple dataset where we know the expected metrics
    let samples = vec![
        CalibrationSample {
            features: vec![1.0, 0.0, 0.0, 0.0, 0.0],
            ground_truth_adapters: vec![0],
            metadata: serde_json::json!({}),
        },
        CalibrationSample {
            features: vec![0.0, 1.0, 0.0, 0.0, 0.0],
            ground_truth_adapters: vec![1],
            metadata: serde_json::json!({}),
        },
    ];

    let dataset = CalibrationDataset { samples };
    let calibrator = Calibrator::new(dataset, OptimizationMethod::GridSearch, 2);

    // Use default weights for validation
    let weights = adapteros_lora_router::RouterWeights::default();
    let metrics = calibrator.validate(&weights);

    // Check that all metrics are in valid ranges
    assert!(metrics.accuracy >= 0.0 && metrics.accuracy <= 1.0);
    assert!(metrics.precision >= 0.0 && metrics.precision <= 1.0);
    assert!(metrics.recall >= 0.0 && metrics.recall <= 1.0);
    assert!(metrics.f1_score >= 0.0 && metrics.f1_score <= 1.0);
    assert!(metrics.mrr >= 0.0 && metrics.mrr <= 1.0);

    Ok(())
}
