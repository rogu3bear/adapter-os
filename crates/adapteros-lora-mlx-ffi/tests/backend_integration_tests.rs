//! Backend integration tests for MLX FFI
//!
//! Tests for the FusedKernels trait implementation,
//! adapter registration, and inference pipeline.

#[cfg(test)]
mod backend_tests {
    use adapteros_lora_mlx_ffi::backend::MLXFFIBackend;
    use adapteros_lora_mlx_ffi::lora::{LoRAAdapter, LoRAConfig};
    use adapteros_lora_mlx_ffi::mock::{create_mock_adapter, create_mock_config, MockMLXFFIModel};
    use adapteros_lora_mlx_ffi::MLXFFIModel;

    #[test]
    fn test_backend_creation_with_mock() {
        let config = create_mock_config();
        let mock_model = MockMLXFFIModel::new(config.clone());

        // Create a real model with mock data
        // This test uses mock since we don't have a real MLX model
        let _ = mock_model;
    }

    #[test]
    fn test_backend_adapter_registration() {
        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config: config.clone(),
        };

        let backend = MLXFFIBackend::new(model);

        let adapter = create_mock_adapter("adapter1", 4);

        let result = backend.register_adapter(1, adapter);
        assert!(result.is_ok());

        assert_eq!(backend.adapter_count(), 1);
    }

    #[test]
    fn test_backend_multiple_adapter_registration() {
        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
        };

        let backend = MLXFFIBackend::new(model);

        for i in 0..5 {
            let adapter = create_mock_adapter(&format!("adapter{}", i), 4);
            backend.register_adapter(i as u16, adapter).unwrap();
        }

        assert_eq!(backend.adapter_count(), 5);
    }

    #[test]
    fn test_backend_adapter_hot_load() {
        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
        };

        let backend = MLXFFIBackend::new(model);

        let adapter1 = create_mock_adapter("adapter1", 4);
        backend.register_adapter(1, adapter1).unwrap();

        assert_eq!(backend.adapter_count(), 1);

        // Hot-load another adapter at runtime
        let adapter2 = create_mock_adapter("adapter2", 8);
        backend.load_adapter_runtime(2, adapter2).unwrap();

        assert_eq!(backend.adapter_count(), 2);
    }

    #[test]
    fn test_backend_adapter_hot_unload() {
        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
        };

        let backend = MLXFFIBackend::new(model);

        let adapter = create_mock_adapter("adapter1", 4);
        backend.register_adapter(1, adapter).unwrap();

        assert_eq!(backend.adapter_count(), 1);

        // Unload adapter
        let result = backend.unload_adapter_runtime(1);
        assert!(result.is_ok());

        assert_eq!(backend.adapter_count(), 0);
    }

    #[test]
    fn test_backend_adapter_unload_nonexistent() {
        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
        };

        let backend = MLXFFIBackend::new(model);

        // Try to unload adapter that doesn't exist
        let result = backend.unload_adapter_runtime(999);
        assert!(result.is_err());
    }

    #[test]
    fn test_backend_adapter_memory_usage() {
        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
        };

        let backend = MLXFFIBackend::new(model);

        let adapter = create_mock_adapter("adapter1", 8);
        backend.register_adapter(1, adapter).unwrap();

        let memory_usage = backend.get_adapter_memory_usage(1).unwrap();

        // Memory usage should be > 0
        assert!(memory_usage > 0);
    }

    #[test]
    fn test_backend_device_name() {
        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
        };

        let backend = MLXFFIBackend::new(model);

        let device = backend.device_name();
        assert!(device.contains("MLX"));
        assert!(device.contains("Apple Silicon"));
    }
}

