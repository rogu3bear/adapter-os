//! Integration tests for test_utils module
//!
//! Demonstrates how to use the test data generators in actual tests.

use adapteros_aos::test_utils::*;
use adapteros_aos::AOS2Writer;
use adapteros_core::Result;
use tempfile::{NamedTempFile, TempDir};

#[test]
fn test_generate_valid_aos_default() -> Result<()> {
    let data = generate_valid_aos()?;

    // Verify basic structure
    assert!(data.len() > 8, "Should have header");

    let manifest_offset = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let manifest_len = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;

    assert!(
        manifest_offset > 8,
        "Manifest should be after header and weights"
    );
    assert!(manifest_len > 0, "Manifest should have content");
    assert!(
        manifest_offset + manifest_len <= data.len(),
        "Manifest should fit in file"
    );

    Ok(())
}

#[test]
fn test_generate_with_custom_config() -> Result<()> {
    let config = GeneratorConfig {
        rank: 4,
        hidden_dim: 256,
        num_tensors: 3,
        seed: Some(12345),
        version: ManifestVersion::V2_0,
        base_model: "custom-model".to_string(),
        adapter_id: Some("test/domain/purpose/r001".to_string()),
        learning_rate: 0.001,
        alpha: 32.0,
        batch_size: 8,
        epochs: 5,
    };

    let mut generator = AosGenerator::new(config);
    let data = generator.generate_valid()?;

    assert!(data.len() > 8);

    // Parse and verify manifest
    let manifest_offset = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let manifest_len = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;

    let manifest_bytes = &data[manifest_offset..manifest_offset + manifest_len];
    let manifest: TestManifest = serde_json::from_slice(manifest_bytes)?;

    assert_eq!(manifest.rank, 4);
    assert_eq!(manifest.base_model, "custom-model");
    assert_eq!(manifest.adapter_id, "test/domain/purpose/r001");
    assert_eq!(manifest.training_config.rank, 4);
    assert_eq!(manifest.training_config.hidden_dim, 256);
    assert_eq!(manifest.training_config.batch_size, 8);
    assert_eq!(manifest.training_config.epochs, 5);

    Ok(())
}

#[test]
fn test_generate_to_file() -> Result<()> {
    let temp_file = NamedTempFile::new()
        .map_err(|e| adapteros_core::AosError::Io(format!("Failed to create temp: {}", e)))?;

    let mut generator = AosGenerator::new(GeneratorConfig::default());
    generator.generate_to_file(temp_file.path())?;

    // Verify file exists and is readable
    assert!(temp_file.path().exists());

    let (offset, len) = AOS2Writer::read_header(temp_file.path())?;
    assert!(offset > 8);
    assert!(len > 0);

    Ok(())
}

#[test]
fn test_all_corruption_types() -> Result<()> {
    let corruption_types = [
        CorruptionType::BadHeader,
        CorruptionType::BadManifest,
        CorruptionType::BadWeights,
        CorruptionType::InvalidOffset,
        CorruptionType::WrongHash,
        CorruptionType::Truncated,
    ];

    for corruption_type in &corruption_types {
        let data = generate_corrupted_aos(*corruption_type)?;
        assert!(
            !data.is_empty(),
            "Corrupted data should not be empty for {:?}",
            corruption_type
        );

        // Some corruption types should make header reading fail
        match corruption_type {
            CorruptionType::BadHeader | CorruptionType::Truncated => {
                // These might fail header parsing
            }
            _ => {
                // Others should have valid header but corrupted content
                assert!(data.len() >= 8, "Should have header bytes");
            }
        }
    }

    Ok(())
}

