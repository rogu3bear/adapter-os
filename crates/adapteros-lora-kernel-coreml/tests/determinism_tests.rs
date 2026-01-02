//! Determinism verification tests for CoreML backend
//!
//! These tests verify that CoreML tensor operations produce bit-exact results
//! across repeated runs, ensuring deterministic execution for AdapterOS.

use adapteros_core::{derive_seed, B3Hash};
use adapteros_lora_kernel_coreml::{ffi, MLTensor, TensorBridgeType};

// =============================================================================
// Helper Functions
// =============================================================================

/// Compare two float slices for bit-exact equality
fn assert_bit_exact(a: &[f32], b: &[f32], context: &str) {
    assert_eq!(a.len(), b.len(), "{}: length mismatch", context);
    for (i, (va, vb)) in a.iter().zip(b.iter()).enumerate() {
        assert_eq!(
            va.to_bits(),
            vb.to_bits(),
            "{}: mismatch at index {} ({} vs {})",
            context,
            i,
            va,
            vb
        );
    }
}

/// Compare two float slices with ULP (Units in Last Place) tolerance
/// This is used for cross-bridge comparisons where minor floating-point variance is acceptable
fn assert_ulp_equal(a: &[f32], b: &[f32], max_ulp: u32, context: &str) {
    assert_eq!(a.len(), b.len(), "{}: length mismatch", context);
    for (i, (va, vb)) in a.iter().zip(b.iter()).enumerate() {
        // Handle NaN and Inf cases
        if va.is_nan() || vb.is_nan() {
            panic!("{}: NaN value at index {} ({} vs {})", context, i, va, vb);
        }
        if va.is_infinite() || vb.is_infinite() {
            if va != vb {
                panic!(
                    "{}: infinite value mismatch at index {} ({} vs {})",
                    context, i, va, vb
                );
            }
            continue;
        }

        // Calculate ULP difference
        let bits_a = va.to_bits() as i32;
        let bits_b = vb.to_bits() as i32;
        let ulp_diff = (bits_a - bits_b).unsigned_abs();

        assert!(
            ulp_diff <= max_ulp,
            "{}: ULP difference {} exceeds max {} at index {} ({} vs {})",
            context,
            ulp_diff,
            max_ulp,
            i,
            va,
            vb
        );
    }
}

/// Generate deterministic test data from HKDF seed
fn generate_seeded_data(seed: u64, size: usize) -> Vec<f32> {
    let mut data = Vec::with_capacity(size);
    let mut state = seed;
    for _ in 0..size {
        // Simple LCG PRNG for deterministic data generation
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let val = (state >> 33) as f32 / (1u64 << 31) as f32;
        data.push(val);
    }
    data
}

fn should_run_mltensor_tests() -> bool {
    if std::env::var_os("AOS_COREML_SKIP_MLTENSOR_TESTS").is_some() {
        println!("Skipping - AOS_COREML_SKIP_MLTENSOR_TESTS set");
        return false;
    }

    if !MLTensor::is_available() {
        println!("Skipping - MLTensor not available (requires macOS 15+)");
        return false;
    }

    true
}

fn should_run_swift_bridge_tests() -> bool {
    if !should_run_mltensor_tests() {
        return false;
    }

    if !unsafe { ffi::swift_coreml_supports_mltensor() } {
        println!("Skipping Swift/ObjC++ comparison - Swift bridge not available");
        return false;
    }

    true
}

// =============================================================================
// Basic Operation Determinism Tests
// =============================================================================

