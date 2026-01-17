//! AOS adapter commands
//!
//! This module provides CLI commands for working with .aos adapter files.
//! Uses adapteros-aos v3.0 types (see `crates/adapteros-aos/src/implementation.rs`).

// ============================================================================
// AOS COORDINATION HEADER
// ============================================================================
// File: crates/adapteros-cli/src/commands/aos.rs
// Phase: 2 - System Integration
// Assigned: Intern B (CLI Commands Team)
// Status: STUBBED - Implementation pending
// Dependencies: SingleFileAdapter, Database, Lifecycle Management
// Last Updated: 2024-01-15
//
// COORDINATION NOTES:
// - This file affects: CLI interface, user workflows, automation
// - Changes require: Updates to SingleFileAdapter and Database schemas
// - Testing needed: CLI integration tests and E2E workflows
// - UI Impact: CLI commands may be called from UI components
// - Lifecycle Impact: Load/verify commands affect adapter lifecycle
// ============================================================================

use super::aos_impl;
use crate::output::OutputWriter;
use adapteros_aos::{AosWriter, BackendTag};
use adapteros_core::{AosError, Result};
use adapteros_crypto::Keypair;
use adapteros_single_file_adapter::{
    migrate_file, LineageInfo, LoadOptions, SingleFileAdapter, SingleFileAdapterLoader,
    SingleFileAdapterValidator, TrainingConfig, WeightGroup,
};
use chrono::Utc;
use safetensors::tensor::TensorView;

use clap::{Parser, Subcommand};
use serde::Serialize;
use std::fs;
use std::path::PathBuf;

/// Serialize WeightGroup to safetensors binary format
fn serialize_weights_to_safetensors(weights: &WeightGroup) -> Result<Vec<u8>> {
    // Flatten lora_a: Vec<Vec<f32>> -> Vec<f32> -> bytes
    let lora_a_flat: Vec<f32> = weights
        .lora_a
        .iter()
        .flat_map(|row| row.iter().copied())
        .collect();
    let lora_a_bytes: Vec<u8> = lora_a_flat.iter().flat_map(|f| f.to_le_bytes()).collect();

    // Flatten lora_b: Vec<Vec<f32>> -> Vec<f32> -> bytes
    let lora_b_flat: Vec<f32> = weights
        .lora_b
        .iter()
        .flat_map(|row| row.iter().copied())
        .collect();
    let lora_b_bytes: Vec<u8> = lora_b_flat.iter().flat_map(|f| f.to_le_bytes()).collect();

    // Get shapes
    let rank = weights.lora_a.len();
    let hidden_dim = weights.lora_a.first().map(|r| r.len()).unwrap_or(0);

    // Create tensor views
    let lora_a_view = TensorView::new(
        safetensors::Dtype::F32,
        vec![rank, hidden_dim],
        &lora_a_bytes,
    )
    .map_err(|e| AosError::Training(format!("Failed to create lora_a tensor: {}", e)))?;

    let lora_b_view = TensorView::new(
        safetensors::Dtype::F32,
        vec![hidden_dim, rank],
        &lora_b_bytes,
    )
    .map_err(|e| AosError::Training(format!("Failed to create lora_b tensor: {}", e)))?;

    // Serialize to safetensors format
    let tensors = vec![("lora_a", lora_a_view), ("lora_b", lora_b_view)];

    safetensors::tensor::serialize(tensors, &None)
        .map_err(|e| AosError::Training(format!("Failed to serialize weights: {}", e)))
}

#[derive(Debug, Parser, Clone)]
#[command(name = "aos")]
pub struct AosArgs {
    #[command(subcommand)]
    pub cmd: AosCmd,
}

#[derive(Debug, Subcommand, Clone)]
pub enum AosCmd {
    /// Create .aos file from existing adapter
    Create(CreateArgs), // COORDINATION: Affects AosWriter
    /// Load .aos file into registry
    Load(LoadArgs), // COORDINATION: Affects Database and Lifecycle Management
    /// Verify .aos file integrity
    Verify(VerifyArgs), // COORDINATION: Affects SingleFileAdapterValidator
    /// Extract components from .aos file
    Extract(ExtractArgs), // COORDINATION: Affects SingleFileAdapterLoader
    /// Show .aos file information
    Info(InfoArgs), // COORDINATION: Affects UI display components
    /// Migrate .aos file to current format version
    Migrate(MigrateArgs), // COORDINATION: Affects format version compatibility
}

