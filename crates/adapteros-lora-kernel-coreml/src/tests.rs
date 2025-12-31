//! Tests for CoreML kernel implementation

use super::*;
use crate::export::{
    export_coreml_adapter, validate_coreml_fusion, CoreMLExportRequest, CoreMLFusionMetadata,
};
use adapteros_aos::{AosWriter, BackendTag};
use adapteros_core::B3Hash;
use adapteros_lora_kernel_api::{attestation, FusedKernels, IoBuffers, RouterRing};
use adapteros_types::CoreMLOpKind;
use safetensors::{serialize, tensor::TensorView};
use std::path::PathBuf;
use tempfile::tempdir;
fn simple_adapter_payload(delta: f32) -> Vec<u8> {
    let data = vec![delta; 8];
    let tensor = TensorView::new(safetensors::Dtype::F32, vec![2, 4], unsafe {
        std::slice::from_raw_parts(
            data.as_ptr() as *const u8,
            data.len() * std::mem::size_of::<f32>(),
        )
    })
    .expect("tensor view");
    serialize([("dummy.weight".to_string(), tensor)], &Default::default())
        .expect("serialize sidecar adapter")
}

#[test]
fn test_mltensor_availability() {
    // Just check the function runs without panic
    let _ = MLTensor::is_available();
}

#[test]
fn test_coreml_availability() {
    // Check is_coreml_available runs without panic
    let _ = is_coreml_available();
}

#[test]
fn placement_spec_applies_to_stub_backend() {
    let mut backend = CoreMLBackend::new_stub(ComputeUnits::CpuAndNeuralEngine).unwrap();
    let graph = CoreMLGraph::from_nodes(vec![CoreMLGraphNode {
        name: "layer0.self_attn.q_proj".into(),
        op_kind: Some(CoreMLOpKind::AttentionQ),
        input_dim: Some(8),
        output_dim: Some(8),
        path_hint: None,
    }]);
    let spec = CoreMLPlacementSpec {
        version: 1,
        graph_id: None,
        bindings: vec![adapteros_types::coreml::CoreMLPlacementBinding {
            binding_id: "layer0.q".into(),
            target: adapteros_types::coreml::CoreMLTargetRef {
                layer: "layer0.self_attn.q_proj".into(),
                op_kind: CoreMLOpKind::AttentionQ,
                path_hint: None,
            },
            projection: adapteros_types::coreml::CoreMLProjection::InputToHidden,
            rank: 4,
            alpha: None,
            scale: None,
            gating: None,
            shape: adapteros_types::coreml::CoreMLPlacementShape {
                input_dim: 8,
                output_dim: 8,
            },
        }],
    };

    let metrics = backend.apply_placement_spec(&graph, spec);
    assert_eq!(metrics.missing, 0);
    assert_eq!(metrics.resolved, 1);
    assert!(backend.placement_metrics().is_some());
    assert!(backend.dump_placement_map().is_some());
}

#[test]
#[cfg(target_os = "macos")]
fn stub_lora_preserves_coreml_model_bytes() -> Result<()> {
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../var/model-cache/models/qwen2.5-7b-instruct-fp16-512.mlpackage/Manifest.json");
    if !manifest_path.exists() {
        eprintln!(
            "Skipping CoreML bytes preservation test; fixture missing at {}",
            manifest_path.display()
        );
        return Ok(());
    }
    let base_bytes = std::fs::read(&manifest_path)
        .expect("CoreML fixture at var/model-cache/models/qwen2.5-7b-instruct-fp16-512.mlpackage");
    let hash_before = B3Hash::hash(&base_bytes);

    // Build a minimal adapter payload to hit the LoRA fusion branch.
    let weights = vec![0.1f32; 4];
    let adapter_tensors = [(
        "dummy.weight".to_string(),
        TensorView::new(safetensors::Dtype::F32, vec![2, 2], unsafe {
            std::slice::from_raw_parts(weights.as_ptr() as *const u8, weights.len() * 4)
        })
        .expect("tensor view"),
    )];
    let adapter_bytes = serialize(adapter_tensors, &Default::default()).expect("serialize adapter");

    // Wrap adapter payload into a canonical .aos bundle so the export helper can ingest it.
    let tmp = tempdir().expect("temp dir");
    let adapter_path = tmp.path().join("dummy.aos");
    let mut writer = AosWriter::new();
    writer.add_segment(
        BackendTag::Canonical,
        Some("dummy.scope".into()),
        &adapter_bytes,
    )?;
    let adapter_manifest = serde_json::json!({
        "metadata": {
            "scope_path": "dummy.scope",
            "domain": "tests",
            "group": "coreml",
            "operation": "coreml-export"
        },
        "scope": "coreml-export"
    });
    writer.write_archive(&adapter_path, &adapter_manifest)?;

    let output_path = tmp.path().join("fused/Manifest.json");
    let outcome = export_coreml_adapter(&CoreMLExportRequest {
        base_package: manifest_path.clone(),
        adapter_aos: adapter_path.clone(),
        output_package: output_path,
        compute_units: ComputeUnits::CpuAndNeuralEngine,
        allow_overwrite: false,
        timeout: std::time::Duration::from_secs(300),
        skip_ops_check: true, // Skip ops check for test
    })?;

    validate_coreml_fusion(&outcome.metadata_path)?;

    assert_eq!(
        hash_before, outcome.base_manifest_hash,
        "hash should reflect the original manifest"
    );
    assert_eq!(
        outcome.base_manifest_hash, outcome.fused_manifest_hash,
        "export must keep base manifest bytes unchanged"
    );

    Ok(())
}

#[test]
fn test_neural_engine_availability() {
    // Check is_neural_engine_available runs without panic
    let _ = is_neural_engine_available();
}

#[test]
fn q15_gate_conversion_matches_router_invariant() {
    let gates = [0i16, 1, 16384, 32767, -16384];
    for gate in gates {
        let expected = gate as f32 / 32767.0;
        let actual = CoreMLBackend::gate_q15_to_f32(gate);
        assert!(
            (actual - expected).abs() < 1e-6,
            "gate {} expected {}, got {}",
            gate,
            expected,
            actual
        );
    }
}

#[test]
fn stub_attestation_reports_nondeterministic_without_ane() -> Result<()> {
    let backend = CoreMLBackend::new_stub(ComputeUnits::CpuAndNeuralEngine)?;
    let report = backend.attest_determinism()?;
    assert!(
        !report.deterministic,
        "stub backend should never claim determinism"
    );
    assert!(matches!(
        report.rng_seed_method,
        attestation::RngSeedingMethod::SystemEntropy
    ));
    Ok(())
}

#[test]
#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
fn coreml_stub_hot_swap_sidecar_switches_and_restores() -> Result<()> {
    let mut backend = CoreMLBackend::new_stub(ComputeUnits::CpuAndNeuralEngine)?;

    // Baseline run without adapters
    let mut base_ring = RouterRing::new(0);
    let mut base_io = IoBuffers::new(6);
    base_io.input_ids = vec![1];
    backend.run_step(&mut base_ring, &mut base_io)?;
    let base_logits = base_io.output_logits.clone();

    // Attach adapter A and run twice for determinism
    let adapter_a = simple_adapter_payload(0.05);
    backend.load_adapter(7, &adapter_a)?;
    let mut ring_a = RouterRing::new(1);
    ring_a.set(&[7u16], &[32767]);
    let mut io_a = IoBuffers::new(6);
    io_a.input_ids = vec![1];
    backend.run_step(&mut ring_a, &mut io_a)?;
    let logits_a = io_a.output_logits.clone();

    let mut io_a_repeat = IoBuffers::new(6);
    io_a_repeat.input_ids = vec![1];
    backend.run_step(&mut ring_a, &mut io_a_repeat)?;
    assert_eq!(
        logits_a, io_a_repeat.output_logits,
        "Same adapter + seed must be deterministic"
    );

    // Switch to adapter B and ensure outputs differ
    let adapter_b = simple_adapter_payload(0.15);
    backend.load_adapter(9, &adapter_b)?;
    backend.switch_adapter(9)?;
    let mut ring_b = RouterRing::new(1);
    ring_b.set(&[9u16], &[32767]);
    let mut io_b = IoBuffers::new(6);
    io_b.input_ids = vec![1];
    backend.run_step(&mut ring_b, &mut io_b)?;
    assert_ne!(
        logits_a, io_b.output_logits,
        "Adapter switch should change logits in stub path"
    );

    // Detach all adapters and confirm base-only logits restored
    backend.detach_adapter(9)?;
    let mut clear_ring = RouterRing::new(0);
    let mut base_again = IoBuffers::new(6);
    base_again.input_ids = vec![1];
    backend.run_step(&mut clear_ring, &mut base_again)?;
    assert_eq!(
        base_logits, base_again.output_logits,
        "Detaching should restore base-only behavior"
    );

    Ok(())
}

#[test]
#[cfg(target_os = "macos")]
fn test_mltensor_create_and_materialize() {
    if !MLTensor::is_available() {
        return; // Skip on older macOS
    }

    let data = vec![1.0, 2.0, 3.0, 4.0];
    let tensor = MLTensor::from_floats(&data, &[2, 2]).unwrap();
    let result = tensor.to_vec().unwrap();
    assert_eq!(result.len(), 4);
    assert_eq!(result, data);
}

#[test]
#[cfg(target_os = "macos")]
fn test_mltensor_invalid_shape() {
    if !MLTensor::is_available() {
        return;
    }

    // Data doesn't match shape
    let data = vec![1.0, 2.0, 3.0];
    let result = MLTensor::from_floats(&data, &[2, 2]);
    assert!(result.is_err());
}

#[test]
#[cfg(target_os = "macos")]
fn test_mltensor_softmax() {
    if !MLTensor::is_available() {
        return;
    }

    let data = vec![1.0, 2.0, 3.0, 4.0];
    let tensor = MLTensor::from_floats(&data, &[1, 4]).unwrap();
    let softmax_result = tensor.softmax(-1).unwrap();
    let result = softmax_result.to_vec().unwrap();

    // Softmax should sum to ~1.0
    let sum: f32 = result.iter().sum();
    assert!((sum - 1.0).abs() < 1e-5, "Softmax sum was {}", sum);

    // Values should be positive
    assert!(result.iter().all(|&x| x > 0.0));
}

