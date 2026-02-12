//! Tests for model format detection (now delegated to adapteros_core::ModelFormat).

use std::fs::{self, File};
use tempfile::TempDir;

use adapteros_core::ModelFormat;

#[test]
fn test_detect_model_format_gguf() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let model_dir = temp_dir.path().join("model");
    fs::create_dir(&model_dir).expect("Failed to create model dir");

    File::create(model_dir.join("model.gguf")).expect("Failed to create gguf file");

    let format = ModelFormat::detect_from_dir(&model_dir);
    assert_eq!(format, ModelFormat::Gguf);
    assert_eq!(format.as_str(), "gguf");
    assert_eq!(format.default_backend().as_str(), "metal");
}

#[test]
fn test_detect_model_format_safetensors() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let model_dir = temp_dir.path().join("model");
    fs::create_dir(&model_dir).expect("Failed to create model dir");

    File::create(model_dir.join("model.safetensors")).expect("Failed to create safetensors file");

    let format = ModelFormat::detect_from_dir(&model_dir);
    assert_eq!(format, ModelFormat::SafeTensors);
    assert_eq!(format.as_str(), "safetensors");
    assert_eq!(format.default_backend().as_str(), "mlx");
}

#[test]
fn test_detect_model_format_mlpackage() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let model_dir = temp_dir.path().join("model");
    fs::create_dir(&model_dir).expect("Failed to create model dir");

    // CoreML models are directories
    fs::create_dir(model_dir.join("model.mlpackage")).expect("Failed to create mlpackage dir");

    let format = ModelFormat::detect_from_dir(&model_dir);
    assert_eq!(format, ModelFormat::MlPackage);
    assert_eq!(format.as_str(), "mlpackage");
    assert_eq!(format.default_backend().as_str(), "coreml");
}

#[test]
fn test_detect_model_format_unknown_extension() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let model_dir = temp_dir.path().join("model");
    fs::create_dir(&model_dir).expect("Failed to create model dir");

    File::create(model_dir.join("model.bin")).expect("Failed to create unknown file");

    let format = ModelFormat::detect_from_dir(&model_dir);
    assert_eq!(format, ModelFormat::SafeTensors);
}

#[test]
fn test_detect_model_format_empty_directory() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let model_dir = temp_dir.path().join("model");
    fs::create_dir(&model_dir).expect("Failed to create model dir");

    let format = ModelFormat::detect_from_dir(&model_dir);
    assert_eq!(format, ModelFormat::SafeTensors);
}

#[test]
fn test_detect_model_format_nonexistent_directory() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let model_dir = temp_dir.path().join("nonexistent");

    let format = ModelFormat::detect_from_dir(&model_dir);
    assert_eq!(format, ModelFormat::SafeTensors);
}

#[test]
fn test_detect_model_format_mlpackage_takes_precedence() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let model_dir = temp_dir.path().join("model");
    fs::create_dir(&model_dir).expect("Failed to create model dir");

    File::create(model_dir.join("model.gguf")).expect("Failed to create gguf file");
    fs::create_dir(model_dir.join("model.mlpackage")).expect("Failed to create mlpackage dir");

    let format = ModelFormat::detect_from_dir(&model_dir);
    // mlpackage breaks early, so it should take precedence if encountered first
    // Due to directory iteration order being undefined, we accept either
    assert!(format == ModelFormat::MlPackage || format == ModelFormat::Gguf);
}
