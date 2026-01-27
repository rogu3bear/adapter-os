//! Comprehensive KV Cache and Attention Verification Tests

use adapteros_lora_mlx_ffi::{CacheLayer, KVCacheConfig};

#[test]
fn test_kv_cache_exports() {
    let config = KVCacheConfig::default();
    assert_eq!(config.num_layers, 32);

    let mut layer = CacheLayer::new(2);
    layer.add_position(vec![1.0, 2.0], vec![3.0, 4.0]);
    assert_eq!(layer.cached_positions, 1);
    assert_eq!(layer.get_key_at(0).unwrap(), &[1.0, 2.0]);
    assert_eq!(layer.get_value_at(0).unwrap(), &[3.0, 4.0]);
}
