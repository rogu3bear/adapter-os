use adapteros_core::identity::IdentityEnvelope;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "aos")]
#[command(about = "AdapterOS CLI", long_about = None)]
pub struct Cli {
    /// Tenant ID (default: default)
    #[arg(short, long, default_value = "default")]
    pub tenant_id: String,

    /// Domain (default: cli)
    #[arg(short = 'D', long, default_value = "cli")]
    pub domain: String,

    /// Purpose (default: maintenance)
    #[arg(short, long, default_value = "maintenance")]
    pub purpose: String,

    /// Revision (auto from git or env)
    #[arg(short, long)]
    pub revision: Option<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Run inference with backpressure check
    Infer {
        /// Prompt to infer on
        #[arg(short, long)]
        prompt: String,
        /// Model to use (optional)
        #[arg(short, long)]
        model: Option<String>,
        /// Max tokens
        #[arg(short, long, default_value = "100")]
        max_tokens: usize,
    },
}

impl Cli {
    /// Get identity envelope from CLI arguments
    pub fn get_identity(&self) -> IdentityEnvelope {
        let revision = self
            .revision
            .clone()
            .unwrap_or_else(|| IdentityEnvelope::default_revision());
        IdentityEnvelope::new(
            self.tenant_id.clone(),
            self.domain.clone(),
            self.purpose.clone(),
            revision,
        )
    }
}
