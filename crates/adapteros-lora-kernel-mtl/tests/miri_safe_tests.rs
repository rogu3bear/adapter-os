//! MIRI-safe tests for pure-Rust unsafe code paths.
//!
//! This module contains tests that MIRI can analyze - they avoid:
//! - FFI calls to Metal/CoreML (unsupported by MIRI)
//! - Inline assembly
//! - System calls
//!
//! Tests here verify memory safety of pure-Rust unsafe operations such as:
//! - Pointer arithmetic and slice creation
//! - Q15 quantization conversions
//! - Ring buffer layout and memory operations
//! - Buffer bounds checking logic
//!
//! Run with: `cargo +nightly miri test -p adapteros-lora-kernel-mtl --test miri_safe_tests`

/// Test Q15 gate quantization conversion (pure arithmetic, no FFI).
///
/// Q15 format uses i16 with denominator 32767.0 for fixed-point representation.
/// This is critical for deterministic router gate calculations.
mod q15_conversion {
    const Q15_DENOMINATOR: f32 = 32767.0;

    /// Convert float gate to signed Q15 format.
    fn float_to_q15(gate: f32) -> i16 {
        (gate.clamp(-1.0, 1.0) * Q15_DENOMINATOR) as i16
    }

    /// Convert signed Q15 gate to float.
    fn q15_to_float(gate: i16) -> f32 {
        gate as f32 / Q15_DENOMINATOR
    }

    #[test]
    fn test_q15_zero() {
        assert_eq!(float_to_q15(0.0), 0);
        assert_eq!(q15_to_float(0), 0.0);
    }

    #[test]
    fn test_q15_positive() {
        // 0.5 should convert to approximately 16383
        let q15 = float_to_q15(0.5);
        assert_eq!(q15, 16383);

        // 1.0 should convert to 32767 (max positive)
        assert_eq!(float_to_q15(1.0), 32767);
    }

    #[test]
    fn test_q15_negative() {
        // -1.0 should convert to -32767 (max negative)
        assert_eq!(float_to_q15(-1.0), -32767);

        // -0.5 should convert to approximately -16383
        let q15 = float_to_q15(-0.5);
        assert_eq!(q15, -16383);
    }

    #[test]
    fn test_q15_clamping() {
        // Values outside [-1, 1] should clamp
        assert_eq!(float_to_q15(2.0), 32767);
        assert_eq!(float_to_q15(-2.0), -32767);
        assert_eq!(float_to_q15(f32::INFINITY), 32767);
        assert_eq!(float_to_q15(f32::NEG_INFINITY), -32767);
    }

    #[test]
    fn test_q15_roundtrip() {
        // Verify roundtrip conversion preserves values (within quantization error)
        let original = 0.75f32;
        let q15 = float_to_q15(original);
        let recovered = q15_to_float(q15);

        // Should be within ~1/32767 of original
        assert!((recovered - original).abs() < 0.0001);
    }
}

/// Test ring buffer layout struct (pure memory layout, no Metal).
///
/// Verifies that the MetalRingBufferLayout struct has correct size and alignment
/// for passing to Metal shaders.
mod ring_buffer_layout {
    /// Metal buffer layout matching common.metal::RingBuffer.
    ///
    /// Metal layout:
    /// - uint top_k (4 bytes)
    /// - uint current_pos (4 bytes)
    /// - ushort adapter_indices[8] (16 bytes)
    /// - short gates[8] (16 bytes)
    ///
    /// Total: 40 bytes
    #[repr(C)]
    struct MetalRingBufferLayout {
        top_k: u32,
        current_pos: u32,
        adapter_indices: [u16; 8],
        gates: [i16; 8],
    }

    #[test]
    fn test_layout_size() {
        // Assert size matches Metal expectation (40 bytes)
        assert_eq!(std::mem::size_of::<MetalRingBufferLayout>(), 40);
    }

    #[test]
    fn test_layout_alignment() {
        // Assert alignment is 4 bytes (matches Metal struct)
        assert_eq!(std::mem::align_of::<MetalRingBufferLayout>(), 4);
    }