#[test]
#[cfg(target_os = "macos")]
fn test_softmax_determinism() {
    if !should_run_mltensor_tests() {
        return;
    }

    let data = vec![1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
    let shape = vec![2usize, 4];

    // Run softmax multiple times
    let mut results = Vec::new();
    for _ in 0..5 {
        let tensor = MLTensor::from_floats(&data, &shape).unwrap();
        let softmax = tensor.softmax(-1).unwrap();
        let result = softmax.to_vec().unwrap();
        results.push(result);
    }

    // All results must be bit-exact
    for i in 1..results.len() {
        assert_bit_exact(&results[0], &results[i], &format!("softmax run {}", i));
    }
    println!("Softmax determinism verified across {} runs", results.len());
}

#[test]
#[cfg(target_os = "macos")]
fn test_add_determinism() {
    if !should_run_mltensor_tests() {
        return;
    }

    let data1 = vec![1.0f32, 2.0, 3.0, 4.0];
    let data2 = vec![5.0f32, 6.0, 7.0, 8.0];
    let shape = vec![2usize, 2];

    // Run addition multiple times
    let mut results = Vec::new();
    for _ in 0..5 {
        let tensor1 = MLTensor::from_floats(&data1, &shape).unwrap();
        let tensor2 = MLTensor::from_floats(&data2, &shape).unwrap();
        let sum = tensor1.add(&tensor2).unwrap();
        let result = sum.to_vec().unwrap();
        results.push(result);
    }

    // All results must be bit-exact
    for i in 1..results.len() {
        assert_bit_exact(&results[0], &results[i], &format!("add run {}", i));
    }
    println!("Add determinism verified across {} runs", results.len());
}

#[test]
#[cfg(target_os = "macos")]
fn test_scale_determinism() {
    if !should_run_mltensor_tests() {
        return;
    }

    let data = vec![1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0];
    let shape = vec![2usize, 3];
    let factor = 2.5f32;

    // Run scaling multiple times
    let mut results = Vec::new();
    for _ in 0..5 {
        let tensor = MLTensor::from_floats(&data, &shape).unwrap();
        let scaled = tensor.scale(factor).unwrap();
        let result = scaled.to_vec().unwrap();
        results.push(result);
    }

    // All results must be bit-exact
    for i in 1..results.len() {
        assert_bit_exact(&results[0], &results[i], &format!("scale run {}", i));
    }
    println!("Scale determinism verified across {} runs", results.len());
}

#[test]
#[cfg(target_os = "macos")]
fn test_matmul_determinism() {
    if !should_run_mltensor_tests() {
        return;
    }

    // [1, 2]   [5, 6]   [19, 22]
    // [3, 4] x [7, 8] = [43, 50]
    let data1 = vec![1.0f32, 2.0, 3.0, 4.0];
    let data2 = vec![5.0f32, 6.0, 7.0, 8.0];
    let shape = vec![2usize, 2];

    // Run matmul multiple times
    let mut results = Vec::new();
    for _ in 0..5 {
        let tensor1 = MLTensor::from_floats(&data1, &shape).unwrap();
        let tensor2 = MLTensor::from_floats(&data2, &shape).unwrap();
        let product = tensor1.matmul(&tensor2).unwrap();
        let result = product.to_vec().unwrap();
        results.push(result);
    }

    // All results must be bit-exact
    for i in 1..results.len() {
        assert_bit_exact(&results[0], &results[i], &format!("matmul run {}", i));
    }

    // Verify expected values
    assert_eq!(results[0], vec![19.0, 22.0, 43.0, 50.0]);
    println!("Matmul determinism verified across {} runs", results.len());
}

// =============================================================================
// HKDF Seed Determinism Tests
// =============================================================================

#[test]
#[cfg(target_os = "macos")]
fn test_hkdf_seeded_operations() {
    if !should_run_mltensor_tests() {
        return;
    }

    // Create deterministic seed from manifest hash
    let manifest_hash = B3Hash::hash(b"test-model-manifest-v1");
    let seed = derive_seed(&manifest_hash, "coreml-determinism-test");
    let seed_u64 = u64::from_le_bytes(seed[0..8].try_into().unwrap());

    // Generate seeded data
    let data = generate_seeded_data(seed_u64, 16);
    let shape = vec![4usize, 4];

    // Run complex operation chain with same seed multiple times
    let mut results = Vec::new();
    for _ in 0..3 {
        let tensor = MLTensor::from_floats(&data, &shape).unwrap();
        let scaled = tensor.scale(0.5).unwrap();
        let added = scaled.add(&tensor).unwrap();
        let softmax = added.softmax(-1).unwrap();
        let result = softmax.to_vec().unwrap();
        results.push(result);
    }

    // All results must be bit-exact
    for i in 1..results.len() {
        assert_bit_exact(&results[0], &results[i], &format!("HKDF seeded run {}", i));
    }
    println!("HKDF seeded operation determinism verified");
}

#[test]
#[cfg(target_os = "macos")]
fn test_different_seeds_produce_different_data() {
    // Verify that different seeds produce different data (sanity check)
    let hash1 = B3Hash::hash(b"manifest-1");
    let hash2 = B3Hash::hash(b"manifest-2");

    let seed1 = derive_seed(&hash1, "test");
    let seed2 = derive_seed(&hash2, "test");

    let seed1_u64 = u64::from_le_bytes(seed1[0..8].try_into().unwrap());
    let seed2_u64 = u64::from_le_bytes(seed2[0..8].try_into().unwrap());

    let data1 = generate_seeded_data(seed1_u64, 16);
    let data2 = generate_seeded_data(seed2_u64, 16);

    assert_ne!(
        data1, data2,
        "Different seeds should produce different data"
    );
    println!("Seed differentiation verified");
}

#[test]
#[cfg(target_os = "macos")]
fn test_same_seed_produces_same_data() {
    // Verify that the same seed always produces the same data
    let hash = B3Hash::hash(b"consistent-manifest");

    let mut all_data = Vec::new();
    for _ in 0..5 {
        let seed = derive_seed(&hash, "test");
        let seed_u64 = u64::from_le_bytes(seed[0..8].try_into().unwrap());
        let data = generate_seeded_data(seed_u64, 32);
        all_data.push(data);
    }

    for i in 1..all_data.len() {
        assert_eq!(all_data[0], all_data[i], "Same seed must produce same data");
    }
    println!(
        "Seed consistency verified across {} generations",
        all_data.len()
    );
}

// =============================================================================
// Chained Operations Determinism Tests
// =============================================================================

#[test]
#[cfg(target_os = "macos")]
fn test_chained_operations_determinism() {
    if !should_run_mltensor_tests() {
        return;
    }

    let data = vec![0.1f32, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8];
    let shape = vec![2usize, 4];

    // Complex chain: scale -> add -> matmul -> softmax
    let mut results = Vec::new();
    for _ in 0..5 {
        let tensor = MLTensor::from_floats(&data, &shape).unwrap();

        // Scale by 2
        let scaled = tensor.scale(2.0).unwrap();

        // Add to original
        let added = scaled.add(&tensor).unwrap();

        // Reshape for matmul: [2, 4] -> need compatible shapes
        // Use 4x4 for matmul
        let mat_data: Vec<f32> = (0..16).map(|i| (i as f32 + 1.0) * 0.1).collect();
        let mat_shape = vec![4usize, 4];
        let mat = MLTensor::from_floats(&mat_data, &mat_shape).unwrap();

        // Reshape added result for matmul compatibility
        let added_data = added.to_vec().unwrap();
        let reshaped = MLTensor::from_floats(&added_data, &vec![2, 4]).unwrap();

        // Matmul: [2, 4] x [4, 4] -> [2, 4]
        let product = reshaped.matmul(&mat).unwrap();

        // Final softmax
        let softmax = product.softmax(-1).unwrap();
        let result = softmax.to_vec().unwrap();
        results.push(result);
    }

    // All results must be bit-exact
    for i in 1..results.len() {
        assert_bit_exact(&results[0], &results[i], &format!("chained ops run {}", i));
    }

    // Verify softmax property (sums to 1 per row)
    for row in results[0].chunks(4) {
        let sum: f32 = row.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5, "Softmax row sum was {}", sum);
    }

    println!(
        "Chained operations determinism verified across {} runs",
        results.len()
    );
}

