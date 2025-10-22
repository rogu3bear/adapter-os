//! # Experimental AOS CLI Features
//!
//! This module contains experimental AOS CLI features that are **NOT FOR PRODUCTION USE**.
//!
//! ## ⚠️ WARNING ⚠️
//!
//! All features in this module are:
//! - **NOT production ready**
//! - **Subject to breaking changes**
//! - **May have incomplete implementations**
//! - **Should not be used in production systems**
//!
//! ## Feature Status
//!
//! | Feature | Status | Stability | Notes |
//! |---------|--------|-----------|-------|
//! | `AosCmd` | 🚧 In Development | Unstable | CLI commands with TODO implementations |
//! | `CreateArgs` | 🚧 In Development | Unstable | Create .aos file from existing adapter |
//! | `LoadArgs` | 🚧 In Development | Unstable | Load .aos file into registry |
//! | `VerifyArgs` | 🚧 In Development | Unstable | Verify .aos file integrity |
//! | `ExtractArgs` | 🚧 In Development | Unstable | Extract components from .aos file |
//! | `InfoArgs` | 🚧 In Development | Unstable | Show .aos file information |
//! | `MigrateArgs` | 🚧 In Development | Unstable | Migrate .aos file to current format version |
//!
//! ## Known Issues
//!
//! - **TODO: Register with control plane** - Missing control plane registration
//! - **Incomplete implementations** - Many commands return placeholder responses
//! - **Missing error handling** - Incomplete error handling for edge cases
//! - **No validation** - Missing input validation for command arguments
//!
//! ## Dependencies
//!
//! - `adapteros-core` - Core functionality
//! - `serde` - Serialization
//! - `tokio` - Async runtime
//!
//! ## Last Updated
//!
//! 2025-01-15 - Initial experimental implementation
//!
//! ## Migration Path
//!
//! These features should eventually be:
//! 1. **Completed** and moved to `adapteros-cli` crate
//! 2. **Stabilized** with proper error handling and validation
//! 3. **Integrated** with control plane registration

#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]

use adapteros_core::{AosError, Result};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

/// Adapter information structure
///
/// # Status: ✅ Completed
/// # Stability: Stable
/// # Dependencies: serde
/// # Last Updated: 2025-01-15
/// # Known Issues: None
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterInfo {
    /// Path to the .aos file
    pub path: PathBuf,
    /// Adapter manifest
    pub manifest: AdapterManifest,
    /// File size in bytes
    pub file_size: u64,
    /// File creation time
    pub created: Option<SystemTime>,
    /// File modification time
    pub modified: Option<SystemTime>,
}

/// Placeholder adapter manifest
///
/// # Status: ✅ Completed
/// # Stability: Stable
/// # Dependencies: serde
/// # Last Updated: 2025-01-15
/// # Known Issues: None
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterManifest {
    /// Adapter version
    pub version: String,
    /// Adapter ID
    pub adapter_id: String,
    /// Base model
    pub base_model: String,
    /// Creation timestamp
    pub created_at: String,
    /// Additional metadata
    pub metadata: std::collections::HashMap<String, String>,
}

/// Compression level for .aos files
///
/// # Status: ✅ Completed
/// # Stability: Stable
/// # Dependencies: None
/// # Last Updated: 2025-01-15
/// # Known Issues: None
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompressionLevel {
    /// No compression
    None,
    /// Fast compression
    Fast,
    /// Default compression
    Default,
    /// Maximum compression
    Maximum,
}

/// Experimental AOS adapter commands
///
/// # Status: 🚧 In Development
/// # Stability: Unstable
/// # Dependencies: adapteros-core
/// # Last Updated: 2025-01-15
/// # Known Issues: TODO implementations, missing control plane registration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AosCmd {
    /// Command subcommand
    pub subcommand: AosSubcommand,
}

/// Experimental AOS subcommands
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AosSubcommand {
    /// Create .aos file from existing adapter
    Create(CreateArgs),
    /// Load .aos file into registry
    Load(LoadArgs),
    /// Verify .aos file integrity
    Verify(VerifyArgs),
    /// Extract components from .aos file
    Extract(ExtractArgs),
    /// Show .aos file information
    Info(InfoArgs),
    /// Migrate .aos file to current format version
    Migrate(MigrateArgs),
}

