#[cfg(target_os = "macos")]
#[test]
fn fused_qkv_and_flash_attention_zero_outputs() {
    use adapteros_lora_kernel_mtl::fused_qkv::{
        FlashAttentionKernel, FusedQkvKernel, GqaConfig, LoraConfig,
    };
    use adapteros_lora_kernel_mtl::ring_buffer::RawRingBuffer;
    use metal::{Device, MTLResourceOptions};
    use std::sync::Arc;

    let device = Device::system_default().expect("Metal device is required on macOS");

    let gqa_config = GqaConfig {
        num_attention_heads: 2,
        num_key_value_heads: 1,
        head_dim: 2,
        kv_width: 2,
        hidden_size: 4,
        rope_theta: 10_000.0,
        attention_scale: 0.0,
        dropout_rate: 0.0,
    };

    let qkv_kernel =
        FusedQkvKernel::new(Arc::new(device.clone()), gqa_config).expect("create fused QKV kernel");
    let flash_kernel = FlashAttentionKernel::new(Arc::new(device.clone()), gqa_config)
        .expect("create flash attention kernel");

    let hidden_size = gqa_config.hidden_size as usize;
    let kv_width = gqa_config.kv_width as usize;

    let input = vec![0.0f32; hidden_size];
    let q_weight = vec![0.0f32; hidden_size * hidden_size];
    let k_weight = vec![0.0f32; hidden_size * kv_width];
    let v_weight = vec![0.0f32; hidden_size * kv_width];
    let input_buf = device.new_buffer_with_data(
        input.as_ptr() as *const _,
        (input.len() * std::mem::size_of::<f32>()) as u64,
        MTLResourceOptions::StorageModeShared,
    );
    let q_weight_buf = device.new_buffer_with_data(
        q_weight.as_ptr() as *const _,
        (q_weight.len() * std::mem::size_of::<f32>()) as u64,
        MTLResourceOptions::StorageModeShared,
    );
    let k_weight_buf = device.new_buffer_with_data(
        k_weight.as_ptr() as *const _,
        (k_weight.len() * std::mem::size_of::<f32>()) as u64,
        MTLResourceOptions::StorageModeShared,
    );
    let v_weight_buf = device.new_buffer_with_data(
        v_weight.as_ptr() as *const _,
        (v_weight.len() * std::mem::size_of::<f32>()) as u64,
        MTLResourceOptions::StorageModeShared,
    );

    let zero_buf_a = device.new_buffer(0, MTLResourceOptions::StorageModeShared);
    let zero_buf_b = device.new_buffer(0, MTLResourceOptions::StorageModeShared);
    let zero_buf_c = device.new_buffer(0, MTLResourceOptions::StorageModeShared);
    let zero_buf_d = device.new_buffer(0, MTLResourceOptions::StorageModeShared);
    let zero_buf_e = device.new_buffer(0, MTLResourceOptions::StorageModeShared);
    let zero_buf_f = device.new_buffer(0, MTLResourceOptions::StorageModeShared);

    let q_output_buf = device.new_buffer(
        (hidden_size * std::mem::size_of::<f32>()) as u64,
        MTLResourceOptions::StorageModeShared,
    );
    let k_output_buf = device.new_buffer(
        (kv_width * std::mem::size_of::<f32>()) as u64,
        MTLResourceOptions::StorageModeShared,
    );
    let v_output_buf = device.new_buffer(
        (kv_width * std::mem::size_of::<f32>()) as u64,
        MTLResourceOptions::StorageModeShared,
    );
    let attention_output_buf = device.new_buffer(
        (hidden_size * std::mem::size_of::<f32>()) as u64,
        MTLResourceOptions::StorageModeShared,
    );

    let lora_config = LoraConfig {
        rank: 0,
        ..LoraConfig::default()
    };
    let ring_state = RawRingBuffer::default();

    qkv_kernel
        .execute(
            &input_buf,
            &q_weight_buf,
            &k_weight_buf,
            &v_weight_buf,
            &q_output_buf,
            &k_output_buf,
            &v_output_buf,
            &zero_buf_a,
            &zero_buf_b,
            &zero_buf_c,
            &zero_buf_d,
            &zero_buf_e,
            &zero_buf_f,
            &lora_config,
            ring_state,
            1,
            1,
        )
        .expect("fused QKV execution");

    flash_kernel
        .execute(
            &q_output_buf,
            &k_output_buf,
            &v_output_buf,
            &attention_output_buf,
        )
        .expect("flash attention execution");

    // All outputs should remain zero with zeroed inputs
    unsafe {
        let q_slice =
            std::slice::from_raw_parts(q_output_buf.contents() as *const f32, hidden_size);
        let attn_slice =
            std::slice::from_raw_parts(attention_output_buf.contents() as *const f32, hidden_size);
        assert!(q_slice.iter().all(|&v| v.to_bits() == 0));
        assert!(attn_slice.iter().all(|&v| v.to_bits() == 0));
    }
}
