use adapteros_config::{BackendPreference, ModelConfig};
use adapteros_core::identity::IdentityEnvelope;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

pub mod formatting;

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
