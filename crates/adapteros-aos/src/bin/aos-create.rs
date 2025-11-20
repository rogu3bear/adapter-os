#!/usr/bin/env cargo
//! AOS Adapter Archive Creator
//!
//! Command-line tool to package adapter directories into .aos archives.
//! Replaces the Python create_aos_adapter.py script with a native Rust implementation.
//!
//! ## Usage
//!
//! ```bash
//! # Create binary format .aos archive
//! aos-create adapters/code_lang_v1/ -o code-assistant.aos
//!
//! # Create with verbose output
//! aos-create adapters/my_adapter/ -o my_adapter.aos -v
//!
//! # Verify after creation
//! aos-create adapters/my_adapter/ -o my_adapter.aos --verify
//!
//! # Specify adapter ID
//! aos-create adapters/my_adapter/ -o my_adapter.aos --adapter-id tenant/domain/purpose/r001
//!
//! # Dry run (preview without creating)
//! aos-create adapters/my_adapter/ -o my_adapter.aos --dry-run
//! ```

use adapteros_aos::AOS2Writer;
use adapteros_core::{AosError, Result};
use anyhow::Context;
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{error, info};

/// AOS Adapter Archive Creator
#[derive(Parser, Debug)]
#[command(name = "aos-create")]
#[command(about = "Create .aos adapter archives from directory structure", long_about = None)]
#[command(version)]
struct Args {
    /// Input directory containing manifest.json and weights.safetensors
    #[arg(value_name = "INPUT_DIR")]
    input_dir: PathBuf,

    /// Output .aos file path
    #[arg(short = 'o', long = "output", value_name = "OUTPUT_FILE")]
    output: Option<PathBuf>,

    /// Archive format (binary only for now)
    #[arg(short = 'f', long = "format", default_value = "binary")]
    format: ArchiveFormat,

    /// Override adapter ID (semantic naming: tenant/domain/purpose/revision)
    #[arg(long = "adapter-id")]
    adapter_id: Option<String>,

    /// Verify the created .aos file
    #[arg(long = "verify")]
    verify: bool,

    /// Dry run - preview without creating file
    #[arg(long = "dry-run")]
    dry_run: bool,

    /// Verbose output
    #[arg(short = 'v', long = "verbose")]
    verbose: bool,
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum ArchiveFormat {
    Binary,
    // Zip support planned for future
}

/// Adapter manifest structure (compatible with Python version)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AdapterManifest {
    /// Format version (always 2 for AOS 2.0)
    #[serde(default = "default_format_version")]
    format_version: u32,

    /// Semantic adapter ID
    #[serde(default)]
    adapter_id: String,

    /// Adapter name
    #[serde(default)]
    name: String,

    /// Version string
    #[serde(default)]
    version: String,

    /// LoRA rank
    #[serde(default = "default_rank")]
    rank: u32,

    /// LoRA alpha
    #[serde(default = "default_alpha")]
    alpha: f32,

    /// Base model identifier
    #[serde(default)]
    base_model: String,

    /// Target modules for LoRA
    #[serde(default = "default_target_modules")]
    target_modules: Vec<String>,

    /// Creation timestamp
    #[serde(default)]
    created_at: String,

    /// BLAKE3 hash of weights (computed during packaging)
    #[serde(default)]
    weights_hash: String,

    /// Training configuration (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    training_config: Option<TrainingConfig>,

    /// Additional metadata
    #[serde(default)]
    metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TrainingConfig {
    rank: u32,
    alpha: f32,
    learning_rate: f64,
    batch_size: u32,
    epochs: u32,
    hidden_dim: u32,
    #[serde(default)]
    dropout: f32,
    #[serde(default)]
    weight_decay: f64,
}

fn default_format_version() -> u32 {
    2
}

fn default_rank() -> u32 {
    16
}

fn default_alpha() -> f32 {
    32.0
}

fn default_target_modules() -> Vec<String> {
    vec![
        "q_proj".to_string(),
        "k_proj".to_string(),
        "v_proj".to_string(),
        "o_proj".to_string(),
    ]
}

impl AdapterManifest {
    /// Update manifest for AOS packaging
    fn prepare_for_aos(&mut self, weights_hash: &str, adapter_id_override: Option<&str>) {
        // Set format version
        self.format_version = 2;

        // Update weights hash
        self.weights_hash = weights_hash.to_string();

        // Override adapter ID if provided
        if let Some(id) = adapter_id_override {
            self.adapter_id = id.to_string();
        }

        // Generate adapter ID if missing
        if self.adapter_id.is_empty() {
            self.adapter_id = self.generate_adapter_id();
        }

        // Set name from adapter ID if missing
        if self.name.is_empty() {
            self.name = self
                .adapter_id
                .split('/')
                .last()
                .unwrap_or("Unnamed Adapter")
                .to_string();
        }

        // Sync training_config with top-level rank/alpha
        if let Some(config) = &self.training_config {
            if config.rank != self.rank {
                self.rank = config.rank;
            }
            if config.alpha != self.alpha {
                self.alpha = config.alpha;
            }
        }

        // Ensure required fields have defaults
        if self.version.is_empty() {
            self.version = "1.0.0".to_string();
        }
        if self.base_model.is_empty() {
            self.base_model = "qwen2.5-7b".to_string();
        }
        if self.created_at.is_empty() {
            self.created_at = chrono::Utc::now().to_rfc3339();
        }
        if self.target_modules.is_empty() {
            self.target_modules = default_target_modules();
        }
    }

