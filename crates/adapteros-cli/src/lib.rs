use adapteros_core::identity::IdentityEnvelope;
use adapteros_core::{Domain, Purpose};
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
    // Map string domain to Domain enum
    let domain = match cli.domain.as_str() {
        "cli" => Domain::CLI,
        "router" => Domain::Router,
        "worker" => Domain::Worker,
        "lifecycle" => Domain::Lifecycle,
        "telemetry" => Domain::Telemetry,
        "policy" => Domain::Policy,
        "plugin" => Domain::Plugin,
        _ => Domain::CLI, // default to CLI
    };

    // Map string purpose to Purpose enum
    let purpose = match cli.purpose.as_str() {
        "inference" => Purpose::Inference,
        "training" => Purpose::Training,
        "replay" => Purpose::Replay,
        "maintenance" => Purpose::Maintenance,
        "plugin_io" => Purpose::PluginIO,
        "audit" => Purpose::Audit,
        _ => Purpose::Maintenance, // default to Maintenance
    };

    IdentityEnvelope::new(
        cli.tenant_id.clone(),
        domain,
        purpose,
        IdentityEnvelope::default_revision(),
    )
}
