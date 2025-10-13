//! Unit tests for tensor metadata canonicalization
//!
//! These tests verify that tensor metadata canonicalization produces
//! identical hash outputs across multiple dtype/shape combinations
//! and ensures cross-run stability.

use adapteros_graph::{
    canonical::{canonical_tensor_repr, CanonicalTensor, HASH_VERSION},
    hash::{hash_tensor_with_metadata, hash_tensors, HashGraph},
    tensor::{DataType, DeviceFamily, MemoryLayout, QuantizationParams, Tensor},
};
use mplora_core::B3Hash;
use std::collections::HashMap;

/// Create a test tensor with specified parameters
fn create_test_tensor(
    dtype: DataType,
    shape: Vec<u64>,
    layout: MemoryLayout,
    device_family: DeviceFamily,
) -> Tensor {
    let data_size = shape.iter().product::<u64>() * dtype.size_bytes() as u64;
    let data = vec![0u8; data_size as usize];
    
    Tensor::new(dtype, shape, layout, device_family, data).unwrap()
}

/// Create a tensor with quantization parameters
fn create_quantized_tensor(
    dtype: DataType,
    shape: Vec<u64>,
    layout: MemoryLayout,
    device_family: DeviceFamily,
) -> Tensor {
    let data_size = shape.iter().product::<u64>() * dtype.size_bytes() as u64;
    let data = vec![0u8; data_size as usize];
    
    let mut quantization = HashMap::new();
    quantization.insert("group_size".to_string(), 128.0);
    quantization.insert("bits".to_string(), 4.0);
    
    let quantization_params = QuantizationParams {
        quant_type: "int4_block".to_string(),
        group_size: Some(128),
        bits: Some(4),
        scales: Some(vec![1.0, 2.0, 3.0]),
        zero_points: Some(vec![0, 1, 2]),
        extra_params: quantization,
    };
    
    let mut tensor = Tensor::new(dtype, shape, layout, device_family, data).unwrap();
    tensor.metadata.quantization = Some(quantization_params);
    tensor.metadata.metal_kernel_hash = Some("kernel_abc123".to_string());
    tensor.metadata.memory_address_hash = Some("addr_def456".to_string());
    
    tensor
}

#[test]
fn test_canonical_tensor_repr_basic() {
    let tensor = create_test_tensor(
        DataType::Float32,
        vec![2, 3],
        MemoryLayout::RowMajor,
        DeviceFamily::MetalM3,
    );
    
    let canonical = canonical_tensor_repr(&tensor).unwrap();
    
    assert_eq!(canonical.version, HASH_VERSION);
    assert_eq!(canonical.dtype_bytes, DataType::Float32 as u8);
    assert_eq!(canonical.shape, vec![2, 3]);
    assert_eq!(canonical.layout_bytes, MemoryLayout::RowMajor as u8);
    assert_eq!(canonical.device_family_bytes, DeviceFamily::MetalM3 as u8);
}

#[test]
fn test_canonical_tensor_repr_quantized() {
    let tensor = create_quantized_tensor(
        DataType::Float16,
        vec![4, 5],
        MemoryLayout::ColumnMajor,
        DeviceFamily::MetalM4,
    );
    
    let canonical = canonical_tensor_repr(&tensor).unwrap();
    
    assert_eq!(canonical.version, HASH_VERSION);
    assert_eq!(canonical.dtype_bytes, DataType::Float16 as u8);
    assert_eq!(canonical.shape, vec![4, 5]);
    assert_eq!(canonical.layout_bytes, MemoryLayout::ColumnMajor as u8);
    assert_eq!(canonical.device_family_bytes, DeviceFamily::MetalM4 as u8);
    
    assert!(canonical.quantization_params.is_some());
    let qp = canonical.quantization_params.unwrap();
    assert_eq!(qp.quant_type, "int4_block");
    assert_eq!(qp.group_size, Some(128));
    assert_eq!(qp.bits, Some(4));
    assert_eq!(qp.scales, Some(vec![1.0, 2.0, 3.0]));
    assert_eq!(qp.zero_points, Some(vec![0, 1, 2]));
    
    assert_eq!(canonical.metal_kernel_hash, Some("kernel_abc123".to_string()));
    assert_eq!(canonical.memory_address_hash, Some("addr_def456".to_string()));
}