#[test]
fn test_all_edge_cases() -> Result<()> {
    let edge_cases = [
        EdgeCaseType::EmptyWeights,
        EdgeCaseType::HugeFile,
        EdgeCaseType::MissingManifest,
        EdgeCaseType::ZeroRank,
        EdgeCaseType::SingleTensor,
        EdgeCaseType::ManyTensors,
    ];

    for edge_case in &edge_cases {
        let data = generate_edge_case_aos(*edge_case)?;
        assert!(
            !data.is_empty(),
            "Edge case data should not be empty for {:?}",
            edge_case
        );

        match edge_case {
            EdgeCaseType::HugeFile => {
                // Should be large
                assert!(
                    data.len() > 1024 * 1024,
                    "Huge file should be > 1MB: {} bytes",
                    data.len()
                );
            }
            EdgeCaseType::EmptyWeights => {
                // Manifest should start at byte 8
                let offset = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                assert_eq!(offset, 8, "Empty weights means manifest at byte 8");
            }
            _ => {
                // Others should have at least header
                assert!(data.len() >= 8);
            }
        }
    }

    Ok(())
}

#[test]
fn test_deterministic_generation_with_seed() -> Result<()> {
    let config1 = GeneratorConfig {
        seed: Some(42),
        rank: 8,
        hidden_dim: 512,
        ..Default::default()
    };

    let config2 = GeneratorConfig {
        seed: Some(42),
        rank: 8,
        hidden_dim: 512,
        ..Default::default()
    };

    let mut gen1 = AosGenerator::new(config1);
    let mut gen2 = AosGenerator::new(config2);

    let data1 = gen1.generate_valid()?;
    let data2 = gen2.generate_valid()?;

    // Should produce identical output with same seed
    assert_eq!(
        data1.len(),
        data2.len(),
        "Same seed should produce same size"
    );

    // Weights section should be identical
    let offset1 = u32::from_le_bytes([data1[0], data1[1], data1[2], data1[3]]) as usize;
    let offset2 = u32::from_le_bytes([data2[0], data2[1], data2[2], data2[3]]) as usize;

    assert_eq!(offset1, offset2, "Offsets should match");

    let weights1 = &data1[8..offset1];
    let weights2 = &data2[8..offset2];

    assert_eq!(
        weights1, weights2,
        "Weights should be identical with same seed"
    );

    Ok(())
}

#[test]
fn test_safetensors_builder() -> Result<()> {
    let mut builder = SafetensorsBuilder::new();

    builder.add_tensor("lora_A".to_string(), vec![1.0, 2.0, 3.0], vec![3, 1]);
    builder.add_tensor("lora_B".to_string(), vec![4.0, 5.0, 6.0], vec![3, 1]);

    let data = builder.build()?;

    // Verify it has safetensors structure
    assert!(data.len() > 8, "Should have header");

    let header_size = u64::from_le_bytes([
        data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
    ]);

    assert!(header_size > 0, "Should have metadata");

    // Parse metadata
    let metadata_end = 8 + header_size as usize;
    let metadata_json = &data[8..metadata_end];
    let metadata: serde_json::Value = serde_json::from_slice(metadata_json)?;

    assert!(metadata.is_object());
    assert!(metadata["lora_A"].is_object());
    assert!(metadata["lora_B"].is_object());

    Ok(())
}

#[test]
fn test_semantic_id_generator() {
    let mut generator = SemanticIdGenerator::new(42);

    for _ in 0..10 {
        let id = generator.generate();
        assert!(validate_adapter_id(&id), "Should generate valid ID: {}", id);

        let parts: Vec<&str> = id.split('/').collect();
        assert_eq!(parts.len(), 4, "Should have 4 parts");
        assert!(!parts[0].is_empty(), "Tenant should not be empty");
        assert!(!parts[1].is_empty(), "Domain should not be empty");
        assert!(!parts[2].is_empty(), "Purpose should not be empty");
        assert!(parts[3].starts_with('r'), "Revision should start with 'r'");
    }
}

