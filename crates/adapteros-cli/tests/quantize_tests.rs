#[path = "common/mod.rs"]
mod common;

use adapteros_cli::commands::quantize_qwen;
use adapteros_cli::output::{OutputMode, OutputWriter};
use std::collections::BTreeMap;

fn json_output() -> OutputWriter {
    OutputWriter::new(OutputMode::Json, false)
}

#[tokio::test]
async fn quantize_qwen_processes_safetensors_file() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().expect("create temp dir");
    let input_file = temp_dir.path().join("test.safetensors");
    let output_dir = temp_dir.path().join("output");

    // Create a synthetic safetensors file with a 2D FP32 tensor
    let data: Vec<f32> = (0..100).map(|i| (i as f32) * 0.1).collect();
    let shape = vec![10, 10];
    let tensor_view = safetensors::tensor::TensorView::new(
        safetensors::Dtype::F32,
        shape.clone(),
        bytemuck::cast_slice(&data),
    )
    .expect("create tensor view");

    let mut tensors = BTreeMap::new();
    tensors.insert("weight".to_string(), tensor_view);

    let safetensors_data =
        safetensors::tensor::serialize(&tensors, &None).expect("serialize safetensors");
    std::fs::write(&input_file, safetensors_data).expect("write input file");

    // Run quantization
    let output = json_output();
    quantize_qwen::run(&input_file, &output_dir, "test_model", None, false, &output)
        .await
        .expect("quantization succeeds");

    // Verify output files exist
    assert!(output_dir.exists());
    let manifest_path = output_dir.join("manifest.json");
    assert!(manifest_path.exists());

    let manifest: quantize_qwen::QuantizationManifest =
        serde_json::from_reader(std::fs::File::open(&manifest_path).expect("open manifest"))
            .expect("parse manifest");

    assert_eq!(manifest.model_name, "test_model");
    assert_eq!(manifest.quant_method, "int4_per_out_channel");
    assert_eq!(manifest.bits, 4);
    assert!(manifest.per_channel);

    assert_eq!(manifest.tensors.len(), 1);
    let tensor_info = manifest.tensors.get("weight").expect("weight tensor");
    assert_eq!(tensor_info.shape, shape);

    let packed_path = output_dir.join(&tensor_info.packed_path);
    let scales_path = output_dir.join(&tensor_info.scales_path);
    let zps_path = output_dir.join(&tensor_info.zero_points_path);

    assert!(packed_path.exists());
    assert!(scales_path.exists());
    assert!(zps_path.exists());

    // Verify packed data size (10 rows * ((10 + 1) / 2) bytes = 50 bytes)
    let packed_data = std::fs::read(&packed_path).expect("read packed data");
    assert_eq!(packed_data.len(), 50);

    // Verify scales data size (10 rows * 4 bytes = 40 bytes)
    let scales_data = std::fs::read(&scales_path).expect("read scales data");
    assert_eq!(scales_data.len(), 40);

    // Verify zero points data size (10 rows * 1 byte = 10 bytes)
    let zps_data = std::fs::read(&zps_path).expect("read zps data");
    assert_eq!(zps_data.len(), 10);
}