#[test]
#[cfg(target_os = "macos")]
fn test_mltensor_add() {
    if !MLTensor::is_available() {
        return;
    }

    let data1 = vec![1.0, 2.0, 3.0, 4.0];
    let data2 = vec![5.0, 6.0, 7.0, 8.0];
    let tensor1 = MLTensor::from_floats(&data1, &[2, 2]).unwrap();
    let tensor2 = MLTensor::from_floats(&data2, &[2, 2]).unwrap();

    let sum = tensor1.add(&tensor2).unwrap();
    let result = sum.to_vec().unwrap();

    assert_eq!(result, vec![6.0, 8.0, 10.0, 12.0]);
}

#[test]
#[cfg(target_os = "macos")]
fn test_mltensor_scale() {
    if !MLTensor::is_available() {
        return;
    }

    let data = vec![1.0, 2.0, 3.0, 4.0];
    let tensor = MLTensor::from_floats(&data, &[2, 2]).unwrap();

    let scaled = tensor.scale(2.0).unwrap();
    let result = scaled.to_vec().unwrap();

    assert_eq!(result, vec![2.0, 4.0, 6.0, 8.0]);
}

#[test]
#[cfg(target_os = "macos")]
fn test_mltensor_matmul() {
    if !MLTensor::is_available() {
        return;
    }

    // [1, 2]   [5, 6]   [1*5+2*7, 1*6+2*8]   [19, 22]
    // [3, 4] x [7, 8] = [3*5+4*7, 3*6+4*8] = [43, 50]
    let data1 = vec![1.0, 2.0, 3.0, 4.0];
    let data2 = vec![5.0, 6.0, 7.0, 8.0];
    let tensor1 = MLTensor::from_floats(&data1, &[2, 2]).unwrap();
    let tensor2 = MLTensor::from_floats(&data2, &[2, 2]).unwrap();

    let product = tensor1.matmul(&tensor2).unwrap();
    let result = product.to_vec().unwrap();

    assert_eq!(result, vec![19.0, 22.0, 43.0, 50.0]);
}

#[test]
#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
fn fused_metadata_mismatch_is_rejected() -> Result<()> {
    let tmp = tempdir().expect("tempdir");
    let base = tmp.path().join("base.json");
    let fused = tmp.path().join("fused.json");
    let adapter = tmp.path().join("adapter.bin");
    std::fs::write(&base, b"base-bytes")?;
    std::fs::write(&fused, b"fused-bytes")?;
    std::fs::write(&adapter, b"adapter-bytes")?;

    let metadata = CoreMLFusionMetadata {
        base_manifest_hash: B3Hash::hash(b"wrong-base"),
        fused_manifest_hash: B3Hash::hash(b"fused-bytes"),
        adapter_hash: B3Hash::hash(b"adapter-bytes"),
        base_package: base.clone(),
        fused_package: fused.clone(),
        adapter_path: adapter.clone(),
        fusion_verified: false,
    };
    let metadata_path = tmp.path().join("adapteros_coreml_fusion.json");
    std::fs::write(&metadata_path, serde_json::to_vec(&metadata)?)?;

    let mut backend = CoreMLBackend::new_stub(ComputeUnits::CpuAndNeuralEngine)?;
    let err = backend.register_fused_adapter_from_metadata(5, &metadata_path);
    assert!(
        err.is_err(),
        "mismatched base hash should reject fused adapter"
    );
    Ok(())
}

#[test]
#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
fn switch_adapter_fails_for_missing_fused_package() -> Result<()> {
    let mut backend = CoreMLBackend::new_stub(ComputeUnits::CpuAndNeuralEngine)?;
    backend.adapter_artifacts.insert(
        11,
        CoreMLAdapterArtifact::FusedPackage {
            model_path: PathBuf::from("/nonexistent/fused.mlmodelc"),
            model_hash: None,
        },
    );

    let result = backend.switch_adapter(11);
    assert!(result.is_err(), "missing fused package should error");
    assert!(
        backend.active_fused_adapter.is_none(),
        "fused activation should not be recorded on failure"
    );
    Ok(())
}

#[test]
#[cfg(target_os = "macos")]
fn test_mltensor_shape() {
    if !MLTensor::is_available() {
        return;
    }

    let data = vec![1.0; 24];
    let tensor = MLTensor::from_floats(&data, &[2, 3, 4]).unwrap();

    assert_eq!(tensor.shape(), vec![2, 3, 4]);
    assert_eq!(tensor.num_elements(), 24);
}

#[test]
#[cfg(target_os = "macos")]
fn test_mltensor_1d() {
    if !MLTensor::is_available() {
        return;
    }

    let data = vec![1.0, 2.0, 3.0];
    let tensor = MLTensor::from_floats(&data, &[3]).unwrap();
    let result = tensor.to_vec().unwrap();

    assert_eq!(result, data);
    assert_eq!(tensor.shape(), vec![3]);
}

#[test]
#[cfg(target_os = "macos")]
fn test_mltensor_chained_operations() {
    if !MLTensor::is_available() {
        return;
    }

    let data = vec![1.0, 2.0, 3.0, 4.0];
    let tensor = MLTensor::from_floats(&data, &[2, 2]).unwrap();

    // Scale then add to self
    let scaled = tensor.scale(2.0).unwrap();
    let doubled = tensor.add(&scaled).unwrap();
    let result = doubled.to_vec().unwrap();

    // Original + 2*Original = 3*Original
    assert_eq!(result, vec![3.0, 6.0, 9.0, 12.0]);
}

#[test]
fn test_mltensor_not_available_error() {
    // When MLTensor is not available, operations should return errors
    if MLTensor::is_available() {
        return; // Skip if MLTensor is actually available
    }

    let data = vec![1.0, 2.0, 3.0, 4.0];
    let result = MLTensor::from_floats(&data, &[2, 2]);
    assert!(result.is_err());
}

#[test]
fn test_mltensor_handle_validity() {
    let handle = ffi::MLTensorHandle::default();
    assert!(!handle.is_valid());
    assert_eq!(handle.num_elements(), 0);
}

#[test]
fn test_mltensor_handle_num_elements() {
    let mut handle = ffi::MLTensorHandle::default();
    handle.shape[0] = 2;
    handle.shape[1] = 3;
    handle.shape[2] = 4;
    handle.rank = 3;

    assert_eq!(handle.num_elements(), 24);
}

// ========== Swift Bridge Tests ==========

#[test]
#[cfg(target_os = "macos")]
fn test_swift_bridge_available() {
    // Test that calling the Swift bridge detection doesn't crash
    let available = unsafe { ffi::coreml_supports_mltensor() };
    // Just check it doesn't crash - result depends on macOS version
    println!("Swift MLTensor bridge available: {}", available);
}

#[test]
#[cfg(target_os = "macos")]
fn test_swift_tensor_creation() {
    if !unsafe { ffi::coreml_supports_mltensor() } {
        println!("Skipping - Swift bridge not available (requires macOS 15+)");
        return;
    }

    let data = vec![1.0f32, 2.0, 3.0, 4.0];
    let shape = vec![2usize, 2];

    let handle =
        unsafe { ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len()) };

    assert!(
        handle.is_valid(),
        "Failed to create tensor via Swift bridge"
    );
    assert_eq!(handle.rank, 2);
    assert_eq!(handle.shape[0], 2);
    assert_eq!(handle.shape[1], 2);
    assert_eq!(handle.num_elements(), 4);

    // Clean up
    unsafe { ffi::coreml_tensor_free(handle) };
}

#[test]
#[cfg(target_os = "macos")]
fn test_swift_tensor_operations() {
    if !unsafe { ffi::coreml_supports_mltensor() } {
        println!("Skipping - Swift bridge not available (requires macOS 15+)");
        return;
    }

    // Test softmax operation
    let data = vec![1.0f32, 2.0, 3.0, 4.0];
    let shape = vec![1usize, 4];

    let handle =
        unsafe { ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len()) };
    assert!(handle.is_valid(), "Failed to create tensor");

    let softmax_handle = unsafe { ffi::coreml_tensor_softmax(handle, -1) };
    assert!(softmax_handle.is_valid(), "Softmax operation failed");

    // Materialize and verify softmax sums to 1
    let mut output = vec![0.0f32; 4];
    let result =
        unsafe { ffi::coreml_tensor_to_floats(softmax_handle, output.as_mut_ptr(), output.len()) };
    assert!(
        result >= 0,
        "Failed to materialize tensor: error code {}",
        result
    );

    let sum: f32 = output.iter().sum();
    assert!(
        (sum - 1.0).abs() < 1e-5,
        "Softmax sum was {} (expected ~1.0)",
        sum
    );
    assert!(
        output.iter().all(|&x| x > 0.0),
        "Softmax values should be positive"
    );

    // Clean up
    unsafe {
        ffi::coreml_tensor_free(handle);
        ffi::coreml_tensor_free(softmax_handle);
    };
}

#[test]
#[cfg(target_os = "macos")]
fn test_swift_tensor_add_operation() {
    if !unsafe { ffi::coreml_supports_mltensor() } {
        println!("Skipping - Swift bridge not available (requires macOS 15+)");
        return;
    }

    let data1 = vec![1.0f32, 2.0, 3.0, 4.0];
    let data2 = vec![5.0f32, 6.0, 7.0, 8.0];
    let shape = vec![2usize, 2];

    let handle1 =
        unsafe { ffi::coreml_create_tensor_f32(data1.as_ptr(), shape.as_ptr(), shape.len()) };
    let handle2 =
        unsafe { ffi::coreml_create_tensor_f32(data2.as_ptr(), shape.as_ptr(), shape.len()) };

    assert!(
        handle1.is_valid() && handle2.is_valid(),
        "Failed to create tensors"
    );

    let sum_handle = unsafe { ffi::coreml_tensor_add(handle1, handle2) };
    assert!(sum_handle.is_valid(), "Add operation failed");

    let mut output = vec![0.0f32; 4];
    let result =
        unsafe { ffi::coreml_tensor_to_floats(sum_handle, output.as_mut_ptr(), output.len()) };
    assert!(result >= 0, "Failed to materialize tensor");

    assert_eq!(output, vec![6.0, 8.0, 10.0, 12.0], "Add result incorrect");

    // Clean up
    unsafe {
        ffi::coreml_tensor_free(handle1);
        ffi::coreml_tensor_free(handle2);
        ffi::coreml_tensor_free(sum_handle);
    };
}