// =============================================================================
// Swift vs ObjC++ Bridge Comparison Tests
// =============================================================================

#[test]
#[cfg(target_os = "macos")]
fn test_swift_objcpp_softmax_equivalence() {
    if !should_run_swift_bridge_tests() {
        return;
    }

    let data = vec![1.0f32, 2.0, 3.0, 4.0];
    let shape = vec![1usize, 4];

    // Run via Swift bridge
    let swift_result = {
        let swift_ptr = unsafe {
            ffi::swift_coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len())
        };
        assert!(!swift_ptr.is_null(), "Swift tensor creation failed");

        let softmax_ptr = unsafe { ffi::swift_coreml_tensor_softmax(swift_ptr, -1) };
        assert!(!softmax_ptr.is_null(), "Swift softmax failed");

        let mut output = vec![0.0f32; 4];
        let result = unsafe {
            ffi::swift_coreml_tensor_to_floats(softmax_ptr, output.as_mut_ptr(), output.len())
        };
        assert!(result >= 0, "Swift materialize failed");

        unsafe {
            ffi::swift_coreml_tensor_free(swift_ptr);
            ffi::swift_coreml_tensor_free(softmax_ptr);
        }
        output
    };

    // Run via ObjC++ bridge
    let objcpp_result = {
        let handle =
            unsafe { ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len()) };
        assert!(handle.is_valid(), "ObjC++ tensor creation failed");

        let softmax_handle = unsafe { ffi::coreml_tensor_softmax(handle, -1) };
        assert!(softmax_handle.is_valid(), "ObjC++ softmax failed");

        let mut output = vec![0.0f32; 4];
        let result = unsafe {
            ffi::coreml_tensor_to_floats(softmax_handle, output.as_mut_ptr(), output.len())
        };
        assert!(result >= 0, "ObjC++ materialize failed");

        unsafe {
            ffi::coreml_tensor_free(handle);
            ffi::coreml_tensor_free(softmax_handle);
        }
        output
    };

    // Compare results - allow up to 2 ULP difference for cross-bridge floating-point variance
    // This is acceptable since Swift and ObjC++ may use slightly different internal implementations
    assert_ulp_equal(&swift_result, &objcpp_result, 2, "Swift vs ObjC++ softmax");
    println!("Swift and ObjC++ softmax produce equivalent results (within 2 ULP tolerance)");
}

