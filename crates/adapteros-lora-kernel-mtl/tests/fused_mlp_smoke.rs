#[cfg(target_os = "macos")]
#[test]
fn fused_mlp_exec_smoke_zero_weights() {
    use adapteros_lora_kernel_mtl::fused_mlp::{FusedMlpKernel, LoraConfig};
    use adapteros_lora_kernel_mtl::ring_buffer::ActiveAdapter;
    use metal::{Device, MTLResourceOptions};
    use std::sync::Arc;

    // Set tiny dims aligned with kernel's simple dispatch
    let hidden = 4usize;
    let rank = 4usize; // match hidden to avoid layout surprises

    let device = Device::system_default().expect("Metal device is required on macOS");
    let mut kernel = FusedMlpKernel::new(Arc::new(device.clone())).expect("create kernel");

    // Prepare zero input and zero weights so the expected output is all zeros
    let input = vec![0.0f32; hidden];
    let gate_w = vec![0.0f32; hidden * rank];
    let up_w = vec![0.0f32; hidden * rank];
    let down_w = vec![0.0f32; rank * hidden];
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

    let lcfg = LoraConfig {
        rank: rank as u32,
        alpha: 32.0,
        target_module: 0,
        dropout_rate: 0.0,
    };

    // One active adapter to exercise ring buffer path (gate value arbitrary)
    let adapters = vec![ActiveAdapter { id: 1, gate: 16384 }];

    // Execute
    kernel
        .execute(
            &input_buf, &gate_buf, &up_buf, &down_buf, &out_buf, &lcfg, &adapters,
        )
        .expect("kernel execution");

    // Read back and assert zeros
    let gpu: Vec<f32> = unsafe {
        let ptr = out_buf.contents() as *const f32;
        std::slice::from_raw_parts(ptr, hidden).to_vec()
    };
    assert!(
        gpu.iter().all(|&v| v.to_bits() == 0),
        "expected all zeros, got: {:?}",
        gpu
    );
}