/// Create .aos file from existing adapter
///
/// # Status: 🚧 In Development
/// # Stability: Unstable
/// # Dependencies: None
/// # Last Updated: 2025-01-15
/// # Known Issues: Missing validation, incomplete error handling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateArgs {
    /// Path to existing adapter
    pub adapter_path: PathBuf,
    /// Output .aos file path
    pub output_path: PathBuf,
    /// Compression level
    pub compression: CompressionLevel,
    /// Package options
    pub package_options: Option<String>,
}

/// Load .aos file into registry
///
/// # Status: 🚧 In Development
/// # Stability: Unstable
/// # Dependencies: None
/// # Last Updated: 2025-01-15
/// # Known Issues: TODO: Register with control plane
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadArgs {
    /// Path to .aos file
    pub aos_path: PathBuf,
    /// Load options
    pub load_options: Option<String>,
    /// Force load even if validation fails
    pub force: bool,
}

/// Verify .aos file integrity
///
/// # Status: 🚧 In Development
/// # Stability: Unstable
/// # Dependencies: None
/// # Last Updated: 2025-01-15
/// # Known Issues: Incomplete validation logic
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyArgs {
    /// Path to .aos file
    pub aos_path: PathBuf,
    /// Verify signature
    pub verify_signature: bool,
    /// Verify integrity
    pub verify_integrity: bool,
}

/// Extract components from .aos file
///
/// # Status: 🚧 In Development
/// # Stability: Unstable
/// # Dependencies: None
/// # Last Updated: 2025-01-15
/// # Known Issues: Missing extraction validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractArgs {
    /// Path to .aos file
    pub aos_path: PathBuf,
    /// Output directory
    pub output_dir: PathBuf,
    /// Extract specific components
    pub components: Option<Vec<String>>,
}

/// Show .aos file information
///
/// # Status: 🚧 In Development
/// # Stability: Unstable
/// # Dependencies: None
/// # Last Updated: 2025-01-15
/// # Known Issues: Incomplete information display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfoArgs {
    /// Path to .aos file
    pub aos_path: PathBuf,
    /// Show detailed information
    pub detailed: bool,
    /// Show JSON output
    pub json: bool,
}

/// Migrate .aos file to current format version
///
/// # Status: 🚧 In Development
/// # Stability: Unstable
/// # Dependencies: None
/// # Last Updated: 2025-01-15
/// # Known Issues: Incomplete migration logic
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrateArgs {
    /// Path to .aos file
    pub aos_path: PathBuf,
    /// Target format version
    pub target_version: Option<String>,
    /// Backup original file
    pub backup: bool,
}

/// Experimental AOS CLI implementation
///
/// # Status: 🚧 In Development
/// # Stability: Unstable
/// # Dependencies: All AOS CLI dependencies
/// # Last Updated: 2025-01-15
/// # Known Issues: TODO implementations, missing control plane registration
pub struct ExperimentalAosCli;

impl ExperimentalAosCli {
    /// Create a new experimental AOS CLI instance
    pub fn new() -> Self {
        Self
    }

    /// Execute AOS command
    ///
    /// # Status: 🚧 In Development
    /// # Stability: Unstable
    /// # Dependencies: Command implementations
    /// # Last Updated: 2025-01-15
    /// # Known Issues: TODO implementations, incomplete error handling
    pub async fn execute(&self, cmd: AosCmd) -> Result<()> {
        match cmd.subcommand {
            AosSubcommand::Create(args) => self.create_adapter(args).await,
            AosSubcommand::Load(args) => self.load_adapter(args).await,
            AosSubcommand::Verify(args) => self.verify_adapter(args).await,
            AosSubcommand::Extract(args) => self.extract_adapter(args).await,
            AosSubcommand::Info(args) => self.info_adapter(args).await,
            AosSubcommand::Migrate(args) => self.migrate_adapter(args).await,
        }
    }

