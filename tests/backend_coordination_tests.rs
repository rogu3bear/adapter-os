//! Comprehensive tests for backend coordination and selection

use adapteros_lora_kernel_api::{BackendHealth, FusedKernels, IoBuffers, RouterRing};
use adapteros_lora_worker::backend_coordinator::BackendCoordinator;
use adapteros_lora_worker::backend_factory::{
    create_backend, create_backend_auto, detect_capabilities, BackendCapabilities, BackendChoice,
    BackendStrategy,
};

#[test]
fn test_capability_detection() {
    let caps = detect_capabilities();

    // On macOS, we should detect Metal
    #[cfg(target_os = "macos")]
    {
        assert!(caps.has_metal, "Metal should be available on macOS");
        assert!(caps.metal_device_name.is_some());
        assert!(caps.vram_capacity > 0);
    }

    // On non-macOS, no Apple hardware
    #[cfg(not(target_os = "macos"))]
    {
        assert!(!caps.has_metal);
        assert!(!caps.has_ane);
        assert!(caps.metal_device_name.is_none());
    }

    // System RAM should always be detected
    assert!(caps.system_ram > 0);

    println!("Detected capabilities: {:?}", caps);
}

#[test]
#[cfg(target_os = "macos")]
fn test_metal_backend_creation() {
    let result = create_backend(BackendChoice::Metal);
    assert!(result.is_ok(), "Metal backend should be available on macOS");

    let backend = result.unwrap();
    assert!(backend.device_name().contains("Metal") || backend.device_name().contains("Apple"));

    // Verify attestation
    let report = backend
        .attest_determinism()
        .expect("Attestation should succeed");
    assert!(
        report.deterministic,
        "Metal backend should be deterministic"
    );
    assert!(
        report.metallib_hash.is_some(),
        "Metal should provide metallib hash"
    );
}

#[test]
#[cfg(not(target_os = "macos"))]
fn test_metal_backend_unavailable() {
    let result = create_backend(BackendChoice::Metal);
    assert!(
        result.is_err(),
        "Metal should not be available on non-macOS"
    );
}

#[test]
#[cfg(not(feature = "experimental-backends"))]
fn test_experimental_backends_disabled_by_default() {
    use std::path::PathBuf;

    // MLX should require feature flag
    let mlx_result = create_backend(BackendChoice::Mlx {
        model_path: PathBuf::from("test"),
    });
    assert!(mlx_result.is_err());
    assert!(format!("{:?}", mlx_result.unwrap_err()).contains("experimental-backends"));

    // CoreML should require feature flag
    let coreml_result = create_backend(BackendChoice::CoreML { model_path: None });
    assert!(coreml_result.is_err());
    assert!(format!("{:?}", coreml_result.unwrap_err()).contains("experimental-backends"));
}

#[test]
#[cfg(all(feature = "experimental-backends", target_os = "macos"))]
fn test_coreml_backend_creation() {
    let result = create_backend(BackendChoice::CoreML { model_path: None });

    if result.is_ok() {
        let backend = result.unwrap();
        assert!(backend.device_name().contains("CoreML"));

        let report = backend
            .attest_determinism()
            .expect("Attestation should succeed");
        assert_eq!(
            report.backend_type,
            adapteros_lora_kernel_api::attestation::BackendType::CoreML
        );
    } else {
        // CoreML may not be available in test environment
        println!("CoreML backend not available: {:?}", result.unwrap_err());
    }
}

#[test]
#[cfg(all(feature = "experimental-backends", target_os = "macos"))]
fn test_mlx_backend_deterministic() {
    use std::path::PathBuf;

    let result = create_backend(BackendChoice::Mlx {
        model_path: PathBuf::from("./tests/fixtures/mock-mlx"),
    });

    assert!(
        result.is_ok(),
        "MLX backend should be available with feature flag"
    );

    let mut backend = result.unwrap();
    backend.load(b"test-plan").expect("Load should succeed");

    // Test deterministic execution
    let mut ring = RouterRing::new(4);
    ring.set(&[0, 1, 2, 3], &[16384, 8192, 4096, 2048]);

    let mut io1 = IoBuffers::new(100);
    io1.input_ids = vec![1, 2, 3];
    backend
        .run_step(&ring, &mut io1)
        .expect("First run should succeed");

    // Recreate backend for determinism test
    let mut backend2 = create_backend(BackendChoice::Mlx {
        model_path: PathBuf::from("./tests/fixtures/mock-mlx"),
    })
    .unwrap();
    backend2.load(b"test-plan").expect("Load should succeed");

    let mut io2 = IoBuffers::new(100);
    io2.input_ids = vec![1, 2, 3];
    backend2
        .run_step(&ring, &mut io2)
        .expect("Second run should succeed");

    // Verify deterministic outputs
    assert_eq!(
        io1.output_logits, io2.output_logits,
        "Outputs should be identical"
    );
    assert_eq!(io1.position, io2.position);
}

#[test]
#[cfg(target_os = "macos")]
fn test_backend_strategy_metal_only() {
    let caps = detect_capabilities();
    let strategy = BackendStrategy::MetalOnly;

    let choice = strategy.select_backend(&caps, None);
    assert!(choice.is_ok());
    assert!(matches!(choice.unwrap(), BackendChoice::Metal));
}

