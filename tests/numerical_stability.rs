#![cfg(all(test, feature = "extended-tests"))]

//! Numerical stability tests for quantization noise tracking
//!
//! This test suite validates that:
//! 1. Error accumulation remains bounded across multiple runs
//! 2. Identical runs produce identical epsilon logs
//! 3. Threshold violations are properly detected
//! 4. Telemetry integration works correctly

<<<<<<< HEAD
#![cfg(feature = "numerics-experimental")]

=======
>>>>>>> integration-branch
use adapteros_lora_kernel_api::{IoBuffers, RouterRing};
use adapteros_lora_kernel_mtl::{MetalKernels, NoiseTracker, NoiseTrackingConfig};
use adapteros_numerics::noise::{
    measure_error, EpsilonStats, GlobalStabilityReport, NumericsError, Tensor,
};
use adapteros_telemetry::TelemetryWriter;
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::TempDir;

/// Test that error accumulation remains bounded across 100 runs
#[test]
fn test_error_accumulation_bounded() {
    let config = NoiseTrackingConfig {
        enabled: true,
        error_threshold: 1e-6,
        strict_mode: false,
        enable_reference: true,
        max_layers_per_step: 10,
    };

    let mut tracker = NoiseTracker::new(config, None);
    let mut total_error = 0.0;
    let mut max_error = 0.0;

    // Simulate 100 kernel steps
    for step in 0..100 {
        // Create test data with controlled noise
        let base_data: Vec<f32> = (0..1000).map(|i| (i as f32) * 0.001).collect();

        let noise_factor = 0.0001 * (step as f32); // Gradually increasing noise
        let quantized_data: Vec<f32> = base_data.iter().map(|&x| x + (x * noise_factor)).collect();

        let reference_data: Vec<f32> = base_data.clone();

        // Track error for this step
        tracker
            .track_layer_error(
                &format!("layer_{}", step),
                &quantized_data,
                Some(&reference_data),
            )
            .unwrap();

        tracker.track_step().unwrap();

        // Accumulate error metrics
        let report = tracker.get_stability_report();
        total_error += report.total_l2_error;
        max_error = max_error.max(report.max_layer_error);
    }

    // Verify error remains bounded
    assert!(
        total_error < 1.0,
        "Total error {} exceeds bound",
        total_error
    );
    assert!(max_error < 0.1, "Max error {} exceeds bound", max_error);

    // Verify stability score is reasonable
    let final_report = tracker.get_stability_report();
    assert!(
        final_report.stability_score() < 10.0,
        "Stability score {} too high",
        final_report.stability_score()
    );
}

/// Test that identical runs produce identical epsilon logs
#[test]
fn test_identical_runs_produce_identical_epsilon() {
    let config = NoiseTrackingConfig::default();

    // First run
    let mut tracker1 = NoiseTracker::new(config.clone(), None);
    let test_data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
    let reference_data = vec![1.01, 1.99, 3.01, 3.99, 5.01];

    tracker1
        .track_layer_error("test_layer", &test_data, Some(&reference_data))
        .unwrap();
    tracker1.track_step().unwrap();

    // Second run with identical data
    let mut tracker2 = NoiseTracker::new(config, None);
    tracker2
        .track_layer_error("test_layer", &test_data, Some(&reference_data))
        .unwrap();
    tracker2.track_step().unwrap();

    // Compare epsilon statistics
    let stats1 = tracker1.get_layer_stats("test_layer").unwrap();
    let stats2 = tracker2.get_layer_stats("test_layer").unwrap();

    assert_eq!(stats1.l2_error, stats2.l2_error);
    assert_eq!(stats1.max_error, stats2.max_error);
    assert_eq!(stats1.mean_error, stats2.mean_error);
    assert_eq!(stats1.element_count, stats2.element_count);

    // Compare global reports
    let report1 = tracker1.get_stability_report();
    let report2 = tracker2.get_stability_report();

    assert_eq!(report1.total_l2_error, report2.total_l2_error);
    assert_eq!(report1.max_layer_error, report2.max_layer_error);
    assert_eq!(report1.stability_score(), report2.stability_score());
}

