//! Example: Using the test data generators
//!
//! This example demonstrates how to use the test_utils module to generate
//! AOS files for testing.
//!
//! Run with: cargo run --example test_data_generation

use adapteros_aos::test_utils::*;
use adapteros_aos::AOS2Writer;
use std::path::PathBuf;

fn main() -> adapteros_core::Result<()> {
    println!("=== AOS Test Data Generator Examples ===\n");

    // Example 1: Generate a simple valid AOS file
    example_1_simple_generation()?;

    // Example 2: Generate with custom configuration
    example_2_custom_config()?;

    // Example 3: Generate corrupted files for error testing
    example_3_corruption_types()?;

    // Example 4: Generate edge cases
    example_4_edge_cases()?;

    // Example 5: Semantic ID generation
    example_5_semantic_ids();

    // Example 6: Safetensors building
    example_6_safetensors()?;

    println!("\n✓ All examples completed successfully!");

    Ok(())
}

fn example_1_simple_generation() -> adapteros_core::Result<()> {
    println!("Example 1: Simple Generation");
    println!("-----------------------------");

    // Generate with default settings
    let data = generate_valid_aos()?;
    println!("Generated AOS file: {} bytes", data.len());

    // Verify structure
    let manifest_offset = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let manifest_len = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;

    println!("  Header: 8 bytes");
    println!("  Weights: {} bytes", manifest_offset - 8);
    println!("  Manifest: {} bytes", manifest_len);

    // Parse manifest
    let manifest_bytes = &data[manifest_offset..manifest_offset + manifest_len];
    let manifest: TestManifest = serde_json::from_slice(manifest_bytes)?;

    println!("  Adapter ID: {}", manifest.adapter_id);
    println!("  Rank: {}", manifest.rank);
    println!("  Base model: {}", manifest.base_model);

    println!();
    Ok(())
}

fn example_2_custom_config() -> adapteros_core::Result<()> {
    println!("Example 2: Custom Configuration");
    println!("--------------------------------");

    let config = GeneratorConfig {
        rank: 4,
        hidden_dim: 256,
        num_tensors: 3,
        seed: Some(42), // Deterministic
        version: ManifestVersion::V2_0,
        base_model: "llama-3-8b".to_string(),
        adapter_id: Some("acme-corp/ml/classifier/r001".to_string()),
        learning_rate: 0.001,
        alpha: 16.0,
        batch_size: 8,
        epochs: 5,
    };

    let mut generator = AosGenerator::new(config);
    let data = generator.generate_valid()?;

    println!("Generated custom AOS: {} bytes", data.len());
    println!("  Rank: 4");
    println!("  Hidden dim: 256");
    println!("  Num tensors: 3");
    println!("  Seed: 42 (deterministic)");

    // Generate to file
    let temp_path = PathBuf::from("/tmp/test_adapter.aos");
    generator.generate_to_file(&temp_path)?;
    println!("  Saved to: {}", temp_path.display());

    // Verify file
    let (offset, len) = AOS2Writer::read_header(&temp_path)?;
    println!("  File header valid: offset={}, len={}", offset, len);

    println!();
    Ok(())
}

fn example_3_corruption_types() -> adapteros_core::Result<()> {
    println!("Example 3: Corruption Types");
    println!("----------------------------");

    let corruption_types = [
        ("Bad Header", CorruptionType::BadHeader),
        ("Bad Manifest", CorruptionType::BadManifest),
        ("Bad Weights", CorruptionType::BadWeights),
        ("Invalid Offset", CorruptionType::InvalidOffset),
        ("Wrong Hash", CorruptionType::WrongHash),
        ("Truncated", CorruptionType::Truncated),
    ];

    for (name, corruption_type) in &corruption_types {
        let data = generate_corrupted_aos(*corruption_type)?;
        println!("  {}: {} bytes", name, data.len());
    }

    println!();
    Ok(())
}

fn example_4_edge_cases() -> adapteros_core::Result<()> {
    println!("Example 4: Edge Cases");
    println!("---------------------");

    let edge_cases = [
        ("Empty Weights", EdgeCaseType::EmptyWeights),
        ("Huge File (5MB)", EdgeCaseType::HugeFile),
        ("Missing Manifest", EdgeCaseType::MissingManifest),
        ("Zero Rank", EdgeCaseType::ZeroRank),
        ("Single Tensor", EdgeCaseType::SingleTensor),
        ("Many Tensors (100)", EdgeCaseType::ManyTensors),
    ];

    for (name, edge_case) in &edge_cases {
        let data = generate_edge_case_aos(*edge_case)?;
        let size_kb = data.len() / 1024;
        if size_kb > 1024 {
            println!(
                "  {}: {:.1} MB",
                name,
                data.len() as f64 / (1024.0 * 1024.0)
            );
        } else {
            println!("  {}: {} KB", name, size_kb);
        }
    }

    println!();
    Ok(())
}

fn example_5_semantic_ids() {
    println!("Example 5: Semantic ID Generation");
    println!("----------------------------------");

    let mut generator = SemanticIdGenerator::new(42);

    println!("Random IDs:");
    for i in 0..5 {
        let id = generator.generate();
        let valid = validate_adapter_id(&id);
        println!("  {}: {} (valid: {})", i + 1, id, valid);
    }

    println!("\nCustom ID:");
    let custom = generator.generate_with(
        Some("acme-corp"),
        Some("engineering"),
        Some("code-review"),
        Some("r042"),
    );
    println!("  {}", custom);

    println!("\nParsing ID:");
    if let Some((tenant, domain, purpose, revision)) = parse_adapter_id(&custom) {
        println!("  Tenant: {}", tenant);
        println!("  Domain: {}", domain);
        println!("  Purpose: {}", purpose);
        println!("  Revision: {}", revision);
    }

    println!();
}

fn example_6_safetensors() -> adapteros_core::Result<()> {
    println!("Example 6: Safetensors Building");
    println!("--------------------------------");

    // Build safetensors with multiple tensors
    let mut builder = SafetensorsBuilder::new();

    // Add lora_A tensor
    let lora_a_data: Vec<f32> = (0..32).map(|i| i as f32 * 0.1).collect();
    builder.add_tensor("lora_A".to_string(), lora_a_data, vec![8, 4]);

    // Add lora_B tensor
    let lora_b_data: Vec<f32> = (0..64).map(|i| (i as f32 * 0.05).sin()).collect();
    builder.add_tensor("lora_B".to_string(), lora_b_data, vec![8, 8]);

    let data = builder.build()?;

    println!("Built safetensors: {} bytes", data.len());

    // Parse header
    let header_size = u64::from_le_bytes([
        data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
    ]);

    println!("  Header size: {} bytes", header_size);
    println!("  Metadata + tensors: {} bytes", data.len() - 8);

    // Parse metadata
    let metadata_end = 8 + header_size as usize;
    let metadata_json = &data[8..metadata_end];
    let metadata: serde_json::Value = serde_json::from_slice(metadata_json)?;

    println!("  Tensors:");
    if let Some(obj) = metadata.as_object() {
        for (name, info) in obj {
            if let Some(shape) = info.get("shape") {
                println!("    {}: shape={}", name, shape);
            }
        }
    }

    // Demonstrate Q15 conversion
    println!("\nQ15 Quantization:");
    let float_values = vec![-1.0, -0.5, 0.0, 0.5, 1.0];
    let q15_values = f32_to_q15(&float_values);
    println!("  Input:  {:?}", float_values);
    println!("  Q15:    {:?}", q15_values);

    println!();
    Ok(())
}