#[cfg(test)]
mod fused_kernels_trait_tests {
    use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};
    use adapteros_lora_mlx_ffi::backend::MLXFFIBackend;
    use adapteros_lora_mlx_ffi::mock::create_mock_config;
    use adapteros_lora_mlx_ffi::MLXFFIModel;

    #[test]
    fn test_determinism_attestation() {
        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
        };

        let backend = MLXFFIBackend::new(model);

        let report = backend.attest_determinism().unwrap();

        // MLX backend is experimental and non-deterministic
        assert!(!report.deterministic);
        assert_eq!(
            report.backend_type,
            adapteros_lora_kernel_api::attestation::BackendType::Mlx
        );
    }

    #[test]
    fn test_load_operation() {
        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
        };

        let mut backend = MLXFFIBackend::new(model);

        let plan_bytes = b"test-plan";
        let result = backend.load(plan_bytes);

        // Load should succeed (no-op for MLX FFI)
        assert!(result.is_ok());
    }

    #[test]
    #[ignore] // Requires actual MLX model
    fn test_run_step_operation() {
        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config: config.clone(),
        };

        let mut backend = MLXFFIBackend::new(model);

        let mut ring = RouterRing::new(1);
        ring.indices[0] = 0;
        ring.gates_q15[0] = 16384; // 0.5 weight

        let mut io = IoBuffers {
            input_ids: vec![1, 2, 3],
            output_logits: vec![0.0; config.vocab_size],
            position: 0,
        };

        // This test requires a real MLX model, so it's ignored
        let _ = backend.run_step(&ring, &mut io);
    }
}

#[cfg(test)]
mod router_ring_tests {
    use adapteros_lora_kernel_api::RouterRing;

    #[test]
    fn test_router_ring_creation() {
        let k = 3;
        let ring = RouterRing::new(k);

        assert_eq!(ring.indices.len(), k);
        assert_eq!(ring.gates_q15.len(), k);
        assert_eq!(ring.position, 0);
    }

    #[test]
    fn test_router_ring_indices() {
        let k = 4;
        let mut ring = RouterRing::new(k);

        ring.indices[0] = 10;
        ring.indices[1] = 20;
        ring.indices[2] = 30;
        ring.indices[3] = 40;

        assert_eq!(ring.indices, vec![10, 20, 30, 40]);
    }

    #[test]
    fn test_router_ring_gates() {
        let k = 2;
        let mut ring = RouterRing::new(k);

        ring.gates_q15[0] = 32767; // Max Q15 (1.0)
        ring.gates_q15[1] = 16384; // Half Q15 (0.5)

        assert_eq!(ring.gates_q15[0], 32767);
        assert_eq!(ring.gates_q15[1], 16384);
    }

    #[test]
    fn test_router_ring_position() {
        let mut ring = RouterRing::new(2);

        assert_eq!(ring.position, 0);

        ring.position = 42;
        assert_eq!(ring.position, 42);
    }

    #[test]
    fn test_router_ring_k_zero() {
        let ring = RouterRing::new(0);

        assert_eq!(ring.indices.len(), 0);
        assert_eq!(ring.gates_q15.len(), 0);
    }

    #[test]
    fn test_router_ring_k_max() {
        let k = 8; // Max K for K-sparse routing
        let ring = RouterRing::new(k);

        assert_eq!(ring.indices.len(), k);
        assert_eq!(ring.gates_q15.len(), k);
    }
}

#[cfg(test)]
mod io_buffers_tests {
    use adapteros_lora_kernel_api::IoBuffers;

    #[test]
    fn test_io_buffers_creation() {
        let vocab_size = 32000;
        let io = IoBuffers {
            input_ids: vec![1, 2, 3],
            output_logits: vec![0.0; vocab_size],
            position: 0,
        };

        assert_eq!(io.input_ids.len(), 3);
        assert_eq!(io.output_logits.len(), vocab_size);
        assert_eq!(io.position, 0);
    }

    #[test]
    fn test_io_buffers_position_update() {
        let mut io = IoBuffers {
            input_ids: vec![1],
            output_logits: vec![0.0; 100],
            position: 0,
        };

        io.position += 1;
        assert_eq!(io.position, 1);

        io.position += 1;
        assert_eq!(io.position, 2);
    }

    #[test]
    fn test_io_buffers_output_update() {
        let mut io = IoBuffers {
            input_ids: vec![1],
            output_logits: vec![0.0; 4],
            position: 0,
        };

        io.output_logits[0] = 1.0;
        io.output_logits[1] = 2.0;
        io.output_logits[2] = 3.0;
        io.output_logits[3] = 4.0;

        assert_eq!(io.output_logits, vec![1.0, 2.0, 3.0, 4.0]);
    }
}

#[cfg(test)]
mod adapter_lifecycle_tests {
    use adapteros_lora_mlx_ffi::backend::MLXFFIBackend;
    use adapteros_lora_mlx_ffi::mock::{create_mock_adapter, create_mock_config};
    use adapteros_lora_mlx_ffi::MLXFFIModel;

