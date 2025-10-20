//! Adapter migration commands
//!
//! Migrate existing adapters to .aos format

use crate::output::OutputWriter;
use adapteros_core::B3Hash;
use adapteros_lora_worker::training::{TrainingConfig, TrainingExample};
use adapteros_single_file_adapter::{
    format::{LineageInfo, Mutation, SingleFileAdapter}, 
    SingleFileAdapterPackager
};
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use toml;

#[derive(Debug, Parser, Clone)]
#[command(name = "migrate")]
pub struct MigrateArgs {
    #[command(subcommand)]
    pub cmd: MigrateCmd,
}

#[derive(Debug, Subcommand, Clone)]
pub enum MigrateCmd {
    /// Migrate adapter directory to .aos file
    Adapter(AdapterMigrateArgs),
}

#[derive(Debug, Parser, Clone)]
pub struct AdapterMigrateArgs {
    /// Source adapter directory
    #[arg(long)]
    pub source: PathBuf,
    
    /// Output .aos file path
    #[arg(long)]
    pub output: PathBuf,
    
    /// Adapter ID for .aos file
    #[arg(long)]
    pub adapter_id: Option<String>,
    
    /// Version for .aos file
    #[arg(long, default_value = "1.0.0")]
    pub version: String,
}

pub async fn run(args: MigrateArgs, output: &OutputWriter) -> Result<()> {
    match args.cmd {
        MigrateCmd::Adapter(mig) => migrate_adapter(mig, output).await,
    }
}

async fn migrate_adapter(args: AdapterMigrateArgs, output: &OutputWriter) -> Result<()> {
    output.info("Migrating adapter to .aos format");
    
    // Resolve paths
    let weights_path = args.source.join("weights.safetensors");
    if !weights_path.exists() {
        return Err(anyhow::anyhow!("Weights file not found: {:?}", weights_path));
    }
    
    let manifest_path = args.source.join("manifest.json");
    let config_path = args.source.join("config.toml");
    
    // Read weights
    let weights = fs::read(&weights_path)
        .with_context(|| format!("reading weights from {:?}", weights_path))?;
    
    let hash = B3Hash::hash(&weights);
    let weights_hash = format!("b3:{}", hash.to_hex());
    
    // Read training data (assume jsonl in dataset dir or from manifest)
    let training_data = load_training_data_from_source(&args.source).await?;
    
    // Read config
    let config = if config_path.exists() {
        let config_str = fs::read_to_string(&config_path)?;
        toml::from_str::<TrainingConfig>(&config_str)?
    } else {
        TrainingConfig::default()
    };
    
    let adapter_id = args.adapter_id.unwrap_or_else(|| {
        args.source.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("migrated_adapter")
            .to_string()
    });
    
    // Create lineage
    let lineage = LineageInfo {
        adapter_id: adapter_id.clone(),
        version: args.version,
        parent_version: None,
        parent_hash: None,
        mutations: vec![],
        quality_delta: 0.0,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    
    // Create manifest
    let manifest = format::AdapterManifest {
        adapter_id: adapter_id.clone(),
        version: args.version.clone(),
        rank: config.rank as u32,
        alpha: config.alpha,
        base_model: "qwen2.5-7b".to_string(), // default
        category: "code".to_string(),
        scope: "global".to_string(),
        tier: "persistent".to_string(),
        target_modules: vec!["q_proj".to_string(), "k_proj".to_string()], // default
        created_at: chrono::Utc::now().to_rfc3339(),
        weights_hash: weights_hash.clone(),
        training_data_hash: B3Hash::hash(&serde_json::to_vec(&training_data)?).to_hex(),
        metadata: HashMap::new(),
    };
    
    // Create .aos adapter
    let adapter = SingleFileAdapter {
        manifest,
        weights,
        training_data,
        config,
        lineage,
        signature: None,
    };
    
    // Save .aos file
    SingleFileAdapterPackager::save(&adapter, &args.output).await?;
    
    output.success(format!("Migrated adapter to .aos: {}", args.output.display()));
    output.kv("Weights Hash", &weights_hash);
    output.kv("Training Examples", &training_data.len().to_string());
    
    Ok(())
}

async fn load_training_data_from_source(source: &Path) -> Result<Vec<TrainingExample>> {
    // Implementation: load from jsonl files or manifest entries
    // For now, placeholder
    Ok(vec![])
}

// Helper to load from manifest or dataset files
// This would parse the manifest and load the actual jsonl files
