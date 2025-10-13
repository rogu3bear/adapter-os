//! Domain Adapter Determinism Tests
//!
//! This test suite verifies that all domain adapters produce byte-identical
//! outputs for identical inputs across multiple runs.

use adapteros_deterministic_exec::{DeterministicExecutor, ExecutorConfig};
use adapteros_domain::{
    DomainAdapter, TextAdapter, TelemetryAdapter, TensorData, VisionAdapter,
};
use adapteros_numerics::noise::Tensor;
use std::collections::HashMap;
use tempfile::NamedTempFile;

/// Helper to create a test manifest file
fn create_manifest(
    name: &str,
    adapter_type: &str,
    params: HashMap<String, serde_json::Value>,
) -> NamedTempFile {
    use adapteros_domain::manifest::{save_manifest, AdapterManifest};

    let mut manifest = AdapterManifest::new(
        name.to_string(),
        "1.0.0".to_string(),
        format!("test_{}_model", adapter_type),
        "b3d9c2a1e8f7d6b5a4938271605e4f3c2d1b0a9e8f7d6c5b4a3928170605".to_string(),
    );

    manifest.adapter.input_format = "canonical".to_string();
    manifest.adapter.output_format = "canonical".to_string();
    manifest.adapter.parameters = params;

    let temp_file = NamedTempFile::new().unwrap();
    save_manifest(&manifest, temp_file.path()).unwrap();

    temp_file
}

#[tokio::test]
async fn test_text_adapter_determinism() {
    // Create manifest
    let mut params = HashMap::new();
    params.insert(
        "vocab_size".to_string(),
        serde_json::Value::Number(1000.into()),
    );
    params.insert(
        "max_sequence_length".to_string(),
        serde_json::Value::Number(128.into()),
    );

    let manifest_file = create_manifest("test_text", "text", params);

    // Create executor with fixed seed
    let config = ExecutorConfig {
        global_seed: [42u8; 32],
        ..Default::default()
    };
    let mut executor = DeterministicExecutor::new(config);

    // Load adapter
    let mut adapter1 = TextAdapter::load(manifest_file.path()).unwrap();
    adapter1.prepare(&mut executor).unwrap();

    // Create test input
    let input_data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
    let input_tensor = Tensor::new(input_data.clone(), vec![1, 5]);
    let input = TensorData::new(input_tensor, "f32".to_string());

    // Run forward pass 100 times
    let mut outputs = Vec::new();
    for i in 0..100 {
        adapter1.reset();
        let output = adapter1.forward(&input).unwrap();
        outputs.push(output);

        // Log progress
        if i % 25 == 0 {
            println!("Text adapter run {}/100", i + 1);
        }
    }

    // Verify all outputs are identical
    let first_output = &outputs[0];
    for (i, output) in outputs.iter().enumerate() {
        assert_eq!(
            output.tensor.data, first_output.tensor.data,
            "Output {} differs from first output",
            i
        );
        assert_eq!(
            output.tensor.shape, first_output.tensor.shape,
            "Output {} shape differs from first output",
            i
        );

        // Verify hash
        assert!(
            output.verify_hash(),
            "Output {} hash verification failed",
            i
        );
    }

    println!("✅ Text adapter determinism verified: 100 identical runs");
}

#[tokio::test]
async fn test_vision_adapter_determinism() {
    // Create manifest
    let mut params = HashMap::new();
    params.insert(
        "image_height".to_string(),
        serde_json::Value::Number(64.into()),
    );
    params.insert(
        "image_width".to_string(),
        serde_json::Value::Number(64.into()),
    );
    params.insert(
        "num_channels".to_string(),
        serde_json::Value::Number(3.into()),
    );

    let manifest_file = create_manifest("test_vision", "vision", params);

    // Create executor with fixed seed
    let config = ExecutorConfig {
        global_seed: [42u8; 32],
        ..Default::default()
    };
    let mut executor = DeterministicExecutor::new(config);

    // Load adapter
    let mut adapter = VisionAdapter::load(manifest_file.path()).unwrap();
    adapter.prepare(&mut executor).unwrap();

    // Create test input (NCHW format: 1x3x64x64)
    let input_data: Vec<f32> = (0..12288).map(|x| (x as f32) / 12288.0).collect();
    let input_tensor = Tensor::new(input_data, vec![1, 3, 64, 64]);
    let input = TensorData::new(input_tensor, "f32".to_string());

    // Run forward pass 100 times
    let mut outputs = Vec::new();
    for i in 0..100 {
        adapter.reset();
        let output = adapter.forward(&input).unwrap();
        outputs.push(output);

        if i % 25 == 0 {
            println!("Vision adapter run {}/100", i + 1);
        }
    }

    // Verify all outputs are identical
    let first_output = &outputs[0];
    for (i, output) in outputs.iter().enumerate() {
        assert_eq!(
            output.tensor.data, first_output.tensor.data,
            "Output {} differs from first output",
            i
        );
        assert_eq!(
            output.tensor.shape, first_output.tensor.shape,
            "Output {} shape differs from first output",
            i
        );

        assert!(
            output.verify_hash(),
            "Output {} hash verification failed",
            i
        );
    }

    println!("✅ Vision adapter determinism verified: 100 identical runs");
}

