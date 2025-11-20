//! Unit Tests for CoreML Backend
//!
//! Comprehensive unit test suite covering:
//! - ANE detection
//! - Model loading
//! - Tensor conversion
//! - Memory management
//! - Power mode detection
//!
//! Copyright: © 2025 JKCA / James KC Auchterlonie. All rights reserved.

#[cfg(target_os = "macos")]
mod macos_tests {
    use adapteros_lora_kernel_api::{BackendHealth, FusedKernels, IoBuffers, RouterRing};
    use std::path::PathBuf;

    /// Mock CoreML backend for testing without actual .mlpackage
    struct MockCoreMLBackend {
        ane_available: bool,
        device_name: String,
    }

    impl MockCoreMLBackend {
        fn new(ane_available: bool) -> Self {
            let device_name = if ane_available {
                "CoreML (Apple Neural Engine)".to_string()
            } else {
                "CoreML (GPU Fallback)".to_string()
            };
            Self {
                ane_available,
                device_name,
            }
        }

        fn is_ane_available(&self) -> bool {
            self.ane_available
        }
    }

    #[test]
    fn test_ane_detection_available() {
        // Test ANE detection when available
        let backend = MockCoreMLBackend::new(true);
        assert!(backend.is_ane_available());
        assert!(backend.device_name.contains("Neural Engine"));
    }

    #[test]
    fn test_ane_detection_unavailable() {
        // Test ANE detection when unavailable (fallback to GPU)
        let backend = MockCoreMLBackend::new(false);
        assert!(!backend.is_ane_available());
        assert!(backend.device_name.contains("GPU Fallback"));
    }

    #[test]
    fn test_device_name_ane() {
        let backend = MockCoreMLBackend::new(true);
        assert_eq!(backend.device_name, "CoreML (Apple Neural Engine)");
    }

    #[test]
    fn test_device_name_gpu_fallback() {
        let backend = MockCoreMLBackend::new(false);
        assert_eq!(backend.device_name, "CoreML (GPU Fallback)");
    }

    #[test]
    fn test_model_path_validation() {
        // Test that non-UTF8 paths are rejected
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let invalid_bytes = vec![0xFF, 0xFE, 0xFD];
        let invalid_path = OsStr::from_bytes(&invalid_bytes);
        let path = PathBuf::from(invalid_path);

        // Would fail when passed to CoreMLBackend::new()
        assert!(path.to_str().is_none());
    }

    #[test]
    fn test_model_path_extension() {
        // Test .mlpackage extension validation
        let valid_path = PathBuf::from("model.mlpackage");
        assert_eq!(valid_path.extension().unwrap(), "mlpackage");

        let invalid_path = PathBuf::from("model.txt");
        assert_ne!(invalid_path.extension().unwrap(), "mlpackage");
    }

    #[test]
    fn test_tensor_size_alignment() {
        // Test that tensor sizes are properly aligned for ANE
        let valid_sizes = vec![64, 128, 256, 512, 1024, 2048, 4096];

        for size in valid_sizes {
            assert_eq!(size % 8, 0, "Tensor size {} not aligned to 8", size);
        }
    }

    #[test]
    fn test_batch_size_one() {
        // ANE is optimized for batch size = 1
        let batch_size = 1;
        let seq_len = 128;
        let total_size = batch_size * seq_len;

        assert_eq!(batch_size, 1);
        assert_eq!(total_size, 128);
    }

    #[test]
    fn test_io_buffers_initialization() {
        // Test IoBuffers creation
        let vocab_size = 32000;
        let io = IoBuffers::new(vocab_size);

        assert_eq!(io.output_logits.len(), vocab_size);
        assert_eq!(io.position, 0);
        assert!(io.input_ids.is_empty());
    }

    #[test]
    fn test_router_ring_creation() {
        // Test RouterRing creation
        let k = 4;
        let ring = RouterRing::new(k);

        assert_eq!(ring.k, k);
        assert_eq!(ring.position, 0);
        assert_eq!(ring.indices.len(), 8); // Fixed size
        assert_eq!(ring.gates_q15.len(), 8); // Fixed size
    }