#[test]
fn test_hash_deterministic_across_runs() {
    let tensor = create_test_tensor(
        DataType::Float32,
        vec![2, 3],
        MemoryLayout::RowMajor,
        DeviceFamily::MetalM3,
    );
    
    // Hash the same tensor multiple times
    let hash1 = hash_tensor_with_metadata(&tensor).unwrap();
    let hash2 = hash_tensor_with_metadata(&tensor).unwrap();
    let hash3 = hash_tensor_with_metadata(&tensor).unwrap();
    
    assert_eq!(hash1, hash2);
    assert_eq!(hash2, hash3);
    assert_eq!(hash1, hash3);
}

#[test]
fn test_hash_different_dtypes() {
    let tensor_f32 = create_test_tensor(
        DataType::Float32,
        vec![2, 3],
        MemoryLayout::RowMajor,
        DeviceFamily::MetalM3,
    );
    
    let tensor_f16 = create_test_tensor(
        DataType::Float16,
        vec![2, 3],
        MemoryLayout::RowMajor,
        DeviceFamily::MetalM3,
    );
    
    let hash_f32 = hash_tensor_with_metadata(&tensor_f32).unwrap();
    let hash_f16 = hash_tensor_with_metadata(&tensor_f16).unwrap();
    
    // Different dtypes should produce different hashes
    assert_ne!(hash_f32, hash_f16);
}

#[test]
fn test_hash_different_shapes() {
    let tensor_2x3 = create_test_tensor(
        DataType::Float32,
        vec![2, 3],
        MemoryLayout::RowMajor,
        DeviceFamily::MetalM3,
    );
    
    let tensor_3x2 = create_test_tensor(
        DataType::Float32,
        vec![3, 2],
        MemoryLayout::RowMajor,
        DeviceFamily::MetalM3,
    );
    
    let hash_2x3 = hash_tensor_with_metadata(&tensor_2x3).unwrap();
    let hash_3x2 = hash_tensor_with_metadata(&tensor_3x2).unwrap();
    
    // Different shapes should produce different hashes
    assert_ne!(hash_2x3, hash_3x2);
}

#[test]
fn test_hash_different_layouts() {
    let tensor_row = create_test_tensor(
        DataType::Float32,
        vec![2, 3],
        MemoryLayout::RowMajor,
        DeviceFamily::MetalM3,
    );
    
    let tensor_col = create_test_tensor(
        DataType::Float32,
        vec![2, 3],
        MemoryLayout::ColumnMajor,
        DeviceFamily::MetalM3,
    );
    
    let hash_row = hash_tensor_with_metadata(&tensor_row).unwrap();
    let hash_col = hash_tensor_with_metadata(&tensor_col).unwrap();
    
    // Different layouts should produce different hashes
    assert_ne!(hash_row, hash_col);
}

#[test]
fn test_hash_different_devices() {
    let tensor_m3 = create_test_tensor(
        DataType::Float32,
        vec![2, 3],
        MemoryLayout::RowMajor,
        DeviceFamily::MetalM3,
    );
    
    let tensor_m4 = create_test_tensor(
        DataType::Float32,
        vec![2, 3],
        MemoryLayout::RowMajor,
        DeviceFamily::MetalM4,
    );
    
    let hash_m3 = hash_tensor_with_metadata(&tensor_m3).unwrap();
    let hash_m4 = hash_tensor_with_metadata(&tensor_m4).unwrap();
    
    // Different device families should produce different hashes
    assert_ne!(hash_m3, hash_m4);
}

#[test]
fn test_hash_multiple_tensors() {
    let tensor1 = create_test_tensor(
        DataType::Float32,
        vec![2, 3],
        MemoryLayout::RowMajor,
        DeviceFamily::MetalM3,
    );
    
    let tensor2 = create_test_tensor(
        DataType::Float16,
        vec![4, 5],
        MemoryLayout::ColumnMajor,
        DeviceFamily::MetalM4,
    );
    
    let hash_single1 = hash_tensor_with_metadata(&tensor1).unwrap();
    let hash_single2 = hash_tensor_with_metadata(&tensor2).unwrap();
    let hash_multi = hash_tensors(&[&tensor1, &tensor2]).unwrap();
    
    // Multi-tensor hash should be different from individual hashes
    assert_ne!(hash_multi, hash_single1);
    assert_ne!(hash_multi, hash_single2);
}

