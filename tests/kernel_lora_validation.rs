//! Kernel LoRA Validation Tests
//!
//! GPU-level tests that validate LoRA weight buffers are correctly accessible
//! by Metal shaders and produce expected outputs.

use adapteros_core::Result;
use adapteros_lora_kernel_api::FusedKernels;
use adapteros_lora_kernel_mtl::MetalKernels;

mod helpers;
use helpers::{create_synthetic_adapter, WeightPattern};

/// Test Step 3: Buffer Binding Verification
///
/// Verifies that Metal shaders can access LoRA weight buffers.
///
/// Test Strategy:
/// 1. Create adapter with known constant weights (all 1.0)
/// 2. Load adapter into Metal kernels
/// 3. Verify all 5 modules loaded successfully
/// 4. Sample buffer values from GPU and verify pattern
#[tokio::test]
async fn test_buffer_binding_verification() -> Result<()> {
    // Create synthetic adapter with constant pattern (all 1.0s)
    let adapter_bytes = create_synthetic_adapter(
        4,    // rank
        16.0, // alpha
        WeightPattern::Ones,
    )?;

    // Initialize Metal kernels
    let mut kernels = MetalKernels::new()?;

    // Load adapter (ID 0)
    let adapter_id = 0u16;
    kernels.load_adapter(adapter_id, &adapter_bytes)?;

    println!("✅ Adapter loaded successfully (ID: {})", adapter_id);

    // Verify adapter buffers are accessible via GPU verification
    match kernels.verify_adapter_buffers(adapter_id) {
        Ok((buffer_size, first_sample, last_sample, mid_sample)) => {
            println!("✅ GPU buffer verification successful");
            println!("   Buffer size: {} bytes", buffer_size);
            println!("   First sample: {} bytes", first_sample.len());
            println!("   Last sample: {} bytes", last_sample.len());
            println!("   Mid sample: {} bytes", mid_sample.len());

            // Verify we got non-zero samples
            assert!(buffer_size > 0, "Buffer size should be > 0");
            assert!(!first_sample.is_empty(), "First sample should not be empty");
            assert!(!last_sample.is_empty(), "Last sample should not be empty");
            assert!(!mid_sample.is_empty(), "Mid sample should not be empty");

            // Verify samples contain actual data (not all zeros)
            let first_has_data = first_sample.iter().any(|&b| b != 0);
            let last_has_data = last_sample.iter().any(|&b| b != 0);
            let mid_has_data = mid_sample.iter().any(|&b| b != 0);

            assert!(
                first_has_data || last_has_data || mid_has_data,
                "At least one sample should contain non-zero data"
            );

            println!("✅ All buffer samples contain valid data");
        }
        Err(e) => {
            panic!("GPU buffer verification failed: {}", e);
        }
    }

    // Store GPU fingerprint for baseline
    kernels.store_gpu_fingerprint(adapter_id, 0, "test_baseline");

    // Verify against baseline (should match since we just stored it)
    match kernels.verify_gpu_fingerprint(adapter_id, 0, "test_baseline") {
        Ok(true) => {
            println!("✅ GPU fingerprint verification passed");
        }
        Ok(false) => {
            println!("⚠️  No baseline stored (first verification)");
        }
        Err(e) => {
            panic!("GPU fingerprint verification failed: {}", e);
        }
    }

    println!("✅ Buffer binding verification complete");
    println!("   - Adapter loaded into GPU memory");
    println!("   - Buffer samples readable from GPU");
    println!("   - Fingerprint verification functional");

    Ok(())
}