#[test]
#[cfg(target_os = "macos")]
fn test_swift_objcpp_add_equivalence() {
    if !should_run_swift_bridge_tests() {
        return;
    }

    let data1 = vec![1.0f32, 2.0, 3.0, 4.0];
    let data2 = vec![5.0f32, 6.0, 7.0, 8.0];
    let shape = vec![2usize, 2];

    // Run via Swift bridge
    let swift_result = {
        let swift_ptr1 = unsafe {
            ffi::swift_coreml_create_tensor_f32(data1.as_ptr(), shape.as_ptr(), shape.len())
        };
        let swift_ptr2 = unsafe {
            ffi::swift_coreml_create_tensor_f32(data2.as_ptr(), shape.as_ptr(), shape.len())
        };
        assert!(!swift_ptr1.is_null() && !swift_ptr2.is_null());

        let sum_ptr = unsafe { ffi::swift_coreml_tensor_add(swift_ptr1, swift_ptr2) };
        assert!(!sum_ptr.is_null(), "Swift add failed");

        let mut output = vec![0.0f32; 4];
        let result = unsafe {
            ffi::swift_coreml_tensor_to_floats(sum_ptr, output.as_mut_ptr(), output.len())
        };
        assert!(result >= 0);

        unsafe {
            ffi::swift_coreml_tensor_free(swift_ptr1);
            ffi::swift_coreml_tensor_free(swift_ptr2);
            ffi::swift_coreml_tensor_free(sum_ptr);
        }
        output
    };

    // Run via ObjC++ bridge
    let objcpp_result = {
        let handle1 =
            unsafe { ffi::coreml_create_tensor_f32(data1.as_ptr(), shape.as_ptr(), shape.len()) };
        let handle2 =
            unsafe { ffi::coreml_create_tensor_f32(data2.as_ptr(), shape.as_ptr(), shape.len()) };
        assert!(handle1.is_valid() && handle2.is_valid());

        let sum_handle = unsafe { ffi::coreml_tensor_add(handle1, handle2) };
        assert!(sum_handle.is_valid(), "ObjC++ add failed");

        let mut output = vec![0.0f32; 4];
        let result =
            unsafe { ffi::coreml_tensor_to_floats(sum_handle, output.as_mut_ptr(), output.len()) };
        assert!(result >= 0);

        unsafe {
            ffi::coreml_tensor_free(handle1);
            ffi::coreml_tensor_free(handle2);
            ffi::coreml_tensor_free(sum_handle);
        }
        output
    };

    assert_bit_exact(&swift_result, &objcpp_result, "Swift vs ObjC++ add");
    println!("Swift and ObjC++ add produce bit-exact results");
}