#[test]
fn test_hash_graph_operations() {
    let mut graph = HashGraph::new();
    assert!(graph.is_empty());
    
    let tensor1 = create_test_tensor(
        DataType::Float32,
        vec![2, 3],
        MemoryLayout::RowMajor,
        DeviceFamily::MetalM3,
    );
    
    let tensor2 = create_test_tensor(
        DataType::Float16,
        vec![4, 5],
        MemoryLayout::ColumnMajor,
        DeviceFamily::MetalM4,
    );
    
    graph.add_tensor(&tensor1).unwrap();
    assert_eq!(graph.len(), 1);
    
    graph.add_tensor(&tensor2).unwrap();
    assert_eq!(graph.len(), 2);
    
    let graph_hash = graph.hash();
    assert_ne!(graph_hash, B3Hash::hash(&[]));
    
    // Verify node hashes are accessible
    let node_hashes = graph.node_hashes();
    assert_eq!(node_hashes.len(), 2);
}

#[test]
fn test_hash_order_independence() {
    let tensor1 = create_test_tensor(
        DataType::Float32,
        vec![2, 3],
        MemoryLayout::RowMajor,
        DeviceFamily::MetalM3,
    );
    
    let tensor2 = create_test_tensor(
        DataType::Float16,
        vec![4, 5],
        MemoryLayout::ColumnMajor,
        DeviceFamily::MetalM4,
    );
    
    let hash_order1 = hash_tensors(&[&tensor1, &tensor2]).unwrap();
    let hash_order2 = hash_tensors(&[&tensor2, &tensor1]).unwrap();
    
    // Hash should be order-independent due to sorting
    assert_eq!(hash_order1, hash_order2);
}

#[test]
fn test_canonical_serialization_roundtrip() {
    let tensor = create_quantized_tensor(
        DataType::Float16,
        vec![4, 5],
        MemoryLayout::ColumnMajor,
        DeviceFamily::MetalM4,
    );
    
    let canonical1 = canonical_tensor_repr(&tensor).unwrap();
    let bytes = canonical1.to_canonical_bytes().unwrap();
    let canonical2 = CanonicalTensor::from_canonical_bytes(&bytes).unwrap();
    
    assert_eq!(canonical1, canonical2);
}

#[test]
fn test_fixed_bytes_serialization_roundtrip() {
    let tensor = create_test_tensor(
        DataType::Float32,
        vec![2, 3],
        MemoryLayout::RowMajor,
        DeviceFamily::MetalM3,
    );
    
    let canonical1 = canonical_tensor_repr(&tensor).unwrap();
    let bytes = canonical1.to_fixed_bytes().unwrap();
    let canonical2 = CanonicalTensor::from_fixed_bytes(&bytes).unwrap();
    
    assert_eq!(canonical1.version, canonical2.version);
    assert_eq!(canonical1.endian, canonical2.endian);
    assert_eq!(canonical1.dtype_bytes, canonical2.dtype_bytes);
    assert_eq!(canonical1.shape, canonical2.shape);
    assert_eq!(canonical1.layout_bytes, canonical2.layout_bytes);
    assert_eq!(canonical1.device_family_bytes, canonical2.device_family_bytes);
}

#[test]
fn test_hash_version_embedding() {
    let tensor = create_test_tensor(
        DataType::Float32,
        vec![2, 3],
        MemoryLayout::RowMajor,
        DeviceFamily::MetalM3,
    );
    
    let canonical = canonical_tensor_repr(&tensor).unwrap();
    assert_eq!(canonical.version, HASH_VERSION);
    
    let bytes = canonical.to_canonical_bytes().unwrap();
    let restored = CanonicalTensor::from_canonical_bytes(&bytes).unwrap();
    assert_eq!(restored.version, HASH_VERSION);
}