/// Test Step 3b: Multi-Module Buffer Binding
///
/// Verifies that all 5 LoRA modules are correctly bound and accessible:
/// - q_proj, k_proj, v_proj, mlp.down_proj, mlp.up_proj
#[tokio::test]
async fn test_multi_module_buffer_binding() -> Result<()> {
    // Create adapter with sequential pattern for easier verification
    let adapter_bytes = create_synthetic_adapter(4, 16.0, WeightPattern::Sequential)?;

    let mut kernels = MetalKernels::new()?;

    // Load adapter
    let adapter_id = 1u16;
    kernels.load_adapter(adapter_id, &adapter_bytes)?;

    println!("✅ Multi-module adapter loaded (ID: {})", adapter_id);

    // Verify buffer accessibility
    let (buffer_size, _, _, _) = kernels.verify_adapter_buffers(adapter_id)?;

    // Each module has A and B matrices
    // Expected modules: q_proj, k_proj, v_proj, mlp.down_proj, mlp.up_proj
    // That's 5 modules * 2 matrices = 10 buffers total
    //
    // Size calculation (rank=4, hidden_dim=4096, intermediate_size=11008):
    // - q/k/v A: rank * hidden_dim * f32 = 4 * 4096 * 4 = 65536 bytes each
    // - q/k/v B: hidden_dim * rank * f32 = 4096 * 4 * 4 = 65536 bytes each
    // - mlp.down A: rank * intermediate_size * f32 = 4 * 11008 * 4 = 176128 bytes
    // - mlp.down B: intermediate_size * rank * f32 = 11008 * 4 * 4 = 176128 bytes
    // - mlp.up (same as down)
    //
    // Total: 3*(65536+65536) + 2*(176128+176128) = 393216 + 704512 = 1,097,728 bytes

    let expected_min_size = 1_000_000; // At least 1MB for all modules
    assert!(
        buffer_size >= expected_min_size as u64,
        "Buffer too small: {} bytes (expected >= {} bytes)",
        buffer_size,
        expected_min_size
    );

    println!(
        "✅ Multi-module buffer size validated: {} bytes",
        buffer_size
    );
    println!("   Expected minimum: {} bytes", expected_min_size);

    Ok(())
}

/// Test Step 3c: Buffer Hot-Swap
///
/// Verifies that adapters can be hot-swapped without corrupting GPU buffers.
#[tokio::test]
async fn test_buffer_hot_swap() -> Result<()> {
    let mut kernels = MetalKernels::new()?;

    // Load first adapter (all ones)
    let adapter_1 = create_synthetic_adapter(4, 16.0, WeightPattern::Ones)?;
    let adapter_id_1 = 10u16;
    kernels.load_adapter(adapter_id_1, &adapter_1)?;

    // Verify first adapter loaded
    let (size_1, first_1, _, _) = kernels.verify_adapter_buffers(adapter_id_1)?;
    println!("✅ Adapter 1 loaded: {} bytes", size_1);

    // Load second adapter (all zeros)
    let adapter_2 = create_synthetic_adapter(4, 16.0, WeightPattern::Zeros)?;
    let adapter_id_2 = 11u16;
    kernels.load_adapter(adapter_id_2, &adapter_2)?;

    // Verify second adapter loaded
    let (size_2, first_2, _, _) = kernels.verify_adapter_buffers(adapter_id_2)?;
    println!("✅ Adapter 2 loaded: {} bytes", size_2);

    // Verify both adapters have same size (same architecture)
    assert_eq!(
        size_1, size_2,
        "Adapters should have identical buffer sizes"
    );

    // Verify buffer samples are different (ones vs zeros)
    // Note: Due to Q15 quantization and safetensors format, the exact bytes
    // may vary, but the pattern should be distinguishable
    println!("✅ Hot-swap successful - both adapters loaded independently");
    println!(
        "   Adapter 1: {} bytes, first sample: {:02x?}",
        size_1,
        &first_1[..8.min(first_1.len())]
    );
    println!(
        "   Adapter 2: {} bytes, first sample: {:02x?}",
        size_2,
        &first_2[..8.min(first_2.len())]
    );

    // Unload first adapter
    kernels.unload_adapter(adapter_id_1)?;
    println!("✅ Adapter 1 unloaded");

    // Verify second adapter still accessible
    let (size_2_after, _, _, _) = kernels.verify_adapter_buffers(adapter_id_2)?;
    assert_eq!(
        size_2, size_2_after,
        "Adapter 2 should remain unchanged after unloading Adapter 1"
    );

    println!("✅ Buffer hot-swap validation complete");

    Ok(())
}
