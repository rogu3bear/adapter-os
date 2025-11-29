//! Adapter Hot-Swap Integration Tests
//!
//! These tests verify that MetalKernels can load and unload adapters at runtime.
//! This is critical for production hot-swap functionality.
//!
//! Created as part of Determinism Rectification v2 to verify the production code path
//! (MetalKernels) actually implements load_adapter/unload_adapter.

use adapteros_core::Result;
use adapteros_lora_kernel_api::FusedKernels;
use safetensors::tensor::TensorView;
use safetensors::{serialize, Dtype};
use std::collections::HashMap;

/// Create minimal SafeTensors bytes with LoRA A/B tensors for testing.
///
/// Creates a simple adapter with:
/// - q_proj lora_A: [rank, in_features] = [4, 8]
/// - q_proj lora_B: [out_features, rank] = [8, 4]
fn create_test_safetensors_weights() -> Vec<u8> {
    let rank = 4usize;
    let features = 8usize;

    // LoRA A: [rank, in_features] = [4, 8]
    let lora_a_data: Vec<f32> = (0..(rank * features)).map(|i| (i as f32) * 0.01).collect();
    let lora_a_bytes: Vec<u8> = lora_a_data.iter().flat_map(|f| f.to_le_bytes()).collect();

    // LoRA B: [out_features, rank] = [8, 4]
    let lora_b_data: Vec<f32> = (0..(features * rank)).map(|i| (i as f32) * 0.02).collect();
    let lora_b_bytes: Vec<u8> = lora_b_data.iter().flat_map(|f| f.to_le_bytes()).collect();

    // Build tensor metadata
    let mut tensors: Vec<(&str, Vec<u8>, Vec<usize>, Dtype)> = Vec::new();

    // q_proj LoRA A and B
    tensors.push((
        "base_model.model.layers.0.self_attn.q_proj.lora_A.weight",
        lora_a_bytes.clone(),
        vec![rank, features],
        Dtype::F32,
    ));
    tensors.push((
        "base_model.model.layers.0.self_attn.q_proj.lora_B.weight",
        lora_b_bytes.clone(),
        vec![features, rank],
        Dtype::F32,
    ));

    // Build the safetensors data
    let tensor_data: Vec<(&str, TensorView<'_>)> = tensors
        .iter()
        .map(|(name, data, shape, dtype)| {
            (*name, TensorView::new(*dtype, shape.clone(), data).unwrap())
        })
        .collect();

    // Create metadata (empty for test)
    let metadata: Option<HashMap<String, String>> = None;

    // Serialize to SafeTensors format
    serialize(tensor_data, &metadata).expect("Failed to serialize test SafeTensors")
}

#[cfg(target_os = "macos")]
mod metal_tests {
    use super::*;
    use adapteros_lora_kernel_mtl::MetalKernels;

    /// Test that MetalKernels can load an adapter from SafeTensors.
    ///
    /// This test verifies the production code path:
    /// Worker → backend_factory → MetalKernels → load_adapter()
    ///
    /// Before rectification v2, this would return "Hot-swap not supported"
    /// because MetalKernels used the default trait implementation.
    #[test]
    fn test_metal_adapter_load() -> Result<()> {
        let mut kernels = MetalKernels::new()?;

        // Create minimal SafeTensors payload
        let weights = create_test_safetensors_weights();

        // Should succeed (not return "Hot-swap not supported")
        kernels.load_adapter(1, &weights)?;

        Ok(())
    }

    /// Test that MetalKernels can unload an adapter.
    #[test]
    fn test_metal_adapter_unload() -> Result<()> {
        let mut kernels = MetalKernels::new()?;

        // Load first
        let weights = create_test_safetensors_weights();
        kernels.load_adapter(1, &weights)?;

        // Unload should succeed
        kernels.unload_adapter(1)?;

        Ok(())
    }

    /// Test that loading and unloading is idempotent.
    ///
    /// - Unloading a non-existent adapter should not error
    /// - Loading the same adapter twice should overwrite
    #[test]
    fn test_metal_adapter_idempotent() -> Result<()> {
        let mut kernels = MetalKernels::new()?;
        let weights = create_test_safetensors_weights();

        // Unload non-existent adapter (should be OK, idempotent)
        kernels.unload_adapter(999)?;

        // Load adapter
        kernels.load_adapter(1, &weights)?;

        // Load same adapter again (overwrite)
        kernels.load_adapter(1, &weights)?;

        // Unload
        kernels.unload_adapter(1)?;

        // Unload again (idempotent)
        kernels.unload_adapter(1)?;

        Ok(())
    }

    /// Test multiple adapters can be loaded simultaneously.
    #[test]
    fn test_metal_multiple_adapters() -> Result<()> {
        let mut kernels = MetalKernels::new()?;
        let weights = create_test_safetensors_weights();

        // Load multiple adapters
        kernels.load_adapter(1, &weights)?;
        kernels.load_adapter(2, &weights)?;
        kernels.load_adapter(3, &weights)?;

        // Unload in different order
        kernels.unload_adapter(2)?;
        kernels.unload_adapter(1)?;
        kernels.unload_adapter(3)?;

        Ok(())
    }
}

/// Non-macOS stub tests to ensure the test file compiles on all platforms.
#[cfg(not(target_os = "macos"))]
mod non_macos_tests {
    #[test]
    fn test_safetensors_creation() {
        let weights = super::create_test_safetensors_weights();
        assert!(!weights.is_empty(), "SafeTensors should be created");

        // Verify it can be parsed
        let tensors = safetensors::SafeTensors::deserialize(&weights)
            .expect("Should parse SafeTensors");

        let names: Vec<_> = tensors.names().into_iter().collect();
        assert!(
            names.iter().any(|n| n.contains("lora_A")),
            "Should contain lora_A tensor"
        );
        assert!(
            names.iter().any(|n| n.contains("lora_B")),
            "Should contain lora_B tensor"
        );
    }
}