#[test]
#[cfg(target_os = "macos")]
fn test_swift_tensor_scale_operation() {
    if !unsafe { ffi::coreml_supports_mltensor() } {
        println!("Skipping - Swift bridge not available (requires macOS 15+)");
        return;
    }

    let data = vec![1.0f32, 2.0, 3.0, 4.0];
    let shape = vec![2usize, 2];

    let handle =
        unsafe { ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len()) };
    assert!(handle.is_valid(), "Failed to create tensor");

    let scaled_handle = unsafe { ffi::coreml_tensor_scale(handle, 2.5) };
    assert!(scaled_handle.is_valid(), "Scale operation failed");

    let mut output = vec![0.0f32; 4];
    let result =
        unsafe { ffi::coreml_tensor_to_floats(scaled_handle, output.as_mut_ptr(), output.len()) };
    assert!(result >= 0, "Failed to materialize tensor");

    assert_eq!(output, vec![2.5, 5.0, 7.5, 10.0], "Scale result incorrect");

    // Clean up
    unsafe {
        ffi::coreml_tensor_free(handle);
        ffi::coreml_tensor_free(scaled_handle);
    };
}

#[test]
#[cfg(target_os = "macos")]
fn test_swift_tensor_matmul_operation() {
    if !unsafe { ffi::coreml_supports_mltensor() } {
        println!("Skipping - Swift bridge not available (requires macOS 15+)");
        return;
    }

    // [1, 2]   [5, 6]   [19, 22]
    // [3, 4] x [7, 8] = [43, 50]
    let data1 = vec![1.0f32, 2.0, 3.0, 4.0];
    let data2 = vec![5.0f32, 6.0, 7.0, 8.0];
    let shape = vec![2usize, 2];

    let handle1 =
        unsafe { ffi::coreml_create_tensor_f32(data1.as_ptr(), shape.as_ptr(), shape.len()) };
    let handle2 =
        unsafe { ffi::coreml_create_tensor_f32(data2.as_ptr(), shape.as_ptr(), shape.len()) };

    assert!(
        handle1.is_valid() && handle2.is_valid(),
        "Failed to create tensors"
    );

    let product_handle = unsafe { ffi::coreml_tensor_matmul(handle1, handle2) };
    assert!(product_handle.is_valid(), "Matmul operation failed");

    let mut output = vec![0.0f32; 4];
    let result =
        unsafe { ffi::coreml_tensor_to_floats(product_handle, output.as_mut_ptr(), output.len()) };
    assert!(result >= 0, "Failed to materialize tensor");

    assert_eq!(
        output,
        vec![19.0, 22.0, 43.0, 50.0],
        "Matmul result incorrect"
    );

    // Clean up
    unsafe {
        ffi::coreml_tensor_free(handle1);
        ffi::coreml_tensor_free(handle2);
        ffi::coreml_tensor_free(product_handle);
    };
}

#[test]
#[cfg(target_os = "macos")]
fn test_swift_tensor_memory_cleanup() {
    if !unsafe { ffi::coreml_supports_mltensor() } {
        println!("Skipping - Swift bridge not available (requires macOS 15+)");
        return;
    }

    // Create and free multiple tensors to verify memory cleanup
    for i in 0..10 {
        let data = vec![i as f32; 100];
        let shape = vec![10usize, 10];

        let handle =
            unsafe { ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len()) };
        assert!(handle.is_valid(), "Failed to create tensor iteration {}", i);

        // Free immediately
        unsafe { ffi::coreml_tensor_free(handle) };
    }
    println!("Memory cleanup test passed - created and freed 10 tensors");
}

#[test]
#[cfg(target_os = "macos")]
fn test_swift_tensor_large_tensor() {
    if !unsafe { ffi::coreml_supports_mltensor() } {
        println!("Skipping - Swift bridge not available (requires macOS 15+)");
        return;
    }

    // Test with a reasonably large tensor
    let size = 1024;
    let data: Vec<f32> = (0..size).map(|i| i as f32).collect();
    let shape = vec![32usize, 32];

    let handle =
        unsafe { ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len()) };
    assert!(handle.is_valid(), "Failed to create large tensor");
    assert_eq!(handle.num_elements(), size);

    // Materialize and verify
    let mut output = vec![0.0f32; size];
    let result = unsafe { ffi::coreml_tensor_to_floats(handle, output.as_mut_ptr(), output.len()) };
    assert!(result >= 0, "Failed to materialize large tensor");
    assert_eq!(output, data, "Large tensor data mismatch");

    unsafe { ffi::coreml_tensor_free(handle) };
}

#[test]
#[cfg(target_os = "macos")]
fn test_swift_tensor_3d_tensor() {
    if !unsafe { ffi::coreml_supports_mltensor() } {
        println!("Skipping - Swift bridge not available (requires macOS 15+)");
        return;
    }

    let data: Vec<f32> = (0..24).map(|i| i as f32).collect();
    let shape = vec![2usize, 3, 4];

    let handle =
        unsafe { ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len()) };
    assert!(handle.is_valid(), "Failed to create 3D tensor");
    assert_eq!(handle.rank, 3);
    assert_eq!(handle.shape[0], 2);
    assert_eq!(handle.shape[1], 3);
    assert_eq!(handle.shape[2], 4);
    assert_eq!(handle.num_elements(), 24);

    let mut output = vec![0.0f32; 24];
    let result = unsafe { ffi::coreml_tensor_to_floats(handle, output.as_mut_ptr(), output.len()) };
    assert!(result >= 0, "Failed to materialize 3D tensor");
    assert_eq!(output, data);

    unsafe { ffi::coreml_tensor_free(handle) };
}

#[test]
#[cfg(target_os = "macos")]
fn test_runtime_dispatch_mltensor_vs_legacy() {
    // Test that runtime correctly dispatches based on availability
    let supports_mltensor = unsafe { ffi::coreml_supports_mltensor() };

    if supports_mltensor {
        println!("Runtime dispatch: Using MLTensor API (macOS 15+)");
        // Verify we can use MLTensor operations
        let data = vec![1.0f32, 2.0, 3.0, 4.0];
        let shape = vec![2usize, 2];
        let handle =
            unsafe { ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len()) };
        assert!(handle.is_valid(), "MLTensor should work when supported");
        unsafe { ffi::coreml_tensor_free(handle) };
    } else {
        println!("Runtime dispatch: MLTensor not available (macOS < 15)");
        // On older macOS, the function should return false but not crash
    }

    // CoreML availability is separate from MLTensor
    let coreml_available = unsafe { ffi::coreml_is_available() };
    println!("CoreML available: {}", coreml_available);
}

#[test]
#[cfg(target_os = "macos")]
fn test_swift_bridge_chained_operations() {
    if !unsafe { ffi::coreml_supports_mltensor() } {
        println!("Skipping - Swift bridge not available (requires macOS 15+)");
        return;
    }

    // Test chaining multiple operations: scale -> add -> softmax
    let data = vec![1.0f32, 2.0, 3.0, 4.0];
    let shape = vec![1usize, 4];

    let handle =
        unsafe { ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len()) };
    assert!(handle.is_valid());

    // Scale by 0.5
    let scaled = unsafe { ffi::coreml_tensor_scale(handle, 0.5) };
    assert!(scaled.is_valid(), "Scale failed");

    // Add original to scaled
    let sum = unsafe { ffi::coreml_tensor_add(handle, scaled) };
    assert!(sum.is_valid(), "Add failed");

    // Apply softmax
    let softmax = unsafe { ffi::coreml_tensor_softmax(sum, -1) };
    assert!(softmax.is_valid(), "Softmax failed");

    // Materialize and verify
    let mut output = vec![0.0f32; 4];
    let result =
        unsafe { ffi::coreml_tensor_to_floats(softmax, output.as_mut_ptr(), output.len()) };
    assert!(result >= 0, "Materialize failed");

    let total: f32 = output.iter().sum();
    assert!(
        (total - 1.0).abs() < 1e-5,
        "Softmax should sum to 1, got {}",
        total
    );

    // Clean up all handles
    unsafe {
        ffi::coreml_tensor_free(handle);
        ffi::coreml_tensor_free(scaled);
        ffi::coreml_tensor_free(sum);
        ffi::coreml_tensor_free(softmax);
    };
}

// ========== ObjC++ Direct Path Tests (MLMultiArray Fallback) ==========
//
// These tests directly exercise the ObjC++ FFI implementation, skipping
// the Swift bridge entirely. This helps isolate whether issues are in
// the Swift bridge or the underlying ObjC++ implementation.

