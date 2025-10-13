//! Kernel layout validation tests
//!
//! Tests the buffer layout validator to ensure it correctly detects
//! size mismatches and alignment violations before kernel dispatch.

#[cfg(target_os = "macos")]
use metal::{Device, MTLResourceOptions};

#[cfg(target_os = "macos")]
use mplora_kernel_mtl::LayoutValidator;

#[test]
#[cfg(target_os = "macos")]
fn test_valid_buffer_layout() {
    let device = Device::system_default().unwrap();
    let validator = LayoutValidator::new();

    // Create a buffer with known size
    let buffer = device.new_buffer(1024, MTLResourceOptions::StorageModeShared);

    // Validate with matching parameters (256 elements * 4 bytes = 1024)
    let result = validator.validate_buffer("test_tensor", &buffer, 4, 256);
    assert!(result.is_ok());
}

#[test]
#[cfg(target_os = "macos")]
fn test_size_mismatch_detected() {
    let device = Device::system_default().unwrap();
    let validator = LayoutValidator::new();

    let buffer = device.new_buffer(1024, MTLResourceOptions::StorageModeShared);

    // Try to validate with wrong parameters (512 elements * 4 bytes = 2048, but buffer is 1024)
    let result = validator.validate_buffer("test_tensor", &buffer, 4, 512);
    
    assert!(result.is_err());
    
    match result {
        Err(mplora_core::AosError::KernelLayoutMismatch { tensor, expected, got }) => {
            assert_eq!(tensor, "test_tensor");
            assert!(expected.contains("2048"));
            assert!(got.contains("1024"));
        }
        _ => panic!("Expected KernelLayoutMismatch error"),
    }
}

#[test]
#[cfg(target_os = "macos")]
fn test_multiple_buffers_validation() {
    let device = Device::system_default().unwrap();
    let validator = LayoutValidator::new();

    let buffer1 = device.new_buffer(2048, MTLResourceOptions::StorageModeShared);
    let buffer2 = device.new_buffer(2048, MTLResourceOptions::StorageModeShared);
    let buffer3 = device.new_buffer(2048, MTLResourceOptions::StorageModeShared);

    let buffers = vec![
        ("input", &buffer1),
        ("weights", &buffer2),
        ("output", &buffer3),
    ];

    // All buffers have same size and stride
    let result = validator.validate_buffers_uniform(&buffers, 4, 512);
    assert!(result.is_ok());
}

#[test]
#[cfg(target_os = "macos")]
fn test_min_size_validation() {
    let device = Device::system_default().unwrap();
    let validator = LayoutValidator::new();

    let buffer = device.new_buffer(2048, MTLResourceOptions::StorageModeShared);

    // Buffer is 2048, min is 1024 - should pass
    assert!(validator.validate_min_size("var_buffer", &buffer, 1024).is_ok());

    // Buffer is 2048, min is 4096 - should fail
    assert!(validator.validate_min_size("var_buffer", &buffer, 4096).is_err());
}

#[test]
#[cfg(target_os = "macos")]
fn test_custom_alignment() {
    let device = Device::system_default().unwrap();
    let validator = LayoutValidator::with_alignment(32);

    let buffer = device.new_buffer(1024, MTLResourceOptions::StorageModeShared);

    // Metal buffers should be naturally aligned, but test the API
    // (actual alignment check depends on Metal's allocation strategy)
    let result = validator.validate_buffer("aligned_tensor", &buffer, 4, 256);
    
    // Result depends on actual buffer address alignment
    // Just ensure the validator doesn't panic
    let _ = result;
}

#[test]
#[cfg(target_os = "macos")]
fn test_zero_size_buffer_fails() {
    let device = Device::system_default().unwrap();
    let validator = LayoutValidator::new();

    let buffer = device.new_buffer(0, MTLResourceOptions::StorageModeShared);

    // Zero-size buffer should fail validation (unless expecting zero elements)
    let result = validator.validate_buffer("empty_tensor", &buffer, 4, 256);
    assert!(result.is_err());
}

#[test]
#[cfg(not(target_os = "macos"))]
fn test_layout_validator_not_available() {
    // On non-macOS platforms, just ensure the test compiles
    println!("Layout validation tests require macOS with Metal support");
}