    #[test]
    fn test_layout_field_offsets() {
        // Verify field offsets match expected Metal layout
        let layout = MetalRingBufferLayout {
            top_k: 0,
            current_pos: 0,
            adapter_indices: [0; 8],
            gates: [0; 8],
        };

        let base = &layout as *const _ as usize;
        let top_k_offset = &layout.top_k as *const _ as usize - base;
        let current_pos_offset = &layout.current_pos as *const _ as usize - base;
        let indices_offset = &layout.adapter_indices as *const _ as usize - base;
        let gates_offset = &layout.gates as *const _ as usize - base;

        assert_eq!(top_k_offset, 0);
        assert_eq!(current_pos_offset, 4);
        assert_eq!(indices_offset, 8);
        assert_eq!(gates_offset, 24);
    }

    #[test]
    fn test_layout_serialization() {
        // Test that we can safely create a byte slice from the struct
        let layout = MetalRingBufferLayout {
            top_k: 3,
            current_pos: 0,
            adapter_indices: [1, 2, 3, 0, 0, 0, 0, 0],
            gates: [16383, 32767, 8192, 0, 0, 0, 0, 0],
        };

        let size = std::mem::size_of::<MetalRingBufferLayout>();
        assert_eq!(size, 40);

        // SAFETY: MetalRingBufferLayout is repr(C) with known size,
        // and we're creating an immutable byte slice for the struct's lifetime.
        let bytes: &[u8] =
            unsafe { std::slice::from_raw_parts(&layout as *const _ as *const u8, size) };

        assert_eq!(bytes.len(), 40);

        // Verify first 4 bytes contain top_k (little endian on most systems)
        let top_k_bytes = &bytes[0..4];
        let top_k_value = u32::from_ne_bytes([
            top_k_bytes[0],
            top_k_bytes[1],
            top_k_bytes[2],
            top_k_bytes[3],
        ]);
        assert_eq!(top_k_value, 3);
    }
}

/// Test pointer arithmetic and bounds checking patterns.
///
/// These patterns are used in buffer operations throughout the kernel code.
mod pointer_safety {
    #[test]
    fn test_slice_bounds_validation() {
        let data = [1.0f32, 2.0, 3.0, 4.0, 5.0];

        // Simulate the bounds checking pattern used in safe_read_floats_from_buffer
        let buffer_size = data.len() * std::mem::size_of::<f32>();
        let requested_count = 3usize;
        let required_size = requested_count * std::mem::size_of::<f32>();

        assert!(required_size <= buffer_size);

        // SAFETY: We validated bounds above, this mirrors the kernel's pattern
        let slice: &[f32] = unsafe { std::slice::from_raw_parts(data.as_ptr(), requested_count) };

        assert_eq!(slice.len(), 3);
        assert_eq!(slice[0], 1.0);
        assert_eq!(slice[2], 3.0);
    }

    #[test]
    fn test_slice_with_offset() {
        let data = [1.0f32, 2.0, 3.0, 4.0, 5.0];

        // Simulate the bounds checking in safe_buffer_slice
        let start = 2usize;
        let len = 2usize;
        let buffer_size = data.len();
        let required_end = start + len;

        assert!(required_end <= buffer_size);

        // SAFETY: Bounds validated above
        let slice: &[f32] = unsafe { std::slice::from_raw_parts(data.as_ptr().add(start), len) };

        assert_eq!(slice.len(), 2);
        assert_eq!(slice[0], 3.0);
        assert_eq!(slice[1], 4.0);
    }

    #[test]
    fn test_byte_slice_creation() {
        let data = [1.0f32, 2.0, 3.0, 4.0];

        // Simulate safe_byte_buffer_slice pattern
        let buffer_bytes = data.len() * std::mem::size_of::<f32>();
        let max_bytes = 8usize; // Request 8 bytes
        let safe_len = max_bytes.min(buffer_bytes);

        // SAFETY: We take the minimum of requested and available
        let bytes: &[u8] =
            unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u8, safe_len) };

        assert_eq!(bytes.len(), 8);
    }

    #[test]
    fn test_copy_nonoverlapping() {
        let src = [1u32, 2, 3, 4];
        let mut dst = vec![0u32; 4];

        // SAFETY: src and dst don't overlap, both are valid for their lengths
        unsafe {
            std::ptr::copy_nonoverlapping(src.as_ptr(), dst.as_mut_ptr(), src.len());
        }

        assert_eq!(dst, vec![1, 2, 3, 4]);
    }
}