#[tokio::test]
async fn test_telemetry_adapter_determinism() {
    // Create manifest
    let mut params = HashMap::new();
    params.insert(
        "num_channels".to_string(),
        serde_json::Value::Number(4.into()),
    );
    params.insert(
        "window_size".to_string(),
        serde_json::Value::Number(32.into()),
    );
    params.insert(
        "sampling_rate".to_string(),
        serde_json::Value::Number(serde_json::Number::from_f64(100.0).unwrap()),
    );

    let manifest_file = create_manifest("test_telemetry", "telemetry", params);

    // Create executor with fixed seed
    let config = ExecutorConfig {
        global_seed: [42u8; 32],
        ..Default::default()
    };
    let mut executor = DeterministicExecutor::new(config);

    // Load adapter
    let mut adapter = TelemetryAdapter::load(manifest_file.path()).unwrap();
    adapter.prepare(&mut executor).unwrap();

    // Create test input (time-series: 1x4x32)
    let input_data: Vec<f32> = (0..128).map(|x| (x as f32) / 128.0).collect();
    let input_tensor = Tensor::new(input_data, vec![1, 4, 32]);
    let input = TensorData::new(input_tensor, "f32".to_string());

    // Run forward pass 100 times
    let mut outputs = Vec::new();
    for i in 0..100 {
        adapter.reset();
        let output = adapter.forward(&input).unwrap();
        outputs.push(output);

        if i % 25 == 0 {
            println!("Telemetry adapter run {}/100", i + 1);
        }
    }

    // Verify all outputs are identical
    let first_output = &outputs[0];
    for (i, output) in outputs.iter().enumerate() {
        assert_eq!(
            output.tensor.data, first_output.tensor.data,
            "Output {} differs from first output",
            i
        );
        assert_eq!(
            output.tensor.shape, first_output.tensor.shape,
            "Output {} shape differs from first output",
            i
        );

        assert!(
            output.verify_hash(),
            "Output {} hash verification failed",
            i
        );
    }

    println!("✅ Telemetry adapter determinism verified: 100 identical runs");
}

#[tokio::test]
async fn test_adapter_hash_stability() {
    // Verify that tensor hashes are stable across runs

    let data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
    let tensor = Tensor::new(data.clone(), vec![5]);

    let tensor_data1 = TensorData::new(tensor.clone(), "f32".to_string());
    let tensor_data2 = TensorData::new(tensor, "f32".to_string());

    assert_eq!(
        tensor_data1.metadata.hash, tensor_data2.metadata.hash,
        "Hashes should be identical for identical tensors"
    );

    assert!(tensor_data1.verify_hash());
    assert!(tensor_data2.verify_hash());

    println!("✅ Tensor hash stability verified");
}

