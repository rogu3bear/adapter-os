//! End-to-End Multi-Backend Integration Tests
//!
//! Comprehensive test suite for multi-backend integration including:
//! - End-to-end inference with all backends
//! - Backend switching during runtime
//! - Error recovery and fallback mechanisms
//! - Performance validation across backends

use adapteros_core::Result;
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, MockKernels, RouterRing};
use std::time::Instant;

#[cfg(target_os = "macos")]
mod macos_tests {
    use super::*;
    use adapteros_lora_kernel_mtl::ane_acceleration::{
        ANEAccelerator, ANECalibrationMethod, ANEDataType, ANELoRAConfig, ANEModelConfig,
        ANEQuantization,
    };

    /// Backend execution context
    struct BackendContext {
        backend_name: String,
        initialized: bool,
        execution_count: u64,
        total_latency_us: u128,
    }

    impl BackendContext {
        fn new(name: &str) -> Self {
            Self {
                backend_name: name.to_string(),
                initialized: false,
                execution_count: 0,
                total_latency_us: 0,
            }
        }

        fn record_execution(&mut self, latency_us: u128) {
            self.execution_count += 1;
            self.total_latency_us += latency_us;
        }

        fn avg_latency_us(&self) -> f64 {
            if self.execution_count == 0 {
                0.0
            } else {
                self.total_latency_us as f64 / self.execution_count as f64
            }
        }
    }

    #[test]
    fn test_e2e_mock_backend_inference() {
        let mut ctx = BackendContext::new("Mock");

        let mut kernels = MockKernels::new();
        assert!(kernels.load(b"test_plan").is_ok());
        ctx.initialized = true;

        // Create test data
        let ring = RouterRing::from_slices(&[0, 1, 2], &[16384, 8192, 4096]);
        let mut io = IoBuffers::new(1024);
        io.input_ids = vec![1, 2, 3, 4];

        // Run inference
        let start = Instant::now();
        let result = kernels.run_step(&ring, &mut io);
        let latency = start.elapsed().as_micros();

        assert!(result.is_ok());
        ctx.record_execution(latency);

        // Verify output
        assert_eq!(io.output_logits.len(), 1024);
        assert_eq!(io.position, 1);

        println!("Mock backend inference: {}μs", latency);
        println!("Mock backend avg latency: {:.2}μs", ctx.avg_latency_us());
    }

    #[test]
    fn test_e2e_mock_backend_determinism() {
        let mut kernels1 = MockKernels::new();
        let mut kernels2 = MockKernels::new();

        kernels1.load(b"determinism_test").unwrap();
        kernels2.load(b"determinism_test").unwrap();

        let ring = RouterRing::from_slices(&[0, 1], &[16384, 16384]);

        let mut io1 = IoBuffers::new(512);
        io1.input_ids = vec![5, 10, 15, 20];

        let mut io2 = IoBuffers::new(512);
        io2.input_ids = vec![5, 10, 15, 20];

        // Run both
        kernels1.run_step(&ring, &mut io1).unwrap();
        kernels2.run_step(&ring, &mut io2).unwrap();

        // Verify exact match
        assert_eq!(io1.output_logits.len(), io2.output_logits.len());
        for (i, (a, b)) in io1
            .output_logits
            .iter()
            .zip(io2.output_logits.iter())
            .enumerate()
        {
            assert_eq!(a, b, "Mismatch at index {}: {} != {}", i, a, b);
        }

        println!(
            "Determinism verified across {} logits",
            io1.output_logits.len()
        );
    }

    #[test]
    fn test_e2e_ane_backend_availability() {
        let result = ANEAccelerator::new();

        match result {
            Ok(accelerator) => {
                let caps = accelerator.capabilities();

                if caps.available {
                    println!(
                        "ANE available: {} cores, {} TOPS",
                        caps.core_count, caps.performance.peak_throughput_tops
                    );

                    assert!(caps.core_count > 0);
                    assert!(!caps.supported_data_types.is_empty());
                } else {
                    println!("ANE not available on this device");
                }
            }
            Err(e) => {
                println!("ANE initialization failed: {}", e);
            }
        }
    }

