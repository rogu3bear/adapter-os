use adapteros_core::identity::IdentityEnvelope;
use clap::{Parser, Subcommand};

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
