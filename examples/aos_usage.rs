//! Example usage of .aos single-file adapter format
//!
//! Demonstrates:
//! - Creating signed adapters with Ed25519
//! - Configurable compression levels
//! - Format versioning and compatibility
//! - Manifest-only loading for fast operations
//! - Migration between format versions

use adapteros_crypto::Keypair;
use adapteros_lora_worker::training::{TrainingConfig, TrainingExample};
use adapteros_single_file_adapter::{
    get_compatibility_report, migrate_adapter, CompressionLevel, LineageInfo, LoadOptions,
    Mutation, PackageOptions, SingleFileAdapter, SingleFileAdapterLoader,
    SingleFileAdapterPackager, SingleFileAdapterValidator, AOS_FORMAT_VERSION,
};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Example 1: Create a new .aos adapter
    create_aos_adapter().await?;

    // Example 2: Load and verify an existing .aos adapter
    load_and_verify_aos_adapter().await?;

    // Example 3: Extract components from .aos adapter
    extract_aos_components().await?;

    // Example 4: Create adapter with lineage tracking
    create_aos_with_lineage().await?;

    // Example 5: Create signed adapter
    create_signed_adapter().await?;

    // Example 6: Create adapter with custom compression
    create_compressed_adapter().await?;

    // Example 7: Fast manifest-only loading
    fast_manifest_loading().await?;

    // Example 8: Format version migration
    format_migration().await?;

    Ok(())
}

async fn create_aos_adapter() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Example 1: Creating .aos adapter ===\n");

    // Create sample weights (in production, these would be real trained weights)
    let weights = vec![1u8; 1024]; // 1KB of dummy weights

    // Create sample training data
    let training_data = vec![
        TrainingExample {
            input: vec![1, 2, 3],
            target: vec![4, 5, 6],
            metadata: HashMap::new(),
            weight: 1.0,
        },
        TrainingExample {
            input: vec![7, 8, 9],
            target: vec![10, 11, 12],
            metadata: HashMap::new(),
            weight: 1.0,
        },
    ];

    // Create training configuration
    let config = TrainingConfig {
        rank: 16,
        alpha: 32.0,
        learning_rate: 0.0005,
        batch_size: 8,
        epochs: 4,
        hidden_dim: 3584,
    };

    // Create lineage info
    let lineage = LineageInfo {
        adapter_id: "example_adapter".to_string(),
        version: "1.0.0".to_string(),
        parent_version: None,
        parent_hash: None,
        mutations: vec![],
        quality_delta: 0.0,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    // Create adapter
    let adapter = SingleFileAdapter::create(
        "example_adapter".to_string(),
        weights,
        training_data,
        config,
        lineage,
    )?;

    // Save to .aos file
    let output_path = "example_adapter.aos";
    SingleFileAdapterPackager::save(&adapter, output_path).await?;

    println!("✓ Created adapter: {}", output_path);
    println!("  - Adapter ID: {}", adapter.manifest.adapter_id);
    println!("  - Version: {}", adapter.manifest.version);
    println!("  - Weights: {} bytes", adapter.weights.len());
    println!("  - Training examples: {}\n", adapter.training_data.len());

    Ok(())
}

async fn load_and_verify_aos_adapter() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Example 2: Loading and verifying .aos adapter ===\n");

    let path = "example_adapter.aos";

    // Load adapter
    let adapter = SingleFileAdapterLoader::load(path).await?;

    println!("✓ Loaded adapter: {}", path);
    println!("  - Adapter ID: {}", adapter.manifest.adapter_id);
    println!("  - Version: {}", adapter.manifest.version);

    // Verify integrity
    let is_valid = adapter.verify()?;
    println!(
        "  - Integrity check: {}\n",
        if is_valid { "✓ PASS" } else { "✗ FAIL" }
    );

    // Validate using validator
    let validation_result = SingleFileAdapterValidator::validate(path).await?;

    println!("✓ Validation result:");
    println!("  - Valid: {}", validation_result.is_valid);
    println!("  - Errors: {}", validation_result.errors.len());
    println!("  - Warnings: {}\n", validation_result.warnings.len());

    for warning in &validation_result.warnings {
        println!("  ⚠ Warning: {}", warning);
    }

    Ok(())
}

async fn extract_aos_components() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Example 3: Extracting components ===\n");

    let path = "example_adapter.aos";
    let adapter = SingleFileAdapterLoader::load(path).await?;

    // Extract weights
    let weights = adapter.extract_weights();
    println!("✓ Extracted weights: {} bytes", weights.len());

    // Extract training data
    let training_data = adapter.extract_training_data();
    println!(
        "✓ Extracted training data: {} examples",
        training_data.len()
    );

    // Extract metadata
    let metadata = adapter.get_metadata();
    println!("✓ Extracted metadata:");
    println!("  - Adapter ID: {}", metadata.adapter_id);
    println!("  - Rank: {}", metadata.rank);
    println!("  - Alpha: {}", metadata.alpha);
    println!("  - Base model: {}", metadata.base_model);
    println!("  - Category: {}", metadata.category);
    println!("  - Tier: {}\n", metadata.tier);

    Ok(())
}

async fn create_aos_with_lineage() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Example 4: Creating adapter with lineage tracking ===\n");

    // Simulate adapter evolution
    let weights = vec![1u8; 1024];
    let training_data = vec![TrainingExample {
        input: vec![1, 2, 3],
        target: vec![4, 5, 6],
        metadata: HashMap::new(),
        weight: 1.0,
    }];
    let config = TrainingConfig {
        rank: 16,
        alpha: 32.0,
        learning_rate: 0.0005,
        batch_size: 8,
        epochs: 4,
        hidden_dim: 3584,
    };

    // Create lineage with mutation history
    let lineage = LineageInfo {
        adapter_id: "evolved_adapter".to_string(),
        version: "1.1.0".to_string(),
        parent_version: Some("1.0.0".to_string()),
        parent_hash: Some("b3:abc123...".to_string()),
        mutations: vec![
            Mutation {
                mutation_type: "add_examples".to_string(),
                examples_added: 50,
                examples_removed: 0,
                quality_impact: 0.05,
                applied_at: chrono::Utc::now().to_rfc3339(),
            },
            Mutation {
                mutation_type: "reweight_examples".to_string(),
                examples_added: 0,
                examples_removed: 0,
                quality_impact: 0.02,
                applied_at: chrono::Utc::now().to_rfc3339(),
            },
        ],
        quality_delta: 0.07,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    let adapter = SingleFileAdapter::create(
        "evolved_adapter".to_string(),
        weights,
        training_data,
        config,
        lineage.clone(),
    )?;

    let output_path = "evolved_adapter.aos";
    SingleFileAdapterPackager::save(&adapter, output_path).await?;

    println!("✓ Created evolved adapter: {}", output_path);
    println!("  - Version: {}", lineage.version);
    println!("  - Parent version: {:?}", lineage.parent_version);
    println!("  - Mutations: {}", lineage.mutations.len());
    println!("  - Quality delta: {}\n", lineage.quality_delta);

    for (i, mutation) in lineage.mutations.iter().enumerate() {
        println!("  Mutation {}:", i + 1);
        println!("    - Type: {}", mutation.mutation_type);
        println!("    - Added: {} examples", mutation.examples_added);
        println!("    - Removed: {} examples", mutation.examples_removed);
        println!("    - Quality impact: {}", mutation.quality_impact);
    }

    Ok(())
}
