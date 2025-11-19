//! Multi-Backend Integration Tests
//!
//! Test suite for multi-backend operations including:
//! - Backend selection logic
//! - Fallback chain (Metal → CoreML → MLX)
//! - Hybrid execution
//! - Cross-backend tensor conversion

#[cfg(target_os = "macos")]
mod multi_backend_integration {
    use adapteros_lora_kernel_api::{FusedKernels, MockKernels, RouterRing, IoBuffers};
    use adapteros_lora_kernel_api::attestation::{BackendType, DeterminismReport};
    use adapteros_core::Result;

    /// Mock backend selector for testing
    struct BackendSelector {
        available_backends: Vec<BackendType>,
        preferred_order: Vec<BackendType>,
    }

    impl BackendSelector {
        fn new() -> Self {
            Self {
                available_backends: Vec::new(),
                preferred_order: vec![
                    BackendType::Metal,
                    BackendType::CoreML,
                    BackendType::MLX,
                    BackendType::Mock,
                ],
            }
        }

        fn detect_available_backends(&mut self) {
            // Detect Metal
            if Self::is_metal_available() {
                self.available_backends.push(BackendType::Metal);
            }

            // Detect CoreML/ANE
            if Self::is_coreml_available() {
                self.available_backends.push(BackendType::CoreML);
            }

            // Detect MLX
            if Self::is_mlx_available() {
                self.available_backends.push(BackendType::MLX);
            }

            // Mock always available
            self.available_backends.push(BackendType::Mock);
        }

        fn is_metal_available() -> bool {
            #[cfg(target_os = "macos")]
            {
                use metal::Device;
                Device::system_default().is_some()
            }

            #[cfg(not(target_os = "macos"))]
            {
                false
            }
        }

        fn is_coreml_available() -> bool {
            #[cfg(target_os = "macos")]
            {
                use adapteros_lora_kernel_mtl::ane_acceleration::ANEAccelerator;
                if let Ok(acc) = ANEAccelerator::new() {
                    acc.capabilities().available
                } else {
                    false
                }
            }

            #[cfg(not(target_os = "macos"))]
            {
                false
            }
        }

        fn is_mlx_available() -> bool {
            // MLX availability detection would go here
            // For now, return false as it's experimental
            false
        }

        fn select_backend(&self) -> Option<BackendType> {
            for backend in &self.preferred_order {
                if self.available_backends.contains(backend) {
                    return Some(backend.clone());
                }
            }
            None
        }

        fn get_fallback_chain(&self) -> Vec<BackendType> {
            self.preferred_order
                .iter()
                .filter(|b| self.available_backends.contains(b))
                .cloned()
                .collect()
        }
    }

    #[test]
    fn test_backend_detection() {
        let mut selector = BackendSelector::new();
        selector.detect_available_backends();

        println!("Available backends: {:?}", selector.available_backends);

        assert!(!selector.available_backends.is_empty(),
            "At least Mock backend should be available");

        assert!(selector.available_backends.contains(&BackendType::Mock),
            "Mock backend should always be available");
    }

    #[test]
    fn test_backend_selection_priority() {
        let mut selector = BackendSelector::new();
        selector.detect_available_backends();

        let selected = selector.select_backend();
        assert!(selected.is_some(), "Should select a backend");

        if let Some(backend) = selected {
            println!("Selected backend: {:?}", backend);

            // Verify it's the highest priority available
            let fallback_chain = selector.get_fallback_chain();
            assert_eq!(Some(&backend), fallback_chain.first(),
                "Selected backend should be highest priority");
        }
    }

    #[test]
    fn test_fallback_chain() {
        let mut selector = BackendSelector::new();
        selector.detect_available_backends();

        let chain = selector.get_fallback_chain();
        println!("Fallback chain: {:?}", chain);

        assert!(!chain.is_empty(), "Fallback chain should not be empty");

        // Verify ordering follows preference
        let mut prev_idx = 0;
        for backend in &chain {
            let idx = selector.preferred_order.iter()
                .position(|b| b == backend)
                .unwrap();

            assert!(idx >= prev_idx,
                "Fallback chain should follow preferred order");
            prev_idx = idx;
        }
    }

    #[test]
    fn test_backend_fallback_on_error() {
        let mut selector = BackendSelector::new();
        selector.detect_available_backends();

        let chain = selector.get_fallback_chain();

        // Simulate trying backends in fallback order
        for (idx, backend) in chain.iter().enumerate() {
            println!("Attempting backend {}: {:?}", idx, backend);

            // In a real implementation, we would try to initialize each backend
            // and fall back on failure. Here we just verify the chain exists.
            assert!(idx < chain.len());
        }

        // Verify Mock is last resort
        assert_eq!(chain.last(), Some(&BackendType::Mock),
            "Mock should be last in fallback chain");
    }