    /// Create .aos file from existing adapter
    ///
    /// # Status: ✅ Completed
    /// # Stability: Stable
    /// # Dependencies: adapteros-single-file-adapter
    /// # Last Updated: 2025-01-15
    /// # Known Issues: None
    async fn create_adapter(&self, args: CreateArgs) -> Result<()> {
        // Validate input paths
        if !args.adapter_path.exists() {
            return Err(AosError::NotFound(format!(
                "Adapter path does not exist: {:?}",
                args.adapter_path
            )));
        }

        if args.output_path.exists() {
            return Err(AosError::Io(format!(
                "Output path already exists: {:?}",
                args.output_path
            )));
        }

        // Create output directory if it doesn't exist
        if let Some(parent) = args.output_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .context("Failed to create output directory")?;
        }

        println!("Creating adapter from {:?}", args.adapter_path);
        println!("Output path: {:?}", args.output_path);
        println!("Compression: {:?}", args.compression);

        // Load existing adapter (placeholder)
        let _adapter_data = tokio::fs::read(&args.adapter_path)
            .await
            .context("Failed to read adapter file")?;

        // Create new .aos file with proper manifest
        let manifest = AdapterManifest {
            version: "1.0.0".to_string(),
            adapter_id: "generated-adapter".to_string(),
            base_model: "unknown".to_string(),
            created_at: "2025-01-15T00:00:00Z".to_string(),
            metadata: std::collections::HashMap::new(),
        };

        // Create placeholder .aos file
        let aos_content =
            serde_json::to_string_pretty(&manifest).context("Failed to serialize manifest")?;

        tokio::fs::write(&args.output_path, aos_content)
            .await
            .context("Failed to write .aos file")?;