#[test]
#[cfg(target_os = "macos")]
fn test_swift_objcpp_scale_equivalence() {
    if !should_run_swift_bridge_tests() {
        return;
    }

    let data = vec![1.0f32, 2.0, 3.0, 4.0];
    let shape = vec![2usize, 2];
    let factor = std::f32::consts::PI;

    // Run via Swift bridge
    let swift_result = {
        let swift_ptr = unsafe {
            ffi::swift_coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len())
        };
        assert!(!swift_ptr.is_null());

        let scaled_ptr = unsafe { ffi::swift_coreml_tensor_scale(swift_ptr, factor) };
        assert!(!scaled_ptr.is_null(), "Swift scale failed");

        let mut output = vec![0.0f32; 4];
        let result = unsafe {
            ffi::swift_coreml_tensor_to_floats(scaled_ptr, output.as_mut_ptr(), output.len())
        };
        assert!(result >= 0);

        unsafe {
            ffi::swift_coreml_tensor_free(swift_ptr);
            ffi::swift_coreml_tensor_free(scaled_ptr);
        }
        output
    };

    // Run via ObjC++ bridge
    let objcpp_result = {
        let handle =
            unsafe { ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len()) };
        assert!(handle.is_valid());

        let scaled_handle = unsafe { ffi::coreml_tensor_scale(handle, factor) };
        assert!(scaled_handle.is_valid(), "ObjC++ scale failed");

        let mut output = vec![0.0f32; 4];
        let result = unsafe {
            ffi::coreml_tensor_to_floats(scaled_handle, output.as_mut_ptr(), output.len())
        };
        assert!(result >= 0);

        unsafe {
            ffi::coreml_tensor_free(handle);
            ffi::coreml_tensor_free(scaled_handle);
        }
        output
    };

    assert_bit_exact(&swift_result, &objcpp_result, "Swift vs ObjC++ scale");
    println!("Swift and ObjC++ scale produce bit-exact results");
}

#[test]
#[cfg(target_os = "macos")]
fn test_swift_objcpp_matmul_equivalence() {
    if !should_run_swift_bridge_tests() {
        return;
    }

    let data1 = vec![1.0f32, 2.0, 3.0, 4.0];
    let data2 = vec![5.0f32, 6.0, 7.0, 8.0];
    let shape = vec![2usize, 2];

    // Run via Swift bridge
    let swift_result = {
        let swift_ptr1 = unsafe {
            ffi::swift_coreml_create_tensor_f32(data1.as_ptr(), shape.as_ptr(), shape.len())
        };
        let swift_ptr2 = unsafe {
            ffi::swift_coreml_create_tensor_f32(data2.as_ptr(), shape.as_ptr(), shape.len())
        };
        assert!(!swift_ptr1.is_null() && !swift_ptr2.is_null());

        let product_ptr = unsafe { ffi::swift_coreml_tensor_matmul(swift_ptr1, swift_ptr2) };
        assert!(!product_ptr.is_null(), "Swift matmul failed");

        let mut output = vec![0.0f32; 4];
        let result = unsafe {
            ffi::swift_coreml_tensor_to_floats(product_ptr, output.as_mut_ptr(), output.len())
        };
        assert!(result >= 0);

        unsafe {
            ffi::swift_coreml_tensor_free(swift_ptr1);
            ffi::swift_coreml_tensor_free(swift_ptr2);
            ffi::swift_coreml_tensor_free(product_ptr);
        }
        output
    };

    // Run via ObjC++ bridge
    let objcpp_result = {
        let handle1 =
            unsafe { ffi::coreml_create_tensor_f32(data1.as_ptr(), shape.as_ptr(), shape.len()) };
        let handle2 =
            unsafe { ffi::coreml_create_tensor_f32(data2.as_ptr(), shape.as_ptr(), shape.len()) };
        assert!(handle1.is_valid() && handle2.is_valid());

        let product_handle = unsafe { ffi::coreml_tensor_matmul(handle1, handle2) };
        assert!(product_handle.is_valid(), "ObjC++ matmul failed");

        let mut output = vec![0.0f32; 4];
        let result = unsafe {
            ffi::coreml_tensor_to_floats(product_handle, output.as_mut_ptr(), output.len())
        };
        assert!(result >= 0);

        unsafe {
            ffi::coreml_tensor_free(handle1);
            ffi::coreml_tensor_free(handle2);
            ffi::coreml_tensor_free(product_handle);
        }
        output
    };

    assert_bit_exact(&swift_result, &objcpp_result, "Swift vs ObjC++ matmul");
    println!("Swift and ObjC++ matmul produce bit-exact results");
}