/// Test embedding configuration struct layout.
///
/// This struct is passed to Metal kernels and must have exact layout.
mod embedding_config {
    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    struct EmbeddingConfig {
        hidden_size: u32,
        vocab_size: u32,
        batch_size: u32,
        _padding: u32,
    }

    #[test]
    fn test_embedding_config_size() {
        // Should be 16 bytes (4 * 4 bytes)
        assert_eq!(std::mem::size_of::<EmbeddingConfig>(), 16);
    }

    #[test]
    fn test_embedding_config_alignment() {
        // Should align to 4 bytes
        assert_eq!(std::mem::align_of::<EmbeddingConfig>(), 4);
    }

    #[test]
    fn test_embedding_config_as_bytes() {
        let config = EmbeddingConfig {
            hidden_size: 4096,
            vocab_size: 32000,
            batch_size: 1,
            _padding: 0,
        };

        let size = std::mem::size_of::<EmbeddingConfig>();

        // SAFETY: EmbeddingConfig is repr(C) with known layout
        let bytes: &[u8] =
            unsafe { std::slice::from_raw_parts(&config as *const _ as *const u8, size) };

        assert_eq!(bytes.len(), 16);
    }
}

/// Test vocabulary projection configuration struct layout.
mod vocab_projection_config {
    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    struct VocabProjectionConfig {
        hidden_size: u32,
        vocab_size: u32,
        batch_size: u32,
        use_bias: u32,
    }

    #[test]
    fn test_vocab_projection_config_size() {
        assert_eq!(std::mem::size_of::<VocabProjectionConfig>(), 16);
    }

    #[test]
    fn test_vocab_projection_config_alignment() {
        assert_eq!(std::mem::align_of::<VocabProjectionConfig>(), 4);
    }

    #[test]
    fn test_vocab_projection_config_values() {
        let config = VocabProjectionConfig {
            hidden_size: 3584,
            vocab_size: 152064,
            batch_size: 1,
            use_bias: 0,
        };

        assert_eq!(config.hidden_size, 3584);
        assert_eq!(config.vocab_size, 152064);
        assert_eq!(config.use_bias, 0);
    }
}

/// Test active adapter struct layout and operations.
mod active_adapter {
    #[derive(Debug, Clone)]
    struct ActiveAdapter {
        id: u16,
        gate: i16,
    }

    #[test]
    fn test_active_adapter_size() {
        // u16 + i16 = 4 bytes
        assert_eq!(std::mem::size_of::<ActiveAdapter>(), 4);
    }

    #[test]
    fn test_active_adapter_creation() {
        let adapter = ActiveAdapter {
            id: 42,
            gate: 16383, // ~0.5 in Q15
        };

        assert_eq!(adapter.id, 42);
        assert_eq!(adapter.gate, 16383);
    }

    #[test]
    fn test_adapter_vec_operations() {
        let mut adapters = Vec::new();

        for i in 0..8 {
            adapters.push(ActiveAdapter {
                id: i,
                gate: (i as i16) * 4096,
            });
        }

        assert_eq!(adapters.len(), 8);
        assert_eq!(adapters[3].id, 3);
        assert_eq!(adapters[3].gate, 12288);
    }
}

/// Test noise tracking data structures.
mod noise_tracking {
    use std::collections::HashMap;

    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    struct EpsilonStats {
        layer_id: String,
        l2_error: f64,
        max_error: f64,
        mean_error: f64,
        element_count: usize,
    }

    impl EpsilonStats {
        fn exceeds_threshold(&self, threshold: f64) -> bool {
            self.l2_error > threshold || self.max_error > threshold
        }
    }

    #[test]
    fn test_epsilon_stats_threshold() {
        let stats = EpsilonStats {
            layer_id: "layer_0".to_string(),
            l2_error: 1e-7,
            max_error: 5e-7,
            mean_error: 2e-7,
            element_count: 1024,
        };

        assert!(!stats.exceeds_threshold(1e-6));
        assert!(stats.exceeds_threshold(1e-8));
    }

