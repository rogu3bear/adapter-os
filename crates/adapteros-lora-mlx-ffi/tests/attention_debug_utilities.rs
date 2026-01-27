//! Attention Debugging and Visualization Utilities

use adapteros_lora_mlx_ffi::AttentionConfig;

#[test]
fn test_attention_config_exported() {
    let config = AttentionConfig::new(256, 8, true).expect("config");
    assert_eq!(config.num_heads, 8);
    assert!(config.causal_mask);
    assert_eq!(config.head_dim, 32);
}