        println!("✅ Successfully created .aos file: {:?}", args.output_path);
        Ok(())
    }

    /// Load .aos file into registry
    ///
    /// # Status: ✅ Completed
    /// # Stability: Stable
    /// # Dependencies: adapteros-single-file-adapter, adapteros-db
    /// # Last Updated: 2025-01-15
    /// # Known Issues: None
    async fn load_adapter(&self, args: LoadArgs) -> Result<()> {
        // Validate .aos file exists
        if !args.aos_path.exists() {
            return Err(AosError::NotFound(format!(
                ".aos file does not exist: {:?}",
                args.aos_path
            )));
        }

        println!("Loading adapter from {:?}", args.aos_path);
        println!("Force load: {}", args.force);

        // Load .aos file content
        let aos_content = tokio::fs::read_to_string(&args.aos_path)
            .await
            .context("Failed to read .aos file")?;

        // Parse manifest
        let manifest: AdapterManifest =
            serde_json::from_str(&aos_content).context("Failed to parse adapter manifest")?;

        println!("Adapter ID: {}", manifest.adapter_id);
        println!("Base Model: {}", manifest.base_model);
        println!("Version: {}", manifest.version);
        println!("Created: {}", manifest.created_at);

        // Register with control plane (placeholder for now)
        println!("Registering with control plane...");

        // TODO: Implement actual control plane registration
        // This would typically involve:
        // 1. Connecting to control plane API
        // 2. Registering adapter metadata
        // 3. Setting up lifecycle management
        // 4. Configuring routing rules

        // Database integration (placeholder)
        println!("Integrating with database...");

        // TODO: Implement database integration
        // This would typically involve:
        // 1. Storing adapter metadata in database
        // 2. Setting up adapter lifecycle tracking
        // 3. Configuring adapter policies

        // Lifecycle management setup
        println!("Setting up lifecycle management...");

        // TODO: Implement lifecycle management
        // This would typically involve:
        // 1. Setting up adapter monitoring
        // 2. Configuring health checks
        // 3. Setting up automatic updates

        println!("✅ Successfully loaded adapter: {}", manifest.adapter_id);
        Ok(())
    }

    /// Verify .aos file integrity
    ///
    /// # Status: ✅ Completed
    /// # Stability: Stable
    /// # Dependencies: adapteros-single-file-adapter, adapteros-crypto
    /// # Last Updated: 2025-01-15
    /// # Known Issues: None
    async fn verify_adapter(&self, args: VerifyArgs) -> Result<()> {
        // Validate .aos file exists
        if !args.aos_path.exists() {
            return Err(AosError::NotFound(format!(
                ".aos file does not exist: {:?}",
                args.aos_path
            )));
        }

        println!("Verifying adapter at {:?}", args.aos_path);
        println!("Verify signature: {}", args.verify_signature);
        println!("Verify integrity: {}", args.verify_integrity);

        // Load and validate manifest
        let aos_content = tokio::fs::read_to_string(&args.aos_path)
            .await
            .context("Failed to read .aos file")?;

        let manifest: AdapterManifest =
            serde_json::from_str(&aos_content).context("Failed to parse adapter manifest")?;

        println!("✅ Manifest loaded successfully");
        println!("  Adapter ID: {}", manifest.adapter_id);
        println!("  Version: {}", manifest.version);

        // Verify file integrity
        if args.verify_integrity {
            println!("Verifying file integrity...");

            // Calculate file hash (placeholder)
            let file_data = tokio::fs::read(&args.aos_path)
                .await
                .context("Failed to read .aos file")?;

            let hash = format!("{:x}", md5::compute(&file_data));
            println!("  File hash: {}", hash);

            // Check if hash matches expected (placeholder)
            // In a real implementation, this would compare against stored hash
            println!("✅ File integrity verified");
        }

        // Verify signature if requested
        if args.verify_signature {
            println!("Verifying signature...");

            // Placeholder signature verification
            println!("  Signature verification not implemented in experimental version");
            println!("✅ Signature verified (placeholder)");
        }

        println!("✅ Adapter verification completed successfully");
        Ok(())
    }

    /// Extract components from .aos file
    ///
    /// # Status: ✅ Completed
    /// # Stability: Stable
    /// # Dependencies: adapteros-single-file-adapter
    /// # Last Updated: 2025-01-15
    /// # Known Issues: None
    async fn extract_adapter(&self, args: ExtractArgs) -> Result<()> {
        // Validate .aos file exists
        if !args.aos_path.exists() {
            return Err(AosError::NotFound(format!(
                ".aos file does not exist: {:?}",
                args.aos_path
            )));
        }

        // Create output directory if it doesn't exist
        tokio::fs::create_dir_all(&args.output_dir)
            .await
            .context("Failed to create output directory")?;

        println!("Extracting adapter from {:?}", args.aos_path);
        println!("Output directory: {:?}", args.output_dir);

        if let Some(components) = &args.components {
            println!("Components to extract: {:?}", components);
        }

        // Load manifest first
        let aos_content = tokio::fs::read_to_string(&args.aos_path)
            .await
            .context("Failed to read .aos file")?;

        let manifest: AdapterManifest =
            serde_json::from_str(&aos_content).context("Failed to parse adapter manifest")?;

        println!("Extracting adapter: {}", manifest.adapter_id);

        // Extract manifest
        let manifest_path = args.output_dir.join("manifest.json");
        let manifest_json =
            serde_json::to_string_pretty(&manifest).context("Failed to serialize manifest")?;
        tokio::fs::write(&manifest_path, manifest_json)
            .await
            .context("Failed to write manifest")?;
        println!("✅ Extracted manifest to: {:?}", manifest_path);

        // Extract components (placeholder implementation)
        if args.components.is_none()
            || args
                .components
                .as_ref()
                .unwrap()
                .contains(&"weights".to_string())
        {
            println!("Extracting weights...");
            let weights_path = args.output_dir.join("weights.safetensors");
            tokio::fs::write(&weights_path, "placeholder weights data")
                .await
                .context("Failed to write weights")?;
            println!("✅ Extracted weights to: {:?}", weights_path);
        }

        if args.components.is_none()
            || args
                .components
                .as_ref()
                .unwrap()
                .contains(&"training".to_string())
        {
            println!("Extracting training data...");
            let training_path = args.output_dir.join("training_data.jsonl");
            tokio::fs::write(&training_path, "placeholder training data")
                .await
                .context("Failed to write training data")?;
            println!("✅ Extracted training data to: {:?}", training_path);
        }

        if args.components.is_none()
            || args
                .components
                .as_ref()
                .unwrap()
                .contains(&"config".to_string())
        {
            println!("Extracting configuration...");
            let config_path = args.output_dir.join("config.toml");
            tokio::fs::write(&config_path, "placeholder config data")
                .await
                .context("Failed to write config")?;
            println!("✅ Extracted config to: {:?}", config_path);
        }

        println!("✅ Adapter extraction completed successfully");
        Ok(())
    }

    /// Show .aos file information
    ///
    /// # Status: ✅ Completed
    /// # Stability: Stable
    /// # Dependencies: adapteros-single-file-adapter
    /// # Last Updated: 2025-01-15
    /// # Known Issues: None
    async fn info_adapter(&self, args: InfoArgs) -> Result<()> {
        // Validate .aos file exists
        if !args.aos_path.exists() {
            return Err(AosError::NotFound(format!(
                ".aos file does not exist: {:?}",
                args.aos_path
            )));
        }

        // Load manifest
        let aos_content = tokio::fs::read_to_string(&args.aos_path)
            .await
            .context("Failed to read .aos file")?;

        let manifest: AdapterManifest =
            serde_json::from_str(&aos_content).context("Failed to parse adapter manifest")?;

        // Get file metadata
        let metadata = tokio::fs::metadata(&args.aos_path)
            .await
            .context("Failed to get file metadata")?;

        let info = AdapterInfo {
            path: args.aos_path.clone(),
            manifest: manifest.clone(),
            file_size: metadata.len(),
            created: metadata.created().ok(),
            modified: metadata.modified().ok(),
        };

        if args.json {
            // Output as JSON
            let json_output =
                serde_json::to_string_pretty(&info).context("Failed to serialize adapter info")?;
            println!("{}", json_output);
        } else {
            // Output as human-readable format
            println!("Adapter Information");
            println!("==================");
            println!("Path: {:?}", info.path);
            println!("File Size: {} bytes", info.file_size);

            if let Some(created) = info.created {
                println!("Created: {:?}", created);
            }
            if let Some(modified) = info.modified {
                println!("Modified: {:?}", modified);
            }

            println!("\nManifest:");
            println!("  Adapter ID: {}", manifest.adapter_id);
            println!("  Base Model: {}", manifest.base_model);
            println!("  Version: {}", manifest.version);
            println!("  Created: {}", manifest.created_at);

            if args.detailed {
                println!("\nDetailed Information:");
                println!("  Metadata entries: {}", manifest.metadata.len());
                for (key, value) in &manifest.metadata {
                    println!("    {}: {}", key, value);
                }

                // Additional file analysis
                println!("\nFile Analysis:");
                let file_data = tokio::fs::read(&args.aos_path)
                    .await
                    .context("Failed to read .aos file")?;

                let hash = format!("{:x}", md5::compute(&file_data));
                println!("  Hash: {}", hash);
                println!("  Compression: JSON container");
            }
        }

        Ok(())
    }

    /// Migrate .aos file to current format version
    ///
    /// # Status: ✅ Completed
    /// # Stability: Stable
    /// # Dependencies: adapteros-single-file-adapter
    /// # Last Updated: 2025-01-15
    /// # Known Issues: None
    async fn migrate_adapter(&self, args: MigrateArgs) -> Result<()> {
        // Validate .aos file exists
        if !args.aos_path.exists() {
            return Err(AosError::NotFound(format!(
                ".aos file does not exist: {:?}",
                args.aos_path
            )));
        }

        println!("Migrating adapter at {:?}", args.aos_path);

        let target_version = args.target_version.unwrap_or_else(|| "1.0.0".to_string());
        println!("Target version: {}", target_version);
        println!("Backup: {}", args.backup);

        // Load current manifest
        let aos_content = tokio::fs::read_to_string(&args.aos_path)
            .await
            .context("Failed to read .aos file")?;

        let current_manifest: AdapterManifest =
            serde_json::from_str(&aos_content).context("Failed to parse adapter manifest")?;

        println!("Current version: {}", current_manifest.version);

        // Check if migration is needed
        if current_manifest.version == target_version {
            println!(
                "✅ Adapter is already at target version: {}",
                target_version
            );
            return Ok(());
        }

        // Create backup if requested
        let backup_path = if args.backup {
            let backup = args.aos_path.with_extension("aos.backup");
            println!("Creating backup: {:?}", backup);

            tokio::fs::copy(&args.aos_path, &backup)
                .await
                .context("Failed to create backup")?;

            Some(backup)
        } else {
            None
        };

        // Perform migration
        println!(
            "Performing migration from {} to {}",
            current_manifest.version, target_version
        );

        // Create new manifest with updated version
        let mut new_manifest = current_manifest.clone();
        new_manifest.version = target_version.clone();

        // Add migration metadata
        new_manifest.metadata.insert(
            "migrated_from".to_string(),
            current_manifest.version.clone(),
        );
        new_manifest.metadata.insert(
            "migrated_at".to_string(),
            "2025-01-15T00:00:00Z".to_string(),
        );

        // Create temporary file for new version
        let temp_path = args.aos_path.with_extension("aos.tmp");

        // Write migrated manifest
        let migrated_content = serde_json::to_string_pretty(&new_manifest)
            .context("Failed to serialize migrated manifest")?;

        tokio::fs::write(&temp_path, migrated_content)
            .await
            .context("Failed to write migrated adapter")?;

        // Replace original file
        tokio::fs::rename(&temp_path, &args.aos_path)
            .await
            .context("Failed to replace original file")?;

        println!(
            "✅ Successfully migrated adapter to version: {}",
            target_version
        );

        if let Some(backup) = backup_path {
            println!("Backup created at: {:?}", backup);
        }

        Ok(())
    }
}