#[test]
#[cfg(target_os = "macos")]
fn test_backend_strategy_with_fallback() {
    let caps = detect_capabilities();
    let strategy = BackendStrategy::MetalWithCoreMLFallback;

    // Should select Metal if available
    let choice = strategy.select_backend(&caps, Some(8 * 1024 * 1024 * 1024));
    assert!(choice.is_ok());

    // Should fallback to CoreML for very large models
    if caps.has_ane {
        let choice_large = strategy.select_backend(&caps, Some(100 * 1024 * 1024 * 1024));
        if let Ok(BackendChoice::CoreML { .. }) = choice_large {
            println!("Correctly fell back to CoreML for large model");
        }
    }
}

#[test]
#[cfg(target_os = "macos")]
fn test_backend_strategy_prefer_ane() {
    let caps = detect_capabilities();
    let strategy = BackendStrategy::PreferANE;

    let choice = strategy.select_backend(&caps, None);

    if caps.has_ane {
        assert!(matches!(choice, Ok(BackendChoice::CoreML { .. })));
    } else {
        // Should fallback to Metal
        assert!(matches!(choice, Ok(BackendChoice::Metal)));
    }
}

#[test]
#[cfg(target_os = "macos")]
fn test_create_backend_auto() {
    let result = create_backend_auto(BackendStrategy::MetalOnly, None);
    assert!(result.is_ok());

    let backend = result.unwrap();
    assert!(backend.device_name().len() > 0);
}

#[test]
#[cfg(target_os = "macos")]
fn test_backend_health_check() {
    let backend = create_backend(BackendChoice::Metal).expect("Failed to create backend");

    let health = backend.health_check().expect("Health check should succeed");
    assert!(matches!(health, BackendHealth::Healthy));
}

#[test]
#[cfg(target_os = "macos")]
fn test_backend_metrics() {
    let backend = create_backend(BackendChoice::Metal).expect("Failed to create backend");

    let metrics = backend.get_metrics();
    assert_eq!(metrics.total_operations, 0);
    assert_eq!(metrics.error_count, 0);
}

#[tokio::test]
#[cfg(target_os = "macos")]
async fn test_coordinator_creation() {
    let coordinator = BackendCoordinator::new(BackendStrategy::MetalOnly, false, None).await;

    assert!(coordinator.is_ok(), "Coordinator creation should succeed");

    let coord = coordinator.unwrap();
    let caps = coord.capabilities();
    assert!(caps.has_metal);
}

#[tokio::test]
#[cfg(all(target_os = "macos", feature = "experimental-backends"))]
async fn test_coordinator_with_fallback() {
    let coordinator =
        BackendCoordinator::new(BackendStrategy::MetalWithCoreMLFallback, true, None).await;

    if coordinator.is_ok() {
        let coord = coordinator.unwrap();
        let metrics = coord.get_metrics().await;
        assert_eq!(metrics.total_operations, 0);
        assert_eq!(metrics.backend_switches, 0);
    }
}

#[tokio::test]
#[cfg(target_os = "macos")]
async fn test_coordinator_inference() {
    let coordinator = BackendCoordinator::new(BackendStrategy::MetalOnly, false, None)
        .await
        .expect("Failed to create coordinator");

    let mut ring = RouterRing::new(2);
    ring.set(&[1, 2], &[16384, 8192]);

    let mut io = IoBuffers::new(100);
    io.input_ids = vec![1, 2, 3, 4, 5];

    let result = coordinator.run_step(&ring, &mut io).await;
    assert!(result.is_ok(), "Inference should succeed");

    let metrics = coordinator.get_metrics().await;
    assert_eq!(metrics.total_operations, 1);
    assert_eq!(metrics.primary_operations, 1);
}

#[tokio::test]
#[cfg(target_os = "macos")]
async fn test_coordinator_metrics_tracking() {
    let coordinator = BackendCoordinator::new(BackendStrategy::MetalOnly, false, None)
        .await
        .expect("Failed to create coordinator");

    // Execute multiple operations
    for _ in 0..5 {
        let mut ring = RouterRing::new(1);
        ring.set(&[1], &[16384]);

        let mut io = IoBuffers::new(50);
        io.input_ids = vec![1, 2, 3];

        coordinator
            .run_step(&ring, &mut io)
            .await
            .expect("Operation should succeed");
    }

    let metrics = coordinator.get_metrics().await;
    assert_eq!(metrics.total_operations, 5);
    assert!(metrics.avg_latency_us > 0.0);
}

#[test]
fn test_backend_capabilities_display() {
    let caps = detect_capabilities();

    println!("=== Backend Capabilities ===");
    println!("Metal GPU: {}", caps.has_metal);
    println!("Apple Neural Engine: {}", caps.has_ane);
    println!("MLX Framework: {}", caps.has_mlx);
    println!(
        "VRAM Capacity: {} GB",
        caps.vram_capacity / (1024 * 1024 * 1024)
    );
    println!("System RAM: {} GB", caps.system_ram / (1024 * 1024 * 1024));
    println!("Metal Device: {:?}", caps.metal_device_name);
    println!("ANE Cores: {}", caps.ane_core_count);
}
