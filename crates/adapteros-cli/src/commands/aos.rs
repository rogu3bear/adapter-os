//! AOS adapter commands
//!
//! This module provides CLI commands for working with .aos adapter files.
//! Uses adapteros-aos v3.0 types (see `crates/adapteros-aos/src/implementation.rs`).

use super::aos_impl;
use crate::output::OutputWriter;
use adapteros_aos::single_file::{
    migrate_file, LineageInfo, LoadOptions, SingleFileAdapter, SingleFileAdapterLoader,
    SingleFileAdapterValidator, TrainingConfig, WeightGroup,
};
use adapteros_aos::{AosWriter, BackendTag};
use adapteros_core::{AosError, Result};
use adapteros_crypto::Keypair;
use chrono::Utc;
use safetensors::tensor::TensorView;

use clap::{Parser, Subcommand};
use owo_colors::OwoColorize;
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

    safetensors::tensor::serialize(tensors, None)
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
    /// Migrate .aos file to current format version [NOT IMPLEMENTED]
    Migrate(MigrateArgs), // COORDINATION: Affects format version compatibility

    // =========================================================================
    // Git-style versioning commands
    // =========================================================================
    /// Initialize adapter repository in var/adapters
    Init(InitArgs),
    /// Create a new adapter (subject, domain, or specialized)
    New(NewArgs),
    /// List versions of an adapter
    Versions(VersionsArgs),
    /// Promote draft to current version
    Promote(PromoteArgs),
    /// Rollback to previous version
    Rollback(RollbackArgs),
    /// Create or update a named tag/ref
    Tag(TagArgs),
    /// Compare two versions
    Diff(DiffArgs),
    /// Show repository status
    Status(StatusArgs),
    /// Garbage collect unreferenced objects
    Gc(GcArgs),
    /// Migrate legacy repo/ adapters to new versioning layout
    MigrateRepo(MigrateRepoArgs),

    /// Stack management (adapter compositions)
    #[command(subcommand)]
    Stack(StackCmd),
}

// =============================================================================
// Stack management types and subcommands
// =============================================================================

/// Stack management subcommands for adapter compositions
#[derive(Debug, Subcommand, Clone)]
pub enum StackCmd {
    /// Create a new stack definition
    #[command(after_help = "\
Examples:
  # Create a stack with two components
  aosctl aos stack new dev-full --components developer.aos@v1,actions.domain.aos@current

  # Create with description
  aosctl aos stack new my-stack --components adapter1@v2 --description \"Production stack\"
")]
    New(StackNewArgs),

    /// Update stack components (add/remove)
    #[command(after_help = "\
Examples:
  # Add a component
  aosctl aos stack update dev-full --add security.domain.aos@stable

  # Remove a component
  aosctl aos stack update dev-full --remove old-adapter.aos

  # Add and remove in one operation
  aosctl aos stack update dev-full --add new.aos@v1 --remove old.aos
")]
    Update(StackUpdateArgs),

    /// List all stacks for a tenant
    #[command(after_help = "\
Examples:
  # List stacks for default tenant
  aosctl aos stack list

  # List stacks for specific tenant
  aosctl aos stack list --tenant-id my-tenant

  # Output as JSON
  aosctl aos stack list --format json
")]
    List(StackListArgs),

    /// Show stack details and resolved component hashes
    #[command(after_help = "\
Examples:
  # Show stack details
  aosctl aos stack show dev-full

  # Show with JSON output
  aosctl aos stack show dev-full --format json
")]
    Show(StackShowArgs),
}

#[derive(Debug, Parser, Clone)]
pub struct StackNewArgs {
    /// Stack name (e.g., dev-full, production)
    pub name: String,

    /// Tenant ID
    #[arg(long, default_value = "default")]
    pub tenant_id: String,

    /// Components in format: adapter@ref (comma-separated)
    /// Example: developer.aos@v1,actions.domain.aos@current
    #[arg(long, value_delimiter = ',')]
    pub components: Vec<String>,

    /// Optional description
    #[arg(long)]
    pub description: Option<String>,
}

#[derive(Debug, Parser, Clone)]
pub struct StackUpdateArgs {
    /// Stack name
    pub name: String,

    /// Tenant ID
    #[arg(long, default_value = "default")]
    pub tenant_id: String,

    /// Components to add (format: adapter@ref)
    #[arg(long, value_delimiter = ',')]
    pub add: Vec<String>,

    /// Components to remove (by adapter name)
    #[arg(long, value_delimiter = ',')]
    pub remove: Vec<String>,
}

#[derive(Debug, Parser, Clone)]
pub struct StackListArgs {
    /// Tenant ID
    #[arg(long, default_value = "default")]
    pub tenant_id: String,

    /// Output format (text, json)
    #[arg(long, default_value = "text")]
    pub format: String,
}

#[derive(Debug, Parser, Clone)]
pub struct StackShowArgs {
    /// Stack name
    pub name: String,

    /// Tenant ID
    #[arg(long, default_value = "default")]
    pub tenant_id: String,

    /// Output format (text, json)
    #[arg(long, default_value = "text")]
    pub format: String,
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
    #[arg(long, default_value = "http://127.0.0.1:18080")]
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

// =========================================================================
// Git-style versioning argument structs
// =========================================================================

#[derive(Debug, Parser, Clone)]
pub struct InitArgs {
    /// Adapter repository root (default: var/adapters)
    #[arg(long)]
    pub root: Option<PathBuf>,

    /// Tenant ID for the repository
    #[arg(long, default_value = "default")]
    pub tenant_id: String,
}

#[derive(Debug, Parser, Clone)]
pub struct NewArgs {
    /// Adapter name (e.g., developer.aos, actions.domain.aos)
    pub name: String,

    /// Tenant ID
    #[arg(long, default_value = "default")]
    pub tenant_id: String,

    /// Base model for the adapter
    #[arg(long)]
    pub base_model: Option<String>,

    /// Description of the adapter
    #[arg(long)]
    pub description: Option<String>,
}

#[derive(Debug, Parser, Clone)]
pub struct VersionsArgs {
    /// Adapter name (e.g., developer.aos)
    pub name: String,

    /// Tenant ID
    #[arg(long, default_value = "default")]
    pub tenant_id: String,

    /// Output format (text, json)
    #[arg(long, default_value = "text")]
    pub format: String,

    /// Show lineage (parent chain) for each version
    #[arg(long)]
    pub lineage: bool,

    /// Show full version history from the repository (not just refs)
    #[arg(long)]
    pub history: bool,
}

#[derive(Debug, Parser, Clone)]
pub struct PromoteArgs {
    /// Adapter name (e.g., developer.aos)
    pub name: String,

    /// Tenant ID
    #[arg(long, default_value = "default")]
    pub tenant_id: String,

    /// Source ref to promote (default: draft)
    #[arg(long, default_value = "draft")]
    pub from: String,

    /// Create a version tag (e.g., v1, v2)
    #[arg(long)]
    pub tag: Option<String>,

    /// Auto-generate version tag (v1, v2, v3, ...)
    #[arg(long)]
    pub auto_tag: bool,
}

#[derive(Debug, Parser, Clone)]
pub struct RollbackArgs {
    /// Adapter name (e.g., developer.aos)
    pub name: String,

    /// Tenant ID
    #[arg(long, default_value = "default")]
    pub tenant_id: String,

