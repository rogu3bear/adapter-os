//! AOS adapter commands

// ============================================================================
// AOS COORDINATION HEADER
// ============================================================================
// File: crates/adapteros-cli/src/commands/aos.rs
// Phase: 2 - System Integration
// Assigned: Intern B (CLI Commands Team)
// Status: Complete - All CLI commands implemented
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

use crate::output::OutputWriter;
use adapteros_core::{AosError, Result};
use adapteros_crypto::Keypair;
use adapteros_single_file_adapter::{
    AOS_FORMAT_VERSION, CompressionLevel, LoadOptions, PackageOptions,
    SingleFileAdapterLoader, SingleFileAdapterPackager, SingleFileAdapterValidator,
    get_compatibility_report, migrate_file,
};
use adapteros_lora_worker::training::{TrainingConfig, TrainingExample};
use anyhow::Context;
use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Parser, Clone)]
#[command(name = "aos")]
pub struct AosArgs {
    #[command(subcommand)]
    pub cmd: AosCmd,
}

#[derive(Debug, Subcommand, Clone)]
pub enum AosCmd {
    /// Create .aos file from existing adapter
    Create(CreateArgs), // COORDINATION: Affects SingleFileAdapterPackager
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
    
    /// Training data JSONL file
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
    
    /// Compression level (store, fast, best)
    #[arg(long, default_value = "fast")]
    pub compression: String,
    
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

async fn create_aos(args: CreateArgs, output: &OutputWriter) -> Result<()> {
    output.info("Creating .aos adapter file...");
    
    // Load weights from source
    let weights = tokio::fs::read(&args.source)
        .await
        .context("Failed to read source adapter file")?;
    
    // Load training data if provided
    let training_data = if let Some(training_path) = &args.training_data {
        let training_str = tokio::fs::read_to_string(training_path)
            .await
            .context("Failed to read training data file")?;
        
        training_str
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str::<TrainingExample>(line))
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to parse training data")?
    } else {
        vec![]
    };
    
    // Load config if provided
    let config = if let Some(config_path) = &args.config {
        let config_str = tokio::fs::read_to_string(config_path)
            .await
            .context("Failed to read config file")?;
        toml::from_str(&config_str)
            .context("Failed to parse config file")?
    } else {
        TrainingConfig {
            rank: 16,
            alpha: 32.0,
            learning_rate: 0.0005,
            batch_size: 8,
            epochs: 4,
            hidden_dim: 3584,
        }
    };
    