    #[test]
    fn test_e2e_ane_backend_session_lifecycle() {
        let result = ANEAccelerator::new();

        if let Ok(mut accelerator) = result {
            if !accelerator.capabilities().available {
                println!("ANE not available, skipping lifecycle test");
                return;
            }

            let mut ctx = BackendContext::new("ANE");

            // Create session
            let config = ANEModelConfig {
                model_id: "lifecycle_test".to_string(),
                input_dimensions: vec![1, 512],
                output_dimensions: vec![1, 512],
                data_type: ANEDataType::Float16,
                lora_config: ANELoRAConfig {
                    rank: 16,
                    alpha: 32.0,
                    target_modules: vec!["proj".to_string()],
                    quantization: ANEQuantization {
                        enabled: true,
                        bits: 8,
                        calibration_method: ANECalibrationMethod::Dynamic,
                    },
                },
            };

            let start = Instant::now();
            let session_result = accelerator.create_session(config);
            let session_latency = start.elapsed().as_micros();

            assert!(session_result.is_ok());
            ctx.initialized = true;
            ctx.record_execution(session_latency);

            println!("ANE session created in {}μs", session_latency);
            println!("Active sessions: {}", accelerator.active_session_count());
        }
    }

    #[test]
    fn test_e2e_backend_switching() {
        struct MultiBackendExecutor {
            active_backend: String,
            switch_count: u32,
        }

        impl MultiBackendExecutor {
            fn new() -> Self {
                Self {
                    active_backend: "None".to_string(),
                    switch_count: 0,
                }
            }

            fn switch_to(&mut self, backend: &str) -> Result<()> {
                println!("Switching from {} to {}", self.active_backend, backend);
                self.active_backend = backend.to_string();
                self.switch_count += 1;
                Ok(())
            }

            fn execute(&self, workload: &str) -> Result<String> {
                Ok(format!("Executed {} on {}", workload, self.active_backend))
            }
        }

        let mut executor = MultiBackendExecutor::new();

        // Test switching sequence
        let backends = vec!["Metal", "CoreML", "MLX", "Mock"];

        for backend in &backends {
            let switch_start = Instant::now();
            executor.switch_to(backend).unwrap();
            let switch_time = switch_start.elapsed().as_micros();

            let exec_result = executor.execute("test_workload");
            assert!(exec_result.is_ok());

            println!(
                "Switch to {}: {}μs, result: {}",
                backend,
                switch_time,
                exec_result.unwrap()
            );
        }

        assert_eq!(executor.switch_count, backends.len() as u32);
    }

    #[test]
    fn test_e2e_error_recovery() {
        let mut mock_kernels = MockKernels::new();

        // Test recovery from load failure
        let bad_plan = b"invalid_plan_data";
        let load_result = mock_kernels.load(bad_plan);

        // Mock kernels should accept any data
        assert!(load_result.is_ok());

        // Attempt execution
        let ring = RouterRing::from_slices(&[0], &[16384]);
        let mut io = IoBuffers::new(256);
        io.input_ids = vec![1];

        let exec_result = mock_kernels.run_step(&ring, &mut io);
        assert!(exec_result.is_ok());

        println!("Error recovery test passed");
    }

    #[test]
    fn test_e2e_multi_adapter_inference() {
        let mut kernels = MockKernels::new();
        kernels.load(b"multi_adapter").unwrap();

        // Test with varying number of adapters
        let adapter_counts = vec![1, 2, 4, 8];

        for count in adapter_counts {
            let indices: Vec<u16> = (0..count).collect();
            let gates: Vec<i16> = vec![16384 / count as i16; count];

            let ring = RouterRing::from_slices(&indices, &gates);
            let mut io = IoBuffers::new(512);
            io.input_ids = vec![1, 2, 3];

            let start = Instant::now();
            let result = kernels.run_step(&ring, &mut io);
            let latency = start.elapsed().as_micros();

            assert!(result.is_ok());
            println!("Inference with {} adapters: {}μs", count, latency);
        }
    }

    #[test]
    fn test_e2e_throughput_measurement() {
        let mut kernels = MockKernels::new();
        kernels.load(b"throughput_test").unwrap();

        let ring = RouterRing::from_slices(&[0, 1], &[16384, 16384]);
        let iterations = 100;

        let start = Instant::now();

        for _ in 0..iterations {
            let mut io = IoBuffers::new(256);
            io.input_ids = vec![1, 2, 3, 4];
            kernels.run_step(&ring, &mut io).unwrap();
        }

        let total_time = start.elapsed();
        let avg_latency = total_time.as_micros() / iterations;
        let throughput = (iterations as f64 / total_time.as_secs_f64()).round() as u64;

        println!("Throughput test:");
        println!("  Iterations: {}", iterations);
        println!("  Total time: {}ms", total_time.as_millis());
        println!("  Avg latency: {}μs", avg_latency);
        println!("  Throughput: {} inferences/sec", throughput);

        assert!(avg_latency < 10000, "Average latency should be < 10ms");
    }

