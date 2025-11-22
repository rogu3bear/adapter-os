//! Backend factory integration tests
//!
//! Tests for CoreML, Metal, and MLX backend integration with the backend factory.
//! Verifies correct capability detection, automatic selection, and fallback behavior.

use adapteros_lora_worker::backend_factory::{
    auto_select_backend, create_backend, create_backend_auto, describe_available_backends,
    detect_capabilities, BackendCapabilities, BackendChoice, BackendStrategy,
};

#[test]
fn test_detect_capabilities() {
    let caps = detect_capabilities();

    // Basic sanity checks
    println!("Detected capabilities:");
    println!("  has_metal: {}", caps.has_metal);
    println!("  has_coreml: {}", caps.has_coreml);
    println!("  has_ane: {}", caps.has_ane);
    println!("  has_mlx: {}", caps.has_mlx);
    println!("  gpu_memory_bytes: {:?}", caps.gpu_memory_bytes);

    // On macOS, Metal should always be available (unless using Intel Mac with Metal disabled)
    #[cfg(target_os = "macos")]
    {
        // We expect at least one backend to be available on macOS
        assert!(
            caps.has_metal || caps.has_coreml || caps.has_mlx,
            "At least one backend should be available on macOS"
        );
    }
}

#[test]
fn test_coreml_detection_with_ane() {
    let caps = detect_capabilities();

    // If CoreML is available, ANE status should be consistent
    if caps.has_coreml {
        // CoreML might be available without ANE (e.g., on older M1 or Intel)
        // But on Apple Silicon (aarch64), it's more likely ANE is available
        println!("CoreML available with ANE: {}", caps.has_ane);

        // Verify ANE status is correct for Apple Silicon
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            // Apple Silicon should likely have ANE, but don't assert it as it's a runtime property
            println!("On Apple Silicon, ANE: {}", caps.has_ane);
        }
    }
}

#[test]
fn test_auto_select_backend_coreml_priority() {
    let mut caps = BackendCapabilities {
        has_metal: true,
        metal_device_name: Some("Test Metal Device".to_string()),
        has_ane: true,
        has_coreml: true,
        has_mlx: false,
        gpu_memory_bytes: Some(8 * 1024 * 1024 * 1024), // 8GB
    };

    // When CoreML and ANE are both available, CoreML should be selected
    let choice = auto_select_backend(&caps).expect("Should select CoreML backend");
    match choice {
        BackendChoice::CoreML => {
            // Expected - CoreML selected
        }
        _ => panic!("Expected CoreML backend choice, got {:?}", choice),
    }

    // Now test Metal fallback when CoreML is unavailable
    caps.has_coreml = false;
    caps.has_ane = false;
    let choice = auto_select_backend(&caps).expect("Should select Metal backend");
    match choice {
        BackendChoice::Metal => {
            // Expected
        }
        _ => panic!("Expected Metal backend fallback, got {:?}", choice),
    }
}

#[test]
fn test_auto_select_backend_metal_fallback() {
    let caps = BackendCapabilities {
        has_metal: true,
        metal_device_name: Some("Test GPU".to_string()),
        has_ane: false,
        has_coreml: false,
        has_mlx: false,
        gpu_memory_bytes: Some(4 * 1024 * 1024 * 1024), // 4GB
    };

    // When only Metal is available, it should be selected
    let choice = auto_select_backend(&caps).expect("Should select Metal backend");
    match choice {
        BackendChoice::Metal => {
            // Expected
        }
        _ => panic!("Expected Metal backend, got {:?}", choice),
    }
}

#[test]
fn test_create_backend_metal() {
    // Metal backend should be available on macOS
    #[cfg(target_os = "macos")]
    {
        match create_backend(BackendChoice::Metal) {
            Ok(_backend) => {
                println!("Successfully created Metal backend");
            }
            Err(e) => {
                // Metal might not be available on all systems
                println!(
                    "Metal backend creation failed (expected on some systems): {}",
                    e
                );
            }
        }
    }

    // On non-macOS, Metal should fail
    #[cfg(not(target_os = "macos"))]
    {
        let result = create_backend(BackendChoice::Metal);
        assert!(result.is_err(), "Metal backend should fail on non-macOS");
    }
}

#[test]
fn test_create_backend_coreml() {
    // CoreML backend with production_mode should handle unavailable ANE gracefully
    match create_backend(BackendChoice::CoreML) {
        Ok(_backend) => {
            println!("Successfully created CoreML backend");
        }
        Err(e) => {
            println!("CoreML backend creation failed: {}", e);
            // CoreML might not be available if the feature is disabled
        }
    }
}