    /// Target ref or version to rollback to (default: previous)
    #[arg(long, default_value = "previous")]
    pub to: String,
}

#[derive(Debug, Parser, Clone)]
pub struct TagArgs {
    /// Adapter name (e.g., developer.aos)
    pub name: String,

    /// Tag name to create (e.g., stable, release-2024)
    pub tag: String,

    /// Tenant ID
    #[arg(long, default_value = "default")]
    pub tenant_id: String,

    /// Target ref (default: current)
    #[arg(long, default_value = "current")]
    pub from: String,

    /// Delete the tag instead of creating it
    #[arg(long)]
    pub delete: bool,
}

#[derive(Debug, Parser, Clone)]
pub struct DiffArgs {
    /// Adapter name (e.g., developer.aos)
    pub name: String,

    /// First ref for comparison (default: current)
    #[arg(long, default_value = "current")]
    pub from: String,

    /// Second ref for comparison
    pub to: String,

    /// Tenant ID
    #[arg(long, default_value = "default")]
    pub tenant_id: String,

    /// Output format (text, json)
    #[arg(long, default_value = "text")]
    pub format: String,
}

#[derive(Debug, Parser, Clone)]
pub struct StatusArgs {
    /// Tenant ID
    #[arg(long, default_value = "default")]
    pub tenant_id: String,

    /// Show detailed status for each adapter
    #[arg(long)]
    pub verbose: bool,

    /// Output format (text, json)
    #[arg(long, default_value = "text")]
    pub format: String,
}

#[derive(Debug, Parser, Clone)]
pub struct GcArgs {
    /// Tenant ID
    #[arg(long, default_value = "default")]
    pub tenant_id: String,

    /// Dry run - show what would be deleted without deleting
    #[arg(long)]
    pub dry_run: bool,

    /// Keep objects newer than N days (default: 30)
    #[arg(long, default_value = "30")]
    pub keep_days: u32,
}

#[derive(Debug, Parser, Clone)]
pub struct MigrateRepoArgs {
    /// Legacy repo/ directory path
    #[arg(long)]
    pub source: Option<PathBuf>,

    /// Target adapter repository root (default: var/adapters)
    #[arg(long)]
    pub target: Option<PathBuf>,

    /// Tenant ID
    #[arg(long, default_value = "default")]
    pub tenant_id: String,