/// Test threshold violation detection
#[test]
fn test_threshold_violation_detection() {
    // Test strict mode - should panic on threshold violation
    let strict_config = NoiseTrackingConfig {
        enabled: true,
        error_threshold: 1e-6,
        strict_mode: true,
        enable_reference: true,
        max_layers_per_step: 10,
    };

    let mut strict_tracker = NoiseTracker::new(strict_config, None);

    // Create data with large error
    let quantized_data = vec![1.0, 2.0, 3.0];
    let reference_data = vec![2.0, 3.0, 4.0]; // Large difference

    let result =
        strict_tracker.track_layer_error("test_layer", &quantized_data, Some(&reference_data));

    assert!(
        result.is_err(),
        "Strict mode should fail on threshold violation"
    );

    // Test warning mode - should not panic
    let warning_config = NoiseTrackingConfig {
        enabled: true,
        error_threshold: 1e-6,
        strict_mode: false,
        enable_reference: true,
        max_layers_per_step: 10,
    };

    let mut warning_tracker = NoiseTracker::new(warning_config, None);

    let result =
        warning_tracker.track_layer_error("test_layer", &quantized_data, Some(&reference_data));

    assert!(
        result.is_ok(),
        "Warning mode should not fail on threshold violation"
    );

    // Verify threshold violation is recorded
    let report = warning_tracker.get_stability_report();
    assert!(
        !report.threshold_violations.is_empty(),
        "Threshold violation should be recorded"
    );
}

/// Test telemetry integration
#[test]
fn test_telemetry_integration() {
    let temp_dir = TempDir::new().unwrap();
    let telemetry_path = temp_dir.path().join("test_telemetry.jsonl");

    let telemetry = Arc::new(
        TelemetryWriter::new(
            "test_node".to_string(),
            "test_tenant".to_string(),
            "test_plan".to_string(),
            "test_cpid".to_string(),
            telemetry_path.to_string_lossy().to_string(),
        )
        .unwrap(),
    );

    let config = NoiseTrackingConfig::default();
    let mut tracker = NoiseTracker::new(config, Some(telemetry.clone()));

    // Track some noise
    let quantized_data = vec![1.0, 2.0, 3.0];
    let reference_data = vec![1.01, 1.99, 3.01];

    tracker
        .track_layer_error("test_layer", &quantized_data, Some(&reference_data))
        .unwrap();
    tracker.track_step().unwrap();

    // Give telemetry time to write
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Verify telemetry file was created and contains data
    assert!(telemetry_path.exists(), "Telemetry file should exist");

    let content = std::fs::read_to_string(&telemetry_path).unwrap();
    assert!(
        content.contains("kernel.noise"),
        "Should contain kernel noise event"
    );
    assert!(
        content.contains("kernel.step"),
        "Should contain kernel step event"
    );
    assert!(content.contains("test_layer"), "Should contain layer ID");
}

/// Test error measurement functions directly
#[test]
fn test_error_measurement_functions() {
    // Test basic error measurement
    let ref_tensor = Tensor::new(vec![1.0, 2.0, 3.0], vec![3]);
    let quant_tensor = Tensor::new(vec![1.01, 1.99, 3.01], vec![3]);

    let stats = measure_error(&ref_tensor, &quant_tensor, "test_layer".to_string()).unwrap();

    assert_eq!(stats.layer_id, "test_layer");
    assert_eq!(stats.element_count, 3);
    assert!(stats.l2_error > 0.0);
    assert!(stats.max_error > 0.0);
    assert!(stats.mean_error > 0.0);

    // Test identical tensors
    let identical_stats = measure_error(&ref_tensor, &ref_tensor, "identical".to_string()).unwrap();
    assert_eq!(identical_stats.l2_error, 0.0);
    assert_eq!(identical_stats.max_error, 0.0);
    assert_eq!(identical_stats.mean_error, 0.0);

    // Test dimension mismatch
    let wrong_tensor = Tensor::new(vec![1.0, 2.0], vec![2]);
    let result = measure_error(&ref_tensor, &wrong_tensor, "mismatch".to_string());
    assert!(matches!(
        result,
        Err(NumericsError::DimensionMismatch { .. })
    ));
}

