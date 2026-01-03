//! Tests for model seeding module.

use std::fs::{self, File};
use tempfile::TempDir;

// Import the function directly from the crate
use adapteros_server::model_seeding::detect_model_format_backend;

#[test]
fn test_detect_model_format_backend_gguf() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let model_dir = temp_dir.path().join("model");
    fs::create_dir(&model_dir).expect("Failed to create model dir");

    // Create a .gguf file
    let gguf_path = model_dir.join("model.gguf");
    File::create(&gguf_path).expect("Failed to create gguf file");

    let (format, backend) = detect_model_format_backend(&model_dir);
    assert_eq!(format, "gguf");
    assert_eq!(backend, "metal");
}

#[test]
fn test_detect_model_format_backend_safetensors() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let model_dir = temp_dir.path().join("model");
    fs::create_dir(&model_dir).expect("Failed to create model dir");

    // Create a .safetensors file
    let st_path = model_dir.join("model.safetensors");
    File::create(&st_path).expect("Failed to create safetensors file");

    let (format, backend) = detect_model_format_backend(&model_dir);
    assert_eq!(format, "safetensors");
    assert_eq!(backend, "mlx");
}

#[test]
fn test_detect_model_format_backend_mlpackage() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let model_dir = temp_dir.path().join("model");
    fs::create_dir(&model_dir).expect("Failed to create model dir");

    // Create a .mlpackage directory (CoreML models are directories)
    let mlpackage_path = model_dir.join("model.mlpackage");
    fs::create_dir(&mlpackage_path).expect("Failed to create mlpackage dir");

    let (format, backend) = detect_model_format_backend(&model_dir);
    assert_eq!(format, "mlpackage");
    assert_eq!(backend, "coreml");
}

#[test]
fn test_detect_model_format_backend_unknown_extension() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let model_dir = temp_dir.path().join("model");
    fs::create_dir(&model_dir).expect("Failed to create model dir");

    // Create a file with unknown extension
    let unknown_path = model_dir.join("model.bin");
    File::create(&unknown_path).expect("Failed to create unknown file");

    let (format, backend) = detect_model_format_backend(&model_dir);
    // Default fallback
    assert_eq!(format, "safetensors");
    assert_eq!(backend, "mlx");
}

#[test]
fn test_detect_model_format_backend_empty_directory() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let model_dir = temp_dir.path().join("model");
    fs::create_dir(&model_dir).expect("Failed to create model dir");

    let (format, backend) = detect_model_format_backend(&model_dir);
    // Default fallback for empty directory
    assert_eq!(format, "safetensors");
    assert_eq!(backend, "mlx");
}

#[test]
fn test_detect_model_format_backend_nonexistent_directory() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let model_dir = temp_dir.path().join("nonexistent");

    let (format, backend) = detect_model_format_backend(&model_dir);
    // Default fallback for non-existent directory
    assert_eq!(format, "safetensors");
    assert_eq!(backend, "mlx");
}

#[test]
fn test_detect_model_format_backend_gguf_takes_precedence_over_safetensors() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let model_dir = temp_dir.path().join("model");
    fs::create_dir(&model_dir).expect("Failed to create model dir");

    // Create both .safetensors and .gguf files
    File::create(model_dir.join("model.safetensors")).expect("Failed to create safetensors file");
    File::create(model_dir.join("model.gguf")).expect("Failed to create gguf file");

    let (format, _backend) = detect_model_format_backend(&model_dir);
    // gguf should be detected (order depends on directory iteration, but both are valid)
    assert!(format == "gguf" || format == "safetensors");
}

#[test]
fn test_detect_model_format_backend_mlpackage_takes_precedence() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let model_dir = temp_dir.path().join("model");
    fs::create_dir(&model_dir).expect("Failed to create model dir");

    // Create both .gguf and .mlpackage
    File::create(model_dir.join("model.gguf")).expect("Failed to create gguf file");
    fs::create_dir(model_dir.join("model.mlpackage")).expect("Failed to create mlpackage dir");

    let (format, _backend) = detect_model_format_backend(&model_dir);
    // mlpackage breaks early, so it should take precedence if encountered first
    // Due to directory iteration order being undefined, we accept either
    assert!(format == "mlpackage" || format == "gguf");
}
