//! Example usage of .aos single-file adapter format
//!
//! Demonstrates:
//! - Creating signed adapters with Ed25519
//! - Package options for adapter creation
//! - Format versioning and compatibility
//! - Manifest-only loading for fast operations
//! - Migration between format versions

#[cfg(feature = "extended-tests")]
use adapteros_crypto::Keypair;
#[cfg(feature = "extended-tests")]
use adapteros_single_file_adapter::{
    get_compatibility_report, migrate_adapter, AdapterWeights, LineageInfo, Mutation,
    PackageOptions, SingleFileAdapter, SingleFileAdapterLoader, SingleFileAdapterPackager,
    SingleFileAdapterValidator, TrainingConfig, TrainingExample, WeightGroup, WeightGroupConfig,
    WeightGroupType, WeightMetadata, AOS_FORMAT_VERSION,
};
#[cfg(feature = "extended-tests")]
use chrono::Utc;
#[cfg(feature = "extended-tests")]
use std::collections::HashMap;

#[cfg(not(feature = "extended-tests"))]
fn main() {
    eprintln!("Enable the `extended-tests` feature to run the advanced AdapterOS .aos example.");
}

#[cfg(feature = "extended-tests")]
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
    create_adapter_with_options().await?;

    // Example 7: Fast manifest-only loading
    fast_manifest_loading().await?;

    // Example 8: Format version migration
    format_migration().await?;

    Ok(())
}

#[cfg(feature = "extended-tests")]
fn sample_training_data() -> Vec<TrainingExample> {
    vec![
        TrainingExample::new(vec![1, 2, 3], vec![4, 5, 6]),
        TrainingExample::new(vec![7, 8, 9], vec![10, 11, 12]),
    ]
}

#[cfg(feature = "extended-tests")]
fn sample_weights() -> AdapterWeights {
    let created_at = Utc::now().to_rfc3339();
    let positive = WeightGroup {
        lora_a: vec![vec![0.1, 0.2, 0.3, 0.4], vec![0.5, 0.6, 0.7, 0.8]],
        lora_b: vec![
            vec![0.01, 0.02],
            vec![0.03, 0.04],
            vec![0.05, 0.06],
            vec![0.07, 0.08],
        ],
        metadata: WeightMetadata {
            example_count: 100,
            avg_loss: 0.12,
            training_time_ms: 1500,
            group_type: WeightGroupType::Positive,
            created_at: created_at.clone(),
        },
    };

    let negative = WeightGroup {
        lora_a: vec![vec![-0.1, -0.2, -0.3, -0.4], vec![-0.5, -0.6, -0.7, -0.8]],
        lora_b: vec![
            vec![-0.01, -0.02],
            vec![-0.03, -0.04],
            vec![-0.05, -0.06],
            vec![-0.07, -0.08],
        ],
        metadata: WeightMetadata {
            example_count: 50,
            avg_loss: 0.2,
            training_time_ms: 1200,
            group_type: WeightGroupType::Negative,
            created_at,
        },
    };

    AdapterWeights {
        positive,
        negative,
        combined: None,
    }
}

#[cfg(feature = "extended-tests")]
fn sample_config() -> TrainingConfig {
    TrainingConfig {
        rank: 2,
        alpha: 4.0,
        learning_rate: 0.0005,
        batch_size: 8,
        epochs: 4,
        hidden_dim: 4,
        weight_group_config: WeightGroupConfig::default(),
    }
}

#[cfg(feature = "extended-tests")]
fn build_lineage(
    adapter_id: &str,
    version: &str,
    parent_version: Option<String>,
    parent_hash: Option<String>,
    mutations: Vec<Mutation>,
    quality_delta: f32,
) -> LineageInfo {
    LineageInfo {
        adapter_id: adapter_id.to_string(),
        version: version.to_string(),
        parent_version,
        parent_hash,
        mutations,
        quality_delta,
        created_at: Utc::now().to_rfc3339(),
    }
}

#[cfg(feature = "extended-tests")]
fn build_adapter(
    adapter_id: &str,
    version: &str,
    parent_version: Option<String>,
    parent_hash: Option<String>,
    mutations: Vec<Mutation>,
    quality_delta: f32,
) -> Result<SingleFileAdapter, Box<dyn std::error::Error>> {
    let weights = sample_weights();
    let training_data = sample_training_data();
    let config = sample_config();
    let lineage = build_lineage(
        adapter_id,
        version,
        parent_version,
        parent_hash,
        mutations,
        quality_delta,
    );

    let adapter = SingleFileAdapter::create(
        adapter_id.to_string(),
        weights,
        training_data,
        config,
        lineage,
    )?;

    Ok(adapter)
}

#[cfg(feature = "extended-tests")]
async fn create_aos_adapter() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Example 1: Creating .aos adapter ===\n");

    let adapter = build_adapter("example_adapter", "1.0.0", None, None, vec![], 0.0)?;

    let output_path = "example_adapter.aos";
    SingleFileAdapterPackager::save(&adapter, output_path).await?;

    println!("✓ Created adapter: {}", output_path);
    println!("  - Adapter ID: {}", adapter.manifest.adapter_id);
    println!("  - Version: {}", adapter.manifest.version);
    println!("  - Weights: {} groups", 2);
    println!("  - Training examples: {}\n", adapter.training_data.len());

    Ok(())
}

#[cfg(feature = "extended-tests")]
async fn load_and_verify_aos_adapter() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Example 2: Loading and verifying .aos adapter ===\n");

    let path = "example_adapter.aos";

    let adapter = SingleFileAdapterLoader::load(path).await?;

    println!("✓ Loaded adapter: {}", path);
    println!("  - Adapter ID: {}", adapter.manifest.adapter_id);
    println!("  - Version: {}", adapter.manifest.version);

    let is_valid = adapter.verify()?;
    println!(
        "  - Integrity check: {}\n",
        if is_valid { "✓ PASS" } else { "✗ FAIL" }
    );

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

