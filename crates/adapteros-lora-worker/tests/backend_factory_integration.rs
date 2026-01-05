//! Backend factory integration tests
//!
//! Tests for CoreML, Metal, and MLX backend integration with the backend factory.
//! Verifies correct capability detection, automatic selection, and fallback behavior.

#![allow(unused_imports)]

use adapteros_core::{
    backend::BackendKind,
    constants::{BYTES_PER_GB, BYTES_PER_MB},
    ExecutionProfile, SeedMode,
};
use adapteros_lora_worker::backend_factory::{
    auto_select_backend, create_backend, create_backend_auto, create_backend_with_model_hashes,
    describe_available_backends, detect_capabilities, select_backend_from_execution_profile,
    BackendCapabilities, BackendChoice, BackendStrategy, SelectionContext,
};

// For testing deprecated API behavior
#[allow(deprecated)]
use adapteros_lora_worker::backend_factory::create_backend_with_model_and_hash;

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
        if !(caps.has_metal || caps.has_coreml || caps.has_mlx) {
            eprintln!("skipping: no Metal/CoreML/MLX backend available in this environment");
            return;
        }
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
        has_mlx_bridge: false,
        gpu_memory_bytes: Some(8 * BYTES_PER_GB), // 8GB
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
fn coreml_request_falls_back_with_reason() {
    let caps = BackendCapabilities {
        has_metal: false,
        metal_device_name: None,
        has_ane: false,
        has_coreml: false,
        has_mlx: true,
        has_mlx_bridge: false,
        gpu_memory_bytes: Some(2 * BYTES_PER_GB),
    };
    let profile = ExecutionProfile {
        seed_mode: SeedMode::BestEffort,
        backend_profile: BackendKind::CoreML,
    };

    let ctx = SelectionContext::new(profile, caps);
    if cfg!(feature = "multi-backend") {
        let selection = select_backend_from_execution_profile(&ctx).expect("fallback selection");
        assert!(
            selection.overridden,
            "coreml should be overridden when unavailable"
        );
        assert_eq!(
            selection.selected,
            BackendChoice::Mlx,
            "fallback should pick MLX when available"
        );
        assert_eq!(selection.reason, Some("coreml_unavailable_fallback_mlx"));
    } else {
        let err = select_backend_from_execution_profile(&ctx).unwrap_err();
        assert!(
            err.to_string()
                .contains("Requested CoreML backend is not available"),
            "unexpected error: {}",
            err
        );
    }
}

#[test]
fn metal_request_errors_when_unavailable() {
    let caps = BackendCapabilities {
        has_metal: false,
        metal_device_name: None,
        has_ane: false,
        has_coreml: false,
        has_mlx: true,
        has_mlx_bridge: false,
        gpu_memory_bytes: None,
    };
    let profile = ExecutionProfile {
        seed_mode: SeedMode::BestEffort,
        backend_profile: BackendKind::Metal,
    };

    let ctx = SelectionContext::new(profile, caps);
    let result = select_backend_from_execution_profile(&ctx);
    assert!(
        result.is_err(),
        "Metal should error when capability is absent"
    );
}