#[test]
fn test_create_backend_auto() {
    // Auto selection should return a valid backend
    match create_backend(BackendChoice::Auto) {
        Ok(_backend) => {
            println!("Successfully created auto-selected backend");
        }
        Err(e) => {
            println!("Auto backend creation failed: {}", e);
        }
    }
}

#[test]
fn test_create_backend_auto_with_model_size() {
    // Test auto selection with model size constraint
    let model_size = 100 * 1024 * 1024; // 100MB model

    match create_backend_auto(Some(model_size)) {
        Ok(_backend) => {
            println!("Successfully created auto-selected backend with model size constraint");
        }
        Err(e) => {
            println!("Auto backend with model size failed: {}", e);
        }
    }
}

#[test]
fn test_backend_strategy_metal_with_coreml_fallback() {
    let strategy = BackendStrategy::MetalWithCoreMLFallback;

    // Test with Metal available
    let caps_metal = BackendCapabilities {
        has_metal: true,
        metal_device_name: Some("GPU".to_string()),
        has_ane: false,
        has_coreml: false,
        has_mlx: false,
        gpu_memory_bytes: Some(4 * 1024 * 1024 * 1024),
    };

    let choice = strategy
        .select_backend(&caps_metal, None)
        .expect("Should select Metal");
    assert!(matches!(choice, BackendChoice::Metal));

    // Test with CoreML fallback
    let caps_coreml = BackendCapabilities {
        has_metal: false,
        metal_device_name: None,
        has_ane: true,
        has_coreml: true,
        has_mlx: false,
        gpu_memory_bytes: None,
    };

    let choice = strategy
        .select_backend(&caps_coreml, None)
        .expect("Should select CoreML");
    assert!(matches!(choice, BackendChoice::CoreML));

    // Test with no available backends
    let caps_none = BackendCapabilities {
        has_metal: false,
        metal_device_name: None,
        has_ane: false,
        has_coreml: false,
        has_mlx: false,
        gpu_memory_bytes: None,
    };

    assert!(
        strategy.select_backend(&caps_none, None).is_err(),
        "Should fail when no backends available"
    );
}

#[test]
fn test_backend_strategy_coreml_with_metal_fallback() {
    let strategy = BackendStrategy::CoreMLWithMetalFallback;

    // Test with CoreML available
    let caps_coreml = BackendCapabilities {
        has_metal: true,
        metal_device_name: Some("GPU".to_string()),
        has_ane: true,
        has_coreml: true,
        has_mlx: false,
        gpu_memory_bytes: Some(4 * 1024 * 1024 * 1024),
    };

    let choice = strategy
        .select_backend(&caps_coreml, None)
        .expect("Should select CoreML");
    assert!(matches!(choice, BackendChoice::CoreML));

    // Test with Metal fallback
    let caps_metal = BackendCapabilities {
        has_metal: true,
        metal_device_name: Some("GPU".to_string()),
        has_ane: false,
        has_coreml: false,
        has_mlx: false,
        gpu_memory_bytes: Some(4 * 1024 * 1024 * 1024),
    };

    let choice = strategy
        .select_backend(&caps_metal, None)
        .expect("Should select Metal");
    assert!(matches!(choice, BackendChoice::Metal));
}

#[test]
fn test_backend_strategy_metal_only() {
    let strategy = BackendStrategy::MetalOnly;

    // Test with Metal available
    let caps_metal = BackendCapabilities {
        has_metal: true,
        metal_device_name: Some("GPU".to_string()),
        has_ane: false,
        has_coreml: false,
        has_mlx: false,
        gpu_memory_bytes: Some(4 * 1024 * 1024 * 1024),
    };

    let choice = strategy
        .select_backend(&caps_metal, None)
        .expect("Should select Metal");
    assert!(matches!(choice, BackendChoice::Metal));

    // Test with Metal unavailable
    let caps_no_metal = BackendCapabilities {
        has_metal: false,
        metal_device_name: None,
        has_ane: true,
        has_coreml: true,
        has_mlx: false,
        gpu_memory_bytes: None,
    };

    assert!(
        strategy.select_backend(&caps_no_metal, None).is_err(),
        "Should fail when Metal not available"
    );
}

#[test]
fn test_describe_available_backends() {
    let description = describe_available_backends();
    println!("Available backends:\n{}", description);

    // Should contain at least some content
    assert!(
        !description.is_empty(),
        "Backend description should not be empty"
    );
    assert!(
        description.contains("Available backends:"),
        "Should mention available backends"
    );
}