    /// Generate semantic adapter ID from name
    fn generate_adapter_id(&self) -> String {
        let purpose = if self.name.is_empty() {
            "adapter".to_string()
        } else {
            self.name.to_lowercase().replace(' ', "-").replace('_', "-")
        };
        format!("default/general/{}/r001", purpose)
    }

    /// Validate manifest fields
    fn validate(&self) -> Result<()> {
        if self.adapter_id.is_empty() {
            return Err(AosError::Validation("adapter_id is required".to_string()));
        }

        // Validate semantic naming format
        let parts: Vec<&str> = self.adapter_id.split('/').collect();
        if parts.len() != 4 {
            return Err(AosError::Validation(format!(
                "adapter_id must follow tenant/domain/purpose/revision format, got: {}",
                self.adapter_id
            )));
        }

        if self.version.is_empty() {
            return Err(AosError::Validation("version is required".to_string()));
        }

        if self.rank == 0 {
            return Err(AosError::Validation("rank must be > 0".to_string()));
        }

        if self.alpha <= 0.0 {
            return Err(AosError::Validation("alpha must be > 0".to_string()));
        }

        Ok(())
    }
}

/// Load manifest from JSON file
fn load_manifest(path: &Path) -> Result<AdapterManifest> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        AosError::Io(format!(
            "Failed to read manifest at {}: {}",
            path.display(),
            e
        ))
    })?;

    let manifest: AdapterManifest = serde_json::from_str(&content).map_err(|e| {
        AosError::Validation(format!(
            "Failed to parse manifest JSON at {}: {}",
            path.display(),
            e
        ))
    })?;

    Ok(manifest)
}

/// Load weights from safetensors file
fn load_weights(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|e| {
        AosError::Io(format!(
            "Failed to read weights at {}: {}",
            path.display(),
            e
        ))
    })
}

/// Compute BLAKE3 hash of data
fn compute_blake3_hash(data: &[u8]) -> String {
    blake3::hash(data).to_hex().to_string()
}

/// Create .aos archive from adapter directory
fn create_aos_archive(
    input_dir: &Path,
    output_path: &Path,
    adapter_id_override: Option<&str>,
    dry_run: bool,
    verbose: bool,
) -> Result<(String, AdapterManifest)> {
    if verbose {
        info!("📦 Packaging {}", input_dir.display());
    }

    // Validate input directory
    if !input_dir.exists() {
        return Err(AosError::Validation(format!(
            "Input directory not found: {}",
            input_dir.display()
        )));
    }

    if !input_dir.is_dir() {
        return Err(AosError::Validation(format!(
            "Input path is not a directory: {}",
            input_dir.display()
        )));
    }

    // Check for required files
    let manifest_path = input_dir.join("manifest.json");
    let weights_path = input_dir.join("weights.safetensors");

    if !manifest_path.exists() {
        return Err(AosError::Validation(format!(
            "Missing manifest.json in {}",
            input_dir.display()
        )));
    }

    if !weights_path.exists() {
        return Err(AosError::Validation(format!(
            "Missing weights.safetensors in {}",
            input_dir.display()
        )));
    }

    // Load manifest
    let mut manifest = load_manifest(&manifest_path)?;

    // Load weights
    let weights_data = load_weights(&weights_path)?;

    // Compute BLAKE3 hash
    let weights_hash = compute_blake3_hash(&weights_data);

    if verbose {
        info!("Weights hash: {}...", &weights_hash[..16]);
    }

    // Update manifest for AOS format
    manifest.prepare_for_aos(&weights_hash, adapter_id_override);

    // Validate manifest
    manifest.validate()?;

    if verbose {
        info!("Adapter ID: {}", manifest.adapter_id);
        info!("Rank: {}, Alpha: {}", manifest.rank, manifest.alpha);
        info!("Base model: {}", manifest.base_model);
    }

    if dry_run {
        println!("🔍 Dry run - would create:");
        println!("   Output: {}", output_path.display());
        println!("   Adapter ID: {}", manifest.adapter_id);
        println!("   Rank: {}", manifest.rank);
        println!("   Alpha: {}", manifest.alpha);
        println!(
            "   Weights size: {:.2} MB",
            weights_data.len() as f64 / 1024.0 / 1024.0
        );
        println!("   Hash: {}...", &weights_hash[..16]);
        return Ok((weights_hash, manifest));
    }

    // Create output directory if needed
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            AosError::Io(format!(
                "Failed to create output directory {}: {}",
                parent.display(),
                e
            ))
        })?;
    }

    // Write .aos archive
    let writer = AOS2Writer::new();
    let total_size = writer.write_archive(output_path, &manifest, &weights_data)?;

    if verbose {
        info!("✅ Created {}", output_path.display());
        info!("   Size: {:.2} MB", total_size as f64 / 1024.0 / 1024.0);
        info!("   Hash: {}...", &weights_hash[..16]);
        info!("   ID: {}", manifest.adapter_id);
        info!("   Rank: {}", manifest.rank);
    } else {
        println!("Created: {}", output_path.display());
    }

    Ok((weights_hash, manifest))
}