#[derive(Debug, Parser, Clone)]
pub struct CreateArgs {
    /// Source adapter directory or weights file
    #[arg(long)]
    pub source: PathBuf,

    /// Output .aos file path
    #[arg(long)]
    pub output: PathBuf,

    /// Training data JSONL file (embedded in manifest metadata, not stored separately)
    #[arg(long)]
    pub training_data: Option<PathBuf>,

    /// Adapter ID
    #[arg(long)]
    pub adapter_id: String,

    /// Adapter version
    #[arg(long, default_value = "1.0.0")]
    pub version: String,

    /// Training configuration TOML file
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Sign the .aos file with Ed25519
    #[arg(long)]
    pub sign: bool,

    /// Hex-encoded signing key (generates new key if not provided)
    #[arg(long)]
    pub signing_key: Option<String>,
}

#[derive(Debug, Parser, Clone)]
pub struct LoadArgs {
    /// Path to .aos file
    #[arg(long)]
    pub path: PathBuf,

    /// Adapter ID for registry
    #[arg(long)]
    pub adapter_id: Option<String>,

    /// Control plane base URL
    #[arg(long, default_value = "http://127.0.0.1:8080/api")]
    pub base_url: String,
}

#[derive(Debug, Parser, Clone)]
pub struct VerifyArgs {
    /// Path to .aos file
    #[arg(long)]
    pub path: PathBuf,

    /// Output format (text, json)
    #[arg(long, default_value = "text")]
    pub format: String,
}

#[derive(Debug, Parser, Clone)]
pub struct ExtractArgs {
    /// Path to .aos file
    #[arg(long)]
    pub path: PathBuf,

    /// Output directory
    #[arg(long)]
    pub output_dir: PathBuf,

    /// Components to extract (weights, training_data, config, lineage, manifest, signature, all)
    #[arg(long, default_value = "all")]
    pub components: String,
}

#[derive(Debug, Parser, Clone)]
pub struct InfoArgs {
    /// Path to .aos file
    #[arg(long)]
    pub path: PathBuf,

    /// Output format (text, json)
    #[arg(long, default_value = "text")]
    pub format: String,
}

#[derive(Debug, Parser, Clone)]
pub struct MigrateArgs {
    /// Path to .aos file to migrate
    #[arg(long)]
    pub path: PathBuf,

    /// Create backup before migrating
    #[arg(long, default_value = "true")]
    pub backup: bool,
}

pub async fn run(args: AosArgs, output: &OutputWriter) -> Result<()> {
    match args.cmd {
        AosCmd::Create(create) => create_aos(create, output).await,
        AosCmd::Load(load) => load_aos(load, output).await,
        AosCmd::Verify(verify) => verify_aos(verify, output).await,
        AosCmd::Extract(extract) => extract_aos(extract, output).await,
        AosCmd::Info(info) => info_aos(info, output).await,
        AosCmd::Migrate(migrate) => migrate_aos(migrate, output).await,
    }
}

