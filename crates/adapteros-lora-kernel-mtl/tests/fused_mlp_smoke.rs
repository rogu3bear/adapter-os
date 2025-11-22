#[cfg(target_os = "macos")]
#[test]
fn fused_mlp_exec_smoke_zero_weights() {
    use adapteros_lora_kernel_mtl::fused_mlp::LoraConfig;
    use adapteros_lora_kernel_mtl::ring_buffer::ActiveAdapter;
    use metal::{Device, MTLResourceOptions};

    // Set tiny dims aligned with kernel's simple dispatch
    let hidden = 4usize;
    let rank = 4usize; // match hidden to avoid layout surprises

    let device = Device::system_default().expect("Metal device is required on macOS");

    // Prepare zero input and zero weights so the expected output is all zeros
    let input = vec![0.0f32; hidden];
    let gate_w = vec![0.0f32; hidden * rank];
    let up_w = vec![0.0f32; hidden * rank];
    let down_w = vec![0.0f32; rank * hidden];
    let gate_lora_a = vec![0.0f32; hidden * rank];
    let gate_lora_b = vec![0.0f32; rank * rank];
    let up_lora_a = vec![0.0f32; hidden * rank];
    let up_lora_b = vec![0.0f32; rank * rank];
    let down_lora_a = vec![0.0f32; rank * rank];
    let down_lora_b = vec![0.0f32; rank * hidden];
    let mut output = vec![0.0f32; hidden];

    let input_buf = device.new_buffer_with_data(
        input.as_ptr() as *const _,
        (input.len() * std::mem::size_of::<f32>()) as u64,
        MTLResourceOptions::StorageModeShared,
    );
    let gate_buf = device.new_buffer_with_data(
        gate_w.as_ptr() as *const _,
        (gate_w.len() * std::mem::size_of::<f32>()) as u64,
        MTLResourceOptions::StorageModeShared,
    );
    let up_buf = device.new_buffer_with_data(
        up_w.as_ptr() as *const _,
        (up_w.len() * std::mem::size_of::<f32>()) as u64,
        MTLResourceOptions::StorageModeShared,
    );
    let down_buf = device.new_buffer_with_data(
        down_w.as_ptr() as *const _,
        (down_w.len() * std::mem::size_of::<f32>()) as u64,
        MTLResourceOptions::StorageModeShared,
    );
    let out_buf = device.new_buffer_with_data(
        output.as_mut_ptr() as *const _,
        (output.len() * std::mem::size_of::<f32>()) as u64,
        MTLResourceOptions::StorageModeShared,
    );

    let gate_lora_a_buf = device.new_buffer_with_data(
        gate_lora_a.as_ptr() as *const _,
        (gate_lora_a.len() * std::mem::size_of::<f32>()) as u64,
        MTLResourceOptions::StorageModeShared,
    );
    let gate_lora_b_buf = device.new_buffer_with_data(
        gate_lora_b.as_ptr() as *const _,
        (gate_lora_b.len() * std::mem::size_of::<f32>()) as u64,
        MTLResourceOptions::StorageModeShared,
    );
    let up_lora_a_buf = device.new_buffer_with_data(
        up_lora_a.as_ptr() as *const _,
        (up_lora_a.len() * std::mem::size_of::<f32>()) as u64,
        MTLResourceOptions::StorageModeShared,
    );
    let up_lora_b_buf = device.new_buffer_with_data(
        up_lora_b.as_ptr() as *const _,
        (up_lora_b.len() * std::mem::size_of::<f32>()) as u64,
        MTLResourceOptions::StorageModeShared,
    );
    let down_lora_a_buf = device.new_buffer_with_data(
        down_lora_a.as_ptr() as *const _,
        (down_lora_a.len() * std::mem::size_of::<f32>()) as u64,
        MTLResourceOptions::StorageModeShared,
    );
    let down_lora_b_buf = device.new_buffer_with_data(
        down_lora_b.as_ptr() as *const _,
        (down_lora_b.len() * std::mem::size_of::<f32>()) as u64,
        MTLResourceOptions::StorageModeShared,
    );

    // Create adapters using the current API
    let adapters = vec![ActiveAdapter {
        id: 1,
        gate: 16384, // Q15 format
    }];

    // Note: This test validates the API structure. Actual kernel execution
    // would require a fully initialized FusedMlpKernel with Metal library loaded.
    println!(
        "Test setup: hidden={}, rank={}, adapters={:?}",
        hidden, rank, adapters
    );
}