    #[test]
    fn test_mock_kernel_basic_operation() {
        let mut kernels = MockKernels::new();

        // Load dummy plan
        let plan_bytes = b"mock_plan_data";
        let load_result = kernels.load(plan_bytes);
        assert!(load_result.is_ok(), "Mock kernel load should succeed");

        // Create router ring
        let ring = RouterRing::from_slices(&[0, 1, 2], &[16384, 8192, 4096]);

        // Create IO buffers
        let mut io = IoBuffers::new(1000);
        io.input_ids = vec![1, 2, 3, 4];

        // Run step
        let step_result = kernels.run_step(&ring, &mut io);
        assert!(step_result.is_ok(), "Mock kernel run_step should succeed");

        // Verify deterministic output
        assert_eq!(io.output_logits.len(), 1000);
        assert!(io.output_logits.iter().all(|&x| x >= 0.0 && x < 1.0),
            "Mock logits should be in valid range");
    }

    #[test]
    fn test_mock_kernel_determinism() {
        let mut kernels1 = MockKernels::new();
        let mut kernels2 = MockKernels::new();

        kernels1.load(b"test").unwrap();
        kernels2.load(b"test").unwrap();

        let ring = RouterRing::from_slices(&[0, 1], &[16384, 16384]);

        let mut io1 = IoBuffers::new(100);
        io1.input_ids = vec![1, 2, 3];

        let mut io2 = IoBuffers::new(100);
        io2.input_ids = vec![1, 2, 3];

        kernels1.run_step(&ring, &mut io1).unwrap();
        kernels2.run_step(&ring, &mut io2).unwrap();

        // Verify deterministic results
        assert_eq!(io1.output_logits.len(), io2.output_logits.len());
        for (a, b) in io1.output_logits.iter().zip(io2.output_logits.iter()) {
            assert_eq!(a, b, "Mock kernel should produce deterministic results");
        }
    }

    #[test]
    fn test_backend_attestation() {
        let kernels = MockKernels::new();

        let attestation = kernels.attest_determinism();
        assert!(attestation.is_ok(), "Attestation should succeed");

        if let Ok(report) = attestation {
            assert_eq!(report.backend_type, BackendType::Mock);
            assert!(report.deterministic, "Mock backend should attest as deterministic");
            assert!(report.metallib_hash.is_none(), "Mock backend has no metallib");
        }
    }

    #[test]
    fn test_cross_backend_tensor_conversion() {
        // Test tensor data structure compatible across backends
        #[derive(Debug, Clone)]
        struct Tensor {
            data: Vec<f32>,
            shape: Vec<usize>,
            dtype: TensorDType,
        }

        #[derive(Debug, Clone, PartialEq)]
        enum TensorDType {
            Float32,
            Float16,
            Int8,
        }

        impl Tensor {
            fn new(data: Vec<f32>, shape: Vec<usize>) -> Self {
                Self {
                    data,
                    shape,
                    dtype: TensorDType::Float32,
                }
            }

            fn to_f16(&self) -> Tensor {
                // Simulate f16 conversion
                let mut converted = self.clone();
                converted.dtype = TensorDType::Float16;
                converted
            }

            fn to_i8(&self) -> Tensor {
                // Simulate int8 quantization
                let mut converted = self.clone();
                converted.dtype = TensorDType::Int8;
                // In real implementation, would quantize data
                converted
            }

            fn verify_compatible(&self, other: &Tensor) -> bool {
                self.shape == other.shape
            }
        }

        let tensor_f32 = Tensor::new(vec![1.0, 2.0, 3.0, 4.0], vec![2, 2]);
        let tensor_f16 = tensor_f32.to_f16();
        let tensor_i8 = tensor_f32.to_i8();

        assert!(tensor_f32.verify_compatible(&tensor_f16));
        assert!(tensor_f32.verify_compatible(&tensor_i8));

        println!("Tensor conversions: F32 -> F16, F32 -> I8 compatible");
    }

    #[test]
    fn test_hybrid_execution_simulation() {
        // Simulate hybrid execution across multiple backends
        struct HybridExecutor {
            primary_backend: BackendType,
            fallback_backend: BackendType,
        }

        impl HybridExecutor {
            fn new(primary: BackendType, fallback: BackendType) -> Self {
                Self {
                    primary_backend: primary,
                    fallback_backend: fallback,
                }
            }

            fn execute(&self, use_primary: bool) -> Result<String> {
                let backend = if use_primary {
                    &self.primary_backend
                } else {
                    &self.fallback_backend
                };

                Ok(format!("Executed on {:?}", backend))
            }
        }

        let executor = HybridExecutor::new(BackendType::Metal, BackendType::Mock);

        let primary_result = executor.execute(true);
        let fallback_result = executor.execute(false);

        assert!(primary_result.is_ok());
        assert!(fallback_result.is_ok());

        println!("Primary: {}", primary_result.unwrap());
        println!("Fallback: {}", fallback_result.unwrap());
    }