pub async fn create_aos(args: CreateArgs, output: &OutputWriter) -> Result<()> {
    output.info(format!(
        "Creating .aos file from: {}",
        args.source.display()
    ));

    // 1. Load weights from source path (directory or file)
    output.info("Loading weights...");
    let weights = aos_impl::load_weights_from_source(&args.source)?;
    output.success("Weights loaded successfully");

    // 2. Load training data from JSONL if provided (stored in manifest metadata)
    let training_data = if let Some(ref training_data_path) = args.training_data {
        output.info(format!(
            "Loading training data from: {}",
            training_data_path.display()
        ));
        let data = aos_impl::load_training_data(training_data_path)?;
        output.success(format!("Loaded {} training examples", data.len()));
        data
    } else {
        output.info("No training data provided");
        Vec::new()
    };

    // 3. Load config from TOML if provided
    let config = if let Some(ref config_path) = args.config {
        output.info(format!("Loading config from: {}", config_path.display()));
        aos_impl::load_config(config_path)?
    } else {
        output.info("No config provided, using defaults");
        TrainingConfig::default()
    };

    // 4. Create lineage info
    let lineage = LineageInfo {
        adapter_id: args.adapter_id.clone(),
        version: args.version.clone(),
        parent_version: None,
        parent_hash: None,
        mutations: Vec::new(),
        quality_delta: 0.0,
        created_at: Utc::now().to_rfc3339(),
    };

    // 5. Create adapter to get manifest and combined weights
    output.info("Creating adapter manifest...");
    let mut adapter = SingleFileAdapter::create(
        args.adapter_id.clone(),
        weights,
        training_data,
        config,
        lineage,
    )?;
    if args.training_data.is_some() {
        let metadata_path = resolve_adapter_metadata_path(&args.source)?;
        if !metadata_path.exists() {
            return Err(AosError::Validation(format!(
                "adapter_metadata.json not found at {}",
                metadata_path.display()
            )));
        }
        let metadata_str = fs::read_to_string(&metadata_path).map_err(|e| {
            AosError::Io(format!(
                "Failed to read adapter metadata {}: {}",
                metadata_path.display(),
                e
            ))
        })?;
        let metadata_value: serde_json::Value =
            serde_json::from_str(&metadata_str).map_err(|e| {
                AosError::Parse(format!(
                    "Failed to parse adapter metadata {}: {}",
                    metadata_path.display(),
                    e
                ))
            })?;
        let required = [
            "dataset_hash_b3",
            "framing_policy",
            "tokenizer_hash_b3",
            "training_config_hash",
            "determinism_tier",
        ];
        for key in required {
            let value = metadata_value
                .get(key)
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    AosError::Validation(format!(
                        "adapter_metadata.json missing required field '{}'",
                        key
                    ))
                })?;
            adapter.manifest.metadata.insert(key.to_string(), value.to_string());
        }
    }

    // 6. Optionally sign with Ed25519
    if args.sign {
        output.info("Signing adapter with Ed25519...");
        let keypair = if let Some(ref key_hex) = args.signing_key {
            let key_bytes = hex::decode(key_hex)
                .map_err(|e| AosError::Config(format!("Invalid signing key hex: {}", e)))?;
            if key_bytes.len() != 32 {
                return Err(AosError::Config(format!(
                    "Signing key must be 32 bytes, got {} bytes",
                    key_bytes.len()
                )));
            }
            let mut seed = [0u8; 32];
            seed.copy_from_slice(&key_bytes);
            Keypair::from_bytes(&seed)
        } else {
            output.info("Generating new Ed25519 keypair");
            Keypair::generate()
        };

        adapter.sign(&keypair)?;

        if let Some((key_id, timestamp)) = adapter.signature_info() {
            output.kv("Key ID", &key_id);
            output.kv("Timestamp", &timestamp.to_string());
        }
        output.success("Adapter signed successfully");
    }

    // 7. Get combined weights for inference and serialize to safetensors
    output.info("Preparing weights for AOS archive...");
    let combined_weights = adapter.get_inference_weights()?;
    let weights_bytes = serialize_weights_to_safetensors(&combined_weights)?;

    // 8. Get scope_path from manifest metadata
    let scope_path = adapter
        .manifest
        .metadata
        .get("scope_path")
        .cloned()
        .unwrap_or_else(|| "unspecified/unspecified/global/unspecified".to_string());

    // 9. Create AOS archive using AosWriter
    output.info("Writing AOS archive...");
    let mut writer = AosWriter::new();
    writer.add_segment(BackendTag::Canonical, Some(scope_path), &weights_bytes)?;

    let total_size = writer.write_archive(&args.output, &adapter.manifest)?;

    // 10. Output summary
    output.success(format!(
        "Successfully created .aos file: {}",
        args.output.display()
    ));
    output.blank();
    output.section("Summary");
    output.kv("Adapter ID", &args.adapter_id);
    output.kv("Version", &args.version);
    output.kv("Format", "aos");
    output.kv("Signed", if args.sign { "yes" } else { "no" });
    output.kv("File Size", &format!("{} bytes", total_size));

    Ok(())
}

fn resolve_adapter_metadata_path(source: &PathBuf) -> Result<PathBuf> {
    if source.is_dir() {
        return Ok(source.join("adapter_metadata.json"));
    }
    let parent = source.parent().ok_or_else(|| {
        AosError::Validation(format!(
            "Source path {} has no parent directory",
            source.display()
        ))
    })?;
    Ok(parent.join("adapter_metadata.json"))
}

