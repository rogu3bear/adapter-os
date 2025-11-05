#![cfg(all(test, feature = "extended-tests"))]

//! Integration tests for MLX model import functionality
//!
//! Tests verify that MLX model import command works correctly with mlx-ffi-backend feature.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

#[test]
#[cfg(any(feature = "mlx-ffi-backend", feature = "experimental-backends"))]
fn test_mlx_import_command_exists_with_feature() {
    // This test verifies that import-model command is available when mlx-ffi-backend is enabled

    let output = Command::new("cargo")
        .args(&[
            "run",
            "--bin",
            "aosctl",
            "--features",
            "mlx-ffi-backend",
            "--",
            "import-model",
            "--help",
        ])
        .output()
        .expect("Failed to run aosctl help");

    let help_text = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Command should exist and help should work
    assert!(
        help_text.contains("import-model") || help_text.contains("Import MLX model"),
        "import-model command should be available in help. stdout: {}, stderr: {}",
        help_text,
        stderr
    );
}

#[test]
#[cfg(not(any(feature = "mlx-ffi-backend", feature = "experimental-backends")))]
fn test_mlx_import_command_not_available_without_feature() {
    // When feature is not enabled, import should fail with feature error
    // Note: This test may not run if extended-tests feature is required
    // but mlx-ffi-backend is not enabled

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let test_weights = temp_dir.path().join("weights.safetensors");
    let test_config = temp_dir.path().join("config.json");
    let test_tokenizer = temp_dir.path().join("tokenizer.json");
    let test_tokenizer_cfg = temp_dir.path().join("tokenizer_config.json");
    let test_license = temp_dir.path().join("LICENSE");

    // Create dummy files with proper content
    fs::write(&test_weights, b"dummy weights data").expect("Failed to write weights");
    fs::write(&test_config, r#"{"model_type": "test_model"}"#).expect("Failed to write config");
    fs::write(&test_tokenizer, r#"{"test": "tokenizer_data"}"#).expect("Failed to write tokenizer");
    fs::write(&test_tokenizer_cfg, r#"{"test": "tokenizer_config"}"#)
        .expect("Failed to write tokenizer config");
    fs::write(&test_license, "Test license content").expect("Failed to write license");

    let output = Command::new("cargo")
        .args(&[
            "run",
            "--bin",
            "aosctl",
            "--",
            "import-model",
            "--name",
            "test-model",
            "--weights",
            &test_weights.to_string_lossy(),
            "--config",
            &test_config.to_string_lossy(),
            "--tokenizer",
            &test_tokenizer.to_string_lossy(),
            "--tokenizer-cfg",
            &test_tokenizer_cfg.to_string_lossy(),
            "--license",
            &test_license.to_string_lossy(),
        ])
        .output()
        .expect("Failed to run import-model");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should fail with feature error message
    assert!(
        !output.status.success(),
        "Import should fail without mlx-ffi-backend feature"
    );

    // Error should mention feature requirement
    assert!(
        stderr.contains("mlx-ffi-backend") || stdout.contains("mlx-ffi-backend"),
        "Error should mention mlx-ffi-backend feature requirement. stderr: {}, stdout: {}",
        stderr,
        stdout
    );
}

#[test]
#[cfg(any(feature = "mlx-ffi-backend", feature = "experimental-backends"))]
fn test_mlx_import_validates_files_exist() {
    // Test that import command validates all required files exist

    let temp_dir = TempDir::new().expect("Failed to create temp directory");

    // Create paths that don't exist
    let missing_weights = temp_dir.path().join("missing_weights.safetensors");
    let missing_config = temp_dir.path().join("missing_config.json");
    let missing_tokenizer = temp_dir.path().join("missing_tokenizer.json");
    let missing_tokenizer_cfg = temp_dir.path().join("missing_tokenizer_config.json");
    let missing_license = temp_dir.path().join("missing_LICENSE");

    let output = Command::new("cargo")
        .args(&[
            "run",
            "--bin",
            "aosctl",
            "--features",
            "mlx-ffi-backend",
            "--",
            "import-model",
            "--name",
            "test-model",
            "--weights",
            &missing_weights.to_string_lossy(),
            "--config",
            &missing_config.to_string_lossy(),
            "--tokenizer",
            &missing_tokenizer.to_string_lossy(),
            "--tokenizer-cfg",
            &missing_tokenizer_cfg.to_string_lossy(),
            "--license",
            &missing_license.to_string_lossy(),
        ])
        .output()
        .expect("Failed to run import-model");

    assert!(
        !output.status.success(),
        "Import should fail when files don't exist"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should mention file not found or similar error
    let error_message = format!("stderr: {}\nstdout: {}", stderr, stdout);
    assert!(
        stderr.contains("not found")
            || stdout.contains("not found")
            || stderr.contains("does not exist")
            || stdout.contains("does not exist")
            || stderr.contains("No such file")
            || stdout.contains("No such file"),
        "Error should mention file not found. {}",
        error_message
    );
}