/// Test global stability report aggregation
#[test]
fn test_global_stability_report() {
    let mut report = GlobalStabilityReport::new();

    // Add multiple layer statistics
    let stats1 = EpsilonStats::new("layer1".to_string(), 0.001, 0.01, 0.005, 1000);
    let stats2 = EpsilonStats::new("layer2".to_string(), 0.002, 0.02, 0.008, 2000);
    let stats3 = EpsilonStats::new("layer3".to_string(), 0.0005, 0.005, 0.003, 500);

    report.add_layer_stats(stats1);
    report.add_layer_stats(stats2);
    report.add_layer_stats(stats3);

    // Verify aggregation
    assert_eq!(report.layer_count, 3);
    assert_eq!(report.total_elements, 3500);
    assert_eq!(report.total_l2_error, 0.0035);
    assert_eq!(report.max_layer_error, 0.02);
    assert!((report.mean_layer_error - 0.0011666666666666668).abs() < 1e-10);

    // Test stability score
    let stability_score = report.stability_score();
    assert!(stability_score > 0.0);
    assert!(stability_score < 1.0);

    // Test stability check
    assert!(report.is_stable(1e-3)); // Should be stable with threshold 1e-3
    assert!(!report.is_stable(1e-6)); // Should not be stable with threshold 1e-6
}

/// Test Metal kernel integration (mock test)
#[test]
fn test_metal_kernel_integration() {
    // This test verifies that the noise tracker can be integrated
    // with Metal kernels without compilation errors

    let config = NoiseTrackingConfig::default();
    let tracker = NoiseTracker::new(config, None);

    // Verify tracker can be created and accessed
    assert_eq!(tracker.step_count(), 0);
    assert!(tracker.is_stable());

    // Verify configuration can be updated
    let mut mutable_tracker = tracker;
    let new_config = NoiseTrackingConfig {
        enabled: false,
        error_threshold: 1e-8,
        strict_mode: true,
        enable_reference: true,
        max_layers_per_step: 5,
    };

    mutable_tracker.update_config(new_config);

    // Verify reset functionality
    mutable_tracker.reset();
    assert_eq!(mutable_tracker.step_count(), 0);
}

/// Test error rate calculation
#[test]
fn test_error_rate_calculation() {
    let ref_tensor = Tensor::new(vec![1.0, 5.0, 10.0], vec![3]);
    let quant_tensor = Tensor::new(vec![1.1, 4.9, 10.1], vec![3]);

    let stats = measure_error(&ref_tensor, &quant_tensor, "test".to_string()).unwrap();

    // Test error rate calculation
    let reference_range = 9.0; // max - min = 10.0 - 1.0
    let error_rate = stats.error_rate(reference_range);

    assert!(error_rate > 0.0);
    assert!(error_rate < 100.0); // Should be reasonable percentage

    // Test with zero range
    let zero_range_rate = stats.error_rate(0.0);
    assert_eq!(zero_range_rate, 0.0);
}

/// Test threshold checking
#[test]
fn test_threshold_checking() {
    let stats = EpsilonStats::new("test".to_string(), 1e-7, 1e-6, 1e-7, 1000);

    // Test threshold checking
    assert!(!stats.exceeds_threshold(1e-6)); // Should not exceed
    assert!(stats.exceeds_threshold(1e-8)); // Should exceed

    // Test edge case
    assert!(!stats.exceeds_threshold(1e-7)); // Exactly at threshold
}

/// Test numerical overflow detection
#[test]
fn test_numerical_overflow_detection() {
    // Test with infinite values
    let ref_tensor = Tensor::new(vec![1.0, f32::INFINITY, 3.0], vec![3]);
    let quant_tensor = Tensor::new(vec![1.0, 2.0, 3.0], vec![3]);

    let result = measure_error(&ref_tensor, &quant_tensor, "test".to_string());
    assert!(matches!(result, Err(NumericsError::NumericalOverflow)));

    // Test with NaN values
    let ref_tensor_nan = Tensor::new(vec![1.0, f32::NAN, 3.0], vec![3]);
    let result_nan = measure_error(&ref_tensor_nan, &quant_tensor, "test".to_string());
    assert!(matches!(result_nan, Err(NumericsError::NumericalOverflow)));
}

/// Test reference range computation
#[test]
fn test_reference_range_computation() {
    use adapteros_numerics::noise::compute_reference_range;

    let tensor = Tensor::new(vec![1.0, 5.0, 3.0], vec![3]);
    let range = compute_reference_range(&tensor);
    assert_eq!(range, 4.0); // max - min = 5.0 - 1.0

    // Test empty tensor
    let empty_tensor = Tensor::new(vec![], vec![]);
    let empty_range = compute_reference_range(&empty_tensor);
    assert_eq!(empty_range, 0.0);

    // Test single element
    let single_tensor = Tensor::new(vec![42.0], vec![1]);
    let single_range = compute_reference_range(&single_tensor);
    assert_eq!(single_range, 0.0); // max - min = 42.0 - 42.0
}