pub async fn load_aos(args: LoadArgs, output: &OutputWriter) -> Result<()> {
    // Step 1: Load the .aos file
    output.info(format!("Loading .aos file: {}", args.path.display()));

    let load_options = LoadOptions::default();
    let adapter = SingleFileAdapterLoader::load_with_options(&args.path, load_options).await?;

    // Step 2: Extract adapter_id from manifest (or use provided one)
    let adapter_id = args
        .adapter_id
        .unwrap_or_else(|| adapter.manifest.adapter_id.clone());

    output.info(format!("Adapter ID: {}", adapter_id));

    // Step 3: Create registration request payload
    #[derive(serde::Serialize)]
    struct RegisterAdapterRequest {
        adapter_id: String,
        name: String,
        hash_b3: String,
        rank: i32,
        tier: String,
        languages: Vec<String>,
        framework: Option<String>,
        category: String,
        scope: Option<String>,
        expires_at: Option<String>,
    }

    let request = RegisterAdapterRequest {
        adapter_id: adapter_id.clone(),
        name: adapter_id.clone(),
        hash_b3: adapter.manifest.weights_hash.clone(),
        rank: adapter.manifest.rank as i32,
        tier: adapter.manifest.tier.clone(),
        languages: vec![], // Empty by default, can be extended
        framework: None,
        category: adapter.manifest.category.clone(),
        scope: Some(adapter.manifest.scope.clone()),
        expires_at: None,
    };

    // Step 4: Make HTTP POST request to /v1/adapters/register
    let client = reqwest::Client::new();
    let url = format!(
        "{}/v1/adapters/register",
        args.base_url.trim_end_matches('/')
    );

    output.info(format!("Registering adapter with control plane: {}", url));

    let response = client
        .post(&url)
        .json(&request)
        .send()
        .await
        .map_err(|e| AosError::Io(format!("HTTP request failed: {}", e)))?;

    let status = response.status();

    // Step 5: Display result
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(AosError::Http(format!(
            "Registration failed: {} {}",
            status, text
        )));
    }

    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|e| AosError::Http(e.to_string()))?;

    if output.is_json() {
        output.result(serde_json::to_string_pretty(&value).unwrap());
    } else {
        output.success(format!("Adapter registered successfully: {}", adapter_id));
        output.kv("Adapter ID", &adapter_id);
        output.kv("Version", &adapter.manifest.version);
        output.kv("Rank", &adapter.manifest.rank.to_string());
        output.kv("Alpha", &adapter.manifest.alpha.to_string());
        output.kv("Base Model", &adapter.manifest.base_model);
        output.kv("Category", &adapter.manifest.category);
        output.kv("Tier", &adapter.manifest.tier);
    }

    Ok(())
}

/// Verification report for .aos file
#[derive(Debug, Serialize)]
struct VerifyReport {
    path: String,
    is_valid: bool,
    errors: Vec<String>,
    warnings: Vec<String>,
}

pub async fn verify_aos(args: VerifyArgs, output: &OutputWriter) -> Result<()> {
    // Validate the .aos file
    let validation_result = SingleFileAdapterValidator::validate(&args.path).await?;

    // Create verification report
    let report = VerifyReport {
        path: args.path.display().to_string(),
        is_valid: validation_result.is_valid,
        errors: validation_result.errors.clone(),
        warnings: validation_result.warnings.clone(),
    };

    // Output based on format
    if args.format == "json" {
        output
            .json(&report)
            .map_err(|e| AosError::Config(format!("Failed to serialize JSON output: {}", e)))?;
    } else {
        // Text format
        output.info("Verifying .aos file");
        output.kv("Path", &report.path);
        output.blank();

        // Display errors
        if !report.errors.is_empty() {
            output.error("Validation Errors:");
            for error in &report.errors {
                output.error(format!("  - {}", error));
            }
            output.blank();
        }

        // Display warnings
        if !report.warnings.is_empty() {
            output.warning("Validation Warnings:");
            for warning in &report.warnings {
                output.warning(format!("  - {}", warning));
            }
            output.blank();
        }

        // Display final result
        if report.is_valid {
            output.success("Validation passed: .aos file is valid");
        } else {
            output.error("Validation failed: .aos file has errors");
        }
    }

    // Return error if validation failed
    if !validation_result.is_valid {
        return Err(AosError::Config("Validation failed".to_string()));
    }

    Ok(())
}