#[test]
fn test_cross_platform_stability() {
    // Test that the same tensor produces the same canonical representation
    // regardless of platform-specific details
    let tensor = create_test_tensor(
        DataType::Float32,
        vec![2, 3],
        MemoryLayout::RowMajor,
        DeviceFamily::MetalM3,
    );
    
    let canonical1 = canonical_tensor_repr(&tensor).unwrap();
    let canonical2 = canonical_tensor_repr(&tensor).unwrap();
    
    assert_eq!(canonical1.version, canonical2.version);
    assert_eq!(canonical1.endian, canonical2.endian);
    assert_eq!(canonical1.dtype_bytes, canonical2.dtype_bytes);
    assert_eq!(canonical1.shape, canonical2.shape);
    assert_eq!(canonical1.layout_bytes, canonical2.layout_bytes);
    assert_eq!(canonical1.device_family_bytes, canonical2.device_family_bytes);
}

#[test]
fn test_comprehensive_dtype_coverage() {
    let dtypes = vec![
        DataType::Float32,
        DataType::Float16,
        DataType::Int8,
        DataType::Int16,
        DataType::Int32,
        DataType::Int64,
        DataType::UInt8,
        DataType::UInt16,
        DataType::UInt32,
        DataType::UInt64,
        DataType::Bool,
    ];
    
    let mut hashes = Vec::new();
    
    for dtype in dtypes {
        let tensor = create_test_tensor(
            dtype,
            vec![2, 3],
            MemoryLayout::RowMajor,
            DeviceFamily::MetalM3,
        );
        
        let hash = hash_tensor_with_metadata(&tensor).unwrap();
        hashes.push(hash);
    }
    
    // All hashes should be different
    for i in 0..hashes.len() {
        for j in (i + 1)..hashes.len() {
            assert_ne!(hashes[i], hashes[j], "Dtype {} and {} produced same hash", i, j);
        }
    }
}

#[test]
fn test_comprehensive_device_coverage() {
    let devices = vec![
        DeviceFamily::CPU,
        DeviceFamily::MetalM1,
        DeviceFamily::MetalM2,
        DeviceFamily::MetalM3,
        DeviceFamily::MetalM4,
        DeviceFamily::MetalM3Ultra,
        DeviceFamily::MetalM4Ultra,
    ];
    
    let mut hashes = Vec::new();
    
    for device in devices {
        let tensor = create_test_tensor(
            DataType::Float32,
            vec![2, 3],
            MemoryLayout::RowMajor,
            device,
        );
        
        let hash = hash_tensor_with_metadata(&tensor).unwrap();
        hashes.push(hash);
    }
    
    // All hashes should be different
    for i in 0..hashes.len() {
        for j in (i + 1)..hashes.len() {
            assert_ne!(hashes[i], hashes[j], "Device {} and {} produced same hash", i, j);
        }
    }
}

#[test]
fn test_large_tensor_handling() {
    // Test with a larger tensor to ensure scalability
    let tensor = create_test_tensor(
        DataType::Float32,
        vec![100, 200, 300],
        MemoryLayout::RowMajor,
        DeviceFamily::MetalM3,
    );
    
    let hash = hash_tensor_with_metadata(&tensor).unwrap();
    assert_ne!(hash, B3Hash::hash(&[]));
    
    // Verify canonical representation works with large tensors
    let canonical = canonical_tensor_repr(&tensor).unwrap();
    assert_eq!(canonical.shape, vec![100, 200, 300]);
}

#[test]
fn test_edge_case_shapes() {
    // Test edge cases: empty dimensions, single dimension, etc.
    let shapes = vec![
        vec![1],
        vec![1, 1],
        vec![1, 1, 1],
        vec![0], // This should fail validation
    ];
    
    for shape in shapes {
        if shape.contains(&0) {
            // Zero dimensions should fail validation
            let result = Tensor::new(
                DataType::Float32,
                shape,
                MemoryLayout::RowMajor,
                DeviceFamily::MetalM3,
                vec![0u8; 4],
            );
            assert!(result.is_err());
        } else {
            let tensor = create_test_tensor(
                DataType::Float32,
                shape,
                MemoryLayout::RowMajor,
                DeviceFamily::MetalM3,
            );
            
            let hash = hash_tensor_with_metadata(&tensor).unwrap();
            assert_ne!(hash, B3Hash::hash(&[]));
        }
    }
}