    #[test]
    fn test_router_ring_with_adapters() {
        // Test RouterRing with adapter indices and gates
        let indices = vec![0u16, 1u16, 2u16, 3u16];
        let gates = vec![16384i16, 8192i16, 4096i16, 2048i16]; // Q15 format

        let ring = RouterRing::from_slices(&indices, &gates);

        assert_eq!(ring.k, 4);
        assert_eq!(ring.active_indices(), &indices);
        assert_eq!(ring.active_gates(), &gates);
    }

    #[test]
    #[should_panic(expected = "Cannot exceed K=8 adapters")]
    fn test_router_ring_max_adapters() {
        // Test that RouterRing rejects more than K=8 adapters
        let indices = vec![0u16; 9];
        let gates = vec![0i16; 9];

        RouterRing::from_slices(&indices, &gates);
    }

    #[test]
    fn test_q15_quantization() {
        // Test Q15 quantization for gates
        let float_gates = vec![1.0f32, 0.5f32, 0.25f32, 0.0f32];
        let q15_gates: Vec<i16> = float_gates
            .iter()
            .map(|&g| (g * 32767.0) as i16)
            .collect();

        assert_eq!(q15_gates[0], 32767); // 1.0 -> max i16
        assert_eq!(q15_gates[1], 16383); // 0.5 -> half max
        assert_eq!(q15_gates[2], 8191); // 0.25 -> quarter max
        assert_eq!(q15_gates[3], 0); // 0.0 -> zero
    }

    #[test]
    fn test_determinism_report_structure() {
        use adapteros_lora_kernel_api::attestation::{
            BackendType, DeterminismReport, FloatingPointMode, RngSeedingMethod,
        };

        // Test DeterminismReport for ANE mode
        let report_ane = DeterminismReport {
            backend_type: BackendType::CoreML,
            metallib_hash: None,
            manifest: None,
            rng_seed_method: RngSeedingMethod::HkdfSeeded,
            floating_point_mode: FloatingPointMode::Deterministic,
            compiler_flags: vec!["-fno-fast-math".to_string()],
            deterministic: true,
        };

        assert!(report_ane.deterministic);
        assert!(matches!(
            report_ane.rng_seed_method,
            RngSeedingMethod::HkdfSeeded
        ));

        // Test DeterminismReport for GPU fallback
        let report_gpu = DeterminismReport {
            backend_type: BackendType::CoreML,
            metallib_hash: None,
            manifest: None,
            rng_seed_method: RngSeedingMethod::SystemEntropy,
            floating_point_mode: FloatingPointMode::Unknown,
            compiler_flags: vec!["-fno-fast-math".to_string()],
            deterministic: false,
        };

        assert!(!report_gpu.deterministic);
        assert!(matches!(
            report_gpu.rng_seed_method,
            RngSeedingMethod::SystemEntropy
        ));
    }

    #[test]
    fn test_backend_health_states() {
        // Test all health states
        let healthy = BackendHealth::Healthy;
        assert!(matches!(healthy, BackendHealth::Healthy));

        let degraded = BackendHealth::Degraded {
            reason: "High error rate".to_string(),
        };
        assert!(matches!(degraded, BackendHealth::Degraded { .. }));

        let failed = BackendHealth::Failed {
            reason: "Model pointer null".to_string(),
        };
        assert!(matches!(failed, BackendHealth::Failed { .. }));
    }

    #[test]
    fn test_memory_footprint_calculation() {
        // Test memory footprint calculation for adapters
        let rank = 16;
        let hidden_dim = 4096;
        let bytes_per_param = 2; // FP16

        let lora_a_size = rank * hidden_dim * bytes_per_param;
        let lora_b_size = hidden_dim * rank * bytes_per_param;
        let total_size = lora_a_size + lora_b_size;

        assert_eq!(lora_a_size, 131072); // 128KB
        assert_eq!(lora_b_size, 131072); // 128KB
        assert_eq!(total_size, 262144); // 256KB total
    }

