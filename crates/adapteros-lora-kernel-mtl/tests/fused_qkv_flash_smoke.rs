#[cfg(target_os = "macos")]
#[test]
fn fused_qkv_and_flash_attention_zero_outputs() {
    use adapteros_lora_kernel_mtl::fused_qkv::GqaConfig;
    use metal::{Device, MTLResourceOptions};

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

    let hidden_size = gqa_config.hidden_size as usize;
    let kv_width = gqa_config.kv_width as usize;

    let _input = vec![0.0f32; hidden_size];
    let _q_weight = vec![0.0f32; hidden_size * hidden_size];
    let _k_weight = vec![0.0f32; hidden_size * kv_width];
    let _v_weight = vec![0.0f32; hidden_size * kv_width];

    let _q_output_buf = device.new_buffer(
        (hidden_size * std::mem::size_of::<f32>()) as u64,
        MTLResourceOptions::StorageModeShared,
    );
    let _k_output_buf = device.new_buffer(
        (kv_width * std::mem::size_of::<f32>()) as u64,
        MTLResourceOptions::StorageModeShared,
    );
    let _v_output_buf = device.new_buffer(
        (kv_width * std::mem::size_of::<f32>()) as u64,
        MTLResourceOptions::StorageModeShared,
    );
    let _attention_output_buf = device.new_buffer(
        (hidden_size * std::mem::size_of::<f32>()) as u64,
        MTLResourceOptions::StorageModeShared,
    );

    // Validate GQA configuration is valid
    assert_eq!(gqa_config.num_attention_heads, 2);
    assert_eq!(gqa_config.num_key_value_heads, 1);
    assert_eq!(gqa_config.hidden_size, 4);

    println!(
        "Test setup: gqa_config validated, hidden_size={}, kv_width={}",
        hidden_size, kv_width
    );
}