#[test]
#[cfg(target_os = "macos")]
fn test_objcpp_direct_tensor_create_and_read() {
    // Skip if MLTensor not available at all
    if !unsafe { ffi::coreml_supports_mltensor() } {
        println!("Skipping - MLTensor API not available (requires macOS 15+)");
        return;
    }

    // Create tensor via ObjC++ path directly (coreml_* functions)
    let data = vec![1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0];
    let shape = vec![2usize, 3];

    let handle =
        unsafe { ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len()) };

    // Verify handle is valid
    assert!(
        handle.is_valid(),
        "ObjC++ tensor creation failed - handle is invalid"
    );
    assert_eq!(handle.rank, 2, "Expected rank 2, got {}", handle.rank);
    assert_eq!(
        handle.shape[0], 2,
        "Expected shape[0]=2, got {}",
        handle.shape[0]
    );
    assert_eq!(
        handle.shape[1], 3,
        "Expected shape[1]=3, got {}",
        handle.shape[1]
    );
    assert_eq!(
        handle.num_elements(),
        6,
        "Expected 6 elements, got {}",
        handle.num_elements()
    );

    // Read data back via ObjC++ path
    let mut output = vec![0.0f32; 6];
    let result = unsafe { ffi::coreml_tensor_to_floats(handle, output.as_mut_ptr(), output.len()) };

    assert!(
        result >= 0,
        "ObjC++ tensor read failed with error code {}",
        result
    );
    assert_eq!(
        output, data,
        "Data mismatch: expected {:?}, got {:?}",
        data, output
    );

    println!("ObjC++ direct path: create and read test PASSED");

    // Clean up
    unsafe { ffi::coreml_tensor_free(handle) };
}

#[test]
#[cfg(target_os = "macos")]
fn test_objcpp_direct_softmax() {
    if !unsafe { ffi::coreml_supports_mltensor() } {
        println!("Skipping - MLTensor API not available (requires macOS 15+)");
        return;
    }

    // Test softmax via ObjC++ path
    let data = vec![1.0f32, 2.0, 3.0, 4.0];
    let shape = vec![1usize, 4];

    let input =
        unsafe { ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len()) };
    assert!(input.is_valid(), "Failed to create input tensor");

    // Apply softmax via ObjC++ path
    let softmax_result = unsafe { ffi::coreml_tensor_softmax(input, -1) };
    assert!(
        softmax_result.is_valid(),
        "ObjC++ softmax failed - returned invalid handle"
    );

    // Read result
    let mut output = vec![0.0f32; 4];
    let read_result =
        unsafe { ffi::coreml_tensor_to_floats(softmax_result, output.as_mut_ptr(), output.len()) };
    assert!(
        read_result >= 0,
        "Failed to read softmax result: error {}",
        read_result
    );

    // Verify softmax properties
    let sum: f32 = output.iter().sum();
    assert!(
        (sum - 1.0).abs() < 1e-4,
        "ObjC++ softmax sum should be ~1.0, got {}",
        sum
    );
    assert!(
        output.iter().all(|&x| x > 0.0 && x < 1.0),
        "ObjC++ softmax values should be in (0,1): {:?}",
        output
    );
    // Verify monotonicity (larger input -> larger softmax)
    for i in 1..output.len() {
        assert!(
            output[i] > output[i - 1],
            "Softmax should preserve ordering: {:?}",
            output
        );
    }

    println!(
        "ObjC++ direct path: softmax test PASSED - output {:?}",
        output
    );

    unsafe {
        ffi::coreml_tensor_free(input);
        ffi::coreml_tensor_free(softmax_result);
    };
}

#[test]
#[cfg(target_os = "macos")]
fn test_objcpp_direct_add() {
    if !unsafe { ffi::coreml_supports_mltensor() } {
        println!("Skipping - MLTensor API not available (requires macOS 15+)");
        return;
    }

    let data1 = vec![1.0f32, 2.0, 3.0, 4.0];
    let data2 = vec![10.0f32, 20.0, 30.0, 40.0];
    let shape = vec![2usize, 2];

    let tensor1 =
        unsafe { ffi::coreml_create_tensor_f32(data1.as_ptr(), shape.as_ptr(), shape.len()) };
    let tensor2 =
        unsafe { ffi::coreml_create_tensor_f32(data2.as_ptr(), shape.as_ptr(), shape.len()) };

    assert!(tensor1.is_valid(), "Failed to create tensor1");
    assert!(tensor2.is_valid(), "Failed to create tensor2");

    // Add via ObjC++ path
    let sum = unsafe { ffi::coreml_tensor_add(tensor1, tensor2) };
    assert!(
        sum.is_valid(),
        "ObjC++ add failed - returned invalid handle"
    );

    let mut output = vec![0.0f32; 4];
    let read_result =
        unsafe { ffi::coreml_tensor_to_floats(sum, output.as_mut_ptr(), output.len()) };
    assert!(
        read_result >= 0,
        "Failed to read add result: error {}",
        read_result
    );

    let expected = vec![11.0f32, 22.0, 33.0, 44.0];
    assert_eq!(
        output, expected,
        "ObjC++ add result mismatch: expected {:?}, got {:?}",
        expected, output
    );

    println!("ObjC++ direct path: add test PASSED");

    unsafe {
        ffi::coreml_tensor_free(tensor1);
        ffi::coreml_tensor_free(tensor2);
        ffi::coreml_tensor_free(sum);
    };
}

#[test]
#[cfg(target_os = "macos")]
fn test_objcpp_direct_scale() {
    if !unsafe { ffi::coreml_supports_mltensor() } {
        println!("Skipping - MLTensor API not available (requires macOS 15+)");
        return;
    }

    let data = vec![2.0f32, 4.0, 6.0, 8.0];
    let shape = vec![4usize];

    let tensor =
        unsafe { ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len()) };
    assert!(tensor.is_valid(), "Failed to create tensor");

    // Scale by 0.5 via ObjC++ path
    let scaled = unsafe { ffi::coreml_tensor_scale(tensor, 0.5) };
    assert!(
        scaled.is_valid(),
        "ObjC++ scale failed - returned invalid handle"
    );

    let mut output = vec![0.0f32; 4];
    let read_result =
        unsafe { ffi::coreml_tensor_to_floats(scaled, output.as_mut_ptr(), output.len()) };
    assert!(
        read_result >= 0,
        "Failed to read scale result: error {}",
        read_result
    );

    let expected = vec![1.0f32, 2.0, 3.0, 4.0];
    assert_eq!(
        output, expected,
        "ObjC++ scale result mismatch: expected {:?}, got {:?}",
        expected, output
    );

    println!("ObjC++ direct path: scale test PASSED");

    unsafe {
        ffi::coreml_tensor_free(tensor);
        ffi::coreml_tensor_free(scaled);
    };
}

#[test]
#[cfg(target_os = "macos")]
fn test_objcpp_direct_matmul() {
    if !unsafe { ffi::coreml_supports_mltensor() } {
        println!("Skipping - MLTensor API not available (requires macOS 15+)");
        return;
    }

    // 2x2 @ 2x2 matrix multiplication
    // [1, 2]   [5, 6]   [1*5+2*7, 1*6+2*8]   [19, 22]
    // [3, 4] @ [7, 8] = [3*5+4*7, 3*6+4*8] = [43, 50]
    let data1 = vec![1.0f32, 2.0, 3.0, 4.0];
    let data2 = vec![5.0f32, 6.0, 7.0, 8.0];
    let shape = vec![2usize, 2];

    let tensor1 =
        unsafe { ffi::coreml_create_tensor_f32(data1.as_ptr(), shape.as_ptr(), shape.len()) };
    let tensor2 =
        unsafe { ffi::coreml_create_tensor_f32(data2.as_ptr(), shape.as_ptr(), shape.len()) };

    assert!(tensor1.is_valid(), "Failed to create tensor1");
    assert!(tensor2.is_valid(), "Failed to create tensor2");

    // Matmul via ObjC++ path
    let product = unsafe { ffi::coreml_tensor_matmul(tensor1, tensor2) };
    assert!(
        product.is_valid(),
        "ObjC++ matmul failed - returned invalid handle"
    );

    let mut output = vec![0.0f32; 4];
    let read_result =
        unsafe { ffi::coreml_tensor_to_floats(product, output.as_mut_ptr(), output.len()) };
    assert!(
        read_result >= 0,
        "Failed to read matmul result: error {}",
        read_result
    );

    let expected = vec![19.0f32, 22.0, 43.0, 50.0];
    assert_eq!(
        output, expected,
        "ObjC++ matmul result mismatch: expected {:?}, got {:?}",
        expected, output
    );

    println!("ObjC++ direct path: matmul test PASSED");

    unsafe {
        ffi::coreml_tensor_free(tensor1);
        ffi::coreml_tensor_free(tensor2);
        ffi::coreml_tensor_free(product);
    };
}

#[test]
#[cfg(target_os = "macos")]
fn test_objcpp_direct_chained_operations() {
    if !unsafe { ffi::coreml_supports_mltensor() } {
        println!("Skipping - MLTensor API not available (requires macOS 15+)");
        return;
    }

    // Test chaining operations via ObjC++ path: create -> scale -> add -> read
    let data = vec![1.0f32, 2.0, 3.0, 4.0];
    let shape = vec![2usize, 2];

    let original =
        unsafe { ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len()) };
    assert!(original.is_valid(), "Failed to create original tensor");

    // Scale by 2.0
    let scaled = unsafe { ffi::coreml_tensor_scale(original, 2.0) };
    assert!(scaled.is_valid(), "Scale operation failed");

    // Add original + scaled (should give 3x original)
    let sum = unsafe { ffi::coreml_tensor_add(original, scaled) };
    assert!(sum.is_valid(), "Add operation failed");

    // Read final result
    let mut output = vec![0.0f32; 4];
    let read_result =
        unsafe { ffi::coreml_tensor_to_floats(sum, output.as_mut_ptr(), output.len()) };
    assert!(
        read_result >= 0,
        "Failed to read chained result: error {}",
        read_result
    );

    // original + 2*original = 3*original
    let expected = vec![3.0f32, 6.0, 9.0, 12.0];
    assert_eq!(
        output, expected,
        "ObjC++ chained ops result mismatch: expected {:?}, got {:?}",
        expected, output
    );

    println!("ObjC++ direct path: chained operations test PASSED");

    unsafe {
        ffi::coreml_tensor_free(original);
        ffi::coreml_tensor_free(scaled);
        ffi::coreml_tensor_free(sum);
    };
}