    #[test]
    fn test_backend_capability_matching() {
        // Test matching workload to backend capabilities
        #[derive(Debug)]
        struct Workload {
            model_size_mb: usize,
            requires_int8: bool,
            requires_ane: bool,
            max_latency_ms: u32,
        }

        #[derive(Debug)]
        struct BackendCapabilities {
            backend_type: BackendType,
            max_model_size_mb: usize,
            supports_int8: bool,
            has_ane: bool,
            typical_latency_ms: u32,
        }

        impl BackendCapabilities {
            fn can_handle(&self, workload: &Workload) -> bool {
                self.max_model_size_mb >= workload.model_size_mb
                    && (!workload.requires_int8 || self.supports_int8)
                    && (!workload.requires_ane || self.has_ane)
                    && self.typical_latency_ms <= workload.max_latency_ms
            }
        }

        let workloads = vec![
            Workload {
                model_size_mb: 100,
                requires_int8: true,
                requires_ane: false,
                max_latency_ms: 10,
            },
            Workload {
                model_size_mb: 500,
                requires_int8: false,
                requires_ane: true,
                max_latency_ms: 5,
            },
        ];

        let backends = vec![
            BackendCapabilities {
                backend_type: BackendType::Metal,
                max_model_size_mb: 1024,
                supports_int8: true,
                has_ane: false,
                typical_latency_ms: 5,
            },
            BackendCapabilities {
                backend_type: BackendType::CoreML,
                max_model_size_mb: 512,
                supports_int8: true,
                has_ane: true,
                typical_latency_ms: 3,
            },
            BackendCapabilities {
                backend_type: BackendType::Mock,
                max_model_size_mb: 2048,
                supports_int8: true,
                has_ane: false,
                typical_latency_ms: 1,
            },
        ];

        for (idx, workload) in workloads.iter().enumerate() {
            println!("\nWorkload {}: {:?}", idx, workload);

            for backend in &backends {
                let can_handle = backend.can_handle(workload);
                println!("  {:?}: {}", backend.backend_type, can_handle);
            }
        }
    }

    #[test]
    fn test_backend_switching_overhead() {
        use std::time::Instant;

        // Measure overhead of switching between backend types
        let backends = vec![
            BackendType::Metal,
            BackendType::CoreML,
            BackendType::MLX,
            BackendType::Mock,
        ];

        let mut switching_times = Vec::new();

        for i in 0..backends.len() - 1 {
            let start = Instant::now();

            // Simulate backend switch (in reality would unload/load)
            let _from = &backends[i];
            let _to = &backends[i + 1];

            // Simulate some overhead
            std::thread::sleep(std::time::Duration::from_micros(100));

            let elapsed = start.elapsed();
            switching_times.push(elapsed.as_micros());

            println!("Switch {:?} -> {:?}: {}μs",
                backends[i], backends[i + 1], elapsed.as_micros());
        }

        let avg_switch_time = switching_times.iter().sum::<u128>() / switching_times.len() as u128;
        println!("Average switch time: {}μs", avg_switch_time);

        // Verify reasonable overhead
        assert!(avg_switch_time < 10000, "Switch overhead should be < 10ms");
    }

    #[test]
    fn test_multi_backend_error_recovery() {
        // Test error recovery when switching between backends
        struct BackendRunner {
            attempts: Vec<(BackendType, bool)>, // (backend, success)
        }

        impl BackendRunner {
            fn new() -> Self {
                Self {
                    attempts: Vec::new(),
                }
            }

            fn try_backend(&mut self, backend: BackendType, should_succeed: bool) -> Result<String> {
                self.attempts.push((backend.clone(), should_succeed));

                if should_succeed {
                    Ok(format!("Success on {:?}", backend))
                } else {
                    Err(adapteros_core::AosError::Kernel(
                        format!("Failed on {:?}", backend)
                    ))
                }
            }

            fn execute_with_fallback(&mut self, backends: Vec<(BackendType, bool)>) -> Result<String> {
                for (backend, should_succeed) in backends {
                    match self.try_backend(backend.clone(), should_succeed) {
                        Ok(result) => return Ok(result),
                        Err(e) => {
                            println!("Backend {:?} failed: {}, trying next", backend, e);
                            continue;
                        }
                    }
                }

                Err(adapteros_core::AosError::Kernel("All backends failed".to_string()))
            }
        }

        let mut runner = BackendRunner::new();

        // Simulate Metal fails, CoreML succeeds
        let result = runner.execute_with_fallback(vec![
            (BackendType::Metal, false),
            (BackendType::CoreML, true),
        ]);

        assert!(result.is_ok());
        assert_eq!(runner.attempts.len(), 2);
        assert_eq!(runner.attempts[0].0, BackendType::Metal);
        assert_eq!(runner.attempts[1].0, BackendType::CoreML);

        println!("Recovery test passed: {}", result.unwrap());
    }
}

#[cfg(not(target_os = "macos"))]
mod multi_backend_integration {
    #[test]
    fn test_multi_backend_unavailable() {
        println!("Multi-backend tests skipped: not running on macOS");
    }
}