#[test]
fn test_backend_capabilities_module() {
    use adapteros_lora_worker::backend_factory::capabilities;

    let backends = capabilities::get_available_backends();
    println!(
        "Available backends from capabilities module: {:?}",
        backends
    );

    // Should return at least 3 backend types
    assert!(
        backends.len() >= 3,
        "Should report at least Metal, CoreML, and MLX"
    );

    // Check that we have the expected backend types
    let backend_types: Vec<_> = backends.iter().map(|b| &b.name).collect();
    assert!(
        backend_types.contains(&&"Metal".to_string()),
        "Should include Metal"
    );
    assert!(
        backend_types.contains(&&"CoreML".to_string()),
        "Should include CoreML"
    );
    assert!(
        backend_types.contains(&&"MLX".to_string()),
        "Should include MLX"
    );
}

#[test]
fn test_backend_capabilities_status_logging() {
    use adapteros_lora_worker::backend_factory::capabilities;

    // Just test that the function runs without panicking
    capabilities::log_backend_status();
    println!("Backend status logging completed");
}

#[test]
fn test_apple_silicon_detection() {
    // Only run on macOS
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        let caps = detect_capabilities();
        // On Apple Silicon, we should have either Metal or CoreML
        assert!(
            caps.has_metal || caps.has_coreml,
            "Apple Silicon should have Metal or CoreML"
        );
    }

    // Non-aarch64 systems shouldn't claim to be Apple Silicon
    #[cfg(all(target_os = "macos", not(target_arch = "aarch64")))]
    {
        let caps = detect_capabilities();
        // x86_64 Macs may or may not have ANE
        println!("Intel Mac detected - ANE available: {}", caps.has_ane);
    }
}

#[test]
fn test_backend_choice_exhaustive_matching() {
    // Test that BackendChoice can be matched on exhaustively
    let choices = vec![
        BackendChoice::Metal,
        BackendChoice::CoreML,
        BackendChoice::Mlx,
        BackendChoice::Auto,
    ];

    for choice in choices {
        match choice {
            BackendChoice::Metal => println!("Metal backend"),
            BackendChoice::CoreML => println!("CoreML backend"),
            BackendChoice::Mlx => println!("MLX backend"),
            BackendChoice::Auto => println!("Auto-selected backend"),
        }
    }
}

#[test]
fn test_backend_capabilities_struct() {
    let caps = BackendCapabilities {
        has_metal: true,
        metal_device_name: Some("Test GPU".to_string()),
        has_ane: true,
        has_coreml: true,
        has_mlx: false,
        gpu_memory_bytes: Some(8_000_000_000),
    };

    assert!(caps.has_metal);
    assert!(caps.has_coreml);
    assert!(caps.has_ane);
    assert!(!caps.has_mlx);
    assert_eq!(caps.metal_device_name, Some("Test GPU".to_string()));
    assert_eq!(caps.gpu_memory_bytes, Some(8_000_000_000));
}

#[test]
fn test_gpu_memory_detection() {
    let caps = detect_capabilities();

    if let Some(memory) = caps.gpu_memory_bytes {
        println!(
            "GPU memory detected: {} bytes ({} MB)",
            memory,
            memory / (1024 * 1024)
        );
        // GPU memory should be reasonable (at least a few hundred MB)
        assert!(
            memory > 100 * 1024 * 1024,
            "GPU memory should be at least 100MB"
        );
    } else {
        println!("No GPU memory detected");
    }
}

#[test]
fn test_headroom_calculation() {
    // Test the 15% headroom policy
    let gpu_memory = 8 * 1024 * 1024 * 1024u64; // 8GB
    let required_headroom = (gpu_memory as f64 * 0.15) as u64;
    let available = gpu_memory - required_headroom;

    println!("Total GPU memory: {}MB", gpu_memory / (1024 * 1024));
    println!(
        "Required headroom (15%): {}MB",
        required_headroom / (1024 * 1024)
    );
    println!("Available for models: {}MB", available / (1024 * 1024));

    assert!(required_headroom > 0, "Headroom should be > 0");
    assert!(
        available < gpu_memory,
        "Available should be less than total"
    );

    // 15% of 8GB = 8GB * 0.15 = 1.2GB
    // Verify it's approximately 15% (within 0.1% tolerance for floating point precision)
    let percentage = (required_headroom as f64) / (gpu_memory as f64);
    assert!(
        (percentage - 0.15).abs() < 0.001,
        "Headroom should be approximately 15% of GPU memory (got {:.2}%)",
        percentage * 100.0
    );
}