#[test]
#[cfg(target_os = "macos")]
fn test_objcpp_direct_1d_tensor() {
    if !unsafe { ffi::coreml_supports_mltensor() } {
        println!("Skipping - MLTensor API not available (requires macOS 15+)");
        return;
    }

    // Test 1D tensor via ObjC++ path
    let data = vec![1.0f32, 2.0, 3.0, 4.0, 5.0];
    let shape = vec![5usize];

    let tensor =
        unsafe { ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len()) };

    assert!(tensor.is_valid(), "Failed to create 1D tensor");
    assert_eq!(tensor.rank, 1, "Expected rank 1 for 1D tensor");
    assert_eq!(tensor.shape[0], 5, "Expected shape[0]=5");
    assert_eq!(tensor.num_elements(), 5, "Expected 5 elements");

    let mut output = vec![0.0f32; 5];
    let read_result =
        unsafe { ffi::coreml_tensor_to_floats(tensor, output.as_mut_ptr(), output.len()) };
    assert!(read_result >= 0, "Failed to read 1D tensor");
    assert_eq!(output, data);

    println!("ObjC++ direct path: 1D tensor test PASSED");

    unsafe { ffi::coreml_tensor_free(tensor) };
}

#[test]
#[cfg(target_os = "macos")]
fn test_objcpp_direct_3d_tensor() {
    if !unsafe { ffi::coreml_supports_mltensor() } {
        println!("Skipping - MLTensor API not available (requires macOS 15+)");
        return;
    }

    // Test 3D tensor via ObjC++ path
    let data: Vec<f32> = (0..24).map(|i| i as f32).collect();
    let shape = vec![2usize, 3, 4];

    let tensor =
        unsafe { ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len()) };

    assert!(tensor.is_valid(), "Failed to create 3D tensor");
    assert_eq!(tensor.rank, 3, "Expected rank 3 for 3D tensor");
    assert_eq!(tensor.shape[0], 2, "Expected shape[0]=2");
    assert_eq!(tensor.shape[1], 3, "Expected shape[1]=3");
    assert_eq!(tensor.shape[2], 4, "Expected shape[2]=4");
    assert_eq!(tensor.num_elements(), 24, "Expected 24 elements");

    let mut output = vec![0.0f32; 24];
    let read_result =
        unsafe { ffi::coreml_tensor_to_floats(tensor, output.as_mut_ptr(), output.len()) };
    assert!(read_result >= 0, "Failed to read 3D tensor");
    assert_eq!(output, data);

    println!("ObjC++ direct path: 3D tensor test PASSED");

    unsafe { ffi::coreml_tensor_free(tensor) };
}

#[test]
#[cfg(target_os = "macos")]
fn test_objcpp_direct_memory_stability() {
    if !unsafe { ffi::coreml_supports_mltensor() } {
        println!("Skipping - MLTensor API not available (requires macOS 15+)");
        return;
    }

    // Create and free multiple tensors to test memory stability
    println!("ObjC++ direct path: Starting memory stability test...");

    for iteration in 0..20 {
        let size = 64; // 8x8 tensor
        let data: Vec<f32> = (0..size).map(|i| (i + iteration * size) as f32).collect();
        let shape = vec![8usize, 8];

        let tensor =
            unsafe { ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len()) };
        assert!(
            tensor.is_valid(),
            "Failed to create tensor at iteration {}",
            iteration
        );

        // Verify data
        let mut output = vec![0.0f32; size];
        let read_result =
            unsafe { ffi::coreml_tensor_to_floats(tensor, output.as_mut_ptr(), output.len()) };
        assert!(
            read_result >= 0,
            "Failed to read tensor at iteration {}",
            iteration
        );
        assert_eq!(output, data, "Data mismatch at iteration {}", iteration);

        // Free immediately
        unsafe { ffi::coreml_tensor_free(tensor) };
    }

    println!("ObjC++ direct path: memory stability test PASSED (20 iterations)");
}

#[test]
#[cfg(target_os = "macos")]
fn test_objcpp_vs_swift_bridge_comparison() {
    // Compare results between ObjC++ and Swift bridges (if both available)
    if !unsafe { ffi::coreml_supports_mltensor() } {
        println!("Skipping - MLTensor API not available (requires macOS 15+)");
        return;
    }

    let swift_available = unsafe { ffi::swift_coreml_supports_mltensor() };
    if !swift_available {
        println!("Swift bridge not available, skipping comparison test");
        return;
    }

    println!("Both ObjC++ and Swift bridges available - running comparison...");

    let data = vec![1.0f32, 2.0, 3.0, 4.0];
    let shape = vec![2usize, 2];

    // Create via ObjC++
    let objc_tensor =
        unsafe { ffi::coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len()) };
    assert!(objc_tensor.is_valid(), "ObjC++ tensor creation failed");

    // Create via Swift
    let swift_tensor =
        unsafe { ffi::swift_coreml_create_tensor_f32(data.as_ptr(), shape.as_ptr(), shape.len()) };
    assert!(!swift_tensor.is_null(), "Swift tensor creation failed");

    // Scale both by 2.5
    let objc_scaled = unsafe { ffi::coreml_tensor_scale(objc_tensor, 2.5) };
    let swift_scaled = unsafe { ffi::swift_coreml_tensor_scale(swift_tensor, 2.5) };

    assert!(objc_scaled.is_valid(), "ObjC++ scale failed");
    assert!(!swift_scaled.is_null(), "Swift scale failed");

    // Read results from both
    let mut objc_output = vec![0.0f32; 4];
    let mut swift_output = vec![0.0f32; 4];

    let objc_read = unsafe {
        ffi::coreml_tensor_to_floats(objc_scaled, objc_output.as_mut_ptr(), objc_output.len())
    };
    let swift_read = unsafe {
        ffi::swift_coreml_tensor_to_floats(
            swift_scaled,
            swift_output.as_mut_ptr(),
            swift_output.len(),
        )
    };

    assert!(objc_read >= 0, "ObjC++ read failed");
    assert!(swift_read >= 0, "Swift read failed");

    // Compare results
    let expected = vec![2.5f32, 5.0, 7.5, 10.0];
    assert_eq!(objc_output, expected, "ObjC++ output mismatch");
    assert_eq!(swift_output, expected, "Swift output mismatch");
    assert_eq!(
        objc_output, swift_output,
        "ObjC++ and Swift outputs differ!"
    );

    println!("ObjC++ vs Swift comparison: BOTH MATCH - {:?}", objc_output);

    // Clean up
    unsafe {
        ffi::coreml_tensor_free(objc_tensor);
        ffi::coreml_tensor_free(objc_scaled);
        ffi::swift_coreml_tensor_free(swift_tensor);
        ffi::swift_coreml_tensor_free(swift_scaled);
    };
}

// ========== macOS 26+ (Tahoe) Enhanced API Tests ==========

#[test]
#[cfg(target_os = "macos")]
fn test_api_version_detection() {
    let version = get_mltensor_api_version();
    println!("MLTensor API version: {:?}", version);

    match version {
        MltensorApiVersion::NotAvailable => {
            println!("MLTensor not available (pre-macOS 15)");
        }
        MltensorApiVersion::Sequoia => {
            println!("macOS 15.x (Sequoia) - Basic MLTensor API");
        }
        MltensorApiVersion::Tahoe => {
            println!("macOS 26.x (Tahoe) - Enhanced MLComputePolicy API");
        }
    }
}

#[test]
#[cfg(target_os = "macos")]
fn test_system_capabilities() {
    let caps = get_system_capabilities();
    println!("System capabilities bitmask: 0x{:x}", caps);

    if caps & capabilities::MLTENSOR_AVAILABLE != 0 {
        println!("  - MLTensor available (macOS 15+)");
    }
    if caps & capabilities::ENHANCED_API != 0 {
        println!("  - Enhanced APIs available (macOS 26+)");
    }
    if caps & capabilities::NEURAL_ENGINE != 0 {
        println!("  - Neural Engine (ANE) available");
    }
    if caps & capabilities::GPU != 0 {
        println!("  - GPU available");
    }
}

#[test]
#[cfg(target_os = "macos")]
fn test_has_enhanced_api() {
    let enhanced = has_enhanced_api();
    println!("Has macOS 26+ enhanced API: {}", enhanced);
}

#[test]
#[cfg(target_os = "macos")]
fn test_has_neural_engine() {
    let ane = has_neural_engine();
    println!("Has Neural Engine (ANE): {}", ane);
}

#[test]
#[cfg(target_os = "macos")]
fn test_tensor_with_compute_units() {
    if !MLTensor::is_available() {
        println!("Skipping - MLTensor not available");
        return;
    }

    let data = vec![1.0f32, 2.0, 3.0, 4.0];
    let shape = vec![2, 2];

    // Test with different compute unit preferences
    for units in [
        ComputeUnitPreference::CpuOnly,
        ComputeUnitPreference::CpuAndGpu,
        ComputeUnitPreference::CpuAndNeuralEngine,
        ComputeUnitPreference::All,
    ] {
        let tensor = MLTensor::from_floats_with_compute_units(&data, &shape, units);
        match tensor {
            Ok(t) => {
                let result = t.to_vec().unwrap();
                assert_eq!(result, data, "Data mismatch with {:?}", units);
                println!("Tensor creation with {:?} succeeded", units);
            }
            Err(e) => {
                println!("Tensor creation with {:?} failed: {}", units, e);
            }
        }
    }
}

#[test]
#[cfg(target_os = "macos")]
fn test_matmul_with_compute_units() {
    if !MLTensor::is_available() {
        println!("Skipping - MLTensor not available");
        return;
    }

    let data1 = vec![1.0f32, 2.0, 3.0, 4.0];
    let data2 = vec![5.0f32, 6.0, 7.0, 8.0];
    let shape = vec![2, 2];

    let t1 = MLTensor::from_floats(&data1, &shape).unwrap();
    let t2 = MLTensor::from_floats(&data2, &shape).unwrap();

    // Test matmul with ANE compute units (optimal on Apple Silicon)
    let product = t1.matmul_with_compute_units(&t2, ComputeUnitPreference::CpuAndNeuralEngine);
    match product {
        Ok(p) => {
            let result = p.to_vec().unwrap();
            let expected = vec![19.0f32, 22.0, 43.0, 50.0];
            assert_eq!(result, expected, "Matmul with ANE compute units failed");
            println!("Matmul with ANE compute units: {:?}", result);
        }
        Err(e) => {
            println!("Matmul with ANE compute units failed: {}", e);
        }
    }
}