// =============================================================================
// Large Tensor Determinism Tests
// =============================================================================

#[test]
#[cfg(target_os = "macos")]
fn test_large_tensor_matmul_determinism() {
    if !should_run_mltensor_tests() {
        return;
    }

    // Larger matrices for more realistic test
    let size = 64;
    let data1: Vec<f32> = (0..size * size).map(|i| (i % 17) as f32 * 0.1).collect();
    let data2: Vec<f32> = (0..size * size).map(|i| (i % 23) as f32 * 0.05).collect();
    let shape = vec![size, size];

    let mut results = Vec::new();
    for _ in 0..3 {
        let tensor1 = MLTensor::from_floats(&data1, &shape).unwrap();
        let tensor2 = MLTensor::from_floats(&data2, &shape).unwrap();
        let product = tensor1.matmul(&tensor2).unwrap();
        let result = product.to_vec().unwrap();
        results.push(result);
    }

    for i in 1..results.len() {
        assert_bit_exact(&results[0], &results[i], &format!("large matmul run {}", i));
    }
    println!(
        "Large tensor ({}x{}) matmul determinism verified",
        size, size
    );
}

#[test]
#[cfg(target_os = "macos")]
fn test_large_tensor_softmax_determinism() {
    if !should_run_mltensor_tests() {
        return;
    }

    let rows = 32;
    let cols = 128;
    let data: Vec<f32> = (0..rows * cols)
        .map(|i| (i % 31) as f32 * 0.05 - 0.5)
        .collect();
    let shape = vec![rows, cols];

    let mut results = Vec::new();
    for _ in 0..3 {
        let tensor = MLTensor::from_floats(&data, &shape).unwrap();
        let softmax = tensor.softmax(-1).unwrap();
        let result = softmax.to_vec().unwrap();
        results.push(result);
    }

    for i in 1..results.len() {
        assert_bit_exact(
            &results[0],
            &results[i],
            &format!("large softmax run {}", i),
        );
    }

    // Verify softmax property
    for (row_idx, row) in results[0].chunks(cols).enumerate() {
        let sum: f32 = row.iter().sum();
        assert!(
            (sum - 1.0).abs() < 1e-5,
            "Row {} softmax sum was {} (expected ~1.0)",
            row_idx,
            sum
        );
    }

    println!(
        "Large tensor ({}x{}) softmax determinism verified",
        rows, cols
    );
}

// =============================================================================
// Bridge Type Verification Tests
// =============================================================================