#[test]
fn test_semantic_id_validation() {
    // Valid IDs
    assert!(validate_adapter_id("tenant/domain/purpose/r001"));
    assert!(validate_adapter_id("a/b/c/r999"));
    assert!(validate_adapter_id("test-org/ml/classifier/r001"));

    // Invalid IDs
    assert!(!validate_adapter_id("tenant/domain/purpose")); // Only 3 parts
    assert!(!validate_adapter_id("tenant/domain/purpose/r001/extra")); // 5 parts
    assert!(!validate_adapter_id("")); // Empty
    assert!(!validate_adapter_id("a/b//r001")); // Empty part
    assert!(!validate_adapter_id("tenant/domain/purpose/r001/"));
}

#[test]
fn test_parse_adapter_id() {
    let id = "tenant-a/engineering/code-review/r001";
    let parsed = parse_adapter_id(id);

    assert!(parsed.is_some());
    let (tenant, domain, purpose, revision) = parsed.unwrap();

    assert_eq!(tenant, "tenant-a");
    assert_eq!(domain, "engineering");
    assert_eq!(purpose, "code-review");
    assert_eq!(revision, "r001");

    // Invalid ID
    let invalid = parse_adapter_id("invalid/id");
    assert!(invalid.is_none());
}

#[test]
fn test_q15_conversion() {
    let values = vec![-1.0, -0.5, 0.0, 0.5, 1.0];
    let q15 = f32_to_q15(&values);

    assert_eq!(q15.len(), 5);
    assert_eq!(q15[0], -32767); // -1.0
    assert!(q15[1] < 0 && q15[1] > -32767); // -0.5
    assert_eq!(q15[2], 0); // 0.0
    assert!(q15[3] > 0 && q15[3] < 32767); // 0.5
    assert_eq!(q15[4], 32767); // 1.0
}

#[test]
fn test_multiple_file_generation() -> Result<()> {
    let temp_dir = TempDir::new()
        .map_err(|e| adapteros_core::AosError::Io(format!("Failed to create temp dir: {}", e)))?;

    let configs = vec![
        GeneratorConfig {
            rank: 4,
            hidden_dim: 256,
            seed: Some(1),
            ..Default::default()
        },
        GeneratorConfig {
            rank: 8,
            hidden_dim: 512,
            seed: Some(2),
            ..Default::default()
        },
        GeneratorConfig {
            rank: 16,
            hidden_dim: 1024,
            seed: Some(3),
            ..Default::default()
        },
    ];

    for (i, config) in configs.iter().enumerate() {
        let mut generator = AosGenerator::new(config.clone());
        let path = temp_dir.path().join(format!("adapter_{}.aos", i));
        generator.generate_to_file(&path)?;

        assert!(path.exists());

        let (offset, len) = AOS2Writer::read_header(&path)?;
        assert!(offset > 8);
        assert!(len > 0);
    }

    Ok(())
}

#[test]
fn test_generate_with_params_helper() -> Result<()> {
    let data = generate_valid_aos_with_params(4, 256)?;
    assert!(data.len() > 8);

    // Verify it's a valid AOS file
    let manifest_offset = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let manifest_len = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;

    assert!(manifest_offset > 8);
    assert!(manifest_len > 0);
    assert!(manifest_offset + manifest_len <= data.len());

    // Parse manifest
    let manifest_bytes = &data[manifest_offset..manifest_offset + manifest_len];
    let manifest: TestManifest = serde_json::from_slice(manifest_bytes)?;

    assert_eq!(manifest.rank, 4);
    assert_eq!(manifest.training_config.hidden_dim, 256);

    Ok(())
}

#[test]
fn test_empty_and_minimal_safetensors() -> Result<()> {
    // Empty safetensors
    let empty = SafetensorsBuilder::build_empty()?;
    assert!(empty.len() >= 8);

    let header_size = u64::from_le_bytes([
        empty[0], empty[1], empty[2], empty[3], empty[4], empty[5], empty[6], empty[7],
    ]);
    assert!(header_size >= 2, "Should have at least {{}}");

    // Minimal safetensors
    let minimal = SafetensorsBuilder::build_minimal()?;
    assert!(minimal.len() > empty.len());

    Ok(())
}
