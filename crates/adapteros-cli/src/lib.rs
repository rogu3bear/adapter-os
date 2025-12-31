use adapteros_config::{BackendPreference, ModelConfig};
use adapteros_core::identity::IdentityEnvelope;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

// Expose the CLI support modules so integration tests can use command plumbing.
pub mod auth_store;
pub mod cli_telemetry;
pub mod commands;
pub mod error_codes;
pub mod formatting;
pub mod http_client;
pub mod logging;
pub mod output;
pub mod validation;

// Re-export app module for testing
pub mod app;

/// Backend type selection for inference (mirrors the binary crate definition).
#[derive(Debug, Clone, clap::ValueEnum)]
pub enum BackendType {
    /// Metal backend (macOS GPU)
    Metal,
    /// MLX backend (Python/MLX)
    #[clap(name = "mlx")]
    MLX,
    /// CoreML backend (macOS Neural Engine)
    CoreML,
}

impl From<BackendType> for adapteros_config::BackendPreference {
    fn from(bt: BackendType) -> Self {
        match bt {
            BackendType::Metal => adapteros_config::BackendPreference::Metal,
            BackendType::MLX => adapteros_config::BackendPreference::Mlx,
            BackendType::CoreML => adapteros_config::BackendPreference::CoreML,
        }
    }
}

#[derive(Parser)]
#[command(name = "aos")]
#[command(about = "AdapterOS CLI", long_about = None)]
pub struct Cli {
    /// Tenant ID
    #[arg(short, long, default_value = "default")]
    pub tenant_id: String,

    /// Domain
    #[arg(short = 'd', long, default_value = "cli")]
    pub domain: String,

    /// Purpose
    #[arg(short, long, default_value = "maintenance")]
    pub purpose: String,

    /// Model path (overrides AOS_MODEL_PATH env var)
    #[arg(long, global = true, env = "AOS_MODEL_PATH")]
    pub model_path: Option<String>,

    /// Model backend preference (overrides AOS_MODEL_BACKEND env var)
    /// Values: auto, coreml, metal, mlx
    #[arg(long, global = true, env = "AOS_MODEL_BACKEND", default_value = "auto")]
    pub model_backend: String,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    // ... existing subcommands ...
}

// Function to get identity from cli
pub fn get_identity(cli: &Cli) -> IdentityEnvelope {
    IdentityEnvelope::new(
        cli.tenant_id.clone(),
        cli.domain.clone(),
        cli.purpose.clone(),
        IdentityEnvelope::default_revision(),
    )
}

impl Cli {
    /// Build a ModelConfig from CLI arguments with precedence: CLI > ENV > defaults
    pub fn get_model_config(&self) -> anyhow::Result<ModelConfig> {
        // Start with environment-based config (or defaults)
        let mut config = ModelConfig::from_env().map_err(|e| anyhow::anyhow!("{}", e))?;

        // Override with CLI args if provided
        if let Some(ref path) = self.model_path {
            config.path = PathBuf::from(path);
        }

        // Parse backend preference from CLI
        config.backend = self
            .model_backend
            .parse::<BackendPreference>()
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        Ok(config)
    }
}