    #[test]
    fn test_layer_stats_map() {
        let mut layer_stats: HashMap<String, EpsilonStats> = HashMap::new();

        layer_stats.insert(
            "mlp.gate".to_string(),
            EpsilonStats {
                layer_id: "mlp.gate".to_string(),
                l2_error: 1e-7,
                max_error: 2e-7,
                mean_error: 1.5e-7,
                element_count: 4096,
            },
        );

        assert!(layer_stats.contains_key("mlp.gate"));
        assert_eq!(layer_stats.get("mlp.gate").unwrap().element_count, 4096);
    }
}

/// Test purgeable state enum operations.
mod purgeable_state {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum PurgeableState {
        KeepCurrent,
        NonVolatile,
        Volatile,
        Empty,
    }

    impl From<u64> for PurgeableState {
        fn from(value: u64) -> Self {
            match value {
                1 => PurgeableState::KeepCurrent,
                2 => PurgeableState::NonVolatile,
                3 => PurgeableState::Volatile,
                4 => PurgeableState::Empty,
                _ => PurgeableState::NonVolatile, // Safe default
            }
        }
    }

    #[test]
    fn test_purgeable_state_from_u64() {
        assert_eq!(PurgeableState::from(1), PurgeableState::KeepCurrent);
        assert_eq!(PurgeableState::from(2), PurgeableState::NonVolatile);
        assert_eq!(PurgeableState::from(3), PurgeableState::Volatile);
        assert_eq!(PurgeableState::from(4), PurgeableState::Empty);
    }

    #[test]
    fn test_purgeable_state_default() {
        // Unknown values should default to NonVolatile (safe)
        assert_eq!(PurgeableState::from(0), PurgeableState::NonVolatile);
        assert_eq!(PurgeableState::from(99), PurgeableState::NonVolatile);
    }

    #[derive(Debug, Clone, Copy)]
    struct PurgeableResult {
        previous: PurgeableState,
        was_purged: bool,
    }

    #[test]
    fn test_purgeable_result() {
        let result = PurgeableResult {
            previous: PurgeableState::Volatile,
            was_purged: false,
        };

        assert_eq!(result.previous, PurgeableState::Volatile);
        assert!(!result.was_purged);
    }
}

/// Test recovery wrapper state management.
mod recovery_state {
    use std::time::Instant;

    struct RecoveryWrapper {
        degraded: bool,
        panic_count: usize,
        recovery_count: usize,
        last_recovery_timestamp: Option<Instant>,
    }

    impl RecoveryWrapper {
        fn new() -> Self {
            Self {
                degraded: false,
                panic_count: 0,
                recovery_count: 0,
                last_recovery_timestamp: None,
            }
        }

        fn mark_degraded(&mut self) {
            self.degraded = true;
            self.panic_count += 1;
        }

        fn mark_recovered(&mut self) {
            self.degraded = false;
            self.recovery_count += 1;
            self.last_recovery_timestamp = Some(Instant::now());
        }
    }

    #[test]
    fn test_recovery_wrapper_initial_state() {
        let wrapper = RecoveryWrapper::new();
        assert!(!wrapper.degraded);
        assert_eq!(wrapper.panic_count, 0);
        assert_eq!(wrapper.recovery_count, 0);
        assert!(wrapper.last_recovery_timestamp.is_none());
    }

    #[test]
    fn test_recovery_wrapper_degradation() {
        let mut wrapper = RecoveryWrapper::new();
        wrapper.mark_degraded();

        assert!(wrapper.degraded);
        assert_eq!(wrapper.panic_count, 1);
    }

    #[test]
    fn test_recovery_wrapper_recovery() {
        let mut wrapper = RecoveryWrapper::new();
        wrapper.mark_degraded();
        wrapper.mark_recovered();

        assert!(!wrapper.degraded);
        assert_eq!(wrapper.panic_count, 1);
        assert_eq!(wrapper.recovery_count, 1);
        assert!(wrapper.last_recovery_timestamp.is_some());
    }

    #[test]
    fn test_multiple_recovery_cycles() {
        let mut wrapper = RecoveryWrapper::new();

        for _ in 0..3 {
            wrapper.mark_degraded();
            wrapper.mark_recovered();
        }

        assert!(!wrapper.degraded);
        assert_eq!(wrapper.panic_count, 3);
        assert_eq!(wrapper.recovery_count, 3);
    }
}