/// Verify .aos archive
fn verify_aos_archive(path: &Path, verbose: bool) -> Result<()> {
    if verbose {
        info!("🔍 Verifying {}", path.display());
    }

    // Read header
    let (manifest_offset, manifest_len) = AOS2Writer::read_header(path)?;

    // Read entire file
    let file_data = std::fs::read(path)
        .map_err(|e| AosError::Io(format!("Failed to read .aos file: {}", e)))?;

    // Extract weights
    let weights_start = 8;
    let weights_end = manifest_offset as usize;
    if weights_end > file_data.len() {
        return Err(AosError::Validation(format!(
            "Invalid manifest_offset: {} exceeds file size {}",
            manifest_offset,
            file_data.len()
        )));
    }
    let weights_data = &file_data[weights_start..weights_end];

    // Extract manifest
    let manifest_start = manifest_offset as usize;
    let manifest_end = manifest_start + manifest_len as usize;
    if manifest_end > file_data.len() {
        return Err(AosError::Validation(format!(
            "Invalid manifest_len: {} extends beyond file size",
            manifest_len
        )));
    }
    let manifest_json = &file_data[manifest_start..manifest_end];

    // Parse manifest
    let manifest: AdapterManifest = serde_json::from_slice(manifest_json)
        .map_err(|e| AosError::Validation(format!("Failed to parse manifest: {}", e)))?;

    // Compute hash of weights
    let computed_hash = compute_blake3_hash(weights_data);

    // Verify hash matches
    if computed_hash != manifest.weights_hash {
        return Err(AosError::Validation(format!(
            "Hash mismatch: computed {}... != stored {}...",
            &computed_hash[..16],
            &manifest.weights_hash[..16]
        )));
    }

    if verbose {
        info!("✅ Valid .aos file");
        info!("   Format version: {}", manifest.format_version);
        info!("   Adapter ID: {}", manifest.adapter_id);
        info!(
            "   Weights size: {:.2} MB",
            weights_data.len() as f64 / 1024.0 / 1024.0
        );
        info!("   Hash verified: {}...", &computed_hash[..16]);
    } else {
        println!("✅ Valid .aos file: {}", path.display());
    }

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Initialize logging
    let log_level = if args.verbose { "info" } else { "warn" };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level)),
        )
        .with_target(false)
        .without_time()
        .init();

    // Determine output path
    let output_path = if let Some(output) = args.output {
        output
    } else {
        let adapter_name = args
            .input_dir
            .file_name()
            .and_then(|n| n.to_str())
            .context("Invalid input directory name")?;
        PathBuf::from("adapters").join(format!("{}.aos", adapter_name))
    };

    // Create .aos archive
    let result = create_aos_archive(
        &args.input_dir,
        &output_path,
        args.adapter_id.as_deref(),
        args.dry_run,
        args.verbose,
    );

    match result {
        Ok((_hash, _manifest)) => {
            // Verify if requested
            if args.verify && !args.dry_run {
                if let Err(e) = verify_aos_archive(&output_path, args.verbose) {
                    error!("Verification failed: {}", e);
                    return Err(e.into());
                }
            }

            Ok(())
        }
        Err(e) => {
            error!("Failed to create .aos archive: {}", e);
            Err(e.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_manifest() -> AdapterManifest {
        AdapterManifest {
            format_version: 2,
            adapter_id: "".to_string(),
            name: "test-adapter".to_string(),
            version: "1.0.0".to_string(),
            rank: 16,
            alpha: 32.0,
            base_model: "qwen2.5-7b".to_string(),
            target_modules: default_target_modules(),
            created_at: "2025-01-19T12:00:00Z".to_string(),
            weights_hash: "".to_string(),
            training_config: None,
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn test_generate_adapter_id() {
        let manifest = create_test_manifest();
        let id = manifest.generate_adapter_id();
        assert_eq!(id, "default/general/test-adapter/r001");
    }

    #[test]
    fn test_prepare_for_aos() {
        let mut manifest = create_test_manifest();
        manifest.prepare_for_aos("test_hash_123", None);

        assert_eq!(manifest.format_version, 2);
        assert_eq!(manifest.weights_hash, "test_hash_123");
        assert!(!manifest.adapter_id.is_empty());
    }

    #[test]
    fn test_validate_manifest() {
        let mut manifest = create_test_manifest();
        manifest.adapter_id = "tenant/domain/purpose/r001".to_string();

        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn test_validate_manifest_invalid_id() {
        let mut manifest = create_test_manifest();
        manifest.adapter_id = "invalid-id".to_string();

        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_compute_blake3_hash() {
        let data = b"test data";
        let hash = compute_blake3_hash(data);
        assert_eq!(hash.len(), 64); // BLAKE3 produces 32 bytes = 64 hex chars
    }
}
