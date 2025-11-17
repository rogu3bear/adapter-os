//! Backend RouterRing Validation Tests (PRD 6)
//!
//! Tests backend error handling when receiving invalid RouterRings.
//! Ensures backends fail gracefully with structured errors on contract violations.

#[cfg(target_os = "macos")]
mod metal_backend_tests {
    use adapteros_core::Result;
    use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};
    use adapteros_lora_kernel_mtl::MetalKernels;

    #[test]
    fn test_metal_backend_rejects_unsorted_indices_in_debug() -> Result<()> {
        // PRD 6: Backend MUST validate ring in debug builds
        let mut backend = MetalKernels::new()?;
        let mut io = IoBuffers::new(1000);

        // Create invalid ring (unsorted indices)
        let mut ring = RouterRing::new();
        // Note: We can't bypass validation via set(), so we construct invalid ring manually
        // by modifying internal fields (simulates corrupted data)
        ring.indices = vec![5, 2, 7].into();
        ring.gates_q15 = vec![100, 200, 300].into();

        // In debug mode, backend should reject invalid ring
        #[cfg(debug_assertions)]
        {
            let result = backend.run_step(&ring, &mut io);
            assert!(result.is_err(), "Debug build must reject unsorted indices");
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("sorted ascending"));
        }

        // In release mode, behavior is undefined (no validation overhead)
        #[cfg(not(debug_assertions))]
        {
            let _ = backend.run_step(&ring, &mut io);
            // No assertion - release builds skip validation for performance
        }

        Ok(())
    }

    #[test]
    fn test_metal_backend_rejects_length_mismatch_in_debug() -> Result<()> {
        let mut backend = MetalKernels::new()?;
        let mut io = IoBuffers::new(1000);

        // Create invalid ring (length mismatch)
        let mut ring = RouterRing::new();
        ring.indices = vec![0, 1, 2].into();
        ring.gates_q15 = vec![100, 200].into(); // Mismatched length

        #[cfg(debug_assertions)]
        {
            let result = backend.run_step(&ring, &mut io);
            assert!(result.is_err(), "Debug build must reject length mismatch");
            assert!(result.unwrap_err().to_string().contains("length mismatch"));
        }

        Ok(())
    }

    #[test]
    fn test_metal_backend_rejects_exceeds_max_in_debug() -> Result<()> {
        use adapteros_lora_kernel_api::MAX_ADAPTERS_PER_STEP;

        let mut backend = MetalKernels::new()?;
        let mut io = IoBuffers::new(1000);

        // Create invalid ring (exceeds MAX_ADAPTERS_PER_STEP)
        let mut ring = RouterRing::new();
        let too_many: Vec<u16> = (0..(MAX_ADAPTERS_PER_STEP + 1) as u16).collect();
        let gates: Vec<i16> = vec![100; MAX_ADAPTERS_PER_STEP + 1];

        ring.indices = too_many.into();
        ring.gates_q15 = gates.into();

        #[cfg(debug_assertions)]
        {
            let result = backend.run_step(&ring, &mut io);
            assert!(result.is_err(), "Debug build must reject exceeds max");
            assert!(result.unwrap_err().to_string().contains("exceeds"));
        }

        Ok(())
    }

    #[test]
    fn test_metal_backend_accepts_valid_ring() -> Result<()> {
        let mut backend = MetalKernels::new()?;
        let mut io = IoBuffers::new(1000);

        // Create valid RouterRing
        let mut ring = RouterRing::new();
        ring.set(&[0, 1, 2, 3], &[10000, 15000, 8000, 5000])?;
        ring.validate_invariants()?;

        // Backend should accept valid ring without error
        backend.run_step(&ring, &mut io)?;

        Ok(())
    }

    #[test]
    fn test_metal_backend_accepts_empty_ring() -> Result<()> {
        let mut backend = MetalKernels::new()?;
        let mut io = IoBuffers::new(1000);

        // Empty ring (k=0) should be valid
        let ring = RouterRing::new();
        assert!(ring.validate_invariants().is_ok());

        // Backend should handle empty ring gracefully
        backend.run_step(&ring, &mut io)?;

        Ok(())
    }

    #[test]
    fn test_metal_backend_accepts_max_k_adapters() -> Result<()> {
        use adapteros_lora_kernel_api::MAX_ADAPTERS_PER_STEP;

        let mut backend = MetalKernels::new()?;
        let mut io = IoBuffers::new(1000);

        // Create ring with exactly MAX_ADAPTERS_PER_STEP adapters
        let indices: Vec<u16> = (0..MAX_ADAPTERS_PER_STEP as u16).collect();
        let gates: Vec<i16> = vec![1000, 2000, 3000, 4000, 5000, 6000, 7000, 8000];

        let mut ring = RouterRing::new();
        ring.set(&indices, &gates)?;
        ring.validate_invariants()?;

        // Backend should accept maximum valid ring
        backend.run_step(&ring, &mut io)?;

        Ok(())
    }

    #[test]
    fn test_metal_backend_error_messages_are_clear() -> Result<()> {
        let mut backend = MetalKernels::new()?;
        let mut io = IoBuffers::new(1000);

        // Test 1: Unsorted indices error message
        let mut ring1 = RouterRing::new();
        ring1.indices = vec![5, 2, 7].into();
        ring1.gates_q15 = vec![100, 200, 300].into();

        #[cfg(debug_assertions)]
        {
            let result1 = backend.run_step(&ring1, &mut io);
            if let Err(e) = result1 {
                let msg = e.to_string();
                assert!(
                    msg.contains("sorted") || msg.contains("ascending"),
                    "Error message should mention sorting: {}",
                    msg
                );
            }
        }

        // Test 2: Length mismatch error message
        let mut ring2 = RouterRing::new();
        ring2.indices = vec![0, 1].into();
        ring2.gates_q15 = vec![100, 200, 300].into();

        #[cfg(debug_assertions)]
        {
            let result2 = backend.run_step(&ring2, &mut io);
            if let Err(e) = result2 {
                let msg = e.to_string();
                assert!(
                    msg.contains("length") || msg.contains("mismatch"),
                    "Error message should mention length mismatch: {}",
                    msg
                );
            }
        }

        Ok(())
    }
}

#[cfg(not(target_os = "macos"))]
#[test]
fn test_skipped_metal_backend_tests_on_non_macos() {
    // Metal backend tests are macOS-only
    println!("Metal backend tests skipped on non-macOS platform");
}

// Future: When MLX backend exists, add MLX validation tests
// #[cfg(feature = "mlx")]
// mod mlx_backend_tests {
//     // Similar validation tests for MLX backend
// }