#[cfg(feature = "extended-tests")]
async fn extract_aos_components() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Example 3: Extracting components ===\n");

    let path = "example_adapter.aos";
    let adapter = SingleFileAdapterLoader::load(path).await?;

    let positive_rows = adapter.weights.positive.lora_a.len();
    let positive_cols = adapter
        .weights
        .positive
        .lora_a
        .first()
        .map(|row| row.len())
        .unwrap_or(0);

    println!(
        "✓ Extracted weights: positive {}x{}, negative {}x{}",
        positive_rows,
        positive_cols,
        adapter.weights.negative.lora_a.len(),
        adapter
            .weights
            .negative
            .lora_a
            .first()
            .map(|row| row.len())
            .unwrap_or(0)
    );

    println!(
        "✓ Extracted training data: {} examples",
        adapter.training_data.len()
    );

    let manifest = &adapter.manifest;
    println!("✓ Extracted manifest metadata:");
    println!("  - Adapter ID: {}", manifest.adapter_id);
    println!("  - Rank: {}", manifest.rank);
    println!("  - Alpha: {}", manifest.alpha);
    println!("  - Base model: {}", manifest.base_model);
    println!("  - Category: {}", manifest.category);
    println!("  - Tier: {}\n", manifest.tier);

    Ok(())
}

#[cfg(feature = "extended-tests")]
async fn create_aos_with_lineage() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Example 4: Creating adapter with lineage tracking ===\n");

    let mutations = vec![
        Mutation {
            mutation_type: "add_examples".to_string(),
            examples_added: 50,
            examples_removed: 0,
            quality_impact: 0.05,
            applied_at: Utc::now().to_rfc3339(),
        },
        Mutation {
            mutation_type: "reweight_examples".to_string(),
            examples_added: 0,
            examples_removed: 0,
            quality_impact: 0.02,
            applied_at: Utc::now().to_rfc3339(),
        },
    ];

    let lineage = build_lineage(
        "evolved_adapter",
        "1.1.0",
        Some("1.0.0".to_string()),
        Some("b3:abc123...".to_string()),
        mutations,
        0.07,
    );

    let adapter = SingleFileAdapter::create(
        "evolved_adapter".to_string(),
        sample_weights(),
        sample_training_data(),
        sample_config(),
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

#[cfg(feature = "extended-tests")]
async fn create_signed_adapter() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Example 5: Creating signed adapter ===\n");

    let mut adapter = build_adapter("signed_adapter", "1.0.0", None, None, vec![], 0.0)?;
    let keypair = Keypair::generate();
    adapter.sign(&keypair)?;

    let output_path = "signed_adapter.aos";
    SingleFileAdapterPackager::save_with_options(
        &adapter,
        output_path,
        PackageOptions::with_combined_weights(),
    )
    .await?;

    println!("✓ Created signed adapter: {}", output_path);
    println!("  - Signed: {}", adapter.is_signed());
    if let Some((key_id, timestamp)) = adapter.signature_info() {
        println!("  - Key ID: {}", key_id);
        println!("  - Timestamp: {}", timestamp);
    }

    Ok(())
}

#[cfg(feature = "extended-tests")]
async fn create_adapter_with_options() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Example 6: Creating adapter with package options ===\n");

    let adapter = build_adapter("options_adapter", "1.0.0", None, None, vec![], 0.0)?;
    let output_path = "options_adapter.aos";

    let options = PackageOptions::with_combined_weights();
    SingleFileAdapterPackager::save_with_options(&adapter, output_path, options).await?;

    println!("✓ Created adapter with options: {}", output_path);
    println!("  - Using combined weights: true");

    Ok(())
}

#[cfg(feature = "extended-tests")]
async fn fast_manifest_loading() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Example 7: Fast manifest-only loading ===\n");

    let path = "example_adapter.aos";
    let manifest = SingleFileAdapterLoader::load_manifest_only(path).await?;

    println!("✓ Loaded manifest only from: {}", path);
    println!("  - Adapter ID: {}", manifest.adapter_id);
    println!("  - Version: {}", manifest.version);
    println!("  - Format version: {}\n", manifest.format_version);

    Ok(())
}

#[cfg(feature = "extended-tests")]
async fn format_migration() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Example 8: Format version migration ===\n");

    let adapter = build_adapter("migration_adapter", "1.0.0", None, None, vec![], 0.0)?;
    let result = migrate_adapter(adapter)?;

    println!("✓ Migration report:");
    println!("  - Original version: {}", result.original_version);
    println!("  - New version: {}", result.new_version);
    println!("  - Current format: {}", AOS_FORMAT_VERSION);
    println!("  - Changes applied: {}", result.changes_applied.len());
    for change in result.changes_applied {
        println!("    - {}", change);
    }

    let compatibility = get_compatibility_report(result.new_version);
    println!("\n✓ Compatibility report:");
    println!("  - Compatible: {}", compatibility.is_compatible);
    println!("  - Current version: {}", compatibility.current_version);
    println!("  - File version: {}", compatibility.file_version);
    if !compatibility.warnings.is_empty() {
        println!("  - Warnings:");
        for warning in &compatibility.warnings {
            println!("    - {}", warning);
        }
    }
    if !compatibility.errors.is_empty() {
        println!("  - Errors:");
        for error in &compatibility.errors {
            println!("    - {}", error);
        }
    }

    Ok(())
}