async fn extract_aos(args: ExtractArgs, output: &OutputWriter) -> Result<()> {
    // Parse components to extract
    let component_list: Vec<&str> = args.components.split(',').map(|s| s.trim()).collect();

    // Determine which components to extract
    let extract_all = component_list.contains(&"all");
    let components_to_extract: Vec<&str> = if extract_all {
        vec![
            "manifest",
            "weights",
            "training_data",
            "config",
            "lineage",
            "signature",
        ]
    } else {
        component_list
    };

    // Create output directory if it doesn't exist
    fs::create_dir_all(&args.output_dir).map_err(|e| {
        AosError::Io(format!(
            "Failed to create output directory {}: {}",
            args.output_dir.display(),
            e
        ))
    })?;

    output.info(format!(
        "Extracting components from {} to {}",
        args.path.display(),
        args.output_dir.display()
    ));

    let mut extracted_count = 0;

    // Extract each requested component
    for component in components_to_extract {
        match component {
            "manifest" => {
                match SingleFileAdapterLoader::extract_component(&args.path, "manifest").await {
                    Ok(data) => {
                        let output_path = args.output_dir.join("manifest.json");
                        fs::write(&output_path, data).map_err(|e| {
                            AosError::Io(format!("Failed to write manifest.json: {}", e))
                        })?;
                        output.success(format!("Extracted manifest to {}", output_path.display()));
                        extracted_count += 1;
                    }
                    Err(e) => {
                        output.warning(format!("Failed to extract manifest: {}", e));
                    }
                }
            }
            "weights" => {
                // Try to extract all weight files (combined, positive, negative)
                let weight_files = vec![
                    ("weights_combined", "weights_combined.safetensors"),
                    ("weights_positive", "weights_positive.safetensors"),
                    ("weights_negative", "weights_negative.safetensors"),
                    ("weights", "weights.safetensors"),
                ];

                let mut found_weights = false;
                for (component_name, file_name) in weight_files {
                    if let Ok(data) =
                        SingleFileAdapterLoader::extract_component(&args.path, component_name).await
                    {
                        let output_path = args.output_dir.join(file_name);
                        fs::write(&output_path, data).map_err(|e| {
                            AosError::Io(format!("Failed to write {}: {}", file_name, e))
                        })?;
                        output.success(format!(
                            "Extracted {} to {}",
                            component_name,
                            output_path.display()
                        ));
                        found_weights = true;
                        extracted_count += 1;
                    }
                }

                if !found_weights {
                    output.warning("No weights files found in .aos file");
                }
            }
            "training_data" => {
                match SingleFileAdapterLoader::extract_component(&args.path, "training_data").await
                {
                    Ok(data) => {
                        let output_path = args.output_dir.join("training_data.jsonl");
                        fs::write(&output_path, data).map_err(|e| {
                            AosError::Io(format!("Failed to write training_data.jsonl: {}", e))
                        })?;
                        output.success(format!(
                            "Extracted training_data to {}",
                            output_path.display()
                        ));
                        extracted_count += 1;
                    }
                    Err(e) => {
                        output.warning(format!("Failed to extract training_data: {}", e));
                    }
                }
            }
            "config" => {
                match SingleFileAdapterLoader::extract_component(&args.path, "config").await {
                    Ok(data) => {
                        let output_path = args.output_dir.join("config.toml");
                        fs::write(&output_path, data).map_err(|e| {
                            AosError::Io(format!("Failed to write config.toml: {}", e))
                        })?;
                        output.success(format!("Extracted config to {}", output_path.display()));
                        extracted_count += 1;
                    }
                    Err(e) => {
                        output.warning(format!("Failed to extract config: {}", e));
                    }
                }
            }
            "lineage" => {
                match SingleFileAdapterLoader::extract_component(&args.path, "lineage").await {
                    Ok(data) => {
                        let output_path = args.output_dir.join("lineage.json");
                        fs::write(&output_path, data).map_err(|e| {
                            AosError::Io(format!("Failed to write lineage.json: {}", e))
                        })?;
                        output.success(format!("Extracted lineage to {}", output_path.display()));
                        extracted_count += 1;
                    }
                    Err(e) => {
                        output.warning(format!("Failed to extract lineage: {}", e));
                    }
                }
            }
            "signature" => {
                match SingleFileAdapterLoader::extract_component(&args.path, "signature").await {
                    Ok(data) => {
                        let output_path = args.output_dir.join("signature.sig");
                        fs::write(&output_path, data).map_err(|e| {
                            AosError::Io(format!("Failed to write signature.sig: {}", e))
                        })?;
                        output.success(format!("Extracted signature to {}", output_path.display()));
                        extracted_count += 1;
                    }
                    Err(e) => {
                        output.warning(format!("Failed to extract signature: {}", e));
                    }
                }
            }
            _ => {
                output.warning(format!("Unknown component: {}", component));
            }
        }
    }

    if extracted_count > 0 {
        output.success(format!(
            "Successfully extracted {} component(s) to {}",
            extracted_count,
            args.output_dir.display()
        ));
        Ok(())
    } else {
        Err(AosError::Config(
            "No components were successfully extracted".to_string(),
        ))
    }
}