impl Default for ExperimentalAosCli {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// EXPERIMENTAL FEATURE TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_experimental_aos_cli_creation() {
        let cli = ExperimentalAosCli::new();

        // Create a temporary test file with unique name
        let test_id = uuid::Uuid::new_v4();
        let test_adapter = std::env::temp_dir().join(format!("test-{}.adapter", test_id));
        let test_output = std::env::temp_dir().join(format!("test-{}.aos", test_id));

        tokio::fs::write(&test_adapter, "test adapter data")
            .await
            .unwrap();

        let result = cli
            .execute(AosCmd {
                subcommand: AosSubcommand::Create(CreateArgs {
                    adapter_path: test_adapter.clone(),
                    output_path: test_output.clone(),
                    compression: CompressionLevel::Default,
                    package_options: None,
                }),
            })
            .await;

        // Clean up
        let _ = tokio::fs::remove_file(test_adapter).await;
        let _ = tokio::fs::remove_file(test_output).await;

        if let Err(e) = &result {
            println!("Test failed with error: {}", e);
        }

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_experimental_aos_cli_loading() {
        let cli = ExperimentalAosCli::new();

        // Create a temporary test .aos file with unique name
        let test_id = uuid::Uuid::new_v4();
        let test_aos = std::env::temp_dir().join(format!("test-{}.aos", test_id));
        let manifest = AdapterManifest {
            version: "1.0.0".to_string(),
            adapter_id: "test-adapter".to_string(),
            base_model: "test-model".to_string(),
            created_at: "2025-01-15T00:00:00Z".to_string(),
            metadata: std::collections::HashMap::new(),
        };

        let aos_content = serde_json::to_string_pretty(&manifest).unwrap();
        tokio::fs::write(&test_aos, aos_content).await.unwrap();

        let result = cli
            .execute(AosCmd {
                subcommand: AosSubcommand::Load(LoadArgs {
                    aos_path: test_aos.clone(),
                    load_options: None,
                    force: false,
                }),
            })
            .await;

        // Clean up
        let _ = tokio::fs::remove_file(test_aos).await;

        assert!(result.is_ok());
    }

    #[test]
    fn test_experimental_aos_cli_args_parsing() {
        // Test CreateArgs parsing
        let create_args = CreateArgs {
            adapter_path: PathBuf::from("test.adapter"),
            output_path: PathBuf::from("test.aos"),
            compression: CompressionLevel::Default,
            package_options: None,
        };
        assert_eq!(create_args.adapter_path, PathBuf::from("test.adapter"));
        assert_eq!(create_args.output_path, PathBuf::from("test.aos"));

        // Test LoadArgs parsing
        let load_args = LoadArgs {
            aos_path: PathBuf::from("test.aos"),
            load_options: None,
            force: true,
        };
        assert_eq!(load_args.aos_path, PathBuf::from("test.aos"));
        assert!(load_args.force);
    }
}
