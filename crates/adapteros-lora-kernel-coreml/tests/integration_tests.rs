//! Integration tests for CoreML tensor operation pipeline

use adapteros_lora_kernel_coreml::TensorBridgeType;

#[test]
fn test_tensor_bridge_type_exported() {
    let swift = TensorBridgeType::Swift;
    let objc = TensorBridgeType::ObjCpp;

    assert_ne!(swift, objc);
}