/// Information report for .aos file
#[derive(Debug, Serialize)]
struct InfoReport {
    adapter_id: String,
    version: String,
    rank: u32,
    alpha: f32,
    base_model: String,
    category: String,
    tier: String,
    created_at: String,
    weights_hash: String,
    format_version: u8,
    file_size_bytes: u64,
}

async fn info_aos(args: InfoArgs, output: &OutputWriter) -> Result<()> {
    // Load manifest only (fast operation without extracting weights)
    let manifest = SingleFileAdapterLoader::load_manifest_only(&args.path).await?;

    // Get file size
    let metadata = fs::metadata(&args.path)
        .map_err(|e| AosError::Io(format!("Failed to read file metadata: {}", e)))?;
    let file_size = metadata.len();

    // Create info report
    let info = InfoReport {
        adapter_id: manifest.adapter_id.clone(),
        version: manifest.version.clone(),
        rank: manifest.rank,
        alpha: manifest.alpha,
        base_model: manifest.base_model.clone(),
        category: manifest.category.clone(),
        tier: manifest.tier.clone(),
        created_at: manifest.created_at.clone(),
        weights_hash: manifest.weights_hash.clone(),
        format_version: manifest.format_version,
        file_size_bytes: file_size,
    };

    // Output based on format
    if args.format == "json" {
        output.json(&info)?;
    } else {
        // Text format
        output.section("Adapter Information");
        output.kv("Adapter ID", &info.adapter_id);
        output.kv("Version", &info.version);
        output.kv("Rank", &info.rank.to_string());
        output.kv("Alpha", &info.alpha.to_string());
        output.kv("Base Model", &info.base_model);
        output.kv("Category", &info.category);
        output.kv("Tier", &info.tier);
        output.kv("Created At", &info.created_at);
        output.kv("Weights Hash", &info.weights_hash);
        output.kv("Format Version", &info.format_version.to_string());
        output.kv("File Size", &format!("{} bytes", info.file_size_bytes));
    }

    Ok(())
}

async fn migrate_aos(args: MigrateArgs, output: &OutputWriter) -> Result<()> {
    // Validate path exists
    if !args.path.exists() {
        return Err(AosError::Io(format!(
            "File not found: {}",
            args.path.display()
        )));
    }

    output.info(format!("Migrating adapter: {}", args.path.display()));

    // Call the migration function (always creates backup when changes occur)
    let result = migrate_file(&args.path).await?;

    // Report results
    if result.original_version == result.new_version {
        output.success(format!(
            "Adapter already at current format version {}",
            result.new_version
        ));
    } else {
        output.success(format!(
            "Migrated from v{} to v{}",
            result.original_version, result.new_version
        ));
        for change in &result.changes_applied {
            output.info(format!("  - {}", change));
        }

        // Report backup location
        let backup_path = args.path.with_extension("aos.bak");
        if args.backup {
            output.info(format!("Backup saved to: {}", backup_path.display()));
        } else {
            // User doesn't want backup - remove it
            if backup_path.exists() {
                if let Err(e) = std::fs::remove_file(&backup_path) {
                    output.warning(format!("Could not remove backup: {}", e));
                } else {
                    output.verbose("Backup removed as requested");
                }
            }
        }
    }

    Ok(())
}