#[tokio::test]
async fn test_cross_adapter_isolation() {
    // Verify that different adapters don't interfere with each other

    let config = ExecutorConfig {
        global_seed: [42u8; 32],
        ..Default::default()
    };
    let mut executor = DeterministicExecutor::new(config);

    // Create manifests for all three adapter types
    let mut text_params = HashMap::new();
    text_params.insert(
        "vocab_size".to_string(),
        serde_json::Value::Number(100.into()),
    );
    text_params.insert(
        "max_sequence_length".to_string(),
        serde_json::Value::Number(16.into()),
    );
    let text_manifest = create_manifest("test_text_iso", "text", text_params);

    let mut vision_params = HashMap::new();
    vision_params.insert(
        "image_height".to_string(),
        serde_json::Value::Number(32.into()),
    );
    vision_params.insert(
        "image_width".to_string(),
        serde_json::Value::Number(32.into()),
    );
    vision_params.insert(
        "num_channels".to_string(),
        serde_json::Value::Number(3.into()),
    );
    let vision_manifest = create_manifest("test_vision_iso", "vision", vision_params);

    let mut telemetry_params = HashMap::new();
    telemetry_params.insert(
        "num_channels".to_string(),
        serde_json::Value::Number(2.into()),
    );
    telemetry_params.insert(
        "window_size".to_string(),
        serde_json::Value::Number(16.into()),
    );
    telemetry_params.insert(
        "sampling_rate".to_string(),
        serde_json::Value::Number(serde_json::Number::from_f64(50.0).unwrap()),
    );
    let telemetry_manifest = create_manifest("test_telemetry_iso", "telemetry", telemetry_params);

    // Load all adapters
    let mut text_adapter = TextAdapter::load(text_manifest.path()).unwrap();
    let mut vision_adapter = VisionAdapter::load(vision_manifest.path()).unwrap();
    let mut telemetry_adapter = TelemetryAdapter::load(telemetry_manifest.path()).unwrap();

    // Prepare all adapters
    text_adapter.prepare(&mut executor).unwrap();
    vision_adapter.prepare(&mut executor).unwrap();
    telemetry_adapter.prepare(&mut executor).unwrap();

    // Create inputs
    let text_input = TensorData::new(Tensor::new(vec![1.0; 16], vec![1, 16]), "f32".to_string());

    let vision_input =
        TensorData::new(Tensor::new(vec![0.5; 3072], vec![1, 3, 32, 32]), "f32".to_string());

    let telemetry_input =
        TensorData::new(Tensor::new(vec![0.3; 32], vec![1, 2, 16]), "f32".to_string());

    // Run forward passes
    let text_output1 = text_adapter.forward(&text_input).unwrap();
    let vision_output1 = vision_adapter.forward(&vision_input).unwrap();
    let telemetry_output1 = telemetry_adapter.forward(&telemetry_input).unwrap();

    // Reset and run again
    text_adapter.reset();
    vision_adapter.reset();
    telemetry_adapter.reset();

    let text_output2 = text_adapter.forward(&text_input).unwrap();
    let vision_output2 = vision_adapter.forward(&vision_input).unwrap();
    let telemetry_output2 = telemetry_adapter.forward(&telemetry_input).unwrap();

    // Verify each adapter's outputs are consistent
    assert_eq!(
        text_output1.tensor.data, text_output2.tensor.data,
        "Text adapter outputs differ"
    );
    assert_eq!(
        vision_output1.tensor.data, vision_output2.tensor.data,
        "Vision adapter outputs differ"
    );
    assert_eq!(
        telemetry_output1.tensor.data, telemetry_output2.tensor.data,
        "Telemetry adapter outputs differ"
    );

    println!("✅ Cross-adapter isolation verified");
}

#[tokio::test]
async fn test_epsilon_bounds() {
    // Verify that numerical drift stays within epsilon bounds

    let mut params = HashMap::new();
    params.insert(
        "vocab_size".to_string(),
        serde_json::Value::Number(100.into()),
    );
    params.insert(
        "max_sequence_length".to_string(),
        serde_json::Value::Number(16.into()),
    );

    let manifest_file = create_manifest("test_epsilon", "text", params);

    let config = ExecutorConfig {
        global_seed: [42u8; 32],
        ..Default::default()
    };
    let mut executor = DeterministicExecutor::new(config);

    let mut adapter = TextAdapter::load(manifest_file.path()).unwrap();
    adapter.prepare(&mut executor).unwrap();

    let input = TensorData::new(Tensor::new(vec![1.0; 16], vec![1, 16]), "f32".to_string());

    // Run multiple times and check epsilon
    for _ in 0..10 {
        adapter.reset();
        let _output = adapter.forward(&input).unwrap();

        // In this deterministic case, epsilon should be None or zero
        // because there's no quantization or lossy operations
        if let Some(stats) = adapter.epsilon_stats() {
            assert!(
                stats.l2_error < 1e-6,
                "Epsilon exceeds threshold: {}",
                stats.l2_error
            );
        }
    }

    println!("✅ Epsilon bounds verified");
}