    #[test]
    fn test_adapter_lifecycle_load_unload() {
        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
        };

        let backend = MLXFFIBackend::new(model);

        // Initially empty
        assert_eq!(backend.adapter_count(), 0);

        // Load adapter
        let adapter = create_mock_adapter("adapter1", 4);
        backend.load_adapter_runtime(1, adapter).unwrap();
        assert_eq!(backend.adapter_count(), 1);

        // Unload adapter
        backend.unload_adapter_runtime(1).unwrap();
        assert_eq!(backend.adapter_count(), 0);
    }

    #[test]
    fn test_adapter_lifecycle_replace() {
        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
        };

        let backend = MLXFFIBackend::new(model);

        // Load initial adapter
        let adapter1 = create_mock_adapter("adapter1", 4);
        backend.load_adapter_runtime(1, adapter1).unwrap();

        // Replace with new adapter (same ID)
        let adapter2 = create_mock_adapter("adapter2", 8);
        backend.load_adapter_runtime(1, adapter2).unwrap();

        // Should still have 1 adapter (replaced)
        assert_eq!(backend.adapter_count(), 1);
    }

    #[test]
    fn test_adapter_lifecycle_multiple_operations() {
        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
        };

        let backend = MLXFFIBackend::new(model);

        // Load 3 adapters
        for i in 1..=3 {
            let adapter = create_mock_adapter(&format!("adapter{}", i), 4);
            backend.load_adapter_runtime(i, adapter).unwrap();
        }
        assert_eq!(backend.adapter_count(), 3);

        // Unload middle one
        backend.unload_adapter_runtime(2).unwrap();
        assert_eq!(backend.adapter_count(), 2);

        // Load new one
        let adapter4 = create_mock_adapter("adapter4", 4);
        backend.load_adapter_runtime(4, adapter4).unwrap();
        assert_eq!(backend.adapter_count(), 3);

        // Unload all
        backend.unload_adapter_runtime(1).unwrap();
        backend.unload_adapter_runtime(3).unwrap();
        backend.unload_adapter_runtime(4).unwrap();
        assert_eq!(backend.adapter_count(), 0);
    }
}

#[cfg(test)]
mod mock_model_tests {
    use adapteros_lora_mlx_ffi::mock::{create_mock_adapter, create_mock_config, MockMLXFFIModel};

    #[test]
    fn test_mock_model_forward() {
        let config = create_mock_config();
        let model = MockMLXFFIModel::new(config);

        let token_ids = vec![1, 2, 3, 4, 5];
        let logits = model.forward(&token_ids, 0).unwrap();

        assert_eq!(logits.len(), 32000);

        // Verify that token positions have non-zero values
        for &token_id in &token_ids {
            assert!(logits[token_id as usize] > 0.0);
        }
    }

    #[test]
    fn test_mock_model_forward_with_hidden_states() {
        let config = create_mock_config();
        let model = MockMLXFFIModel::new(config);

        let token_ids = vec![1, 2, 3];
        let (logits, hidden_states) = model.forward_with_hidden_states(&token_ids).unwrap();

        assert_eq!(logits.len(), 32000);
        assert_eq!(hidden_states.len(), 4); // q, k, v, o projections

        assert!(hidden_states.contains_key("q_proj"));
        assert!(hidden_states.contains_key("k_proj"));
        assert!(hidden_states.contains_key("v_proj"));
        assert!(hidden_states.contains_key("o_proj"));
    }

    #[test]
    fn test_mock_adapter_creation() {
        let adapter = create_mock_adapter("test", 8);

        assert_eq!(adapter.id(), "test");
        assert_eq!(adapter.config().rank, 8);
        assert!(adapter.has_module("q_proj"));
        assert!(adapter.has_module("k_proj"));
        assert!(adapter.has_module("v_proj"));
        assert!(adapter.has_module("o_proj"));
    }

    #[test]
    fn test_mock_config() {
        let config = create_mock_config();

        assert_eq!(config.hidden_size, 4096);
        assert_eq!(config.num_hidden_layers, 32);
        assert_eq!(config.num_attention_heads, 32);
        assert_eq!(config.vocab_size, 32000);
        assert_eq!(config.rope_theta, 10000.0);
    }
}