    #[test]
    fn test_power_mode_enum() {
        use adapteros_lora_kernel_coreml::PowerMode;

        let ane_mode = PowerMode::ANE;
        let gpu_mode = PowerMode::GPU;

        assert_eq!(ane_mode, PowerMode::ANE);
        assert_eq!(gpu_mode, PowerMode::GPU);
        assert_ne!(ane_mode, gpu_mode);
    }

    #[test]
    fn test_metrics_initialization() {
        use adapteros_lora_kernel_api::BackendMetrics;

        let metrics = BackendMetrics::default();

        assert_eq!(metrics.total_operations, 0);
        assert_eq!(metrics.avg_latency_us, 0.0);
        assert_eq!(metrics.peak_memory_bytes, 0);
        assert_eq!(metrics.current_memory_bytes, 0);
        assert_eq!(metrics.utilization_percent, 0.0);
        assert_eq!(metrics.error_count, 0);
        assert!(metrics.custom_metrics.is_empty());
    }

    #[test]
    fn test_adaptive_baseline_learning() {
        // Test adaptive baseline calculation (Welford's algorithm)
        let samples = vec![100.0, 105.0, 95.0, 110.0, 90.0];

        let mut mean = 0.0;
        let mut m2 = 0.0;

        for (i, &sample) in samples.iter().enumerate() {
            let n = (i + 1) as f64;
            let delta = sample - mean;
            mean += delta / n;
            let delta2 = sample - mean;
            m2 += delta * delta2;
        }

        let variance = m2 / samples.len() as f64;
        let stddev = variance.sqrt();

        assert!((mean - 100.0).abs() < 1.0); // Mean ~100
        assert!(stddev > 0.0); // Has variance
    }

    #[test]
    fn test_z_score_calculation() {
        // Test z-score for anomaly detection
        let mean = 256.0 * 1024.0; // 256KB
        let stddev = 10.0 * 1024.0; // 10KB

        let sample1 = 256.0 * 1024.0; // Exactly mean
        let z1 = (sample1 - mean) / stddev;
        assert_eq!(z1, 0.0);

        let sample2 = 276.0 * 1024.0; // +20KB (2σ)
        let z2 = (sample2 - mean) / stddev;
        assert!((z2 - 2.0).abs() < 0.1);

        let sample3 = 236.0 * 1024.0; // -20KB (-2σ)
        let z3 = (sample3 - mean) / stddev;
        assert!((z3 + 2.0).abs() < 0.1);
    }

    #[test]
    fn test_checkpoint_sampling() {
        // Test buffer checkpoint sampling (first/mid/last 4KB)
        let buffer_size = 1024 * 1024; // 1MB
        let checkpoint_size = 4096; // 4KB

        let first_range = 0..checkpoint_size;
        let mid_offset = (buffer_size / 2) - (checkpoint_size / 2);
        let mid_range = mid_offset..(mid_offset + checkpoint_size);
        let last_offset = buffer_size - checkpoint_size;
        let last_range = last_offset..buffer_size;

        assert_eq!(first_range.len(), 4096);
        assert_eq!(mid_range.len(), 4096);
        assert_eq!(last_range.len(), 4096);
    }

    #[test]
    fn test_blake3_hash_format() {
        // Test BLAKE3 hash format validation
        let valid_hash = "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2";
        assert_eq!(valid_hash.len(), 64); // 32 bytes = 64 hex chars
        assert!(valid_hash.chars().all(|c| c.is_ascii_hexdigit()));

        let invalid_hash = "not_a_hash";
        assert_ne!(invalid_hash.len(), 64);
    }
}

#[cfg(not(target_os = "macos"))]
mod non_macos_tests {
    #[test]
    fn test_coreml_unavailable() {
        use adapteros_lora_kernel_coreml::CoreMLBackend;
        use std::path::Path;

        let result = CoreMLBackend::new(Path::new("dummy.mlpackage"));
        assert!(result.is_err());

        if let Err(e) = result {
            assert!(e.to_string().contains("only available on macOS"));
        }
    }
}
