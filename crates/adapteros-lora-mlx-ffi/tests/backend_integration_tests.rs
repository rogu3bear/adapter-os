//! Backend integration tests for MLX FFI

use adapteros_lora_mlx_ffi::mock::{create_mock_config, MockMLXFFIModel};

#[test]
fn test_mock_backend_available() {
    let config = create_mock_config();
    let model = MockMLXFFIModel::new(config);

    assert_eq!(model.config().vocab_size, 32000);
}
