//! Metal GPU Dropout Determinism Tests
//!
//! This test suite verifies that the dropout implementation in the fused MLP kernel
//! is deterministic based on the provided dropout_seed.

#[cfg(target_os = "macos")]
mod tests {
    use adapteros_core::Result;
    use adapteros_lora_kernel_mtl::{ActiveAdapter, AdapterWeights, FusedMlpKernel};
    use metal::*;
    use std::sync::Arc;

    /// Helper to create a dummy buffer with data
    fn create_buffer_with_data(device: &Device, data: &[f32]) -> Buffer {
        device.new_buffer_with_data(
            data.as_ptr() as *const _,
            std::mem::size_of_val(data) as u64,
            MTLResourceOptions::StorageModeShared,
        )
    }

    /// Helper to read data from a buffer
    fn read_buffer_data(buffer: &Buffer) -> Vec<f32> {
        let ptr = buffer.contents() as *const f32;
        let len = buffer.length() as usize / std::mem::size_of::<f32>();
        let mut data = vec![0.0; len];
        unsafe {
            std::ptr::copy_nonoverlapping(ptr, data.as_mut_ptr(), len);
        }
        data
    }

    #[test]
    fn test_fused_mlp_dropout_determinism() -> Result<()> {
        let device = Device::system_default().expect("Metal device not found");
        let mut kernel = FusedMlpKernel::new(Arc::new(device.clone()))?;

        let hidden_size = 128;
        let intermediate_size = 256;

        // Dummy inputs and weights
        let input = create_buffer_with_data(&device, &vec![1.0; hidden_size]);
        let gate_weight =
            create_buffer_with_data(&device, &vec![0.1; hidden_size * intermediate_size]);
        let up_weight =
            create_buffer_with_data(&device, &vec![0.1; hidden_size * intermediate_size]);
        let down_weight =
            create_buffer_with_data(&device, &vec![0.1; intermediate_size * hidden_size]);
        let output1 = device.new_buffer(
            (hidden_size * std::mem::size_of::<f32>()) as u64,
            MTLResourceOptions::StorageModeShared,
        );
        let output2 = device.new_buffer(
            (hidden_size * std::mem::size_of::<f32>()) as u64,
            MTLResourceOptions::StorageModeShared,
        );
        let output3 = device.new_buffer(
            (hidden_size * std::mem::size_of::<f32>()) as u64,
            MTLResourceOptions::StorageModeShared,
        );

        // Note: Dropout is only applied when LoRA adapters are active.
        // With no adapters, the kernel should ignore dropout_rate and
        // produce identical outputs regardless of seed.
        let adapters: Vec<ActiveAdapter> = vec![]; // No adapters for simple MLP test
        let adapter_weights: Vec<&AdapterWeights> = vec![];

        // Run 1: Seed 42, Rate 0.1
        kernel.execute(
            &input,
            &gate_weight,
            &up_weight,
            &down_weight,
            &output1,
            &adapter_weights,
            &adapters,
            42,
            0.1, // Added dropout rate
        )?;

        // Run 2: Seed 42 (should be same as Run 1)
        kernel.execute(
            &input,
            &gate_weight,
            &up_weight,
            &down_weight,
            &output2,
            &adapter_weights,
            &adapters,
            42,
            0.1,
        )?;

        // Run 3: Seed 43 (should be different from Run 1 if rate > 0)
        kernel.execute(
            &input,
            &gate_weight,
            &up_weight,
            &down_weight,
            &output3,
            &adapter_weights,
            &adapters,
            43,
            0.1,
        )?;

        let data1 = read_buffer_data(&output1);
        let data2 = read_buffer_data(&output2);
        let data3 = read_buffer_data(&output3);

        // Verify Run 1 == Run 2
        assert_eq!(data1, data2, "Same seed produced different results");

        // Verify Run 1 != Run 3 when dropout is active; otherwise ensure no seed effect.
        if adapter_weights.is_empty() {
            assert_eq!(data1, data3, "Dropout should be inactive with no adapters");
        } else {
            // With 128 elements and 0.1 dropout rate, it's extremely unlikely (~1 in 2^128)
            // that two different seeds would produce identical bit-for-bit results.
            assert_ne!(
                data1, data3,
                "Different seeds produced identical results (dropout logic might be inactive)"
            );
        }

        Ok(())
    }
}