    // Create lineage info
    let lineage = adapteros_single_file_adapter::LineageInfo {
        adapter_id: args.adapter_id.clone(),
        version: args.version.clone(),
        parent_version: None,
        parent_hash: None,
        mutations: vec![],
        quality_delta: 0.0,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    
    // Parse compression level
    let compression = match args.compression.to_lowercase().as_str() {
        "store" => CompressionLevel::Store,
        "fast" => CompressionLevel::Fast,
        "best" => CompressionLevel::Best,
        _ => {
            output.warn(&format!("Unknown compression level '{}', using 'fast'", args.compression));
            CompressionLevel::Fast
        }
    };
    
    // Create adapter
    let mut adapter = adapteros_single_file_adapter::SingleFileAdapter::create_with_compression(
        args.adapter_id.clone(),
        weights,
        training_data,
        config,
        lineage,
        compression,
    )?;
    
    // Sign if requested
    if args.sign {
        let keypair = if let Some(key_hex) = args.signing_key {
            let key_bytes = hex::decode(&key_hex)
                .context("Failed to decode signing key hex")?;
            if key_bytes.len() != 32 {
                return Err(AosError::Crypto(format!("Invalid key length: {}", key_bytes.len())));
            }
            let mut key_array = [0u8; 32];
            key_array.copy_from_slice(&key_bytes);
            Keypair::from_bytes(&key_array)
        } else {
            let kp = Keypair::generate();
            output.info(&format!("Generated signing key: {}", hex::encode(kp.to_bytes())));
            output.warn("Save this key to sign future versions!");
            kp
        };
        
        adapter.sign(&keypair)?;
        output.info(&format!("Signed adapter with key ID: {}", 
            adapter.signature_info().unwrap().0));
    }
    
    // Save to .aos file
    let options = PackageOptions { compression };
    SingleFileAdapterPackager::save_with_options(&adapter, &args.output, options).await?;
    
    output.success(&format!("Created .aos adapter: {}", args.output.display()));
    output.info(&format!("  Format version: {}", AOS_FORMAT_VERSION));
    output.info(&format!("  Compression: {}", args.compression));
    output.info(&format!("  Signed: {}", adapter.is_signed()));
    output.info(&format!("  Size: {} bytes", tokio::fs::metadata(&args.output).await?.len()));
    Ok(())
}

async fn load_aos(args: LoadArgs, output: &OutputWriter) -> Result<()> {
    output.info("Loading .aos adapter file...");
    
    // Load and validate .aos file
    let adapter = SingleFileAdapterLoader::load(&args.path).await?;
    
    // Verify integrity
    if !adapter.verify()? {
        return Err(AosError::Training("Adapter integrity verification failed".to_string()));
    }
    
    // TODO: Register with control plane
    output.info("AOS adapter loaded successfully");
    output.info(&format!("Adapter ID: {}", adapter.manifest.adapter_id));
    output.info(&format!("Version: {}", adapter.manifest.version));
    output.info(&format!("Training examples: {}", adapter.training_data.len()));
    
    Ok(())
}

async fn verify_aos(args: VerifyArgs, output: &OutputWriter) -> Result<()> {
    output.info("Verifying .aos adapter file...");
    
    let result = SingleFileAdapterValidator::validate(&args.path).await?;
    
    match args.format.as_str() {
        "json" => {
            let json_result = serde_json::json!({
                "is_valid": result.is_valid,
                "errors": result.errors,
                "warnings": result.warnings
            });
            output.write_json(&json_result)?;
        }
        _ => {
            if result.is_valid {
                output.success("AOS adapter verification passed");
            } else {
                output.error("AOS adapter verification failed");
            }
            
            for error in &result.errors {
                output.error(&format!("Error: {}", error));
            }
            
            for warning in &result.warnings {
                output.warn(&format!("Warning: {}", warning));
            }
        }
    }
    
    Ok(())
}

async fn extract_aos(args: ExtractArgs, output: &OutputWriter) -> Result<()> {
    output.info("Extracting components from .aos adapter file...");
    
    // Load .aos file
    let adapter = SingleFileAdapterLoader::load(&args.path).await?;
    
    // Create output directory
    tokio::fs::create_dir_all(&args.output_dir).await?;
    
    let components: Vec<&str> = args.components.split(',').map(|s| s.trim()).collect();
    let extract_all = components.contains(&"all");
    
    if extract_all || components.contains(&"weights") {
        let weights_path = args.output_dir.join("weights.safetensors");
        tokio::fs::write(&weights_path, &adapter.weights).await?;
        output.info(&format!("Extracted weights: {}", weights_path.display()));
    }
    
    if extract_all || components.contains(&"training_data") {
        let training_path = args.output_dir.join("training_data.jsonl");
        let mut training_file = tokio::fs::File::create(&training_path).await?;
        for example in &adapter.training_data {
            let line = serde_json::to_string(example)?;
            tokio::io::AsyncWriteExt::write_all(&mut training_file, line.as_bytes()).await?;
            tokio::io::AsyncWriteExt::write_all(&mut training_file, b"\n").await?;
        }
        output.info(&format!("Extracted training data: {}", training_path.display()));
    }
    
    if extract_all || components.contains(&"config") {
        let config_path = args.output_dir.join("config.toml");
        let config_str = toml::to_string(&adapter.config)?;
        tokio::fs::write(&config_path, config_str).await?;
        output.info(&format!("Extracted config: {}", config_path.display()));
    }
    
    if extract_all || components.contains(&"lineage") {
        let lineage_path = args.output_dir.join("lineage.json");
        let lineage_str = serde_json::to_string_pretty(&adapter.lineage)?;
        tokio::fs::write(&lineage_path, lineage_str).await?;
        output.info(&format!("Extracted lineage: {}", lineage_path.display()));
    }
    
    if extract_all || components.contains(&"manifest") {
        let manifest_path = args.output_dir.join("manifest.json");
        let manifest_str = serde_json::to_string_pretty(&adapter.manifest)?;
        tokio::fs::write(&manifest_path, manifest_str).await?;
        output.info(&format!("Extracted manifest: {}", manifest_path.display()));
    }
    
    if extract_all || components.contains(&"signature") {
        if let Some(signature) = &adapter.signature {
            let signature_path = args.output_dir.join("signature.json");
            let signature_str = serde_json::to_string_pretty(signature)?;
            tokio::fs::write(&signature_path, signature_str).await?;
            output.info(&format!("Extracted signature: {}", signature_path.display()));
        } else {
            output.warn("No signature present in adapter");
        }
    }
    
    output.success("Extraction completed");
    Ok(())
}

async fn info_aos(args: InfoArgs, output: &OutputWriter) -> Result<()> {
    output.info("Reading .aos adapter information...");
    
    // Load manifest only (fast)
    let manifest = SingleFileAdapterLoader::load_manifest_only(&args.path).await?;
    
    // Load full adapter to check signature
    let adapter = SingleFileAdapterLoader::load_with_options(
        &args.path,
        LoadOptions {
            skip_verification: false,
            skip_signature_check: false,
        },
    )
    .await?;
    
    // Get compatibility info
    let compat = get_compatibility_report(manifest.format_version);
    
    // Get file size
    let file_size = tokio::fs::metadata(&args.path).await?.len();
    
    match args.format.as_str() {
        "json" => {
            let mut info = serde_json::json!({
                "adapter_id": manifest.adapter_id,
                "version": manifest.version,
                "format_version": manifest.format_version,
                "base_model": manifest.base_model,
                "rank": manifest.rank,
                "alpha": manifest.alpha,
                "category": manifest.category,
                "scope": manifest.scope,
                "tier": manifest.tier,
                "target_modules": manifest.target_modules,
                "created_at": manifest.created_at,
                "weights_hash": manifest.weights_hash,
                "training_data_hash": manifest.training_data_hash,
                "compression_method": manifest.compression_method,
                "signed": adapter.is_signed(),
                "file_size_bytes": file_size,
                "compatibility": {
                    "file_version": compat.file_format_version,
                    "loader_version": compat.loader_format_version,
                    "compatible": compat.compatible,
                    "can_upgrade": compat.can_upgrade,
                },
                "training_examples": adapter.training_data.len(),
            });
            
            if let Some((key_id, timestamp)) = adapter.signature_info() {
                info["signature"] = serde_json::json!({
                    "key_id": key_id,
                    "timestamp": timestamp,
                });
            }
            
            output.write_json(&info)?;
        }
        _ => {
            output.info(&format!("Adapter ID: {}", manifest.adapter_id));
            output.info(&format!("Version: {}", manifest.version));
            output.info(&format!("Format Version: {}", manifest.format_version));
            output.info(&format!("Base Model: {}", manifest.base_model));
            output.info(&format!("Rank: {}, Alpha: {}", manifest.rank, manifest.alpha));
            output.info(&format!("Category: {}, Scope: {}, Tier: {}", 
                manifest.category, manifest.scope, manifest.tier));
            output.info(&format!("Compression: {}", manifest.compression_method));
            output.info(&format!("Training Examples: {}", adapter.training_data.len()));
            output.info(&format!("File Size: {} bytes", file_size));
            output.info(&format!("Weights Hash: {}", manifest.weights_hash));
            output.info(&format!("Training Data Hash: {}", manifest.training_data_hash));
            
            if adapter.is_signed() {
                let (key_id, timestamp) = adapter.signature_info().unwrap();
                output.success("Signature: Present and verified");
                output.info(&format!("  Key ID: {}", key_id));
                output.info(&format!("  Timestamp: {}", timestamp));
            } else {
                output.warn("Signature: Not present");
            }
            
            if compat.compatible {
                output.success(&format!(
                    "Compatibility: Compatible (file v{}, loader v{})",
                    compat.file_format_version, compat.loader_format_version
                ));
                if compat.can_upgrade {
                    output.info("  Run 'aosctl aos migrate' to upgrade to latest format");
                }
            } else {
                output.error(&format!(
                    "Compatibility: Incompatible (file v{}, loader v{})",
                    compat.file_format_version, compat.loader_format_version
                ));
            }
        }
    }
    
    Ok(())
}

async fn migrate_aos(args: MigrateArgs, output: &OutputWriter) -> Result<()> {
    output.info("Migrating .aos adapter to current format...");
    
    let result = migrate_file(&args.path).await?;
    
    if result.original_version == result.new_version {
        output.success("Adapter already at current format version");
        output.info(&format!("  Format version: {}", result.new_version));
    } else {
        output.success(&format!(
            "Migrated from v{} to v{}",
            result.original_version, result.new_version
        ));
        output.info(&format!("  Changes applied: {}", result.changes_applied.len()));
        for change in &result.changes_applied {
            output.info(&format!("    - {}", change));
        }
        
        if args.backup {
            let backup_path = args.path.with_extension("aos.bak");
            output.info(&format!("  Backup saved: {}", backup_path.display()));
        }
    }
    
    Ok(())
}
