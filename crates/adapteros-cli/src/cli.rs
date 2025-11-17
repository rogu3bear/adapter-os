use adapteros_core::identity::IdentityEnvelope;
use adapteros_core::{Domain, Purpose};
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
    pub fn get_identity(&self) -> IdentityEnvelope {
        // Map string domain to Domain enum
        let domain = match self.domain.as_str() {
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
        let purpose = match self.purpose.as_str() {
            "inference" => Purpose::Inference,
            "training" => Purpose::Training,
            "replay" => Purpose::Replay,
            "maintenance" => Purpose::Maintenance,
            "plugin_io" => Purpose::PluginIO,
            "audit" => Purpose::Audit,
            _ => Purpose::Maintenance, // default to Maintenance
        };

        IdentityEnvelope::new(
            self.tenant_id.clone(),
            domain,
            purpose,
            IdentityEnvelope::default_revision(),
        )
    }
}