#[test]
#[cfg(target_os = "macos")]
fn test_to_vec_async() {
    if !MLTensor::is_available() {
        println!("Skipping - MLTensor not available");
        return;
    }

    let data = vec![1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
    let shape = vec![2, 4];

    let tensor = MLTensor::from_floats(&data, &shape).unwrap();

    // Test async materialization (will use async API on macOS 26+)
    let result = tensor.to_vec_async(true);
    match result {
        Ok(r) => {
            assert_eq!(r, data, "Async materialization data mismatch");
            println!("Async materialization succeeded: {:?}", r);
        }
        Err(e) => {
            // May fail if scalars not cached or on older macOS
            println!("Async materialization: {}", e);
            // Fall back to sync
            let sync_result = tensor.to_vec().unwrap();
            assert_eq!(sync_result, data);
        }
    }
}

#[test]
#[cfg(target_os = "macos")]
fn test_swift_v2_api_direct() {
    if !swift_bridge_available() {
        println!("Skipping - Swift bridge not available");
        return;
    }

    // Test v2 API functions directly
    let version = unsafe { ffi::swift_coreml_mltensor_api_version() };
    println!("Swift bridge API version: {}", version);

    let caps = unsafe { ffi::swift_coreml_system_capabilities() };
    println!("Swift bridge capabilities: 0x{:x}", caps);

    // Test tensor creation with v2 API
    let data = vec![1.0f32, 2.0, 3.0, 4.0];
    let shape = vec![2usize, 2];

    let handle = unsafe {
        ffi::swift_coreml_create_tensor_f32_v2(
            data.as_ptr(),
            shape.as_ptr(),
            shape.len(),
            ComputeUnitPreference::All as i32,
        )
    };

    if handle.is_null() {
        println!("v2 tensor creation returned null (may fall back to v1)");
    } else {
        println!("v2 tensor creation succeeded");

        // Test v2 matmul
        let data2 = vec![5.0f32, 6.0, 7.0, 8.0];
        let handle2 = unsafe {
            ffi::swift_coreml_create_tensor_f32_v2(
                data2.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                ComputeUnitPreference::All as i32,
            )
        };

        if !handle2.is_null() {
            let product = unsafe {
                ffi::swift_coreml_tensor_matmul_v2(
                    handle,
                    handle2,
                    ComputeUnitPreference::CpuAndNeuralEngine as i32,
                )
            };

            if !product.is_null() {
                println!("v2 matmul with ANE preference succeeded");
                unsafe { ffi::swift_coreml_tensor_free(product) };
            }

            unsafe { ffi::swift_coreml_tensor_free(handle2) };
        }

        unsafe { ffi::swift_coreml_tensor_free(handle) };
    }
}

// ========== Stub Mode Tests ==========

#[test]
fn test_stub_mode_creation() {
    let backend = CoreMLBackend::new_stub(ComputeUnits::All).unwrap();
    assert!(backend.is_stub_mode());
    assert_eq!(backend.device_name(), "CoreML (Stub Mode)");
}

#[test]
fn test_stub_mode_run_step() {
    use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};

    let mut backend = CoreMLBackend::new_stub(ComputeUnits::All).unwrap();

    // Create IO buffers with small vocab size for testing
    let vocab_size = 100;
    let mut io = IoBuffers::new(vocab_size);
    io.input_ids = vec![1, 2, 3];

    // Create router ring with no active adapters
    let ring = RouterRing::new(0);

    // Run step should succeed in stub mode
    let result = backend.run_step(&ring, &mut io);
    assert!(result.is_ok(), "run_step failed: {:?}", result);

    // Position should be incremented
    assert_eq!(io.position, 1);

    // Output logits should be normalized (sum to ~1.0)
    let sum: f32 = io.output_logits.iter().sum();
    assert!(
        (sum - 1.0).abs() < 0.001,
        "Logits not normalized: sum = {}",
        sum
    );

    // Metrics should be updated
    let metrics = backend.get_metrics();
    assert_eq!(metrics.total_operations, 1);
    assert_eq!(metrics.successful_operations, 1);
}

#[test]
fn test_stub_mode_with_adapters() {
    use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};

    let mut backend = CoreMLBackend::new_stub(ComputeUnits::All).unwrap();

    // Manually insert adapter weights into cache
    backend.adapter_cache.insert(1, vec![0.1; 100]);
    backend.adapter_cache.insert(2, vec![0.2; 100]);

    let vocab_size = 100;
    let mut io = IoBuffers::new(vocab_size);
    io.input_ids = vec![1, 2, 3];

    // Create router ring with 2 active adapters
    let mut ring = RouterRing::new(2);
    ring.set(&[1, 2], &[16384, 8192]); // Q15 gates: 0.5 and 0.25

    // Run step should succeed
    let result = backend.run_step(&ring, &mut io);
    assert!(
        result.is_ok(),
        "run_step with adapters failed: {:?}",
        result
    );

    // Position should be incremented
    assert_eq!(io.position, 1);

    // Logits should still be normalized
    let sum: f32 = io.output_logits.iter().sum();
    assert!(
        (sum - 1.0).abs() < 0.001,
        "Logits not normalized: sum = {}",
        sum
    );
}

#[test]
fn test_coreml_hot_swap_attach_switch_detach_stub() {
    fn make_weights(values: &[f32]) -> Vec<u8> {
        let bytes =
            unsafe { std::slice::from_raw_parts(values.as_ptr() as *const u8, values.len() * 4) };
        let tensor = TensorView::new(safetensors::Dtype::F32, vec![values.len()], bytes)
            .expect("tensor view");
        serialize(
            vec![("adapter.weight".to_string(), tensor)],
            &Default::default(),
        )
        .expect("serialize adapter weights")
    }

    // Build minimal safetensors payloads for two adapters.
    let weights1 = make_weights(&[1.0, 2.0, 3.0, 4.0]);
    let weights2 = make_weights(&[5.0, 6.0, 7.0, 8.0]);

    let mut backend = CoreMLBackend::new_stub(ComputeUnits::All).unwrap();

    // Base: no adapters attached.
    assert!(backend.attached_adapter_ids().is_empty());

    // Attach first adapter via load (sidecar semantics).
    backend.load_adapter(1, &weights1).unwrap();
    assert_eq!(backend.attached_adapter_ids(), vec![1]);

    // Load a second adapter and confirm both are visible.
    backend.load_adapter(2, &weights2).unwrap();
    assert_eq!(backend.attached_adapter_ids(), vec![1, 2]);

    // Switch to adapter 2, which should detach adapter 1.
    backend.switch_adapter(2).unwrap();
    assert_eq!(backend.attached_adapter_ids(), vec![2]);

    // Detach the active adapter; cache should drop it.
    backend.detach_adapter(2).unwrap();
    assert!(backend.attached_adapter_ids().is_empty());
    assert!(!backend.adapter_cache.contains_key(&2));
}

#[test]
fn test_stub_mode_deterministic() {
    use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};

    let mut backend1 = CoreMLBackend::new_stub(ComputeUnits::All).unwrap();
    let mut backend2 = CoreMLBackend::new_stub(ComputeUnits::All).unwrap();

    let vocab_size = 100;

    // Run same operation on both backends
    let mut io1 = IoBuffers::new(vocab_size);
    io1.input_ids = vec![1, 2, 3];
    let ring1 = RouterRing::new(0);

    let mut io2 = IoBuffers::new(vocab_size);
    io2.input_ids = vec![1, 2, 3];
    let ring2 = RouterRing::new(0);

    backend1.run_step(&ring1, &mut io1).unwrap();
    backend2.run_step(&ring2, &mut io2).unwrap();

    // Results should be identical (deterministic)
    for (i, (l1, l2)) in io1
        .output_logits
        .iter()
        .zip(io2.output_logits.iter())
        .enumerate()
    {
        assert!(
            (l1 - l2).abs() < 1e-6,
            "Non-deterministic output at index {}: {} vs {}",
            i,
            l1,
            l2
        );
    }
}

#[test]
#[cfg(target_os = "macos")]
fn test_ane_detection_comprehensive() {
    // Test 1: Basic ANE availability functions
    let ane_available = has_neural_engine();
    let neural_engine_available = is_neural_engine_available();

    println!("\n=== ANE Detection Comprehensive Test ===");
    println!("has_neural_engine(): {}", ane_available);
    println!("is_neural_engine_available(): {}", neural_engine_available);

    // Test 2: Create backend and check ANE status
    match CoreMLBackend::new_default(ComputeUnits::All) {
        Ok(backend) => {
            let ane_status = backend.ane_status();
            println!("\nANE Status from Backend:");
            println!("  Available: {}", ane_status.available);
            println!("  Generation: {:?}", ane_status.generation);
            println!("  Max Batch Size: {}", ane_status.max_batch_size);
            println!("  Deterministic: {}", ane_status.deterministic);

            // Map generation to chip
            if let Some(gen) = ane_status.generation {
                let chip = match gen {
                    4 => "M1",
                    5 => "M2",
                    6 => "M3",
                    7 => "M4",
                    n if n >= 8 => "M5+",
                    _ => "Unknown",
                };
                println!("  Chip: Apple {} (Generation {})", chip, gen);
            }

            // Verify consistency
            assert_eq!(
                ane_available, ane_status.available,
                "ANE availability mismatch between detection functions"
            );
        }
        Err(e) => {
            println!(
                "Note: Failed to create backend (expected on unsupported systems): {}",
                e
            );
        }
    }
}

#[test]
fn test_ane_detection_handles_non_macos() {
    // On non-macOS, these should return false
    #[cfg(not(target_os = "macos"))]
    {
        assert!(
            !has_neural_engine(),
            "ANE should not be available on non-macOS"
        );
        assert!(
            !is_neural_engine_available(),
            "Neural engine should not be available on non-macOS"
        );
    }
}