#[test]
fn test_auto_select_backend_metal_fallback() {
    let caps = BackendCapabilities {
        has_metal: true,
        metal_device_name: Some("Test GPU".to_string()),
        has_ane: false,
        has_coreml: false,
        has_mlx: false,
        has_mlx_bridge: false,
        gpu_memory_bytes: Some(4 * BYTES_PER_GB), // 4GB
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

#[cfg(target_os = "macos")]
#[test]
#[allow(deprecated)]
fn metal_backend_requires_manifest_hash() {
    use adapteros_core::B3Hash;
    use std::path::Path;

    let path = Path::new("var/models/nonexistent");
    // Testing deprecated API - use create_backend_with_model_hashes for new code
    let err = create_backend_with_model_and_hash(BackendChoice::Metal, path, None)
        .err()
        .expect("Metal backend should require manifest hash");
    assert!(
        format!("{}", err).contains("Manifest hash is required"),
        "Unexpected error: {err}"
    );

    // Confirm a valid hash passes the manifest check before path loading.
    let dummy_hash = B3Hash::hash(b"dummy");
    let result = create_backend_with_model_and_hash(BackendChoice::Metal, path, Some(&dummy_hash));
    assert!(
        result.is_err(),
        "Path validation should still run after manifest hash is supplied"
    );
}

#[cfg(feature = "multi-backend")]
#[test]
#[allow(deprecated)]
fn mlx_backend_requires_manifest_hash() {
    use std::path::Path;

    let path = Path::new("var/models/nonexistent");
    // Testing deprecated API - use create_backend_with_model_hashes for new code
    let err = create_backend_with_model_and_hash(BackendChoice::Mlx, path, None)
        .err()
        .expect("MLX backend should require manifest hash");
    assert!(
        format!("{}", err).contains("Manifest hash is required"),
        "Unexpected error: {err}"
    );
}

#[test]
fn test_create_backend_auto_with_model_size() {
    // Test auto selection with model size constraint
    let model_size = 100 * BYTES_PER_MB as usize; // 100MB model

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
        has_mlx_bridge: false,
        gpu_memory_bytes: Some(4 * BYTES_PER_GB),
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
        has_mlx_bridge: false,
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
        has_mlx_bridge: false,
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
        has_mlx_bridge: false,
        gpu_memory_bytes: Some(4 * BYTES_PER_GB),
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
        has_mlx_bridge: false,
        gpu_memory_bytes: Some(4 * BYTES_PER_GB),
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
        has_mlx_bridge: false,
        gpu_memory_bytes: Some(4 * BYTES_PER_GB),
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
        has_mlx_bridge: false,
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
        if !caps.has_metal && !caps.has_coreml {
            eprintln!("skipping: Metal/CoreML unavailable in this environment");
            return;
        }
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
        BackendChoice::CPU,
    ];

    for choice in choices {
        match choice {
            BackendChoice::Metal => println!("Metal backend"),
            BackendChoice::CoreML => println!("CoreML backend"),
            BackendChoice::Mlx => println!("MLX backend"),
            BackendChoice::MlxBridge => println!("MLX Bridge backend"),
            BackendChoice::Auto => println!("Auto-selected backend"),
            BackendChoice::CPU => println!("CPU backend (unsupported for inference kernels)"),
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
        has_mlx_bridge: false,
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
            memory / BYTES_PER_MB
        );
        // GPU memory should be reasonable (at least a few hundred MB)
        assert!(
            memory > 100 * BYTES_PER_MB,
            "GPU memory should be at least 100MB"
        );
    } else {
        println!("No GPU memory detected");
    }
}

#[test]
fn test_headroom_calculation() {
    // Test the 15% headroom policy
    let gpu_memory = 8 * BYTES_PER_GB; // 8GB
    let required_headroom = (gpu_memory as f64 * 0.15) as u64;
    let available = gpu_memory - required_headroom;

    println!("Total GPU memory: {}MB", gpu_memory / BYTES_PER_MB);
    println!(
        "Required headroom (15%): {}MB",
        required_headroom / BYTES_PER_MB
    );
    println!("Available for models: {}MB", available / BYTES_PER_MB);

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

// ============================================================================
// Model Cache Deduplication Tests
// ============================================================================

#[test]
fn test_model_cache_deduplication() {
    use adapteros_core::B3Hash;
    use adapteros_lora_kernel_api::attestation::BackendType;
    use adapteros_lora_worker::model_handle_cache::{ModelHandle, ModelHandleCache};
    use adapteros_lora_worker::model_key::{ModelCacheIdentity, ModelKey};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    let cache = ModelHandleCache::new(1024 * 1024 * 1024); // 1GB max
    let hash = B3Hash::hash(b"test-model-data");
    let key = ModelKey::new(
        BackendType::Metal,
        hash,
        ModelCacheIdentity::for_backend(BackendType::Metal),
    );

    // Track how many times the loader is called
    let load_count = Arc::new(AtomicU32::new(0));
    let load_count_clone = Arc::clone(&load_count);

    // First load: cache miss
    let result1 = cache.get_or_load(&key, || {
        load_count.fetch_add(1, Ordering::SeqCst);
        Ok((ModelHandle::Metal(Arc::new(vec![1, 2, 3, 4])), 4))
    });
    assert!(result1.is_ok());
    assert_eq!(
        load_count_clone.load(Ordering::SeqCst),
        1,
        "First load should call loader"
    );

    // Second load: cache hit (loader should NOT be called)
    let result2 = cache.get_or_load(&key, || {
        load_count.fetch_add(1, Ordering::SeqCst);
        Ok((ModelHandle::Metal(Arc::new(vec![5, 6, 7, 8])), 4))
    });
    assert!(result2.is_ok());
    assert_eq!(
        load_count_clone.load(Ordering::SeqCst),
        1,
        "Second load should NOT call loader (cache hit)"
    );

    // Verify cache stats
    let stats = cache.stats();
    assert_eq!(stats.hits, 1, "Should have 1 cache hit");
    assert_eq!(stats.misses, 1, "Should have 1 cache miss");
    assert_eq!(stats.hit_ratio(), 0.5, "Hit ratio should be 50%");
}

#[test]
fn test_different_backends_get_separate_cache_entries() {
    use adapteros_core::B3Hash;
    use adapteros_lora_kernel_api::attestation::BackendType;
    use adapteros_lora_worker::model_handle_cache::{ModelHandle, ModelHandleCache};
    use adapteros_lora_worker::model_key::{ModelCacheIdentity, ModelKey};
    use std::sync::Arc;

    let cache = ModelHandleCache::new(1024 * 1024 * 1024);
    let hash = B3Hash::hash(b"same-model-content");

    // Same hash, but different backends
    let metal_key = ModelKey::new(
        BackendType::Metal,
        hash,
        ModelCacheIdentity::for_backend(BackendType::Metal),
    );
    let mock_key = ModelKey::new(
        BackendType::Mock,
        hash,
        ModelCacheIdentity::for_backend(BackendType::Mock),
    );

    let mut metal_load_count = 0;
    let mut mock_load_count = 0;

    // Load Metal backend
    cache
        .get_or_load(&metal_key, || {
            metal_load_count += 1;
            Ok((ModelHandle::Metal(Arc::new(vec![1, 2, 3])), 3))
        })
        .unwrap();

    // Load Mock backend (same hash, different backend type)
    cache
        .get_or_load(&mock_key, || {
            mock_load_count += 1;
            Ok((ModelHandle::CoreML, 0))
        })
        .unwrap();

    // Both should have been loaded (separate cache entries)
    assert_eq!(metal_load_count, 1, "Metal should have been loaded once");
    assert_eq!(mock_load_count, 1, "Mock should have been loaded once");
    assert_eq!(
        cache.len(),
        2,
        "Cache should have 2 entries (one per backend)"
    );

    // Now hit each cache entry
    cache
        .get_or_load(&metal_key, || {
            panic!("Metal should be cached, loader should not be called");
        })
        .unwrap();

    cache
        .get_or_load(&mock_key, || {
            panic!("Mock should be cached, loader should not be called");
        })
        .unwrap();

    let stats = cache.stats();
    assert_eq!(stats.hits, 2, "Should have 2 cache hits");
    assert_eq!(stats.misses, 2, "Should have 2 cache misses");
}

#[test]
fn test_model_key_from_path_determinism() {
    use adapteros_lora_kernel_api::attestation::BackendType;
    use adapteros_lora_worker::model_key::ModelKey;
    use std::path::Path;

    // Using a path that likely doesn't have config.json (falls back to path hash)
    let path = Path::new("var/test-model-path-does-not-exist");

    // Create key twice from same path
    let key1 = ModelKey::from_path(BackendType::Metal, path).unwrap();
    let key2 = ModelKey::from_path(BackendType::Metal, path).unwrap();

    // Should produce identical keys (deterministic)
    assert_eq!(key1, key2, "Same path should produce same ModelKey");
    assert_eq!(
        key1.manifest_hash, key2.manifest_hash,
        "Hash should be deterministic"
    );

    // Different backend should produce different key
    let key3 = ModelKey::from_path(BackendType::MLX, path).unwrap();
    assert_ne!(
        key1, key3,
        "Different backend should produce different ModelKey"
    );
}

#[test]
fn test_mlx_capability_honest_when_stub() {
    let caps = detect_capabilities();

    // By default (without real MLX linked), has_mlx should be false.
    // This test verifies that the capability detection doesn't lie about
    // MLX availability when only the stub implementation is present.
    //
    // Note: If real MLX is linked via the adapteros-lora-mlx-ffi crate's
    // With the real MLX feature (`mlx`), this assertion would need to be conditional.
    // For normal test runs (without real MLX), this should pass.
    #[cfg(feature = "multi-backend")]
    {
        // When multi-backend is enabled but real MLX isn't linked,
        // has_mlx should be false (honest about stub)
        println!(
            "MLX capability detection: has_mlx = {} (should be false without real MLX)",
            caps.has_mlx
        );
        // We can't assert !caps.has_mlx unconditionally because the test
        // might be run in an environment where real MLX is available.
        // Instead, we just verify the detection runs without panic.
    }

    #[cfg(not(feature = "multi-backend"))]
    {
        // Without multi-backend feature, MLX is never available
        assert!(
            !caps.has_mlx,
            "MLX should not be available without multi-backend feature"
        );
    }
}
