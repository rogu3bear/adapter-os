//! Integration tests for adapteros-aos
//!
//! These tests require the `mmap` feature to be enabled.

#![cfg(feature = "mmap")]

use adapteros_aos::{AosLoader, AosManager, AosManifest, AosWriter, BackendTag, MoEConfigManifest};
use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::tempdir;

fn test_adapter_path() -> Option<PathBuf> {
    let adapters_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()?
        .parent()?
        .join("adapters");

    if !adapters_dir.exists() {
        return None;
    }

    std::fs::read_dir(adapters_dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .find(|entry| entry.path().extension().and_then(|s| s.to_str()) == Some("aos"))
        .map(|entry| entry.path())
}

#[tokio::test]
async fn test_load_adapter() {
    let Some(adapter_path) = test_adapter_path() else {
        eprintln!("No .aos files found, skipping test");
        return;
    };

    let loader = AosLoader::new().expect("Failed to create AosLoader");
    let result = loader.load_from_path(&adapter_path).await;

    match result {
        Ok(adapter) => {
            println!("Loaded: {}", adapter.adapter_id());
            println!("Version: {}", adapter.version());
            println!("Size: {} bytes", adapter.size_bytes());
            println!("Tensors: {}", adapter.tensor_count());
            assert!(!adapter.adapter_id().is_empty());
        }
        Err(e) => {
            eprintln!("Failed to load: {}", e);
        }
    }
}

#[tokio::test]
async fn test_manager_with_cache() {
    let Some(adapter_path) = test_adapter_path() else {
        eprintln!("No .aos files found, skipping test");
        return;
    };

    let manager = AosManager::builder()
        .with_cache(1024 * 1024 * 1024)
        .build()
        .unwrap();

    let result1 = manager.load(&adapter_path).await;

    if let Ok(_adapter1) = result1 {
        let result2 = manager.load(&adapter_path).await;
        assert!(result2.is_ok());

        if let Some(cache) = manager.cache() {
            assert_eq!(cache.len(), 1);
            assert!(cache.metrics().hits() > 0);
        }
    }
}

#[tokio::test]
async fn test_hot_swap() {
    let Some(adapter_path) = test_adapter_path() else {
        eprintln!("No .aos files found, skipping test");
        return;
    };

    let manager = AosManager::builder().with_hot_swap().build().unwrap();

    let result = manager.preload("slot1", &adapter_path).await;

    if result.is_ok() {
        let swap_result = manager.commit_swap(&["slot1".to_string()]);
        assert!(swap_result.is_ok());

        if let Some(hot_swap) = manager.hot_swap_manager() {
            let active_slots = hot_swap.active_slots();
            assert!(active_slots.contains(&"slot1".to_string()));
        }
    }
}

/// Create a minimal safetensors payload with dummy LoRA weights
fn create_dummy_lora_safetensors() -> Vec<u8> {
    use std::io::Write;

    // Create minimal safetensors format:
    // 8 bytes: header_size (u64 little-endian)
    // header_size bytes: JSON header
    // remaining: tensor data

    // Dummy LoRA weights: rank=4, in=16, out=16
    let lora_a: Vec<f32> = (0..64).map(|i| (i as f32) * 0.01).collect(); // 4x16
    let lora_b: Vec<f32> = (0..64).map(|i| (i as f32) * -0.01).collect(); // 16x4

    let header = serde_json::json!({
        "q_proj.lora_A": {
            "dtype": "F32",
            "shape": [4, 16],
            "data_offsets": [0, 256]
        },
        "q_proj.lora_B": {
            "dtype": "F32",
            "shape": [16, 4],
            "data_offsets": [256, 512]
        }
    });

    let header_json = serde_json::to_string(&header).unwrap();
    let header_bytes = header_json.as_bytes();
    let header_size = header_bytes.len() as u64;

    let mut buffer = Vec::new();
    buffer.write_all(&header_size.to_le_bytes()).unwrap();
    buffer.write_all(header_bytes).unwrap();

    // Write tensor data
    for val in &lora_a {
        buffer.write_all(&val.to_le_bytes()).unwrap();
    }
    for val in &lora_b {
        buffer.write_all(&val.to_le_bytes()).unwrap();
    }

    buffer
}

#[tokio::test]
async fn test_moe_adapter_creation_and_loading() {
    let temp = tempdir().expect("Failed to create temp dir");
    let aos_path = temp.path().join("test_moe_adapter.aos");

    // Create manifest with MoE config
    let mut metadata = HashMap::new();
    metadata.insert("scope_path".to_string(), "test/moe/qwen3".to_string());

    let manifest = AosManifest {
        adapter_id: "test/moe/qwen3-30b/v1".to_string(),
        name: Some("Test MoE Adapter".to_string()),
        version: "1.0.0".to_string(),
        rank: 4,
        alpha: 8.0,
        base_model: "Qwen/Qwen3-Coder-30B-A3B".to_string(),
        target_modules: vec!["q_proj".to_string(), "v_proj".to_string()],
        category: Some("code".to_string()),
        tier: Some("ephemeral".to_string()),
        created_at: Some("2024-01-01T00:00:00Z".to_string()),
        weights_hash: None,
        per_layer_hashes: None,
        training_config: None,
        moe_config: Some(MoEConfigManifest {
            num_experts: 128,
            num_experts_per_token: 8,
            num_shared_experts: Some(0),
            moe_intermediate_size: Some(768),
            lora_strategy: "routing_weighted_shared".to_string(),
            use_routing_weights: true,
        }),
        metadata,
    };

    // Create .aos file
    let mut writer = AosWriter::new();
    let weights = create_dummy_lora_safetensors();
    writer
        .add_segment(
            BackendTag::Canonical,
            Some("test/moe/qwen3".to_string()),
            &weights,
        )
        .expect("Failed to add segment");

    writer
        .write_archive(&aos_path, &manifest)
        .expect("Failed to write .aos archive");

    assert!(aos_path.exists(), "AOS file should exist");

    // Load and validate as MoE adapter
    let manager = AosManager::builder()
        .with_cache(1024 * 1024 * 100) // 100MB cache
        .with_hot_swap()
        .build()
        .expect("Failed to create manager");

    // Test load_moe (validates adapter has MoE config)
    let adapter = manager
        .load_moe(&aos_path)
        .await
        .expect("Failed to load MoE adapter");

    assert!(adapter.is_moe_adapter(), "Adapter should be MoE");
    assert_eq!(adapter.adapter_id(), "test/moe/qwen3-30b/v1");

    let moe_config = adapter.moe_config().expect("Should have MoE config");
    assert_eq!(moe_config.num_experts, 128);
    assert_eq!(moe_config.num_experts_per_token, 8);
    assert!(moe_config.use_routing_weights);

    println!("✓ MoE adapter loaded successfully");
    println!("  Adapter ID: {}", adapter.adapter_id());
    println!("  Experts: {}", moe_config.num_experts);
    println!("  Experts/token: {}", moe_config.num_experts_per_token);
    println!("  Strategy: {}", moe_config.lora_strategy);
}

#[tokio::test]
async fn test_moe_adapter_validation_mismatch() {
    let temp = tempdir().expect("Failed to create temp dir");
    let aos_path = temp.path().join("test_moe_mismatch.aos");

    // Create manifest with MoE config (128 experts)
    let mut metadata = HashMap::new();
    metadata.insert("scope_path".to_string(), "test/moe/mismatch".to_string());

    let manifest = AosManifest {
        adapter_id: "test/moe/mismatch/v1".to_string(),
        name: None,
        version: "1.0.0".to_string(),
        rank: 4,
        alpha: 8.0,
        base_model: "Qwen/Qwen3-Coder-30B-A3B".to_string(),
        target_modules: vec!["q_proj".to_string()],
        category: None,
        tier: None,
        created_at: None,
        weights_hash: None,
        per_layer_hashes: None,
        training_config: None,
        moe_config: Some(MoEConfigManifest {
            num_experts: 128, // Adapter has 128 experts
            num_experts_per_token: 8,
            num_shared_experts: None,
            moe_intermediate_size: None,
            lora_strategy: "routing_weighted_shared".to_string(),
            use_routing_weights: true,
        }),
        metadata,
    };

    let mut writer = AosWriter::new();
    let weights = create_dummy_lora_safetensors();
    writer
        .add_segment(
            BackendTag::Canonical,
            Some("test/moe/mismatch".to_string()),
            &weights,
        )
        .unwrap();
    writer.write_archive(&aos_path, &manifest).unwrap();

    let manager = AosManager::builder().build().unwrap();

    // Try to load with mismatched config (64 experts instead of 128)
    let expected_config = MoEConfigManifest {
        num_experts: 64, // Mismatch! Adapter has 128
        num_experts_per_token: 8,
        num_shared_experts: None,
        moe_intermediate_size: None,
        lora_strategy: "routing_weighted_shared".to_string(),
        use_routing_weights: true,
    };

    let result = manager
        .load_moe_validated(&aos_path, &expected_config)
        .await;

    assert!(
        result.is_err(),
        "Should fail validation due to expert count mismatch"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("expert count mismatch") || err.contains("128") && err.contains("64"),
        "Error should mention expert count mismatch: {}",
        err
    );

    println!("✓ MoE validation correctly rejected mismatched config");
}

#[tokio::test]
async fn test_dense_adapter_rejected_as_moe() {
    let temp = tempdir().expect("Failed to create temp dir");
    let aos_path = temp.path().join("test_dense.aos");

    // Create manifest WITHOUT MoE config (dense model adapter)
    let mut metadata = HashMap::new();
    metadata.insert("scope_path".to_string(), "test/dense/llama".to_string());

    let manifest = AosManifest {
        adapter_id: "test/dense/llama-8b/v1".to_string(),
        name: None,
        version: "1.0.0".to_string(),
        rank: 8,
        alpha: 16.0,
        base_model: "meta-llama/Llama-3.1-8B".to_string(),
        target_modules: vec!["q_proj".to_string(), "v_proj".to_string()],
        category: None,
        tier: None,
        created_at: None,
        weights_hash: None,
        per_layer_hashes: None,
        training_config: None,
        moe_config: None, // No MoE config - this is a dense model adapter
        metadata,
    };

    let mut writer = AosWriter::new();
    let weights = create_dummy_lora_safetensors();
    writer
        .add_segment(
            BackendTag::Canonical,
            Some("test/dense/llama".to_string()),
            &weights,
        )
        .unwrap();
    writer.write_archive(&aos_path, &manifest).unwrap();

    let manager = AosManager::builder().build().unwrap();

    // Try to load as MoE adapter - should fail
    let result = manager.load_moe(&aos_path).await;

    assert!(
        result.is_err(),
        "Dense adapter should be rejected by load_moe"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("not configured for MoE"),
        "Error should indicate adapter is not MoE: {}",
        err
    );

    // But regular load should work fine
    let adapter = manager
        .load(&aos_path)
        .await
        .expect("Regular load should work");
    assert!(!adapter.is_moe_adapter(), "Should not be MoE adapter");

    println!("✓ Dense adapter correctly rejected by load_moe, accepted by load");
}