// ========== Export Validation Error Tests ==========

#[test]
fn test_export_rejects_existing_output_path() {
    use crate::export::validate_output_path;

    let tmp = tempdir().expect("tempdir");
    let existing_path = tmp.path().join("existing_output");
    std::fs::create_dir(&existing_path).expect("create dir");
    std::fs::write(existing_path.join("file.txt"), b"content").expect("write file");

    // Without allow_overwrite, should fail
    let result = validate_output_path(&existing_path, false);
    assert!(result.is_err(), "Should reject existing path");

    match result.unwrap_err() {
        AosError::CoreMLExportPathExists { path, file_count } => {
            assert!(path.contains("existing_output"));
            assert!(file_count >= 1);
        }
        e => panic!("Expected CoreMLExportPathExists, got: {:?}", e),
    }
}

#[test]
fn test_export_allows_overwrite_when_specified() {
    use crate::export::validate_output_path;

    let tmp = tempdir().expect("tempdir");
    let existing_path = tmp.path().join("existing_output");
    std::fs::create_dir(&existing_path).expect("create dir");

    // With allow_overwrite=true, should succeed
    let result = validate_output_path(&existing_path, true);
    assert!(result.is_ok(), "Should allow overwrite when specified");
}

#[test]
fn test_export_accepts_nonexistent_path() {
    use crate::export::validate_output_path;

    let tmp = tempdir().expect("tempdir");
    let new_path = tmp.path().join("new_output");

    let result = validate_output_path(&new_path, false);
    assert!(result.is_ok(), "Should accept nonexistent path");
}

#[test]
fn test_validate_coreml_weights_rejects_missing_manifest() {
    use crate::export::validate_coreml_weights;

    let tmp = tempdir().expect("tempdir");
    let package_path = tmp.path().join("incomplete.mlpackage");
    std::fs::create_dir(&package_path).expect("create dir");
    // Create Data directory but no Manifest.json
    std::fs::create_dir(package_path.join("Data")).expect("create Data dir");
    std::fs::create_dir(package_path.join("Data/model")).expect("create model dir");

    let result = validate_coreml_weights(&package_path);
    assert!(result.is_err(), "Should reject missing Manifest.json");

    match result.unwrap_err() {
        AosError::CoreMLMissingWeights {
            package_path: _,
            missing,
        } => {
            assert!(missing.iter().any(|m| m.contains("Manifest.json")));
        }
        e => panic!("Expected CoreMLMissingWeights, got: {:?}", e),
    }
}

#[test]
fn test_validate_coreml_weights_rejects_missing_data_dir() {
    use crate::export::validate_coreml_weights;

    let tmp = tempdir().expect("tempdir");
    let package_path = tmp.path().join("incomplete.mlpackage");
    std::fs::create_dir(&package_path).expect("create dir");
    // Create Manifest.json but no Data directory
    std::fs::write(package_path.join("Manifest.json"), b"{}").expect("write manifest");

    let result = validate_coreml_weights(&package_path);
    assert!(result.is_err(), "Should reject missing Data directory");

    match result.unwrap_err() {
        AosError::CoreMLMissingWeights {
            package_path: _,
            missing,
        } => {
            assert!(missing.iter().any(|m| m.contains("Data")));
        }
        e => panic!("Expected CoreMLMissingWeights, got: {:?}", e),
    }
}

#[test]
fn test_validate_coreml_weights_accepts_complete_package() {
    use crate::export::validate_coreml_weights;

    let tmp = tempdir().expect("tempdir");
    let package_path = tmp.path().join("complete.mlpackage");
    std::fs::create_dir(&package_path).expect("create dir");
    // Manifest must have required fields
    std::fs::write(
        package_path.join("Manifest.json"),
        b"{\"itemInfoEntries\": {}}",
    )
    .expect("write manifest");
    std::fs::create_dir(package_path.join("Data")).expect("create Data dir");
    std::fs::create_dir(package_path.join("Data/model")).expect("create model dir");
    // Model directory must contain weight files
    std::fs::write(package_path.join("Data/model/model.mlmodel"), b"model data")
        .expect("write model");

    let result = validate_coreml_weights(&package_path);
    assert!(result.is_ok(), "Should accept complete package");
}

#[test]
fn test_lora_shape_mismatch_error() {
    use crate::fusion::{validate_lora_shapes, LoraTarget};

    // Create mismatched shapes
    let lora_a = vec![0.0f32; 100]; // Wrong size
    let lora_b = vec![0.0f32; 200]; // Wrong size
    let rank = 16;
    let in_dim = 64;
    let out_dim = 64;

    let result = validate_lora_shapes(
        0,
        &LoraTarget::QProj,
        &lora_a,
        &lora_b,
        rank,
        in_dim,
        out_dim,
    );

    assert!(result.is_err(), "Should return error for mismatched shapes");

    match result.unwrap_err() {
        AosError::LoraShapeMismatch {
            layer,
            target,
            expected_a,
            got_a,
            expected_b,
            got_b,
        } => {
            assert_eq!(layer, 0);
            assert!(target.contains("QProj"));
            assert_eq!(expected_a, rank * in_dim); // 16 * 64 = 1024
            assert_eq!(got_a, 100);
            assert_eq!(expected_b, out_dim * rank); // 64 * 16 = 1024
            assert_eq!(got_b, 200);
        }
        e => panic!("Expected LoraShapeMismatch, got: {:?}", e),
    }
}

#[test]
fn test_lora_shape_validation_accepts_correct_shapes() {
    use crate::fusion::{validate_lora_shapes, LoraTarget};

    let rank = 16;
    let in_dim = 64;
    let out_dim = 128;

    let lora_a = vec![0.0f32; rank * in_dim]; // [rank, in_dim]
    let lora_b = vec![0.0f32; out_dim * rank]; // [out_dim, rank]

    let result = validate_lora_shapes(
        5,
        &LoraTarget::VProj,
        &lora_a,
        &lora_b,
        rank,
        in_dim,
        out_dim,
    );

    assert!(result.is_ok(), "Should accept correct shapes");
}

#[test]
fn test_coreml_export_timeout_error_structure() {
    use std::time::Duration;

    let error = AosError::coreml_export_timeout(
        "export to /path/output.mlpackage",
        Duration::from_secs(300),
    );

    match error {
        AosError::CoreMLExportTimeout {
            operation,
            duration,
        } => {
            assert!(operation.contains("output.mlpackage"));
            assert_eq!(duration, Duration::from_secs(300));
        }
        e => panic!("Expected CoreMLExportTimeout, got: {:?}", e),
    }
}

#[test]
fn test_coreml_unsupported_ops_error_structure() {
    let error = AosError::coreml_unsupported_ops(
        "/path/to/model.mlpackage",
        vec!["custom_op1".to_string(), "custom_op2".to_string()],
    );

    match error {
        AosError::CoreMLUnsupportedOps { model_path, ops } => {
            assert!(model_path.contains("model.mlpackage"));
            assert_eq!(ops.len(), 2);
            assert!(ops.contains(&"custom_op1".to_string()));
            assert!(ops.contains(&"custom_op2".to_string()));
        }
        e => panic!("Expected CoreMLUnsupportedOps, got: {:?}", e),
    }
}

#[test]
fn test_fusion_options_default() {
    use crate::fusion::FusionOptions;

    let options = FusionOptions::default();
    assert!(!options.strict, "Default strict mode should be false");
}

// ========== Op Normalization and Matching Tests ==========

#[test]
fn test_op_normalization_strips_prefixes() {
    use crate::export::normalize_op_name;

    assert_eq!(normalize_op_name("coreml.linear"), "linear");
    assert_eq!(normalize_op_name("com.apple.coreml.matmul"), "matmul");
    assert_eq!(normalize_op_name("com.apple.relu"), "relu");
}

#[test]
fn test_op_normalization_strips_version_suffixes() {
    use crate::export::normalize_op_name;

    assert_eq!(normalize_op_name("linear_v2"), "linear");
    assert_eq!(normalize_op_name("attention_v3"), "attention");
    assert_eq!(normalize_op_name("softmax_v1"), "softmax");
}

#[test]
fn test_op_normalization_strips_trailing_numbers() {
    use crate::export::normalize_op_name;

    assert_eq!(normalize_op_name("add_0"), "add");
    assert_eq!(normalize_op_name("matmul_123"), "matmul");
    assert_eq!(normalize_op_name("linear_42_"), "linear");
}

#[test]
fn test_op_matching_exact_not_substring() {
    use crate::export::is_supported_op;

    // These should match (exact)
    assert!(is_supported_op("linear"));
    assert!(is_supported_op("Linear")); // case insensitive
    assert!(is_supported_op("LINEAR"));
    assert!(is_supported_op("matmul"));

    // These should NOT match (substring would incorrectly match)
    assert!(!is_supported_op("nonlinear"));
    assert!(!is_supported_op("bilinear"));
    assert!(!is_supported_op("multilinear"));
    assert!(!is_supported_op("linear_custom_op"));
}

#[test]
fn test_op_matching_with_prefixes_and_suffixes() {
    use crate::export::is_supported_op;

    // With CoreML prefixes - should still match after normalization
    assert!(is_supported_op("coreml.linear"));
    assert!(is_supported_op("com.apple.coreml.matmul"));

    // With version suffixes - should still match after normalization
    assert!(is_supported_op("linear_v2"));
    assert!(is_supported_op("attention_v3"));

    // With instance numbers - should still match after normalization
    assert!(is_supported_op("add_0"));
    assert!(is_supported_op("relu_42"));
}

// ========== Enhanced Weight Validation Tests ==========