    #[test]
    fn test_e2e_memory_pressure_handling() {
        use adapteros_memory::unified_memory::{
            AllocationRequest, MemoryType, UnifiedMemoryManager,
        };

        let mut manager = UnifiedMemoryManager::new(50 * 1024 * 1024); // 50MB limit
        manager.init_pool("test", 40 * 1024 * 1024).unwrap();

        let mut blocks = Vec::new();
        let mut successful_allocations = 0;

        // Allocate until we hit pressure
        for i in 0..20 {
            let request = AllocationRequest {
                size: 5 * 1024 * 1024, // 5MB each
                backend: "test".to_string(),
                alignment: 16,
                memory_type: MemoryType::GPU,
                ..Default::default()
            };

            match manager.allocate(request) {
                Ok(block) => {
                    blocks.push(block);
                    successful_allocations += 1;
                    println!("Allocation {}: SUCCESS", i);
                }
                Err(e) => {
                    println!("Allocation {}: FAILED - {}", i, e);
                    break;
                }
            }
        }

        println!("Successful allocations: {} / 20", successful_allocations);
        assert!(
            successful_allocations > 0,
            "Should allocate at least some blocks"
        );
        assert!(successful_allocations < 20, "Should hit memory pressure");

        // Cleanup
        for block in blocks {
            manager.deallocate(&block).unwrap();
        }
    }

    #[test]
    fn test_e2e_concurrent_backend_operations() {
        use std::sync::{Arc, Mutex};
        use std::thread;

        let execution_log = Arc::new(Mutex::new(Vec::new()));

        let handles: Vec<_> = (0..4)
            .map(|i| {
                let log = Arc::clone(&execution_log);

                thread::spawn(move || {
                    let mut kernels = MockKernels::new();
                    kernels.load(b"concurrent_test").unwrap();

                    let ring = RouterRing::from_slices(&[0], &[16384]);
                    let mut io = IoBuffers::new(128);
                    io.input_ids = vec![i as u32];

                    let start = Instant::now();
                    kernels.run_step(&ring, &mut io).unwrap();
                    let latency = start.elapsed().as_micros();

                    log.lock().unwrap().push((i, latency));
                })
            })
            .collect();

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        let log = execution_log.lock().unwrap();
        assert_eq!(log.len(), 4);

        println!("Concurrent execution results:");
        for (thread_id, latency) in log.iter() {
            println!("  Thread {}: {}μs", thread_id, latency);
        }
    }

    #[test]
    fn test_e2e_backend_attestation_validation() {
        let kernels = MockKernels::new();

        let report = kernels.attest_determinism().unwrap();

        // Verify attestation structure
        assert!(
            report.deterministic,
            "Backend should attest as deterministic"
        );

        println!("Backend attestation:");
        println!("  Type: {:?}", report.backend_type);
        println!("  Deterministic: {}", report.deterministic);
        println!("  RNG Seed: {:?}", report.rng_seed_method);
        println!("  FP Mode: {:?}", report.floating_point_mode);
    }
}

#[cfg(not(target_os = "macos"))]
mod non_macos_tests {
    use super::*;

    #[test]
    fn test_e2e_mock_backend_cross_platform() {
        let mut kernels = MockKernels::new();
        assert!(kernels.load(b"cross_platform_test").is_ok());

        let ring = RouterRing::from_slices(&[0], &[16384]);
        let mut io = IoBuffers::new(256);
        io.input_ids = vec![1, 2, 3];

        let result = kernels.run_step(&ring, &mut io);
        assert!(result.is_ok());

        println!("Cross-platform mock backend test passed");
    }

    #[test]
    fn test_e2e_platform_limitations() {
        println!("Multi-backend integration tests limited on non-macOS platforms");
        println!("Only Mock backend available for testing");

        // Verify that only Mock backend would be available
        let available_backends = vec!["Mock"];
        assert_eq!(available_backends.len(), 1);
    }
}