    /// Dry run - show what would be migrated without migrating
    #[arg(long)]
    pub dry_run: bool,
}

pub async fn run(args: AosArgs, output: &OutputWriter) -> Result<()> {
    match args.cmd {
        AosCmd::Create(create) => create_aos(create, output).await,
        AosCmd::Load(load) => load_aos(load, output).await,
        AosCmd::Verify(verify) => verify_aos(verify, output).await,
        AosCmd::Extract(extract) => extract_aos(extract, output).await,
        AosCmd::Info(info) => info_aos(info, output).await,
        AosCmd::Migrate(migrate) => migrate_aos(migrate, output).await,
        // Git-style versioning commands
        AosCmd::Init(init) => init_repo(init, output).await,
        AosCmd::New(new) => new_adapter(new, output).await,
        AosCmd::Versions(versions) => list_versions(versions, output).await,
        AosCmd::Promote(promote) => promote_adapter(promote, output).await,
        AosCmd::Rollback(rollback) => rollback_adapter(rollback, output).await,
        AosCmd::Tag(tag) => tag_adapter(tag, output).await,
        AosCmd::Diff(diff) => diff_versions(diff, output).await,
        AosCmd::Status(status) => repo_status(status, output).await,
        AosCmd::Gc(gc) => gc_repo(gc, output).await,
        AosCmd::MigrateRepo(migrate) => migrate_repo(migrate, output).await,
        AosCmd::Stack(stack_cmd) => run_stack_cmd(stack_cmd, output).await,
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
            adapter
                .manifest
                .metadata
                .insert(key.to_string(), value.to_string());
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

fn resolve_adapter_metadata_path(source: &std::path::Path) -> Result<PathBuf> {
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

// =============================================================================
// Git-style versioning command implementations
// =============================================================================

use adapteros_storage::redb::RedbBackend;
use adapteros_storage::{
    AdapterKind, AdapterLayout, AdapterName, AdapterRef, AdapterVersion, AdapterVersionRepository,
    FsRefStore, KvIndexManager, RefStore,
};
use std::sync::Arc;

/// Default adapter repository root
fn default_adapter_root() -> PathBuf {
    std::env::var("AOS_ADAPTERS_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("var/adapters"))
}

/// Open or create the adapter version repository.
///
/// Uses a ReDB database at `{adapter_root}/index.redb`.
/// Returns a tuple of (backend, index_manager, repository) for flexibility.
fn open_version_repository(adapter_root: &std::path::Path) -> Result<AdapterVersionRepository> {
    let index_path = adapter_root.join("index.redb");

    // Ensure parent directory exists
    if let Some(parent) = index_path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            AosError::Io(format!(
                "Failed to create adapter repository directory {}: {}",
                parent.display(),
                e
            ))
        })?;
    }

    let backend = Arc::new(RedbBackend::open(&index_path).map_err(|e| {
        AosError::Config(format!(
            "Failed to open version index at {}: {}",
            index_path.display(),
            e
        ))
    })?);

    let index_manager = Arc::new(KvIndexManager::new(backend.clone()));
    let repo = AdapterVersionRepository::new(backend, index_manager);

    Ok(repo)
}

/// Initialize the adapter repository
async fn init_repo(args: InitArgs, output: &OutputWriter) -> Result<()> {
    let root = args.root.unwrap_or_else(default_adapter_root);
    let layout = AdapterLayout::new(&root);

    // Create directory structure
    let dirs = [
        layout.objects_dir(),
        layout.subjects_dir(),
        layout.domains_dir(),
        layout.specialized_dir(),
        layout.stacks_dir(),
    ];

    for dir in &dirs {
        fs::create_dir_all(dir).map_err(|e| {
            AosError::Io(format!(
                "Failed to create directory {}: {}",
                dir.display(),
                e
            ))
        })?;
    }

    output.success(format!(
        "Initialized adapter repository at {}",
        root.display()
    ));
    output.blank();
    output.section("Directory Structure");
    output.kv("Objects", &layout.objects_dir().display().to_string());
    output.kv("Subjects", &layout.subjects_dir().display().to_string());
    output.kv("Domains", &layout.domains_dir().display().to_string());
    output.kv(
        "Specialized",
        &layout.specialized_dir().display().to_string(),
    );
    output.kv("Stacks", &layout.stacks_dir().display().to_string());

    Ok(())
}

/// Create a new adapter
async fn new_adapter(args: NewArgs, output: &OutputWriter) -> Result<()> {
    let adapter_name = AdapterName::parse(&args.name)
        .map_err(|e| AosError::Config(format!("Invalid adapter name: {}", e)))?;

    let root = default_adapter_root();
    let layout = AdapterLayout::new(&root);

    // Create refs directory for this adapter
    let refs_dir = layout.refs_dir(&adapter_name, &args.tenant_id);
    fs::create_dir_all(&refs_dir).map_err(|e| {
        AosError::Io(format!(
            "Failed to create refs directory {}: {}",
            refs_dir.display(),
            e
        ))
    })?;

    output.success(format!("Created adapter: {}", adapter_name));
    output.blank();
    output.section("Adapter Details");
    output.kv("Name", &adapter_name.to_string());
    output.kv("Kind", &adapter_name.kind.to_string());
    output.kv("Tenant", &args.tenant_id);
    output.kv("Refs Directory", &refs_dir.display().to_string());

    if let Some(subject) = &adapter_name.subject {
        output.kv("Subject", subject);
    }
    if let Some(domain) = &adapter_name.domain {
        output.kv("Domain", domain);
    }
    if let Some(base_model) = &args.base_model {
        output.kv("Base Model", base_model);
    }

    output.blank();
    output.info("Next steps:");
    output.info("  1. Train the adapter with: aosctl train-docs --docs-dir ./docs");
    output.info("  2. Import with: aosctl aos import <path-to-trained.aos>");
    output.info("  3. Promote with: aosctl aos promote");

    Ok(())
}

/// List versions of an adapter
async fn list_versions(args: VersionsArgs, output: &OutputWriter) -> Result<()> {
    let adapter_name = AdapterName::parse(&args.name)
        .map_err(|e| AosError::Config(format!("Invalid adapter name: {}", e)))?;

    let root = default_adapter_root();
    let layout = AdapterLayout::new(&root);
    let store = FsRefStore::new(layout);

    let refs = store
        .list_refs(&adapter_name, &args.tenant_id)
        .await
        .map_err(|e| AosError::Config(format!("Failed to list refs: {}", e)))?;

    // Load version history from repository if --history flag is set
    let version_history: Vec<AdapterVersion> = if args.history {
        match open_version_repository(&root) {
            Ok(repo) => repo
                .list_by_name(&args.tenant_id, &adapter_name)
                .await
                .map_err(|e| AosError::Config(format!("Failed to list version history: {}", e)))?,
            Err(e) => {
                let _ = output.warn(format!("Could not open version repository: {}", e));
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    if refs.is_empty() && version_history.is_empty() {
        output.info(format!("No versions found for {}", adapter_name));
        return Ok(());
    }

    if args.format == "json" {
        // Include both refs and history in JSON output
        #[derive(serde::Serialize)]
        struct VersionsOutput<'a> {
            refs: &'a [AdapterRef],
            #[serde(skip_serializing_if = "<[_]>::is_empty")]
            history: &'a [AdapterVersion],
        }
        output.json(&VersionsOutput {
            refs: &refs,
            history: &version_history,
        })?;
    } else {
        output.section(format!("Versions of {}", adapter_name));
        output.blank();

        // Separate version tags from named refs
        let mut version_refs: Vec<&AdapterRef> =
            refs.iter().filter(|r| r.is_version_tag()).collect();
        let named_refs: Vec<&AdapterRef> = refs.iter().filter(|r| !r.is_version_tag()).collect();

        // Sort versions by semver (descending)
        version_refs.sort_by(|a, b| {
            let a_ver = a.parse_version().unwrap_or((0, 0, 0));
            let b_ver = b.parse_version().unwrap_or((0, 0, 0));
            b_ver.cmp(&a_ver)
        });

        // Show named refs first (current, previous, draft, etc.)
        if !named_refs.is_empty() {
            output.info("Named refs:");
            for r in named_refs {
                let hash_short = &r.target_hash[..r.target_hash.len().min(12)];
                output.kv(&format!("  {}", r.ref_name), hash_short);
            }
            output.blank();
        }

        // Show version tags
        if !version_refs.is_empty() {
            output.info("Version tags:");
            for r in version_refs {
                let hash_short = &r.target_hash[..r.target_hash.len().min(12)];
                output.kv(&format!("  {}", r.ref_name), hash_short);
            }
            output.blank();
        }

        // Show version history from repository
        if !version_history.is_empty() {
            output.info("Version history (from repository):");
            for v in &version_history {
                let hash_short = &v.hash[..v.hash.len().min(12)];
                let parent_info = v
                    .parent_hash
                    .as_ref()
                    .map(|p| format!(" <- {}", &p[..p.len().min(8)]))
                    .unwrap_or_default();
                // Format the timestamp (already RFC 3339 string, show first 16 chars: YYYY-MM-DDTHH:MM)
                let timestamp = if v.created_at.len() >= 16 {
                    &v.created_at[..16]
                } else {
                    &v.created_at
                };
                output.kv(
                    &format!("  v{}", v.version),
                    &format!("{}{} ({})", hash_short, parent_info, timestamp),
                );
            }

            // Show lineage for each version if requested (separate pass to avoid nested async)
            if args.lineage {
                if let Ok(repo) = open_version_repository(&root) {
                    output.blank();
                    output.info("Lineage:");
                    for v in &version_history {
                        if let Ok(lineage) = repo.get_lineage(&v.hash, 10).await {
                            if lineage.len() > 1 {
                                let hash_short = &v.hash[..v.hash.len().min(8)];
                                output.info(format!("  {} (v{}):", hash_short, v.version));
                                for ancestor in lineage.iter().skip(1) {
                                    let ancestor_hash =
                                        &ancestor.hash[..ancestor.hash.len().min(12)];
                                    output.info(format!(
                                        "    -> v{} ({})",
                                        ancestor.version, ancestor_hash
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Promote draft to current version
async fn promote_adapter(args: PromoteArgs, output: &OutputWriter) -> Result<()> {
    let adapter_name = AdapterName::parse(&args.name)
        .map_err(|e| AosError::Config(format!("Invalid adapter name: {}", e)))?;

    let root = default_adapter_root();
    let layout = AdapterLayout::new(&root);
    let store = FsRefStore::new(layout);

    // Get the source ref
    let source_hash = store
        .get_ref(&adapter_name, &args.tenant_id, &args.from)
        .await
        .map_err(|e| AosError::Config(format!("Failed to get source ref: {}", e)))?
        .ok_or_else(|| AosError::Config(format!("Source ref '{}' not found", args.from)))?;

    // Get current ref hash (will become parent of new version)
    let current_hash = store
        .get_ref(&adapter_name, &args.tenant_id, "current")
        .await
        .map_err(|e| AosError::Config(format!("Failed to get current ref: {}", e)))?;

    // Determine version tag
    let version_tag = if args.auto_tag {
        Some(
            adapteros_storage::refs::next_version_tag(&store, &adapter_name, &args.tenant_id)
                .await
                .map_err(|e| AosError::Config(format!("Failed to get next version: {}", e)))?,
        )
    } else {
        args.tag.clone()
    };

    // Promote refs
    adapteros_storage::refs::promote_version(
        &store,
        &adapter_name,
        &args.tenant_id,
        &source_hash,
        version_tag.as_deref(),
    )
    .await
    .map_err(|e| AosError::Config(format!("Failed to promote: {}", e)))?;

    // Create AdapterVersion record in the repository for persistent tracking
    if let Some(ref tag) = version_tag {
        match open_version_repository(&root) {
            Ok(repo) => {
                // Parse version string from tag (e.g., "v1" -> "1", "v1.2.3" -> "1.2.3")
                let version_str = tag.strip_prefix('v').unwrap_or(tag);
                let mut adapter_version =
                    AdapterVersion::new(source_hash.clone(), adapter_name.clone(), version_str);

                // Set parent hash if there was a current version before
                if let Some(parent) = current_hash.as_ref() {
                    adapter_version = adapter_version.with_parent(parent);
                }

                // Add tenant_id to metadata for indexing
                adapter_version
                    .metadata
                    .insert("tenant_id".to_string(), args.tenant_id.clone());

                // Save the version record
                if let Err(e) = repo.create(&adapter_version).await {
                    let _ = output.warn(format!("Failed to persist version record: {}", e));
                } else {
                    output.info("Version record persisted to repository");
                }
            }
            Err(e) => {
                let _ = output.warn(format!("Could not open version repository: {}", e));
            }
        }
    }

    output.success(format!(
        "Promoted {} from {} to current",
        adapter_name, args.from
    ));
    output.kv("Hash", &source_hash);
    if let Some(tag) = version_tag {
        output.kv("Tagged as", &tag);
    }

    Ok(())
}

/// Rollback to previous version
async fn rollback_adapter(args: RollbackArgs, output: &OutputWriter) -> Result<()> {
    let adapter_name = AdapterName::parse(&args.name)
        .map_err(|e| AosError::Config(format!("Invalid adapter name: {}", e)))?;

    let root = default_adapter_root();
    let layout = AdapterLayout::new(&root);
    let store = FsRefStore::new(layout);

    // Get the target ref
    let target_hash = store
        .get_ref(&adapter_name, &args.tenant_id, &args.to)
        .await
        .map_err(|e| AosError::Config(format!("Failed to get target ref: {}", e)))?
        .ok_or_else(|| AosError::Config(format!("Target ref '{}' not found", args.to)))?;

    // Get current for comparison
    let current_hash = store
        .get_ref(&adapter_name, &args.tenant_id, "current")
        .await
        .map_err(|e| AosError::Config(format!("Failed to get current ref: {}", e)))?;

    // Update current to target
    store
        .update_ref(&adapter_name, &args.tenant_id, "current", &target_hash)
        .await
        .map_err(|e| AosError::Config(format!("Failed to update current: {}", e)))?;

    output.success(format!("Rolled back {} to {}", adapter_name, args.to));
    output.kv("New current", &target_hash);
    if let Some(old) = current_hash {
        output.kv("Previous current", &old);
    }

    Ok(())
}

/// Create or update a named tag/ref
async fn tag_adapter(args: TagArgs, output: &OutputWriter) -> Result<()> {
    let adapter_name = AdapterName::parse(&args.name)
        .map_err(|e| AosError::Config(format!("Invalid adapter name: {}", e)))?;

    let root = default_adapter_root();
    let layout = AdapterLayout::new(&root);
    let store = FsRefStore::new(layout);

    if args.delete {
        // Delete the tag
        let deleted = store
            .delete_ref(&adapter_name, &args.tenant_id, &args.tag)
            .await
            .map_err(|e| AosError::Config(format!("Failed to delete tag: {}", e)))?;

        if deleted {
            output.success(format!("Deleted tag '{}' from {}", args.tag, adapter_name));
        } else {
            output.warning(format!("Tag '{}' not found", args.tag));
        }
    } else {
        // Get the source ref
        let source_hash = store
            .get_ref(&adapter_name, &args.tenant_id, &args.from)
            .await
            .map_err(|e| AosError::Config(format!("Failed to get source ref: {}", e)))?
            .ok_or_else(|| AosError::Config(format!("Source ref '{}' not found", args.from)))?;

        // Create the tag
        store
            .update_ref(&adapter_name, &args.tenant_id, &args.tag, &source_hash)
            .await
            .map_err(|e| AosError::Config(format!("Failed to create tag: {}", e)))?;

        output.success(format!(
            "Tagged {} as '{}' -> {}",
            adapter_name,
            args.tag,
            &source_hash[..source_hash.len().min(12)]
        ));
    }

    Ok(())
}

/// Diff report for JSON output
#[derive(Debug, Serialize)]
struct DiffReport {
    from_ref: String,
    to_ref: String,
    from_hash: String,
    to_hash: String,
    identical: bool,
    differences: Vec<FieldDiff>,
    from_size_bytes: Option<u64>,
    to_size_bytes: Option<u64>,
    size_diff_bytes: Option<i64>,
}

/// Individual field difference
#[derive(Debug, Serialize)]
struct FieldDiff {
    field: String,
    from_value: Option<String>,
    to_value: Option<String>,
    change_type: String, // "modified", "added", "removed"
}

/// Compare two versions with detailed manifest comparison
async fn diff_versions(args: DiffArgs, output: &OutputWriter) -> Result<()> {
    use adapteros_aos::single_file::AdapterManifest;

    let adapter_name = AdapterName::parse(&args.name)
        .map_err(|e| AosError::Config(format!("Invalid adapter name: {}", e)))?;

    let root = default_adapter_root();
    let layout = AdapterLayout::new(&root);
    let store = FsRefStore::new(layout.clone());

    // Resolve both refs
    let from_hash = store
        .get_ref(&adapter_name, &args.tenant_id, &args.from)
        .await
        .map_err(|e| AosError::Config(format!("Failed to get from ref: {}", e)))?
        .ok_or_else(|| AosError::Config(format!("From ref '{}' not found", args.from)))?;

    let to_hash = store
        .get_ref(&adapter_name, &args.tenant_id, &args.to)
        .await
        .map_err(|e| AosError::Config(format!("Failed to get to ref: {}", e)))?
        .ok_or_else(|| AosError::Config(format!("To ref '{}' not found", args.to)))?;

    // Get file paths and sizes
    let from_path = layout.object_path(&from_hash);
    let to_path = layout.object_path(&to_hash);

    let from_size = fs::metadata(&from_path).ok().map(|m| m.len());
    let to_size = fs::metadata(&to_path).ok().map(|m| m.len());
    let size_diff = match (from_size, to_size) {
        (Some(f), Some(t)) => Some(t as i64 - f as i64),
        _ => None,
    };

    // Check if identical
    if from_hash == to_hash {
        if args.format == "json" {
            let report = DiffReport {
                from_ref: args.from.clone(),
                to_ref: args.to.clone(),
                from_hash: from_hash.clone(),
                to_hash: to_hash.clone(),
                identical: true,
                differences: vec![],
                from_size_bytes: from_size,
                to_size_bytes: to_size,
                size_diff_bytes: None,
            };
            output.json(&report)?;
        } else {
            output.info("Both refs point to the same version");
            output.kv("Hash", &from_hash);
        }
        return Ok(());
    }

    // Load manifests from both .aos files
    let from_manifest: Option<AdapterManifest> = if from_path.exists() {
        SingleFileAdapterLoader::load_manifest_only(&from_path)
            .await
            .ok()
    } else {
        None
    };

    let to_manifest: Option<AdapterManifest> = if to_path.exists() {
        SingleFileAdapterLoader::load_manifest_only(&to_path)
            .await
            .ok()
    } else {
        None
    };

    // Collect differences
    let mut differences: Vec<FieldDiff> = Vec::new();

    if let (Some(ref from_m), Some(ref to_m)) = (&from_manifest, &to_manifest) {
        // Compare version
        if from_m.version != to_m.version {
            differences.push(FieldDiff {
                field: "version".to_string(),
                from_value: Some(from_m.version.clone()),
                to_value: Some(to_m.version.clone()),
                change_type: "modified".to_string(),
            });
        }

        // Compare rank
        if from_m.rank != to_m.rank {
            differences.push(FieldDiff {
                field: "rank".to_string(),
                from_value: Some(from_m.rank.to_string()),
                to_value: Some(to_m.rank.to_string()),
                change_type: "modified".to_string(),
            });
        }

        // Compare alpha
        if (from_m.alpha - to_m.alpha).abs() > f32::EPSILON {
            differences.push(FieldDiff {
                field: "alpha".to_string(),
                from_value: Some(format!("{:.2}", from_m.alpha)),
                to_value: Some(format!("{:.2}", to_m.alpha)),
                change_type: "modified".to_string(),
            });
        }

        // Compare base_model
        if from_m.base_model != to_m.base_model {
            differences.push(FieldDiff {
                field: "base_model".to_string(),
                from_value: Some(from_m.base_model.clone()),
                to_value: Some(to_m.base_model.clone()),
                change_type: "modified".to_string(),
            });
        }

        // Compare weights_hash
        if from_m.weights_hash != to_m.weights_hash {
            differences.push(FieldDiff {
                field: "weights_hash".to_string(),
                from_value: Some(adapteros_id::format_hash_short(&from_m.weights_hash)),
                to_value: Some(adapteros_id::format_hash_short(&to_m.weights_hash)),
                change_type: "modified".to_string(),
            });
        }

        // Compare created_at
        if from_m.created_at != to_m.created_at {
            differences.push(FieldDiff {
                field: "created_at".to_string(),
                from_value: Some(from_m.created_at.clone()),
                to_value: Some(to_m.created_at.clone()),
                change_type: "modified".to_string(),
            });
        }

        // Compare format_version
        if from_m.format_version != to_m.format_version {
            differences.push(FieldDiff {
                field: "format_version".to_string(),
                from_value: Some(from_m.format_version.to_string()),
                to_value: Some(to_m.format_version.to_string()),
                change_type: "modified".to_string(),
            });
        }

        // Compare category
        if from_m.category != to_m.category {
            differences.push(FieldDiff {
                field: "category".to_string(),
                from_value: Some(from_m.category.clone()),
                to_value: Some(to_m.category.clone()),
                change_type: "modified".to_string(),
            });
        }

        // Compare tier
        if from_m.tier != to_m.tier {
            differences.push(FieldDiff {
                field: "tier".to_string(),
                from_value: Some(from_m.tier.clone()),
                to_value: Some(to_m.tier.clone()),
                change_type: "modified".to_string(),
            });
        }

        // Compare scope
        if from_m.scope != to_m.scope {
            differences.push(FieldDiff {
                field: "scope".to_string(),
                from_value: Some(from_m.scope.clone()),
                to_value: Some(to_m.scope.clone()),
                change_type: "modified".to_string(),
            });
        }

        // Compare training_data_hash
        if from_m.training_data_hash != to_m.training_data_hash {
            differences.push(FieldDiff {
                field: "training_data_hash".to_string(),
                from_value: Some(adapteros_id::format_hash_short(&from_m.training_data_hash)),
                to_value: Some(adapteros_id::format_hash_short(&to_m.training_data_hash)),
                change_type: "modified".to_string(),
            });
        }

        // Compare target_modules
        if from_m.target_modules != to_m.target_modules {
            differences.push(FieldDiff {
                field: "target_modules".to_string(),
                from_value: Some(from_m.target_modules.join(", ")),
                to_value: Some(to_m.target_modules.join(", ")),
                change_type: "modified".to_string(),
            });
        }
    }

    // JSON output
    if args.format == "json" {
        let report = DiffReport {
            from_ref: args.from.clone(),
            to_ref: args.to.clone(),
            from_hash: from_hash.clone(),
            to_hash: to_hash.clone(),
            identical: false,
            differences,
            from_size_bytes: from_size,
            to_size_bytes: to_size,
            size_diff_bytes: size_diff,
        };
        output.json(&report)?;
        return Ok(());
    }

    // Text output with colors
    output.section(format!("Diff: {} -> {}", args.from, args.to));
    output.blank();

    // Show refs and hashes
    println!("  {}: {}", args.from.dimmed(), from_hash);
    println!("  {}: {}", args.to.dimmed(), to_hash);
    output.blank();

    // Show file sizes
    if let (Some(fs), Some(ts)) = (from_size, to_size) {
        let diff_str = match size_diff {
            Some(d) if d > 0 => format!("+{} bytes", d).green().to_string(),
            Some(d) if d < 0 => format!("{} bytes", d).red().to_string(),
            _ => "0 bytes".to_string(),
        };
        println!(
            "  {}: {} bytes -> {} bytes ({})",
            "Size".bold(),
            fs,
            ts,
            diff_str
        );
    } else {
        if from_size.is_none() {
            println!(
                "  {} {}",
                "!".red(),
                format!("{} object not found on disk", args.from).red()
            );
        }
        if to_size.is_none() {
            println!(
                "  {} {}",
                "!".red(),
                format!("{} object not found on disk", args.to).red()
            );
        }
    }
    output.blank();

    // Show manifest differences
    if !differences.is_empty() {
        println!("  {}:", "Changes".bold());
        for diff in &differences {
            let from_val = diff.from_value.as_deref().unwrap_or("-");
            let to_val = diff.to_value.as_deref().unwrap_or("-");
            println!(
                "    {}: {} -> {}",
                diff.field.cyan(),
                from_val.red(),
                to_val.green()
            );
        }
    } else if from_manifest.is_some() && to_manifest.is_some() {
        println!("  {}", "No manifest field differences detected".dimmed());
        println!(
            "  {}",
            "(Files differ only in hash/binary content)".dimmed()
        );
    }

    output.blank();

    Ok(())
}

/// Show repository status
async fn repo_status(args: StatusArgs, output: &OutputWriter) -> Result<()> {
    let root = default_adapter_root();
    let layout = AdapterLayout::new(&root);

    if !root.exists() {
        output.warning("Adapter repository not initialized");
        output.info(format!("Run: aosctl aos init --root {}", root.display()));
        return Ok(());
    }

    // Scan for adapters
    let mut adapters: Vec<(AdapterKind, String)> = Vec::new();

    for kind in [
        AdapterKind::Subject,
        AdapterKind::Domain,
        AdapterKind::Specialized,
        AdapterKind::Stack,
    ] {
        let kind_dir = root.join(kind.dir_name()).join(&args.tenant_id);
        if kind_dir.exists() {
            if let Ok(entries) = fs::read_dir(&kind_dir) {
                for entry in entries.flatten() {
                    if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        if let Some(name) = entry.file_name().to_str() {
                            adapters.push((kind, name.to_string()));
                        }
                    }
                }
            }
        }
    }

    // Count objects
    let mut object_count = 0;
    let mut total_size: u64 = 0;
    if layout.objects_dir().exists() {
        fn count_objects(dir: &PathBuf) -> (usize, u64) {
            let mut count = 0;
            let mut size = 0;
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        let (c, s) = count_objects(&path);
                        count += c;
                        size += s;
                    } else if path.extension().map(|e| e == "aos").unwrap_or(false) {
                        count += 1;
                        size += entry.metadata().map(|m| m.len()).unwrap_or(0);
                    }
                }
            }
            (count, size)
        }
        let (c, s) = count_objects(&layout.objects_dir());
        object_count = c;
        total_size = s;
    }

    if args.format == "json" {
        #[derive(Serialize)]
        struct StatusReport {
            root: String,
            tenant_id: String,
            adapter_count: usize,
            object_count: usize,
            total_size_bytes: u64,
            adapters: Vec<AdapterInfo>,
        }
        #[derive(Serialize)]
        struct AdapterInfo {
            kind: String,
            name: String,
        }

        let report = StatusReport {
            root: root.display().to_string(),
            tenant_id: args.tenant_id.clone(),
            adapter_count: adapters.len(),
            object_count,
            total_size_bytes: total_size,
            adapters: adapters
                .iter()
                .map(|(k, n)| AdapterInfo {
                    kind: k.to_string(),
                    name: n.clone(),
                })
                .collect(),
        };

        output.json(&report)?;
    } else {
        output.section("Adapter Repository Status");
        output.blank();
        output.kv("Root", &root.display().to_string());
        output.kv("Tenant", &args.tenant_id);
        output.kv("Adapters", &adapters.len().to_string());
        output.kv("Objects", &object_count.to_string());
        output.kv("Total Size", &format!("{} bytes", total_size));
        output.blank();

        if adapters.is_empty() {
            output.info("No adapters found");
            output.info("Create one with: aosctl aos new developer.aos");
        } else {
            output.info("Adapters:");
            for (kind, name) in &adapters {
                output.kv(&format!("  {}", kind), name);
            }
        }
    }

    Ok(())
}

/// Garbage collect unreferenced objects
async fn gc_repo(args: GcArgs, output: &OutputWriter) -> Result<()> {
    let root = default_adapter_root();
    let layout = AdapterLayout::new(&root);
    let store = FsRefStore::new(layout.clone());

    // Collect all referenced hashes
    let mut referenced_hashes = std::collections::HashSet::new();

    // Scan all adapter refs
    for kind in [
        AdapterKind::Subject,
        AdapterKind::Domain,
        AdapterKind::Specialized,
        AdapterKind::Stack,
    ] {
        let kind_dir = root.join(kind.dir_name()).join(&args.tenant_id);
        if !kind_dir.exists() {
            continue;
        }

        if let Ok(entries) = fs::read_dir(&kind_dir) {
            for entry in entries.flatten() {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    if let Some(name) = entry.file_name().to_str() {
                        // Reconstruct adapter name
                        let adapter_name = match kind {
                            AdapterKind::Subject => AdapterName::subject(name),
                            AdapterKind::Domain => AdapterName::domain(name),
                            AdapterKind::Stack => AdapterName::stack(name),
                            AdapterKind::Specialized => {
                                // Parse "subject.domain" format
                                if let Some((subj, dom)) = name.split_once('.') {
                                    AdapterName::specialized(subj, dom)
                                } else {
                                    continue;
                                }
                            }
                        };

                        // Get all refs for this adapter
                        if let Ok(refs) = store.list_refs(&adapter_name, &args.tenant_id).await {
                            for r in refs {
                                referenced_hashes.insert(r.target_hash);
                            }
                        }
                    }
                }
            }
        }
    }

    // Scan objects directory for unreferenced objects
    let mut unreferenced: Vec<(PathBuf, u64)> = Vec::new();
    let cutoff = chrono::Utc::now() - chrono::Duration::days(args.keep_days as i64);

    fn scan_objects(
        dir: &PathBuf,
        referenced: &std::collections::HashSet<String>,
        cutoff: chrono::DateTime<chrono::Utc>,
        unreferenced: &mut Vec<(PathBuf, u64)>,
    ) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    scan_objects(&path, referenced, cutoff, unreferenced);
                } else if path.extension().map(|e| e == "aos").unwrap_or(false) {
                    if let Some(hash) = path.file_stem().and_then(|s| s.to_str()) {
                        if !referenced.contains(hash) {
                            // Check age
                            if let Ok(meta) = entry.metadata() {
                                let modified = meta
                                    .modified()
                                    .ok()
                                    .map(chrono::DateTime::<chrono::Utc>::from);
                                if let Some(mtime) = modified {
                                    if mtime < cutoff {
                                        unreferenced.push((path, meta.len()));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if layout.objects_dir().exists() {
        scan_objects(
            &layout.objects_dir(),
            &referenced_hashes,
            cutoff,
            &mut unreferenced,
        );
    }

    let total_size: u64 = unreferenced.iter().map(|(_, s)| s).sum();

    if unreferenced.is_empty() {
        output.success("No unreferenced objects found");
        return Ok(());
    }

    output.section("Garbage Collection");
    output.kv("Unreferenced objects", &unreferenced.len().to_string());
    output.kv("Total reclaimable", &format!("{} bytes", total_size));
    output.blank();

    if args.dry_run {
        output.info("Dry run - no files deleted");
        output.blank();
        output.info("Would delete:");
        for (path, size) in &unreferenced {
            output.info(format!("  {} ({} bytes)", path.display(), size));
        }
    } else {
        let mut deleted_count = 0;
        let mut deleted_size: u64 = 0;

        for (path, size) in &unreferenced {
            match fs::remove_file(path) {
                Ok(()) => {
                    deleted_count += 1;
                    deleted_size += size;
                }
                Err(e) => {
                    output.warning(format!("Failed to delete {}: {}", path.display(), e));
                }
            }
        }

        output.success(format!(
            "Deleted {} objects, reclaimed {} bytes",
            deleted_count, deleted_size
        ));
    }

    Ok(())
}

// =============================================================================
// migrate_repo: Migrate legacy repo/ adapters to new versioning layout
// =============================================================================

/// Migration result for a single adapter
#[derive(Debug, Serialize)]
struct MigratedAdapter {
    source_path: String,
    adapter_name: String,
    kind: String,
    hash: String,
    object_path: String,
    refs_dir: String,
}

/// Migrate legacy var/adapters/repo/*.aos files to the new versioning layout
///
/// For each .aos file found in the legacy repo/ directory:
/// 1. Compute BLAKE3 hash of the file content
/// 2. Copy to objects/{hash[0:2]}/{hash[2:10]}/{full_hash}.aos
/// 3. Parse adapter name from filename to determine AdapterKind
/// 4. Create refs directory at appropriate location (subjects/, domains/, etc.)
/// 5. Create `current` ref symlink pointing to the object
/// 6. Create `v1` version tag
async fn migrate_repo(args: MigrateRepoArgs, output: &OutputWriter) -> Result<()> {
    let target_root = args.target.unwrap_or_else(default_adapter_root);
    let source_root = args.source.unwrap_or_else(|| target_root.join("repo"));
    let layout = AdapterLayout::new(&target_root);

    output.section("Legacy Adapter Repository Migration");
    output.blank();
    output.kv("Source", &source_root.display().to_string());
    output.kv("Target", &target_root.display().to_string());
    output.kv("Tenant", &args.tenant_id);
    output.kv("Dry run", if args.dry_run { "yes" } else { "no" });
    output.blank();

    // Check source exists
    if !source_root.exists() {
        output.warning(format!(
            "Legacy repo directory not found: {}",
            source_root.display()
        ));
        output.info("Nothing to migrate");
        return Ok(());
    }

    // Scan for .aos files in the legacy repo directory
    let mut aos_files: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = fs::read_dir(&source_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "aos").unwrap_or(false) {
                aos_files.push(path);
            }
        }
    }

    if aos_files.is_empty() {
        output.info("No .aos files found in legacy repo directory");
        return Ok(());
    }

    output.info(format!("Found {} .aos file(s) to migrate", aos_files.len()));
    output.blank();

    // Ensure target directories exist (unless dry run)
    if !args.dry_run {
        fs::create_dir_all(layout.objects_dir())
            .map_err(|e| AosError::Io(format!("Failed to create objects directory: {}", e)))?;
    }

    let store = FsRefStore::new(layout.clone());
    let mut migrated: Vec<MigratedAdapter> = Vec::new();
    let mut failed: Vec<(PathBuf, String)> = Vec::new();

    for aos_path in &aos_files {
        let filename = aos_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        output.info(format!("Processing: {}", filename.cyan()));

        // Parse adapter name from filename
        let adapter_name = match AdapterName::parse(filename) {
            Ok(name) => name,
            Err(e) => {
                let err_msg = format!("Failed to parse adapter name: {}", e);
                output.error(format!("  {}", err_msg));
                failed.push((aos_path.clone(), err_msg));
                continue;
            }
        };

        // Read file and compute BLAKE3 hash
        let file_bytes = match fs::read(aos_path) {
            Ok(bytes) => bytes,
            Err(e) => {
                let err_msg = format!("Failed to read file: {}", e);
                output.error(format!("  {}", err_msg));
                failed.push((aos_path.clone(), err_msg));
                continue;
            }
        };

        let hash = blake3::hash(&file_bytes).to_hex().to_string();
        let object_path = layout.object_path(&hash);

        output.kv("  Hash", &hash[..hash.len().min(16)]);
        output.kv("  Kind", &adapter_name.kind.to_string());

        if args.dry_run {
            output.info(format!("  Would copy to: {}", object_path.display()));
            output.info(format!(
                "  Would create refs at: {}",
                layout.refs_dir(&adapter_name, &args.tenant_id).display()
            ));
        } else {
            // Create object directory structure
            if let Some(parent) = object_path.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    AosError::Io(format!(
                        "Failed to create object directory {}: {}",
                        parent.display(),
                        e
                    ))
                })?;
            }

            // Copy file to objects store (if not already there)
            if !object_path.exists() {
                fs::copy(aos_path, &object_path).map_err(|e| {
                    AosError::Io(format!(
                        "Failed to copy {} to {}: {}",
                        aos_path.display(),
                        object_path.display(),
                        e
                    ))
                })?;
                output.success(format!("  Copied to: {}", object_path.display()));
            } else {
                output.info(format!(
                    "  Object already exists at: {}",
                    object_path.display()
                ));
            }

            // Create refs directory
            let refs_dir = layout.refs_dir(&adapter_name, &args.tenant_id);
            fs::create_dir_all(&refs_dir).map_err(|e| {
                AosError::Io(format!(
                    "Failed to create refs directory {}: {}",
                    refs_dir.display(),
                    e
                ))
            })?;

            // Create `current` ref (promote to current)
            store
                .update_ref(&adapter_name, &args.tenant_id, "current", &hash)
                .await
                .map_err(|e| AosError::Config(format!("Failed to create current ref: {}", e)))?;
            output.success("  Created ref: current");

            // Create `v1` version tag
            store
                .update_ref(&adapter_name, &args.tenant_id, "v1", &hash)
                .await
                .map_err(|e| AosError::Config(format!("Failed to create v1 tag: {}", e)))?;
            output.success("  Created tag: v1");
        }

        migrated.push(MigratedAdapter {
            source_path: aos_path.display().to_string(),
            adapter_name: adapter_name.to_string(),
            kind: adapter_name.kind.to_string(),
            hash: hash.clone(),
            object_path: object_path.display().to_string(),
            refs_dir: layout
                .refs_dir(&adapter_name, &args.tenant_id)
                .display()
                .to_string(),
        });

        output.blank();
    }

    // Summary
    output.section("Migration Summary");
    output.kv("Total files", &aos_files.len().to_string());
    output.kv("Migrated", &migrated.len().to_string());
    output.kv("Failed", &failed.len().to_string());

    if args.dry_run {
        output.blank();
        output.info("Dry run complete - no changes were made");
        output.info("Remove --dry-run to perform the actual migration");
    } else if !failed.is_empty() {
        output.blank();
        output.warning("Some adapters failed to migrate:");
        for (path, err) in &failed {
            output.error(format!("  {} - {}", path.display(), err));
        }
    } else {
        output.blank();
        output.success("Migration complete");
    }

    Ok(())
}

// =============================================================================
// Stack management command implementations
// =============================================================================

use adapteros_storage::{StackComponent, StackDefinition};

/// Run a stack subcommand
async fn run_stack_cmd(cmd: StackCmd, output: &OutputWriter) -> Result<()> {
    match cmd {
        StackCmd::New(args) => stack_new(args, output).await,
        StackCmd::Update(args) => stack_update(args, output).await,
        StackCmd::List(args) => stack_list(args, output).await,
        StackCmd::Show(args) => stack_show(args, output).await,
    }
}

/// Get the path to a stack definition file
fn stack_definition_path(root: &std::path::Path, tenant_id: &str, name: &str) -> PathBuf {
    root.join("stacks")
        .join(tenant_id)
        .join(name)
        .join("stack.json")
}

/// Parse a component string like "developer.aos@v1" into (AdapterName, ref_name)
fn parse_component_spec(spec: &str) -> Result<(AdapterName, String)> {
    let parts: Vec<&str> = spec.splitn(2, '@').collect();
    if parts.len() != 2 {
        return Err(AosError::Config(format!(
            "Invalid component spec '{}': expected format adapter@ref (e.g., developer.aos@v1)",
            spec
        )));
    }
    let adapter = AdapterName::parse(parts[0])
        .map_err(|e| AosError::Config(format!("Invalid adapter name '{}': {}", parts[0], e)))?;
    Ok((adapter, parts[1].to_string()))
}

/// Resolve a component's ref to its current hash using the ref store
async fn resolve_component_hash(
    store: &FsRefStore,
    adapter: &AdapterName,
    ref_name: &str,
    tenant_id: &str,
) -> Result<Option<String>> {
    store
        .get_ref(adapter, tenant_id, ref_name)
        .await
        .map_err(|e| AosError::Config(format!("Failed to resolve ref: {}", e)))
}

/// Load a stack definition from disk
fn load_stack_definition(path: &PathBuf) -> Result<StackDefinition> {
    let content = fs::read_to_string(path).map_err(|e| {
        AosError::Io(format!(
            "Failed to read stack definition {}: {}",
            path.display(),
            e
        ))
    })?;

    serde_json::from_str(&content).map_err(|e| {
        AosError::Parse(format!(
            "Failed to parse stack definition {}: {}",
            path.display(),
            e
        ))
    })
}

/// Save a stack definition to disk
fn save_stack_definition(path: &PathBuf, def: &StackDefinition) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            AosError::Io(format!(
                "Failed to create stack directory {}: {}",
                parent.display(),
                e
            ))
        })?;
    }

    let content = serde_json::to_string_pretty(def)
        .map_err(|e| AosError::Config(format!("Failed to serialize stack definition: {}", e)))?;

    fs::write(path, content).map_err(|e| {
        AosError::Io(format!(
            "Failed to write stack definition {}: {}",
            path.display(),
            e
        ))
    })?;

    Ok(())
}

/// Create a new stack definition
async fn stack_new(args: StackNewArgs, output: &OutputWriter) -> Result<()> {
    let root = default_adapter_root();
    let layout = AdapterLayout::new(&root);
    let store = FsRefStore::new(layout);
    let path = stack_definition_path(&root, &args.tenant_id, &args.name);

    if path.exists() {
        return Err(AosError::Config(format!(
            "Stack '{}' already exists at {}",
            args.name,
            path.display()
        )));
    }

    // Parse components and resolve their hashes
    let mut components = Vec::new();
    for spec in &args.components {
        let (adapter, ref_name) = parse_component_spec(spec)?;

        // Resolve the hash for this component
        let resolved_hash =
            resolve_component_hash(&store, &adapter, &ref_name, &args.tenant_id).await?;
        let hash = resolved_hash.unwrap_or_else(|| "unresolved".to_string());

        components.push(StackComponent {
            adapter,
            ref_name,
            resolved_hash: hash,
            weight: 1.0,
        });
    }

    if components.is_empty() {
        return Err(AosError::Config(
            "At least one component is required (use --components)".to_string(),
        ));
    }

    // Create stack name as AdapterName
    let stack_name = AdapterName::stack(&args.name);

    let def = StackDefinition {
        name: stack_name,
        version: "1.0.0".to_string(),
        components,
        description: args.description,
        created_at: Utc::now().to_rfc3339(),
    };

    save_stack_definition(&path, &def)?;

    output.success(format!("Created stack '{}'", args.name));
    output.blank();
    output.section("Stack Details");
    output.kv("Name", &args.name);
    output.kv("Version", "1.0.0");
    output.kv("Path", &path.display().to_string());
    output.kv("Components", &def.components.len().to_string());
    output.blank();

    output.info("Components:");
    for comp in &def.components {
        let hash_short = &comp.resolved_hash[..comp.resolved_hash.len().min(12)];
        output.kv(
            &format!("  {}", comp.adapter),
            &format!("@{} -> {}", comp.ref_name, hash_short),
        );
    }

    Ok(())
}

/// Update stack components (add/remove)
async fn stack_update(args: StackUpdateArgs, output: &OutputWriter) -> Result<()> {
    let root = default_adapter_root();
    let layout = AdapterLayout::new(&root);
    let store = FsRefStore::new(layout);
    let path = stack_definition_path(&root, &args.tenant_id, &args.name);

    if !path.exists() {
        return Err(AosError::Config(format!(
            "Stack '{}' not found at {}",
            args.name,
            path.display()
        )));
    }

    let mut def = load_stack_definition(&path)?;
    let mut added = Vec::new();
    let mut removed = Vec::new();

    // Remove components by adapter name (match on the string representation)
    for name in &args.remove {
        let initial_len = def.components.len();
        def.components
            .retain(|c| c.adapter.to_string() != *name && c.adapter.name != *name);
        if def.components.len() < initial_len {
            removed.push(name.clone());
        } else {
            output.warning(format!("Component '{}' not found in stack", name));
        }
    }

    // Add new components
    for spec in &args.add {
        let (adapter, ref_name) = parse_component_spec(spec)?;
        let adapter_str = adapter.to_string();

        // Check for duplicates
        if def.components.iter().any(|c| c.adapter == adapter) {
            output.warning(format!(
                "Component '{}' already exists, updating ref",
                adapter_str
            ));
            // Resolve new hash and update
            let resolved_hash =
                resolve_component_hash(&store, &adapter, &ref_name, &args.tenant_id).await?;
            let hash = resolved_hash.unwrap_or_else(|| "unresolved".to_string());

            for c in &mut def.components {
                if c.adapter == adapter {
                    c.ref_name = ref_name.clone();
                    c.resolved_hash = hash.clone();
                }
            }
        } else {
            // Resolve hash for new component
            let resolved_hash =
                resolve_component_hash(&store, &adapter, &ref_name, &args.tenant_id).await?;
            let hash = resolved_hash.unwrap_or_else(|| "unresolved".to_string());

            def.components.push(StackComponent {
                adapter,
                ref_name,
                resolved_hash: hash,
                weight: 1.0,
            });
            added.push(adapter_str);
        }
    }

    if added.is_empty() && removed.is_empty() {
        output.info("No changes made to stack");
        return Ok(());
    }

    save_stack_definition(&path, &def)?;

    output.success(format!("Updated stack '{}'", args.name));
    output.blank();

    if !added.is_empty() {
        output.info(format!("Added: {}", added.join(", ")));
    }
    if !removed.is_empty() {
        output.info(format!("Removed: {}", removed.join(", ")));
    }

    output.blank();
    output.kv("Total components", &def.components.len().to_string());

    Ok(())
}

/// List all stacks for a tenant
async fn stack_list(args: StackListArgs, output: &OutputWriter) -> Result<()> {
    let root = default_adapter_root();
    let stacks_dir = root.join("stacks").join(&args.tenant_id);

    if !stacks_dir.exists() {
        if args.format == "json" {
            output.json(&Vec::<StackDefinition>::new())?;
        } else {
            output.info("No stacks found");
            output.info("Create one with: aosctl aos stack new <name> --components adapter@ref");
        }
        return Ok(());
    }

    let mut stacks = Vec::new();

    if let Ok(entries) = fs::read_dir(&stacks_dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let stack_file = entry.path().join("stack.json");
                if stack_file.exists() {
                    if let Ok(def) = load_stack_definition(&stack_file) {
                        stacks.push(def);
                    }
                }
            }
        }
    }

    // Sort by name (use the string representation)
    stacks.sort_by(|a, b| a.name.name.cmp(&b.name.name));

    if args.format == "json" {
        output.json(&stacks)?;
    } else {
        if stacks.is_empty() {
            output.info("No stacks found");
            output.info("Create one with: aosctl aos stack new <name> --components adapter@ref");
            return Ok(());
        }

        output.section(format!("Stacks for tenant '{}'", args.tenant_id));
        output.blank();

        for stack in &stacks {
            output.kv(
                &stack.name.name,
                &format!("{} components", stack.components.len()),
            );
            if let Some(desc) = &stack.description {
                output.info(format!("  {}", desc));
            }
        }
    }

    Ok(())
}

/// Show stack details and resolved component hashes
async fn stack_show(args: StackShowArgs, output: &OutputWriter) -> Result<()> {
    let root = default_adapter_root();
    let path = stack_definition_path(&root, &args.tenant_id, &args.name);

    if !path.exists() {
        return Err(AosError::Config(format!(
            "Stack '{}' not found at {}",
            args.name,
            path.display()
        )));
    }

    let mut def = load_stack_definition(&path)?;

    // Re-resolve hashes for all components (in case refs have been updated)
    let layout = AdapterLayout::new(&root);
    let store = FsRefStore::new(layout);

    for comp in &mut def.components {
        if let Ok(Some(hash)) =
            resolve_component_hash(&store, &comp.adapter, &comp.ref_name, &args.tenant_id).await
        {
            comp.resolved_hash = hash;
        }
        // If resolution fails, keep the existing hash
    }

    if args.format == "json" {
        output.json(&def)?;
    } else {
        output.section(format!("Stack: {}", def.name.name));
        output.blank();

        output.kv("Version", &def.version);
        if let Some(desc) = &def.description {
            output.kv("Description", desc);
        }
        output.kv("Created", &def.created_at);
        output.blank();

        output.info(format!("Components ({}):", def.components.len()));
        for comp in &def.components {
            let hash_short = &comp.resolved_hash[..comp.resolved_hash.len().min(12)];
            let weight_str = if (comp.weight - 1.0).abs() > 0.001 {
                format!(" (weight: {:.2})", comp.weight)
            } else {
                String::new()
            };

            output.kv(
                &format!("  {}", comp.adapter),
                &format!("@{} -> {}{}", comp.ref_name, hash_short, weight_str),
            );
        }
    }

    Ok(())
}