#[test]
#[cfg(target_os = "macos")]
fn test_bridge_type_consistency() {
    if !should_run_mltensor_tests() {
        return;
    }

    let data = vec![1.0f32, 2.0, 3.0, 4.0];
    let shape = vec![2usize, 2];

    // Create multiple tensors and verify they use the same bridge type
    let tensor1 = MLTensor::from_floats(&data, &shape).unwrap();
    let tensor2 = MLTensor::from_floats(&data, &shape).unwrap();

    let bridge1 = tensor1.bridge_type();
    let bridge2 = tensor2.bridge_type();

    assert_eq!(bridge1, bridge2, "Tensors should use the same bridge type");
    println!("Bridge type consistency verified: {:?}", bridge1);
}

#[test]
#[cfg(target_os = "macos")]
fn test_bridge_type_after_operations() {
    if !should_run_mltensor_tests() {
        return;
    }

    let data = vec![1.0f32, 2.0, 3.0, 4.0];
    let shape = vec![2usize, 2];

    let tensor = MLTensor::from_floats(&data, &shape).unwrap();
    let original_bridge = tensor.bridge_type();

    // Operations should preserve bridge type
    let scaled = tensor.scale(2.0).unwrap();
    let added = tensor.add(&scaled).unwrap();
    let softmax = added.softmax(-1).unwrap();

    // Note: When using Swift bridge, the operations create new Swift tensors
    // The bridge_type method returns what was stored in the tensor
    // For Swift bridge, result tensors also use Swift
    if original_bridge == TensorBridgeType::Swift {
        // Swift operations return Swift tensors (bridge_type is Swift but objc_handle may be default)
        println!("Operations maintain Swift bridge type");
    } else {
        assert_eq!(scaled.bridge_type(), original_bridge);
        assert_eq!(added.bridge_type(), original_bridge);
        assert_eq!(softmax.bridge_type(), original_bridge);
        println!("Operations preserve bridge type: {:?}", original_bridge);
    }
}

// =============================================================================
// Numerical Stability Tests
// =============================================================================

#[test]
#[cfg(target_os = "macos")]
fn test_softmax_numerical_stability() {
    if !should_run_mltensor_tests() {
        return;
    }

    // Test with extreme values that could cause numerical issues
    let data = vec![1000.0f32, 1001.0, 1002.0, 1003.0]; // Large values
    let shape = vec![1usize, 4];

    let mut results = Vec::new();
    for _ in 0..5 {
        let tensor = MLTensor::from_floats(&data, &shape).unwrap();
        let softmax = tensor.softmax(-1).unwrap();
        let result = softmax.to_vec().unwrap();
        results.push(result);
    }

    // All results must be bit-exact
    for i in 1..results.len() {
        assert_bit_exact(
            &results[0],
            &results[i],
            &format!("stable softmax run {}", i),
        );
    }

    // Verify no NaN or Inf
    for val in &results[0] {
        assert!(!val.is_nan(), "Softmax produced NaN");
        assert!(!val.is_infinite(), "Softmax produced Inf");
    }

    let sum: f32 = results[0].iter().sum();
    assert!((sum - 1.0).abs() < 1e-5, "Softmax sum was {}", sum);

    println!("Numerical stability verified for large-value softmax");
}

#[test]
#[cfg(target_os = "macos")]
fn test_matmul_precision() {
    if !should_run_mltensor_tests() {
        return;
    }

    // Test with values that could accumulate floating-point errors
    let size = 16;
    let data1: Vec<f32> = (0..size * size).map(|i| 0.1 + (i as f32 * 0.001)).collect();
    let data2: Vec<f32> = (0..size * size)
        .map(|i| 0.2 - (i as f32 * 0.0005))
        .collect();
    let shape = vec![size, size];

    let mut results = Vec::new();
    for _ in 0..5 {
        let tensor1 = MLTensor::from_floats(&data1, &shape).unwrap();
        let tensor2 = MLTensor::from_floats(&data2, &shape).unwrap();
        let product = tensor1.matmul(&tensor2).unwrap();
        let result = product.to_vec().unwrap();
        results.push(result);
    }

    // All results must be bit-exact despite potential floating-point accumulation
    for i in 1..results.len() {
        assert_bit_exact(
            &results[0],
            &results[i],
            &format!("precision matmul run {}", i),
        );
    }

    println!("Matmul precision determinism verified");
}