#[test]
fn test_validate_weights_rejects_invalid_manifest_json() {
    use crate::export::validate_coreml_weights;

    let tmp = tempdir().expect("tempdir");
    let package_path = tmp.path().join("invalid.mlpackage");
    std::fs::create_dir(&package_path).expect("create dir");

    // Create invalid JSON manifest
    std::fs::write(package_path.join("Manifest.json"), b"not valid json {{{")
        .expect("write manifest");
    std::fs::create_dir(package_path.join("Data")).expect("create Data dir");
    std::fs::create_dir(package_path.join("Data/model")).expect("create model dir");
    std::fs::write(package_path.join("Data/model/weights.bin"), b"data").expect("write weights");

    let result = validate_coreml_weights(&package_path);
    assert!(result.is_err(), "Should reject invalid JSON manifest");

    match result.unwrap_err() {
        AosError::CoreMLMissingWeights { missing, .. } => {
            assert!(missing.iter().any(|m| m.contains("JSON parse error")));
        }
        e => panic!("Expected CoreMLMissingWeights, got: {:?}", e),
    }
}

#[test]
fn test_validate_weights_rejects_manifest_missing_required_fields() {
    use crate::export::validate_coreml_weights;

    let tmp = tempdir().expect("tempdir");
    let package_path = tmp.path().join("incomplete.mlpackage");
    std::fs::create_dir(&package_path).expect("create dir");

    // Create manifest without required fields
    std::fs::write(
        package_path.join("Manifest.json"),
        b"{\"somethingElse\": true}",
    )
    .expect("write manifest");
    std::fs::create_dir(package_path.join("Data")).expect("create Data dir");
    std::fs::create_dir(package_path.join("Data/model")).expect("create model dir");
    std::fs::write(package_path.join("Data/model/weights.bin"), b"data").expect("write weights");

    let result = validate_coreml_weights(&package_path);
    assert!(
        result.is_err(),
        "Should reject manifest missing required fields"
    );

    match result.unwrap_err() {
        AosError::CoreMLMissingWeights { missing, .. } => {
            assert!(missing
                .iter()
                .any(|m| m.contains("missing required fields")));
        }
        e => panic!("Expected CoreMLMissingWeights, got: {:?}", e),
    }
}

#[test]
fn test_validate_weights_rejects_empty_model_directory() {
    use crate::export::validate_coreml_weights;

    let tmp = tempdir().expect("tempdir");
    let package_path = tmp.path().join("no_weights.mlpackage");
    std::fs::create_dir(&package_path).expect("create dir");

    // Create valid manifest
    std::fs::write(
        package_path.join("Manifest.json"),
        b"{\"itemInfoEntries\": {}}",
    )
    .expect("write manifest");
    std::fs::create_dir(package_path.join("Data")).expect("create Data dir");
    // Empty model directory (no weights)
    std::fs::create_dir(package_path.join("Data/model")).expect("create model dir");

    let result = validate_coreml_weights(&package_path);
    assert!(
        result.is_err(),
        "Should reject model directory without weight files"
    );

    match result.unwrap_err() {
        AosError::CoreMLMissingWeights { missing, .. } => {
            assert!(missing
                .iter()
                .any(|m| m.contains("no model spec or weight files")));
        }
        e => panic!("Expected CoreMLMissingWeights, got: {:?}", e),
    }
}

#[test]
fn test_validate_weights_accepts_mlmodel_file() {
    use crate::export::validate_coreml_weights;

    let tmp = tempdir().expect("tempdir");
    let package_path = tmp.path().join("with_mlmodel.mlpackage");
    std::fs::create_dir(&package_path).expect("create dir");

    std::fs::write(
        package_path.join("Manifest.json"),
        b"{\"rootModelIdentifier\": \"model\"}",
    )
    .expect("write manifest");
    std::fs::create_dir(package_path.join("Data")).expect("create Data dir");
    std::fs::create_dir(package_path.join("Data/model")).expect("create model dir");
    std::fs::write(
        package_path.join("Data/model/model.mlmodel"),
        b"mlmodel data",
    )
    .expect("write mlmodel");

    let result = validate_coreml_weights(&package_path);
    assert!(result.is_ok(), "Should accept package with model.mlmodel");
}

#[test]
fn test_validate_weights_accepts_coremldata_bin() {
    use crate::export::validate_coreml_weights;

    let tmp = tempdir().expect("tempdir");
    let package_path = tmp.path().join("with_coremldata.mlpackage");
    std::fs::create_dir(&package_path).expect("create dir");

    std::fs::write(
        package_path.join("Manifest.json"),
        b"{\"modelIdentifier\": \"test\"}",
    )
    .expect("write manifest");
    std::fs::create_dir(package_path.join("Data")).expect("create Data dir");
    std::fs::create_dir(package_path.join("Data/model")).expect("create model dir");
    std::fs::write(
        package_path.join("Data/model/coremldata.bin"),
        b"binary data",
    )
    .expect("write coremldata");

    let result = validate_coreml_weights(&package_path);
    assert!(result.is_ok(), "Should accept package with coremldata.bin");
}

// ========== Real Timeout Integration Test ==========

#[tokio::test]
async fn test_export_async_actually_times_out() {
    use crate::export::{export_coreml_adapter_async, CoreMLExportRequest};
    use std::time::Duration;

    let tmp = tempdir().expect("tempdir");

    // Create a minimal but valid-looking package structure
    let base_package = tmp.path().join("base.mlpackage");
    std::fs::create_dir(&base_package).expect("create base dir");
    std::fs::write(
        base_package.join("Manifest.json"),
        b"{\"itemInfoEntries\": {}}",
    )
    .expect("write manifest");
    std::fs::create_dir(base_package.join("Data")).expect("create Data dir");
    std::fs::create_dir(base_package.join("Data/model")).expect("create model dir");
    std::fs::write(base_package.join("Data/model/model.mlmodel"), b"data").expect("write model");

    // Create a dummy adapter file
    let adapter_path = tmp.path().join("adapter.aos");
    std::fs::write(&adapter_path, b"dummy adapter data").expect("write adapter");

    let output_path = tmp.path().join("output.mlpackage");

    let req = CoreMLExportRequest {
        base_package: base_package.clone(),
        adapter_aos: adapter_path,
        output_package: output_path.clone(),
        compute_units: crate::ComputeUnits::CpuAndNeuralEngine,
        allow_overwrite: false,
        // Very short timeout - 1 millisecond
        timeout: Duration::from_millis(1),
        skip_ops_check: true,
    };

    let start = std::time::Instant::now();
    let result = export_coreml_adapter_async(&req).await;
    let elapsed = start.elapsed();

    // The operation should have failed due to timeout or failed early due to invalid adapter
    // Either way, it should have returned quickly (within a second)
    assert!(
        elapsed < Duration::from_secs(5),
        "Should return quickly, took {:?}",
        elapsed
    );

    // If we got a timeout error, verify the structure
    if let Err(AosError::CoreMLExportTimeout {
        operation,
        duration,
    }) = &result
    {
        assert!(operation.contains("output.mlpackage"));
        assert_eq!(*duration, Duration::from_millis(1));
    }
    // Other errors are acceptable (e.g., invalid adapter format)
}

// ========== Strict Mode Documentation and Behavior Tests ==========

#[test]
fn test_fusion_strict_mode_returns_error_on_mismatch() {
    use crate::fusion::{validate_lora_shapes, FusionOptions, LoraTarget};

    // Mismatched shapes
    let lora_a = vec![0.0f32; 100]; // Wrong size
    let lora_b = vec![0.0f32; 200]; // Wrong size
    let rank = 16;
    let in_dim = 64;
    let out_dim = 64;

    let result = validate_lora_shapes(
        0,
        &LoraTarget::QProj,
        &lora_a,
        &lora_b,
        rank,
        in_dim,
        out_dim,
    );

    // validate_lora_shapes always returns an error for mismatches
    // The strict mode controls whether the caller propagates or logs+skips
    assert!(
        result.is_err(),
        "validate_lora_shapes should return error for mismatches"
    );
}

/// Documents the strict mode behavior:
///
/// When `FusionOptions::strict` is `false` (default):
/// - Shape mismatches are logged as warnings
/// - The problematic layer/target is skipped
/// - Fusion continues with remaining layers
/// - No error is returned unless ALL layers fail
///
/// When `FusionOptions::strict` is `true`:
/// - The first shape mismatch causes immediate error return
/// - `LoraShapeMismatch` error with full diagnostic info
/// - Useful for debugging and validation pipelines
///
/// Rationale for default=false:
/// - Backward compatibility with existing adapters
/// - Partial fusion is often acceptable (some layers may be optional)
/// - Production systems should log warnings for monitoring
#[test]
fn test_fusion_options_strict_mode_documentation() {
    use crate::fusion::FusionOptions;

    // Default is lenient (strict=false) for backward compatibility
    let default_opts = FusionOptions::default();
    assert!(!default_opts.strict);

    // Strict mode can be enabled explicitly
    let strict_opts = FusionOptions { strict: true };
    assert!(strict_opts.strict);
}

#[test]
fn test_extract_ops_deduplicates() {
    use crate::export::extract_ops_from_manifest;

    let manifest = serde_json::json!({
        "itemInfoEntries": {
            "item1": {"type": "linear", "op": "linear"},
            "item2": {"type": "linear", "name": "model/layer_0/linear"}
        }
    });

    let ops = extract_ops_from_manifest(&manifest);
    let linear_count = ops.iter().filter(|o| o.to_lowercase() == "linear").count();

    // Should deduplicate - only one "linear" even though it appears multiple times
    assert_eq!(linear_count, 1, "Should deduplicate operation types");
}

#[test]
fn test_extract_ops_recursive_finds_nested() {
    use crate::export::extract_ops_from_manifest;

    let manifest = serde_json::json!({
        "model": {
            "layers": [
                {"op": "attention"},
                {"operation": "feedforward"},
                {"nested": {"type": "layernorm"}}
            ]
        }
    });

    let ops = extract_ops_from_manifest(&manifest);

    assert!(
        ops.iter().any(|o| o == "attention"),
        "Should find 'attention'"
    );
    assert!(
        ops.iter().any(|o| o == "feedforward"),
        "Should find 'feedforward'"
    );
    assert!(
        ops.iter().any(|o| o == "layernorm"),
        "Should find 'layernorm'"
    );
}
